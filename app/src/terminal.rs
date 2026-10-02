//! Um terminal de agente: a conexão com o núcleo roda numa thread própria,
//! o alacritty_terminal interpreta a saída e o egui desenha a grade.

use std::cell::Cell;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
    tamanho: (usize, usize),
    intervalo: Cell<u32>,
    /// O programa do terminal terminou (ou a conexão caiu).
    encerrado: Arc<AtomicBool>,
    /// Quando houve a última colagem de texto, para não tratar o mesmo Ctrl+V como imagem.
    colou_texto_em: Cell<f64>,
}

/// Ritmos de atualização pedidos ao núcleo, em milissegundos.
pub const TEMPO_REAL: u32 = 16;
pub const MINIATURA: u32 = 250;
pub const SO_CARTAO: u32 = 1000;

impl TerminalAgente {
    /// Liga a tela ao terminal do núcleo em `caminho` (o WebSocket de um agente
    /// ou, na demonstração, de um terminal de teste).
    pub fn conectar(caminho: String, ctx: egui::Context, bytes: Arc<AtomicU64>, intervalo: u32) -> Self {
        let tamanho = (80, 24);
        let term = Arc::new(Mutex::new(Term::new(Config { scrolling_history: 1000, ..Config::default() }, &TermSize::new(tamanho.0, tamanho.1), Ouvinte)));
        let (envio, recebimento) = mpsc::channel();
        let term_rede = term.clone();
        let encerrado = Arc::new(AtomicBool::new(false));
        let encerrado_rede = encerrado.clone();
        thread::spawn(move || {
            conexao(&caminho, intervalo, term_rede, recebimento, &ctx, bytes);
            encerrado_rede.store(true, Ordering::Relaxed);
            ctx.request_repaint();
        });
        Self { term, envio, tamanho, intervalo: Cell::new(intervalo), encerrado, colou_texto_em: Cell::new(0.0) }
    }

    pub fn encerrado(&self) -> bool {
        self.encerrado.load(Ordering::Relaxed)
    }

    /// Envia uma mensagem para o agente, como se tivesse sido digitada e seguida de Enter.
    pub fn enviar(&self, texto: &str) {
        let mut bytes = colagem(texto);
        bytes.push(b'\r');
        self.term.lock().unwrap().scroll_display(Scroll::Bottom);
        let _ = self.envio.send(ParaNucleo::Digitacao(bytes));
    }

    /// Muda o ritmo em que o núcleo envia a saída deste terminal.
    pub fn definir_intervalo(&self, ms: u32) {
        if self.intervalo.replace(ms) != ms {
            let _ = self.envio.send(ParaNucleo::Texto(format!(r#"{{"intervalo":{ms}}}"#)));
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

    pub fn mostrar(&mut self, ui: &mut egui::Ui, tamanho_fonte: f32) -> egui::Response {
        let (rect, resposta) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let pintor = ui.painter_at(rect);
        pintor.rect_filled(rect, 0.0, cores().terminal_fundo);

        let fonte = FontId::monospace(tamanho_fonte);
        let letra = pintor.layout_no_wrap("M".into(), fonte.clone(), cores().terminal_texto).size();
        let margem = 4.0;
        let colunas = (((rect.width() - 2.0 * margem) / letra.x) as usize).max(10);
        let linhas = (((rect.height() - 2.0 * margem) / letra.y) as usize).max(3);

        let mut term = self.term.lock().unwrap();
        if (colunas, linhas) != self.tamanho {
            self.tamanho = (colunas, linhas);
            term.resize(TermSize::new(colunas, linhas));
            let _ = self.envio.send(ParaNucleo::Texto(format!(r#"{{"cols":{colunas},"rows":{linhas}}}"#)));
        }

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
            let _ = self.envio.send(ParaNucleo::Digitacao(saida));
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

/// Roda até o programa do terminal terminar ou a conexão cair.
fn conexao(caminho: &str, intervalo: u32, term: Arc<Mutex<Term<Ouvinte>>>, recebimento: Receiver<ParaNucleo>, ctx: &egui::Context, bytes: Arc<AtomicU64>) {
    let mut socket = match canal::websocket(&format!("{caminho}?intervalo={intervalo}")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("terminal {caminho}: {e}");
            return;
        }
    };
    // Espera curta na leitura para também atender a digitação e o tamanho.
    let _ = socket.get_mut().set_read_timeout(Some(Duration::from_millis(5)));
    let mut interpretador: Processor = Processor::new();
    let mut desenhados = 0;
    loop {
        while let Ok(msg) = recebimento.try_recv() {
            let msg = match msg {
                ParaNucleo::Digitacao(b) => Message::binary(b),
                ParaNucleo::Texto(t) => Message::text(t),
            };
            if socket.send(msg).is_err() {
                return;
            }
        }
        match socket.read() {
            Ok(Message::Binary(dados)) => {
                bytes.fetch_add(dados.len() as u64, Ordering::Relaxed);
                interpretador.advance(&mut *term.lock().unwrap(), &dados);
                // Controle de fluxo: confirma ao núcleo o que já foi processado.
                desenhados += dados.len();
                if desenhados >= CONFIRMAR_A_CADA {
                    if socket.send(Message::text(format!(r#"{{"ack":{desenhados}}}"#))).is_err() {
                        return;
                    }
                    desenhados = 0;
                }
                ctx.request_repaint();
            }
            // O núcleo avisa quando o programa do terminal termina.
            Ok(Message::Text(t)) if t.contains(r#""fim":true"#) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
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
