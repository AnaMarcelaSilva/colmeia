//! Um terminal de agente: a conexão com o núcleo roda numa thread própria,
//! o alacritty_terminal interpreta a saída e o egui desenha a grade.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};
use eframe::egui::{self, Color32, FontId, Sense, text::LayoutJob, text::TextFormat};
use tungstenite::Message;

use crate::canal;
use crate::tema::cores;

const CONFIRMAR_A_CADA: usize = 64 * 1024;
/// Por enquanto o terminal não precisa avisar nada à tela.
pub struct Ouvinte;
impl EventListener for Ouvinte {}

enum ParaNucleo {
    Digitacao(Vec<u8>),
    Texto(String),
}

pub struct TerminalAgente {
    term: Arc<Mutex<Term<Ouvinte>>>,
    envio: Sender<ParaNucleo>,
    /// Acorda a thread de rede, que dorme no poll, quando há algo para enviar.
    despertador: Despertador,
    tamanho: (usize, usize),
    intervalo: Cell<u32>,
    /// O programa do terminal terminou (ou a conexão caiu).
    encerrado: Arc<AtomicBool>,
    /// Quando houve a última colagem de texto, para não tratar o mesmo Ctrl+V como imagem.
    colou_texto_em: Cell<f64>,
    area: Cell<Option<egui::Rect>>,
}

/// Tamanho do terminal em foco (colunas << 16 | linhas), ou 0 enquanto nenhum
/// foi medido. Só o terminal em foco decide o tamanho do terminal no núcleo:
/// um programa como o Claude Code redesenha a cada mudança de tamanho, e um
/// desenho feito em outra largura fica embaralhado.
static TAMANHO_EM_FOCO: AtomicU32 = AtomicU32::new(0);

/// O tamanho em que um terminal novo deve nascer, se a tela já sabe.
pub fn tamanho_em_foco() -> Option<(u16, u16)> {
    match TAMANHO_EM_FOCO.load(Ordering::Relaxed) {
        0 => None,
        v => Some(((v >> 16) as u16, v as u16)),
    }
}

fn guardar_tamanho_em_foco(colunas: usize, linhas: usize) {
    TAMANHO_EM_FOCO.store(((colunas.min(u16::MAX as usize) as u32) << 16) | linhas.min(u16::MAX as usize) as u32, Ordering::Relaxed);
}

const MARGEM: f32 = 4.0;

/// Quantas colunas e linhas cabem em `rect` com a fonte do terminal.
fn grade(ui: &egui::Ui, rect: egui::Rect, tamanho_fonte: f32) -> (usize, usize, egui::Vec2) {
    let letra = ui.painter().layout_no_wrap("M".into(), FontId::monospace(tamanho_fonte), cores().terminal_texto).size();
    let colunas = (((rect.width() - 2.0 * MARGEM) / letra.x) as usize).max(10);
    let linhas = (((rect.height() - 2.0 * MARGEM) / letra.y) as usize).max(3);
    (colunas, linhas, letra)
}

/// Calcula o tamanho do terminal em foco antes de ele existir (a tarefa ainda
/// não tem agente), para o primeiro agente já nascer do tamanho certo.
pub fn estimar_em_foco(ui: &egui::Ui, rect: egui::Rect, tamanho_fonte: f32) {
    let (colunas, linhas, _) = grade(ui, rect, tamanho_fonte);
    guardar_tamanho_em_foco(colunas, linhas);
}

/// Ritmos de atualização pedidos ao núcleo, em milissegundos.
pub const TEMPO_REAL: u32 = 16;
pub const MINIATURA: u32 = 250;
pub const SO_CARTAO: u32 = 1000;

impl TerminalAgente {
    /// Liga a tela ao terminal do núcleo em `caminho` (o WebSocket de um agente
    /// ou, na demonstração, de um terminal de teste).
    pub fn conectar(caminho: String, ctx: egui::Context, bytes: Arc<AtomicU64>, intervalo: u32) -> Self {
        // Nasce no tamanho do terminal em foco, que a conexão também informa ao
        // núcleo: o histórico chega desenhado na largura certa.
        let tamanho = tamanho_em_foco().map_or((80, 24), |(c, l)| (c as usize, l as usize));
        let caminho = match tamanho_em_foco() {
            Some((c, l)) => format!("{caminho}?cols={c}&rows={l}"),
            None => format!("{caminho}?"),
        };
        let term = Arc::new(Mutex::new(Term::new(Config { scrolling_history: 1000, ..Config::default() }, &TermSize::new(tamanho.0, tamanho.1), Ouvinte)));
        let (envio, recebimento) = mpsc::channel();
        let term_rede = term.clone();
        let encerrado = Arc::new(AtomicBool::new(false));
        let encerrado_rede = encerrado.clone();
        let (despertador, alarme) = Despertador::novo();
        thread::spawn(move || {
            conexao(&caminho, intervalo, term_rede, recebimento, alarme, &ctx, bytes);
            encerrado_rede.store(true, Ordering::Relaxed);
            ctx.request_repaint();
        });
        Self { term, envio, despertador, tamanho, intervalo: Cell::new(intervalo), encerrado, colou_texto_em: Cell::new(0.0), area: Cell::new(None) }
    }

    fn mandar(&self, msg: ParaNucleo) {
        if self.envio.send(msg).is_ok() {
            self.despertador.acordar();
        }
    }

    /// O retângulo onde o terminal foi desenhado no último quadro (para a captura).
    pub fn area(&self) -> Option<egui::Rect> {
        self.area.get()
    }

    pub fn encerrado(&self) -> bool {
        self.encerrado.load(Ordering::Relaxed)
    }

    /// Envia uma mensagem para o agente, como se tivesse sido digitada e seguida de Enter.
    pub fn enviar(&self, texto: &str) {
        let mut bytes = colagem(texto);
        bytes.push(b'\r');
        self.term.lock().unwrap().scroll_display(Scroll::Bottom);
        self.mandar(ParaNucleo::Digitacao(bytes));
    }

    /// Muda o ritmo em que o núcleo envia a saída deste terminal.
    pub fn definir_intervalo(&self, ms: u32) {
        if self.intervalo.replace(ms) != ms {
            self.mandar(ParaNucleo::Texto(format!(r#"{{"intervalo":{ms}}}"#)));
        }
    }

    /// A linha onde o agente está escrevendo, para mostrar no cartão.
    pub fn ultima_linha(&self) -> String {
        let term = self.term.lock().unwrap();
        let grid = term.grid();
        let cursor = grid.cursor.point.line.0;
        for linha in [cursor, cursor - 1] {
            if linha < grid.topmost_line().0 {
                break;
            }
            let texto: String = (0..grid.columns()).map(|c| grid[Line(linha)][Column(c)].c).collect();
            let texto = texto.trim();
            if !texto.is_empty() {
                return texto.to_string();
            }
        }
        String::new()
    }

    /// Desenha o terminal. Só o terminal `em_foco` muda o tamanho do terminal
    /// no núcleo; uma miniatura mostra a mesma grade com letra menor, cortada.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, tamanho_fonte: f32, em_foco: bool) -> egui::Response {
        let (rect, resposta) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        self.area.set(Some(rect));
        let pintor = ui.painter_at(rect);
        pintor.rect_filled(rect, 0.0, cores().terminal_fundo);

        let fonte = FontId::monospace(tamanho_fonte);
        let margem = MARGEM;
        let (colunas, linhas, letra) = grade(ui, rect, tamanho_fonte);

        let mut term = self.term.lock().unwrap();
        if em_foco {
            guardar_tamanho_em_foco(colunas, linhas);
            if (colunas, linhas) != self.tamanho {
                self.tamanho = (colunas, linhas);
                term.resize(TermSize::new(colunas, linhas));
                self.mandar(ParaNucleo::Texto(format!(r#"{{"cols":{colunas},"rows":{linhas}}}"#)));
            }
        }
        let linhas = self.tamanho.1;

        if resposta.hovered() {
            let rolagem = ui.input(|i| i.smooth_scroll_delta.y);
            if rolagem.abs() >= 1.0 {
                term.scroll_display(Scroll::Delta((rolagem / letra.y).round() as i32));
            }
        }
        // O terminal só recebe o teclado depois de clicado, como qualquer campo de
        // texto; assim a caixa de mensagem do painel e o terminal convivem.
        if resposta.clicked() {
            ui.memory_mut(|m| m.request_focus(resposta.id));
        }
        let com_teclado = ui.memory(|m| m.has_focus(resposta.id));
        if com_teclado {
            // Tab, setas e Esc vão para o terminal em vez de mover o foco da tela.
            let filtro = egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true };
            ui.memory_mut(|m| m.set_focus_lock_filter(resposta.id, filtro));
            self.ler_teclado(ui, &mut term);
        }

        // Uma linha de texto por linha da tela, juntando trechos da mesma cor.
        let conteudo = term.renderable_content();
        let deslocamento = conteudo.display_offset as i32;
        let mut trabalho = LayoutJob::default();
        let mut trecho = String::new();
        let mut cor_trecho = cores().terminal_texto;
        let mut linha_atual = 0;
        let desenhar = |trabalho: &mut LayoutJob, linha: i32| {
            let galeria = pintor.layout_job(std::mem::take(trabalho));
            let pos = rect.min + egui::vec2(margem, margem + linha as f32 * letra.y);
            pintor.galley(pos, galeria, cores().terminal_texto);
        };
        for celula in conteudo.display_iter {
            let linha = celula.point.line.0 + deslocamento;
            if linha != linha_atual {
                fechar_trecho(&mut trabalho, &mut trecho, cor_trecho, &fonte);
                desenhar(&mut trabalho, linha_atual);
                linha_atual = linha;
            }
            if celula.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let cor = converter_cor(celula.fg, celula.flags);
            if cor != cor_trecho {
                fechar_trecho(&mut trabalho, &mut trecho, cor_trecho, &fonte);
                cor_trecho = cor;
            }
            trecho.push(celula.c);
        }
        fechar_trecho(&mut trabalho, &mut trecho, cor_trecho, &fonte);
        desenhar(&mut trabalho, linha_atual);

        let cursor = conteudo.cursor.point;
        let linha_cursor = cursor.line.0 + deslocamento;
        if (0..linhas as i32).contains(&linha_cursor) {
            let pos = rect.min + egui::vec2(margem + cursor.column.0 as f32 * letra.x, margem + linha_cursor as f32 * letra.y);
            let caixa = egui::Rect::from_min_size(pos, letra);
            let cor = cores().destaque;
            if com_teclado {
                pintor.rect_filled(caixa, 0.0, cor.gamma_multiply(0.6));
            } else {
                pintor.rect_stroke(caixa, 0.0, (1.0, cor), egui::StrokeKind::Inside);
            }
        }
        resposta
    }

    /// Lê o teclado da tela. Recebe o terminal já travado por quem chama, porque
    /// o Mutex não pode ser travado duas vezes pela mesma thread.
    fn ler_teclado(&self, ui: &egui::Ui, term: &mut Term<Ouvinte>) {
        let cursor_de_aplicacao = term.mode().contains(TermMode::APP_CURSOR);
        let mut saida = Vec::new();
        let mut rolar: Option<Scroll> = None;
        ui.input(|i| {
            // Alt + tecla chega como texto; no terminal vira ESC antes da letra.
            let alt = i.modifiers.alt && !i.modifiers.ctrl;
            for evento in &i.events {
                match evento {
                    egui::Event::Text(t) => {
                        if alt {
                            saida.push(0x1b);
                        }
                        saida.extend_from_slice(t.as_bytes());
                    }
                    egui::Event::Paste(t) => {
                        self.colou_texto_em.set(i.time);
                        saida.extend_from_slice(&colagem(t));
                    }
                    // O egui transforma Ctrl+C e Ctrl+X em copiar e recortar; no terminal
                    // eles são os de sempre (interromper, e o Ctrl+X dos editores).
                    egui::Event::Copy => saida.push(0x03),
                    egui::Event::Cut => saida.push(0x18),
                    // Ctrl+V sem texto na área de transferência (uma imagem, por exemplo):
                    // o egui não avisa a colagem, só a tecla solta. O Ctrl+V vai para o
                    // programa, e o Claude Code lê a imagem da área de transferência.
                    egui::Event::Key { key: egui::Key::V, pressed: false, modifiers, .. } if modifiers.command && i.time - self.colou_texto_em.get() > 0.5 => {
                        saida.push(0x16)
                    }
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        // Shift + PgUp/PgDn rolam o histórico da tela, como nos terminais comuns.
                        match key {
                            egui::Key::PageUp if modifiers.shift && !modifiers.ctrl => rolar = Some(Scroll::PageUp),
                            egui::Key::PageDown if modifiers.shift && !modifiers.ctrl => rolar = Some(Scroll::PageDown),
                            _ => {
                                if let Some(seq) = sequencia(*key, *modifiers, cursor_de_aplicacao) {
                                    saida.extend_from_slice(&seq);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
        if let Some(r) = rolar {
            term.scroll_display(r);
        }
        if !saida.is_empty() {
            term.scroll_display(Scroll::Bottom);
            self.mandar(ParaNucleo::Digitacao(saida));
        }
    }
}

/// O que uma tecla especial manda para o programa do terminal, como num xterm.
/// Letras e símbolos comuns chegam como texto e não passam por aqui.
fn sequencia(tecla: egui::Key, m: egui::Modifiers, cursor_de_aplicacao: bool) -> Option<Vec<u8>> {
    use egui::Key::*;
    let esc = |resto: &[u8]| [b"\x1b".as_slice(), resto].concat();
    // Parâmetro de modificadores do xterm: 1 + Shift + 2·Alt + 4·Ctrl.
    let parametro = 1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8;
    let com_alt = |b: Vec<u8>| if m.alt { esc(&b) } else { b };

    if m.ctrl {
        let nome = tecla.name();
        if nome.len() == 1 && nome.as_bytes()[0].is_ascii_uppercase() {
            // Ctrl+letra é o caractere de controle da letra (Ctrl+A = 1 ... Ctrl+Z = 26).
            return Some(com_alt(vec![nome.as_bytes()[0] - b'A' + 1]));
        }
        let controle = match tecla {
            Space => Some(0x00),
            OpenBracket => Some(0x1b),
            Backslash => Some(0x1c),
            CloseBracket => Some(0x1d),
            Slash | Minus => Some(0x1f),
            // Ctrl+Backspace apaga a palavra anterior, como Ctrl+W.
            Backspace => Some(0x17),
            _ => None,
        };
        if let Some(c) = controle {
            return Some(com_alt(vec![c]));
        }
    }

    let seta = |letra: u8| {
        if parametro > 1 {
            format!("\x1b[1;{parametro}{}", letra as char).into_bytes()
        } else if cursor_de_aplicacao {
            vec![0x1b, b'O', letra]
        } else {
            vec![0x1b, b'[', letra]
        }
    };
    let til = |n: u8| {
        if parametro > 1 { format!("\x1b[{n};{parametro}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() }
    };
    let funcao = |letra: u8| {
        if parametro > 1 { format!("\x1b[1;{parametro}{}", letra as char).into_bytes() } else { vec![0x1b, b'O', letra] }
    };

    let seq = match tecla {
        // Shift+Enter e Alt+Enter quebram a linha no Claude Code sem enviar.
        Enter if m.shift || m.alt => esc(b"\r"),
        Enter => b"\r".to_vec(),
        Tab if m.shift => b"\x1b[Z".to_vec(),
        Tab => b"\t".to_vec(),
        Backspace => com_alt(vec![0x7f]),
        Escape => vec![0x1b],
        ArrowUp => seta(b'A'),
        ArrowDown => seta(b'B'),
        ArrowRight => seta(b'C'),
        ArrowLeft => seta(b'D'),
        Home => seta(b'H'),
        End => seta(b'F'),
        Insert => til(2),
        Delete => til(3),
        PageUp => til(5),
        PageDown => til(6),
        F1 => funcao(b'P'),
        F2 => funcao(b'Q'),
        F3 => funcao(b'R'),
        F4 => funcao(b'S'),
        F5 => til(15),
        F6 => til(17),
        F7 => til(18),
        F8 => til(19),
        F9 => til(20),
        F10 => til(21),
        F11 => til(23),
        F12 => til(24),
        _ => return None,
    };
    Some(seq)
}

/// Texto colado ou enviado com várias linhas vai como "colagem" (bracketed paste):
/// o shell e o Claude Code recebem o bloco inteiro, sem executar linha a linha.
fn colagem(texto: &str) -> Vec<u8> {
    if texto.contains('\n') { [b"\x1b[200~".as_slice(), texto.as_bytes(), b"\x1b[201~"].concat() } else { texto.as_bytes().to_vec() }
}

fn fechar_trecho(trabalho: &mut LayoutJob, trecho: &mut String, cor: Color32, fonte: &FontId) {
    if !trecho.is_empty() {
        trabalho.append(trecho, 0.0, TextFormat::simple(fonte.clone(), cor));
        trecho.clear();
    }
}

fn converter_cor(cor: Color, flags: Flags) -> Color32 {
    let base = match cor {
        Color::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => cor_indexada(i),
        Color::Named(nome) => match nome as usize {
            n @ 0..=15 => cores().ansi[n],
            _ if nome == NamedColor::Background => cores().terminal_fundo,
            _ => cores().terminal_texto,
        },
    };
    if flags.contains(Flags::DIM) { base.gamma_multiply(0.6) } else { base }
}

fn cor_indexada(i: u8) -> Color32 {
    match i {
        0..=15 => cores().ansi[i as usize],
        16..=231 => {
            let n = i - 16;
            let nivel = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(nivel(n / 36), nivel((n / 6) % 6), nivel(n % 6))
        }
        _ => {
            let cinza = 8 + (i - 232) * 10;
            Color32::from_rgb(cinza, cinza, cinza)
        }
    }
}

/// Acorda a thread de rede. No Unix é um par de sockets: escrever um byte
/// faz o poll da thread voltar.
struct Despertador {
    #[cfg(unix)]
    escrita: Option<std::os::unix::net::UnixStream>,
}

/// O lado que a thread de rede espera.
#[cfg(unix)]
type Alarme = Option<std::os::unix::net::UnixStream>;
#[cfg(not(unix))]
type Alarme = ();

impl Despertador {
    #[cfg(unix)]
    fn novo() -> (Despertador, Alarme) {
        match std::os::unix::net::UnixStream::pair() {
            Ok((escrita, leitura)) => {
                let _ = escrita.set_nonblocking(true);
                let _ = leitura.set_nonblocking(true);
                (Despertador { escrita: Some(escrita) }, Some(leitura))
            }
            Err(_) => (Despertador { escrita: None }, None),
        }
    }

    #[cfg(not(unix))]
    fn novo() -> (Despertador, Alarme) {
        (Despertador {}, ())
    }

    fn acordar(&self) {
        #[cfg(unix)]
        if let Some(mut e) = self.escrita.as_ref() {
            use std::io::Write;
            // Cheio quer dizer que já há um aviso pendente: tanto faz.
            let _ = e.write(&[1]);
        }
    }
}

/// Roda até o programa do terminal terminar ou a conexão cair. A thread dorme
/// no poll(2) do socket e do despertador: sem saída e sem digitação, não
/// acorda nenhuma vez (antes ela acordava 200 vezes por segundo por terminal).
#[cfg(unix)]
fn conexao(
    caminho: &str,
    intervalo: u32,
    term: Arc<Mutex<Term<Ouvinte>>>,
    recebimento: Receiver<ParaNucleo>,
    alarme: Alarme,
    ctx: &egui::Context,
    bytes: Arc<AtomicU64>,
) {
    use std::io::{ErrorKind, Read};
    use std::os::fd::AsRawFd;

    let mut socket = match canal::websocket(&format!("{caminho}&intervalo={intervalo}")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("terminal {caminho}: {e}");
            return;
        }
    };
    let Some(mut alarme) = alarme else { return };
    if socket.get_mut().set_nonblocking(true).is_err() {
        return;
    }
    let fd_socket = socket.get_ref().as_raw_fd();
    let fd_alarme = alarme.as_raw_fd();
    let mut interpretador: Processor = Processor::new();
    let mut desenhados = 0;
    // Há mensagem na fila do tungstenite esperando o socket aceitar mais.
    let mut falta_enviar = false;
    let bloqueou = |e: &tungstenite::Error| matches!(e, tungstenite::Error::Io(e) if e.kind() == ErrorKind::WouldBlock);
    loop {
        let mut fds = [
            libc::pollfd { fd: fd_socket, events: libc::POLLIN | if falta_enviar { libc::POLLOUT } else { 0 }, revents: 0 },
            libc::pollfd { fd: fd_alarme, events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: `fds` é um vetor válido de dois pollfd durante a chamada.
        let pronto = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if pronto < 0 {
            if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if fds[1].revents != 0 {
            let mut lixo = [0u8; 64];
            while matches!(alarme.read(&mut lixo), Ok(n) if n > 0) {}
            while let Ok(msg) = recebimento.try_recv() {
                let msg = match msg {
                    ParaNucleo::Digitacao(b) => Message::binary(b),
                    ParaNucleo::Texto(t) => Message::text(t),
                };
                match socket.write(msg) {
                    Ok(()) => {}
                    Err(e) if bloqueou(&e) => {}
                    Err(_) => return,
                }
            }
            falta_enviar = true;
        }
        if falta_enviar {
            match socket.flush() {
                Ok(()) => falta_enviar = false,
                Err(e) if bloqueou(&e) => {}
                Err(_) => return,
            }
        }
        if fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) == 0 {
            continue;
        }
        // Lê tudo o que já chegou e desenha uma vez só.
        let mut chegou = false;
        loop {
            match socket.read() {
                Ok(Message::Binary(dados)) => {
                    bytes.fetch_add(dados.len() as u64, Ordering::Relaxed);
                    interpretador.advance(&mut *term.lock().unwrap(), &dados);
                    chegou = true;
                    // Controle de fluxo: confirma ao núcleo o que já foi processado.
                    desenhados += dados.len();
                    if desenhados >= CONFIRMAR_A_CADA {
                        match socket.write(Message::text(format!(r#"{{"ack":{desenhados}}}"#))) {
                            Ok(()) => {}
                            Err(e) if bloqueou(&e) => {}
                            Err(_) => return,
                        }
                        falta_enviar = true;
                        desenhados = 0;
                    }
                }
                // O núcleo avisa quando o programa do terminal termina.
                Ok(Message::Text(t)) if t.contains(r#""fim":true"#) => {
                    ctx.request_repaint();
                    return;
                }
                Ok(_) => {}
                Err(e) if bloqueou(&e) => break,
                Err(_) => {
                    ctx.request_repaint();
                    return;
                }
            }
        }
        if chegou {
            ctx.request_repaint();
        }
    }
}

#[cfg(not(unix))]
fn conexao(caminho: &str, _: u32, _: Arc<Mutex<Term<Ouvinte>>>, recebimento: Receiver<ParaNucleo>, _: Alarme, _: &egui::Context, _: Arc<AtomicU64>) {
    // No Windows o canal ainda não existe (veja canal.rs): o que a tela mandar é descartado.
    eprintln!("terminal {caminho}: canal local ainda não implementado nesta plataforma");
    for m in recebimento.try_iter() {
        let _ = match m {
            ParaNucleo::Digitacao(b) => b.len(),
            ParaNucleo::Texto(t) => t.len(),
        };
    }
}

/// Pede uma carga de teste ao núcleo (só existe com o núcleo em modo demonstração).
pub fn pedir_carga(modo: &'static str) {
    thread::spawn(move || {
        if let Err(e) = canal::pedir("POST", &format!("/v1/demo/carga?modo={modo}")) {
            eprintln!("carga {modo}: {e}");
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn uma_linha_vai_como_digitacao() {
        assert_eq!(colagem("git status"), b"git status");
    }

    #[test]
    fn varias_linhas_vao_como_um_bloco_colado() {
        // Sem o bracketed paste, cada linha seria executada separadamente.
        assert_eq!(colagem("linha um\nlinha dois"), b"\x1b[200~linha um\nlinha dois\x1b[201~");
    }

    #[test]
    fn teclas_especiais_seguem_o_xterm() {
        use egui::{Key, Modifiers};
        let nada = Modifiers::NONE;
        let ctrl = Modifiers { ctrl: true, command: true, ..nada };
        let shift = Modifiers { shift: true, ..nada };
        let alt = Modifiers { alt: true, ..nada };
        let s = |k, m| sequencia(k, m, false).unwrap();
        assert_eq!(s(Key::R, ctrl), vec![0x12]);
        assert_eq!(s(Key::A, ctrl), vec![0x01]);
        assert_eq!(s(Key::Tab, shift), b"\x1b[Z");
        assert_eq!(s(Key::Enter, shift), b"\x1b\r");
        assert_eq!(s(Key::ArrowRight, ctrl), b"\x1b[1;5C");
        assert_eq!(s(Key::ArrowUp, nada), b"\x1b[A");
        assert_eq!(sequencia(Key::ArrowUp, nada, true).unwrap(), b"\x1bOA");
        assert_eq!(s(Key::Delete, nada), b"\x1b[3~");
        assert_eq!(s(Key::PageUp, ctrl), b"\x1b[5;5~");
        assert_eq!(s(Key::F1, nada), b"\x1bOP");
        assert_eq!(s(Key::F12, nada), b"\x1b[24~");
        assert_eq!(s(Key::Backspace, alt), b"\x1b\x7f");
        // Letras sem Ctrl chegam como texto, não por aqui.
        assert!(sequencia(Key::A, nada, false).is_none());
    }

    #[test]
    fn cores_indexadas_seguem_a_escala_do_terminal() {
        assert_eq!(cor_indexada(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(cor_indexada(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(cor_indexada(232), Color32::from_rgb(8, 8, 8));
    }
}
