//! O que a abelha desenha além dela mesma: o ponto ou o contador, o cartão
//! do resumo e o balão. Sempre escuros, com a borda roxa da marca: ficam por
//! cima de qualquer janela, em qualquer tema.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};

use crate::resumo::{Aviso, Contagem, Tipo};

pub const FUNDO: Color32 = Color32::from_rgba_premultiplied(0x16, 0x14, 0x1f, 0xf5);
const BORDA: Color32 = Color32::from_rgb(0x3a, 0x35, 0x50);
const DESTAQUE: Color32 = Color32::from_rgb(0xae, 0x8f, 0xf7);
const TEXTO: Color32 = Color32::from_rgb(0xec, 0xe9, 0xf5);
const SUAVE: Color32 = Color32::from_rgb(0xa2, 0x9d, 0xb5);
const ERRO: Color32 = Color32::from_rgb(0xf4, 0x70, 0x7c);
const ALERTA: Color32 = Color32::from_rgb(0xf1, 0xb6, 0x5f);
const OK: Color32 = Color32::from_rgb(0x6f, 0xd4, 0x9b);
const RAIO: u8 = 14;

pub const LARGURA_CARTAO: f32 = 320.0;

/// O balão de aviso: cabeçalho, título em até duas linhas, detalhe em até
/// duas e os botões de 32 px.
const LINHA_TITULO: f32 = 19.0;
const LINHA_DETALHE: f32 = 17.0;
const ALTURA_BOTAO: f32 = 32.0;

fn altura_com(titulo: usize, detalhe: Option<usize>) -> f32 {
    let detalhe = detalhe.map_or(0.0, |n| 4.0 + n as f32 * LINHA_DETALHE);
    14.0 + 22.0 + titulo as f32 * LINHA_TITULO + detalhe + 14.0 + ALTURA_BOTAO + 16.0
}

/// A altura do balão medindo o texto como ele vai ser desenhado.
pub fn altura_balao_medida(ctx: &egui::Context, aviso: &Aviso) -> f32 {
    let largura = LARGURA_CARTAO - 28.0 - 16.0;
    let medir = |texto: &str, fonte: FontId| {
        let mut job = egui::text::LayoutJob::simple(texto.trim().to_owned(), fonte, TEXTO, largura);
        job.wrap.max_rows = 2;
        ctx.fonts_mut(|f| f.layout_job(job)).rows.len().clamp(1, 2)
    };
    let titulo = medir(&aviso.titulo, FontId::proportional(14.0));
    let detalhe = (!aviso.detalhe.trim().is_empty()).then(|| medir(&aviso.detalhe, FontId::proportional(12.5)));
    altura_com(titulo, detalhe)
}

/// A altura do resumo: com ou sem a tarefa em destaque e a caixa para escrever à abelha.
pub fn altura_resumo(com_destaque: bool, com_caixa: bool) -> f32 {
    let base = if com_destaque { 196.0 } else { 150.0 };
    if com_caixa { base + ALTURA_CAIXA } else { base }
}

/// A caixa do resumo para escrever à abelha (com os comandos ou a pergunta livre).
const ALTURA_CAIXA: f32 = 40.0;

fn cor(tipo: Tipo) -> Color32 {
    match tipo {
        Tipo::Erro => ERRO,
        Tipo::Espera => ALERTA,
        Tipo::Concluiu => DESTAQUE,
    }
}

/// O vermelho dos selos: escuro o bastante para o número branco (contraste AA).
const ERRO_SELO: Color32 = Color32::from_rgb(0xc8, 0x34, 0x4a);
const TINTA_ESCURA: Color32 = Color32::from_rgb(0x1a, 0x17, 0x22);

/// O que o selo mostra: um "!" com um aviso só, o número com mais de um.
pub fn texto_do_selo(avisos: usize) -> Option<String> {
    match avisos {
        0 => None,
        1 => Some("!".into()),
        n if n > 99 => Some("99+".into()),
        n => Some(n.to_string()),
    }
}

/// Um indicador só, no canto de cima à direita da abelha (onde a abelha
/// "Aguardando você" punha o "!", que aqui ela não desenha): o selo vermelho
/// com erro, âmbar com quem espera você ("!" com um, o número com mais) e,
/// sem nada pedindo você, um ponto verde pequeno com agentes trabalhando.
pub fn indicador(pintor: &egui::Painter, abelha: Rect, c: Contagem) {
    let canto = abelha.right_top() + vec2(-15.0, 15.0);
    let anel = Color32::from_rgb(0x0d, 0x0c, 0x14);
    let avisos = c.com_erro + c.esperam;
    let Some(texto) = texto_do_selo(avisos) else {
        if c.trabalhando > 0 {
            pintor.circle_filled(canto, 7.5, anel);
            pintor.circle_filled(canto, 5.0, OK);
        }
        return;
    };
    let (fundo, letra) = if c.com_erro > 0 { (ERRO_SELO, Color32::WHITE) } else { (ALERTA, TINTA_ESCURA) };
    let fonte = FontId::new(12.5, egui::FontFamily::Proportional);
    let largura = (texto.chars().count() as f32 * 7.5 + 9.0).max(22.0);
    let r = Rect::from_center_size(canto, vec2(largura, 22.0));
    pintor.rect(r.expand(2.5), CornerRadius::same(14), anel, Stroke::NONE, StrokeKind::Outside);
    pintor.rect_filled(r, CornerRadius::same(11), fundo);
    pintor.text(r.center() + vec2(0.0, 0.5), Align2::CENTER_CENTER, texto, fonte, letra);
}

/// O anel suave em volta da abelha com o mouse em cima.
pub fn brilho(pintor: &egui::Painter, abelha: Rect, t: f32) {
    if t <= 0.0 {
        return;
    }
    let c = abelha.center();
    let r = abelha.width() * 0.46;
    pintor.circle_filled(c, r, DESTAQUE.gamma_multiply(0.16 * t));
    pintor.circle_stroke(c, r, Stroke::new(1.5, DESTAQUE.gamma_multiply(0.55 * t)));
}

fn cartao(pintor: &egui::Painter, rect: Rect, borda: Color32) {
    // Sombra curta embaixo, para descolar da janela de trás.
    pintor.rect_filled(rect.translate(vec2(0.0, 3.0)), CornerRadius::same(RAIO), Color32::from_black_alpha(60));
    pintor.rect(rect, CornerRadius::same(RAIO), FUNDO, Stroke::new(1.0, borda), StrokeKind::Inside);
}

fn botao(ui: &mut Ui, texto: &str, principal: bool) -> egui::Response {
    let (fundo, letra, borda) = if principal { (DESTAQUE, Color32::from_rgb(0x16, 0x12, 0x24), DESTAQUE) } else { (Color32::TRANSPARENT, TEXTO, BORDA) };
    let botao = egui::Button::new(egui::RichText::new(texto).color(letra).size(13.0))
        .fill(fundo)
        .stroke(Stroke::new(1.0, borda))
        .corner_radius(CornerRadius::same(16))
        .min_size(vec2(64.0, ALTURA_BOTAO));
    ui.add(botao).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// O "x" de fechar no canto do cartão (28 px de área para o clique).
fn fechar(ui: &mut Ui, canto: Pos2, id: egui::Id) -> bool {
    let rect = Rect::from_center_size(canto, vec2(28.0, 28.0));
    let r = ui.interact(rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let cor = if r.hovered() { TEXTO } else { SUAVE };
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
    }
    let d = 4.0;
    ui.painter().line_segment([rect.center() - vec2(d, d), rect.center() + vec2(d, d)], Stroke::new(1.5, cor));
    ui.painter().line_segment([rect.center() + vec2(-d, d), rect.center() + vec2(d, -d)], Stroke::new(1.5, cor));
    r.clicked()
}

/// Um texto numa linha, cortado com "…" se passar da largura.
fn linha(ui: &Ui, pos: Pos2, texto: &str, fonte: FontId, cor: Color32, largura: f32) {
    let mut job = egui::text::LayoutJob::simple_singleline(texto.to_owned(), fonte, cor);
    job.wrap = egui::text::TextWrapping::truncate_at_width(largura);
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos, g, cor);
}

pub enum Pedido {
    Fechar,
    /// A frase escrita no resumo foi mandada.
    Escrever,
    Abrir {
        tarefa: i64,
        agente: Option<i64>,
    },
    AbrirColmeia,
    Desafixar,
}

/// O resumo, fixo até fechar: o nome, a contagem, o que mais pede você, a
/// caixa para escrever à abelha (com `rascunho`) e os botões.
pub fn resumo(ui: &mut Ui, rect: Rect, nome: &str, ligada: bool, c: Contagem, destaque: Option<&Aviso>, rascunho: Option<&mut String>) -> Option<Pedido> {
    let mut pedido = None;
    cartao(ui.painter(), rect, BORDA);
    let dentro = rect.shrink(14.0);
    let largura = dentro.width();
    linha(ui, dentro.left_top(), nome, FontId::proportional(14.5), TEXTO, largura - 30.0);
    if fechar(ui, pos2(dentro.right() - 6.0, dentro.top() + 9.0), egui::Id::new("fechar-resumo")) {
        pedido = Some(Pedido::Fechar);
    }

    // "2 esperam você · 1 trabalhando · 0 com erro", cada número na cor dele.
    let y = dentro.top() + 30.0;
    if ligada {
        let partes = [
            (c.esperam, if c.esperam == 1 { "espera você" } else { "esperam você" }, ALERTA),
            (c.trabalhando, "trabalhando", OK),
            (c.com_erro, "com erro", ERRO),
        ];
        let mut job = egui::text::LayoutJob::default();
        for (i, (n, texto, cor)) in partes.into_iter().enumerate() {
            if i > 0 {
                job.append(" · ", 0.0, egui::TextFormat::simple(FontId::proportional(12.5), SUAVE));
            }
            job.append(&n.to_string(), 0.0, egui::TextFormat::simple(FontId::proportional(12.5), if n > 0 { cor } else { SUAVE }));
            job.append(&format!(" {texto}"), 0.0, egui::TextFormat::simple(FontId::proportional(12.5), SUAVE));
        }
        let g = ui.painter().layout_job(job);
        ui.painter().galley(pos2(dentro.left(), y), g, SUAVE);
    } else {
        linha(ui, pos2(dentro.left(), y), "Sem ligação com o núcleo", FontId::proportional(12.5), SUAVE, largura);
    }

    let mut topo = y + 28.0;
    if let Some(d) = destaque {
        let caixa = Rect::from_min_size(pos2(dentro.left(), topo), vec2(largura, 48.0));
        ui.painter().rect_filled(caixa, CornerRadius::same(10), cor(d.tipo).gamma_multiply(0.10));
        ui.painter().circle_filled(pos2(caixa.left() + 12.0, caixa.top() + 15.0), 4.0, cor(d.tipo));
        let acao = if d.tipo == Tipo::Erro { "Ver o erro" } else { "Abrir" };
        let largura_botao = 76.0;
        let texto = largura - 24.0 - largura_botao - 8.0;
        linha(ui, pos2(caixa.left() + 24.0, caixa.top() + 7.0), &d.titulo, FontId::proportional(13.0), TEXTO, texto);
        linha(ui, pos2(caixa.left() + 24.0, caixa.top() + 26.0), &d.detalhe, FontId::proportional(11.5), SUAVE, texto);
        let lugar = Rect::from_min_size(pos2(caixa.right() - largura_botao - 8.0, caixa.center().y - 14.0), vec2(largura_botao, 28.0));
        let clicou = ui.scope_builder(UiBuilder::new().max_rect(lugar), |ui| botao(ui, acao, false).clicked()).inner;
        if clicou {
            pedido = Some(Pedido::Abrir { tarefa: d.tarefa, agente: d.agente });
        }
        topo = caixa.bottom() + 12.0;
    } else {
        linha(ui, pos2(dentro.left(), topo + 2.0), "Nada precisa de você agora.", FontId::proportional(12.5), SUAVE, largura);
        topo += 26.0;
    }

    if let Some(texto) = rascunho {
        let caixa = Rect::from_min_size(pos2(dentro.left(), topo), vec2(largura, 30.0));
        if caixa_para_a_abelha(ui, caixa, nome, texto) {
            pedido = Some(Pedido::Escrever);
        }
        topo += ALTURA_CAIXA;
    }

    let rodape = Rect::from_min_max(pos2(dentro.left(), topo), pos2(dentro.right(), topo + ALTURA_BOTAO));
    ui.scope_builder(UiBuilder::new().max_rect(rodape).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        if botao(ui, "Abrir a Colmeia", true).clicked() {
            pedido = Some(Pedido::AbrirColmeia);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if botao(ui, "Desafixar", false).on_hover_text("A abelha sai da área de trabalho").clicked() {
                pedido = Some(Pedido::Desafixar);
            }
        });
    });
    pedido
}

/// Uma linha para escrever à abelha; Enter (ou a seta) manda.
fn caixa_para_a_abelha(ui: &mut Ui, caixa: Rect, nome: &str, texto: &mut String) -> bool {
    ui.painter().rect(caixa, CornerRadius::same(8), Color32::from_white_alpha(6), Stroke::new(1.0, BORDA), StrokeKind::Inside);
    let campo = Rect::from_min_max(caixa.min + vec2(10.0, 0.0), pos2(caixa.right() - 34.0, caixa.bottom()));
    let resposta = ui
        .scope_builder(UiBuilder::new().max_rect(campo).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.add(
                egui::TextEdit::singleline(texto)
                    .id(egui::Id::new("pergunta-abelha"))
                    .frame(egui::Frame::NONE)
                    .desired_width(campo.width())
                    .char_limit(500)
                    .text_color(TEXTO)
                    .font(FontId::proportional(12.5))
                    .hint_text(egui::RichText::new(format!("Pergunte ou peça à {nome}…")).color(SUAVE)),
            )
        })
        .inner;
    // Sem foco na janela, o teclado não chega: o clique na caixa pede o foco.
    if resposta.clicked() && !ui.input(|i| i.viewport().focused.unwrap_or(false)) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    let enter = resposta.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let seta = Rect::from_center_size(pos2(caixa.right() - 16.0, caixa.center().y), vec2(24.0, 24.0));
    let botao = ui.interact(seta, egui::Id::new("mandar-pergunta"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let cor = if botao.hovered() { TEXTO } else { DESTAQUE };
    let c = seta.center();
    ui.painter().line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(1.6, cor));
    ui.painter().line_segment([c + vec2(1.0, -4.0), c + vec2(5.0, 0.0)], Stroke::new(1.6, cor));
    ui.painter().line_segment([c + vec2(1.0, 4.0), c + vec2(5.0, 0.0)], Stroke::new(1.6, cor));
    (enter || botao.clicked()) && !texto.trim().is_empty()
}

/// Um texto em até duas linhas na largura, com "…" só no fim da segunda.
fn paragrafo(ui: &Ui, pos: Pos2, texto: &str, fonte: FontId, cor: Color32, largura: f32) -> f32 {
    let mut job = egui::text::LayoutJob::simple(texto.trim().to_owned(), fonte, cor, largura);
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    let g = ui.painter().layout_job(job);
    let altura = g.size().y;
    ui.painter().galley(pos, g, cor);
    altura
}

/// O balão de um aviso: some sozinho em 8 s (o fio embaixo mostra o tempo)
/// e fica parado com o mouse em cima. O nome da abelha é o cabeçalho; o
/// título e o detalhe quebram em até duas linhas.
pub fn balao(ui: &mut Ui, rect: Rect, nome: &str, aviso: &Aviso, resta: f32) -> Option<Pedido> {
    let mut pedido = None;
    cartao(ui.painter(), rect, DESTAQUE.gamma_multiply(0.6));
    let dentro = rect.shrink2(vec2(14.0, 14.0));
    let largura = dentro.width();
    linha(ui, dentro.left_top() + vec2(0.0, 1.0), nome, FontId::proportional(12.0), SUAVE, largura - 40.0);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 7.0), egui::Id::new("fechar-balao")) {
        pedido = Some(Pedido::Fechar);
    }
    let mut y = dentro.top() + 24.0;
    ui.painter().circle_filled(pos2(dentro.left() + 5.0, y + 9.5), 4.5, cor(aviso.tipo));
    y += paragrafo(ui, pos2(dentro.left() + 16.0, y), &aviso.titulo, FontId::proportional(14.0), TEXTO, largura - 16.0);
    if !aviso.detalhe.trim().is_empty() {
        y += 4.0 + paragrafo(ui, pos2(dentro.left() + 16.0, y + 4.0), &aviso.detalhe, FontId::proportional(12.5), SUAVE, largura - 16.0);
    }
    let topo = (y + 12.0).min(dentro.bottom() - ALTURA_BOTAO - 4.0);
    let botoes = Rect::from_min_size(pos2(dentro.left() + 16.0, topo), vec2(largura - 16.0, ALTURA_BOTAO));
    ui.scope_builder(UiBuilder::new().max_rect(botoes).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let acao = if aviso.tipo == Tipo::Erro { "Ver o erro" } else { "Abrir" };
        if botao(ui, acao, true).clicked() {
            pedido = Some(Pedido::Abrir { tarefa: aviso.tarefa, agente: aviso.agente });
        }
        if botao(ui, "Depois", false).clicked() {
            pedido = Some(Pedido::Fechar);
        }
    });
    let fio = Rect::from_min_size(pos2(rect.left() + 14.0, rect.bottom() - 5.0), vec2((rect.width() - 28.0) * resta.clamp(0.0, 1.0), 2.0));
    ui.painter().rect_filled(fio, CornerRadius::same(1), DESTAQUE.gamma_multiply(0.7));
    pedido
}

const ALTURA_BALAO_VOZ: f32 = 128.0;
/// A resposta da pergunta livre pode ser longa: o balão cresce até estas linhas.
const LINHAS_BALAO_VOZ: usize = 12;
const ALTURA_LINHA: f32 = 18.0;
/// Quantas letras cabem numa linha do balão, por alto (só para a altura).
const LETRAS_POR_LINHA: usize = 44;

/// O rodapé (letra menor) cabe mais letras por linha.
const LETRAS_POR_LINHA_RODAPE: usize = 52;
const ALTURA_LINHA_RODAPE: f32 = 15.0;

/// As linhas do texto, por alto: cada linha dele (a lista vem uma por item)
/// quebra a cada `letras`.
fn linhas_de(texto: &str, letras: usize) -> usize {
    texto.split('\n').map(|l| l.chars().count().div_ceil(letras).max(1)).sum()
}

fn linhas_da_resposta(texto: &str) -> usize {
    linhas_de(texto, LETRAS_POR_LINHA).clamp(2, LINHAS_BALAO_VOZ)
}

/// A altura do balão da voz: a resposta (ou a lista) em duas linhas ou mais
/// e, com a lista, o rodapé (no lugar do que foi ouvido) em uma ou mais.
pub fn altura_balao_voz(texto: &str, rodape: &str) -> f32 {
    let extra = if rodape.is_empty() { 0.0 } else { (linhas_de(rodape, LETRAS_POR_LINHA_RODAPE) - 1) as f32 * ALTURA_LINHA_RODAPE };
    ALTURA_BALAO_VOZ + (linhas_da_resposta(texto) - 2) as f32 * ALTURA_LINHA + extra
}

/// O que o balão da voz pede.
pub enum PedidoVoz {
    Fechar,
    Sim,
    Nao,
    Abrir(i64),
}

/// O que a abelha está fazendo com o comando falado, ou a resposta.
pub struct Fala<'a> {
    pub texto: &'a str,
    /// Embaixo da lista: o que acontece agora ou o que dá para dizer.
    pub rodape: &'a str,
    pub ouvido: &'a str,
    /// A frase foi escrita no resumo, não falada.
    pub escrito: bool,
    pub erro: bool,
    pub pergunta: bool,
    pub abrir: Option<i64>,
    pub microfone: bool,
}

/// O balão da voz: a resposta (ou "Ouvindo…"), o que foi ouvido e, se a
/// resposta pergunta, Sim e Não.
pub fn balao_voz(ui: &mut Ui, rect: Rect, nome: &str, f: &Fala) -> Option<PedidoVoz> {
    let mut pedido = None;
    cartao(ui.painter(), rect, if f.erro { ERRO.gamma_multiply(0.6) } else { DESTAQUE.gamma_multiply(0.6) });
    let dentro = rect.shrink2(vec2(13.0, 12.0));
    let largura = dentro.width();
    let mut x = dentro.left();
    if f.microfone {
        ui.painter().circle_filled(pos2(x + 5.0, dentro.top() + 9.0), 4.5, ERRO);
        x += 16.0;
    }
    linha(ui, pos2(x, dentro.top()), nome, FontId::proportional(13.0), SUAVE, largura - 40.0);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 9.0), egui::Id::new("fechar-voz")) {
        pedido = Some(PedidoVoz::Fechar);
    }
    // A resposta em duas linhas, ou mais (a da pergunta livre).
    let linhas = linhas_da_resposta(f.texto);
    let mut job = egui::text::LayoutJob::simple(f.texto.to_owned(), FontId::proportional(13.5), if f.erro { ERRO } else { TEXTO }, largura);
    job.wrap.max_rows = linhas;
    let g = ui.painter().layout_job(job);
    // O que foi dito vem logo abaixo da resposta (a altura do balão é estimada antes).
    let fim = dentro.top() + 22.0 + g.size().y.max(2.0 * ALTURA_LINHA);
    ui.painter().galley(pos2(dentro.left(), dentro.top() + 22.0), g, TEXTO);
    if !f.rodape.is_empty() {
        let job = egui::text::LayoutJob::simple(f.rodape.to_owned(), FontId::proportional(11.5), SUAVE, largura);
        let g = ui.painter().layout_job(job);
        ui.painter().galley(pos2(dentro.left(), fim + 4.0), g, SUAVE);
    } else if !f.ouvido.is_empty() {
        let quem = if f.escrito { "Você escreveu" } else { "Você disse" };
        let y = fim + 4.0;
        linha(ui, pos2(dentro.left(), y), &format!("{quem}: \u{201c}{}\u{201d}", f.ouvido), FontId::proportional(11.5), SUAVE, largura);
    }
    if f.pergunta || f.abrir.is_some() {
        let botoes = Rect::from_min_size(pos2(dentro.left(), dentro.bottom() - ALTURA_BOTAO), vec2(largura, ALTURA_BOTAO));
        ui.scope_builder(UiBuilder::new().max_rect(botoes).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            if f.pergunta {
                if botao(ui, "Sim", true).clicked() {
                    pedido = Some(PedidoVoz::Sim);
                }
                if botao(ui, "Não", false).clicked() {
                    pedido = Some(PedidoVoz::Nao);
                }
            } else if let Some(t) = f.abrir
                && botao(ui, "Abrir", true).clicked()
            {
                pedido = Some(PedidoVoz::Abrir(t));
            }
        });
    }
    pedido
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn selo_com_um_aviso_e_o_numero_com_mais() {
        assert_eq!(texto_do_selo(0), None);
        assert_eq!(texto_do_selo(1).as_deref(), Some("!"));
        assert_eq!(texto_do_selo(3).as_deref(), Some("3"));
        assert_eq!(texto_do_selo(140).as_deref(), Some("99+"));
    }

    #[test]
    fn balao_cresce_com_as_linhas_ate_duas() {
        assert!(altura_com(1, None) < altura_com(1, Some(1)));
        assert!(altura_com(1, Some(1)) < altura_com(2, Some(1)));
        assert!(altura_com(2, Some(1)) < altura_com(2, Some(2)));
        // Os botões (32 px) sempre cabem embaixo do texto.
        assert!(altura_com(1, None) >= 14.0 + 22.0 + LINHA_TITULO + 14.0 + ALTURA_BOTAO + 16.0);
    }
}
