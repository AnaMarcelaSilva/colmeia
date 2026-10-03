//! Os itens de uma lousa como a tela os guarda, a fila do que falta gravar
//! e o desfazer. Sem egui e sem rede: a tela muda os itens aqui na hora
//! (otimista), monta um lote e manda em segundo plano; a resposta volta para
//! cá. Um lote por vez por lousa: o que muda enquanto um lote está no ar
//! espera o próximo.
//!
//! Itens criados ganham um id negativo até o núcleo responder; aí o id
//! verdadeiro troca o negativo em todo lugar (itens, pontas das ligações,
//! fila e desfazer). Desfazer uma remoção recria os itens com ids novos, e a
//! pilha troca os antigos pelos novos do mesmo jeito.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::api::{self, CamposElemento, ElementoLousa as Elemento, NovoElemento, Operacao, Ponta, TipoElemento};

/// Campos mudados de um item que ainda não foram ao núcleo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mudou(u16);

impl Mudou {
    pub const POSICAO: Mudou = Mudou(1);
    pub const TAMANHO: Mudou = Mudou(2);
    pub const Z: Mudou = Mudou(4);
    pub const COR: Mudou = Mudou(8);
    pub const TITULO: Mudou = Mudou(16);
    pub const TEXTO: Mudou = Mudou(32);
    pub const TIPO: Mudou = Mudou(64);
    pub const PONTAS: Mudou = Mudou(128);

    pub fn tem(self, outro: Mudou) -> bool {
        self.0 & outro.0 != 0
    }

    pub fn com(self, outro: Mudou) -> Mudou {
        Mudou(self.0 | outro.0)
    }

    /// Os campos em que `a` e `b` diferem.
    pub fn entre(a: &Elemento, b: &Elemento) -> Mudou {
        let mut m = Mudou::default();
        let pares = [
            (a.x != b.x || a.y != b.y, Mudou::POSICAO),
            (a.largura != b.largura || a.altura != b.altura, Mudou::TAMANHO),
            (a.z != b.z, Mudou::Z),
            (a.cor != b.cor, Mudou::COR),
            (a.titulo != b.titulo, Mudou::TITULO),
            (a.texto != b.texto, Mudou::TEXTO),
            (a.tipo != b.tipo, Mudou::TIPO),
            (a.de != b.de || a.para != b.para, Mudou::PONTAS),
        ];
        for (difere, campo) in pares {
            if difere {
                m = m.com(campo);
            }
        }
        m
    }
}

/// Um passo do desfazer: cada par é o mesmo item antes e depois (None: não
/// existia ou deixou de existir).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comando {
    /// O número do comando (dado ao registrar): o "Desfazer" de um aviso
    /// desfaz o comando que ele anunciou, não o último.
    pub numero: u64,
    pub pares: Vec<(Option<Elemento>, Option<Elemento>)>,
}

impl Comando {
    pub fn vazio(&self) -> bool {
        self.pares.iter().all(|(a, d)| a == d)
    }
}

/// O que foi para o núcleo num lote, para devolver à fila se ele falhar.
#[derive(Clone, Debug, Default)]
pub struct Lote {
    pub operacoes: Vec<api::Operacao>,
    criados: Vec<(String, i64)>,
    alterados: Vec<(i64, Mudou)>,
    removidos: Vec<(i64, i64)>,
    /// O lote leva um desfazer (Some(true)) ou um refazer (Some(false)):
    /// num 409 o aviso é outro e o comando sai da pilha.
    pub desfazer: Option<bool>,
}

/// O que a tela precisa saber depois de uma resposta.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resposta {
    /// Ids que trocaram (negativo → verdadeiro): seleção e edição acompanham.
    pub trocas: Vec<(i64, i64)>,
    /// O lote voltou com 409: o que mudou fora foi aplicado.
    pub mudou_fora: bool,
    /// O 409 caiu num desfazer: ele não foi feito.
    pub nao_desfez: bool,
    /// Itens em conflito que você está editando (o texto se junta ao sair).
    pub conflitos: Vec<i64>,
    pub erro: Option<String>,
}

pub const MAX_DESFAZER: usize = 200;
/// O limite do núcleo por lote.
pub const MAX_OPERACOES: usize = 500;

#[derive(Default)]
pub struct Modelo {
    pub id: i64,
    /// Os itens, na ordem de desenho (z e id).
    pub elementos: Vec<Elemento>,
    proximo_temporario: i64,
    rev: u64,
    // Fila de gravação.
    criar: Vec<i64>,
    alterar: BTreeMap<i64, Mudou>,
    remover: Vec<(i64, i64)>,
    /// Itens criados que você apagou antes de o núcleo responder: saem assim
    /// que o id verdadeiro chegar.
    apagar_ao_chegar: HashSet<i64>,
    pub em_voo: Option<Lote>,
    /// O último lote falhou (não 409): a fila espera "Tentar de novo" ou outra mudança.
    pub falhou: Option<String>,
    /// Itens que o último lote com erro levava (o ponto vermelho "Não salvo").
    pub nao_salvos: HashSet<i64>,
    /// O estado do núcleo dos itens em conflito enquanto você edita o texto.
    pub remotos: HashMap<i64, Elemento>,
    /// Itens que o agente acrescentou desde o seu último clique (contorno "novo").
    pub novos: HashSet<i64>,
    /// Itens de fora com id que a tela ainda não conhece, chegados enquanto
    /// um lote com criações estava no ar: podem ser o eco das suas criações
    /// (o aviso chega antes da resposta, já com o id verdadeiro). Esperam a
    /// resposta; o que não for seu entra depois.
    ecos: Vec<Elemento>,
    desfazer: Vec<Comando>,
    refazer: Vec<Comando>,
    comandos: u64,
    /// O último desfazer (true) ou refazer (false) ainda não foi num lote.
    desfazer_pendente: Option<bool>,
}

impl Modelo {
    pub fn novo(aberta: api::LousaAberta) -> Modelo {
        let mut m = Modelo { id: aberta.lousa.id, proximo_temporario: -1, ..Default::default() };
        for mut e in aberta.elementos {
            m.rev += 1;
            e.rev = m.rev;
            m.elementos.push(e);
        }
        m.ordenar();
        m
    }

    fn ordenar(&mut self) {
        self.elementos.sort_by_key(|e| (e.z, e.id));
    }

    fn proxima_rev(&mut self) -> u64 {
        self.rev += 1;
        self.rev
    }

    pub fn indice(&self, id: i64) -> Option<usize> {
        self.elementos.iter().position(|e| e.id == id)
    }

    pub fn buscar(&self, id: i64) -> Option<&Elemento> {
        self.elementos.iter().find(|e| e.id == id)
    }

    pub fn z_maximo(&self) -> i64 {
        self.elementos.iter().map(|e| e.z).max().unwrap_or(0)
    }

    pub fn z_minimo(&self) -> i64 {
        self.elementos.iter().map(|e| e.z).min().unwrap_or(0)
    }

    /// Há algo na fila esperando o próximo lote.
    pub fn na_fila(&self) -> bool {
        !self.criar.is_empty() || self.alterar.keys().any(|id| *id > 0) || !self.remover.is_empty()
    }

    /// O item tem mudança sua que ainda não chegou ao núcleo.
    pub fn sujo(&self, id: i64) -> bool {
        id < 0
            || self.alterar.contains_key(&id)
            || self.em_voo.as_ref().is_some_and(|l| l.alterados.iter().any(|(a, _)| *a == id) || l.criados.iter().any(|(_, t)| *t == id))
    }

    // Mudanças da tela

    /// Cria o item (id negativo até o núcleo responder) por cima de todos.
    pub fn criar_local(&mut self, mut e: Elemento) -> i64 {
        e.id = self.proximo_temporario;
        self.proximo_temporario -= 1;
        e.lousa_id = self.id;
        e.z = self.z_maximo() + 1;
        e.autor = "voce".into();
        e.versao = 0;
        e.rev = self.proxima_rev();
        self.criar.push(e.id);
        self.elementos.push(e.clone());
        self.ordenar();
        self.falhou = None;
        e.id
    }

    /// Recoloca um item como ele era (desfazer uma remoção), com id novo.
    /// As pilhas passam a apontar para o id novo.
    fn recriar(&mut self, mut e: Elemento) -> i64 {
        let antigo = e.id;
        e.id = self.proximo_temporario;
        self.proximo_temporario -= 1;
        e.versao = 0;
        e.rev = self.proxima_rev();
        e.autor = "voce".into();
        self.criar.push(e.id);
        let novo = e.id;
        self.elementos.push(e);
        self.ordenar();
        self.trocar_id(antigo, novo);
        novo
    }

    /// Muda um item e põe os campos na fila. Mover e trocar a ordem não
    /// mudam o desenho do conteúdo: a revisão (e o cache) ficam.
    pub fn mudar(&mut self, id: i64, campos: Mudou, f: impl FnOnce(&mut Elemento)) {
        let conteudo = campos.tem(Mudou::COR) || campos.tem(Mudou::TITULO) || campos.tem(Mudou::TEXTO) || campos.tem(Mudou::TIPO) || campos.tem(Mudou::PONTAS);
        let rev = if conteudo { self.proxima_rev() } else { 0 };
        let Some(e) = self.elementos.iter_mut().find(|e| e.id == id) else { return };
        f(e);
        if conteudo {
            e.rev = rev;
        }
        if campos.tem(Mudou::Z) {
            self.ordenar();
        }
        // Um item novo ainda na fila vai inteiro quando for criado; um que
        // já está no ar guarda a mudança (sob o id negativo, trocado quando
        // o verdadeiro chegar) para a resposta da criação não apagá-la.
        if id > 0 || !self.criar.contains(&id) {
            let atual = self.alterar.entry(id).or_default();
            *atual = atual.com(campos);
        }
        self.falhou = None;
    }

    /// Ligações que tocam algum dos itens.
    pub fn ligacoes_de(&self, ids: &HashSet<i64>) -> Vec<i64> {
        self.elementos.iter().filter(|e| e.tipo == TipoElemento::Ligacao && (ids.contains(&e.de) || ids.contains(&e.para))).map(|e| e.id).collect()
    }

    /// Remove os itens e as ligações deles; devolve o que saiu (para o desfazer).
    pub fn remover_local(&mut self, ids: &HashSet<i64>) -> Vec<Elemento> {
        let mut todos = ids.clone();
        todos.extend(self.ligacoes_de(ids));
        let mut saiu = Vec::new();
        self.elementos.retain(|e| {
            if todos.contains(&e.id) {
                saiu.push(e.clone());
                false
            } else {
                true
            }
        });
        for e in &saiu {
            self.alterar.remove(&e.id);
            if e.id > 0 {
                // O mesmo id duas vezes no lote faz o núcleo recusar o lote inteiro.
                if !self.remover.iter().any(|(id, _)| *id == e.id) {
                    self.remover.push((e.id, e.versao));
                }
            } else if let Some(i) = self.criar.iter().position(|t| *t == e.id) {
                self.criar.remove(i);
            } else {
                // Está no ar: sai quando o id verdadeiro chegar.
                self.apagar_ao_chegar.insert(e.id);
            }
            self.remotos.remove(&e.id);
        }
        self.falhou = None;
        saiu
    }

    // Mudanças de fora

    /// Aplica o que veio do núcleo (aviso de outra tela, do agente ou o
    /// retrato ao reconectar). Um item com mudança sua ainda não gravada
    /// fica como está: o aviso pode ser o eco do seu próprio lote (que chega
    /// antes da resposta dele); se for de outra tela, o próximo lote volta
    /// com 409 e aí se resolve. Aplicar duas vezes não muda nada (só entra
    /// versão maior).
    pub fn aplicar_remoto(&mut self, elementos: Vec<Elemento>, removidos: &[i64]) -> bool {
        let mut mudou = false;
        for mut e in elementos {
            if e.lousa_id != 0 && e.lousa_id != self.id {
                continue;
            }
            if self.sujo(e.id) {
                continue;
            }
            match self.indice(e.id) {
                Some(i) if e.versao <= self.elementos[i].versao => {}
                Some(i) => {
                    e.rev = self.proxima_rev();
                    self.elementos[i] = e;
                    mudou = true;
                }
                None if e.id > 0 && self.em_voo.as_ref().is_some_and(|l| !l.criados.is_empty()) => {
                    self.ecos.retain(|x| x.id != e.id);
                    self.ecos.push(e);
                }
                None => {
                    e.rev = self.proxima_rev();
                    self.elementos.push(e);
                    mudou = true;
                }
            }
        }
        if !removidos.is_empty() {
            self.ecos.retain(|e| !removidos.contains(&e.id));
            let antes = self.elementos.len();
            self.elementos.retain(|e| !removidos.contains(&e.id));
            mudou |= self.elementos.len() != antes;
            for id in removidos {
                self.alterar.remove(id);
                self.remotos.remove(id);
            }
        }
        if mudou {
            self.ordenar();
        }
        mudou
    }

    /// O retrato inteiro da lousa (ao reconectar): aplica o que mudou e tira
    /// o que sumiu de lá (menos o que tem mudança sua no caminho).
    pub fn sincronizar(&mut self, elementos: Vec<Elemento>) -> bool {
        let existentes: HashSet<i64> = elementos.iter().map(|e| e.id).collect();
        let sumiram: Vec<i64> = self.elementos.iter().filter(|e| e.id > 0 && !existentes.contains(&e.id) && !self.sujo(e.id)).map(|e| e.id).collect();
        self.aplicar_remoto(elementos, &sumiram)
    }

    // Lotes

    /// Monta o próximo lote com o que está na fila (até 500 operações).
    /// `fora`: itens que não vão agora (o texto em conflito que você edita).
    pub fn montar_lote(&mut self, fora: &HashSet<i64>) -> Option<Lote> {
        if self.em_voo.is_some() || self.falhou.is_some() || !self.na_fila() {
            return None;
        }
        let mut lote = Lote::default();
        let temporarios: HashSet<i64> = self.criar.iter().copied().collect();
        let mut criar = std::mem::take(&mut self.criar).into_iter();
        for temp in criar.by_ref() {
            if lote.operacoes.len() >= MAX_OPERACOES {
                self.criar.push(temp);
                break;
            }
            let Some(e) = self.buscar(temp).cloned() else { continue };
            let referencia = |id: i64| if id < 0 && temporarios.contains(&id) { Ponta::Ref(nome_ref(id)) } else { Ponta::Id(id) };
            let ligacao = e.tipo == TipoElemento::Ligacao;
            let novo = NovoElemento {
                tipo: e.tipo,
                x: (!ligacao).then_some(e.x),
                y: (!ligacao).then_some(e.y),
                largura: (!ligacao).then_some(e.largura),
                altura: (!ligacao).then_some(e.altura),
                z: Some(e.z),
                cor: e.cor.clone(),
                titulo: e.titulo.clone(),
                texto: e.texto.clone(),
                anexo_id: e.anexo_id,
                tarefa_ref: e.tarefa_ref,
                de: ligacao.then(|| referencia(e.de)),
                para: ligacao.then(|| referencia(e.para)),
            };
            lote.operacoes.push(Operacao::Criar { r#ref: nome_ref(temp), elemento: novo });
            lote.criados.push((nome_ref(temp), temp));
        }
        self.criar.extend(criar);
        let ids: Vec<i64> = self.alterar.keys().copied().collect();
        for id in ids {
            // Negativo: ainda esperando o id verdadeiro (vai no próximo lote).
            if id < 0 || fora.contains(&id) || lote.operacoes.len() >= MAX_OPERACOES {
                continue;
            }
            let campos = self.alterar.remove(&id).unwrap_or_default();
            let Some(e) = self.buscar(id) else { continue };
            let mut c = CamposElemento::default();
            if campos.tem(Mudou::POSICAO) {
                (c.x, c.y) = (Some(e.x), Some(e.y));
            }
            if campos.tem(Mudou::TAMANHO) {
                (c.largura, c.altura) = (Some(e.largura), Some(e.altura));
            }
            if campos.tem(Mudou::Z) {
                c.z = Some(e.z);
            }
            if campos.tem(Mudou::COR) {
                c.cor = Some(e.cor.clone());
            }
            if campos.tem(Mudou::TITULO) {
                c.titulo = Some(e.titulo.clone());
            }
            if campos.tem(Mudou::TEXTO) {
                c.texto = Some(e.texto.clone());
            }
            if campos.tem(Mudou::TIPO) {
                c.tipo = Some(e.tipo);
            }
            if campos.tem(Mudou::PONTAS) {
                (c.de, c.para) = (Some(e.de), Some(e.para));
            }
            lote.operacoes.push(Operacao::Alterar { id, versao: e.versao, campos: c });
            lote.alterados.push((id, campos));
        }
        while lote.operacoes.len() < MAX_OPERACOES && !self.remover.is_empty() {
            let (id, versao) = self.remover.remove(0);
            lote.operacoes.push(Operacao::Remover { id, versao });
            lote.removidos.push((id, versao));
        }
        if lote.operacoes.is_empty() {
            return None;
        }
        lote.desfazer = self.desfazer_pendente.take();
        self.em_voo = Some(lote.clone());
        Some(lote)
    }

    /// Recebe a resposta do lote que estava no ar. `editando`: o item cujo
    /// texto está aberto no editor (num 409 ele não é trocado).
    pub fn receber_lote(&mut self, resultado: Result<api::LoteGravado, String>, editando: Option<i64>) -> Resposta {
        let mut resposta = Resposta::default();
        let Some(lote) = self.em_voo.take() else { return resposta };
        // Os ecos das suas criações saem; o resto (de outra tela, do agente) entra.
        let mut ecos = std::mem::take(&mut self.ecos);
        if let Ok(api::LoteGravado::Ok { refs, .. }) = &resultado {
            let criados: HashSet<i64> = lote.criados.iter().filter_map(|(n, _)| refs.get(n).copied()).collect();
            ecos.retain(|e| !criados.contains(&e.id));
        }
        self.receber_resultado(lote, resultado, editando, &mut resposta);
        if !ecos.is_empty() {
            self.aplicar_remoto(ecos, &[]);
        }
        resposta
    }

    fn receber_resultado(&mut self, lote: Lote, resultado: Result<api::LoteGravado, String>, editando: Option<i64>, resposta: &mut Resposta) {
        match resultado {
            Ok(api::LoteGravado::Ok { elementos, removidos, refs }) => {
                for (nome, temp) in &lote.criados {
                    if let Some(&id) = refs.get(nome) {
                        if self.apagar_ao_chegar.remove(temp) {
                            self.remover.push((id, 1));
                        } else {
                            self.trocar_id(*temp, id);
                            resposta.trocas.push((*temp, id));
                        }
                    }
                }
                for id in lote.alterados.iter().map(|(id, _)| id).chain(lote.criados.iter().filter_map(|(n, _)| refs.get(n))) {
                    self.nao_salvos.remove(id);
                }
                for mut e in elementos {
                    let Some(i) = self.indice(e.id) else { continue };
                    if self.alterar.contains_key(&e.id) {
                        // Mudou de novo enquanto o lote estava no ar: fica o que está
                        // na tela, com a versão nova (o próximo lote leva).
                        self.elementos[i].versao = e.versao;
                        self.elementos[i].atualizado_em = e.atualizado_em;
                    } else {
                        e.rev = self.proxima_rev();
                        self.elementos[i] = e;
                    }
                }
                self.elementos.retain(|e| !removidos.contains(&e.id));
                self.ordenar();
            }
            Ok(api::LoteGravado::Mudou { elementos, removidos }) => {
                resposta.mudou_fora = true;
                // O desfazer não foi feito: o comando sai da pilha.
                match lote.desfazer {
                    Some(true) => {
                        self.refazer.pop();
                        resposta.nao_desfez = true;
                    }
                    Some(false) => {
                        self.desfazer.pop();
                        resposta.nao_desfez = true;
                    }
                    None => {}
                }
                let conflitantes: HashSet<i64> = elementos.iter().map(|e| e.id).chain(removidos.iter().copied()).collect();
                // Nada foi gravado: o resto do lote volta para a fila.
                let criados: Vec<i64> = lote.criados.iter().map(|(_, t)| *t).collect();
                self.criar.splice(0..0, criados);
                for (id, campos) in &lote.alterados {
                    if !conflitantes.contains(id) || Some(*id) == editando {
                        let atual = self.alterar.entry(*id).or_default();
                        *atual = atual.com(*campos);
                    }
                }
                for (id, versao) in &lote.removidos {
                    if !conflitantes.contains(id) {
                        self.remover.push((*id, *versao));
                    }
                }
                for mut e in elementos {
                    if Some(e.id) == editando {
                        // O texto aberto não é trocado: junta ao sair da edição.
                        resposta.conflitos.push(e.id);
                        self.remotos.insert(e.id, e);
                        continue;
                    }
                    self.alterar.remove(&e.id);
                    e.rev = self.proxima_rev();
                    match self.indice(e.id) {
                        Some(i) => self.elementos[i] = e,
                        // Você tinha removido; mudou fora: volta como está lá.
                        None => self.elementos.push(e),
                    }
                }
                self.elementos.retain(|e| !removidos.contains(&e.id));
                for id in &removidos {
                    self.alterar.remove(id);
                }
                self.ordenar();
            }
            Err(e) => {
                // Não gravou: tudo volta para a fila, que espera.
                self.criar.splice(0..0, lote.criados.iter().map(|(_, t)| *t));
                for (id, campos) in &lote.alterados {
                    let atual = self.alterar.entry(*id).or_default();
                    *atual = atual.com(*campos);
                }
                self.remover.splice(0..0, lote.removidos.iter().copied());
                self.nao_salvos.extend(lote.criados.iter().map(|(_, t)| *t).chain(lote.alterados.iter().map(|(id, _)| *id)));
                self.falhou = Some(e.clone());
                resposta.erro = Some(e);
            }
        }
    }

    /// "Tentar de novo": a fila volta a andar.
    pub fn tentar_de_novo(&mut self) {
        self.falhou = None;
    }

    /// O texto em conflito foi resolvido ao sair da edição: o item fica com
    /// a versão do núcleo e o texto final (que vai no próximo lote).
    pub fn resolver_conflito(&mut self, id: i64, texto: String) {
        let Some(remoto) = self.remotos.remove(&id) else { return };
        self.mudar(id, Mudou::TEXTO, |e| {
            e.versao = remoto.versao;
            e.texto = texto;
        });
    }

    // Desfazer

    /// Guarda o comando no desfazer; devolve o número dele (0: vazio, não entrou).
    pub fn registrar(&mut self, mut comando: Comando) -> u64 {
        if comando.vazio() {
            return 0;
        }
        self.comandos += 1;
        comando.numero = self.comandos;
        self.desfazer.push(comando);
        if self.desfazer.len() > MAX_DESFAZER {
            self.desfazer.remove(0);
        }
        self.refazer.clear();
        self.comandos
    }

    /// Desfaz o comando `numero` (o "Desfazer" do aviso). No topo da pilha é
    /// o desfazer de sempre; mais abaixo (você fez outra coisa depois), só
    /// ele é desfeito e sai da pilha, sem refazer. None: já não está na pilha.
    pub fn desfazer_comando(&mut self, numero: u64) -> Option<Vec<i64>> {
        let i = self.desfazer.iter().rposition(|c| c.numero == numero)?;
        if i + 1 == self.desfazer.len() {
            return self.desfazer();
        }
        let comando = self.desfazer.remove(i);
        // Fora da ordem não há refazer: num 409 nada sai da pilha.
        let (tocados, _) = self.aplicar_comando(&comando, true);
        self.desfazer_pendente = None;
        self.falhou = None;
        Some(tocados)
    }

    /// Tira do desfazer a criação do item (a nota criada e deixada vazia).
    pub fn descartar_ultimo_comando_de(&mut self, id: i64) {
        if let Some(c) = self.desfazer.last()
            && c.pares.iter().any(|(a, d)| a.is_none() && d.as_ref().is_some_and(|e| e.id == id))
        {
            self.desfazer.pop();
        }
    }

    #[cfg(test)]
    pub fn pode_desfazer(&self) -> bool {
        !self.desfazer.is_empty()
    }

    #[cfg(test)]
    pub fn pode_refazer(&self) -> bool {
        !self.refazer.is_empty()
    }

    /// Desfaz o último comando. Devolve os itens que ele tocou (para a seleção).
    pub fn desfazer(&mut self) -> Option<Vec<i64>> {
        let mut comando = self.desfazer.pop()?;
        let (tocados, trocas) = self.aplicar_comando(&comando, true);
        for (antigo, novo) in trocas {
            trocar_no_comando(&mut comando, antigo, novo);
        }
        self.refazer.push(comando);
        self.desfazer_pendente = Some(true);
        self.falhou = None;
        Some(tocados)
    }

    pub fn refazer(&mut self) -> Option<Vec<i64>> {
        let mut comando = self.refazer.pop()?;
        let (tocados, trocas) = self.aplicar_comando(&comando, false);
        for (antigo, novo) in trocas {
            trocar_no_comando(&mut comando, antigo, novo);
        }
        self.desfazer.push(comando);
        self.desfazer_pendente = Some(false);
        self.falhou = None;
        Some(tocados)
    }

    /// Aplica o comando para trás (`voltar`) ou para a frente. Devolve os
    /// itens tocados e os ids trocados (itens recriados).
    fn aplicar_comando(&mut self, comando: &Comando, voltar: bool) -> (Vec<i64>, Vec<(i64, i64)>) {
        let mut tocados = Vec::new();
        let mut trocas: Vec<(i64, i64)> = Vec::new();
        let pares: Vec<(Option<Elemento>, Option<Elemento>)> =
            comando.pares.iter().map(|(a, d)| if voltar { (d.clone(), a.clone()) } else { (a.clone(), d.clone()) }).collect();
        // Primeiro o que sai, depois o que muda, por fim o que volta (itens antes das ligações).
        let saindo: HashSet<i64> = pares.iter().filter_map(|(de, para)| if para.is_none() { de.as_ref().map(|e| e.id) } else { None }).collect();
        if !saindo.is_empty() {
            self.remover_local(&saindo);
        }
        for (de, para) in &pares {
            if let (Some(_), Some(alvo)) = (de, para) {
                let Some(atual) = self.buscar(alvo.id).cloned() else { continue };
                let campos = Mudou::entre(&atual, alvo);
                let alvo = alvo.clone();
                self.mudar(alvo.id, campos, |e| {
                    let (id, versao, autor, agente) = (e.id, e.versao, e.autor.clone(), e.agente_id);
                    *e = alvo;
                    (e.id, e.versao, e.autor, e.agente_id) = (id, versao, autor, agente);
                });
                tocados.push(atual.id);
            }
        }
        let mut voltando: Vec<Elemento> = pares.iter().filter_map(|(de, para)| if de.is_none() { para.clone() } else { None }).collect();
        voltando.sort_by_key(|e| e.tipo == TipoElemento::Ligacao);
        for mut e in voltando {
            // As pontas podem ter sido recriadas neste mesmo passo.
            for (antigo, novo) in &trocas {
                if e.de == *antigo {
                    e.de = *novo;
                }
                if e.para == *antigo {
                    e.para = *novo;
                }
            }
            if e.tipo == TipoElemento::Ligacao && (self.buscar(e.de).is_none() || self.buscar(e.para).is_none()) {
                continue;
            }
            let antigo = e.id;
            let novo = self.recriar(e);
            trocas.push((antigo, novo));
            tocados.push(novo);
        }
        (tocados, trocas)
    }

    /// Troca um id por outro em todo lugar.
    pub fn trocar_id(&mut self, antigo: i64, novo: i64) {
        if antigo == novo {
            return;
        }
        // O id novo já está na lista (um eco que entrou antes da resposta):
        // fica o item da tela, que tem o que você mudou depois.
        if self.indice(antigo).is_some() {
            self.elementos.retain(|e| e.id != novo);
        }
        for e in &mut self.elementos {
            if e.id == antigo {
                e.id = novo;
            }
            if e.de == antigo {
                e.de = novo;
            }
            if e.para == antigo {
                e.para = novo;
            }
        }
        for t in &mut self.criar {
            if *t == antigo {
                *t = novo;
            }
        }
        if let Some(c) = self.alterar.remove(&antigo) {
            self.alterar.insert(novo, c);
        }
        for pilha in [&mut self.desfazer, &mut self.refazer] {
            for comando in pilha.iter_mut() {
                trocar_no_comando(comando, antigo, novo);
            }
        }
        if self.nao_salvos.remove(&antigo) {
            self.nao_salvos.insert(novo);
        }
        if let Some(r) = self.remotos.remove(&antigo) {
            self.remotos.insert(novo, r);
        }
        if self.novos.remove(&antigo) {
            self.novos.insert(novo);
        }
    }
}

fn trocar_no_comando(comando: &mut Comando, antigo: i64, novo: i64) {
    for (a, d) in &mut comando.pares {
        for e in [a, d].into_iter().flatten() {
            if e.id == antigo {
                e.id = novo;
            }
            if e.de == antigo {
                e.de = novo;
            }
            if e.para == antigo {
                e.para = novo;
            }
        }
    }
}

/// O ref de um item criado: "t3" para o id -3.
pub fn nome_ref(temporario: i64) -> String {
    format!("t{}", -temporario)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn elemento(id: i64, x: f32) -> Elemento {
        Elemento {
            id,
            lousa_id: 1,
            tipo: TipoElemento::Nota,
            x,
            y: 0.0,
            largura: 240.0,
            altura: 120.0,
            z: id,
            cor: "amarelo".into(),
            versao: 1,
            ..Default::default()
        }
    }

    fn ligacao(id: i64, de: i64, para: i64) -> Elemento {
        Elemento { id, lousa_id: 1, tipo: TipoElemento::Ligacao, de, para, largura: 16.0, altura: 16.0, z: id, versao: 1, ..Default::default() }
    }

    fn modelo() -> Modelo {
        Modelo::novo(api::LousaAberta {
            lousa: api::InfoLousa { id: 1, dono: api::DonoLousa { workspace_id: 1, tarefa_id: 0 } },
            elementos: vec![elemento(10, 0.0), elemento(11, 300.0), ligacao(12, 10, 11)],
        })
    }

    fn ok(elementos: Vec<Elemento>, removidos: Vec<i64>, refs: &[(&str, i64)]) -> Result<api::LoteGravado, String> {
        Ok(api::LoteGravado::Ok { elementos, removidos, refs: refs.iter().map(|(n, id)| (n.to_string(), *id)).collect() })
    }

    #[test]
    fn aplicar_do_nucleo_e_idempotente_pela_versao() {
        let mut m = modelo();
        let mut movido = elemento(10, 50.0);
        movido.versao = 2;
        assert!(m.aplicar_remoto(vec![movido.clone()], &[]));
        // A mesma mensagem de novo (o eco) não muda nada.
        assert!(!m.aplicar_remoto(vec![movido.clone()], &[]));
        let mut velho = elemento(10, 999.0);
        velho.versao = 1;
        assert!(!m.aplicar_remoto(vec![velho], &[]));
        assert_eq!(m.buscar(10).unwrap().x, 50.0);
        // Item de outra lousa não entra.
        let mut outro = elemento(99, 0.0);
        outro.lousa_id = 2;
        assert!(!m.aplicar_remoto(vec![outro], &[]));
        assert!(m.aplicar_remoto(vec![], &[12]));
        assert!(m.buscar(12).is_none());
        // Item com mudança sua: fica como está até o próximo lote.
        m.mudar(11, Mudou::POSICAO, |e| e.x = 1.0);
        let mut de_fora = elemento(11, 777.0);
        de_fora.versao = 2;
        assert!(!m.aplicar_remoto(vec![de_fora], &[]));
        assert_eq!(m.buscar(11).unwrap().x, 1.0);
        // O eco não vira conflito (só um 409 vira).
        assert!(m.remotos.is_empty());
    }

    #[test]
    fn criar_mandar_e_receber_o_id() {
        let mut m = modelo();
        let a = m.criar_local(elemento(0, 5.0));
        let b = m.criar_local(elemento(0, 6.0));
        let l = m.criar_local(ligacao(0, a, b));
        assert!(a < 0 && b < 0 && l < 0 && m.buscar(l).unwrap().z > m.buscar(a).unwrap().z);
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert_eq!(lote.operacoes.len(), 3);
        let Operacao::Criar { elemento: e, .. } = &lote.operacoes[2] else { panic!() };
        assert_eq!((e.de.clone(), e.para.clone()), (Some(Ponta::Ref(nome_ref(a))), Some(Ponta::Ref(nome_ref(b)))));
        // Um lote por vez.
        m.mudar(10, Mudou::POSICAO, |e| e.x = 7.0);
        assert!(m.montar_lote(&HashSet::new()).is_none());
        let mut ea = elemento(20, 5.0);
        ea.z = 13;
        let r = m.receber_lote(ok(vec![ea], vec![], &[(&nome_ref(a), 20), (&nome_ref(b), 21), (&nome_ref(l), 22)]), None);
        assert_eq!(r.trocas.len(), 3);
        assert!(m.buscar(20).is_some() && m.buscar(a).is_none());
        let lig = m.buscar(22).unwrap();
        assert_eq!((lig.de, lig.para), (20, 21));
        // O que mudou enquanto o lote estava no ar vai no próximo.
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(matches!(&lote.operacoes[..], [Operacao::Alterar { id: 10, campos, .. }] if campos.x == Some(7.0) && campos.texto.is_none()));
    }

    #[test]
    fn mudanca_feita_durante_o_lote_fica_na_tela() {
        let mut m = modelo();
        m.mudar(10, Mudou::POSICAO, |e| e.x = 100.0);
        m.montar_lote(&HashSet::new()).unwrap();
        m.mudar(10, Mudou::POSICAO, |e| e.x = 200.0);
        let mut gravado = elemento(10, 100.0);
        gravado.versao = 2;
        m.receber_lote(ok(vec![gravado], vec![], &[]), None);
        let e = m.buscar(10).unwrap();
        assert_eq!((e.x, e.versao), (200.0, 2));
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(matches!(&lote.operacoes[..], [Operacao::Alterar { id: 10, versao: 2, .. }]));
    }

    #[test]
    fn conflito_aplica_o_nucleo_e_devolve_o_resto_a_fila() {
        let mut m = modelo();
        m.mudar(10, Mudou::POSICAO, |e| e.x = 100.0);
        m.mudar(11, Mudou::COR, |e| e.cor = "azul".into());
        m.montar_lote(&HashSet::new()).unwrap();
        let mut de_fora = elemento(10, 555.0);
        de_fora.versao = 3;
        let r = m.receber_lote(Ok(api::LoteGravado::Mudou { elementos: vec![de_fora], removidos: vec![] }), None);
        assert!(r.mudou_fora && !r.nao_desfez);
        assert_eq!(m.buscar(10).unwrap().x, 555.0);
        // A cor do outro item não foi gravada: volta para a fila.
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(matches!(&lote.operacoes[..], [Operacao::Alterar { id: 11, .. }]));
    }

    #[test]
    fn conflito_no_texto_aberto_espera_a_edicao() {
        let mut m = modelo();
        m.mudar(10, Mudou::TEXTO, |e| e.texto = "meu".into());
        m.montar_lote(&HashSet::new()).unwrap();
        let mut de_fora = elemento(10, 0.0);
        (de_fora.versao, de_fora.texto) = (2, "do agente".into());
        let r = m.receber_lote(Ok(api::LoteGravado::Mudou { elementos: vec![de_fora], removidos: vec![] }), Some(10));
        assert_eq!(r.conflitos, vec![10]);
        assert_eq!(m.buscar(10).unwrap().texto, "meu");
        // Enquanto edita, o item não vai.
        assert!(m.montar_lote(&[10].into()).is_none());
        m.resolver_conflito(10, "meu\n\ndo agente".into());
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(matches!(&lote.operacoes[..], [Operacao::Alterar { id: 10, versao: 2, campos }] if campos.texto.as_deref() == Some("meu\n\ndo agente")));
    }

    #[test]
    fn erro_guarda_a_fila_ate_tentar_de_novo() {
        let mut m = modelo();
        m.criar_local(elemento(0, 1.0));
        m.montar_lote(&HashSet::new()).unwrap();
        let r = m.receber_lote(Err("Parece ter senha ou chave; não salvei este texto".into()), None);
        assert!(r.erro.is_some() && m.falhou.is_some() && m.nao_salvos.len() == 1);
        assert!(m.montar_lote(&HashSet::new()).is_none());
        m.tentar_de_novo();
        assert_eq!(m.montar_lote(&HashSet::new()).unwrap().operacoes.len(), 1);
    }

    #[test]
    fn remover_leva_as_ligacoes_e_desfazer_recria_com_ids_novos() {
        let mut m = modelo();
        let saiu = m.remover_local(&[10].into());
        assert_eq!(saiu.len(), 2);
        m.registrar(Comando { numero: 0, pares: saiu.into_iter().map(|e| (Some(e), None)).collect() });
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert_eq!(lote.operacoes.len(), 2);
        m.receber_lote(ok(vec![], vec![10, 12], &[]), None);
        // Desfazer: o item e a ligação voltam, com ids novos e a ligação apontando para o novo.
        let tocados = m.desfazer().unwrap();
        assert_eq!(tocados.len(), 2);
        let novo = m.elementos.iter().find(|e| e.tipo == TipoElemento::Nota && e.id < 0).unwrap().id;
        let lig = m.elementos.iter().find(|e| e.tipo == TipoElemento::Ligacao).unwrap().clone();
        assert_eq!((lig.de, lig.para), (novo, 11));
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(lote.desfazer == Some(true));
        let lig_temp = lig.id;
        m.receber_lote(ok(vec![], vec![], &[(&nome_ref(novo), 30), (&nome_ref(lig_temp), 31)]), None);
        // Refazer remove de novo os itens pelos ids verdadeiros.
        m.refazer().unwrap();
        assert!(m.buscar(30).is_none() && m.buscar(31).is_none());
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        let removidos: HashSet<i64> = lote.operacoes.iter().filter_map(|o| if let Operacao::Remover { id, .. } = o { Some(*id) } else { None }).collect();
        assert_eq!(removidos, [30, 31].into());
    }

    #[test]
    fn desfazer_uma_mudanca_volta_os_campos() {
        let mut m = modelo();
        let antes = m.buscar(10).cloned();
        m.mudar(10, Mudou::POSICAO.com(Mudou::COR), |e| {
            e.x = 400.0;
            e.cor = "rosa".into();
        });
        m.registrar(Comando { numero: 0, pares: vec![(antes, m.buscar(10).cloned())] });
        m.desfazer();
        let e = m.buscar(10).unwrap();
        assert_eq!((e.x, e.cor.as_str()), (0.0, "amarelo"));
        m.refazer();
        assert_eq!(m.buscar(10).unwrap().cor, "rosa");
        assert!(!m.pode_refazer() && m.pode_desfazer());
    }

    #[test]
    fn desfazer_com_409_sai_da_pilha() {
        let mut m = modelo();
        let antes = m.buscar(10).cloned();
        m.mudar(10, Mudou::POSICAO, |e| e.x = 400.0);
        m.registrar(Comando { numero: 0, pares: vec![(antes, m.buscar(10).cloned())] });
        m.montar_lote(&HashSet::new()).unwrap();
        let mut gravado = elemento(10, 400.0);
        gravado.versao = 2;
        m.receber_lote(ok(vec![gravado], vec![], &[]), None);
        m.desfazer();
        m.montar_lote(&HashSet::new()).unwrap();
        let mut de_fora = elemento(10, 9.0);
        de_fora.versao = 3;
        let r = m.receber_lote(Ok(api::LoteGravado::Mudou { elementos: vec![de_fora], removidos: vec![] }), None);
        assert!(r.nao_desfez && !m.pode_refazer());
        assert_eq!(m.buscar(10).unwrap().x, 9.0);
    }

    #[test]
    fn texto_digitado_durante_a_criacao_nao_se_perde() {
        let mut m = modelo();
        let a = m.criar_local(elemento(0, 1.0));
        m.mudar(a, Mudou::TEXTO, |e| e.texto = "# Pa".into());
        m.montar_lote(&HashSet::new()).unwrap();
        // Continua digitando enquanto a criação está no ar.
        m.mudar(a, Mudou::TEXTO, |e| e.texto = "# Passos".into());
        assert!(!m.na_fila(), "o id ainda é provisório: nada a mandar");
        let mut criado = elemento(50, 1.0);
        criado.texto = "# Pa".into();
        m.receber_lote(ok(vec![criado], vec![], &[(&nome_ref(a), 50)]), None);
        let e = m.buscar(50).unwrap();
        assert_eq!((e.texto.as_str(), e.versao), ("# Passos", 1));
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(matches!(&lote.operacoes[..], [Operacao::Alterar { id: 50, versao: 1, campos }] if campos.texto.as_deref() == Some("# Passos")));
    }

    #[test]
    fn apagar_o_que_ainda_esta_no_ar() {
        let mut m = modelo();
        let a = m.criar_local(elemento(0, 1.0));
        m.montar_lote(&HashSet::new()).unwrap();
        m.remover_local(&[a].into());
        m.receber_lote(ok(vec![], vec![], &[(&nome_ref(a), 40)]), None);
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(matches!(&lote.operacoes[..], [Operacao::Remover { id: 40, .. }]));
    }

    #[test]
    fn eco_da_criacao_antes_da_resposta_nao_duplica() {
        let mut m = modelo();
        let a = m.criar_local(elemento(0, 1.0));
        m.mudar(a, Mudou::TEXTO, |e| e.texto = "primeiro".into());
        m.montar_lote(&HashSet::new()).unwrap();
        // Continua digitando; o eco "lousa.mudou" chega antes da resposta, já com o id verdadeiro.
        m.mudar(a, Mudou::TEXTO, |e| e.texto = "primeiro e mais".into());
        let mut eco = elemento(50, 1.0);
        eco.texto = "primeiro".into();
        // Um item de outra tela chega no mesmo intervalo.
        let outro = elemento(60, 900.0);
        m.aplicar_remoto(vec![eco.clone(), outro], &[]);
        assert!(m.buscar(50).is_none(), "o eco espera a resposta");
        m.receber_lote(ok(vec![eco.clone()], vec![], &[(&nome_ref(a), 50)]), None);
        assert_eq!(m.elementos.iter().filter(|e| e.id == 50).count(), 1);
        assert_eq!(m.buscar(50).unwrap().texto, "primeiro e mais");
        assert!(m.buscar(60).is_some(), "o de outra tela entra depois da resposta");
        // O eco atrasado (depois da resposta) também não duplica.
        m.aplicar_remoto(vec![eco], &[]);
        assert_eq!(m.elementos.iter().filter(|e| e.id == 50).count(), 1);
        // Apagar manda o id uma vez só.
        m.montar_lote(&HashSet::new()).unwrap();
        m.receber_lote(ok(vec![], vec![], &[]), None);
        m.remover_local(&[50].into());
        let lote = m.montar_lote(&HashSet::new()).unwrap();
        assert!(matches!(&lote.operacoes[..], [Operacao::Remover { id: 50, .. }]));
    }

    #[test]
    fn trocar_id_tira_o_eco_que_entrou_antes() {
        let mut m = modelo();
        let a = m.criar_local(elemento(0, 1.0));
        // O eco entrou sem lote no ar (a lista ficou com o id verdadeiro e o provisório).
        m.elementos.push(elemento(70, 1.0));
        m.trocar_id(a, 70);
        assert_eq!(m.elementos.iter().filter(|e| e.id == 70).count(), 1);
        let ids: HashSet<i64> = [70, 70].into();
        m.remover_local(&ids);
        m.remover_local(&ids);
        assert_eq!(m.remover.len(), 1);
    }

    #[test]
    fn desfazer_do_aviso_desfaz_a_remocao_anunciada() {
        let mut m = modelo();
        let saiu = m.remover_local(&[10].into());
        let numero = m.registrar(Comando { numero: 0, pares: saiu.into_iter().map(|e| (Some(e), None)).collect() });
        // Depois de apagar, a dona muda a cor de outro item.
        let antes = m.buscar(11).cloned();
        m.mudar(11, Mudou::COR, |e| e.cor = "azul".into());
        m.registrar(Comando { numero: 0, pares: vec![(antes, m.buscar(11).cloned())] });
        let tocados = m.desfazer_comando(numero).unwrap();
        assert_eq!(tocados.len(), 2, "o item e a ligação voltam");
        assert_eq!(m.buscar(11).unwrap().cor, "azul", "a cor fica");
        assert!(m.elementos.iter().any(|e| e.tipo == TipoElemento::Nota && e.id < 0));
        // De novo: já não está na pilha.
        assert!(m.desfazer_comando(numero).is_none());
        // O Ctrl+Z seguinte desfaz a cor.
        m.desfazer();
        assert_eq!(m.buscar(11).unwrap().cor, "amarelo");
    }

    #[test]
    fn lote_grande_vai_em_partes() {
        let mut m = modelo();
        for i in 0..(MAX_OPERACOES + 20) {
            m.criar_local(elemento(0, i as f32));
        }
        let primeiro = m.montar_lote(&HashSet::new()).unwrap();
        assert_eq!(primeiro.operacoes.len(), MAX_OPERACOES);
        m.receber_lote(ok(vec![], vec![], &[]), None);
        assert_eq!(m.montar_lote(&HashSet::new()).unwrap().operacoes.len(), 20);
    }
}
