//! Registro do trabalho: a linha do tempo, a página da daily e a da sprint,
//! do escopo da barra lateral (o perfil inteiro, um workspace ou um projeto;
//! com mais de um projeto, a daily e a sprint separam por projeto). Os textos vêm prontos do núcleo; aqui só
//! se mostra, copia e salva. A daily e a sprint são uma prévia do deck da
//! apresentação: os mesmos cartões, na mesma ordem dos slides.
//!
//! Desempenho: a linha do tempo é virtualizada (só o que aparece é
//! desenhado), os pedidos e as imagens são buscados fora da thread da tela, e
//! as miniaturas ficam num cache limitado. Nada se atualiza por tempo: um
//! evento novo traz a página visível de novo e só marca as outras.
//!
//! O tempo dos agentes só aparece com a opção do perfil ligada; o núcleo nem
//! manda os tempos sem ela. Ao desligar, o que estava na tela (com tempo)
//! some na hora e a página busca de novo; copiar fica para depois da resposta.

mod comum;
mod daily;
mod linha;
mod sprint;

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, CornerRadius, FontId, Id, RichText, Sense, TextureHandle, TextureOptions, vec2};

pub use comum::{
    CacheImagens, MARGEM_ROLAGEM, Miniatura, data_da_tela, decodificar, em_segundo_plano, estado_do_slide, largura_numeros, numeros, pilula_do_slide,
    sombra_rolagem, tamanho, texto_cortado, texto_em_linhas, texto_tempo,
};

use crate::api;
use crate::dados::Projeto;
use crate::tema::{self, TipoAviso, cores};

/// O que a página sabe do escopo, tirado dos projetos da barra lateral (sem
/// consulta): o nome para os títulos e quais projetos ele tem.
pub(super) struct Escopo<'a> {
    pub recorte: api::Recorte,
    /// "loja-web", "estudos" ou "todos os projetos".
    pub nome: String,
    /// Os projetos do recorte, na ordem da lateral.
    pub projetos: Vec<&'a Projeto>,
    /// O workspace (id e nome) quando o recorte é um projeto de um workspace com
    /// mais projetos: o cabeçalho oferece "Ver o workspace inteiro".
    pub workspace_maior: Option<(i64, String)>,
}

impl<'a> Escopo<'a> {
    fn novo(recorte: api::Recorte, todos: &'a [Projeto]) -> Self {
        let projetos: Vec<&Projeto> = todos
            .iter()
            .filter(|p| match recorte {
                api::Recorte::Perfil => true,
                api::Recorte::Workspace(w) => p.workspace_id == w,
                api::Recorte::Projeto(id) => p.id == id,
            })
            .collect();
        let nome = match (recorte, projetos.first()) {
            (api::Recorte::Projeto(_), Some(p)) => p.nome.clone(),
            (api::Recorte::Workspace(_), Some(p)) => p.workspace.clone(),
            _ => "todos os projetos".into(),
        };
        let workspace_maior = match (recorte, projetos.first()) {
            (api::Recorte::Projeto(_), Some(p)) if p.workspace_id != 0 && todos.iter().filter(|x| x.workspace_id == p.workspace_id).count() > 1 => {
                Some((p.workspace_id, p.workspace.clone()))
            }
            _ => None,
        };
        Escopo { recorte, nome, projetos, workspace_maior }
    }

    /// O recorte tem mais de um projeto (a página separa por projeto).
    pub fn varios(&self) -> bool {
        self.projetos.len() > 1
    }

    /// Os projetos do recorte sem nenhuma seção (sem atividade no período).
    pub fn sem_atividade(&self, secoes: &[i64]) -> Vec<&'a Projeto> {
        self.projetos.iter().filter(|p| !secoes.contains(&p.id)).copied().collect()
    }
}

/// As páginas do registro.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Aba {
    Linha,
    Daily,
    Sprint,
}

impl Aba {
    fn indice(self) -> usize {
        match self {
            Aba::Linha => 0,
            Aba::Daily => 1,
            Aba::Sprint => 2,
        }
    }
}

/// O que o registro pede à tela principal.
pub enum Acao {
    AbrirTarefa {
        tarefa: i64,
        agente: Option<i64>,
        /// O agente acabou de mexer na lousa da tarefa: ela abre junto, enquadrada.
        lousa: bool,
    },
    IrParaQuadro,
    VerTodos,
    /// Mudar o escopo (o perfil, um workspace ou um projeto): "Só este
    /// projeto", "Ver o workspace inteiro", o chip "Ver".
    Recorte(api::Recorte),
    /// Abrir o quadro deste projeto.
    AbrirQuadro(i64),
    /// Abrir a tela de bancos nesta conexão (um item da linha do tempo).
    AbrirBanco(i64),
    Avisar(TipoAviso, String),
    /// Abrir a apresentação no slide da tarefa (o clique num cartão abre em janela).
    Apresentar {
        deck: Box<api::Deck>,
        periodo: Option<api::PeriodoSprint>,
        tarefa: Option<i64>,
    },
    /// Abrir (ou fechar) a caixa "Pedir ao agente" da tarefa, presa ao botão
    /// do cartão. A resposta vai para a nota do deck (tipo e período).
    PedirAoAgente {
        tarefa: i64,
        tipo: String,
        periodo: String,
        botao: egui::Rect,
    },
}

/// O clique num cartão vira o pedido à tela principal; tirar da daily o
/// registro resolve sozinho (veja `tirar_da_daily`).
fn acao_do_clique(clique: comum::CliqueCartao, deck: &api::Deck, periodo: Option<api::PeriodoSprint>) -> Option<Acao> {
    Some(match clique {
        comum::CliqueCartao::Abrir(tarefa) => Acao::Apresentar { deck: Box::new(deck.clone()), periodo, tarefa: Some(tarefa) },
        comum::CliqueCartao::Pedir(tarefa, botao) => Acao::PedirAoAgente { tarefa, tipo: deck.tipo.clone(), periodo: deck.chave_nota.clone(), botao },
        comum::CliqueCartao::AbrirTarefa(tarefa) => Acao::AbrirTarefa { tarefa, agente: None, lousa: false },
        comum::CliqueCartao::TirarDaDaily(_) => return None,
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Periodo {
    Dias7,
    Dias14,
    Mes,
    Escolher,
}

/// Filtros da linha do tempo (um por vez).
#[derive(Clone, Copy, PartialEq)]
enum Filtro {
    Conclusoes,
    Erros,
    Capturas,
}

/// Perfil e recorte da tela: respostas de outro escopo são descartadas.
type Chave = (i64, api::Recorte);

enum Resposta {
    Pagina { chave: Chave, primeira: bool, resultado: Result<api::PaginaLinha, String> },
    Daily { chave: Chave, resultado: Result<api::Daily, String> },
    DeckDaily { chave: Chave, resultado: Result<api::Deck, String> },
    Sprint { chave: Chave, periodo: api::PeriodoSprint, resultado: Result<api::Sprint, String> },
    DeckSprint { chave: Chave, periodo: api::PeriodoSprint, resultado: Result<api::Deck, String> },
    Grande { id: i64, resultado: Result<egui::ColorImage, String> },
    Salvo(Result<String, String>),
    Removido(Result<(), String>),
    ForaDaDaily { fora: bool, resultado: Result<(), String> },
}

/// Imagem aberta em tamanho grande.
struct Visor {
    anexo: i64,
    texto: String,
    quando: String,
    tarefa: i64,
    imagem: Option<Result<TextureHandle, String>>,
    confirmar: bool,
}

const MAX_MINIATURAS: usize = 64;
/// Altura das miniaturas guardadas (o dobro do que aparece, para telas de alta densidade).
const ALTURA_MINIATURA: u32 = 128;

pub struct Registro {
    chave: Option<Chave>,
    canal: (Sender<Resposta>, Receiver<Resposta>),
    miniaturas: CacheImagens,
    visor: Option<Visor>,
    /// Chegou evento com a página fora da tela: ao voltar, ela busca de novo
    /// (sem consulta periódica). Uma marca por página.
    sujas: [bool; 3],
    /// O núcleo está ligado; sem ele, nada é pedido nem copiado.
    pub conectado: bool,

    // Linha do tempo
    dias: Vec<api::Dia>,
    proximo: i64,
    criado_em: String,
    carregando: bool,
    carregou: bool,
    erro: Option<String>,
    novidades: usize,
    no_topo: bool,
    ir_ao_topo: bool,
    filtro: Option<Filtro>,
    /// Cartões abertos com "Ver mais" (dia e tarefa).
    expandidos: HashSet<(String, i64)>,

    // Daily
    daily: Option<api::Daily>,
    deck_daily: Option<api::Deck>,
    erro_daily: Option<String>,
    texto_daily: String,
    /// O texto da daily (ou da sprint) veio de uma resposta com o tempo dos
    /// agentes: ao desligar a opção, ele é refeito mesmo editado.
    texto_daily_com_tempo: bool,
    texto_daily_aberto: bool,
    focar_daily: bool,

    // Sprint
    sprint: Option<api::Sprint>,
    deck_sprint: Option<api::Deck>,
    erro_sprint: Option<String>,
    texto_sprint: String,
    texto_sprint_com_tempo: bool,
    periodo: Periodo,
    de: String,
    ate: String,
    galeria_aberta: bool,
    salvando: bool,

    /// Ao voltar da apresentação, a página rola até o cartão do último slide visto.
    pub rolar_ate: Option<i64>,
    /// O pedido ao agente de cada tarefa (a tela principal põe a cada quadro)
    /// e a tarefa com a caixa "Pedir ao agente" aberta.
    pub pedidos: crate::pedido::Pedidos,
    pub caixa_aberta: Option<i64>,

    /// A opção "Mostrar tempo dos agentes" do perfil (a tela principal põe a
    /// cada quadro) e a última vista aqui, para notar a troca.
    pub mostrar_tempo: bool,
    tempo_visto: Option<bool>,
    /// Desligou o tempo e a resposta sem ele ainda não chegou: nada de copiar
    /// nem apresentar (o que havia na memória tinha os tempos).
    atualizando: bool,
    /// O texto editado da daily ou da sprint com tempo é trocado pelo novo.
    refazer_sem_tempo: bool,
    /// Ir para a seção deste projeto (a fileira de saltos).
    pub(super) rolar_secao: Option<i64>,
    /// O escopo mudou: a daily e a sprint abrem no topo (com os números e os
    /// saltos), não no deslocamento da visão anterior. A mesma visão
    /// atualizada por evento mantém a rolagem.
    pub(super) ao_topo: [bool; 2],
}

impl Default for Registro {
    fn default() -> Self {
        Registro {
            chave: None,
            canal: mpsc::channel(),
            miniaturas: CacheImagens::new(Some(ALTURA_MINIATURA), MAX_MINIATURAS, "miniatura"),
            visor: None,
            sujas: [false; 3],
            conectado: true,
            dias: Vec::new(),
            proximo: 0,
            criado_em: String::new(),
            carregando: false,
            carregou: false,
            erro: None,
            novidades: 0,
            no_topo: true,
            ir_ao_topo: false,
            filtro: None,
            expandidos: HashSet::new(),
            daily: None,
            deck_daily: None,
            erro_daily: None,
            texto_daily: String::new(),
            texto_daily_com_tempo: false,
            texto_daily_aberto: false,
            focar_daily: false,
            sprint: None,
            deck_sprint: None,
            erro_sprint: None,
            texto_sprint: String::new(),
            texto_sprint_com_tempo: false,
            periodo: Periodo::Dias14,
            de: String::new(),
            ate: String::new(),
            galeria_aberta: false,
            salvando: false,
            rolar_ate: None,
            pedidos: Default::default(),
            caixa_aberta: None,
            mostrar_tempo: false,
            tempo_visto: None,
            atualizando: false,
            refazer_sem_tempo: false,
            rolar_secao: None,
            ao_topo: [false; 2],
        }
    }
}

/// O texto tem o tempo dos agentes ("trabalhou 12 min", "esperou você",
/// "agentes trabalharam", "desde 14:26"; na sprint "• Agentes: 1 min",
/// "Agentes no total: …" e "Tempo de agentes" no Markdown)? A marca de
/// origem (o texto veio de uma resposta com o tempo) vale mais que isto:
/// aqui é a rede de segurança para o texto que você editou.
pub fn tem_tempo(texto: &str) -> bool {
    let desde_hora = texto.match_indices("desde ").any(|(i, _)| texto[i + 6..].chars().next().is_some_and(|c| c.is_ascii_digit()));
    ["trabalhou", "esperou", "trabalharam", "Tempo de agentes", "Agentes: ", "Agentes no total"].iter().any(|t| texto.contains(t)) || desde_hora
}

impl Registro {
    /// Ctrl+Shift+D: a página da daily com o texto aberto e em foco.
    pub fn abrir_daily(&mut self) {
        self.texto_daily_aberto = true;
        self.focar_daily = true;
    }

    /// Algo mudou no perfil. A página visível busca de novo (a linha do tempo,
    /// rolada para baixo, só conta a novidade); as outras ficam marcadas.
    pub fn novidade(&mut self, aba: Aba, perfil: i64, ctx: &egui::Context) {
        if self.chave.is_none_or(|c| c.0 != perfil) {
            return;
        }
        self.sujas = [true; 3];
        self.sujas[aba.indice()] = false;
        match aba {
            Aba::Linha if self.no_topo => self.pedir_pagina(ctx, true),
            Aba::Linha => self.novidades += 1,
            Aba::Daily => self.pedir_daily(ctx),
            Aba::Sprint => self.pedir_sprint(ctx),
        }
    }

    /// Algo mudou com o registro fora da tela (no quadro, numa tarefa ou na
    /// apresentação). Nada é pedido agora; ao voltar, a página busca de novo.
    pub fn marcar_suja(&mut self) {
        self.sujas = [true; 3];
    }

    /// Um agente mudou de estado: a sessão aberta no topo diz o estado de agora.
    pub fn estado_mudou(&mut self, aba: Aba, perfil: i64, ctx: &egui::Context) {
        if self.chave.is_none_or(|c| c.0 != perfil) {
            return;
        }
        match aba {
            Aba::Linha if self.no_topo => self.pedir_pagina(ctx, true),
            Aba::Daily => self.pedir_daily(ctx),
            _ => self.sujas[aba.indice()] = true,
        }
    }

    fn pedir_pagina(&mut self, ctx: &egui::Context, primeira: bool) {
        let Some(chave) = self.chave else { return };
        if self.carregando && !primeira {
            return;
        }
        self.carregando = true;
        let antes = if primeira { 0 } else { self.proximo };
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Pagina { chave, primeira, resultado: api::linha_do_tempo(chave.0, chave.1, antes) });
    }

    /// Tira a tarefa da daily de hoje (`fora`) ou traz de volta; a daily
    /// busca de novo quando o núcleo confirma.
    pub(super) fn tirar_da_daily(&mut self, ctx: &egui::Context, tarefa: i64, dia: String, fora: bool) {
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::ForaDaDaily { fora, resultado: api::tirar_da_daily(tarefa, &dia, fora) });
    }

    fn pedir_daily(&mut self, ctx: &egui::Context) {
        let Some(chave) = self.chave else { return };
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Daily { chave, resultado: api::daily(chave.0, chave.1) });
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::DeckDaily { chave, resultado: api::apresentacao(chave.0, chave.1, None) });
    }

    /// O período escolhido na sprint, já no formato do núcleo.
    pub fn periodo_sprint(&mut self) -> Option<api::PeriodoSprint> {
        match self.periodo {
            Periodo::Dias7 => Some(api::PeriodoSprint::Ultimos(7)),
            Periodo::Dias14 => Some(api::PeriodoSprint::Ultimos(14)),
            Periodo::Mes => Some(api::PeriodoSprint::MesAtual),
            Periodo::Escolher => match (data_da_tela(&self.de), data_da_tela(&self.ate)) {
                (Some(de), Some(ate)) => Some(api::PeriodoSprint::Datas(de, ate)),
                _ => {
                    if !self.de.is_empty() && !self.ate.is_empty() {
                        self.erro_sprint = Some("Use datas no formato DD/MM/AAAA.".into());
                    }
                    None
                }
            },
        }
    }

    fn pedir_sprint(&mut self, ctx: &egui::Context) {
        let Some(chave) = self.chave else { return };
        self.erro_sprint = None;
        let Some(periodo) = self.periodo_sprint() else { return };
        let outro = periodo.clone();
        em_segundo_plano(&self.canal.0, ctx, move || {
            let resultado = api::sprint(chave.0, chave.1, &periodo);
            Resposta::Sprint { chave, periodo, resultado }
        });
        em_segundo_plano(&self.canal.0, ctx, move || {
            let resultado = api::apresentacao(chave.0, chave.1, Some(&outro));
            Resposta::DeckSprint { chave, periodo: outro, resultado }
        });
    }

    /// A opção do tempo mudou. Ao desligar, tudo o que veio com tempo sai da
    /// memória na hora e a página visível busca de novo (o texto editado
    /// com tempo é refeito quando a resposta chegar). Ao ligar, o conteúdo
    /// fica e o tempo aparece quando a resposta chegar.
    fn tempo_mudou(&mut self, aba: Aba, ctx: &egui::Context) {
        if !self.mostrar_tempo {
            self.deck_daily = None;
            self.deck_sprint = None;
            self.dias.clear();
            self.carregou = false;
            self.novidades = 0;
            self.atualizando = true;
            self.refazer_sem_tempo = true;
        }
        self.sujas = [true; 3];
        if !self.conectado {
            return;
        }
        self.sujas[aba.indice()] = false;
        match aba {
            Aba::Linha => {
                self.carregando = false;
                self.pedir_pagina(ctx, true);
            }
            Aba::Daily => self.pedir_daily(ctx),
            Aba::Sprint => self.pedir_sprint(ctx),
        }
    }

    /// A resposta pode ficar: com o tempo desligado aqui, uma que ainda veio
    /// com tempo (pedida antes de o núcleo gravar a opção) é descartada.
    fn aceita(&mut self, com_tempo: bool) -> bool {
        if com_tempo && !self.mostrar_tempo {
            return false;
        }
        if !com_tempo {
            self.atualizando = false;
        }
        true
    }

    /// Desligou o tempo e a página ainda não tem a resposta sem ele.
    pub fn atualizando(&self) -> bool {
        self.atualizando
    }

    /// A página pode mostrar o tempo dos agentes (a opção e a resposta).
    pub(super) fn com_tempo(&self, deck: &api::Deck) -> bool {
        self.mostrar_tempo && deck.tempo_agentes
    }

    /// O deck pronto da página, se houver trabalho para apresentar.
    pub fn deck(&self, aba: Aba) -> Option<&api::Deck> {
        let deck = match aba {
            Aba::Daily => self.deck_daily.as_ref(),
            Aba::Sprint => self.deck_sprint.as_ref(),
            Aba::Linha => None,
        }?;
        (!deck.vazio).then_some(deck)
    }

    /// O deck da página já veio e não há nada para apresentar (F5 não abre).
    pub fn sem_o_que_apresentar(&self, aba: Aba) -> bool {
        match aba {
            Aba::Daily => self.deck_daily.as_ref().is_some_and(|d| d.vazio),
            Aba::Sprint => self.deck_sprint.as_ref().is_some_and(|d| d.vazio),
            Aba::Linha => false,
        }
    }

    /// O texto para copiar da página (o editado, se você editou).
    pub fn texto(&self, aba: Aba) -> Option<String> {
        if self.atualizando {
            return None;
        }
        match aba {
            Aba::Daily => self.daily.as_ref().filter(|d| !d.vazio).map(|_| self.texto_daily.clone()),
            Aba::Sprint => self.sprint.as_ref().filter(|s| !s.vazio).map(|_| self.texto_sprint.clone()),
            Aba::Linha => None,
        }
    }

    pub fn markdown(&self) -> Option<String> {
        if self.atualizando {
            return None;
        }
        self.sprint.as_ref().filter(|s| !s.vazio).map(|s| s.markdown.clone())
    }

    fn receber(&mut self, ctx: &egui::Context, acoes: &mut Vec<Acao>) {
        self.miniaturas.receber(ctx);
        while let Ok(r) = self.canal.1.try_recv() {
            match r {
                Resposta::Pagina { chave, primeira, resultado } if Some(chave) == self.chave => {
                    self.carregando = false;
                    match resultado {
                        Ok(pagina) if !self.aceita(pagina.tempo_agentes) => {}
                        Ok(pagina) => {
                            self.carregou = true;
                            self.erro = None;
                            self.proximo = pagina.proximo;
                            self.criado_em = pagina.perfil_criado_em;
                            if primeira {
                                self.dias = pagina.dias;
                                self.novidades = 0;
                            } else {
                                for dia in pagina.dias {
                                    // Um dia pode vir dividido entre duas páginas.
                                    match self.dias.last_mut() {
                                        Some(ultimo) if ultimo.dia == dia.dia => ultimo.itens.extend(dia.itens),
                                        _ => self.dias.push(dia),
                                    }
                                }
                            }
                        }
                        Err(e) => self.erro = Some(e),
                    }
                }
                // Sem o núcleo, a faixa do topo já avisa: a página guarda o que tinha.
                Resposta::Daily { resultado: Err(e), .. }
                | Resposta::DeckDaily { resultado: Err(e), .. }
                | Resposta::Sprint { resultado: Err(e), .. }
                | Resposta::DeckSprint { resultado: Err(e), .. }
                    if api::erro_de_conexao(&e) => {}
                Resposta::Daily { chave, resultado } if Some(chave) == self.chave => match resultado {
                    Ok(d) if !self.aceita(d.tempo_agentes) => {}
                    Ok(d) => {
                        // Só troca o texto se você não estava editando (ou se o
                        // editado tem o tempo dos agentes, que foi desligado).
                        let editado = self.daily.as_ref().is_some_and(|antigo| antigo.texto != self.texto_daily);
                        let refazer = self.refazer_sem_tempo && !d.tempo_agentes && (self.texto_daily_com_tempo || tem_tempo(&self.texto_daily));
                        if !editado || refazer {
                            if editado {
                                acoes.push(Acao::Avisar(TipoAviso::Neutro, "Texto refeito sem o tempo dos agentes".into()));
                            }
                            self.texto_daily = d.texto.clone();
                            self.texto_daily_com_tempo = d.tempo_agentes && tem_tempo(&d.texto);
                        }
                        self.daily = Some(d);
                    }
                    Err(e) => self.erro_daily = Some(if api::erro_inesperado(&e) { "Não consegui montar a daily.".into() } else { e }),
                },
                Resposta::DeckDaily { chave, resultado } if Some(chave) == self.chave => match resultado {
                    Ok(d) if !self.aceita(d.tempo_agentes) => {}
                    Ok(d) => {
                        self.deck_daily = Some(d);
                        self.erro_daily = None;
                    }
                    Err(e) => self.erro_daily = Some(if api::erro_inesperado(&e) { "Não consegui montar a daily.".into() } else { e }),
                },
                Resposta::Sprint { chave, periodo, resultado } if Some(chave) == self.chave && self.periodo_atual().as_ref() == Some(&periodo) => {
                    match resultado {
                        Ok(s) if !self.aceita(s.tempo_agentes) => {}
                        Ok(s) => {
                            let refazer = self.refazer_sem_tempo && !s.tempo_agentes && (self.texto_sprint_com_tempo || tem_tempo(&self.texto_sprint));
                            if refazer || self.sprint.as_ref().is_none_or(|antigo| antigo.texto == self.texto_sprint || antigo.periodo != s.periodo) {
                                self.texto_sprint = s.texto.clone();
                                self.texto_sprint_com_tempo = s.tempo_agentes && tem_tempo(&s.texto);
                            }
                            self.sprint = Some(s);
                        }
                        // O núcleo explica (datas invertidas, mais de 92 dias): aparece abaixo dos campos.
                        Err(e) => self.erro_sprint = Some(if api::erro_inesperado(&e) { "Não consegui montar a sprint.".into() } else { e }),
                    }
                }
                Resposta::DeckSprint { chave, periodo, resultado } if Some(chave) == self.chave && self.periodo_atual().as_ref() == Some(&periodo) => {
                    match resultado {
                        Ok(d) if !self.aceita(d.tempo_agentes) => {}
                        Ok(d) => {
                            self.deck_sprint = Some(d);
                            self.erro_sprint = None;
                        }
                        Err(e) => self.erro_sprint = Some(if api::erro_inesperado(&e) { "Não consegui montar a sprint.".into() } else { e }),
                    }
                }
                Resposta::Grande { id, resultado } => {
                    if let Some(v) = self.visor.as_mut().filter(|v| v.anexo == id) {
                        v.imagem = Some(resultado.map(|img| ctx.load_texture(format!("captura-{id}"), img, TextureOptions::LINEAR)));
                    }
                }
                Resposta::Salvo(resultado) => {
                    self.salvando = false;
                    match resultado {
                        Ok(caminho) => acoes.push(Acao::Avisar(TipoAviso::Neutro, format!("Salvo em {caminho}"))),
                        Err(e) if e.is_empty() => {}
                        Err(e) => acoes.push(Acao::Avisar(TipoAviso::Erro, format!("Não consegui salvar: {e}"))),
                    }
                }
                Resposta::ForaDaDaily { fora, resultado } => match resultado {
                    Ok(()) => {
                        let texto = if fora { "Tirada desta daily; a sprint continua com ela" } else { "De volta à daily" };
                        acoes.push(Acao::Avisar(TipoAviso::Neutro, texto.into()));
                        self.pedir_daily(ctx);
                    }
                    Err(e) => acoes.push(Acao::Avisar(TipoAviso::Erro, format!("Não consegui mudar a daily: {e}"))),
                },
                Resposta::Removido(resultado) => match resultado {
                    Ok(()) => {
                        if let Some(v) = self.visor.take() {
                            self.miniaturas.esquecer(v.anexo);
                        }
                        acoes.push(Acao::Avisar(TipoAviso::Neutro, "Captura removida".into()));
                        self.pedir_pagina(ctx, true);
                        self.pedir_daily(ctx);
                        self.pedir_sprint(ctx);
                    }
                    Err(e) => acoes.push(Acao::Avisar(TipoAviso::Erro, format!("Não consegui remover: {e}"))),
                },
                // Resposta de um escopo (ou período) que já não está na tela.
                _ => {}
            }
        }
    }

    /// O período sem mexer no aviso de erro (para comparar respostas).
    fn periodo_atual(&self) -> Option<api::PeriodoSprint> {
        match self.periodo {
            Periodo::Dias7 => Some(api::PeriodoSprint::Ultimos(7)),
            Periodo::Dias14 => Some(api::PeriodoSprint::Ultimos(14)),
            Periodo::Mes => Some(api::PeriodoSprint::MesAtual),
            Periodo::Escolher => match (data_da_tela(&self.de), data_da_tela(&self.ate)) {
                (Some(de), Some(ate)) => Some(api::PeriodoSprint::Datas(de, ate)),
                _ => None,
            },
        }
    }

    /// Desenha a página do `recorte`. `projetos` são todos os do perfil, na
    /// ordem da barra lateral (para os nomes, os links e o "sem atividade").
    pub fn mostrar(&mut self, ui: &mut egui::Ui, aba: Aba, perfil: i64, recorte: api::Recorte, projetos: &[Projeto]) -> Vec<Acao> {
        let ctx = ui.ctx().clone();
        let mut acoes = Vec::new();
        let chave = (perfil, recorte);
        if self.chave != Some(chave) {
            let guardado =
                (self.periodo, self.texto_daily_aberto, self.focar_daily, self.galeria_aberta, self.de.clone(), self.ate.clone(), self.mostrar_tempo);
            *self = Registro { canal: std::mem::replace(&mut self.canal, mpsc::channel()), ..Registro::default() };
            let tempo;
            (self.periodo, self.texto_daily_aberto, self.focar_daily, self.galeria_aberta, self.de, self.ate, tempo) = guardado;
            self.mostrar_tempo = tempo;
            self.tempo_visto = Some(tempo);
            self.chave = Some(chave);
            self.sujas = [true; 3];
            self.ir_ao_topo = true;
            self.ao_topo = [true; 2];
        }
        if self.tempo_visto != Some(self.mostrar_tempo) {
            let primeira = self.tempo_visto.is_none();
            self.tempo_visto = Some(self.mostrar_tempo);
            if !primeira {
                self.tempo_mudou(aba, &ctx);
            }
        }
        if self.conectado && std::mem::take(&mut self.sujas[aba.indice()]) {
            // A página aparece depois de eventos que ela não viu (ou pela primeira vez).
            match aba {
                Aba::Linha => {
                    self.carregando = false;
                    if self.no_topo || self.dias.is_empty() {
                        self.pedir_pagina(&ctx, true);
                    } else {
                        self.novidades = self.novidades.max(1);
                    }
                }
                Aba::Daily => self.pedir_daily(&ctx),
                Aba::Sprint => self.pedir_sprint(&ctx),
            }
        }
        self.receber(&ctx, &mut acoes);

        // As três páginas na mesma moldura de 1120, com o título na mesma
        // coluna esquerda; a linha do tempo lê melhor em 880, alinhada à
        // esquerda da moldura (o título não pula ao trocar de aba).
        let area = ui.available_rect_before_wrap();
        let largura = area.width().min(1120.0);
        let largura = if aba == Aba::Linha { largura.min(880.0 + comum::MARGEM_ROLAGEM) } else { largura };
        let esquerda = area.center().x - area.width().min(1120.0) / 2.0;
        let caixa = egui::Rect::from_min_size(egui::pos2(esquerda, area.top()), vec2(largura, area.height()));
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa).layout(egui::Layout::top_down(egui::Align::Min)));
        let escopo = Escopo::novo(recorte, projetos);
        match aba {
            Aba::Linha => self.pagina_linha(&mut filho, recorte, &mut acoes),
            Aba::Daily => self.pagina_daily(&mut filho, &escopo, &mut acoes),
            Aba::Sprint => self.pagina_sprint(&mut filho, &escopo, &mut acoes),
        }
        ui.allocate_rect(area, Sense::hover());
        self.mostrar_visor(&ctx, &mut acoes);
        acoes
    }

    fn abrir_visor(&mut self, ctx: &egui::Context, anexo: i64, texto: &str, quando: &str, tarefa: i64) {
        self.visor = Some(Visor { anexo, texto: texto.to_string(), quando: quando.to_string(), tarefa, imagem: None, confirmar: false });
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Grande { id: anexo, resultado: api::ler_anexo(anexo).and_then(|png| decodificar(&png, None)) });
    }

    fn mostrar_visor(&mut self, ctx: &egui::Context, acoes: &mut Vec<Acao>) {
        let Some(visor) = &mut self.visor else { return };
        let p = cores();
        let tela = ctx.content_rect();
        let maximo = vec2((tela.width() * 0.9).min(1200.0), (tela.height() * 0.9 - 160.0).min(700.0));
        let (mut fechar, mut remover, mut abrir) = (false, false, false);
        let modal = egui::Modal::new(Id::new("visor-captura"))
            .frame(tema::moldura_janela())
            .backdrop_color(Color32::from_black_alpha(if tema::claro() { 60 } else { 140 }))
            .show(ctx, |ui| {
                ui.label(tema::texto_forte(&visor.texto, 15.0).color(p.texto));
                ui.label(RichText::new(&visor.quando).color(p.suave).size(12.5));
                ui.add_space(10.0);
                match &visor.imagem {
                    Some(Ok(t)) => {
                        let tamanho = t.size_vec2();
                        let escala = (maximo.x / tamanho.x).min(maximo.y / tamanho.y).min(1.0);
                        ui.add(egui::Image::new(t).fit_to_exact_size(tamanho * escala).corner_radius(CornerRadius::same(tema::RAIO_CONTROLE)));
                    }
                    Some(Err(_)) => {
                        let (r, _) = ui.allocate_exact_size(vec2(480.0, 200.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "imagem indisponível", FontId::proportional(12.0), p.suave);
                    }
                    None => {
                        let (r, _) = ui.allocate_exact_size(vec2(480.0, 270.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
                    }
                }
                ui.add_space(16.0);
                if visor.confirmar {
                    ui.label(
                        RichText::new("Remover esta imagem? Ela sai da linha do tempo, da daily e da sprint; o arquivo é apagado.").color(p.texto).size(13.5),
                    );
                    ui.add_space(10.0);
                }
                ui.horizontal(|ui| {
                    if visor.confirmar {
                        if tema::botao_secundario(ui, "Cancelar").clicked() {
                            visor.confirmar = false;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            remover = tema::botao_principal(ui, "Remover", true).clicked();
                        });
                        return;
                    }
                    if visor.tarefa != 0 {
                        abrir = tema::botao_secundario(ui, "Abrir tarefa").clicked();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        fechar = tema::botao_secundario(ui, "Fechar").clicked();
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Remover").clicked() {
                            visor.confirmar = true;
                        }
                    });
                });
            });
        // Na confirmação, Enter remove e Esc volta (sem fechar o visor).
        if visor.confirmar {
            let (enter, esc) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if enter {
                remover = true;
            } else if esc {
                visor.confirmar = false;
                ctx.request_repaint();
                return;
            }
        }
        let (anexo, tarefa) = (visor.anexo, visor.tarefa);
        if abrir {
            acoes.push(Acao::AbrirTarefa { tarefa, agente: None, lousa: false });
            fechar = true;
        }
        if remover {
            em_segundo_plano(&self.canal.0, ctx, move || Resposta::Removido(api::remover_anexo(anexo)));
        }
        if fechar || modal.should_close() {
            self.visor = None;
        }
    }

    /// Salva o .md e as capturas numa pasta "capturas" ao lado, onde você
    /// escolher. É o único ponto em que a tela escreve no disco.
    pub fn salvar_sprint(&mut self, ctx: &egui::Context) {
        let Some(sprint) = self.sprint.clone().filter(|s| !s.vazio) else { return };
        if self.salvando {
            return;
        }
        self.salvando = true;
        let nome = format!("sprint-{}-a-{}.md", sprint.de.replace('/', "-"), sprint.ate.replace('/', "-"));
        let markdown = sprint.markdown.clone();
        let capturas: Vec<i64> = sprint.capturas.iter().map(|c| c.anexo).collect();
        em_segundo_plano(&self.canal.0, ctx, move || {
            let Some(arquivo) = rfd::FileDialog::new().set_title("Salvar a sprint").set_file_name(&nome).add_filter("Markdown", &["md"]).save_file() else {
                // Cancelou: nada a avisar.
                return Resposta::Salvo(Err(String::new()));
            };
            let resultado = (|| -> Result<String, String> {
                std::fs::write(&arquivo, markdown).map_err(|e| e.to_string())?;
                if !capturas.is_empty() {
                    let pasta = arquivo.with_file_name("capturas");
                    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
                    for id in capturas {
                        let png = api::ler_anexo(id)?;
                        std::fs::write(pasta.join(format!("{id}.png")), png).map_err(|e| e.to_string())?;
                    }
                }
                Ok(arquivo.display().to_string())
            })();
            Resposta::Salvo(resultado)
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn reconhece_o_tempo_nos_textos_da_daily_e_da_sprint() {
        // Daily.
        assert!(tem_tempo("• Importar planilha: Claude Code trabalhou 12 min"));
        assert!(tem_tempo("esperando você desde 14:26"));
        assert!(!tem_tempo("esperando você desde ontem"));
        // Sprint (texto e Markdown).
        assert!(tem_tempo("• Agentes: 1 min\n\nAgentes no total: 1 min"));
        assert!(tem_tempo("Agentes no total: 3 min (Claude Code 3 min)"));
        assert!(tem_tempo("\n**Tempo de agentes:** 1 min\n"));
        // Sem tempo.
        assert!(!tem_tempo("• Importar planilha: terminou uma sessão"));
    }
}
