//! O que a abelha sabe do perfil: um retrato pequeno do quadro (tarefas,
//! agentes e projetos, só os campos que ela usa), atualizado pelos eventos,
//! e o que sai dele: a contagem do resumo, a tarefa que pede você e os
//! balões. Nada aqui fala com o núcleo nem desenha.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use mascote::Estado;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Projeto {
    pub id: i64,
    pub nome: String,
    /// "git" ou "pasta" (sem branches) e a branch base: a tarefa nova vai nela.
    #[serde(default)]
    pub tipo: String,
    #[serde(default)]
    pub branch_padrao: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Tarefa {
    pub id: i64,
    pub projeto_id: i64,
    pub titulo: String,
    pub coluna: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Fim {
    #[serde(default)]
    pub erro: bool,
    #[serde(default)]
    pub texto: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Agente {
    pub id: i64,
    pub tarefa_id: i64,
    pub ferramenta: String,
    pub papel: String,
    #[serde(default)]
    pub ativo: bool,
    #[serde(default)]
    pub estado: String,
    #[serde(default)]
    pub motivo: String,
}

/// Os comandos falados do perfil (veja o núcleo: dados.OpcoesVoz).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct OpcoesVoz {
    #[serde(default)]
    pub voz_comandos: bool,
    #[serde(default)]
    pub voz_abelha: bool,
    #[serde(default)]
    pub voz_efeito: bool,
    #[serde(default)]
    pub voz_nome: bool,
    #[serde(default = "sensibilidade_padrao")]
    pub voz_sensibilidade: u8,
    /// A pergunta livre com IA: o que não é comando vai ao Claude Code (usa tokens).
    #[serde(default)]
    pub voz_ia: bool,
}

fn sensibilidade_padrao() -> u8 {
    5
}

impl Default for OpcoesVoz {
    fn default() -> Self {
        OpcoesVoz { voz_comandos: false, voz_abelha: false, voz_efeito: false, voz_nome: false, voz_sensibilidade: sensibilidade_padrao(), voz_ia: false }
    }
}

/// O retrato do perfil (`GET /v1/perfis/{id}/quadro`), só com o que a abelha usa.
#[derive(Debug, Deserialize)]
pub struct Quadro {
    pub seq: u64,
    #[serde(default)]
    pub projetos: Vec<Projeto>,
    #[serde(default)]
    pub tarefas: Vec<Tarefa>,
    #[serde(default)]
    pub agentes: Vec<Agente>,
}

/// As mensagens do WebSocket de eventos que mudam o resumo. As outras são ignoradas.
#[derive(Debug, Deserialize)]
#[serde(tag = "tipo")]
pub enum Evento {
    #[serde(rename = "ola")]
    Ola {},
    #[serde(rename = "recarregar")]
    Recarregar {},
    #[serde(rename = "projeto.criado")]
    ProjetoCriado { seq: u64, projeto: Projeto },
    #[serde(rename = "projeto.removido")]
    ProjetoRemovido { seq: u64, projeto_id: i64 },
    #[serde(rename = "tarefa.criada", alias = "tarefa.atualizada")]
    TarefaMudou { seq: u64, tarefa: Tarefa },
    #[serde(rename = "tarefa.removida")]
    TarefaRemovida { seq: u64, tarefa_id: i64 },
    #[serde(rename = "agente.criado")]
    AgenteCriado { seq: u64, agente: Agente },
    #[serde(rename = "agente.removido")]
    AgenteRemovido { seq: u64, agente_id: i64 },
    #[serde(rename = "agente.iniciou")]
    AgenteIniciou { seq: u64, agente_id: i64 },
    #[serde(rename = "agente.estado")]
    AgenteEstado {
        seq: u64,
        agente_id: i64,
        estado: String,
        #[serde(default)]
        motivo: String,
    },
    #[serde(rename = "agente.terminou")]
    AgenteTerminou { seq: u64, agente_id: i64, fim: Fim },
    #[serde(rename = "perfil.mascote")]
    PerfilMascote { modelo: String, nome: String },
    #[serde(rename = "perfil.abelha_fixa")]
    PerfilAbelhaFixa { ligada: bool },
    #[serde(rename = "perfil.voz")]
    PerfilVoz { voz: OpcoesVoz },
    #[serde(other)]
    Outro,
}

impl Evento {
    /// O número da mensagem; as que não entram no retrato (0) valem sempre.
    pub fn seq(&self) -> u64 {
        match self {
            Evento::ProjetoCriado { seq, .. }
            | Evento::ProjetoRemovido { seq, .. }
            | Evento::TarefaMudou { seq, .. }
            | Evento::TarefaRemovida { seq, .. }
            | Evento::AgenteCriado { seq, .. }
            | Evento::AgenteRemovido { seq, .. }
            | Evento::AgenteIniciou { seq, .. }
            | Evento::AgenteEstado { seq, .. }
            | Evento::AgenteTerminou { seq, .. } => *seq,
            _ => 0,
        }
    }
}

/// O tipo de um balão (e do item em destaque no resumo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tipo {
    Erro,
    Espera,
    Concluiu,
    /// Um agente terminou sem erro (só no diário, não vira balão).
    Terminou,
}

/// Um aviso que sai da abelha: o que aconteceu, onde e a tarefa para abrir.
/// O balão mostra `titulo` e `detalhe`; o resumo põe a tarefa em destaque e
/// o projeto e o que aconteceu embaixo.
#[derive(Clone, Debug, PartialEq)]
pub struct Aviso {
    pub tipo: Tipo,
    pub titulo: String,
    pub detalhe: String,
    /// O título da tarefa e o nome do projeto, soltos, para o resumo.
    pub tarefa_titulo: String,
    pub projeto: String,
    /// Quem é o agente ("Claude Code (dev)"), para a bolinha do resumo; vazio sem agente.
    pub quem: String,
    pub tarefa: i64,
    pub agente: Option<i64>,
}

/// Uma tarefa na lista de trocar a conversa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Escolha {
    pub tarefa: i64,
    pub agente: Option<i64>,
    pub titulo: String,
    pub projeto: String,
    /// "pede aprovação", "trabalhando", "parou", "sem agente"…
    pub estado: String,
    pub espera: bool,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Contagem {
    pub esperam: usize,
    pub trabalhando: usize,
    pub com_erro: usize,
}

#[derive(Default)]
pub struct Resumo {
    projetos: HashMap<i64, String>,
    /// A branch base de cada projeto git (vazia num projeto sem git).
    branches: HashMap<i64, String>,
    tarefas: HashMap<i64, Tarefa>,
    agentes: HashMap<i64, Agente>,
    /// Erros que chegaram com a abelha ligada e você ainda não abriu (os de
    /// antes dela, como na janela, contam como vistos).
    erros: Vec<(i64, String)>,
    /// O que aconteceu com a abelha ligada, do mais velho ao mais novo, para
    /// contar o que se passou enquanto você esteve fora.
    diario: Vec<(Instant, Aviso)>,
    /// O número da última mensagem incluída no retrato.
    pub base: Option<u64>,
}

/// Quantas coisas o diário guarda.
const DIARIO: usize = 60;

/// O que a abelha faz depois de um evento.
#[derive(Debug, PartialEq)]
pub enum Efeito {
    Avisar(Aviso),
    Comemorar,
    Vestir {
        modelo: String,
        nome: String,
    },
    Sair,
    /// Ficou para trás: pedir o retrato de novo.
    Recarregar,
    /// Os comandos falados mudaram.
    Voz(OpcoesVoz),
}

fn nome_da_ferramenta(ferramenta: &str) -> &str {
    match ferramenta {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "opencode" => "OpenCode",
        "shell" => "Terminal",
        outro => outro,
    }
}

fn acao_de_espera(motivo: &str) -> &'static str {
    match motivo {
        "pede aprovação" => "pede aprovação",
        "aprovar consulta" => "quer consultar o banco",
        _ => "espera sua resposta",
    }
}

impl Resumo {
    /// O agente mais novo da tarefa, rodando ou não: é com ele que a conversa
    /// abre (e ele que "Retomar" liga de novo).
    pub fn ultimo_agente(&self, tarefa: i64) -> Option<&Agente> {
        self.agentes.values().filter(|a| a.tarefa_id == tarefa).max_by_key(|a| a.id)
    }

    /// Se a tarefa pede você: está em "Aguardando você" ou tem agente esperando.
    pub fn pede_voce(&self, tarefa: i64) -> bool {
        self.tarefas.get(&tarefa).is_some_and(|t| t.coluna == "aguardando") || self.agentes.values().any(|a| a.tarefa_id == tarefa && Self::espera(a))
    }

    /// Se o agente está rodando agora.
    pub fn ativo(&self, agente: i64) -> bool {
        self.agentes.get(&agente).is_some_and(|a| a.ativo)
    }

    /// O título da tarefa e quem é o agente ("Claude Code (dev)"), mesmo
    /// parado; sem agente, "sem agente".
    pub fn sobre(&self, tarefa: i64, agente: Option<i64>) -> (String, String) {
        let titulo = self.tarefas.get(&tarefa).map(|t| t.titulo.clone()).unwrap_or_else(|| "Tarefa".to_string());
        let quem = agente.and_then(|a| self.agentes.get(&a)).map(Self::quem).unwrap_or_else(|| "sem agente".to_string());
        (titulo, quem)
    }

    /// Os projetos do perfil, por nome, com a branch em que uma tarefa nova entra.
    pub fn projetos_lista(&self) -> Vec<(i64, String, String)> {
        let mut lista: Vec<(i64, String, String)> =
            self.projetos.iter().map(|(id, nome)| (*id, nome.clone(), self.branches.get(id).cloned().unwrap_or_default())).collect();
        lista.sort_by_key(|(_, nome, _)| nome.to_lowercase());
        lista
    }

    fn guardar_branch(&mut self, p: &Projeto) {
        let branch = if p.tipo == "pasta" { String::new() } else { p.branch_padrao.clone() };
        self.branches.insert(p.id, branch);
    }

    pub fn carregar(&mut self, q: Quadro) {
        self.branches.clear();
        for p in &q.projetos {
            self.guardar_branch(p);
        }
        self.projetos = q.projetos.into_iter().map(|p| (p.id, p.nome)).collect();
        self.tarefas = q.tarefas.into_iter().map(|t| (t.id, t)).collect();
        self.agentes = q.agentes.into_iter().map(|a| (a.id, a)).collect();
        // Um erro que não existe mais (agente removido, de novo rodando) sai.
        let agentes = &self.agentes;
        self.erros.retain(|(id, _)| agentes.get(id).is_some_and(|a| !a.ativo));
        self.base = Some(q.seq);
    }

    /// "“Título” · projeto", o detalhe dos balões.
    fn onde(&self, tarefa: i64) -> String {
        match self.tarefas.get(&tarefa) {
            Some(t) => match self.projetos.get(&t.projeto_id) {
                Some(p) => format!("“{}” · {p}", t.titulo),
                None => format!("“{}”", t.titulo),
            },
            None => String::new(),
        }
    }

    /// Monta um aviso com a tarefa e o projeto preenchidos a partir do retrato.
    fn aviso(&self, tipo: Tipo, titulo: String, tarefa: i64, agente: Option<i64>) -> Aviso {
        let t = self.tarefas.get(&tarefa);
        let projeto = t.and_then(|t| self.projetos.get(&t.projeto_id)).cloned().unwrap_or_default();
        let quem = agente.and_then(|a| self.agentes.get(&a)).map(Self::quem).unwrap_or_default();
        Aviso { tipo, titulo, detalhe: self.onde(tarefa), tarefa_titulo: t.map(|t| t.titulo.clone()).unwrap_or_default(), projeto, quem, tarefa, agente }
    }

    /// Guarda no diário o que acabou de acontecer.
    fn anotar(&mut self, aviso: Aviso) {
        if self.diario.len() >= DIARIO {
            self.diario.remove(0);
        }
        self.diario.push((Instant::now(), aviso));
    }

    fn quem(a: &Agente) -> String {
        format!("{} ({})", nome_da_ferramenta(&a.ferramenta), a.papel)
    }

    fn espera(a: &Agente) -> bool {
        a.ativo && a.estado == "aguardando"
    }

    /// Aplica um evento. Os de antes do retrato já estão nele e são ignorados.
    pub fn aplicar(&mut self, evento: Evento) -> Vec<Efeito> {
        let seq = evento.seq();
        if seq != 0 && self.base.is_none_or(|b| seq <= b) {
            return Vec::new();
        }
        let mut efeitos = Vec::new();
        match evento {
            Evento::Ola {} | Evento::Recarregar {} => efeitos.push(Efeito::Recarregar),
            Evento::ProjetoCriado { projeto, .. } => {
                self.guardar_branch(&projeto);
                self.projetos.insert(projeto.id, projeto.nome);
            }
            Evento::ProjetoRemovido { projeto_id, .. } => {
                self.projetos.remove(&projeto_id);
                let fora: HashSet<i64> = self.tarefas.values().filter(|t| t.projeto_id == projeto_id).map(|t| t.id).collect();
                self.tarefas.retain(|id, _| !fora.contains(id));
                self.agentes.retain(|_, a| !fora.contains(&a.tarefa_id));
            }
            Evento::TarefaMudou { tarefa, .. } => {
                let antes = self.tarefas.get(&tarefa.id).map(|t| t.coluna.clone());
                let concluiu = tarefa.coluna == "concluido" && antes.is_some_and(|c| c != "concluido");
                let id = tarefa.id;
                self.tarefas.insert(id, tarefa);
                if concluiu {
                    let aviso = self.aviso(Tipo::Concluiu, "Tarefa concluída".into(), id, None);
                    self.anotar(aviso.clone());
                    efeitos.push(Efeito::Comemorar);
                    efeitos.push(Efeito::Avisar(aviso));
                }
            }
            Evento::TarefaRemovida { tarefa_id, .. } => {
                self.tarefas.remove(&tarefa_id);
                self.agentes.retain(|_, a| a.tarefa_id != tarefa_id);
            }
            Evento::AgenteCriado { agente, .. } => {
                self.agentes.insert(agente.id, agente);
            }
            Evento::AgenteRemovido { agente_id, .. } => {
                self.agentes.remove(&agente_id);
            }
            Evento::AgenteIniciou { agente_id, .. } => {
                if let Some(a) = self.agentes.get_mut(&agente_id) {
                    a.ativo = true;
                    a.estado = "trabalhando".into();
                    a.motivo.clear();
                }
            }
            Evento::AgenteEstado { agente_id, estado, motivo, .. } => {
                let Some(a) = self.agentes.get_mut(&agente_id) else { return efeitos };
                let esperava = Self::espera(a);
                (a.ativo, a.estado, a.motivo) = (true, estado, motivo);
                if !esperava && Self::espera(a) {
                    let a = a.clone();
                    let titulo = format!("{} {}", Self::quem(&a), acao_de_espera(&a.motivo));
                    let aviso = self.aviso(Tipo::Espera, titulo, a.tarefa_id, Some(a.id));
                    self.anotar(aviso.clone());
                    efeitos.push(Efeito::Avisar(aviso));
                }
            }
            Evento::AgenteTerminou { agente_id, fim, .. } => {
                let Some(a) = self.agentes.get_mut(&agente_id) else { return efeitos };
                (a.ativo, a.estado, a.motivo) = (false, String::new(), String::new());
                let a = a.clone();
                if fim.erro {
                    let titulo = if fim.texto.is_empty() { format!("{} parou com erro", Self::quem(&a)) } else { fim.texto };
                    self.erros.retain(|(id, _)| *id != a.id);
                    self.erros.push((a.id, titulo.clone()));
                    let aviso = self.aviso(Tipo::Erro, titulo, a.tarefa_id, Some(a.id));
                    self.anotar(aviso.clone());
                    efeitos.push(Efeito::Avisar(aviso));
                } else {
                    // Sem erro não há balão, mas o diário guarda a frase de fim.
                    let titulo = if fim.texto.is_empty() { format!("{} terminou", Self::quem(&a)) } else { fim.texto };
                    let aviso = self.aviso(Tipo::Terminou, titulo, a.tarefa_id, Some(a.id));
                    self.anotar(aviso);
                }
            }
            Evento::PerfilMascote { modelo, nome } => efeitos.push(Efeito::Vestir { modelo, nome }),
            Evento::PerfilAbelhaFixa { ligada: false } => efeitos.push(Efeito::Sair),
            Evento::PerfilVoz { voz } => efeitos.push(Efeito::Voz(voz)),
            Evento::PerfilAbelhaFixa { ligada: true } | Evento::Outro => {}
        }
        // Um agente que voltou a rodar ou saiu leva o erro dele.
        let agentes = &self.agentes;
        self.erros.retain(|(id, _)| agentes.get(id).is_some_and(|a| !a.ativo));
        efeitos
    }

    /// Você abriu o erro (ou a tarefa dele): deixa de contar.
    pub fn visto(&mut self, tarefa: i64) {
        let agentes = &self.agentes;
        self.erros.retain(|(id, _)| agentes.get(id).is_none_or(|a| a.tarefa_id != tarefa));
    }

    /// O que aconteceu desde `quando` e ainda vale, para o cartão de quando
    /// você volta: os erros que ainda estão de pé, os agentes que ainda
    /// esperam, as tarefas que continuam concluídas e os agentes que
    /// terminaram. Na ordem do resumo (erro, espera, concluiu, terminou),
    /// o mais novo primeiro, cada tarefa uma vez por tipo.
    pub fn desde(&self, quando: Instant) -> Vec<Aviso> {
        let vale = |a: &Aviso| match a.tipo {
            Tipo::Erro => a.agente.is_some_and(|id| self.erros.iter().any(|(e, _)| *e == id)),
            Tipo::Espera => a.agente.and_then(|id| self.agentes.get(&id)).is_some_and(Self::espera),
            Tipo::Concluiu => self.tarefas.get(&a.tarefa).is_some_and(|t| t.coluna == "concluido"),
            Tipo::Terminou => self.tarefas.contains_key(&a.tarefa),
        };
        let mut lista: Vec<Aviso> = Vec::new();
        for tipo in [Tipo::Erro, Tipo::Espera, Tipo::Concluiu, Tipo::Terminou] {
            for (_, a) in self.diario.iter().rev().filter(|(em, a)| *em >= quando && a.tipo == tipo) {
                if vale(a) && !lista.iter().any(|l| l.tarefa == a.tarefa && l.tipo == a.tipo) {
                    lista.push(a.clone());
                }
            }
        }
        lista
    }

    pub fn contagem(&self) -> Contagem {
        let agentes = self.agentes.values().filter(|a| self.tarefas.contains_key(&a.tarefa_id));
        let esperando = agentes.clone().filter(|a| Self::espera(a)).count();
        let cartoes = self.tarefas.values().filter(|t| t.coluna == "aguardando").count();
        Contagem {
            // Como na janela: os agentes esperando ou, se forem mais, os cartões em "Aguardando você".
            esperam: esperando.max(cartoes),
            trabalhando: agentes.filter(|a| a.ativo && a.estado == "trabalhando").count(),
            com_erro: self.erros.len(),
        }
    }

    /// Todas as tarefas abertas (as concluídas ficam de fora), para trocar a
    /// conversa: as que esperam você primeiro, depois as trabalhando, depois
    /// as outras; dentro do grupo, pelo projeto e pelo título.
    pub fn tarefas_lista(&self) -> Vec<Escolha> {
        let mut lista: Vec<Escolha> = self
            .tarefas
            .values()
            .filter(|t| t.coluna != "concluido")
            .map(|t| {
                let agente = self.ultimo_agente(t.id);
                let estado = match agente {
                    Some(a) if Self::espera(a) => acao_de_espera(&a.motivo).to_string(),
                    _ if t.coluna == "aguardando" => "espera você".to_string(),
                    Some(a) if a.ativo => "trabalhando".to_string(),
                    Some(_) => "parou".to_string(),
                    None => "sem agente".to_string(),
                };
                let projeto = self.projetos.get(&t.projeto_id).cloned().unwrap_or_default();
                Escolha { tarefa: t.id, agente: agente.map(|a| a.id), titulo: t.titulo.clone(), projeto, estado, espera: self.pede_voce(t.id) }
            })
            .collect();
        let grupo = |e: &Escolha| {
            if e.espera {
                0
            } else if e.estado == "trabalhando" {
                1
            } else {
                2
            }
        };
        lista.sort_by(|a, b| {
            grupo(a)
                .cmp(&grupo(b))
                .then_with(|| a.projeto.to_lowercase().cmp(&b.projeto.to_lowercase()))
                .then_with(|| a.titulo.to_lowercase().cmp(&b.titulo.to_lowercase()))
        });
        lista
    }

    /// O estado da abelha: erro, espera, trabalho ou sono, nessa ordem.
    pub fn estado(&self) -> Estado {
        let c = self.contagem();
        if c.com_erro > 0 {
            Estado::Bugado
        } else if c.esperam > 0 {
            Estado::Aguardando
        } else if c.trabalhando > 0 {
            Estado::Trabalhando
        } else {
            Estado::Dormindo
        }
    }

    /// O que mais pede você, para o destaque do balão: o primeiro da lista do resumo.
    #[cfg(test)]
    pub fn destaque(&self) -> Option<Aviso> {
        self.destaques().into_iter().next()
    }

    /// Tudo o que pede você, na ordem do resumo: os erros (o mais novo
    /// primeiro), os agentes esperando e os cartões em "Aguardando você"
    /// que não têm agente esperando (cada tarefa uma vez).
    pub fn destaques(&self) -> Vec<Aviso> {
        let mut lista = Vec::new();
        for (agente, titulo) in self.erros.iter().rev() {
            if let Some(a) = self.agentes.get(agente) {
                lista.push(self.aviso(Tipo::Erro, titulo.clone(), a.tarefa_id, Some(a.id)));
            }
        }
        let mut esperando: Vec<&Agente> = self.agentes.values().filter(|a| Self::espera(a) && self.tarefas.contains_key(&a.tarefa_id)).collect();
        esperando.sort_by_key(|a| a.id);
        for a in esperando {
            lista.push(self.aviso(Tipo::Espera, format!("{} {}", Self::quem(a), acao_de_espera(&a.motivo)), a.tarefa_id, Some(a.id)));
        }
        let mut cartoes: Vec<&Tarefa> = self.tarefas.values().filter(|t| t.coluna == "aguardando").collect();
        cartoes.sort_by_key(|t| t.id);
        for t in cartoes {
            if !lista.iter().any(|a| a.tarefa == t.id && a.tipo == Tipo::Espera) {
                lista.push(self.aviso(Tipo::Espera, "esperando você, sem agente".into(), t.id, None));
            }
        }
        lista
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn evento(json: &str) -> Evento {
        serde_json::from_str(json).expect(json)
    }

    fn quadro() -> Resumo {
        let mut r = Resumo::default();
        let q: Quadro = serde_json::from_str(
            r#"{"seq":5,"projetos":[{"id":1,"nome":"loja-web","caminho":"/x"}],
               "tarefas":[{"id":10,"projeto_id":1,"titulo":"Corrigir frete","coluna":"trabalhando","branch":"main"},
                          {"id":11,"projeto_id":1,"titulo":"Tela de cupons","coluna":"revisao","branch":"main"}],
               "agentes":[{"id":7,"tarefa_id":10,"ferramenta":"claude","papel":"dev","ativo":true,"estado":"trabalhando"},
                          {"id":8,"tarefa_id":11,"ferramenta":"codex","papel":"revisor","ativo":false,"ultimo_fim":{"erro":true,"texto":"x"}}],
               "pedidos":[]}"#,
        )
        .unwrap();
        r.carregar(q);
        r
    }

    #[test]
    fn retrato_conta_e_erros_de_antes_contam_como_vistos() {
        let r = quadro();
        assert_eq!(r.contagem(), Contagem { esperam: 0, trabalhando: 1, com_erro: 0 });
        assert_eq!(r.estado(), Estado::Trabalhando);
        assert_eq!(r.destaque(), None);
    }

    #[test]
    fn agente_esperando_vira_balao_uma_vez_e_destaque() {
        let mut r = quadro();
        let e = r.aplicar(evento(r#"{"tipo":"agente.estado","seq":6,"agente_id":7,"tarefa_id":10,"estado":"aguardando","motivo":"pede aprovação"}"#));
        let aviso = Aviso {
            tipo: Tipo::Espera,
            titulo: "Claude Code (dev) pede aprovação".into(),
            detalhe: "“Corrigir frete” · loja-web".into(),
            tarefa_titulo: "Corrigir frete".into(),
            projeto: "loja-web".into(),
            quem: "Claude Code (dev)".into(),
            tarefa: 10,
            agente: Some(7),
        };
        assert_eq!(e, vec![Efeito::Avisar(aviso.clone())]);
        // O mesmo estado de novo não repete o balão.
        assert!(
            r.aplicar(evento(r#"{"tipo":"agente.estado","seq":7,"agente_id":7,"tarefa_id":10,"estado":"aguardando","motivo":"pede aprovação"}"#)).is_empty()
        );
        assert_eq!(r.contagem(), Contagem { esperam: 1, trabalhando: 0, com_erro: 0 });
        assert_eq!(r.estado(), Estado::Aguardando);
        assert_eq!(r.destaque(), Some(aviso));
    }

    #[test]
    fn resumo_lista_todos_que_esperam_cada_tarefa_uma_vez() {
        let mut r = quadro();
        r.aplicar(evento(r#"{"tipo":"agente.estado","seq":6,"agente_id":7,"tarefa_id":10,"estado":"aguardando","motivo":"pede aprovação"}"#));
        // A outra tarefa vai para "Aguardando você" sem agente esperando.
        r.aplicar(evento(
            r#"{"tipo":"tarefa.atualizada","seq":7,"tarefa":{"id":11,"projeto_id":1,"titulo":"Tela de cupons","coluna":"aguardando","branch":"main"}}"#,
        ));
        let lista = r.destaques();
        assert_eq!(lista.iter().map(|a| (a.tarefa, a.agente)).collect::<Vec<_>>(), vec![(10, Some(7)), (11, None)]);
        assert_eq!(r.destaque().map(|a| a.tarefa), Some(10));
    }

    #[test]
    fn erro_novo_conta_ate_ser_visto_ou_o_agente_voltar() {
        let mut r = quadro();
        let e = r.aplicar(evento(
            r#"{"tipo":"agente.terminou","seq":6,"agente_id":7,"tarefa_id":10,"fim":{"codigo":1,"erro":true,"motivo":"erro","hora":"14:40","texto":"Claude Code (dev) parou com erro (código 1)"}}"#,
        ));
        assert!(matches!(&e[..], [Efeito::Avisar(Aviso { tipo: Tipo::Erro, tarefa: 10, .. })]));
        assert_eq!(r.contagem().com_erro, 1);
        assert_eq!(r.estado(), Estado::Bugado);
        assert_eq!(r.destaque().map(|a| a.tipo), Some(Tipo::Erro));
        r.visto(10);
        assert_eq!(r.contagem().com_erro, 0);

        r.aplicar(evento(r#"{"tipo":"agente.terminou","seq":7,"agente_id":7,"tarefa_id":10,"fim":{"erro":true,"texto":"parou"}}"#));
        assert_eq!(r.contagem().com_erro, 1);
        r.aplicar(evento(r#"{"tipo":"agente.iniciou","seq":8,"agente_id":7,"tarefa_id":10,"desde_hora":"14:41"}"#));
        assert_eq!(r.contagem(), Contagem { esperam: 0, trabalhando: 1, com_erro: 0 });
    }

    #[test]
    fn concluir_comemora_e_eventos_velhos_sao_ignorados() {
        let mut r = quadro();
        // Já incluído no retrato (seq 5).
        assert!(
            r.aplicar(evento(r#"{"tipo":"tarefa.atualizada","seq":5,"tarefa":{"id":11,"projeto_id":1,"titulo":"Tela de cupons","coluna":"concluido"}}"#))
                .is_empty()
        );
        let e = r.aplicar(evento(r#"{"tipo":"tarefa.atualizada","seq":9,"tarefa":{"id":11,"projeto_id":1,"titulo":"Tela de cupons","coluna":"concluido","branch":"x"},"origem":"automatico"}"#));
        assert_eq!(e.first(), Some(&Efeito::Comemorar));
        assert!(matches!(&e[1], Efeito::Avisar(Aviso { tipo: Tipo::Concluiu, detalhe, .. }) if detalhe == "“Tela de cupons” · loja-web"));
        // De novo em Concluído: nada.
        assert!(
            r.aplicar(evento(r#"{"tipo":"tarefa.atualizada","seq":10,"tarefa":{"id":11,"projeto_id":1,"titulo":"Tela de cupons","coluna":"concluido"}}"#))
                .is_empty()
        );
    }

    #[test]
    fn diario_conta_o_que_aconteceu_desde_a_pausa_e_ainda_vale() {
        let mut r = quadro();
        let antes = Instant::now();
        // Terminou sem erro (não vira balão), outra tarefa foi concluída e o agente dela esperou.
        assert!(
            r.aplicar(evento(r#"{"tipo":"agente.terminou","seq":6,"agente_id":7,"tarefa_id":10,"fim":{"erro":false,"texto":"Claude Code (dev) terminou"}}"#))
                .is_empty()
        );
        r.aplicar(evento(r#"{"tipo":"tarefa.atualizada","seq":7,"tarefa":{"id":11,"projeto_id":1,"titulo":"Tela de cupons","coluna":"concluido"}}"#));
        r.aplicar(evento(r#"{"tipo":"agente.iniciou","seq":8,"agente_id":8,"tarefa_id":11}"#));
        r.aplicar(evento(r#"{"tipo":"agente.estado","seq":9,"agente_id":8,"tarefa_id":11,"estado":"aguardando","motivo":"pede aprovação"}"#));
        let lista = r.desde(antes);
        assert_eq!(lista.iter().map(|a| (a.tipo, a.tarefa)).collect::<Vec<_>>(), vec![(Tipo::Espera, 11), (Tipo::Concluiu, 11), (Tipo::Terminou, 10)]);
        assert_eq!(lista[2].titulo, "Claude Code (dev) terminou");
        // O agente voltou a trabalhar: a espera já não vale. Nada aconteceu depois de agora.
        r.aplicar(evento(r#"{"tipo":"agente.estado","seq":10,"agente_id":8,"tarefa_id":11,"estado":"trabalhando"}"#));
        assert_eq!(r.desde(antes).iter().map(|a| a.tipo).collect::<Vec<_>>(), vec![Tipo::Concluiu, Tipo::Terminou]);
        assert!(r.desde(Instant::now()).is_empty());
    }

    #[test]
    fn perfil_e_mensagens_desconhecidas() {
        let mut r = quadro();
        assert_eq!(r.aplicar(evento(r#"{"tipo":"perfil.abelha_fixa","seq":12,"perfil_id":1,"ligada":false}"#)), vec![Efeito::Sair]);
        assert!(r.aplicar(evento(r#"{"tipo":"perfil.abelha_fixa","seq":13,"perfil_id":1,"ligada":true}"#)).is_empty());
        assert_eq!(
            r.aplicar(evento(r#"{"tipo":"perfil.mascote","seq":14,"perfil_id":1,"modelo":"favo","nome":"Zuzu"}"#)),
            vec![Efeito::Vestir { modelo: "favo".into(), nome: "Zuzu".into() }]
        );
        assert!(r.aplicar(evento(r#"{"tipo":"lousa.mudou","seq":15,"lousa_id":1}"#)).is_empty());
        assert_eq!(r.aplicar(evento(r#"{"tipo":"recarregar"}"#)), vec![Efeito::Recarregar]);
        let voz = r.aplicar(evento(r#"{"tipo":"perfil.voz","seq":13,"perfil_id":1,"voz":{"voz_comandos":true,"voz_abelha":false,"voz_efeito":false,"voz_nome":true,"voz_sensibilidade":7}}"#));
        assert_eq!(voz, vec![Efeito::Voz(OpcoesVoz { voz_comandos: true, voz_nome: true, voz_sensibilidade: 7, ..Default::default() })]);
        assert_eq!(r.aplicar(evento(r#"{"tipo":"ola","seq":3,"nucleo":"0.4.0"}"#)), vec![Efeito::Recarregar]);
    }

    #[test]
    fn cartao_aguardando_sem_agente_conta_e_remover_limpa() {
        let mut r = quadro();
        r.aplicar(evento(r#"{"tipo":"tarefa.atualizada","seq":6,"tarefa":{"id":11,"projeto_id":1,"titulo":"Tela de cupons","coluna":"aguardando"}}"#));
        assert_eq!(r.contagem().esperam, 1);
        assert_eq!(r.destaque().map(|a| a.tarefa), Some(11));
        r.aplicar(evento(r#"{"tipo":"projeto.removido","seq":7,"projeto_id":1}"#));
        assert_eq!(r.contagem(), Contagem::default());
        assert_eq!(r.estado(), Estado::Dormindo);
    }
}
