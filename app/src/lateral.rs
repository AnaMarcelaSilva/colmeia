//! A barra lateral: o trilho com os workspaces em quadrados e, ao lado, o
//! painel flutuante do workspace aberto, com os projetos, as tarefas em
//! andamento e os agentes em árvore; no pé dele, a abelha e o uso. A barra
//! só mostra e pede: o que é clicado volta como `Acao` para a tela principal.

use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, PI};

use eframe::egui::{self, Align, Color32, CornerRadius, FontId, Id, Layout, Rect, RectAlign, Response, Sense, Shape, Stroke, Ui, UiBuilder, pos2, vec2};

use crate::Escopo;
use crate::abelha::{self, Abelha};
use crate::api;
use crate::dados::{AgenteTela, Coluna, Projeto, Tarefa};
use crate::tema::{self, EstadoVisual, Icone, cores};

/// Largura do trilho.
pub const TRILHO: f32 = 64.0;
/// Largura do painel e o vão até o cartão do conteúdo.
const PAINEL: f32 = 292.0;
const VAO: f32 = 10.0;
/// Abrir e recolher o painel: curto, e só o tempo da transição pede quadros.
const ANIMACAO_PAINEL: f32 = 0.2;
/// As linhas da árvore têm altura fixa: a altura de um ramo aberto se sabe
/// antes de desenhar, e abrir e fechar anima sem medir nada.
const ALTURA_PROJETO: f32 = 32.0;
const ALTURA_TAREFA: f32 = 30.0;
const ALTURA_AGENTE: f32 = 28.0;
const ALTURA_RESTO: f32 = 24.0;
/// O bloco da abelha com o uso, no pé do painel.
const ALTURA_USO: f32 = 92.0;

/// O que está na tela, para marcar o item escolhido.
#[derive(Clone, Copy, PartialEq)]
pub enum Lugar {
    Quadro,
    Tarefa { id: i64, foco: i64 },
    Lousa(i64),
    Banco,
}

/// O que a barra precisa ver da tela principal neste quadro.
pub struct Vista<'a> {
    pub projetos: &'a [Projeto],
    pub tarefas: &'a [Tarefa],
    pub perfil: Option<&'a api::Perfil>,
    pub perfis: &'a [api::Perfil],
    pub tema: tema::Escolha,
    pub escopo: Escopo,
    pub lugar: Lugar,
    /// Pedidos de consulta ao banco esperando você.
    pub pedidos_banco: usize,
    /// Quantas conexões de banco o perfil tem (o subtítulo do painel dos bancos).
    pub conexoes_banco: usize,
    pub demo: bool,
    /// Ligada ao núcleo (ou na demonstração): dá para mudar as coisas.
    pub pode: bool,
    /// Os agentes ativos estão rodando de fato (na demonstração, a carga de teste).
    pub rodando: bool,
    /// A linha de resumo embaixo do estado da abelha.
    pub linha: &'a str,
    /// O microfone dos comandos falados está aberto: um ponto vermelho na abelha.
    pub microfone: bool,
    pub com_abelha: bool,
    pub agora: f64,
    /// O uso dos agentes e da conta (pela statusLine do Claude Code).
    pub uso: &'a crate::dados::Uso,
}

/// O que foi pedido na barra.
pub enum Acao {
    Escopo(Escopo),
    AbrirTarefa {
        tarefa: i64,
        agente: Option<i64>,
    },
    Lousa(i64),
    Bancos,
    /// Novo projeto já no workspace dado; `None` começa num workspace novo.
    NovoProjeto(Option<i64>),
    RemoverProjeto {
        id: i64,
        nome: String,
    },
    /// O menu do perfil abriu: a lista é lida de novo (um perfil criado em outro lugar aparece).
    LerPerfis,
    TrocarPerfil(api::Perfil),
    Entrada {
        criar: bool,
    },
    Tema(tema::Escolha),
    TempoAgentes(bool),
    AvisoCaptura(bool),
    /// Escolher o acabamento e o nome da abelha.
    Mascote,
    /// Fixar (ou desafixar) a abelha na área de trabalho.
    AbelhaFixa(bool),
    /// Abrir os ajustes do ditado por voz.
    Ditado,
    Proximo,
    Linha,
    Daily,
    /// A abelha foi clicada: abre ou fecha o resumo (ou, com os comandos
    /// falados, abre o microfone).
    Abelha,
    /// O botão direito na abelha: o resumo, mesmo com os comandos falados.
    ResumoDaAbelha,
    /// Abrir os ajustes dos comandos falados e da voz da abelha.
    Voz,
    /// Abrir a opção da pergunta livre com IA (à parte: usa tokens).
    PerguntaIA,
}

#[derive(Default)]
pub struct Saida {
    pub acoes: Vec<Acao>,
    /// Onde a abelha está (o resumo abre ao lado).
    pub abelha: Option<Rect>,
    /// Na tela de bancos, o miolo do painel fica para o painel dos bancos.
    pub banco: Option<PainelBanco>,
}

/// Onde a tela principal desenha o painel dos bancos (com o recorte e a
/// opacidade da animação de abrir e recolher).
pub struct PainelBanco {
    pub miolo: Rect,
    pub recorte: Rect,
    pub opacidade: f32,
}

/// O uso do agente em foco e da conta, como o `/context` e o `/usage`: o
/// contexto, a janela de 5 horas e a semana. Sem dados, a barra fica vazia.
#[derive(Default)]
pub struct Uso {
    pub contexto: Option<Medida>,
    pub cinco_horas: Option<Medida>,
    pub semana: Option<Medida>,
}

impl Uso {
    /// Monta o uso para a tela: o contexto do agente em foco e os limites que
    /// ainda não voltaram. `agora` em segundos Unix.
    fn montar(uso: &crate::dados::Uso, foco: Option<&AgenteTela>, agora: i64) -> Uso {
        let contexto = foco.and_then(|a| {
            let c = uso.contextos.get(&a.id)?;
            Some(Medida {
                fracao: c.porcentagem / 100.0, detalhe: format!("{} · {} de {} tokens", a.nome_com_papel(), milhares(c.usado), milhares(c.tamanho))
            })
        });
        let limite = |l: &Option<api::LimiteUso>| {
            l.as_ref().filter(|l| l.volta > agora).map(|l| Medida { fracao: l.porcentagem / 100.0, detalhe: l.volta_texto.clone() })
        };
        Uso { contexto, cinco_horas: limite(&uso.cinco_horas), semana: limite(&uso.semana) }
    }
}

/// "84 mil", "1,2 mi": o tamanho do contexto como se fala.
fn milhares(n: i64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{} mil", n / 1_000)
    } else {
        format!("{:.1} mi", n as f64 / 1e6).replace('.', ",")
    }
}

/// Uma das três medidas: quanto já foi (de 0 a 1) e o detalhe da dica
/// (o agente, a hora em que o limite volta).
pub struct Medida {
    pub fracao: f32,
    pub detalhe: String,
}

/// Um nó da árvore que você abriu ou fechou à mão.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum No {
    Projeto(i64),
    Tarefa(i64),
}

pub struct Lateral {
    aberto: bool,
    /// O workspace do painel.
    workspace: Option<i64>,
    /// O workspace do que estava na tela no quadro anterior: quando ele
    /// muda (abriu uma tarefa de outro workspace), o painel acompanha.
    foco_anterior: Option<i64>,
    /// Os nós trocados à mão; o resto fica no padrão (aberto o que anda).
    trocados: HashSet<No>,
    uso: Uso,
    /// De onde o `uso` foi montado: o agente em foco, a versão do uso e o
    /// minuto (um limite que voltou some sem esperar evento).
    uso_de: Option<(i64, u64, i64)>,
}

impl Default for Lateral {
    fn default() -> Self {
        Lateral { aberto: true, workspace: None, foco_anterior: None, trocados: HashSet::new(), uso: Uso::default(), uso_de: None }
    }
}

/// Um workspace como o trilho mostra: tirado dos projetos, na ordem deles.
struct Grupo<'a> {
    id: i64,
    nome: &'a str,
    projetos: Vec<&'a Projeto>,
}

fn grupos(projetos: &[Projeto]) -> Vec<Grupo<'_>> {
    let mut lista: Vec<Grupo> = Vec::new();
    for p in projetos {
        match lista.iter_mut().find(|g| g.id == p.workspace_id && g.nome == p.workspace) {
            Some(g) => g.projetos.push(p),
            None => lista.push(Grupo { id: p.workspace_id, nome: &p.workspace, projetos: vec![p] }),
        }
    }
    lista
}

impl Lateral {
    /// A largura de agora: o painel entra e sai aos poucos.
    pub fn largura(&self, ctx: &egui::Context) -> f32 {
        TRILHO + self.abertura(ctx) * (PAINEL + VAO)
    }

    /// O painel aparece, ainda que só em parte (abrindo ou recolhendo).
    pub fn painel_visivel(&self, ctx: &egui::Context) -> bool {
        self.abertura(ctx) > 0.0
    }

    fn abertura(&self, ctx: &egui::Context) -> f32 {
        ctx.animate_bool_with_time(Id::new("lateral-painel"), self.aberto, ANIMACAO_PAINEL)
    }

    pub fn mostrar(&mut self, ui: &mut Ui, abelha: &mut Abelha, v: &Vista) -> Saida {
        let mut saida = Saida::default();
        self.atualizar_uso(v);
        let grupos = grupos(v.projetos);
        self.seguir_foco(v, &grupos);
        let area = ui.max_rect();
        let t = self.abertura(ui.ctx());
        self.trilho(ui, Rect::from_min_size(area.min, vec2(TRILHO, area.height())), &grupos, abelha, v, &mut saida);
        let grupo = grupos.iter().find(|g| Some(g.id) == self.workspace);
        // Nos bancos o painel mostra as conexões, mesmo sem workspace.
        if t > 0.0 && (v.lugar == Lugar::Banco || grupo.is_some()) {
            self.painel(ui, area, grupo, abelha, v, &mut saida);
        }
        saida
    }

    /// Remonta o uso só quando muda o foco, chega uso novo ou vira o minuto.
    fn atualizar_uso(&mut self, v: &Vista) {
        let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let foco = match v.lugar {
            Lugar::Tarefa { id, foco } => v.tarefas.iter().find(|t| t.id == id).and_then(|t| t.agentes.iter().find(|a| a.id == foco)),
            _ => None,
        };
        let chave = (foco.map_or(0, |a| a.id), v.uso.versao, agora / 60);
        if self.uso_de != Some(chave) {
            self.uso = Uso::montar(v.uso, foco, agora);
            self.uso_de = Some(chave);
        }
    }

    fn seguir_foco(&mut self, v: &Vista, grupos: &[Grupo]) {
        let foco = workspace_em_foco(v);
        if foco != self.foco_anterior {
            if foco.is_some() {
                self.workspace = foco;
            }
            self.foco_anterior = foco;
        }
        if !grupos.iter().any(|g| Some(g.id) == self.workspace) {
            self.workspace = grupos.first().map(|g| g.id);
        }
    }

    fn aberto_no(&self, no: No, padrao: bool) -> bool {
        padrao != self.trocados.contains(&no)
    }

    fn trocar(&mut self, no: No) {
        if !self.trocados.remove(&no) {
            self.trocados.insert(no);
        }
    }

    // O trilho

    fn trilho(&mut self, ui: &mut Ui, rect: Rect, grupos: &[Grupo], abelha: &mut Abelha, v: &Vista, saida: &mut Saida) {
        let p = cores();
        ui.painter().rect_filled(rect, CornerRadius::ZERO, tema::fundo_lateral(p));
        brilho_do_pe(ui.painter(), rect);

        // O pé, de baixo para cima: perfil, tema, ajustes, recolher e, com o
        // painel recolhido, a abelha.
        let com_abelha_aqui = v.com_abelha && !self.aberto;
        let altura_pe = 12.0 + 34.0 + 3.0 * 48.0 + if com_abelha_aqui { 48.0 } else { 0.0 } + 18.0;
        let pe = Rect::from_min_max(pos2(rect.left(), rect.bottom() - altura_pe), rect.max);
        let mut baixo = ui.new_child(UiBuilder::new().id_salt("trilho-pe").max_rect(pe.shrink2(vec2(0.0, 12.0))).layout(Layout::bottom_up(Align::Center)));
        baixo.spacing_mut().item_spacing = vec2(0.0, 8.0);
        self.pe_do_trilho(&mut baixo, abelha, v, com_abelha_aqui, saida);

        let cima = Rect::from_min_max(rect.min + vec2(0.0, 16.0), pos2(rect.right(), pe.top()));
        let mut ui = ui.new_child(UiBuilder::new().id_salt("trilho-cima").max_rect(cima).layout(Layout::top_down(Align::Center)));
        ui.spacing_mut().item_spacing = vec2(0.0, 10.0);
        let (logo, _) = ui.allocate_exact_size(vec2(TRILHO, 28.0), Sense::hover());
        tema::logo(ui.painter(), logo.center(), 12.0);
        ui.add_space(4.0);

        // Os quadrados rolam quando não cabem; os botões de baixo ficam.
        let botoes = 40.0 + 21.0 + 3.0 * 50.0;
        let altura = (ui.available_height() - botoes).max(50.0);
        egui::ScrollArea::vertical()
            .id_salt("trilho-workspaces")
            .max_height(altura)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 10.0);
                for g in grupos {
                    let ativo = self.aberto && self.workspace == Some(g.id);
                    let espera = abelha::esperando(v.tarefas.iter().filter(|t| g.projetos.iter().any(|p| p.id == t.projeto_id)));
                    if quadrado_do_workspace(ui, g, ativo, espera).clicked() {
                        if ativo {
                            self.aberto = false;
                        } else {
                            self.workspace = Some(g.id);
                            self.aberto = true;
                        }
                    }
                }
            });
        if !v.demo && v.pode && botao_novo_workspace(&mut ui).clicked() {
            saida.acoes.push(Acao::NovoProjeto(None));
        }
        let (r, _) = ui.allocate_exact_size(vec2(TRILHO, 1.0), Sense::hover());
        ui.painter().hline(r.center().x - 14.0..=r.center().x + 14.0, r.center().y, Stroke::new(1.0, p.borda));

        let todos = v.escopo == Escopo::Perfil && v.lugar == Lugar::Quadro;
        if botao_do_trilho(&mut ui, Icone::Grade, todos, 0).on_hover_text("Todos os projetos").clicked() {
            saida.acoes.push(Acao::Escopo(Escopo::Perfil));
        }
        if !v.demo && botao_do_trilho(&mut ui, Icone::Banco, v.lugar == Lugar::Banco, v.pedidos_banco).on_hover_text("Bancos de dados (Ctrl+Shift+K)").clicked()
        {
            saida.acoes.push(Acao::Bancos);
        }
        let atalhos = botao_do_trilho(&mut ui, Icone::Teclado, false, 0);
        let atalhos = dica_fora_do_menu(atalhos, "Atalhos");
        egui::Popup::menu(&atalhos).align(RectAlign::RIGHT_START).gap(10.0).show(|ui| {
            ui.set_min_width(300.0);
            // Na demonstração os atalhos de teclado ficam desligados; daqui, só o primeiro anda.
            let itens = [
                ("Próximo que precisa de você", "Ctrl+Shift+P", true),
                ("Histórico", "Ctrl+Shift+L", !v.demo),
                ("Daily", "Ctrl+Shift+D", !v.demo),
                ("Bancos de dados", "Ctrl+Shift+K", !v.demo),
            ];
            for (i, (texto, tecla, ativa)) in itens.into_iter().enumerate() {
                if tema::opcao_menu_com(ui, texto, Some(tecla), ativa) {
                    saida.acoes.push(match i {
                        0 => Acao::Proximo,
                        1 => Acao::Linha,
                        2 => Acao::Daily,
                        _ => Acao::Bancos,
                    });
                    ui.close();
                }
            }
        });
    }

    fn pe_do_trilho(&mut self, ui: &mut Ui, abelha: &mut Abelha, v: &Vista, com_abelha: bool, saida: &mut Saida) {
        let p = cores();
        // Perfil: trocar, criar ou sair.
        let nome = v.perfil.map_or("", |p| p.nome.as_str());
        let (rect, _) = ui.allocate_exact_size(vec2(TRILHO, 34.0), Sense::hover());
        let circulo = Rect::from_center_size(rect.center(), vec2(34.0, 34.0));
        let resposta = ui.interact(circulo, ui.id().with("perfil"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let mouse = tema::transicao(ui, resposta.id.with("mouse"), resposta.hovered());
        let fundo = p.superficie_alta.lerp_to_gamma(p.realce, mouse);
        ui.painter().circle(circulo.center(), 17.0, fundo, Stroke::new(1.5, tema::misturar(p.superficie_alta, p.destaque, 0.55)));
        ui.painter().text(circulo.center(), egui::Align2::CENTER_CENTER, inicial(nome), tema::forte(13.0), p.destaque);
        let resposta = dica_fora_do_menu(resposta, &format!("Perfil: {nome}"));
        if resposta.clicked() && !v.demo {
            saida.acoes.push(Acao::LerPerfis);
        }
        if !v.demo {
            egui::Popup::menu(&resposta).align(RectAlign::RIGHT_END).gap(10.0).show(|ui| {
                ui.set_min_width(220.0);
                for outro in v.perfis {
                    let atual = v.perfil.is_some_and(|p| p.id == outro.id);
                    if tema::opcao_menu(ui, &outro.nome, atual) {
                        if !atual {
                            saida.acoes.push(Acao::TrocarPerfil(outro.clone()));
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if tema::opcao_menu(ui, "Criar perfil…", false) {
                    saida.acoes.push(Acao::Entrada { criar: true });
                    ui.close();
                }
                if tema::opcao_menu(ui, "Sair do perfil", false) {
                    saida.acoes.push(Acao::Entrada { criar: false });
                    ui.close();
                }
            });
        }

        let tema_atual = botao_do_trilho(ui, Icone::Tema, false, 0);
        let tema_atual = dica_fora_do_menu(tema_atual, &format!("Tema: {}", v.tema.nome()));
        egui::Popup::menu(&tema_atual).align(RectAlign::RIGHT_END).gap(10.0).show(|ui| {
            ui.set_min_width(160.0);
            for opcao in tema::Escolha::TODAS {
                if tema::opcao_menu(ui, opcao.nome(), opcao == v.tema) {
                    if opcao != v.tema {
                        saida.acoes.push(Acao::Tema(opcao));
                    }
                    ui.close();
                }
            }
        });

        let ajustes = botao_do_trilho(ui, Icone::Engrenagem, false, 0);
        let ajustes = dica_fora_do_menu(ajustes, "Ajustes do perfil");
        egui::Popup::menu(&ajustes).align(RectAlign::RIGHT_END).gap(10.0).show(|ui| {
            ui.set_min_width(300.0);
            let pode = v.pode && !v.demo;
            let tempo = v.perfil.is_some_and(|p| p.tempo_agentes);
            let aviso = v.perfil.is_some_and(|p| p.aviso_captura);
            for (texto, ligado, acao) in [
                ("Mostrar tempo dos agentes", tempo, Acao::TempoAgentes as fn(bool) -> Acao),
                ("Avisar antes de capturar um terminal", aviso, Acao::AvisoCaptura as fn(bool) -> Acao),
            ] {
                let clicou = if pode {
                    tema::opcao_menu(ui, texto, ligado)
                } else {
                    tema::opcao_menu_com(ui, texto, Some(if ligado { "ligado" } else { "desligado" }), false)
                };
                if clicou {
                    saida.acoes.push(acao(!ligado));
                    ui.close();
                }
            }
            ui.separator();
            let nome = v.perfil.map_or(api::NOME_DA_ABELHA, |p| p.mascote_nome.as_str());
            let modelo = mascote::Modelo::do_codigo(v.perfil.map_or("", |p| p.mascote_modelo.as_str())).nome();
            if tema::opcao_menu_com(ui, &format!("Abelha: {nome}"), Some(modelo), pode) {
                saida.acoes.push(Acao::Mascote);
                ui.close();
            }
            let fixa = v.perfil.is_some_and(|p| p.abelha_fixa);
            let texto = "Fixar a abelha na área de trabalho";
            let clicou = if pode { tema::opcao_menu(ui, texto, fixa) } else { tema::opcao_menu_com(ui, texto, Some("desligado"), false) };
            if clicou {
                saida.acoes.push(Acao::AbelhaFixa(!fixa));
                ui.close();
            }
            ui.separator();
            let ditado = if v.perfil.is_some_and(|p| p.ditado) { "ligado" } else { "desligado" };
            if tema::opcao_menu_com(ui, "Ditado por voz", Some(ditado), pode) {
                saida.acoes.push(Acao::Ditado);
                ui.close();
            }
            let comandos = if v.perfil.is_some_and(|p| p.voz.voz_comandos) { "ligados" } else { "desligados" };
            if tema::opcao_menu_com(ui, "Comandos falados e voz da abelha", Some(comandos), pode) {
                saida.acoes.push(Acao::Voz);
                ui.close();
            }
            let pergunta = if v.perfil.is_some_and(|p| p.voz.voz_ia) { "ligada" } else { "usa tokens" };
            if tema::opcao_menu_com(ui, "Pergunta livre com IA", Some(pergunta), pode) {
                saida.acoes.push(Acao::PerguntaIA);
                ui.close();
            }
        });

        let dica = if self.aberto { "Recolher o painel" } else { "Abrir o painel" };
        if botao_do_trilho(ui, Icone::Painel, !self.aberto, 0).on_hover_text(dica).clicked() {
            self.aberto = !self.aberto;
        }

        if com_abelha {
            let (linha, _) = ui.allocate_exact_size(vec2(TRILHO, 40.0), Sense::hover());
            let caixa = Rect::from_center_size(linha.center(), vec2(40.0, 40.0));
            let em_cima = ui.rect_contains_pointer(caixa) || abelha.resumo_aberto;
            let t = tema::transicao(ui, ui.id().with("abelha-trilho"), em_cima);
            if t > 0.0 {
                ui.painter().rect_filled(caixa, CornerRadius::same(11), tema::realce_lateral(p).gamma_multiply(t));
            }
            let resposta = abelha.mostrar(ui, caixa, caixa.shrink(3.0), Id::new("abelha-trilho"), v.agora);
            let resposta =
                if abelha.resumo_aberto { resposta } else { resposta.on_hover_text(format!("{} · {} · {}", abelha.nome, abelha.nome_do_estado(), v.linha)) };
            if resposta.clicked() {
                saida.acoes.push(Acao::Abelha);
            }
            if resposta.secondary_clicked() {
                saida.acoes.push(Acao::ResumoDaAbelha);
            }
            if v.microfone {
                ponto_do_microfone(ui, caixa);
            }
            saida.abelha = Some(caixa);
        }
    }

    // O painel

    fn painel(&mut self, ui: &mut Ui, area: Rect, grupo: Option<&Grupo>, abelha: &mut Abelha, v: &Vista, saida: &mut Saida) {
        let p = cores();
        let t = self.abertura(ui.ctx());
        // Recolhendo, o painel escorrega 12 px para baixo do trilho e some.
        let x = area.left() + TRILHO - (1.0 - t) * 12.0;
        let rect = Rect::from_min_max(pos2(x, area.top() + tema::MARGEM_MOLDURA), pos2(x + PAINEL, area.bottom() - tema::MARGEM_MOLDURA));
        let recorte = Rect::from_min_max(pos2(area.left() + TRILHO, area.top()), area.max);
        let mut ui = ui.new_child(UiBuilder::new().id_salt("painel").max_rect(rect).layout(Layout::top_down(Align::Min)));
        ui.shrink_clip_rect(recorte);
        if t < 1.0 {
            ui.multiply_opacity(t);
        }
        tema::cartao_solto(ui.painter(), rect, p.superficie);

        let uso = Rect::from_min_size(pos2(rect.left() + 12.0, rect.bottom() - 12.0 - ALTURA_USO), vec2(rect.width() - 24.0, ALTURA_USO));
        let miolo = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top() + 62.0), pos2(rect.right() - 10.0, uso.top() - 8.0));
        match grupo.filter(|_| v.lugar != Lugar::Banco) {
            Some(grupo) => {
                self.cabeca_do_painel(&mut ui, rect, grupo, v, saida);
                let mut miolo_ui = ui.new_child(UiBuilder::new().id_salt("painel-miolo").max_rect(miolo).layout(Layout::top_down(Align::Min)));
                miolo_ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                self.miolo(&mut miolo_ui, grupo, v, saida);
            }
            None => {
                self.cabeca_dos_bancos(&mut ui, rect, v);
                saida.banco = Some(PainelBanco { miolo, recorte: ui.clip_rect(), opacidade: t });
            }
        }

        if v.com_abelha {
            bloco_do_uso(&mut ui, uso, abelha, &self.uso, v, saida);
        } else {
            barras_de_uso(&ui, Rect::from_min_max(pos2(uso.left() + 10.0, uso.bottom() - 30.0), uso.max - vec2(10.0, 8.0)), &self.uso);
        }
    }

    /// O topo do painel na tela de bancos: o selo, o título e quantas
    /// conexões e pedidos há.
    fn cabeca_dos_bancos(&mut self, ui: &mut Ui, rect: Rect, v: &Vista) {
        let p = cores();
        let fechar = Rect::from_min_size(pos2(rect.right() - 12.0 - 28.0, rect.top() + 16.0), vec2(28.0, 28.0));
        if tema::botao_icone_em(ui, fechar, ui.id().with("recolher"), Icone::Anterior)
            .on_hover_text("Recolher o painel (as conexões vão para o lado do console)")
            .clicked()
        {
            self.aberto = false;
        }
        let selo = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 18.0), vec2(24.0, 24.0));
        ui.painter().rect_filled(selo, CornerRadius::same(7), p.destaque);
        tema::desenhar_icone(ui.painter(), selo.center(), Icone::Banco, tema::sobre_destaque());
        let x = selo.right() + 10.0;
        let largura = fechar.left() - 6.0 - x;
        let nome = tema::cortar(ui.painter(), "Bancos de dados", egui::TextFormat::simple(tema::forte(15.0), p.texto), largura, 1, false);
        let conexoes = if v.conexoes_banco == 1 { "1 conexão".to_string() } else { format!("{} conexões", v.conexoes_banco) };
        let sub = match v.pedidos_banco {
            0 => conexoes,
            1 => format!("{conexoes} · 1 pedido dos agentes"),
            n => format!("{conexoes} · {n} pedidos dos agentes"),
        };
        let sub = tema::cortar(ui.painter(), &sub, egui::TextFormat::simple(FontId::proportional(11.5), p.suave), largura, 1, false);
        let y = selo.center().y - (nome.size().y + sub.size().y) / 2.0;
        ui.painter().galley(pos2(x, y), nome.clone(), p.texto);
        ui.painter().galley(pos2(x, y + nome.size().y), sub, p.suave);
    }

    fn cabeca_do_painel(&mut self, ui: &mut Ui, rect: Rect, grupo: &Grupo, v: &Vista, saida: &mut Saida) {
        let p = cores();
        let fechar = Rect::from_min_size(pos2(rect.right() - 12.0 - 28.0, rect.top() + 16.0), vec2(28.0, 28.0));
        if tema::botao_icone_em(ui, fechar, ui.id().with("recolher"), Icone::Anterior).on_hover_text("Recolher o painel").clicked() {
            self.aberto = false;
        }
        // O selo e o nome abrem todos os projetos do workspace.
        let titulo = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top() + 10.0), pos2(fechar.left() - 6.0, rect.top() + 50.0));
        let clicavel = grupo.id != 0;
        let resposta = ui.interact(titulo, ui.id().with("titulo"), if clicavel { Sense::click() } else { Sense::hover() });
        let ativo = clicavel && v.escopo == Escopo::Workspace(grupo.id) && v.lugar == Lugar::Quadro;
        if clicavel {
            fundo_do_item(ui, &resposta, titulo, ativo, resposta.hovered());
        }
        let selo = Rect::from_min_size(pos2(titulo.left() + 6.0, titulo.center().y - 12.0), vec2(24.0, 24.0));
        ui.painter().rect_filled(selo, CornerRadius::same(7), tema::cor_do_nome(grupo.nome));
        ui.painter().text(selo.center(), egui::Align2::CENTER_CENTER, inicial(grupo.nome), tema::forte(12.0), tema::sobre_destaque());
        let x = selo.right() + 10.0;
        let largura = titulo.right() - x - 6.0;
        let nome = tema::cortar(ui.painter(), grupo.nome, egui::TextFormat::simple(tema::forte(15.0), p.texto), largura, 1, false);
        let n = grupo.projetos.len();
        let quantos = if n == 1 { "1 projeto".to_string() } else { format!("{n} projetos") };
        let sub = match v.perfil {
            Some(perfil) => format!("{quantos} · {}", perfil.nome),
            None => quantos,
        };
        let sub = tema::cortar(ui.painter(), &sub, egui::TextFormat::simple(FontId::proportional(11.5), p.suave), largura, 1, false);
        let alto = nome.size().y + sub.size().y;
        let y = titulo.center().y - alto / 2.0;
        ui.painter().galley(pos2(x, y), nome.clone(), p.texto);
        ui.painter().galley(pos2(x, y + nome.size().y), sub, p.suave);
        if clicavel && resposta.on_hover_text(format!("Todos os projetos de {} (daily, sprint e quadro juntos)", grupo.nome)).clicked() {
            saida.acoes.push(Acao::Escopo(Escopo::Workspace(grupo.id)));
        }
    }

    fn miolo(&mut self, ui: &mut Ui, grupo: &Grupo, v: &Vista, saida: &mut Saida) {
        let p = cores();
        if !v.demo && grupo.id != 0 {
            let dica = format!("Lousa do workspace {}: notas, código, imagens e ligações", grupo.nome);
            if item(ui, "Lousa do workspace", Some(Icone::Lousa), v.lugar == Lugar::Lousa(grupo.id)).on_hover_text(dica).clicked() {
                saida.acoes.push(Acao::Lousa(grupo.id));
            }
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(r.x_range().shrink(6.0), r.center().y, Stroke::new(1.0, p.borda));
            ui.add_space(6.0);
        }
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        ui.painter().text(r.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, "PROJETOS E O QUE ESTÁ RODANDO", tema::forte(10.5), p.suave);

        // Barra de rolagem sólida e fina: aparece sempre que há mais embaixo
        // e não cobre as contagens à direita.
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        ui.spacing_mut().scroll.bar_width = 6.0;
        egui::ScrollArea::vertical().id_salt("painel-arvore").auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for projeto in &grupo.projetos {
                self.ramo_do_projeto(ui, projeto, v, saida);
            }
            if !v.demo && v.pode && item(ui, "+ Novo projeto", None, false).clicked() {
                saida.acoes.push(Acao::NovoProjeto((grupo.id != 0).then_some(grupo.id)));
            }
        });
    }

    fn ramo_do_projeto(&mut self, ui: &mut Ui, projeto: &Projeto, v: &Vista, saida: &mut Saida) {
        let tarefas: Vec<&Tarefa> = v.tarefas.iter().filter(|t| t.projeto_id == projeto.id).collect();
        let andamento: Vec<&Tarefa> = tarefas.iter().copied().filter(|t| em_andamento(t)).collect();
        let resto = resto_do_projeto(&tarefas);
        let tem_filhos = !andamento.is_empty() || resto.is_some();
        let aberto = tem_filhos && self.aberto_no(No::Projeto(projeto.id), !andamento.is_empty());
        let contagem = Contagem {
            espera: abelha::esperando(tarefas.iter().copied()),
            erro: tarefas.iter().filter(|t| t.erro.is_some()).count(),
            rodando: if v.rodando { tarefas.iter().flat_map(|t| &t.agentes).filter(|a| a.ativo && a.visual() == EstadoVisual::Trabalhando).count() } else { 0 },
        };
        let ativo = v.escopo == Escopo::Projeto(projeto.id) && v.lugar == Lugar::Quadro;
        let (linha, seta) = linha_do_projeto(ui, projeto, ativo, tem_filhos.then_some(aberto), &contagem);
        if seta.is_some_and(|s| s.clicked()) {
            self.trocar(No::Projeto(projeto.id));
        } else if linha.clicked() {
            saida.acoes.push(Acao::Escopo(Escopo::Projeto(projeto.id)));
        }
        if !v.demo {
            linha.context_menu(|ui| {
                ui.set_min_width(240.0);
                if tema::opcao_menu_com(ui, "Remover da Colmeia…", None, v.pode) {
                    saida.acoes.push(Acao::RemoverProjeto { id: projeto.id, nome: projeto.nome.clone() });
                    ui.close();
                }
            });
        }
        if !tem_filhos {
            return;
        }

        // A altura do ramo aberto: as tarefas, os agentes das abertas (que
        // também podem estar abrindo) e a linha do resto.
        let ctx = ui.ctx().clone();
        let aberturas: Vec<f32> = andamento
            .iter()
            .map(|t| {
                let aberta = !t.agentes.is_empty() && self.aberto_no(No::Tarefa(t.id), true);
                abertura_do_no(&ctx, No::Tarefa(t.id), aberta)
            })
            .collect();
        let altura = andamento.iter().zip(&aberturas).map(|(t, a)| ALTURA_TAREFA + a * t.agentes.len() as f32 * ALTURA_AGENTE).sum::<f32>()
            + if resto.is_some() { ALTURA_RESTO } else { 0.0 };
        let t = abertura_do_no(&ctx, No::Projeto(projeto.id), aberto);
        recolhivel(ui, t, altura, |ui| {
            for (tarefa, abertura) in andamento.iter().zip(&aberturas) {
                self.ramo_da_tarefa(ui, tarefa, *abertura, v, saida);
            }
            if let Some(texto) = &resto {
                let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), ALTURA_RESTO), Sense::click());
                let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Ver no quadro do projeto");
                let cor = tema::cor_animada(ui, resposta.id.with("cor"), resposta.hovered(), cores().suave, cores().texto);
                ui.painter().text(rect.left_center() + vec2(36.0, 0.0), egui::Align2::LEFT_CENTER, texto, FontId::proportional(11.5), cor);
                if resposta.clicked() {
                    saida.acoes.push(Acao::Escopo(Escopo::Projeto(projeto.id)));
                }
            }
        });
    }

    fn ramo_da_tarefa(&mut self, ui: &mut Ui, tarefa: &Tarefa, abertura: f32, v: &Vista, saida: &mut Saida) {
        let com_agentes = !tarefa.agentes.is_empty();
        let aberta = com_agentes && self.aberto_no(No::Tarefa(tarefa.id), true);
        let ativa = matches!(v.lugar, Lugar::Tarefa { id, .. } if id == tarefa.id);
        let (linha, seta) = linha_da_tarefa(ui, tarefa, ativa, com_agentes.then_some(aberta));
        if seta.is_some_and(|s| s.clicked()) {
            self.trocar(No::Tarefa(tarefa.id));
        } else if linha.clicked() {
            saida.acoes.push(Acao::AbrirTarefa { tarefa: tarefa.id, agente: None });
        }
        if !com_agentes {
            return;
        }
        recolhivel(ui, abertura, tarefa.agentes.len() as f32 * ALTURA_AGENTE, |ui| {
            for (i, agente) in tarefa.agentes.iter().enumerate() {
                let focado = matches!(v.lugar, Lugar::Tarefa { id, foco } if id == tarefa.id && foco == agente.id);
                if linha_do_agente(ui, agente, focado, i + 1 == tarefa.agentes.len()).clicked() {
                    saida.acoes.push(Acao::AbrirTarefa { tarefa: tarefa.id, agente: Some(agente.id) });
                }
            }
        });
    }
}

/// O workspace do que está na tela: o da tarefa, da lousa ou do escopo.
fn workspace_em_foco(v: &Vista) -> Option<i64> {
    let do_projeto = |id: i64| v.projetos.iter().find(|p| p.id == id).map(|p| p.workspace_id);
    match v.lugar {
        Lugar::Tarefa { id, .. } => v.tarefas.iter().find(|t| t.id == id).and_then(|t| do_projeto(t.projeto_id)),
        Lugar::Lousa(ws) => Some(ws),
        Lugar::Quadro | Lugar::Banco => match v.escopo {
            Escopo::Workspace(w) => Some(w),
            Escopo::Projeto(p) => do_projeto(p),
            Escopo::Perfil => None,
        },
    }
}

/// A tarefa entra na árvore enquanto anda: trabalhando, esperando você,
/// com agente rodando ou com erro por resolver.
fn em_andamento(t: &Tarefa) -> bool {
    matches!(t.coluna, Coluna::Trabalhando | Coluna::AguardandoVoce) || t.agentes.iter().any(|a| a.ativo) || (t.erro.is_some() && t.coluna != Coluna::Concluido)
}

/// O que ficou de fora da árvore: "+ 2 em revisão · 3 concluídas".
fn resto_do_projeto(tarefas: &[&Tarefa]) -> Option<String> {
    let fora = |coluna: Coluna| tarefas.iter().filter(|t| t.coluna == coluna && !em_andamento(t)).count();
    let partes: Vec<String> = [(fora(Coluna::Revisao), "em revisão", "em revisão"), (fora(Coluna::Concluido), "concluída", "concluídas")]
        .into_iter()
        .filter(|(n, _, _)| *n > 0)
        .map(|(n, um, varios)| format!("{n} {}", if n == 1 { um } else { varios }))
        .collect();
    (!partes.is_empty()).then(|| format!("+ {}", partes.join(" · ")))
}

/// O estado de uma tarefa no ponto da árvore: o mais urgente dos agentes.
fn estado_da_tarefa(t: &Tarefa) -> EstadoVisual {
    let estados = || t.agentes.iter().map(AgenteTela::visual);
    if t.erro.is_some() || estados().any(|e| e == EstadoVisual::Erro) {
        EstadoVisual::Erro
    } else if t.coluna == Coluna::AguardandoVoce || estados().any(EstadoVisual::espera_voce) {
        EstadoVisual::SuaVez
    } else if estados().any(|e| e == EstadoVisual::Trabalhando) {
        EstadoVisual::Trabalhando
    } else {
        EstadoVisual::Parado
    }
}

/// A letra do quadrado: a primeira letra ou número do nome.
fn inicial(nome: &str) -> String {
    nome.chars().find(|c| c.is_alphanumeric()).map_or_else(|| "·".to_string(), |c| c.to_uppercase().collect())
}

/// "Pede aprovação" vira "pede aprovação": na árvore o estado vem depois do nome.
fn minuscula(texto: &str) -> String {
    let mut letras = texto.chars();
    letras.next().map_or_else(String::new, |c| c.to_lowercase().chain(letras).collect())
}

fn abertura_do_no(ctx: &egui::Context, no: No, aberto: bool) -> f32 {
    ctx.animate_bool_with_time(Id::new(("lateral-no", no)), aberto, tema::TRANSICAO)
}

/// Um ramo que abre e fecha: ocupa `altura × t` e mostra só essa parte do conteúdo.
fn recolhivel(ui: &mut Ui, t: f32, altura: f32, conteudo: impl FnOnce(&mut Ui)) {
    if t <= 0.0 {
        return;
    }
    let topo = ui.cursor().min;
    let largura = ui.available_width();
    let visivel = Rect::from_min_size(topo, vec2(largura, altura * t));
    let mut filho = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(topo, vec2(largura, altura))).layout(Layout::top_down(Align::Min)));
    filho.shrink_clip_rect(visivel);
    filho.spacing_mut().item_spacing = vec2(0.0, 0.0);
    if t < 1.0 {
        filho.multiply_opacity(t);
    }
    conteudo(&mut filho);
    ui.allocate_rect(visivel, Sense::hover());
}

/// O brilho do destaque subindo do pé do trilho.
fn brilho_do_pe(pintor: &egui::Painter, rect: Rect) {
    let p = cores();
    let alto = 220.0_f32.min(rect.height());
    let mut malha = egui::Mesh::default();
    for (y, forca) in [(rect.bottom() - alto, 0.0), (rect.bottom() - alto * 0.45, 0.06), (rect.bottom(), 0.22)] {
        let cor = p.destaque.gamma_multiply(forca);
        malha.colored_vertex(pos2(rect.left(), y), cor);
        malha.colored_vertex(pos2(rect.right(), y), cor);
    }
    for i in [0, 2] {
        malha.add_triangle(i, i + 1, i + 2);
        malha.add_triangle(i + 1, i + 3, i + 2);
    }
    pintor.add(Shape::mesh(malha));
}

/// Fundo de um item: o realce no escolhido e mais leve com o mouse em cima,
/// entrando e saindo com a transição do tema.
fn fundo_do_item(ui: &Ui, resposta: &Response, rect: Rect, ativo: bool, em_cima: bool) {
    let realce = tema::realce_lateral(cores());
    let mouse = tema::transicao(ui, resposta.id.with("mouse"), em_cima);
    let escolhido = tema::transicao(ui, resposta.id.with("ativo"), ativo);
    let forca = escolhido.max(mouse * 0.6);
    if forca > 0.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(9), realce.gamma_multiply(forca));
    }
}

/// A dica só com o menu fechado (as duas juntas se cobrem).
fn dica_fora_do_menu(resposta: Response, dica: &str) -> Response {
    if egui::Popup::is_id_open(&resposta.ctx, egui::Popup::default_response_id(&resposta)) { resposta } else { resposta.on_hover_text(dica) }
}

/// O quadrado do workspace: a inicial na cor do nome. Escolhido, ele se
/// enche da cor e ganha a marca à esquerda; o contador âmbar diz quantos
/// esperam você ali.
fn quadrado_do_workspace(ui: &mut Ui, g: &Grupo, ativo: bool, espera: usize) -> Response {
    let p = cores();
    let (linha, _) = ui.allocate_exact_size(vec2(TRILHO, 40.0), Sense::hover());
    let id = ui.id().with(("workspace", g.id, g.nome));
    let resposta = ui.interact(Rect::from_center_size(linha.center(), vec2(40.0, 40.0)), id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let mouse = tema::transicao(ui, id.with("mouse"), resposta.hovered());
    let escolhido = tema::transicao(ui, id.with("ativo"), ativo);
    let rect = Rect::from_center_size(linha.center() - vec2(0.0, mouse), vec2(40.0, 40.0));
    let cor = tema::cor_do_nome(g.nome);
    let raio = egui::lerp(egui::lerp(14.0..=11.0, escolhido)..=10.0, mouse);
    let fundo = tema::misturar(p.trilho, cor, 0.16).lerp_to_gamma(cor, escolhido);
    let borda = tema::misturar(p.trilho, cor, 0.3).lerp_to_gamma(cor, escolhido);
    let pintor = ui.painter();
    if escolhido > 0.0 {
        let sombra = egui::Shadow { offset: [0, 6], blur: 14, spread: 0, color: cor.gamma_multiply(0.35 * escolhido) };
        pintor.add(sombra.as_shape(rect, CornerRadius::same(raio as u8)));
    }
    pintor.rect(rect, CornerRadius::same(raio as u8), fundo, Stroke::new(1.0, borda), egui::StrokeKind::Inside);
    pintor.text(rect.center(), egui::Align2::CENTER_CENTER, inicial(g.nome), tema::forte(15.0), cor.lerp_to_gamma(tema::sobre_destaque(), escolhido));
    let marca = (26.0 * escolhido).max(18.0 * mouse);
    if marca > 0.5 {
        let r = Rect::from_center_size(pos2(linha.left() + 2.0, linha.center().y), vec2(4.0, marca));
        pintor.rect_filled(r, CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, cor);
    }
    if espera > 0 {
        tema::contador(pintor, pos2(rect.right() + 5.0, rect.top() + 4.0), espera, Some(p.trilho));
    }
    let dica = match espera {
        0 => g.nome.to_string(),
        1 => format!("{} · 1 espera você", g.nome),
        n => format!("{} · {n} esperam você", g.nome),
    };
    resposta.on_hover_text(dica)
}

fn botao_novo_workspace(ui: &mut Ui) -> Response {
    let p = cores();
    let (linha, _) = ui.allocate_exact_size(vec2(TRILHO, 40.0), Sense::hover());
    let rect = Rect::from_center_size(linha.center(), vec2(40.0, 40.0));
    let resposta = ui.interact(rect, ui.id().with("novo-workspace"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let mouse = tema::transicao(ui, resposta.id.with("mouse"), resposta.hovered());
    if mouse > 0.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(14), tema::realce_lateral(p).gamma_multiply(mouse));
    }
    // O contorno arredondado tracejado: os cantos de dentro afastados pelo raio.
    let dentro = rect.shrink(13.0);
    let mut fechado = mascote::arredondar(&[dentro.left_top(), dentro.right_top(), dentro.right_bottom(), dentro.left_bottom()], 12.0);
    fechado.push(fechado[0]);
    for forma in Shape::dashed_line(&fechado, Stroke::new(1.5, p.borda), 4.0, 3.0) {
        ui.painter().add(forma);
    }
    tema::desenhar_icone(ui.painter(), rect.center(), Icone::MaisZoom, p.suave.lerp_to_gamma(p.texto, mouse));
    resposta.on_hover_text("Novo workspace (com um projeto)")
}

/// Botão de ícone do trilho, 40 × 40, com o contador âmbar no canto.
fn botao_do_trilho(ui: &mut Ui, icone: Icone, ativo: bool, selo: usize) -> Response {
    let p = cores();
    let (linha, _) = ui.allocate_exact_size(vec2(TRILHO, 40.0), Sense::hover());
    let rect = Rect::from_center_size(linha.center(), vec2(40.0, 40.0));
    let resposta = ui.interact(rect, ui.id().with(("trilho", icone as u8)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let mouse = tema::transicao(ui, resposta.id.with("mouse"), resposta.hovered());
    let escolhido = tema::transicao(ui, resposta.id.with("ativo"), ativo);
    let pintor = ui.painter();
    if escolhido > 0.0 {
        pintor.rect_filled(rect, CornerRadius::same(11), p.destaque.gamma_multiply(0.14 * escolhido));
    }
    if mouse > 0.0 {
        pintor.rect_filled(rect, CornerRadius::same(11), tema::realce_lateral(p).gamma_multiply(mouse));
    }
    let cor = p.suave.lerp_to_gamma(p.texto, mouse).lerp_to_gamma(p.destaque, escolhido);
    tema::desenhar_icone(pintor, rect.center(), icone, cor);
    if selo > 0 {
        tema::contador(pintor, pos2(rect.right() + 3.0, rect.top() + 5.0), selo, Some(p.trilho));
    }
    resposta
}

/// Item simples do painel (a lousa, o novo projeto).
fn item(ui: &mut Ui, texto: &str, icone: Option<Icone>, ativo: bool) -> Response {
    let p = cores();
    let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    fundo_do_item(ui, &resposta, rect, ativo, resposta.hovered());
    let cor = tema::cor_animada(ui, resposta.id.with("cor"), ativo || resposta.hovered(), p.suave, p.texto);
    let x = match icone {
        Some(i) => {
            tema::desenhar_icone(ui.painter(), rect.left_center() + vec2(19.0, 0.0), i, cor);
            36.0
        }
        None => 10.0,
    };
    let fonte = if ativo { tema::forte(13.5) } else { FontId::proportional(13.5) };
    ui.painter().text(rect.left_center() + vec2(x, 0.0), egui::Align2::LEFT_CENTER, texto, fonte, cor);
    resposta
}

/// A área de 20 px da seta que abre e fecha o ramo; devolvida à parte,
/// para o clique nela não abrir o item. Desenhada depois do fundo.
fn seta_do_ramo(ui: &mut Ui, linha: Rect, x: f32, aberta: bool, id: Id) -> Response {
    let area = Rect::from_min_size(pos2(linha.left() + x, linha.top()), vec2(20.0, linha.height()));
    let resposta = ui.interact(area, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    resposta.on_hover_text(if aberta { "Recolher" } else { "Mostrar o que está rodando" })
}

/// A seta desenhada (a fonte não tem ▸/▾): para a direita fechada, para baixo aberta.
fn desenhar_seta(ui: &Ui, seta: &Response, aberta: bool) {
    let p = cores();
    let cor = if seta.hovered() { p.texto } else { p.suave };
    let c = seta.rect.center();
    let pontas = if aberta {
        vec![c + vec2(-3.5, -2.0), c + vec2(3.5, -2.0), c + vec2(0.0, 2.5)]
    } else {
        vec![c + vec2(-2.0, -3.5), c + vec2(2.5, 0.0), c + vec2(-2.0, 3.5)]
    };
    ui.painter().add(Shape::convex_polygon(pontas, cor, Stroke::NONE));
}

struct Contagem {
    espera: usize,
    erro: usize,
    rodando: usize,
}

fn linha_do_projeto(ui: &mut Ui, projeto: &Projeto, ativo: bool, aberto: Option<bool>, contagem: &Contagem) -> (Response, Option<Response>) {
    let p = cores();
    let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), ALTURA_PROJETO), Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let seta = aberto.map(|a| seta_do_ramo(ui, rect, 4.0, a, resposta.id.with("seta")));
    let em_cima = resposta.hovered() || seta.as_ref().is_some_and(|s| s.hovered());
    fundo_do_item(ui, &resposta, rect, ativo, em_cima);
    if let (Some(s), Some(a)) = (&seta, aberto) {
        desenhar_seta(ui, s, a);
    }

    // À direita: quantos esperam você, quantos com erro e quantos agentes
    // trabalham; "parado" quando nada anda.
    let pintor = ui.painter();
    let mut direita = rect.right() - 10.0;
    let mut numero = |n: usize, cor: Color32, texto: Color32| {
        let g = pintor.layout_no_wrap(n.to_string(), tema::forte(11.5), texto);
        direita -= g.size().x;
        pintor.galley(pos2(direita, rect.center().y - g.size().y / 2.0), g, texto);
        pintor.circle_filled(pos2(direita - 7.0, rect.center().y), 3.5, cor);
        direita -= 18.0;
    };
    if contagem.rodando > 0 {
        numero(contagem.rodando, p.ok, p.suave);
    }
    if contagem.erro > 0 {
        numero(contagem.erro, p.erro, p.erro);
    }
    if contagem.espera > 0 {
        numero(contagem.espera, p.alerta, p.alerta);
    }
    if contagem.rodando + contagem.erro + contagem.espera == 0 {
        let g = pintor.layout_no_wrap("parado".into(), FontId::proportional(11.5), p.suave);
        direita -= g.size().x;
        pintor.galley(pos2(direita, rect.center().y - g.size().y / 2.0), g, p.suave);
        direita -= 8.0;
    }

    let forte = ativo || aberto == Some(true);
    let fonte = if forte { tema::forte(14.0) } else { FontId::proportional(14.0) };
    let cor = tema::cor_animada(ui, resposta.id.with("cor"), forte || em_cima, p.suave, p.texto);
    let x = rect.left() + 26.0;
    let nome = tema::cortar(ui.painter(), &projeto.nome, egui::TextFormat::simple(fonte, cor), (direita - x).max(24.0), 1, false);
    ui.painter().galley(pos2(x, rect.center().y - nome.size().y / 2.0), nome, cor);
    let resposta = if projeto.caminho.is_empty() { resposta } else { resposta.on_hover_text(&projeto.caminho) };
    (resposta, seta)
}

fn linha_da_tarefa(ui: &mut Ui, tarefa: &Tarefa, ativa: bool, aberta: Option<bool>) -> (Response, Option<Response>) {
    let p = cores();
    let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), ALTURA_TAREFA), Sense::click());
    let rect = rect.with_min_x(rect.left() + 10.0);
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let seta = aberta.map(|a| seta_do_ramo(ui, rect, 0.0, a, resposta.id.with("seta")));
    let em_cima = resposta.hovered() || seta.as_ref().is_some_and(|s| s.hovered());
    fundo_do_item(ui, &resposta, rect, ativa, em_cima);
    if let (Some(s), Some(a)) = (&seta, aberta) {
        desenhar_seta(ui, s, a);
    }
    let estado = estado_da_tarefa(tarefa);
    tema::ponto(ui.painter(), pos2(rect.left() + 26.0, rect.center().y), 4.0, estado);
    let numero = ui.painter().layout_no_wrap(format!("#{}", tarefa.id), FontId::proportional(11.5), p.suave);
    let x = rect.left() + 36.0;
    let largura = rect.right() - 10.0 - numero.size().x - 8.0 - x;
    let titulo = tema::cortar(ui.painter(), &tarefa.titulo, egui::TextFormat::simple(FontId::proportional(13.5), p.texto), largura.max(24.0), 1, false);
    ui.painter().galley(pos2(x, rect.center().y - titulo.size().y / 2.0), titulo, p.texto);
    ui.painter().galley(pos2(rect.right() - 10.0 - numero.size().x, rect.center().y - numero.size().y / 2.0), numero, p.suave);
    (resposta.on_hover_text(&tarefa.titulo), seta)
}

/// O agente na árvore: o cotovelo vindo da guia da tarefa e a pílula com o
/// ponto de estado, o nome, o papel e o estado.
fn linha_do_agente(ui: &mut Ui, agente: &AgenteTela, focado: bool, ultimo: bool) -> Response {
    let p = cores();
    let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), ALTURA_AGENTE), Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let pintor = ui.painter();
    let guia = rect.left() + 30.0;
    let meio = rect.center().y;
    let traco = Stroke::new(1.0, p.borda);
    let raio = 6.0;
    let mut cotovelo = vec![pos2(guia, rect.top())];
    cotovelo.extend((0..=6).map(|i| {
        let a = PI - FRAC_PI_2 * i as f32 / 6.0;
        pos2(guia + raio + raio * a.cos(), meio - raio + raio * a.sin())
    }));
    cotovelo.push(pos2(guia + 10.0, meio));
    pintor.add(Shape::line(cotovelo, traco));
    if !ultimo {
        pintor.line_segment([pos2(guia, rect.top()), pos2(guia, rect.bottom())], traco);
    }

    let pilula = Rect::from_min_max(pos2(guia + 14.0, rect.top() + 1.0), pos2(rect.right(), rect.bottom() - 1.0));
    let mouse = tema::transicao(ui, resposta.id.with("mouse"), resposta.hovered());
    let escolhido = tema::transicao(ui, resposta.id.with("ativo"), focado);
    let redondo = CornerRadius::same((pilula.height() / 2.0) as u8);
    if mouse > 0.0 {
        pintor.rect_filled(pilula, redondo, tema::realce_lateral(p).gamma_multiply(mouse * 0.6));
    }
    if escolhido > 0.0 {
        let fundo = p.destaque.gamma_multiply(0.15 * escolhido);
        pintor.rect(pilula, redondo, fundo, Stroke::new(1.0, p.destaque.gamma_multiply(0.5 * escolhido)), egui::StrokeKind::Inside);
    }
    let estado = agente.visual();
    tema::ponto(pintor, pos2(pilula.left() + 12.0, meio), 3.5, estado);
    let cor_nome = if estado.cheio() { p.texto } else { p.suave };
    let mut x = pilula.left() + 22.0;
    let limite = pilula.right() - 10.0;
    let nome = tema::cortar(pintor, &agente.nome(), egui::TextFormat::simple(FontId::proportional(13.0), cor_nome), (limite - x).max(24.0), 1, false);
    x += nome.size().x;
    pintor.galley(pos2(pilula.left() + 22.0, meio - nome.size().y / 2.0), nome, cor_nome);
    let papel = pintor.layout_no_wrap(agente.papel.clone(), FontId::proportional(12.0), p.suave);
    if x + 6.0 + papel.size().x <= limite {
        pintor.galley(pos2(x + 6.0, meio - papel.size().y / 2.0), papel.clone(), p.suave);
        x += 6.0 + papel.size().x;
    }
    let cor_estado = if estado.cheio() { estado.cor() } else { p.suave };
    let livre = limite - x - 10.0;
    if livre > 24.0 {
        let texto = minuscula(&agente.estado_curto().0);
        let g = tema::cortar(pintor, &texto, egui::TextFormat::simple(FontId::proportional(11.5), cor_estado), livre, 1, false);
        pintor.galley(pos2(limite - g.size().x, meio - g.size().y / 2.0), g, cor_estado);
    }
    resposta.on_hover_text(format!("{} · {}", agente.nome_com_papel(), agente.texto_estado()))
}

/// O pé do painel: a abelha com o estado e a linha de resumo, e embaixo as
/// três barras do uso.
fn bloco_do_uso(ui: &mut Ui, rect: Rect, abelha: &mut Abelha, uso: &Uso, v: &Vista, saida: &mut Saida) {
    let p = cores();
    let pintor = ui.painter();
    pintor.rect(rect, CornerRadius::same(14), p.superficie_alta, Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
    pintor.hline((rect.left() + 14.0)..=(rect.right() - 14.0), rect.top() + 1.5, Stroke::new(1.0, p.luz));

    let linha = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(rect.width() - 8.0, 48.0));
    let em_cima = ui.rect_contains_pointer(linha) || abelha.resumo_aberto;
    let t = tema::transicao(ui, Id::new("abelha-painel-fundo"), em_cima);
    if t > 0.0 {
        ui.painter().rect_filled(linha, CornerRadius::same(11), p.realce.gamma_multiply(t));
    }
    let caixa = Rect::from_min_size(pos2(linha.left() + 4.0, linha.top() + 4.0), vec2(40.0, 40.0));
    let resposta = abelha.mostrar(ui, linha, caixa, Id::new("abelha-painel"), v.agora);
    if resposta.clicked() {
        saida.acoes.push(Acao::Abelha);
    }
    if resposta.secondary_clicked() {
        saida.acoes.push(Acao::ResumoDaAbelha);
    }
    if v.microfone {
        ponto_do_microfone(ui, caixa);
    }
    saida.abelha = Some(linha);
    let x = caixa.right() + 8.0;
    let largura = linha.right() - 8.0 - x;
    let titulo = format!("{} · {}", abelha.nome, abelha.nome_do_estado());
    let estado = tema::cortar(ui.painter(), &titulo, egui::TextFormat::simple(tema::forte(12.5), p.texto), largura, 1, false);
    let resumo = tema::cortar(ui.painter(), v.linha, egui::TextFormat::simple(FontId::proportional(11.0), p.suave), largura, 1, false);
    let y = linha.center().y - (estado.size().y + resumo.size().y) / 2.0;
    ui.painter().galley(pos2(x, y), estado.clone(), p.texto);
    ui.painter().galley(pos2(x, y + estado.size().y), resumo, p.suave);

    barras_de_uso(ui, Rect::from_min_max(pos2(rect.left() + 10.0, linha.bottom() + 8.0), rect.max - vec2(10.0, 10.0)), uso);
}

/// As três barras finas numa linha: contexto, 5 h e semana. Sem dados, o
/// trilho fica vazio, o número vira "—" e a dica diz que não há dados.
fn barras_de_uso(ui: &Ui, rect: Rect, uso: &Uso) {
    let p = cores();
    let vao = 10.0;
    let largura = (rect.width() - 2.0 * vao) / 3.0;
    for (i, (rotulo, medida, cor)) in
        [("Contexto", &uso.contexto, p.destaque), ("5 h", &uso.cinco_horas, p.ok), ("Semana", &uso.semana, p.alerta)].into_iter().enumerate()
    {
        let coluna = Rect::from_min_size(pos2(rect.left() + i as f32 * (largura + vao), rect.top()), vec2(largura, rect.height()));
        let pintor = ui.painter();
        let fonte = FontId::proportional(10.5);
        let g = pintor.layout_no_wrap(rotulo.to_string(), fonte, p.suave);
        pintor.galley(coluna.left_top(), g, p.suave);
        let (valor, cor_valor) = match medida {
            Some(m) => (format!("{:.0}%", m.fracao.clamp(0.0, 1.0) * 100.0), p.texto),
            None => ("—".to_string(), p.suave),
        };
        pintor.text(coluna.right_top(), egui::Align2::RIGHT_TOP, valor, tema::forte(10.5), cor_valor);
        let trilho = Rect::from_min_size(pos2(coluna.left(), coluna.bottom() - 4.0), vec2(coluna.width(), 4.0));
        pintor.rect_filled(trilho, CornerRadius::same(2), if medida.is_some() { p.realce } else { p.realce.gamma_multiply(0.6) });
        if let Some(m) = medida {
            let cheia = Rect::from_min_size(trilho.min, vec2(trilho.width() * m.fracao.clamp(0.0, 1.0), trilho.height()));
            pintor.rect_filled(cheia, CornerRadius::same(2), cor);
        }
        let dica = match medida {
            Some(m) => format!("{rotulo} · {:.0}% · {}", m.fracao.clamp(0.0, 1.0) * 100.0, m.detalhe),
            None => format!("{rotulo}: sem dados por enquanto"),
        };
        ui.interact(coluna, Id::new(("uso", i)), Sense::hover()).on_hover_text(dica);
    }
}

/// O microfone aberto fica sempre à vista: um ponto vermelho com o
/// microfone no canto de cima da abelha.
fn ponto_do_microfone(ui: &Ui, caixa: Rect) {
    let c = caixa.right_top() + vec2(-6.0, 6.0);
    ui.painter().circle_filled(c, 7.5, cores().superficie_alta);
    ui.painter().circle_filled(c, 6.0, cores().erro);
    ui.painter().circle_filled(c, 2.2, egui::Color32::WHITE);
}

#[cfg(test)]
mod testes {
    use super::*;

    fn projeto(id: i64, workspace_id: i64, workspace: &str) -> Projeto {
        Projeto {
            id,
            nome: format!("projeto-{id}"),
            workspace_id,
            workspace: workspace.into(),
            caminho: String::new(),
            sem_git: false,
            branch_padrao: "main".into(),
        }
    }

    #[test]
    fn workspaces_saem_dos_projetos_na_ordem() {
        let projetos = [projeto(1, 2, "Clientes"), projeto(2, 2, "Clientes"), projeto(3, 5, "Meus projetos")];
        let g = grupos(&projetos);
        assert_eq!(g.iter().map(|g| (g.id, g.nome, g.projetos.len())).collect::<Vec<_>>(), vec![(2, "Clientes", 2), (5, "Meus projetos", 1)]);
    }

    #[test]
    fn inicial_pula_simbolos_e_fica_maiuscula() {
        assert_eq!(inicial("clientes"), "C");
        assert_eq!(inicial("#2 open source"), "2");
        assert_eq!(inicial("ótica"), "Ó");
        assert_eq!(inicial(""), "·");
    }

    #[test]
    fn resto_diz_revisao_e_concluidas() {
        let mut tarefas = crate::dados::gerar_demo(0);
        for t in &mut tarefas {
            t.agentes.iter_mut().for_each(|a| a.ativo = false);
        }
        tarefas[0].coluna = Coluna::Revisao;
        tarefas[1].coluna = Coluna::Concluido;
        tarefas[2].coluna = Coluna::Concluido;
        tarefas[3].coluna = Coluna::Backlog;
        let refs: Vec<&Tarefa> = tarefas.iter().take(4).collect();
        assert_eq!(resto_do_projeto(&refs).as_deref(), Some("+ 1 em revisão · 2 concluídas"));
        assert_eq!(resto_do_projeto(&refs[3..]), None);
    }

    #[test]
    fn uso_mostra_o_agente_em_foco_e_os_limites_que_nao_voltaram() {
        let mut dados = crate::dados::Uso::default();
        dados.contextos.insert(7, api::ContextoUso { usado: 84_500, tamanho: 1_000_000, porcentagem: 8.45 });
        dados.cinco_horas = Some(api::LimiteUso { porcentagem: 12.0, volta: 2_000, volta_texto: "volta às 14:30".into() });
        dados.semana = Some(api::LimiteUso { porcentagem: 30.0, volta: 500, volta_texto: "volta às 09:00 de 18/10".into() });
        let agente = AgenteTela::da_api(
            &serde_json::from_str(r#"{"id":7,"tarefa_id":10,"ferramenta":"claude","papel":"dev","ativo":true,"estado":"trabalhando"}"#).unwrap(),
        );
        let uso = Uso::montar(&dados, Some(&agente), 1_000);
        let contexto = uso.contexto.expect("contexto do agente em foco");
        assert!((contexto.fracao - 0.0845).abs() < 1e-6);
        assert_eq!(contexto.detalhe, "Claude Code (dev) · 84 mil de 1,0 mi tokens");
        assert_eq!(uso.cinco_horas.map(|m| m.detalhe).as_deref(), Some("volta às 14:30"));
        // A semana já voltou: sem dados até chegar uso novo.
        assert!(uso.semana.is_none());
        // Sem agente em foco (ou de outra ferramenta), o contexto fica sem dados.
        assert!(Uso::montar(&dados, None, 1_000).contexto.is_none());
        assert_eq!(milhares(950), "950");
    }

    #[test]
    fn medida_sem_dados_e_o_padrao() {
        let uso = Uso::default();
        assert!(uso.contexto.is_none() && uso.cinco_horas.is_none() && uso.semana.is_none());
        let m = Medida { fracao: 0.4, detalhe: "Claude Code (dev)".into() };
        assert_eq!(m.detalhe, "Claude Code (dev)");
    }
}
