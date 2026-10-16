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
pub const ALTURA_BALAO: f32 = 112.0;

/// A altura do resumo: com ou sem a tarefa em destaque.
pub fn altura_resumo(com_destaque: bool) -> f32 {
    if com_destaque { 196.0 } else { 150.0 }
}

fn cor(tipo: Tipo) -> Color32 {
    match tipo {
        Tipo::Erro => ERRO,
        Tipo::Espera => ALERTA,
        Tipo::Concluiu => DESTAQUE,
    }
}

/// O ponto da cor do que mais pede você (vermelho com o número de erros,
/// âmbar espera resposta, verde trabalho), no canto de baixo da abelha.
pub fn ponto(pintor: &egui::Painter, abelha: Rect, c: Contagem) {
    let canto = abelha.right_bottom() - vec2(14.0, 14.0);
    let anel = Color32::from_rgb(0x0d, 0x0c, 0x14);
    if c.com_erro > 0 {
        let texto = c.com_erro.min(99).to_string();
        let largura = if texto.len() > 1 { 24.0 } else { 20.0 };
        let r = Rect::from_center_size(canto, vec2(largura, 20.0));
        pintor.rect(r.expand(2.5), CornerRadius::same(13), anel, Stroke::NONE, StrokeKind::Outside);
        pintor.rect_filled(r, CornerRadius::same(10), ERRO);
        pintor.text(r.center(), Align2::CENTER_CENTER, texto, FontId::proportional(11.5), Color32::WHITE);
        return;
    }
    let cor = if c.esperam > 0 {
        ALERTA
    } else if c.trabalhando > 0 {
        OK
    } else {
        return;
    };
    pintor.circle_filled(canto, 9.0, anel);
    pintor.circle_filled(canto, 6.5, cor);
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
    let botao = egui::Button::new(egui::RichText::new(texto).color(letra).size(12.5))
        .fill(fundo)
        .stroke(Stroke::new(1.0, borda))
        .corner_radius(CornerRadius::same(15))
        .min_size(vec2(0.0, 28.0));
    ui.add(botao).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// O "x" de fechar no canto do cartão.
fn fechar(ui: &mut Ui, canto: Pos2, id: egui::Id) -> bool {
    let rect = Rect::from_center_size(canto, vec2(22.0, 22.0));
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
    Abrir { tarefa: i64, agente: Option<i64> },
    AbrirColmeia,
    Desafixar,
}

/// O resumo, fixo até fechar: o nome, a contagem, o que mais pede você e os botões.
pub fn resumo(ui: &mut Ui, rect: Rect, nome: &str, ligada: bool, c: Contagem, destaque: Option<&Aviso>) -> Option<Pedido> {
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

    let rodape = Rect::from_min_max(pos2(dentro.left(), topo), pos2(dentro.right(), topo + 30.0));
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

/// O balão de um aviso: some sozinho em 8 s (o fio embaixo mostra o tempo)
/// e fica parado com o mouse em cima.
pub fn balao(ui: &mut Ui, rect: Rect, nome: &str, aviso: &Aviso, resta: f32) -> Option<Pedido> {
    let mut pedido = None;
    cartao(ui.painter(), rect, DESTAQUE.gamma_multiply(0.6));
    let dentro = rect.shrink2(vec2(13.0, 12.0));
    let largura = dentro.width();
    ui.painter().circle_filled(pos2(dentro.left() + 5.0, dentro.top() + 9.0), 4.5, cor(aviso.tipo));
    linha(ui, pos2(dentro.left() + 16.0, dentro.top()), &aviso.titulo, FontId::proportional(13.5), TEXTO, largura - 16.0 - 30.0 - 50.0);
    ui.painter().text(pos2(dentro.right() - 30.0, dentro.top() + 9.0), Align2::RIGHT_CENTER, nome, FontId::proportional(11.0), SUAVE);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 9.0), egui::Id::new("fechar-balao")) {
        pedido = Some(Pedido::Fechar);
    }
    linha(ui, pos2(dentro.left() + 16.0, dentro.top() + 22.0), &aviso.detalhe, FontId::proportional(12.0), SUAVE, largura - 16.0);
    let botoes = Rect::from_min_size(pos2(dentro.left() + 16.0, dentro.top() + 48.0), vec2(largura - 16.0, 30.0));
    ui.scope_builder(UiBuilder::new().max_rect(botoes).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        let acao = if aviso.tipo == Tipo::Erro { "Ver o erro" } else { "Abrir" };
        if botao(ui, acao, true).clicked() {
            pedido = Some(Pedido::Abrir { tarefa: aviso.tarefa, agente: aviso.agente });
        }
        if botao(ui, "Depois", false).clicked() {
            pedido = Some(Pedido::Fechar);
        }
    });
    let fio = Rect::from_min_size(pos2(rect.left() + 14.0, rect.bottom() - 4.0), vec2((rect.width() - 28.0) * resta.clamp(0.0, 1.0), 2.0));
    ui.painter().rect_filled(fio, CornerRadius::same(1), DESTAQUE.gamma_multiply(0.7));
    pedido
}
