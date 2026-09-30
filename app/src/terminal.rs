//! Um terminal de agente: a conexão com o núcleo roda numa thread própria,
//! o alacritty_terminal interpreta a saída e o egui desenha a grade.

use std::io::ErrorKind;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
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
}

/// Ritmos de atualização pedidos ao núcleo, em milissegundos.
pub const TEMPO_REAL: u32 = 16;
pub const MINIATURA: u32 = 250;
pub const SO_CARTAO: u32 = 1000;

impl TerminalAgente {
    pub fn conectar(id: usize, ctx: egui::Context, bytes: Arc<AtomicU64>, intervalo: u32) -> Self {
        let tamanho = (80, 24);
        let term = Arc::new(Mutex::new(Term::new(
            Config { scrolling_history: 1000, ..Config::default() },
            &TermSize::new(tamanho.0, tamanho.1),
            Ouvinte,
        )));
        let (envio, recebimento) = mpsc::channel();
        let term_rede = term.clone();
        thread::spawn(move || conexao(id, intervalo, term_rede, recebimento, ctx, bytes));
        Self { term, envio, tamanho, intervalo: Cell::new(intervalo) }
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
            self.ler_teclado(ui);
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

    fn ler_teclado(&self, ui: &egui::Ui) {
        let mut saida = Vec::new();
        ui.input(|i| {
            for evento in &i.events {
                match evento {
                    egui::Event::Text(t) => saida.extend_from_slice(t.as_bytes()),
                    egui::Event::Paste(t) => saida.extend_from_slice(&colagem(t)),
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        use egui::Key::*;
                        let seq: &[u8] = match key {
                            C if modifiers.ctrl => b"\x03",
                            D if modifiers.ctrl => b"\x04",
                            L if modifiers.ctrl => b"\x0c",
                            Enter => b"\r",
                            Backspace => b"\x7f",
                            Tab => b"\t",
                            Escape => b"\x1b",
                            ArrowUp => b"\x1b[A",
                            ArrowDown => b"\x1b[B",
                            ArrowRight => b"\x1b[C",
                            ArrowLeft => b"\x1b[D",
                            _ => b"",
                        };
                        saida.extend_from_slice(seq);
                    }
                    _ => {}
                }
            }
        });
        if !saida.is_empty() {
            self.term.lock().unwrap().scroll_display(Scroll::Bottom);
            let _ = self.envio.send(ParaNucleo::Digitacao(saida));
        }
    }
}

/// Texto colado ou enviado com várias linhas vai como "colagem" (bracketed paste):
/// o shell e o Claude Code recebem o bloco inteiro, sem executar linha a linha.
fn colagem(texto: &str) -> Vec<u8> {
    if texto.contains('\n') {
        [b"\x1b[200~".as_slice(), texto.as_bytes(), b"\x1b[201~"].concat()
    } else {
        texto.as_bytes().to_vec()
    }
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

fn conexao(
    id: usize,
    intervalo: u32,
    term: Arc<Mutex<Term<Ouvinte>>>,
    recebimento: Receiver<ParaNucleo>,
    ctx: egui::Context,
    bytes: Arc<AtomicU64>,
) {
    let mut socket = match canal::websocket(&format!("/v1/terminais/{id}?intervalo={intervalo}")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("terminal {id}: {e}");
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
    fn cores_indexadas_seguem_a_escala_do_terminal() {
        assert_eq!(cor_indexada(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(cor_indexada(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(cor_indexada(232), Color32::from_rgb(8, 8, 8));
    }
}
