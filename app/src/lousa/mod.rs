//! Lousa (quadro livre) do workspace e da tarefa: um quadro infinito onde a
//! dona solta notas, textos, trechos de código, imagens, vídeos, cartões de
//! tarefa e as ligações entre eles. O mesmo componente serve aos dois donos
//! e, só leitura, ao slide e ao palco.
//!
//! Tudo por evento: arrastar redesenha por entrada; a gravação do texto
//! espera 800 ms depois da última tecla com um único `request_repaint_after`;
//! parada, a lousa não redesenha. Os itens mudam aqui na hora e vão ao núcleo
//! em lotes, em segundo plano (veja `modelo`).

pub mod barra;
pub mod camera;
pub mod desenho;
pub mod interacao;
pub mod markdown;
pub mod modelo;
pub mod palco;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Pos2, Rect, Vec2, pos2, vec2};

use crate::api::{self, ElementoLousa as Elemento, TipoElemento};
use crate::dados;
use crate::registro;
use crate::tema::{self, TipoAviso, cores};
use camera::{Camera, Pinca};
use desenho::{Desenho, InfoTarefa};
use modelo::{Comando, Modelo, Mudou};

/// Espera da gravação do texto depois da última tecla.
const ESPERA_TEXTO: f64 = 0.8;
/// "Abrindo lousa…" só aparece se demorar mais que isto.
const ESPERA_ABRINDO: f64 = 0.15;
/// Deslocamento da cascata (duplicar, colar, criar no mesmo ponto).
const CASCATA: f32 = 24.0;

/// Tamanhos iniciais (unidades do quadro).
pub fn tamanho_inicial(tipo: TipoElemento) -> Vec2 {
    match tipo {
        TipoElemento::Nota => vec2(240.0, 96.0),
        TipoElemento::Texto => vec2(200.0, 40.0),
        TipoElemento::Codigo => vec2(440.0, 120.0),
        TipoElemento::Imagem => vec2(480.0, 270.0),
        TipoElemento::Video => vec2(320.0, 180.0),
        TipoElemento::Tarefa => vec2(260.0, 96.0),
        TipoElemento::Ligacao => vec2(16.0, 16.0),
    }
}

/// O que a lousa pede à tela principal.
#[derive(Clone, Debug, PartialEq)]
pub enum Acao {
    AbrirTarefa(i64),
    Avisar(TipoAviso, String),
    /// "3 itens apagados · Desfazer": o texto e o número do comando da remoção.
    AvisarDesfazer(String, u64),
    /// Falha ao gravar: "… · Tentar de novo".
    AvisarTentar(String),
    /// Apresentar a lousa no palco, a partir do item (Shift+F5) ou do começo.
    Apresentar(Option<i64>),
    /// Pôr um texto na caixa de mensagem do agente (sem enviar).
    PedirAoAgente(String),
}

/// O que a tela principal dá à lousa a cada quadro.
pub struct Contexto<'a> {
    pub perfil: i64,
    pub pode_mudar: bool,
    /// A faixa do núcleo desconectado está no topo da janela: a da lousa não aparece.
    pub faixa_global: bool,
    pub modelo: &'a dados::Modelo,
    /// Projetos que aparecem primeiro na busca de tarefas.
    pub preferidos: Vec<i64>,
    /// A lousa da tarefa tem agente (o estado vazio oferece "Montar o fluxo").
    pub tem_agente: bool,
    pub agora: f64,
}

/// O cartão de tarefa ao vivo, a partir do quadro (kanban).
pub fn info_tarefa(modelo: &dados::Modelo, id: i64) -> Option<InfoTarefa> {
    let t = modelo.tarefas.iter().find(|t| t.id == id)?;
    let agente = t.agentes.iter().find(|a| a.visual().pede_voce()).or_else(|| t.agentes.iter().find(|a| a.ativo)).or(t.agentes.first());
    Some(InfoTarefa {
        titulo: t.titulo.clone(),
        projeto: t.projeto.clone(),
        coluna: t.coluna.chave().into(),
        erro: t.erro.is_some(),
        estado: agente.map(|a| a.visual()),
    })
}

enum Estado {
    Abrindo(f64),
    Erro(String),
    Pronta,
}

/// Respostas do núcleo, pedidas em segundo plano.
enum Resposta {
    Aberta(Result<api::LousaAberta, String>),
    Recarregada(Result<api::LousaAberta, String>),
    Lote(Result<api::LoteGravado, String>),
    Anexo { envio: u64, resultado: Result<api::AnexoLousa, String> },
    VideoFalhou(i64, String),
}

/// Uma imagem ou um vídeo indo para o núcleo: o cartão provisório.
pub struct Envio {
    pub numero: u64,
    pub posicao: Pos2,
    pub tamanho: Vec2,
    pub video: bool,
    pub erro: Option<String>,
    /// O que mandar de novo no "Tentar de novo".
    origem: OrigemEnvio,
}

#[derive(Clone)]
enum OrigemEnvio {
    Png(std::sync::Arc<Vec<u8>>),
    Arquivo(PathBuf),
}

/// O texto aberto no editor.
pub struct Edicao {
    pub id: i64,
    /// Número da sessão: o id do campo (o do item troca quando o núcleo responde).
    pub sessao: u64,
    pub antes: Elemento,
    pub texto: String,
    pub focar: bool,
    /// O núcleo recusou o texto (parece ter senha): o editor fica, e o segundo Esc descarta.
    pub segredo: bool,
    pub esc: bool,
    /// O campo é o rótulo de uma ligação (uma linha).
    pub rotulo: bool,
}

/// O que a lousa está fazendo com o mouse.
#[derive(Clone, Debug)]
pub enum Arrasto {
    Mover { ultimo: Pos2, antes: Vec<Elemento> },
    Redimensionar { id: i64, alca: interacao::Alca, antes: Box<Elemento> },
    Vista,
    Caixa { inicio: Pos2, atual: Pos2, somar: Vec<i64> },
    Ligar { de: i64, ponta: Pos2 },
}

pub struct Lousa {
    pub dono: api::DonoLousa,
    estado: Estado,
    pub modelo: Modelo,
    pub camera: Camera,
    enquadrada: bool,
    /// A seleção, na ordem dos cliques (a "Ligação" liga o primeiro ao segundo).
    pub selecao: Vec<i64>,
    pub arrasto: Option<Arrasto>,
    pub edicao: Option<Edicao>,
    sessoes: u64,
    pinca: Pinca,
    pub desenho: Desenho,
    canal: (Sender<Resposta>, Receiver<Resposta>),
    /// Quando mandar o próximo lote (o texto espera; o resto vai ao soltar).
    enviar_em: Option<f64>,
    pub envios: Vec<Envio>,
    envios_feitos: u64,
    /// A lousa tem o teclado (o último clique foi nela).
    pub ativa: bool,
    /// Espaço segurado (arrastar move a vista).
    espaco: bool,
    pub busca: Option<barra::Busca>,
    /// O item (ou o fundo, None) do menu de contexto, e onde ele abriu (no quadro).
    pub menu: Option<(Option<i64>, Pos2)>,
    /// Vídeo abrindo ou com erro: o item, o texto, se é erro e quando.
    pub video: Option<(i64, String, bool, f64)>,
    /// Desenhada neste quadro (senão a edição é encerrada e o pendente vai já).
    vista_no_quadro: bool,
    pub area: Rect,
    /// Ao ficar pronta, enquadrar o que o agente pôs (aberta pela linha do tempo).
    pub enquadrar_agente: bool,
    /// O título (ou a legenda) em edição na barra da seleção: o item, o
    /// texto do campo e o item antes (para o desfazer).
    pub titulo: Option<(i64, String, Elemento)>,
    /// Onde ficou a barra de formatação (um clique nela não fecha o editor).
    pub barra_formato: Option<Rect>,
    /// Envios que falharam e onde estão na tela (os botões ficam por cima).
    pub envios_com_erro: Vec<(u64, Rect)>,
    grade: tema::Grade,
    /// Avisos que nasceram fora de um quadro com ações (ao fechar o editor).
    avisos: Vec<Acao>,
    /// A maior largura que a barra da seleção já teve para esta seleção: a
    /// barra não anda quando o tipo troca e ela encolhe ou cresce.
    pub largura_barra: Option<(Vec<i64>, f32)>,
}

impl Lousa {
    pub fn nova(ctx: &egui::Context, dono: api::DonoLousa, agora: f64) -> Lousa {
        let mut l = Lousa {
            dono,
            estado: Estado::Abrindo(agora),
            modelo: Modelo::default(),
            camera: Camera::default(),
            enquadrada: false,
            selecao: Vec::new(),
            arrasto: None,
            edicao: None,
            sessoes: 0,
            pinca: Pinca::default(),
            desenho: Desenho::novo("lousa"),
            canal: mpsc::channel(),
            enviar_em: None,
            envios: Vec::new(),
            envios_feitos: 0,
            ativa: false,
            espaco: false,
            busca: None,
            menu: None,
            video: None,
            vista_no_quadro: false,
            area: Rect::NOTHING,
            enquadrar_agente: false,
            titulo: None,
            barra_formato: None,
            envios_com_erro: Vec::new(),
            grade: tema::Grade::default(),
            avisos: Vec::new(),
            largura_barra: None,
        };
        l.abrir(ctx);
        l
    }

    fn abrir(&mut self, ctx: &egui::Context) {
        let dono = self.dono;
        registro::em_segundo_plano(&self.canal.0, ctx, move || Resposta::Aberta(api::abrir_lousa(dono)));
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(ESPERA_ABRINDO));
    }

    pub fn pronta(&self) -> bool {
        matches!(self.estado, Estado::Pronta)
    }

    /// O retrato de novo (ao reconectar ou quando a tela ficou para trás).
    pub fn recarregar(&mut self, ctx: &egui::Context) {
        match self.estado {
            Estado::Pronta => {
                let id = self.modelo.id;
                registro::em_segundo_plano(&self.canal.0, ctx, move || Resposta::Recarregada(api::ler_lousa(id)));
            }
            Estado::Erro(_) => {
                self.estado = Estado::Abrindo(ctx.input(|i| i.time));
                self.abrir(ctx);
            }
            Estado::Abrindo(_) => {}
        }
    }

    /// Itens que podem ir ao palco (sem textos soltos nem ligações).
    pub fn cartoes(&self) -> usize {
        self.modelo.elementos.iter().filter(|e| palco::vai_ao_palco(e)).count()
    }

    /// Uma mensagem "lousa.mudou" do núcleo. Devolve quantos itens o agente
    /// acrescentou (para o aviso e o ponto no chip).
    pub fn aplicar_evento(&mut self, lousa: i64, elementos: Vec<Elemento>, removidos: &[i64], agente: i64) -> usize {
        if !self.pronta() || lousa != self.modelo.id {
            return 0;
        }
        let novos: Vec<i64> = if agente != 0 { elementos.iter().filter(|e| self.modelo.buscar(e.id).is_none()).map(|e| e.id).collect() } else { Vec::new() };
        self.modelo.aplicar_remoto(elementos, removidos);
        self.selecao.retain(|id| self.modelo.buscar(*id).is_some());
        self.modelo.novos.extend(novos.iter().copied());
        novos.len()
    }

    /// Enquadra os itens que o agente acabou de pôr (o "Ver" do aviso).
    pub fn ver_novos(&mut self) {
        let caixas = self.modelo.elementos.iter().filter(|e| self.modelo.novos.contains(&e.id) && e.tipo != TipoElemento::Ligacao).map(desenho::caixa_quadro);
        if let Some(caixa) = camera::envolver(caixas)
            && self.area.is_positive()
        {
            self.camera = Camera::enquadrar(self.area, caixa, 64.0, 1.0, true);
        }
    }

    /// Algum item novo do agente está fora da vista.
    pub fn novos_fora_da_vista(&self) -> bool {
        let vista = self.camera.visivel(self.area);
        self.modelo
            .elementos
            .iter()
            .any(|e| self.modelo.novos.contains(&e.id) && e.tipo != TipoElemento::Ligacao && !vista.intersects(desenho::caixa_quadro(e)))
    }

    /// Recebe as respostas e manda o pendente: chamado a cada quadro, mesmo
    /// com a lousa fora da tela (trocar de tela grava na hora).
    pub fn fundo(&mut self, ctx: &egui::Context, agora: f64, pode_mudar: bool) -> Vec<Acao> {
        let mut acoes = Vec::new();
        self.desenho.receber(ctx);
        while let Ok(r) = self.canal.1.try_recv() {
            match r {
                Resposta::Aberta(Ok(aberta)) => {
                    self.modelo = Modelo::novo(aberta);
                    self.estado = Estado::Pronta;
                }
                Resposta::Aberta(Err(e)) => self.estado = Estado::Erro(e),
                Resposta::Recarregada(Ok(aberta)) => {
                    self.modelo.sincronizar(aberta.elementos);
                    self.selecao.retain(|id| self.modelo.buscar(*id).is_some());
                }
                Resposta::Recarregada(Err(_)) => {}
                Resposta::Lote(resultado) => {
                    let editando = self.edicao.as_ref().map(|e| e.id);
                    let segredo = matches!(&resultado, Err(e) if e.contains("senha ou chave"));
                    let resposta = self.modelo.receber_lote(resultado, editando);
                    for (antigo, novo) in &resposta.trocas {
                        for s in &mut self.selecao {
                            if s == antigo {
                                *s = *novo;
                            }
                        }
                        if let Some(e) = &mut self.edicao
                            && e.id == *antigo
                        {
                            e.id = *novo;
                            e.antes.id = *novo;
                        }
                    }
                    self.selecao.retain(|id| self.modelo.buscar(*id).is_some());
                    if resposta.nao_desfez {
                        acoes.push(Acao::Avisar(TipoAviso::Alerta, "Mudou em outra tela; não desfiz".into()));
                    } else if resposta.mudou_fora && resposta.conflitos.is_empty() {
                        acoes.push(Acao::Avisar(TipoAviso::Alerta, "Mudou em outra tela; atualizei".into()));
                    }
                    if let Some(e) = resposta.erro {
                        if segredo && let Some(ed) = &mut self.edicao {
                            ed.segredo = true;
                            ed.focar = true;
                        } else if e.starts_with("A lousa chegou ao limite") {
                            acoes.push(Acao::Avisar(TipoAviso::Erro, e));
                        } else if !api::erro_de_conexao(&e) {
                            acoes.push(Acao::AvisarTentar(format!("Não consegui salvar a lousa: {e}")));
                        }
                    }
                    if self.modelo.na_fila() {
                        self.enviar_em = Some(agora);
                    }
                }
                Resposta::Anexo { envio, resultado } => self.anexo_chegou(envio, resultado, &mut acoes),
                Resposta::VideoFalhou(id, e) => self.video = Some((id, e, true, agora)),
            }
        }
        if !self.vista_no_quadro && self.edicao.is_some() {
            self.encerrar_edicao(ctx, agora);
        }
        acoes.append(&mut self.avisos);
        let parado = self.arrasto.is_none();
        let vencido = self.enviar_em.is_some_and(|t| agora >= t) || (!self.vista_no_quadro && self.modelo.na_fila());
        if pode_mudar && parado && vencido && self.modelo.em_voo.is_none() {
            self.enviar(ctx);
        } else if let Some(t) = self.enviar_em
            && t > agora
        {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(t - agora));
        }
        self.vista_no_quadro = false;
        acoes
    }

    /// Manda o próximo lote (se houver), em segundo plano.
    fn enviar(&mut self, ctx: &egui::Context) {
        let fora: HashSet<i64> = self.edicao.as_ref().filter(|e| self.modelo.remotos.contains_key(&e.id)).map(|e| e.id).into_iter().collect();
        let Some(lote) = self.modelo.montar_lote(&fora) else {
            self.enviar_em = None;
            return;
        };
        self.enviar_em = None;
        let id = self.modelo.id;
        registro::em_segundo_plano(&self.canal.0, ctx, move || Resposta::Lote(api::gravar_lousa(id, &lote.operacoes)));
    }

    /// Grava agora, esperando a resposta (ao fechar a janela).
    pub fn gravar_ja(&mut self) {
        if let Some(e) = self.edicao.take() {
            self.terminar_texto(e);
        }
        for _ in 0..4 {
            if self.modelo.em_voo.is_some() {
                // A resposta do lote no ar chega pelo canal.
                if let Ok(Resposta::Lote(r)) = self.canal.1.recv_timeout(std::time::Duration::from_secs(5)) {
                    self.modelo.receber_lote(r, None);
                }
                continue;
            }
            let Some(lote) = self.modelo.montar_lote(&HashSet::new()) else { return };
            let resultado = api::gravar_lousa(self.modelo.id, &lote.operacoes);
            self.modelo.receber_lote(resultado, None);
        }
    }

    /// Pede o lote: já (`agora`) ou depois da espera do texto.
    fn agendar(&mut self, ctx: &egui::Context, quando: f64) {
        let quando = self.enviar_em.map_or(quando, |t| t.min(quando));
        self.enviar_em = Some(quando);
        let falta = (quando - ctx.input(|i| i.time)).max(0.0);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(falta));
    }

    pub fn tentar_de_novo(&mut self, ctx: &egui::Context) {
        self.modelo.tentar_de_novo();
        let agora = ctx.input(|i| i.time);
        self.agendar(ctx, agora);
    }

    // Criar

    /// Um item novo do tipo, com o tamanho inicial, centrado em `centro` e
    /// inteiro dentro da vista (perto da borda, ele entra o necessário; a
    /// cascata, se o lugar já tem um item). Os de texto já entram em edição.
    pub fn criar(&mut self, ctx: &egui::Context, tipo: TipoElemento, centro: Pos2, preencher: impl FnOnce(&mut Elemento)) -> i64 {
        let tamanho = tamanho_inicial(tipo);
        let mut canto = centro - tamanho / 2.0;
        if self.area.is_positive() {
            // Em cima fica a barra de ferramentas.
            let z = self.camera.zoom;
            let v = self.camera.visivel(self.area);
            let vista = Rect::from_min_max(v.min + vec2(16.0, 64.0) / z, v.max - vec2(16.0, 16.0) / z);
            if vista.width() >= tamanho.x && vista.height() >= tamanho.y {
                canto.x = canto.x.clamp(vista.left(), vista.right() - tamanho.x);
                canto.y = canto.y.clamp(vista.top(), vista.bottom() - tamanho.y);
            }
        }
        while self.modelo.elementos.iter().any(|e| e.tipo != TipoElemento::Ligacao && (pos2(e.x, e.y) - canto).length() < 4.0) {
            canto += Vec2::splat(CASCATA);
        }
        let mut e =
            Elemento { tipo, x: canto.x.round(), y: canto.y.round(), largura: tamanho.x, altura: tamanho.y, cor: "amarelo".into(), ..Default::default() };
        preencher(&mut e);
        let id = self.modelo.criar_local(e);
        let criado = self.modelo.buscar(id).cloned();
        self.modelo.registrar(Comando { numero: 0, pares: vec![(None, criado)] });
        self.selecao = vec![id];
        if tipo.de_texto() {
            self.editar(id);
        } else {
            let agora = ctx.input(|i| i.time);
            self.agendar(ctx, agora);
        }
        id
    }

    /// Cria uma ligação entre dois itens.
    pub fn ligar(&mut self, ctx: &egui::Context, de: i64, para: i64) {
        if de == para || self.modelo.elementos.iter().any(|e| e.tipo == TipoElemento::Ligacao && e.de == de && e.para == para) {
            return;
        }
        let e = Elemento { tipo: TipoElemento::Ligacao, de, para, largura: 16.0, altura: 16.0, ..Default::default() };
        let id = self.modelo.criar_local(e);
        let criado = self.modelo.buscar(id).cloned();
        self.modelo.registrar(Comando { numero: 0, pares: vec![(None, criado)] });
        self.selecao = vec![id];
        let agora = ctx.input(|i| i.time);
        self.agendar(ctx, agora);
    }

    /// Cola itens (copiados ou duplicados) deslocados, com as ligações entre eles.
    pub fn colar_itens(&mut self, ctx: &egui::Context, itens: &[Elemento], deslocamento: Vec2) {
        let mut trocas: Vec<(i64, i64)> = Vec::new();
        let mut pares = Vec::new();
        let mut nova_selecao = Vec::new();
        let (normais, ligacoes): (Vec<&Elemento>, Vec<&Elemento>) = itens.iter().partition(|e| e.tipo != TipoElemento::Ligacao);
        for e in normais {
            let mut copia = e.clone();
            copia.x += deslocamento.x;
            copia.y += deslocamento.y;
            copia.autor = "voce".into();
            copia.agente_id = 0;
            let novo = self.modelo.criar_local(copia);
            trocas.push((e.id, novo));
            pares.push((None, self.modelo.buscar(novo).cloned()));
            nova_selecao.push(novo);
        }
        for l in ligacoes {
            let (Some(de), Some(para)) = (trocas.iter().find(|(a, _)| *a == l.de).map(|t| t.1), trocas.iter().find(|(a, _)| *a == l.para).map(|t| t.1)) else {
                continue;
            };
            let mut copia = l.clone();
            (copia.de, copia.para) = (de, para);
            let novo = self.modelo.criar_local(copia);
            pares.push((None, self.modelo.buscar(novo).cloned()));
        }
        self.modelo.registrar(Comando { numero: 0, pares });
        self.selecao = nova_selecao;
        let agora = ctx.input(|i| i.time);
        self.agendar(ctx, agora);
    }

    /// Os itens selecionados, com as ligações entre eles (copiar, duplicar).
    pub fn copiar_selecao(&self) -> Vec<Elemento> {
        let ids: HashSet<i64> = self.selecao.iter().copied().collect();
        self.modelo
            .elementos
            .iter()
            .filter(|e| ids.contains(&e.id) || (e.tipo == TipoElemento::Ligacao && ids.contains(&e.de) && ids.contains(&e.para)))
            .cloned()
            .collect()
    }

    /// Apaga a seleção (e as ligações dela); devolve o aviso com "Desfazer".
    pub fn apagar_selecao(&mut self, ctx: &egui::Context) -> Option<Acao> {
        if self.selecao.is_empty() {
            return None;
        }
        let ids: HashSet<i64> = self.selecao.iter().copied().collect();
        let saiu = self.modelo.remover_local(&ids);
        let n = saiu.iter().filter(|e| ids.contains(&e.id)).count();
        let numero = self.modelo.registrar(Comando { numero: 0, pares: saiu.into_iter().map(|e| (Some(e), None)).collect() });
        self.selecao.clear();
        let agora = ctx.input(|i| i.time);
        self.agendar(ctx, agora);
        Some(Acao::AvisarDesfazer(if n == 1 { "1 item apagado".into() } else { format!("{n} itens apagados") }, numero))
    }

    pub fn desfazer(&mut self, ctx: &egui::Context) {
        if let Some(e) = self.edicao.take() {
            self.terminar_texto(e);
        }
        if let Some(tocados) = self.modelo.desfazer() {
            self.selecao = tocados.into_iter().filter(|id| self.modelo.buscar(*id).is_some()).collect();
            let agora = ctx.input(|i| i.time);
            self.agendar(ctx, agora);
        }
    }

    /// O "Desfazer" do aviso: desfaz a remoção que ele anunciou, mesmo que
    /// você tenha feito outra coisa depois. false: ela já não está na pilha.
    pub fn desfazer_comando(&mut self, ctx: &egui::Context, numero: u64) -> bool {
        if let Some(e) = self.edicao.take() {
            self.terminar_texto(e);
        }
        let Some(tocados) = self.modelo.desfazer_comando(numero) else { return false };
        self.selecao = tocados.into_iter().filter(|id| self.modelo.buscar(*id).is_some()).collect();
        let agora = ctx.input(|i| i.time);
        self.agendar(ctx, agora);
        true
    }

    pub fn refazer(&mut self, ctx: &egui::Context) {
        if let Some(tocados) = self.modelo.refazer() {
            self.selecao = tocados.into_iter().filter(|id| self.modelo.buscar(*id).is_some()).collect();
            let agora = ctx.input(|i| i.time);
            self.agendar(ctx, agora);
        }
    }

    /// Muda os itens dados de uma vez, num comando só de desfazer.
    pub fn mudar_varios(&mut self, ctx: &egui::Context, ids: &[i64], campos: Mudou, f: impl Fn(&mut Elemento)) {
        let mut pares = Vec::new();
        for id in ids {
            let antes = self.modelo.buscar(*id).cloned();
            self.modelo.mudar(*id, campos, &f);
            pares.push((antes, self.modelo.buscar(*id).cloned()));
        }
        self.modelo.registrar(Comando { numero: 0, pares });
        let agora = ctx.input(|i| i.time);
        self.agendar(ctx, agora);
    }

    // Edição de texto

    pub fn editar(&mut self, id: i64) {
        let Some(e) = self.modelo.buscar(id).cloned() else { return };
        if !(e.tipo.de_texto() || e.tipo == TipoElemento::Ligacao) {
            return;
        }
        self.sessoes += 1;
        self.edicao = Some(Edicao {
            id,
            sessao: self.sessoes,
            texto: e.texto.clone(),
            antes: e.clone(),
            focar: true,
            segredo: false,
            esc: false,
            rotulo: e.tipo == TipoElemento::Ligacao,
        });
        self.selecao = vec![id];
    }

    /// O texto do editor mudou: o item muda na hora e a gravação espera 800 ms.
    pub fn texto_mudou(&mut self, ctx: &egui::Context, pintor: &egui::Painter) {
        let Some(ed) = &mut self.edicao else { return };
        ed.segredo = false;
        ed.esc = false;
        let (id, texto) = (ed.id, ed.texto.clone());
        self.modelo.mudar(id, Mudou::TEXTO, |e| e.texto = texto);
        self.crescer(pintor, id);
        let agora = ctx.input(|i| i.time);
        self.agendar(ctx, agora + ESPERA_TEXTO);
    }

    /// A altura cresce sozinha quando o texto não cabe (nunca diminui).
    fn crescer(&mut self, pintor: &egui::Painter, id: i64) {
        let Some(e) = self.modelo.buscar(id).cloned() else { return };
        if !e.tipo.de_texto() {
            return;
        }
        let precisa = self.desenho.altura_do_conteudo(pintor, &e).ceil().min(modelo_max_lado());
        if precisa > e.altura + 0.5 {
            self.modelo.mudar(id, Mudou::TAMANHO, |e| e.altura = precisa);
        }
    }

    /// Fecha o editor: um comando de desfazer pela sessão inteira, e grava já.
    pub fn encerrar_edicao(&mut self, ctx: &egui::Context, agora: f64) {
        if let Some(e) = self.edicao.take() {
            self.terminar_texto(e);
            self.agendar(ctx, agora);
        }
    }

    fn terminar_texto(&mut self, ed: Edicao) {
        // Conflito (o texto mudou fora enquanto você editava): junta como na nota do slide.
        if let Some(remoto) = self.modelo.remotos.get(&ed.id).cloned() {
            let juntado = crate::apresentacao::juntar_nota(&ed.antes.texto, &ed.texto, &remoto.texto);
            // O que mudou fora não some calado: diga o que foi feito.
            let quem = if remoto.do_agente() { "O agente" } else { "Outra tela" };
            let aviso = match &juntado {
                Some(_) => format!("{quem} acrescentou texto aqui; juntei ao seu"),
                None if remoto.do_agente() => "O agente mudou isto; mantive o seu".to_string(),
                None => "Mudou em outra tela; mantive o seu".to_string(),
            };
            self.avisos.push(Acao::Avisar(TipoAviso::Alerta, aviso));
            self.modelo.resolver_conflito(ed.id, juntado.unwrap_or(ed.texto.clone()));
        }
        let depois = self.modelo.buscar(ed.id).cloned();
        if let Some(d) = &depois
            && d.texto.trim().is_empty()
            && ed.antes.texto.is_empty()
            && d.tipo != TipoElemento::Ligacao
            && d.titulo.is_empty()
        {
            // Nota criada e deixada vazia: some sem deixar rastro no desfazer.
            self.modelo.remover_local(&[ed.id].into());
            self.modelo.descartar_ultimo_comando_de(ed.id);
            self.selecao.retain(|s| *s != ed.id);
            return;
        }
        self.modelo.registrar(Comando { numero: 0, pares: vec![(Some(ed.antes), depois)] });
    }

    // Imagens e vídeos

    /// Cola a imagem da área de transferência (Ctrl+V sem texto).
    pub fn colar_imagem(&mut self, ctx: &egui::Context, perfil: i64, posicao: Pos2) -> Option<Acao> {
        let Ok(mut area) = arboard::Clipboard::new() else { return None };
        let imagem = area.get_image().ok()?;
        if imagem.width * imagem.height > 40_000_000 {
            return Some(Acao::Avisar(TipoAviso::Alerta, "Imagem grande demais para anexar.".into()));
        }
        let mut png = Vec::new();
        let mut codificador = png::Encoder::new(&mut png, imagem.width as u32, imagem.height as u32);
        codificador.set_color(png::ColorType::Rgba);
        codificador.set_depth(png::BitDepth::Eight);
        if codificador.write_header().and_then(|mut e| e.write_image_data(&imagem.bytes).and_then(|_| e.finish())).is_err() {
            return None;
        }
        let largura = (imagem.width as f32).min(480.0);
        let tamanho = vec2(largura, (largura * imagem.height as f32 / imagem.width.max(1) as f32).max(16.0));
        self.enviar_anexo(ctx, perfil, posicao, tamanho, OrigemEnvio::Png(std::sync::Arc::new(png)), false);
        None
    }

    /// Arquivos soltos na lousa ou escolhidos: imagens e vídeos, em cascata.
    pub fn anexar_arquivos(&mut self, ctx: &egui::Context, perfil: i64, arquivos: Vec<PathBuf>, posicao: Pos2) -> Option<Acao> {
        let mut aviso = None;
        let mut pos = posicao;
        for arquivo in arquivos {
            match api::tipo_do_arquivo(&arquivo) {
                Some((_, video)) => {
                    let bytes = std::fs::metadata(&arquivo).map(|m| m.len()).unwrap_or(0);
                    if video && bytes > api::MAIOR_VIDEO {
                        aviso = Some(Acao::Avisar(TipoAviso::Alerta, format!("O vídeo tem {}; o limite é 512 MB.", registro::tamanho(bytes))));
                        continue;
                    }
                    if !video && bytes > 8 << 20 {
                        aviso = Some(Acao::Avisar(TipoAviso::Alerta, format!("A foto tem {}; o limite é 8 MB.", registro::tamanho(bytes))));
                        continue;
                    }
                    let tamanho = tamanho_inicial(if video { TipoElemento::Video } else { TipoElemento::Imagem });
                    self.enviar_anexo(ctx, perfil, pos, tamanho, OrigemEnvio::Arquivo(arquivo), video);
                    pos += Vec2::splat(CASCATA);
                }
                None => aviso = Some(Acao::Avisar(TipoAviso::Alerta, "Só imagens e vídeos entram na lousa".into())),
            }
        }
        aviso
    }

    fn enviar_anexo(&mut self, ctx: &egui::Context, perfil: i64, centro: Pos2, tamanho: Vec2, origem: OrigemEnvio, video: bool) {
        self.envios_feitos += 1;
        let numero = self.envios_feitos;
        self.envios.push(Envio { numero, posicao: centro - tamanho / 2.0, tamanho, video, erro: None, origem: origem.clone() });
        self.mandar_envio(ctx, perfil, numero, origem);
    }

    fn mandar_envio(&self, ctx: &egui::Context, perfil: i64, numero: u64, origem: OrigemEnvio) {
        registro::em_segundo_plano(&self.canal.0, ctx, move || Resposta::Anexo {
            envio: numero,
            resultado: match origem {
                OrigemEnvio::Png(png) => api::anexar_png_na_lousa(perfil, &png),
                OrigemEnvio::Arquivo(caminho) => api::anexar_arquivo_na_lousa(perfil, &caminho),
            },
        });
    }

    /// "Tentar de novo" no cartão de um envio que falhou.
    pub fn reenviar(&mut self, ctx: &egui::Context, perfil: i64, numero: u64) {
        if let Some(e) = self.envios.iter_mut().find(|e| e.numero == numero) {
            e.erro = None;
            let origem = e.origem.clone();
            self.mandar_envio(ctx, perfil, numero, origem);
        }
    }

    fn anexo_chegou(&mut self, numero: u64, resultado: Result<api::AnexoLousa, String>, acoes: &mut Vec<Acao>) {
        let Some(i) = self.envios.iter().position(|e| e.numero == numero) else { return };
        match resultado {
            Ok(anexo) => {
                let envio = self.envios.remove(i);
                let tipo = if envio.video { TipoElemento::Video } else { TipoElemento::Imagem };
                let mut tamanho = envio.tamanho;
                if !envio.video && anexo.largura > 0 && anexo.altura > 0 {
                    let largura = (anexo.largura as f32).min(480.0);
                    tamanho = vec2(largura, (largura * anexo.altura as f32 / anexo.largura as f32).max(16.0));
                }
                let e = Elemento {
                    tipo,
                    x: envio.posicao.x.round(),
                    y: envio.posicao.y.round(),
                    largura: tamanho.x.max(16.0),
                    altura: tamanho.y.clamp(16.0, 6000.0),
                    anexo_id: anexo.id,
                    anexo: Some(api::AnexoDoElemento { largura: anexo.largura, altura: anexo.altura, bytes: anexo.bytes, nome: String::new() }),
                    cor: "amarelo".into(),
                    ..Default::default()
                };
                let id = self.modelo.criar_local(e);
                let criado = self.modelo.buscar(id).cloned();
                self.modelo.registrar(Comando { numero: 0, pares: vec![(None, criado)] });
                self.selecao = vec![id];
                self.enviar_em = Some(0.0);
            }
            Err(e) => {
                if self.envios[i].video {
                    acoes.push(Acao::Avisar(TipoAviso::Erro, format!("Não consegui enviar o vídeo: {e}")));
                }
                self.envios[i].erro = Some(e);
            }
        }
    }

    /// Abre o vídeo no reprodutor do sistema.
    pub fn abrir_video(&mut self, ctx: &egui::Context, id: i64, anexo: i64, agora: f64) {
        self.video = Some((id, "Abrindo no reprodutor…".into(), false, agora));
        ctx.request_repaint_after(std::time::Duration::from_secs(3));
        let envio = self.canal.0.clone();
        abrir_no_reprodutor(ctx, anexo, move |e| {
            let _ = envio.send(Resposta::VideoFalhou(id, e));
        });
    }

    // Câmera

    /// Primeira abertura: enquadra o conteúdo, com no máximo 100%.
    fn enquadrar_se_preciso(&mut self, area: Rect) {
        if self.enquadrada || !self.pronta() || !area.is_positive() {
            return;
        }
        self.enquadrada = true;
        let caixas = self.modelo.elementos.iter().filter(|e| e.tipo != TipoElemento::Ligacao).map(desenho::caixa_quadro);
        self.camera = match camera::envolver(caixas) {
            Some(caixa) => Camera::enquadrar(area, caixa, 48.0, 1.0, true),
            None => Camera { origem: pos2(-area.width() / 2.0, -area.height() / 2.0), zoom: 1.0 },
        };
    }

    pub fn ajustar_tudo(&mut self) {
        let caixas = self.modelo.elementos.iter().filter(|e| e.tipo != TipoElemento::Ligacao).map(desenho::caixa_quadro);
        if let Some(caixa) = camera::envolver(caixas) {
            self.camera = Camera::enquadrar(self.area, caixa, 48.0, 1.0, true);
        }
    }

    pub fn ajustar_selecao(&mut self) {
        let caixas = self.modelo.elementos.iter().filter(|e| self.selecao.contains(&e.id) && e.tipo != TipoElemento::Ligacao).map(desenho::caixa_quadro);
        if let Some(caixa) = camera::envolver(caixas) {
            self.camera = Camera::enquadrar(self.area, caixa, 64.0, 2.0, true);
        }
    }

    pub fn zoom_100(&mut self) {
        let centro = self.area.center();
        self.camera.zoom_em(self.area, centro, 1.0);
    }

    /// O centro da vista, no quadro (onde nasce o item da barra).
    pub fn centro_da_vista(&self) -> Pos2 {
        self.camera.para_quadro(self.area, self.area.center())
    }

    // Desenho

    /// Desenha a lousa editável na área e trata a interação. `area` fica
    /// inteira para a lousa (a da tarefa cobre o corpo do terminal).
    pub fn mostrar(&mut self, ui: &mut egui::Ui, area: Rect, c: &Contexto) -> Vec<Acao> {
        self.vista_no_quadro = true;
        self.area = area;
        let p = cores();
        let mut acoes = Vec::new();
        ui.painter().rect_filled(area, 0, p.fundo);
        match &self.estado {
            Estado::Abrindo(desde) => {
                if c.agora - desde >= ESPERA_ABRINDO {
                    ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, "Abrindo lousa…", egui::FontId::proportional(13.5), p.suave);
                }
                return acoes;
            }
            Estado::Erro(e) => {
                let texto = format!("Não consegui abrir a lousa: {e}");
                ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, texto, egui::FontId::proportional(13.5), p.erro);
                return acoes;
            }
            Estado::Pronta => {}
        }
        self.enquadrar_se_preciso(area);
        if std::mem::take(&mut self.enquadrar_agente) {
            let caixas = self.modelo.elementos.iter().filter(|e| e.do_agente() && e.tipo != TipoElemento::Ligacao).map(desenho::caixa_quadro);
            if let Some(caixa) = camera::envolver(caixas) {
                self.camera = Camera::enquadrar(area, caixa, 64.0, 1.0, true);
            }
        }
        acoes.extend(interacao::tratar(self, ui, area, c));
        self.pintar(ui, area, c);
        acoes.extend(barra::mostrar(self, ui, area, c));
        if barra::com_faixa(c) {
            barra::faixa_sem_nucleo(ui, area);
        }
        acoes
    }

    fn pintar(&mut self, ui: &mut egui::Ui, area: Rect, c: &Contexto) {
        let pintor = ui.painter_at(area);
        self.grade.desenhar(&pintor, area, self.camera.origem, self.camera.zoom);
        let destacadas: Vec<i64> = self.selecao.iter().copied().chain(interacao::ligacao_em_cima(self, ui)).collect();
        let video = self.video.as_ref().filter(|(_, _, erro, desde)| *erro || c.agora - desde < 3.0).map(|(id, t, erro, _)| (*id, t.clone(), *erro));
        let marcas = desenho::Marcas {
            editando: self.edicao.as_ref().filter(|e| !e.rotulo).map(|e| e.id),
            novos: Some(&self.modelo.novos),
            nao_salvos: Some(&self.modelo.nao_salvos),
            video,
            ligacoes_destacadas: destacadas,
            em_cima: interacao::item_em_cima(self, ui),
            limpo: false,
            foco: None,
        };
        let modelo = c.modelo;
        let tarefas = |id: i64| info_tarefa(modelo, id);
        self.desenho.desenhar(ui, area, &self.camera, &self.modelo.elementos, &tarefas, &marcas);
        interacao::pintar_moldura(self, ui, area, c);
    }
}

/// Abre o vídeo do anexo no reprodutor do sistema, em segundo plano (a
/// lousa e o palco). `falhou` recebe o erro, de onde quer que ele venha.
pub fn abrir_no_reprodutor(ctx: &egui::Context, anexo: i64, falhou: impl Fn(String) + Clone + Send + 'static) {
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let depois = falhou.clone();
        let acordar = ctx.clone();
        let resultado = api::info_anexo(anexo).and_then(|info| {
            crate::sistema::abrir_arquivo(&info.caminho, move |e| {
                depois(e);
                acordar.request_repaint();
            })
        });
        if let Err(e) = resultado {
            falhou(e);
            ctx.request_repaint();
        }
    });
}

fn modelo_max_lado() -> f32 {
    6000.0
}

fn modelo_min_lado() -> f32 {
    16.0
}
