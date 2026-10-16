//! A abelha fixa da Colmeia: um programa à parte, pequeno e sem moldura,
//! sempre por cima e em todas as áreas de trabalho (no X11), que continua
//! com a janela da Colmeia fechada. Fala com o núcleo pelo mesmo canal e
//! token da janela e só escuta os eventos do perfil: parada, mostra o ponto
//! da cor do que mais pede você; um clique abre o resumo, que fica até ser
//! fechado; os avisos saem dela em balões. Só redesenha quando algo muda.
//!
//! Uso: colmeia-abelha --perfil N (a janela abre com a opção "Fixar a
//! abelha na área de trabalho").

mod fixar;
mod ligacao;
mod posicao;
mod resumo;
mod tela;

#[cfg(any(unix, windows))]
use std::fs::File;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui::{self, Pos2, Rect, Sense, ViewportCommand, pos2, vec2};
use mascote::{Estado, Modelo, Tempo};

use ligacao::Mensagem;
use resumo::{Aviso, Efeito, Resumo};

/// O quadrado da abelha na tela (com folga para o ponto e o pulo).
const ABELHA: f32 = 76.0;
/// Entre a abelha e o cartão.
const VAO: f32 = 8.0;
/// Animação depois de uma troca de estado ou do mouse chegar; depois, parada.
const ANIMACAO: f64 = 2.5;
const COMEMORACAO: f64 = 3.0;
const BALAO: f64 = 8.0;
const QUADROS_ANIMACAO: f64 = 30.0;
/// O fio do balão não precisa de mais que isso para andar liso.
const QUADROS_BALAO: f64 = 10.0;

struct Balao {
    aviso: Aviso,
    resta: f64,
}

struct Fixa {
    perfil: i64,
    modelo: Modelo,
    nome: String,
    resumo: Resumo,
    recebe: Receiver<Mensagem>,
    ligada: bool,
    estado: Estado,
    mudou_em: f64,
    animar_ate: f64,
    comemorar_ate: f64,
    em_cima: bool,
    /// Começou a arrastar e espera o foco para pedir ao gerenciador.
    arrastar: bool,
    aberto: bool,
    balao: Option<Balao>,
    /// O canto de cima à esquerda da abelha, na tela.
    posicao: Option<Pos2>,
    /// A janela como foi pedida por último (na tela).
    janela: Option<Rect>,
    /// Depois de mover ou redimensionar, o gerenciador leva um tempo: até
    /// lá, uma posição diferente não é você arrastando.
    ajustando_ate: f64,
    ultimo_quadro: f64,
    quadros: u32,
    /// Sem compositor, a janela é opaca, no fundo escuro dos cartões.
    transparente: bool,
    /// O tamanho da tela quando o egui não sabe o do monitor (lido uma vez).
    tela: Option<egui::Vec2>,
    _trava: Trava,
}

/// Uma abelha só por perfil: a segunda vê a trava e sai.
/// Segura o arquivo aberto (e a trava) enquanto a abelha vive.
struct Trava {
    #[cfg(any(unix, windows))]
    _arquivo: File,
}

#[cfg(unix)]
fn travar(perfil: i64) -> Option<Trava> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    let caminho = canal::diretorio().join(format!("abelha-{perfil}.trava"));
    let arquivo = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(caminho).ok()?;
    // SAFETY: flock num descritor aberto e nosso; não mexe em memória.
    let ok = unsafe { libc::flock(arquivo.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    ok.then_some(Trava { _arquivo: arquivo })
}

/// No Windows, o arquivo aberto sem compartilhar já é a trava.
#[cfg(windows)]
fn travar(perfil: i64) -> Option<Trava> {
    use std::os::windows::fs::OpenOptionsExt;
    let caminho = canal::diretorio().join(format!("abelha-{perfil}.trava"));
    let arquivo = std::fs::OpenOptions::new().write(true).create(true).truncate(false).share_mode(0).open(caminho).ok()?;
    Some(Trava { _arquivo: arquivo })
}

#[cfg(not(any(unix, windows)))]
fn travar(_: i64) -> Option<Trava> {
    Some(Trava {})
}

impl Fixa {
    fn vestir(&mut self, ctx: &egui::Context, modelo: &str, nome: &str) {
        self.modelo = Modelo::do_codigo(modelo);
        self.nome = if nome.trim().is_empty() { "Colmeia".into() } else { nome.to_string() };
        ctx.send_viewport_cmd(ViewportCommand::Title(self.nome.clone()));
        self.animar_ate = self.ultimo_quadro + ANIMACAO;
    }

    fn receber(&mut self, ctx: &egui::Context, agora: f64) {
        while let Ok(m) = self.recebe.try_recv() {
            match m {
                Mensagem::Perfil(Some(p)) => self.vestir(ctx, &p.mascote_modelo, &p.mascote_nome),
                // O perfil foi apagado: não há mais o que mostrar.
                Mensagem::Perfil(None) => ctx.send_viewport_cmd(ViewportCommand::Close),
                Mensagem::Quadro(q) => {
                    self.resumo.carregar(q);
                    self.ligada = true;
                }
                Mensagem::Fora => self.ligada = false,
                Mensagem::Evento(e) => {
                    for efeito in self.resumo.aplicar(e) {
                        match efeito {
                            // Com o resumo aberto, o que mudou já está nele.
                            Efeito::Avisar(aviso) if !self.aberto => self.balao = Some(Balao { aviso, resta: BALAO }),
                            Efeito::Avisar(_) | Efeito::Recarregar => {}
                            Efeito::Comemorar => self.comemorar_ate = agora + COMEMORACAO,
                            Efeito::Vestir { modelo, nome } => self.vestir(ctx, &modelo, &nome),
                            Efeito::Sair => ctx.send_viewport_cmd(ViewportCommand::Close),
                        }
                    }
                }
            }
        }
    }

    /// O cartão que aparece com a abelha (o resumo ou o balão) e a altura dele.
    fn cartao(&self) -> Option<f32> {
        if self.aberto { Some(tela::altura_resumo(self.resumo.destaque().is_some())) } else { self.balao.as_ref().map(|_| tela::ALTURA_BALAO) }
    }

    /// A janela na tela para a abelha em `p`, com o cartão (se houver) para
    /// o lado de dentro da tela, e onde a abelha e o cartão ficam nela.
    fn arranjo(p: Pos2, cartao: Option<f32>, monitor: egui::Vec2) -> (Rect, Rect, Option<Rect>) {
        let abelha = Rect::from_min_size(p, vec2(ABELHA, ABELHA));
        let Some(altura) = cartao else { return (abelha, Rect::from_min_size(Pos2::ZERO, abelha.size()), None) };
        let acima = abelha.center().y > monitor.y / 2.0;
        let direita = abelha.center().x > monitor.x / 2.0;
        let x = if direita { abelha.right() - tela::LARGURA_CARTAO } else { abelha.left() };
        let y = if acima { abelha.top() - VAO - altura } else { abelha.bottom() + VAO };
        let cartao = Rect::from_min_size(pos2(x, y), vec2(tela::LARGURA_CARTAO, altura));
        let janela = abelha.union(cartao);
        let local = |r: Rect| r.translate(-janela.min.to_vec2());
        (janela, local(abelha), Some(local(cartao)))
    }

    fn tempo(&self, agora: f64, animando: bool) -> Tempo {
        // Parada: um instante fixo depois da animação (no bugado, sem rajada forte).
        let mut t = if animando { agora } else { self.mudou_em + ANIMACAO };
        if !animando && self.estado == Estado::Bugado && t % 0.9 < 0.3 {
            t += 0.35 - t % 0.9;
        }
        Tempo { total: t as f32, no_estado: (t - self.mudou_em) as f32 }
    }

    fn atender(&mut self, ctx: &egui::Context, pedido: tela::Pedido) {
        match pedido {
            tela::Pedido::Fechar => {
                self.aberto = false;
                self.balao = None;
            }
            tela::Pedido::Abrir { tarefa, agente } => {
                self.resumo.visto(tarefa);
                self.balao = None;
                ligacao::abrir_colmeia(self.perfil, tarefa, agente);
            }
            tela::Pedido::AbrirColmeia => ligacao::abrir_colmeia(self.perfil, 0, None),
            tela::Pedido::Desafixar => {
                if !ligacao::desafixar(self.perfil) {
                    eprintln!("não consegui gravar o Desafixar no perfil; a abelha sai mesmo assim");
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for Fixa {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        if self.transparente { [0.0; 4] } else { egui::Rgba::from(tela::FUNDO).to_array() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let agora = ui.input(|i| i.time);
        let passou = agora - self.ultimo_quadro;
        self.ultimo_quadro = agora;
        self.quadros += 1;
        // O pedido ao gerenciador de janelas vai com a janela já na tela.
        if self.quadros == 1 {
            ctx.request_repaint();
        } else if self.quadros == 2
            && let Err(e) = fixar::em_todas_as_areas(frame)
        {
            eprintln!("abelha fixa: {e}");
        }
        self.receber(&ctx, agora);

        let estado = if agora < self.comemorar_ate { Estado::Comemorando } else { self.resumo.estado() };
        if estado != self.estado {
            self.estado = estado;
            self.mudou_em = agora;
            self.animar_ate = agora + ANIMACAO;
        }

        // Onde ela está: a lembrada, ou o canto de baixo à direita.
        let (monitor, fora) = ui.input(|i| (i.viewport().monitor_size, i.viewport().outer_rect));
        let monitor = match monitor.filter(|m| m.x >= 320.0 && m.y >= 240.0) {
            Some(m) => m,
            None => *self.tela.get_or_insert_with(|| {
                let (l, a) = fixar::tamanho_da_tela().unwrap_or((1920, 1080));
                vec2(l as f32, a as f32) / ctx.pixels_per_point()
            }),
        };
        let p = self.posicao.unwrap_or_else(|| pos2(monitor.x - ABELHA - 24.0, monitor.y - ABELHA - 64.0));
        let p = pos2(p.x.clamp(0.0, (monitor.x - ABELHA).max(0.0)), p.y.clamp(0.0, (monitor.y - ABELHA).max(0.0)));

        // Você arrastou (o gerenciador moveu a janela): a abelha vai junto e fica lembrada.
        let p = match (fora, self.janela) {
            (Some(f), Some(j)) if agora > self.ajustando_ate && (f.min - j.min).length() > 1.0 => {
                let nova = p + (f.min - j.min);
                self.janela = Some(j.translate(f.min - j.min));
                posicao::gravar(self.perfil, nova);
                nova
            }
            _ => p,
        };
        self.posicao = Some(p);

        let (janela, abelha, cartao) = Self::arranjo(p, self.cartao(), monitor);
        if self.janela != Some(janela) {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(janela.min));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(janela.size()));
            self.janela = Some(janela);
            self.ajustando_ate = agora + 0.6;
        }

        // A abelha: clique abre ou fecha o resumo; arrastar move a janela.
        let resposta = ui.interact(abelha, egui::Id::new("abelha"), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Grab);
        // O gerenciador só aceita arrastar a janela em foco: sem foco, pede
        // o foco e arrasta no quadro seguinte, com o botão ainda apertado.
        if resposta.drag_started() {
            self.arrastar = true;
        }
        if self.arrastar && resposta.dragged() {
            if ui.input(|i| i.viewport().focused.unwrap_or(false)) {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                self.arrastar = false;
            } else {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
        } else {
            self.arrastar = false;
        }
        if resposta.clicked() {
            self.aberto = !self.aberto;
            self.balao = None;
        }
        let em_cima = resposta.hovered();
        if em_cima && !self.em_cima {
            self.animar_ate = self.animar_ate.max(agora + ANIMACAO);
        }
        self.em_cima = em_cima;
        let brilho = ctx.animate_bool(egui::Id::new("brilho"), em_cima || self.aberto);
        let animando = agora < self.animar_ate || self.estado == Estado::Comemorando;
        let pintor = ui.painter();
        tela::brilho(pintor, abelha, brilho);
        let lado = ABELHA * 0.7;
        mascote::desenhar(pintor, abelha.center() + vec2(0.0, 2.0), lado, self.modelo, self.estado, self.tempo(agora, animando));
        tela::ponto(pintor, abelha, self.resumo.contagem());

        let mut pedido = None;
        if let Some(rect) = cartao {
            if self.aberto {
                let destaque = self.resumo.destaque();
                pedido = tela::resumo(ui, rect, &self.nome, self.ligada, self.resumo.contagem(), destaque.as_ref());
            } else if let Some(b) = &mut self.balao {
                // O tempo do balão só corre sem o mouse em cima.
                if !ui.rect_contains_pointer(rect) {
                    b.resta -= passou.min(0.25);
                }
                pedido = tela::balao(ui, rect, &self.nome, &b.aviso, (b.resta / BALAO) as f32);
                if b.resta <= 0.0 {
                    self.balao = None;
                }
            }
        }
        if let Some(p) = pedido {
            self.atender(&ctx, p);
        }

        // Só pede quadros enquanto algo se mexe; parada, dorme até um evento.
        if animando {
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / QUADROS_ANIMACAO));
        } else if self.balao.is_some() && !self.aberto {
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / QUADROS_BALAO));
        }
    }
}

fn perfil_da_linha() -> Option<i64> {
    let mut argumentos = std::env::args().skip(1);
    while let Some(a) = argumentos.next() {
        if a == "--perfil" {
            return argumentos.next()?.parse().ok().filter(|n: &i64| *n > 0);
        }
    }
    None
}

fn main() -> eframe::Result {
    let Some(perfil) = perfil_da_linha() else {
        eprintln!("uso: colmeia-abelha --perfil N");
        std::process::exit(2);
    };
    let Some(trava) = travar(perfil) else {
        // Já há uma abelha deste perfil na tela.
        return Ok(());
    };
    let posicao = posicao::ler(perfil);
    let transparente = fixar::tem_compositor();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Colmeia")
        .with_app_id("colmeia-abelha")
        .with_inner_size([ABELHA, ABELHA])
        .with_decorations(false)
        .with_transparent(transparente)
        .with_resizable(false)
        .with_taskbar(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);
    if let Some(p) = posicao {
        viewport = viewport.with_position(p);
    }
    let opcoes = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "colmeia-abelha",
        opcoes,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let (envio, recebe) = mpsc::channel();
            ligacao::iniciar(perfil, cc.egui_ctx.clone(), envio);
            Ok(Box::new(Fixa {
                perfil,
                modelo: Modelo::default(),
                nome: "Colmeia".into(),
                resumo: Resumo::default(),
                recebe,
                ligada: false,
                estado: Estado::Dormindo,
                mudou_em: 0.0,
                animar_ate: ANIMACAO,
                comemorar_ate: 0.0,
                em_cima: false,
                arrastar: false,
                aberto: false,
                balao: None,
                posicao,
                janela: None,
                ajustando_ate: 0.0,
                ultimo_quadro: 0.0,
                quadros: 0,
                tela: None,
                transparente,
                _trava: trava,
            }))
        }),
    )
}
