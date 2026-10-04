//! Aparência da Colmeia: paletas clara e escura, fontes, estilo dos controles,
//! os componentes de filtro (chip e seletor segmentado) e o fundo em favo.
//!
//! As decisões seguem a seção "Linguagem visual" do documento: poucos tons
//! neutros, separação por claridade em vez de linhas, três tamanhos de canto
//! e uma única cor de destaque.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Mesh, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, TextStyle, Theme,
    ThemePreference, Visuals, pos2, vec2,
};

/// Cantos: etiquetas, controles, cartões, superfícies (colunas, painéis) e
/// janelas. Uma pílula usa metade da altura. As peças novas usam só estes.
pub const RAIO_ETIQUETA: u8 = 6;
pub const RAIO_CONTROLE: u8 = 8;
pub const RAIO_CARTAO: u8 = 10;
pub const RAIO_SUPERFICIE: u8 = 12;
pub const RAIO_JANELA: u8 = 16;

pub struct Paleta {
    pub fundo: Color32,
    pub lateral: Color32,
    /// Colunas do quadro e áreas recuadas.
    pub superficie: Color32,
    /// Cartões, menus e janelas: o que fica "por cima".
    pub superficie_alta: Color32,
    pub realce: Color32,
    pub borda: Color32,
    pub texto: Color32,
    pub suave: Color32,
    pub destaque: Color32,
    pub ok: Color32,
    pub alerta: Color32,
    pub erro: Color32,
    pub favo: Color32,
    pub terminal_fundo: Color32,
    pub terminal_texto: Color32,
    pub ansi: [Color32; 16],
}

const fn rgb(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

#[rustfmt::skip]
pub const ESCURO: Paleta = Paleta {
    fundo: rgb(0x0e1117),
    lateral: rgb(0x11151c),
    superficie: rgb(0x151a22),
    superficie_alta: rgb(0x1b212b),
    realce: rgb(0x232a36),
    borda: rgb(0x262d39),
    texto: rgb(0xe4e8ef),
    suave: rgb(0x8e97a8),
    destaque: rgb(0xb794f6),
    ok: rgb(0x7fd18b),
    alerta: rgb(0xf2b35b),
    erro: rgb(0xf2616f),
    favo: Color32::from_rgba_premultiplied(13, 13, 14, 13),
    terminal_fundo: rgb(0x0f131a),
    terminal_texto: rgb(0xd7dae0),
    ansi: [
        rgb(0x1b1f27), rgb(0xf2616f), rgb(0x7fd18b), rgb(0xf2c46b), rgb(0x6aa7f8), rgb(0xc792ea), rgb(0x5fd7e6), rgb(0xd7dae0),
        rgb(0x5c6370), rgb(0xff7b86), rgb(0x9be3a5), rgb(0xffd98a), rgb(0x8cbcff), rgb(0xdcb0f5), rgb(0x86e6f0), rgb(0xffffff),
    ],
};

#[rustfmt::skip]
pub const CLARO: Paleta = Paleta {
    fundo: rgb(0xf4f5f8),
    lateral: rgb(0xeceef3),
    superficie: rgb(0xe9ecf1),
    superficie_alta: rgb(0xffffff),
    realce: rgb(0xe2e6ee),
    borda: rgb(0xd9dde5),
    texto: rgb(0x1b1f27),
    suave: rgb(0x667085),
    destaque: rgb(0x7c4ddb),
    ok: rgb(0x1f8450),
    alerta: rgb(0xa86400),
    erro: rgb(0xd6364a),
    favo: Color32::from_rgba_premultiplied(0, 0, 0, 13),
    terminal_fundo: rgb(0xfbfbfd),
    terminal_texto: rgb(0x1b1f27),
    ansi: [
        rgb(0x1b1f27), rgb(0xc4283a), rgb(0x1f8a4c), rgb(0x9a6a00), rgb(0x2463c9), rgb(0x8a3fd1), rgb(0x0f7f91), rgb(0x5c6370),
        rgb(0x667085), rgb(0xd6364a), rgb(0x23915a), rgb(0xb07d0c), rgb(0x3b7be0), rgb(0x9c57e0), rgb(0x1596a8), rgb(0x1b1f27),
    ],
};

/// Tema leitura: tons quentes de papel, contraste suave para ler por muito tempo.
#[rustfmt::skip]
pub const LEITURA: Paleta = Paleta {
    fundo: rgb(0xf3ecdc),
    lateral: rgb(0xebe2cd),
    superficie: rgb(0xe7ddc6),
    superficie_alta: rgb(0xfbf7ee),
    realce: rgb(0xe0d4b8),
    borda: rgb(0xd6c8a8),
    texto: rgb(0x3a2f24),
    suave: rgb(0x77695a),
    destaque: rgb(0x9a4a22),
    ok: rgb(0x4d7a36),
    // Âmbar puxado para o amarelo: perto do destaque (ferrugem) e do erro,
    // um marrom a mais não se distinguia no telão.
    alerta: rgb(0x806300),
    erro: rgb(0xb3372d),
    favo: Color32::from_rgba_premultiplied(0, 0, 0, 12),
    terminal_fundo: rgb(0xf7f1e3),
    terminal_texto: rgb(0x3a2f24),
    ansi: [
        rgb(0x3a2f24), rgb(0xb3372d), rgb(0x4d7a36), rgb(0x8a6410), rgb(0x3b5e8c), rgb(0x86457a), rgb(0x2f7470), rgb(0x77695a),
        rgb(0x8a7c6a), rgb(0xc7473c), rgb(0x5e8f45), rgb(0xa07418), rgb(0x4a70a3), rgb(0x9a568d), rgb(0x3a8781), rgb(0x3a2f24),
    ],
};

/// Mistura opaca de `a` para `b` (t = 0 dá `a`, t = 1 dá `b`). Para fundos
/// tingidos: uma cor translúcida sobre um painel sem nada embaixo se mistura
/// com o preto da janela, não com o fundo do tema.
pub fn misturar(a: Color32, b: Color32, t: f32) -> Color32 {
    let canal = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(canal(a.r(), b.r()), canal(a.g(), b.g()), canal(a.b(), b.b()))
}

/// Fundo de faixa tingido pela cor do aviso, já opaco sobre o fundo do tema.
pub fn fundo_tingido(p: &Paleta, cor: Color32, claro: bool) -> Color32 {
    misturar(p.fundo, cor, if claro { 0.12 } else { 0.18 })
}

/// 0 = escuro, 1 = claro, 2 = leitura.
static ATUAL: AtomicU8 = AtomicU8::new(0);

/// As cores do tema que está na tela agora.
pub fn cores() -> &'static Paleta {
    match ATUAL.load(Ordering::Relaxed) {
        1 => &CLARO,
        2 => &LEITURA,
        _ => &ESCURO,
    }
}

/// Tema de fundo claro (claro ou leitura).
pub fn claro() -> bool {
    ATUAL.load(Ordering::Relaxed) != 0
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Escolha {
    Escuro,
    Claro,
    Leitura,
}

impl Escolha {
    pub const TODAS: [Escolha; 3] = [Escolha::Escuro, Escolha::Claro, Escolha::Leitura];

    pub fn nome(self) -> &'static str {
        match self {
            Escolha::Escuro => "Escuro",
            Escolha::Claro => "Claro",
            Escolha::Leitura => "Leitura",
        }
    }

    /// Nome gravado no perfil pelo núcleo.
    pub fn chave(self) -> &'static str {
        match self {
            Escolha::Escuro => "escuro",
            Escolha::Claro => "claro",
            Escolha::Leitura => "leitura",
        }
    }

    pub fn da_chave(chave: &str) -> Escolha {
        Escolha::TODAS.into_iter().find(|e| e.chave() == chave).unwrap_or(Escolha::Escuro)
    }

    /// O egui só tem os temas escuro e claro; o de leitura usa a vaga do claro
    /// com as próprias cores.
    pub fn aplicar(self, ctx: &egui::Context) {
        match self {
            Escolha::Escuro => {
                ATUAL.store(0, Ordering::Relaxed);
                ctx.set_theme(ThemePreference::Dark);
            }
            Escolha::Claro => {
                ATUAL.store(1, Ordering::Relaxed);
                ctx.set_visuals_of(Theme::Light, visuais(&CLARO, Visuals::light()));
                ctx.set_theme(ThemePreference::Light);
            }
            Escolha::Leitura => {
                ATUAL.store(2, Ordering::Relaxed);
                ctx.set_visuals_of(Theme::Light, visuais(&LEITURA, Visuals::light()));
                ctx.set_theme(ThemePreference::Light);
            }
        }
    }
}

/// Fonte do texto em peso seminegrito (títulos, nomes, valores).
pub fn forte(tamanho: f32) -> FontId {
    FontId::new(tamanho, FontFamily::Name("forte".into()))
}

pub fn texto_forte(texto: impl Into<String>, tamanho: f32) -> RichText {
    RichText::new(texto).font(forte(tamanho))
}

pub fn instalar(ctx: &egui::Context) {
    let mut fontes = FontDefinitions::default();
    let mut adicionar = |nome: &str, dados: &'static [u8]| {
        fontes.font_data.insert(nome.to_owned(), Arc::new(FontData::from_static(dados)));
    };
    adicionar("inter", include_bytes!("../fontes/Inter-Regular.ttf"));
    adicionar("inter-forte", include_bytes!("../fontes/Inter-SemiBold.ttf"));
    adicionar("jetbrains", include_bytes!("../fontes/JetBrainsMono-Regular.ttf"));
    // As fontes padrão do egui ficam de reserva para símbolos que a Inter não tem.
    fontes.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".to_owned());
    fontes.families.entry(FontFamily::Monospace).or_default().insert(0, "jetbrains".to_owned());
    let reserva = fontes.families[&FontFamily::Proportional].clone();
    fontes.families.insert(FontFamily::Name("forte".into()), std::iter::once("inter-forte".to_owned()).chain(reserva).collect());
    ctx.set_fonts(fontes);

    ctx.set_visuals_of(Theme::Dark, visuais(&ESCURO, Visuals::dark()));
    ctx.set_visuals_of(Theme::Light, visuais(&CLARO, Visuals::light()));
    // Nos dois estilos (escuro e claro): o claro e o leitura usam a vaga do
    // claro e, sem isto, ficariam com os tamanhos e espaçamentos padrão do egui.
    ctx.all_styles_mut(|s| {
        s.interaction.selectable_labels = false;
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.menu_spacing = 4.0;
        let tamanhos = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
            (TextStyle::Heading, forte(17.0)),
        ];
        s.text_styles = tamanhos.into_iter().collect();
    });
}

fn visuais(p: &Paleta, mut v: Visuals) -> Visuals {
    let raio = CornerRadius::same(RAIO_CONTROLE);
    v.panel_fill = p.fundo;
    v.window_fill = p.superficie_alta;
    v.window_stroke = Stroke::new(1.0, p.borda);
    v.window_corner_radius = CornerRadius::same(RAIO_SUPERFICIE);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.superficie;
    v.faint_bg_color = p.superficie;
    v.override_text_color = Some(p.texto);
    v.selection.bg_fill = p.destaque.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.destaque);
    v.hyperlink_color = p.destaque;
    // Cursor de texto fixo: o piscar redesenha a janela duas vezes por
    // segundo enquanto um campo tem o foco (a caixa de mensagem tem quase
    // sempre), e a tela parada deixa de ficar parada.
    v.text_cursor.blink = false;
    let w = &mut v.widgets;
    for (estado, fundo, borda) in [
        (&mut w.inactive, p.superficie_alta, p.borda),
        (&mut w.hovered, p.realce, p.borda),
        (&mut w.active, p.realce, p.destaque),
        (&mut w.open, p.realce, p.borda),
    ] {
        estado.bg_fill = fundo;
        estado.weak_bg_fill = fundo;
        estado.bg_stroke = Stroke::new(1.0, borda);
        estado.fg_stroke = Stroke::new(1.0, p.texto);
        estado.corner_radius = raio;
        estado.expansion = 0.0;
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.borda);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.texto);
    w.noninteractive.corner_radius = raio;
    v
}

/// Logo: um favo com a célula do meio preenchida.
pub fn logo(pintor: &egui::Painter, centro: Pos2, raio: f32) {
    let p = cores();
    let hexagono = |c: Pos2, r: f32| -> Vec<Pos2> {
        (0..6)
            .map(|i| {
                let a = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 90.0);
                c + vec2(a.cos() * r, a.sin() * r)
            })
            .collect()
    };
    pintor.add(Shape::convex_polygon(hexagono(centro, raio), p.destaque.gamma_multiply(0.18), Stroke::new(1.5, p.destaque)));
    pintor.add(Shape::convex_polygon(hexagono(centro, raio * 0.45), p.destaque, Stroke::NONE));
}

/// Seta para baixo desenhada, sem depender de a fonte ter o símbolo.
fn seta(pintor: &egui::Painter, centro: Pos2, cor: Color32) {
    let pontos = vec![centro + vec2(-3.5, -1.5), centro + vec2(3.5, -1.5), centro + vec2(0.0, 2.5)];
    pintor.add(Shape::convex_polygon(pontos, cor, Stroke::NONE));
}

/// Filtro em forma de chip arredondado: "Rótulo: valor ▾". Destacado quando o
/// filtro está em uso, para ficar claro que a lista está filtrada.
pub fn chip(ui: &mut egui::Ui, rotulo: &str, valor: &str, em_uso: bool) -> Response {
    let p = cores();
    let fonte = FontId::proportional(13.0);
    // Sem valor, o chip vira um botão de menu: só o rótulo, na cor do texto.
    let (rotulo, cor_rotulo) = if valor.is_empty() { (rotulo.to_owned(), p.texto) } else { (format!("{rotulo}: "), p.suave) };
    let texto_rotulo = ui.painter().layout_no_wrap(rotulo, fonte.clone(), cor_rotulo);
    let texto_valor = ui.painter().layout_no_wrap(valor.to_owned(), forte(13.0), if em_uso { p.destaque } else { p.texto });
    // Respiro: 14 antes do texto, 10 entre o texto e a seta, 14 depois da seta.
    let largura = 14.0 + texto_rotulo.size().x + texto_valor.size().x + 10.0 + 7.0 + 14.0;
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura, 32.0), Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let fundo = if em_uso {
        p.destaque.gamma_multiply(if claro() { 0.12 } else { 0.18 })
    } else if resposta.hovered() {
        p.realce
    } else {
        p.superficie_alta
    };
    let pintor = ui.painter();
    pintor.rect(rect, CornerRadius::same(16), fundo, Stroke::new(1.0, if em_uso { p.destaque.gamma_multiply(0.6) } else { p.borda }), egui::StrokeKind::Inside);
    let y = rect.center().y;
    let x = rect.left() + 14.0;
    pintor.galley(pos2(x, y - texto_rotulo.size().y / 2.0), texto_rotulo.clone(), cor_rotulo);
    pintor.galley(pos2(x + texto_rotulo.size().x, y - texto_valor.size().y / 2.0), texto_valor, p.texto);
    seta(pintor, pos2(rect.right() - 17.5, y + 0.5), if em_uso { p.destaque } else { p.suave });
    resposta
}

/// Seletor segmentado: várias opções numa pílula só, a escolhida preenchida.
/// Retorna o índice clicado, se houver.
pub fn segmentado(ui: &mut egui::Ui, opcoes: &[&str], escolhida: usize) -> Option<usize> {
    segmentado_com_largura(ui, opcoes, escolhida, None)
}

/// Igual ao `segmentado`, mas ocupando exatamente `largura` (partes iguais):
/// usado onde o espaço é fixo, como a barra lateral.
pub fn segmentado_com_largura(ui: &mut egui::Ui, opcoes: &[&str], escolhida: usize, largura: Option<f32>) -> Option<usize> {
    let p = cores();
    let fonte = FontId::proportional(13.0);
    let larguras: Vec<f32> = match largura {
        Some(total) => vec![(total - 8.0) / opcoes.len() as f32; opcoes.len()],
        None => opcoes.iter().map(|o| ui.painter().layout_no_wrap((*o).to_owned(), fonte.clone(), p.texto).size().x + 30.0).collect(),
    };
    let (rect, _) = ui.allocate_exact_size(vec2(larguras.iter().sum::<f32>() + 8.0, 34.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(17), p.superficie);
    let mut x = rect.left() + 4.0;
    let mut clicada = None;
    for (i, (opcao, largura)) in opcoes.iter().zip(&larguras).enumerate() {
        let parte = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(*largura, rect.height() - 8.0));
        let resposta = ui.interact(parte, ui.id().with(("segmentado", opcoes[0], i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if i == escolhida {
            ui.painter().rect(parte, CornerRadius::same(12), p.superficie_alta, Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
        } else if resposta.hovered() {
            ui.painter().rect_filled(parte, CornerRadius::same(12), p.realce);
        }
        let cor = if i == escolhida { p.texto } else { p.suave };
        let fonte = if i == escolhida { forte(13.0) } else { fonte.clone() };
        ui.painter().text(parte.center(), egui::Align2::CENTER_CENTER, *opcao, fonte, cor);
        if resposta.clicked() {
            clicada = Some(i);
        }
        x += largura;
    }
    clicada
}

/// Item de menu com respiro nas laterais, fundo ao passar o mouse e marca na
/// opção escolhida. Retorna se foi clicado.
pub fn opcao_menu(ui: &mut egui::Ui, texto: &str, marcada: bool) -> bool {
    let p = cores();
    let fonte = if marcada { forte(13.5) } else { FontId::proportional(13.5) };
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), fonte, p.texto);
    // A largura mínima do menu (set_min_width) manda; ocupar o disponível faria um
    // menu de contexto tomar a tela inteira.
    let largura = (galeria.size().x + 56.0).max(ui.min_rect().width());
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura, 32.0), Sense::click());
    if resposta.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), p.realce);
    }
    ui.painter().galley(pos2(rect.left() + 14.0, rect.center().y - galeria.size().y / 2.0), galeria, p.texto);
    if marcada {
        ui.painter().circle_filled(pos2(rect.right() - 16.0, rect.center().y), 3.5, p.destaque);
    }
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Campo de texto com rótulo em cima, no estilo da Colmeia.
pub fn campo(ui: &mut egui::Ui, rotulo: &str, texto: &mut String, dica: &str) -> Response {
    let p = cores();
    ui.label(RichText::new(rotulo).color(p.suave).size(12.5));
    ui.add_space(2.0);
    let largura = ui.available_width();
    egui::Frame::new()
        .fill(fundo_campo(p, false))
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(RAIO_CONTROLE))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(texto)
                    .frame(egui::Frame::NONE)
                    .desired_width(largura - 22.0)
                    .font(FontId::proportional(14.0))
                    .hint_text(RichText::new(dica).color(p.suave)),
            )
        })
        .inner
}

/// Fundo de um campo de texto. Nos temas claros, o editável fica no tom mais
/// claro (como um papel em branco) e o só leitura no recuado; no escuro, o
/// recuado já se destaca do painel.
pub fn fundo_campo(p: &Paleta, so_leitura: bool) -> Color32 {
    if claro() && !so_leitura { p.superficie_alta } else { p.superficie }
}

/// Título e explicação de uma tela ou diálogo.
pub fn cabecalho(ui: &mut egui::Ui, titulo: &str, explicacao: &str) {
    let p = cores();
    ui.label(texto_forte(titulo, 19.0).color(p.texto));
    if !explicacao.is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new(explicacao).color(p.suave).size(13.5));
    }
}

/// Moldura dos diálogos e da tela de entrada.
pub fn moldura_janela() -> egui::Frame {
    let p = cores();
    egui::Frame::new()
        .fill(p.superficie_alta)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(RAIO_JANELA))
        .inner_margin(egui::Margin::same(24))
        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if claro() { 40 } else { 110 }) })
}

/// Botão principal: pílula preenchida na cor de destaque.
pub fn botao_principal(ui: &mut egui::Ui, texto: &str, ativo: bool) -> Response {
    let p = cores();
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), forte(13.5), p.texto);
    let (rect, resposta) = ui.allocate_exact_size(vec2(galeria.size().x + 32.0, 34.0), if ativo { Sense::click() } else { Sense::hover() });
    let fundo = if !ativo {
        p.realce
    } else if resposta.hovered() {
        p.destaque.gamma_multiply(0.85)
    } else {
        p.destaque
    };
    let cor = if !ativo {
        p.suave
    } else if claro() {
        Color32::WHITE
    } else {
        p.fundo
    };
    ui.painter().rect_filled(rect, CornerRadius::same(17), fundo);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, texto, forte(13.5), cor);
    if ativo { resposta.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resposta }
}

/// Botão secundário: pílula discreta, sem seta (para ações, não para menus).
pub fn botao_secundario(ui: &mut egui::Ui, texto: &str) -> Response {
    botao_secundario_com(ui, texto, true)
}

/// Botão secundário que pode ficar inativo: texto suave, sem fundo e com a
/// borda apagada, para parecer do mesmo estado que o principal inativo.
pub fn botao_secundario_com(ui: &mut egui::Ui, texto: &str, ativo: bool) -> Response {
    let p = cores();
    let cor = if ativo { p.texto } else { p.suave };
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), FontId::proportional(13.0), cor);
    let (rect, resposta) = ui.allocate_exact_size(vec2(galeria.size().x + 28.0, 32.0), if ativo { Sense::click() } else { Sense::hover() });
    let (fundo, borda) = match (ativo, resposta.hovered()) {
        (false, _) => (Color32::TRANSPARENT, p.borda.gamma_multiply(0.5)),
        (true, true) => (p.realce, p.borda),
        (true, false) => (p.superficie_alta, p.borda),
    };
    // Com o foco do teclado (a confirmação de escrita começa em "Cancelar"), o contorno de foco.
    let borda = if resposta.has_focus() { Stroke::new(1.5, p.destaque) } else { Stroke::new(1.0, borda) };
    ui.painter().rect(rect, CornerRadius::same(16), fundo, borda, egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galeria.size() / 2.0, galeria, cor);
    if ativo { resposta.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resposta }
}

/// Botão de uma ação que encerra algo (parar agentes, por exemplo): pílula
/// com borda e texto na cor de erro, sem preencher (o principal continua
/// sendo o caminho seguro).
pub fn botao_alerta(ui: &mut egui::Ui, texto: &str) -> Response {
    let p = cores();
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), FontId::proportional(13.0), p.erro);
    let (rect, resposta) = ui.allocate_exact_size(vec2(galeria.size().x + 28.0, 32.0), Sense::click());
    let fundo = if resposta.hovered() { p.erro.gamma_multiply(0.12) } else { p.superficie_alta };
    let borda = if resposta.has_focus() { Stroke::new(1.5, p.destaque) } else { Stroke::new(1.0, p.erro) };
    ui.painter().rect(rect, CornerRadius::same(16), fundo, borda, egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galeria.size() / 2.0, galeria, p.erro);
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Estado de um agente (ou de uma tarefa concluída) como a tela mostra: a
/// mesma cor, forma e palavra no cartão, no painel, na abelha e na linha do tempo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EstadoVisual {
    Trabalhando,
    PedeAprovacao,
    /// Um pedido de consulta ao banco do agente espera a sua aprovação.
    PedeConsulta,
    SuaVez,
    Parado,
    Terminou,
    Interrompido,
    Erro,
    Concluiu,
}

impl EstadoVisual {
    pub fn cor(self) -> Color32 {
        let p = cores();
        match self {
            EstadoVisual::Trabalhando => p.ok,
            EstadoVisual::PedeAprovacao | EstadoVisual::PedeConsulta | EstadoVisual::SuaVez | EstadoVisual::Interrompido => p.alerta,
            EstadoVisual::Parado | EstadoVisual::Terminou => p.suave,
            EstadoVisual::Erro => p.erro,
            EstadoVisual::Concluiu => p.destaque,
        }
    }

    /// Palavra do selo.
    pub fn selo(self) -> &'static str {
        match self {
            EstadoVisual::Trabalhando => "Trabalhando",
            EstadoVisual::PedeAprovacao => "Pede aprovação",
            EstadoVisual::PedeConsulta => "Quer consultar o banco",
            EstadoVisual::SuaVez => "Sua vez",
            EstadoVisual::Parado => "Parado",
            EstadoVisual::Terminou => "Terminou",
            EstadoVisual::Interrompido => "Interrompido",
            EstadoVisual::Erro => "Erro",
            EstadoVisual::Concluiu => "Concluiu",
        }
    }

    /// Ponto cheio: está acontecendo ou pede você. Anel: parou. Assim os
    /// estados se separam mesmo sem distinguir a cor.
    pub fn cheio(self) -> bool {
        !matches!(self, EstadoVisual::Parado | EstadoVisual::Terminou | EstadoVisual::Interrompido)
    }

    /// Pede você: aparece na abelha, no aviso e no título da janela.
    pub fn pede_voce(self) -> bool {
        matches!(self, EstadoVisual::PedeAprovacao | EstadoVisual::PedeConsulta | EstadoVisual::SuaVez | EstadoVisual::Erro)
    }

    /// Espera uma resposta sua (sem contar o erro).
    pub fn espera_voce(self) -> bool {
        matches!(self, EstadoVisual::PedeAprovacao | EstadoVisual::PedeConsulta | EstadoVisual::SuaVez)
    }
}

/// Ponto de estado: cheio ou anel; o erro ganha um anel externo de leve.
pub fn ponto(pintor: &egui::Painter, centro: Pos2, raio: f32, estado: EstadoVisual) {
    let cor = estado.cor();
    if estado.cheio() {
        pintor.circle_filled(centro, raio, cor);
    } else {
        pintor.circle_stroke(centro, raio - 0.75, Stroke::new(1.5, cor));
    }
    if estado == EstadoVisual::Erro {
        pintor.circle_stroke(centro, raio + 2.5, Stroke::new(1.0, cor.gamma_multiply(0.4)));
    }
}

/// Etiqueta: texto na cor sobre um fundo bem leve da mesma cor. Serve para a
/// branch, o selo de estado e de coluna e o projeto na linha do tempo.
/// `pos` é o canto de cima à esquerda; devolve onde ficou.
pub fn etiqueta(pintor: &egui::Painter, pos: Pos2, texto: &str, fonte: FontId, cor: Color32) -> Rect {
    let galeria = pintor.layout_no_wrap(texto.to_owned(), fonte, cor);
    let rect = Rect::from_min_size(pos, vec2(galeria.size().x + 12.0, 19.0));
    pintor.rect_filled(rect, CornerRadius::same(RAIO_ETIQUETA), cor.gamma_multiply(0.14));
    pintor.galley(rect.center() - galeria.size() / 2.0, galeria, cor);
    rect
}

/// Etiqueta dentro de um layout (ocupa o lugar dela na linha).
pub fn etiqueta_ui(ui: &mut egui::Ui, texto: &str, fonte: FontId, cor: Color32) -> Response {
    let largura = ui.painter().layout_no_wrap(texto.to_owned(), fonte.clone(), cor).size().x + 12.0;
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura, 19.0), Sense::hover());
    etiqueta(ui.painter(), rect.min, texto, fonte, cor);
    resposta
}

/// Fonte das etiquetas de texto (selos de estado, coluna e projeto).
pub fn fonte_etiqueta() -> FontId {
    forte(11.5)
}

/// Rótulo da pílula de cada coluna, no feminino de "tarefa": o mesmo no
/// cabeçalho da tarefa, na linha do tempo, na Daily, na Sprint e no slide
/// (o nome da coluna no quadro continua "Revisão", "Concluído").
pub fn rotulo_coluna(coluna: crate::dados::Coluna) -> &'static str {
    use crate::dados::Coluna;
    match coluna {
        Coluna::Backlog => "Backlog",
        Coluna::Trabalhando => "Trabalhando",
        Coluna::AguardandoVoce => "Aguardando você",
        Coluna::Revisao => "Em revisão",
        Coluna::Concluido => "Concluída",
    }
}

/// Forma da marca de estado, para o estado não depender só da cor (no tema
/// leitura, destaque, alerta e erro são tons próximos): cheia para concluída,
/// em revisão e trabalhando; anel para o que espera (você ou parado); anel
/// grosso para erro.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Marca {
    Cheia,
    Anel,
    AnelGrosso,
}

pub fn marca(pintor: &egui::Painter, centro: Pos2, raio: f32, cor: Color32, marca: Marca) {
    match marca {
        Marca::Cheia => {
            pintor.circle_filled(centro, raio, cor);
        }
        Marca::Anel => {
            pintor.circle_stroke(centro, raio - 0.75, Stroke::new(1.5, cor));
        }
        Marca::AnelGrosso => {
            let grossura = (raio * 0.55).max(2.5);
            pintor.circle_stroke(centro, raio - grossura / 2.0, Stroke::new(grossura, cor));
        }
    }
}

/// Estado de uma tarefa no registro (linha do tempo, Daily, Sprint e slide):
/// cor, rótulo e marca, de um lugar só. `coluna` é a chave do núcleo.
pub fn estado_da_tarefa(coluna: &str, erro: bool, removida: bool) -> (Color32, &'static str, Marca) {
    use crate::dados::Coluna;
    let p = cores();
    if removida {
        return (p.suave, "Removida", Marca::Anel);
    }
    let conhecida = Coluna::TODAS.into_iter().find(|c| c.chave() == coluna);
    if erro && conhecida != Some(Coluna::Concluido) {
        return (p.erro, "Com erro", Marca::AnelGrosso);
    }
    match conhecida {
        Some(c @ (Coluna::Concluido | Coluna::Revisao | Coluna::Trabalhando)) => (cor_coluna(c), rotulo_coluna(c), Marca::Cheia),
        Some(c) => (cor_coluna(c), rotulo_coluna(c), Marca::Anel),
        None => (p.suave, "Parada", Marca::Anel),
    }
}

/// Cor de cada coluna, usada no selo do cabeçalho da tarefa.
pub fn cor_coluna(coluna: crate::dados::Coluna) -> Color32 {
    use crate::dados::Coluna;
    let p = cores();
    match coluna {
        Coluna::Backlog => p.suave,
        Coluna::Trabalhando => p.ok,
        Coluna::AguardandoVoce => p.alerta,
        Coluna::Revisao => p.texto,
        Coluna::Concluido => p.destaque,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Icone {
    Mais,
    Fechar,
    Anterior,
    Proximo,
    // Lousa: a barra de ferramentas e o zoom.
    Nota,
    Texto,
    Codigo,
    Imagem,
    Video,
    Tarefa,
    Ligacao,
    Menos,
    MaisZoom,
    Ajustar,
    Lousa,
    Duplicar,
    Apagar,
    // Bancos de dados: a árvore, os selos da conexão e o cancelar.
    Banco,
    Esquema,
    Tabela,
    Visao,
    Lapis,
    Agente,
    Parar,
}

/// Botão só com ícone, desenhado (a fonte não garante os símbolos): sem
/// fundo parado, `realce` ao passar o mouse.
pub fn botao_icone(ui: &mut egui::Ui, icone: Icone, lado: f32) -> Response {
    let (rect, resposta) = ui.allocate_exact_size(vec2(lado, lado), Sense::click());
    pintar_icone(ui, rect, icone, &resposta);
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// O mesmo botão num retângulo fixo (por cima de um cartão, por exemplo).
pub fn botao_icone_em(ui: &mut egui::Ui, rect: Rect, id: egui::Id, icone: Icone) -> Response {
    let resposta = ui.interact(rect, id, Sense::click());
    pintar_icone(ui, rect, icone, &resposta);
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Botão de ícone que pode ficar inativo: o ícone apagado, sem fundo ao passar o mouse.
pub fn botao_icone_com(ui: &mut egui::Ui, icone: Icone, lado: f32, ativo: bool) -> Response {
    if ativo {
        return botao_icone(ui, icone, lado);
    }
    let (rect, resposta) = ui.allocate_exact_size(vec2(lado, lado), Sense::hover());
    desenhar_icone(ui.painter(), rect.center(), icone, cores().suave.gamma_multiply(0.5));
    resposta
}

fn pintar_icone(ui: &egui::Ui, rect: Rect, icone: Icone, resposta: &Response) {
    let p = cores();
    let pintor = ui.painter();
    if resposta.hovered() {
        pintor.rect_filled(rect, CornerRadius::same(RAIO_CONTROLE), p.realce);
    }
    let cor = if resposta.hovered() && icone == Icone::Apagar {
        p.erro
    } else if resposta.hovered() {
        p.texto
    } else {
        p.suave
    };
    desenhar_icone(pintor, rect.center(), icone, cor);
}

/// O desenho de um ícone em 16×16 (traço de 1,5), centrado em `c`.
pub fn desenhar_icone(pintor: &egui::Painter, c: Pos2, icone: Icone, cor: Color32) {
    let traco = Stroke::new(1.5, cor);
    let caixa = |l: f32, a: f32| Rect::from_center_size(c, vec2(l, a));
    match icone {
        Icone::Mais => {
            for dx in [-5.0, 0.0, 5.0] {
                pintor.circle_filled(c + vec2(dx, 0.0), 1.6, cor);
            }
        }
        Icone::Fechar => {
            let m = 4.5;
            pintor.line_segment([c + vec2(-m, -m), c + vec2(m, m)], traco);
            pintor.line_segment([c + vec2(-m, m), c + vec2(m, -m)], traco);
        }
        Icone::Anterior | Icone::Proximo => {
            let lado = if icone == Icone::Anterior { 1.0 } else { -1.0 };
            let pontos = vec![c + vec2(2.5 * lado, -5.0), c + vec2(-2.5 * lado, 0.0), c + vec2(2.5 * lado, 5.0)];
            pintor.add(Shape::line(pontos, Stroke::new(1.5, cor)));
        }
        Icone::Nota => {
            // Quadrado com o canto de baixo à direita dobrado.
            let r = caixa(13.0, 13.0);
            let dobra = 4.5;
            let contorno =
                vec![r.left_top(), r.right_top(), r.right_bottom() - vec2(0.0, dobra), r.right_bottom() - vec2(dobra, 0.0), r.left_bottom(), r.left_top()];
            pintor.add(Shape::line(contorno, traco));
            pintor
                .add(Shape::line(vec![r.right_bottom() - vec2(0.0, dobra), r.right_bottom() - vec2(dobra, dobra), r.right_bottom() - vec2(dobra, 0.0)], traco));
        }
        Icone::Texto => {
            pintor.line_segment([c + vec2(-5.5, -5.5), c + vec2(5.5, -5.5)], traco);
            pintor.line_segment([c + vec2(0.0, -5.5), c + vec2(0.0, 6.0)], traco);
        }
        Icone::Codigo => {
            pintor.add(Shape::line(vec![c + vec2(-2.5, -5.0), c + vec2(-6.5, 0.0), c + vec2(-2.5, 5.0)], traco));
            pintor.add(Shape::line(vec![c + vec2(2.5, -5.0), c + vec2(6.5, 0.0), c + vec2(2.5, 5.0)], traco));
        }
        Icone::Imagem => {
            let r = caixa(15.0, 12.0);
            pintor.rect_stroke(r, CornerRadius::same(2), traco, egui::StrokeKind::Middle);
            pintor.add(Shape::line(
                vec![r.left_bottom() + vec2(1.5, -2.0), c + vec2(-1.5, 0.5), c + vec2(1.5, 3.0), c + vec2(4.0, 0.5), r.right_bottom() + vec2(-1.5, -2.0)],
                traco,
            ));
            pintor.circle_filled(c + vec2(3.5, -2.5), 1.4, cor);
        }
        Icone::Video => {
            let r = caixa(15.0, 12.0);
            pintor.rect_stroke(r, CornerRadius::same(2), traco, egui::StrokeKind::Middle);
            play(pintor, c, 3.0, cor);
        }
        Icone::Tarefa => {
            let r = caixa(15.0, 11.0);
            pintor.rect_stroke(r, CornerRadius::same(2), traco, egui::StrokeKind::Middle);
            pintor.line_segment([r.left_top() + vec2(3.0, 3.5), r.right_top() + vec2(-3.0, 3.5)], traco);
            pintor.circle_filled(r.left_bottom() + vec2(4.0, -3.0), 1.5, cor);
        }
        Icone::Ligacao => {
            let (a, b) = (c + vec2(-6.0, 4.0), c + vec2(5.0, -4.0));
            pintor.circle_filled(a, 1.8, cor);
            for forma in Shape::dashed_line(&[a, b - vec2(2.0, -1.5)], traco, 2.5, 2.0) {
                pintor.add(forma);
            }
            let direcao = (b - a).normalized();
            let normal = vec2(-direcao.y, direcao.x);
            pintor.add(Shape::convex_polygon(vec![b, b - direcao * 4.5 + normal * 2.5, b - direcao * 4.5 - normal * 2.5], cor, Stroke::NONE));
        }
        Icone::Menos => {
            pintor.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], traco);
        }
        Icone::MaisZoom => {
            pintor.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], traco);
            pintor.line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 5.0)], traco);
        }
        Icone::Ajustar => {
            let (m, p) = (6.0, 3.0);
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let canto = c + vec2(m * sx, m * sy);
                pintor.add(Shape::line(vec![canto - vec2(0.0, p * sy), canto, canto - vec2(p * sx, 0.0)], traco));
            }
        }
        Icone::Lousa => {
            let r = caixa(13.0, 11.0);
            pintor.rect_stroke(r, CornerRadius::same(3), traco, egui::StrokeKind::Middle);
            pintor.line_segment([r.left_top() + vec2(3.0, 4.0), r.left_top() + vec2(8.0, 4.0)], traco);
            pintor.line_segment([r.left_top() + vec2(3.0, 7.0), r.left_top() + vec2(6.5, 7.0)], traco);
        }
        Icone::Duplicar => {
            pintor.rect_stroke(Rect::from_min_size(c + vec2(-6.0, -6.0), vec2(9.0, 9.0)), CornerRadius::same(2), traco, egui::StrokeKind::Middle);
            pintor.rect_stroke(Rect::from_min_size(c + vec2(-3.0, -3.0), vec2(9.0, 9.0)), CornerRadius::same(2), traco, egui::StrokeKind::Middle);
        }
        Icone::Apagar => {
            pintor.line_segment([c + vec2(-6.0, -4.0), c + vec2(6.0, -4.0)], traco);
            pintor.line_segment([c + vec2(-2.0, -6.0), c + vec2(2.0, -6.0)], traco);
            pintor.add(Shape::line(vec![c + vec2(-4.5, -4.0), c + vec2(-3.5, 6.0), c + vec2(3.5, 6.0), c + vec2(4.5, -4.0)], traco));
        }
        Icone::Banco => {
            // Cilindro: a elipse de cima, as laterais e o arco de baixo.
            let topo = c + vec2(0.0, -5.0);
            pintor.add(Shape::ellipse_stroke(topo, vec2(6.0, 2.0), traco));
            pintor.line_segment([topo + vec2(-6.0, 0.0), topo + vec2(-6.0, 10.0)], traco);
            pintor.line_segment([topo + vec2(6.0, 0.0), topo + vec2(6.0, 10.0)], traco);
            let arco: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 12.0;
                    topo + vec2(6.0 * a.cos(), 10.0 + 2.0 * a.sin())
                })
                .collect();
            pintor.add(Shape::line(arco, traco));
        }
        Icone::Esquema => {
            pintor.rect_stroke(Rect::from_min_size(c + vec2(-5.5, -5.5), vec2(7.0, 7.0)), CornerRadius::same(1), traco, egui::StrokeKind::Middle);
            pintor.rect_stroke(Rect::from_min_size(c + vec2(-1.5, -1.5), vec2(7.0, 7.0)), CornerRadius::same(1), traco, egui::StrokeKind::Middle);
        }
        Icone::Tabela | Icone::Visao => {
            let r = caixa(12.0, 10.0);
            if icone == Icone::Tabela {
                pintor.rect_stroke(r, CornerRadius::same(1), traco, egui::StrokeKind::Middle);
            } else {
                let contorno = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                for forma in Shape::dashed_line(&contorno, traco, 3.0, 2.0) {
                    pintor.add(forma);
                }
            }
            pintor.line_segment([r.left_top() + vec2(0.0, 3.0), r.right_top() + vec2(0.0, 3.0)], traco);
            pintor.line_segment([r.center_top() + vec2(0.0, 3.0), r.center_bottom()], traco);
        }
        Icone::Lapis => {
            // Lápis inclinado em 14 px: corpo de 2 traços paralelos (4 px de
            // largura), a ponta em triângulo e a faixa da borracha.
            let d = vec2(1.0, -1.0) / std::f32::consts::SQRT_2;
            let n = vec2(1.0, 1.0) / std::f32::consts::SQRT_2;
            let ponta = c + vec2(-5.5, 5.5);
            let fim = c + vec2(5.5, -5.5);
            let base = ponta + d * 4.5;
            let meia = 2.0;
            let corpo = vec![base + n * meia, fim + n * meia, fim - n * meia, base - n * meia];
            pintor.add(Shape::closed_line(corpo, traco));
            pintor.add(Shape::line(vec![base + n * meia, ponta, base - n * meia], traco));
            let faixa = fim - d * 3.0;
            pintor.line_segment([faixa + n * meia, faixa - n * meia], traco);
            pintor.circle_filled(ponta + d * 1.0, 1.0, cor);
        }
        Icone::Agente => {
            // O favo do logo, só o contorno.
            let pontos: Vec<Pos2> = (0..=6)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_3 * i as f32 + std::f32::consts::FRAC_PI_6;
                    c + vec2(5.5 * a.cos(), 5.5 * a.sin())
                })
                .collect();
            pintor.add(Shape::line(pontos, traco));
        }
        Icone::Parar => {
            pintor.rect_filled(caixa(9.0, 9.0), CornerRadius::same(2), cor);
        }
    }
}

/// Divisória arrastável entre duas superfícies: invisível parada (o vão de
/// 8 px já separa), um traço de 2 px no destaque com o mouse ou arrastando.
/// Devolve quanto foi arrastado neste quadro.
pub fn divisoria(ui: &mut egui::Ui, rect: Rect, id: egui::Id, vertical: bool) -> f32 {
    let p = cores();
    let resposta = ui.interact(rect, id, Sense::drag());
    let cursor = if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical };
    let resposta = resposta.on_hover_cursor(cursor);
    if resposta.hovered() || resposta.dragged() {
        let traco = if vertical {
            Rect::from_center_size(rect.center(), vec2(2.0, rect.height()))
        } else {
            Rect::from_center_size(rect.center(), vec2(rect.width(), 2.0))
        };
        ui.painter().rect_filled(traco, CornerRadius::same(1), p.destaque.gamma_multiply(0.6));
    }
    if resposta.dragged_by(egui::PointerButton::Primary) {
        let d = resposta.drag_delta();
        if vertical { d.x } else { d.y }
    } else {
        0.0
    }
}

/// Contador de pedidos pendentes: pílula de 18 px na cor de alerta, com o
/// número (9+ acima de nove). `centro_direito` é o meio da borda direita.
pub fn contador(pintor: &egui::Painter, centro_direito: Pos2, n: usize) -> Rect {
    let texto = if n > 9 { "9+".to_string() } else { n.to_string() };
    let galeria = pintor.layout_no_wrap(texto, forte(11.5), sobre_destaque());
    let largura = (galeria.size().x + 10.0).max(18.0);
    let rect = Rect::from_min_max(pos2(centro_direito.x - largura, centro_direito.y - 9.0), pos2(centro_direito.x, centro_direito.y + 9.0));
    pintor.rect_filled(rect, CornerRadius::same(9), cores().alerta);
    pintor.galley(rect.center() - galeria.size() / 2.0, galeria, sobre_destaque());
    rect
}

/// Caixa de marcar que pode ficar inativa (contorno apagado, texto suave, sem clique).
pub fn caixa_marcar_com(ui: &mut egui::Ui, texto: &str, marcada: &mut bool, ativa: bool) -> Response {
    if ativa {
        return caixa_marcar(ui, texto, marcada);
    }
    let p = cores();
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), FontId::proportional(13.5), p.suave);
    let (rect, resposta) = ui.allocate_exact_size(vec2(16.0 + 8.0 + galeria.size().x, 24.0_f32.max(galeria.size().y)), Sense::hover());
    let caixa = Rect::from_center_size(pos2(rect.left() + 8.0, rect.center().y), vec2(16.0, 16.0));
    ui.painter().rect(caixa, CornerRadius::same(4), Color32::TRANSPARENT, Stroke::new(1.5, p.borda.gamma_multiply(0.5)), egui::StrokeKind::Inside);
    ui.painter().galley(pos2(caixa.right() + 8.0, rect.center().y - galeria.size().y / 2.0), galeria, p.suave);
    resposta
}

/// ↗ desenhado em 10×10: a diagonal e o canto (a fonte não garante o símbolo).
pub fn externo(pintor: &egui::Painter, centro: Pos2, cor: Color32) {
    let traco = Stroke::new(1.5, cor);
    pintor.line_segment([centro + vec2(-3.5, 3.5), centro + vec2(3.5, -3.5)], traco);
    pintor.add(Shape::line(vec![centro + vec2(-1.0, -3.5), centro + vec2(3.5, -3.5), centro + vec2(3.5, 1.0)], traco));
}

/// Fundo da linha escolhida (árvore de arquivos, item com o cursor do
/// teclado): opaco e diferente do `realce` do mouse. Texto secundário sobre
/// ele vai em `texto` (o `suave` não passa de 4,5).
/// Nos temas claros a mistura é maior (26%): com 10%, a linha escolhida ficava mais
/// clara que o `realce` do mouse e parecia sumir quando o mouse passava por
/// outra linha. Quem desenha soma uma barrinha `destaque` na borda esquerda.
pub fn fundo_escolhido() -> Color32 {
    escolhido_em(cores(), claro())
}

fn escolhido_em(p: &Paleta, eh_claro: bool) -> Color32 {
    misturar(p.superficie_alta, p.destaque, if eh_claro { 0.26 } else { 0.16 })
}

/// Moldura das peças que flutuam presas a um botão (caixa de pedido,
/// endereço do navegador, gaveta de arquivos): a do aviso, como função.
///
/// Nos temas claros, o contorno é mais escuro e a sombra mais forte: a peça
/// abre sobre capturas que podem ter a mesma cor dela (o creme do Leitura).
pub fn moldura_flutuante() -> egui::Frame {
    let p = cores();
    let (contorno, sombra) = if claro() {
        (misturar(p.borda, p.texto, 0.3), egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(64) })
    } else {
        (p.borda, sombra(6, 18))
    };
    egui::Frame::new()
        .fill(p.superficie_alta)
        .stroke(Stroke::new(1.0, contorno))
        .corner_radius(CornerRadius::same(RAIO_SUPERFICIE))
        .inner_margin(egui::Margin::same(16))
        .shadow(sombra)
}

/// Largura do botão dividido. O lugar do ponto de estado é sempre reservado:
/// o botão não muda de tamanho (nem de lugar) quando o estado muda.
pub fn largura_botao_dividido(pintor: &egui::Painter, texto: &str) -> f32 {
    let w = pintor.layout_no_wrap(texto.to_owned(), FontId::proportional(13.0), Color32::WHITE).size().x;
    14.0 + 14.0 + w + 12.0 + 1.0 + 28.0
}

/// Botão com duas partes: a ação principal à esquerda (com um ponto de
/// estado: cheio na cor dada, ou um anel `suave` quando não há estado) e o
/// menu (▾) à direita. Devolve as duas respostas.
pub fn botao_dividido(ui: &mut egui::Ui, texto: &str, ponto: Option<Color32>) -> (Response, Response) {
    let p = cores();
    let largura = largura_botao_dividido(ui.painter(), texto);
    let (rect, _) = ui.allocate_exact_size(vec2(largura, 32.0), Sense::hover());
    let esquerda = Rect::from_min_max(rect.min, pos2(rect.right() - 29.0, rect.bottom()));
    let direita = Rect::from_min_max(pos2(rect.right() - 28.0, rect.top()), rect.max);
    let r_esq = ui.interact(esquerda, ui.id().with(("dividido", texto, 0)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let r_dir = ui.interact(direita, ui.id().with(("dividido", texto, 1)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let pintor = ui.painter();
    pintor.rect(rect, CornerRadius::same(16), p.superficie_alta, Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
    // O realce cobre só a metade sob o mouse.
    if r_esq.hovered() {
        pintor.rect_filled(esquerda.shrink(1.0), CornerRadius { nw: 15, sw: 15, ne: 0, se: 0 }, p.realce);
    }
    if r_dir.hovered() {
        pintor.rect_filled(direita.shrink(1.0), CornerRadius { nw: 0, sw: 0, ne: 15, se: 15 }, p.realce);
    }
    let x = rect.left() + 14.0;
    match ponto {
        Some(cor) => {
            pintor.circle_filled(pos2(x + 3.5, rect.center().y), 3.5, cor);
        }
        None => {
            pintor.circle_stroke(pos2(x + 3.5, rect.center().y), 3.0, Stroke::new(1.0, p.suave));
        }
    }
    let x = x + 14.0;
    let galeria = pintor.layout_no_wrap(texto.to_owned(), FontId::proportional(13.0), p.texto);
    pintor.galley(pos2(x, rect.center().y - galeria.size().y / 2.0), galeria, p.texto);
    pintor.line_segment([pos2(direita.left() - 0.5, rect.top() + 8.0), pos2(direita.left() - 0.5, rect.bottom() - 8.0)], Stroke::new(1.0, p.borda));
    seta(pintor, pos2(direita.center().x, rect.center().y + 0.5), if r_dir.hovered() { p.texto } else { p.suave });
    (r_esq, r_dir)
}

/// Triângulo de "tocar", um pouco à direita do centro (o centro visual dele).
pub fn play(pintor: &egui::Painter, centro: Pos2, tamanho: f32, cor: Color32) {
    let c = centro + vec2(tamanho * 0.15, 0.0);
    let pontos = vec![c + vec2(-tamanho * 0.6, -tamanho), c + vec2(tamanho, 0.0), c + vec2(-tamanho * 0.6, tamanho)];
    pintor.add(Shape::convex_polygon(pontos, cor, Stroke::NONE));
}

/// Cor do texto (ou do ícone) sobre o destaque, como no botão principal.
pub fn sobre_destaque() -> Color32 {
    if claro() { Color32::WHITE } else { cores().fundo }
}

/// Pílula de estado: ponto na cor do estado e a palavra na cor do texto, sobre
/// o fundo tingido (legível nos três temas, mesmo no telão). `cheio`: ponto
/// cheio ou anel; `ponto` falso deixa só a palavra (o chip neutro do projeto).
/// `pos` é o canto de cima à esquerda; devolve onde ficou.
pub struct Pilula<'a> {
    pub texto: &'a str,
    pub cor: Color32,
    pub cheio: bool,
    pub ponto: bool,
    pub grande: bool,
}

impl<'a> Pilula<'a> {
    pub fn grande(mut self) -> Self {
        self.grande = true;
        self
    }

    pub fn neutra(texto: &'a str) -> Self {
        Pilula { texto, cor: cores().suave, cheio: true, ponto: false, grande: false }
    }

    fn medidas(&self) -> (f32, f32, f32, FontId) {
        if self.grande { (32.0, 14.0, 5.0, forte(15.0)) } else { (22.0, 10.0, 3.5, forte(12.0)) }
    }

    pub fn largura(&self, pintor: &egui::Painter) -> f32 {
        let (_, margem, raio, fonte) = self.medidas();
        let texto = pintor.layout_no_wrap(self.texto.to_owned(), fonte, Color32::WHITE).size().x;
        margem * 2.0 + texto + if self.ponto { raio * 2.0 + 7.0 } else { 0.0 }
    }

    pub fn pintar(&self, pintor: &egui::Painter, pos: Pos2) -> Rect {
        let p = cores();
        let (altura, margem, raio, fonte) = self.medidas();
        let rect = Rect::from_min_size(pos, vec2(self.largura(pintor), altura));
        if self.ponto {
            pintor.rect_filled(rect, CornerRadius::same((altura / 2.0) as u8), fundo_tingido(p, self.cor, claro()));
            let centro = pos2(rect.left() + margem + raio, rect.center().y);
            if self.cheio {
                pintor.circle_filled(centro, raio, self.cor);
            } else {
                pintor.circle_stroke(centro, raio - 0.75, Stroke::new(1.5, self.cor));
            }
        } else {
            pintor.rect(rect, CornerRadius::same((altura / 2.0) as u8), p.superficie_alta, Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
        }
        let x = rect.left() + margem + if self.ponto { raio * 2.0 + 7.0 } else { 0.0 };
        let galeria = pintor.layout_no_wrap(self.texto.to_owned(), fonte, p.texto);
        pintor.galley(pos2(x, rect.center().y - galeria.size().y / 2.0), galeria, p.texto);
        rect
    }
}

/// Um número grande com o rótulo embaixo (e a marca do estado antes do rótulo).
/// A mesma peça na Daily, na Sprint, na capa e no slide final.
pub fn metrica(ui: &mut egui::Ui, valor: &str, rotulo: &str, ponto: Option<(Color32, Marca)>, tamanho: f32) -> Response {
    let p = cores();
    let rotulo_tamanho = if tamanho >= 48.0 { 16.0 } else { 13.0 };
    let pintor = ui.painter();
    let g_valor = pintor.layout_no_wrap(valor.to_owned(), forte(tamanho), p.texto);
    let g_rotulo = pintor.layout_no_wrap(rotulo.to_owned(), FontId::proportional(rotulo_tamanho), p.suave);
    let recuo = if ponto.is_some() { 14.0 } else { 0.0 };
    let largura = g_valor.size().x.max(g_rotulo.size().x + recuo);
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura, g_valor.size().y + 4.0 + g_rotulo.size().y), Sense::hover());
    let pintor = ui.painter();
    pintor.galley(rect.min, g_valor.clone(), p.texto);
    let y = rect.top() + g_valor.size().y + 4.0;
    if let Some((cor, forma)) = ponto {
        marca(pintor, pos2(rect.left() + 4.0, y + g_rotulo.size().y / 2.0), 4.5, cor, forma);
    }
    pintor.galley(pos2(rect.left() + recuo, y), g_rotulo, p.suave);
    resposta
}

/// Filtro de liga e desliga: o desenho do `chip`, sem a seta (que quer dizer menu).
pub fn chip_alternar(ui: &mut egui::Ui, rotulo: &str, ativo: bool) -> Response {
    let p = cores();
    let fonte = if ativo { forte(13.0) } else { FontId::proportional(13.0) };
    let galeria = ui.painter().layout_no_wrap(rotulo.to_owned(), fonte, p.texto);
    // A largura é a do texto em negrito nos dois estados: ligar não empurra os vizinhos.
    let largura = ui.painter().layout_no_wrap(rotulo.to_owned(), forte(13.0), p.texto).size().x;
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura + 28.0, 32.0), Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let (fundo, borda) = if ativo {
        (p.destaque.gamma_multiply(if claro() { 0.12 } else { 0.18 }), p.destaque.gamma_multiply(0.6))
    } else if resposta.hovered() {
        (p.realce, p.borda)
    } else {
        (p.superficie_alta, p.borda)
    };
    ui.painter().rect(rect, CornerRadius::same(16), fundo, Stroke::new(1.0, borda), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galeria.size() / 2.0, galeria, p.texto);
    resposta
}

/// Uma tecla desenhada (no painel de atalhos e nas dicas).
pub fn tecla(ui: &mut egui::Ui, texto: &str) -> Response {
    let p = cores();
    // Uma letra sozinha vai na fonte do texto: o "O" na monoespaçada parece zero.
    let letra = texto.chars().count() == 1 && texto.chars().all(char::is_alphabetic);
    let fonte = if letra { forte(12.5) } else { FontId::monospace(12.5) };
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), fonte, p.texto);
    let (rect, resposta) = ui.allocate_exact_size(vec2((galeria.size().x + 16.0).max(22.0), 22.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(RAIO_ETIQUETA), p.superficie, Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galeria.size() / 2.0, galeria, p.texto);
    resposta
}

/// Seção que abre e fecha ("Texto da daily", "Galeria"): seta e título, fundo
/// só ao passar o mouse. Devolve se foi clicada (e já troca `aberta`).
pub fn secao_recolhivel(ui: &mut egui::Ui, titulo: &str, aberta: &mut bool) -> Response {
    let p = cores();
    let galeria = ui.painter().layout_no_wrap(titulo.to_owned(), forte(13.5), p.texto);
    let (rect, resposta) = ui.allocate_exact_size(vec2(galeria.size().x + 40.0, 32.0), Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    if resposta.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(RAIO_CONTROLE), p.realce);
    }
    let centro = pos2(rect.left() + 14.0, rect.center().y);
    let pontos = if *aberta {
        vec![centro + vec2(-3.5, -1.5), centro + vec2(3.5, -1.5), centro + vec2(0.0, 2.5)]
    } else {
        vec![centro + vec2(-1.5, -3.5), centro + vec2(-1.5, 3.5), centro + vec2(2.5, 0.0)]
    };
    ui.painter().add(Shape::convex_polygon(pontos, p.suave, Stroke::NONE));
    ui.painter().galley(pos2(rect.left() + 28.0, rect.center().y - galeria.size().y / 2.0), galeria, p.texto);
    if resposta.clicked() {
        *aberta = !*aberta;
    }
    resposta
}

/// Bloco parado do estado "carregando" (desenhado uma vez, sem animação).
pub fn esqueleto(pintor: &egui::Painter, rect: Rect, raio: u8) {
    pintor.rect_filled(rect, CornerRadius::same(raio), cores().superficie);
}

/// Campo de várias linhas, com a moldura do `campo` (e borda de destaque com foco).
pub fn campo_multilinha(ui: &mut egui::Ui, texto: &mut String, linhas: usize, altura_maxima: f32, id: egui::Id, so_leitura: bool) -> Response {
    campo_multilinha_com(ui, texto, linhas, altura_maxima, id, so_leitura, FontId::proportional(13.5))
}

/// O mesmo campo, com a fonte escolhida (a nota do slide é de 18 px).
pub fn campo_multilinha_com(
    ui: &mut egui::Ui,
    texto: &mut String,
    linhas: usize,
    altura_maxima: f32,
    id: egui::Id,
    so_leitura: bool,
    fonte: FontId,
) -> Response {
    let p = cores();
    let com_foco = ui.memory(|m| m.has_focus(id));
    let largura = ui.available_width();
    egui::Frame::new()
        .fill(fundo_campo(p, so_leitura))
        .stroke(if com_foco { Stroke::new(1.5, p.destaque) } else { Stroke::new(1.0, p.borda) })
        .corner_radius(CornerRadius::same(RAIO_CONTROLE))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(altura_maxima)
                .id_salt(id.with("rolagem"))
                .show(ui, |ui| {
                    let cor = if so_leitura { p.suave } else { p.texto };
                    ui.add(
                        egui::TextEdit::multiline(texto)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .desired_rows(linhas)
                            .desired_width(largura - 22.0)
                            .font(fonte)
                            .text_color(cor)
                            .interactive(!so_leitura),
                    )
                })
                .inner
        })
        .inner
}

/// Caixa de marcar da Colmeia: um quadrado de verdade (o checkbox do egui,
/// com o raio dos controles, vira um círculo e lembra uma escolha exclusiva).
pub fn caixa_marcar(ui: &mut egui::Ui, texto: &str, marcada: &mut bool) -> Response {
    let p = cores();
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), FontId::proportional(13.5), p.texto);
    let (rect, mut resposta) = ui.allocate_exact_size(vec2(16.0 + 8.0 + galeria.size().x, 24.0_f32.max(galeria.size().y)), Sense::click());
    if resposta.clicked() {
        *marcada = !*marcada;
        resposta.mark_changed();
    }
    let caixa = Rect::from_center_size(pos2(rect.left() + 8.0, rect.center().y), vec2(16.0, 16.0));
    let raio = CornerRadius::same(4);
    if *marcada {
        ui.painter().rect_filled(caixa, raio, p.destaque);
        let cor = if claro() { Color32::WHITE } else { p.fundo };
        let visto = vec![caixa.left_center() + vec2(3.5, 0.5), caixa.center() + vec2(-1.0, 3.5), caixa.right_center() + vec2(-3.5, -3.5)];
        ui.painter().add(Shape::line(visto, Stroke::new(2.0, cor)));
    } else {
        let borda = if resposta.hovered() { p.texto } else { p.suave };
        ui.painter().rect(caixa, raio, p.superficie, Stroke::new(1.5, borda), egui::StrokeKind::Inside);
    }
    ui.painter().galley(pos2(caixa.right() + 8.0, rect.center().y - galeria.size().y / 2.0), galeria, p.texto);
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Texto em até `linhas` linhas, cortado com "…" (U+2026) no fim. Antes do
/// "…" não fica espaço nem vírgula: "paginação…", nunca "paginação, …". É o
/// único jeito de cortar texto na tela, para todo corte terminar igual.
/// `qualquer_ponto`: corta no meio da palavra (linhas de lista); falso corta
/// entre palavras (títulos, notas).
pub fn cortar(pintor: &egui::Painter, texto: &str, formato: egui::TextFormat, largura: f32, linhas: usize, qualquer_ponto: bool) -> Arc<egui::Galley> {
    let montar = |t: String| {
        let mut trabalho = egui::text::LayoutJob::single_section(t, formato.clone());
        trabalho.wrap =
            egui::text::TextWrapping { max_width: largura.max(10.0), max_rows: linhas.max(1), break_anywhere: qualquer_ponto, overflow_character: Some('…') };
        pintor.layout_job(trabalho)
    };
    let galeria = montar(texto.to_owned());
    if !galeria.elided {
        return galeria;
    }
    // O que coube, sem o "…" e sem espaço ou pontuação no fim.
    let mut coube = String::new();
    for linha in &galeria.rows {
        coube.extend(linha.glyphs.iter().map(|g| g.chr));
        if linha.ends_with_newline {
            coube.push('\n');
        }
    }
    // Tudo junto: o "…" do egui pode vir antes de uma quebra de linha, e o
    // texto antes dele pode terminar em ponto (saía "texto.……").
    let limpo = coube.trim_end_matches(|c: char| c.is_whitespace() || "…,;:·-–.".contains(c));
    let mut galeria = montar(format!("{limpo}…"));
    // O texto refeito é do mesmo tamanho ou menor: cabe; o "elided" segue valendo.
    Arc::make_mut(&mut galeria).elided = true;
    galeria
}

/// Texto em até `linhas` linhas mostrando o FIM: quando não cabe, o começo
/// sai e entra um "…" na frente. Para a nota que o agente complementa (a
/// resposta vem no fim). Se o fim ainda não couber, corta como `cortar`.
pub fn cortar_pelo_fim(pintor: &egui::Painter, texto: &str, formato: egui::TextFormat, largura: f32, linhas: usize) -> Arc<egui::Galley> {
    let linhas = linhas.max(1);
    let mut trabalho = egui::text::LayoutJob::single_section(texto.to_owned(), formato.clone());
    trabalho.wrap = egui::text::TextWrapping { max_width: largura.max(10.0), ..Default::default() };
    let inteiro = pintor.layout_job(trabalho);
    let total = inteiro.rows.len();
    if total <= linhas {
        return cortar(pintor, texto, formato, largura, linhas, false);
    }
    // Pula as primeiras linhas; o "…" pode empurrar uma linha a mais: pula mais uma.
    for pular in (total - linhas)..total {
        let caracteres: usize = inteiro.rows[..pular].iter().map(|r| r.char_count_including_newline().0).sum();
        let resto: String = texto.chars().skip(caracteres).collect();
        let galeria = cortar(pintor, &format!("…{}", resto.trim_start()), formato.clone(), largura, linhas, false);
        if !galeria.elided {
            return galeria;
        }
    }
    cortar(pintor, texto, formato, largura, linhas, false)
}

/// Uma linha em duas partes: o começo pode ser cortado com "…", o fim fica
/// inteiro (a hora, o projeto). `pos` é o canto de cima à esquerda. Diz se cortou.
#[allow(clippy::too_many_arguments)]
pub fn texto_com_fim(pintor: &egui::Painter, pos: Pos2, corta: &str, fim: &str, fonte: FontId, cor: Color32, cor_fim: Color32, largura: f32) -> bool {
    let galeria_fim = pintor.layout_no_wrap(fim.to_owned(), fonte.clone(), cor_fim);
    let galeria = cortar(pintor, corta, egui::TextFormat::simple(fonte, cor), (largura - galeria_fim.size().x).max(24.0), 1, true);
    let cortou = galeria.elided;
    let x_fim = pos.x + galeria.size().x;
    pintor.galley(pos, galeria, cor);
    pintor.galley(pos2(x_fim, pos.y), galeria_fim, cor_fim);
    cortou
}

/// Uma linha cortada pelo começo, com "…" na frente: para o fim de um caminho
/// ou do prompt, onde o que importa está à direita.
pub fn texto_sem_inicio(pintor: &egui::Painter, pos: Pos2, texto: &str, fonte: FontId, cor: Color32, largura: f32) {
    let caber = |t: &str| pintor.layout_no_wrap(t.to_owned(), fonte.clone(), cor).size().x <= largura;
    if caber(texto) {
        pintor.text(pos, egui::Align2::LEFT_TOP, texto, fonte, cor);
        return;
    }
    // Busca binária pelo menor corte do começo que faz "…" + resto caber.
    let inicios: Vec<usize> = texto.char_indices().map(|(i, _)| i).collect();
    let (mut baixo, mut alto) = (0, inicios.len());
    while baixo < alto {
        let meio = (baixo + alto) / 2;
        if caber(&format!("…{}", &texto[inicios[meio]..])) {
            alto = meio;
        } else {
            baixo = meio + 1;
        }
    }
    let resto = inicios.get(baixo).map_or("", |&i| &texto[i..]);
    pintor.text(pos, egui::Align2::LEFT_TOP, format!("…{resto}"), fonte, cor);
}

/// Item de menu com o atalho alinhado à direita e a opção de ficar inativo
/// (sem fundo ao passar o mouse e sem clique).
pub fn opcao_menu_com(ui: &mut egui::Ui, texto: &str, atalho: Option<&str>, ativa: bool) -> bool {
    let p = cores();
    let cor = if ativa { p.texto } else { p.suave };
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), FontId::proportional(13.5), cor);
    let atalho = atalho.map(|a| ui.painter().layout_no_wrap(a.to_owned(), FontId::proportional(12.0), p.suave));
    let largura = (galeria.size().x + atalho.as_ref().map_or(0.0, |a| a.size().x + 24.0) + 40.0).max(ui.min_rect().width());
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura, 32.0), if ativa { Sense::click() } else { Sense::hover() });
    if ativa && resposta.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(RAIO_ETIQUETA), p.realce);
    }
    ui.painter().galley(pos2(rect.left() + 14.0, rect.center().y - galeria.size().y / 2.0), galeria, cor);
    if let Some(a) = atalho {
        ui.painter().galley(pos2(rect.right() - 14.0 - a.size().x, rect.center().y - a.size().y / 2.0), a, p.suave);
    }
    ativa && resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Tipo do aviso do rodapé: muda a borda e o ponto.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TipoAviso {
    Neutro,
    Alerta,
    Erro,
}

/// Desenha o aviso do rodapé ancorado em `ancora` (centro de baixo) e diz se a
/// ação dele foi clicada.
/// O que a pessoa fez no aviso do rodapé.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CliqueAviso {
    Nada,
    Acao,
    Fechar,
}

/// Aviso do rodapé. `fechavel` põe um × no fim (os avisos que ficam até a
/// pessoa ver).
pub fn aviso(ctx: &egui::Context, ancora: Pos2, tipo: TipoAviso, texto: &str, acao: Option<&str>, fechavel: bool) -> CliqueAviso {
    let p = cores();
    let (borda, ponto) = match tipo {
        TipoAviso::Neutro => (Stroke::new(1.0, p.borda), None),
        TipoAviso::Alerta => (Stroke::new(1.0, p.alerta.gamma_multiply(0.6)), Some(p.alerta)),
        TipoAviso::Erro => (Stroke::new(1.0, p.erro), Some(p.erro)),
    };
    let mut clicou = CliqueAviso::Nada;
    egui::Area::new(egui::Id::new("aviso-rodape")).order(egui::Order::Foreground).pivot(egui::Align2::CENTER_BOTTOM).fixed_pos(ancora).show(ctx, |ui| {
        egui::Frame::new()
            .fill(p.superficie_alta)
            .stroke(borda)
            .corner_radius(CornerRadius::same(RAIO_SUPERFICIE))
            .inner_margin(egui::Margin::symmetric(16, 10))
            .shadow(sombra(6, 18))
            .show(ui, |ui| {
                ui.set_max_width(560.0);
                ui.horizontal(|ui| {
                    // A altura do botão reservada antes do texto: tudo no mesmo eixo.
                    if acao.is_some() {
                        ui.set_min_height(32.0);
                    }
                    if let Some(cor) = ponto {
                        let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 3.5, cor);
                    }
                    ui.label(RichText::new(texto).color(p.texto).size(13.5));
                    if let Some(a) = acao {
                        ui.add_space(12.0);
                        if botao_secundario(ui, a).clicked() {
                            clicou = CliqueAviso::Acao;
                        }
                    }
                    if fechavel {
                        ui.add_space(4.0);
                        if botao_icone(ui, Icone::Fechar, 24.0).on_hover_text("Fechar o aviso").clicked() {
                            clicou = CliqueAviso::Fechar;
                        }
                    }
                });
            });
    });
    clicou
}

/// Sombra das peças que flutuam (avisos, cartão de fim, pílula).
pub fn sombra(deslocamento: i8, borrao: u8) -> egui::Shadow {
    egui::Shadow { offset: [0, deslocamento], blur: borrao, spread: 0, color: Color32::from_black_alpha(if claro() { 40 } else { 90 }) }
}

// Lousa: as cores fixas do papel das notas e do bloco de código, e a malha
// de pontos do quadro infinito.

/// As seis cores da nota: a chave do núcleo, o nome da dica e a base pastel
/// (no Claro e no Leitura; no Escuro ela se mistura 16% ao fundo, para não
/// ofuscar a tela escura nem o telão).
pub const NOTA_BASE: [(&str, &str, u32); 6] = [
    ("amarelo", "Amarelo", 0xfbefa6),
    ("azul", "Azul", 0xcfe3fb),
    ("verde", "Verde", 0xd3efd3),
    ("rosa", "Rosa", 0xf9d6dc),
    ("lilas", "Lilás", 0xe3d9f7),
    ("cinza", "Cinza", 0xe3e6eb),
];

/// As cores de uma nota: o papel, a tinta (sempre escura), o secundário
/// (título, marcador, contador), a linha (borda, tabela) e o recuo (faixa do
/// título, cabeçalho da tabela, código dentro da nota).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoresNota {
    pub fundo: Color32,
    pub tinta: Color32,
    pub secundaria: Color32,
    pub linha: Color32,
    pub recuo: Color32,
}

/// As cores da nota no tema atual.
pub fn cor_nota(chave: &str) -> CoresNota {
    cor_nota_em(ATUAL.load(Ordering::Relaxed), chave)
}

fn cor_nota_em(tema: u8, chave: &str) -> CoresNota {
    let base = rgb(NOTA_BASE.iter().find(|(c, _, _)| *c == chave).map_or(NOTA_BASE[0].2, |(_, _, v)| *v));
    let fundo = if tema == 0 { misturar(base, ESCURO.fundo, 0.16) } else { base };
    let tinta = if tema == 2 { LEITURA.texto } else { CLARO.texto };
    CoresNota { fundo, tinta, secundaria: misturar(tinta, fundo, 0.26), linha: misturar(fundo, tinta, 0.18), recuo: misturar(fundo, tinta, 0.07) }
}

/// A borda da bolinha de cor (barra da seleção, menu): a cor da nota puxada
/// para a tinta, para as bolinhas claras não sumirem no painel dos temas claros.
pub fn borda_da_bolinha(c: &CoresNota) -> Color32 {
    misturar(c.fundo, c.tinta, 0.3)
}

/// O bloco de código da lousa: sempre escuro, a cara de "trecho de terminal".
pub struct CoresCodigo {
    pub fundo: Color32,
    pub texto: Color32,
    pub suave: Color32,
    pub borda: Color32,
}

pub const CODIGO: CoresCodigo = CoresCodigo { fundo: ESCURO.superficie_alta, texto: ESCURO.terminal_texto, suave: ESCURO.suave, borda: ESCURO.realce };

/// A malha de pontos da lousa, presa ao quadro (anda com a vista). Refeita só
/// quando a câmera, a área ou o tema mudam: parada, não custa nada.
#[derive(Default)]
pub struct Grade {
    feita_para: Option<(Rect, [i32; 3], Color32)>,
    malha: Option<Arc<Mesh>>,
}

impl Grade {
    /// `origem`: o ponto do quadro no canto da área; `zoom`: a escala.
    pub fn desenhar(&mut self, pintor: &egui::Painter, area: Rect, origem: Pos2, zoom: f32) {
        let cor = cores().favo;
        // Pontos a cada 24 do quadro; entre 25% e 50%, a cada 96; abaixo de 25%, nenhum.
        let passo = if zoom >= 0.5 {
            24.0
        } else if zoom >= 0.25 {
            96.0
        } else {
            return;
        };
        let na_tela = passo * zoom;
        let fase = vec2((-origem.x).rem_euclid(passo) * zoom, (-origem.y).rem_euclid(passo) * zoom);
        let chave = [(zoom * 1000.0) as i32, (fase.x * 4.0) as i32, (fase.y * 4.0) as i32];
        if self.feita_para != Some((area, chave, cor)) {
            let mut malha = Mesh::default();
            let mut y = area.top() + fase.y;
            while y <= area.bottom() {
                let mut x = area.left() + fase.x;
                while x <= area.right() {
                    malha.add_colored_rect(Rect::from_center_size(pos2(x, y), vec2(2.0, 2.0)), cor);
                    x += na_tela;
                }
                y += na_tela;
            }
            self.malha = Some(Arc::new(malha));
            self.feita_para = Some((area, chave, cor));
        }
        if let Some(malha) = &self.malha {
            pintor.add(Shape::Mesh(malha.clone()));
        }
    }
}

/// Fundo em favo de mel, calculado uma vez por tamanho e tema e reaproveitado
/// em todo quadro: desenhar a malha pronta não custa quase nada.
#[derive(Default)]
pub struct Favo {
    feito_para: Option<(Rect, Color32)>,
    malha: Option<Arc<Mesh>>,
}

impl Favo {
    pub fn desenhar(&mut self, pintor: &egui::Painter, area: Rect) {
        let cor = cores().favo;
        if self.feito_para != Some((area, cor)) {
            self.malha = Some(Arc::new(malha_favo(area, cor)));
            self.feito_para = Some((area, cor));
        }
        if let Some(malha) = &self.malha {
            pintor.add(Shape::Mesh(malha.clone()));
        }
    }
}

fn malha_favo(area: Rect, cor: Color32) -> Mesh {
    let raio = 26.0;
    let largura = 3.0_f32.sqrt() * raio;
    let passo_y = 1.5 * raio;
    let mut malha = Mesh::default();
    let vertice = |c: Pos2, i: usize| {
        let angulo = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 90.0);
        c + vec2(angulo.cos() * raio, angulo.sin() * raio)
    };
    let linhas = (area.height() / passo_y) as i32 + 2;
    let colunas = (area.width() / largura) as i32 + 2;
    for l in -1..linhas {
        for c in -1..colunas {
            let deslocamento = if l.rem_euclid(2) == 1 { largura / 2.0 } else { 0.0 };
            let centro = pos2(area.left() + c as f32 * largura + deslocamento, area.top() + l as f32 * passo_y);
            // Três lados por célula bastam: os outros três são das vizinhas,
            // e assim nenhuma linha é desenhada duas vezes.
            for i in 0..3 {
                linha_suave(&mut malha, vertice(centro, i), vertice(centro, i + 1), 1.0, cor);
            }
        }
    }
    malha
}

/// Linha com borda suavizada (1 px que desbota até transparente), em triângulos.
fn linha_suave(malha: &mut Mesh, a: Pos2, b: Pos2, espessura: f32, cor: Color32) {
    let direcao = (b - a).normalized();
    let normal = vec2(-direcao.y, direcao.x);
    let (miolo, borda) = (normal * espessura / 2.0, normal * (espessura / 2.0 + 1.0));
    let base = malha.vertices.len() as u32;
    let transparente = Color32::TRANSPARENT;
    for (ponto, deslocamento, c) in [
        (a, -borda, transparente),
        (a, -miolo, cor),
        (a, miolo, cor),
        (a, borda, transparente),
        (b, -borda, transparente),
        (b, -miolo, cor),
        (b, miolo, cor),
        (b, borda, transparente),
    ] {
        malha.colored_vertex(ponto + deslocamento, c);
    }
    for i in 0..3 {
        malha.add_triangle(base + i, base + i + 1, base + i + 4);
        malha.add_triangle(base + i + 1, base + i + 5, base + i + 4);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn temas_vao_e_voltam_pela_chave_do_perfil() {
        for e in Escolha::TODAS {
            assert_eq!(Escolha::da_chave(e.chave()), e);
        }
        assert_eq!(Escolha::da_chave("sistema"), Escolha::Escuro);
    }

    /// Contraste WCAG entre duas cores (1 a 21).
    fn contraste(a: Color32, b: Color32) -> f32 {
        let canal = |v: u8| {
            let c = v as f32 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        let luz = |c: Color32| 0.2126 * canal(c.r()) + 0.7152 * canal(c.g()) + 0.0722 * canal(c.b());
        let (x, y) = (luz(a), luz(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn textos_de_estado_sao_legiveis_nos_cartoes() {
        // Os estados aparecem em texto de 12 px sobre os cartões: pelo menos 4,5:1.
        for (nome, p) in [("escuro", &ESCURO), ("claro", &CLARO), ("leitura", &LEITURA)] {
            for (cor, c) in [("ok", p.ok), ("alerta", p.alerta), ("erro", p.erro), ("suave", p.suave)] {
                let r = contraste(c, p.superficie_alta);
                assert!(r >= 4.5, "{cor} no tema {nome}: {r:.2}");
            }
        }
    }

    #[test]
    fn suave_e_legivel_sobre_o_fundo_e_os_cartoes() {
        // `suave` (horas, títulos de tarefa removida, rótulos) só vai sobre o
        // fundo e o cartão: sobre `superficie` não passa de 4,5:1 nos claros.
        for (nome, p) in [("escuro", &ESCURO), ("claro", &CLARO), ("leitura", &LEITURA)] {
            for (onde, fundo) in [("fundo", p.fundo), ("superficie_alta", p.superficie_alta)] {
                let r = contraste(p.suave, fundo);
                assert!(r >= 4.5, "suave sobre {onde} no tema {nome}: {r:.2}");
            }
        }
    }

    #[test]
    fn alerta_se_separa_do_destaque_e_do_erro() {
        // Os pontos de "Aguardando você", "Concluídas" e "Com erro" lado a lado:
        // o alerta precisa de matiz própria (mais de 20° de distância).
        let matiz = |c: Color32| {
            let (r, g, b) = (c.r() as f32, c.g() as f32, c.b() as f32);
            let (max, min) = (r.max(g).max(b), r.min(g).min(b));
            let d = (max - min).max(1.0);
            let h = if max == r {
                ((g - b) / d).rem_euclid(6.0)
            } else if max == g {
                (b - r) / d + 2.0
            } else {
                (r - g) / d + 4.0
            };
            h * 60.0
        };
        for (nome, p) in [("escuro", &ESCURO), ("claro", &CLARO), ("leitura", &LEITURA)] {
            for (outra, c) in [("destaque", p.destaque), ("erro", p.erro)] {
                let distancia = (matiz(p.alerta) - matiz(c)).abs();
                assert!(distancia.min(360.0 - distancia) > 20.0, "alerta e {outra} no tema {nome}: {distancia:.0}°");
            }
        }
        // No leitura o alerta também vai sobre o fundo (capa, números).
        assert!(contraste(LEITURA.alerta, LEITURA.fundo) >= 4.5, "alerta sobre o fundo no tema leitura");
    }

    #[test]
    fn corte_termina_com_reticencias_sem_espaco_nem_virgula() {
        let ctx = egui::Context::default();
        let mut fim = String::new();
        let mut saida = ctx.run_ui(egui::RawInput::default(), |ui| {
            let pintor = ui.painter().clone();
            let formato = egui::TextFormat::simple(FontId::proportional(13.0), Color32::WHITE);
            let largura = pintor.layout_no_wrap("Migrar os filtros da listagem, ".into(), FontId::proportional(13.0), Color32::WHITE).size().x + 4.0;
            let g = cortar(&pintor, "Migrar os filtros da listagem, com paginação e cache", formato, largura, 1, false);
            fim = g.rows.iter().flat_map(|r| r.glyphs.iter().map(|g| g.chr)).collect();
        });
        saida.textures_delta.clear();
        assert!(fim.ends_with('…') && !fim.ends_with(" …") && !fim.ends_with(",…"), "{fim:?}");
    }

    #[test]
    fn corte_de_linha_que_termina_em_ponto_tem_um_so_reticencias() {
        let ctx = egui::Context::default();
        let mut textos = Vec::new();
        let mut saida = ctx.run_ui(egui::RawInput::default(), |ui| {
            let pintor = ui.painter().clone();
            let formato = egui::TextFormat::simple(FontId::proportional(13.0), Color32::WHITE);
            for texto in ["Primeira linha da nota.\nSegunda linha.\nTerceira.", "Uma frase que termina em ponto. E continua por mais um bom pedaço de texto"] {
                let g = cortar(&pintor, texto, formato.clone(), 200.0, 1, false);
                textos.push(g.rows.iter().flat_map(|r| r.glyphs.iter().map(|g| g.chr)).collect::<String>());
            }
        });
        saida.textures_delta.clear();
        for fim in textos {
            assert!(fim.ends_with('…') && !fim.ends_with(".…") && !fim.ends_with("……"), "{fim:?}");
        }
    }

    #[test]
    fn corte_pelo_fim_mostra_o_que_o_agente_acrescentou() {
        let ctx = egui::Context::default();
        let (mut curto, mut longo) = (String::new(), String::new());
        let mut linhas = 0;
        let mut saida = ctx.run_ui(egui::RawInput::default(), |ui| {
            let pintor = ui.painter().clone();
            let formato = egui::TextFormat::simple(FontId::proportional(13.0), Color32::WHITE);
            let texto = |g: &egui::Galley| g.rows.iter().flat_map(|r| r.glyphs.iter().map(|g| g.chr)).collect::<String>();
            curto = texto(&cortar_pelo_fim(&pintor, "Só uma linha", formato.clone(), 400.0, 2));
            let nota = "Primeiro parágrafo.\nSegundo parágrafo.\nTerceiro parágrafo.\nQuarto.\nCobertura de testes: 87%";
            let g = cortar_pelo_fim(&pintor, nota, formato, 400.0, 2);
            linhas = g.rows.len();
            longo = texto(&g);
        });
        saida.textures_delta.clear();
        assert_eq!(curto, "Só uma linha");
        assert_eq!(linhas, 2);
        assert!(longo.starts_with('…') && longo.ends_with("Cobertura de testes: 87%"), "{longo:?}");
    }

    #[test]
    fn faixas_tingidas_sao_opacas_e_legiveis() {
        // A faixa "Núcleo desconectado" (alerta) e a de núcleo antigo (erro):
        // texto do tema sobre o fundo tingido, pelo menos 4,5:1.
        for (nome, p, eh_claro) in [("escuro", &ESCURO, false), ("claro", &CLARO, true), ("leitura", &LEITURA, true)] {
            for (cor, c) in [("alerta", p.alerta), ("erro", p.erro)] {
                let fundo = fundo_tingido(p, c, eh_claro);
                assert_eq!(fundo.a(), 255, "faixa de {cor} no tema {nome} não é opaca");
                let r = contraste(p.texto, fundo);
                assert!(r >= 4.5, "texto na faixa de {cor} no tema {nome}: {r:.2}");
            }
        }
    }

    #[test]
    fn pilulas_sao_legiveis_em_todos_os_temas() {
        // A pílula de estado põe a palavra em `texto` sobre o fundo tingido
        // da cor do estado: pelo menos 4,5:1 com destaque e ok também.
        for (nome, p, eh_claro) in [("escuro", &ESCURO, false), ("claro", &CLARO, true), ("leitura", &LEITURA, true)] {
            for (cor, c) in [("destaque", p.destaque), ("ok", p.ok), ("suave", p.suave), ("alerta", p.alerta), ("erro", p.erro)] {
                let r = contraste(p.texto, fundo_tingido(p, c, eh_claro));
                assert!(r >= 4.5, "texto na pílula de {cor} no tema {nome}: {r:.2}");
            }
        }
    }

    #[test]
    fn linha_escolhida_e_calha_do_terminal_sao_legiveis() {
        // A linha escolhida da árvore leva `texto`; os números de linha da
        // pré-visualização vão em `suave` sobre o fundo do terminal.
        for (nome, p, eh_claro) in [("escuro", &ESCURO, false), ("claro", &CLARO, true), ("leitura", &LEITURA, true)] {
            let escolhido = escolhido_em(p, eh_claro);
            let r = contraste(p.texto, escolhido);
            assert!(r >= 4.5, "texto na linha escolhida no tema {nome}: {r:.2}");
            // A escolhida se afasta do fundo pelo menos tanto quanto o realce
            // do mouse: com o mouse em outra linha, ela não some.
            let distancia = |c: Color32| {
                let (a, b) = (c, p.superficie_alta);
                (a.r() as f32 - b.r() as f32).abs() + (a.g() as f32 - b.g() as f32).abs() + (a.b() as f32 - b.b() as f32).abs()
            };
            assert!(distancia(escolhido) >= distancia(p.realce), "linha escolhida mais fraca que o realce no tema {nome}");
            let r = contraste(p.suave, p.terminal_fundo);
            assert!(r >= 4.5, "suave sobre o terminal no tema {nome}: {r:.2}");
        }
    }

    #[test]
    fn temas_claros_tem_os_mesmos_tamanhos_do_escuro() {
        let ctx = egui::Context::default();
        instalar(&ctx);
        let (escuro, claro) = (ctx.style_of(Theme::Dark), ctx.style_of(Theme::Light));
        assert_eq!(escuro.text_styles, claro.text_styles);
        assert_eq!(escuro.spacing.item_spacing, claro.spacing.item_spacing);
        assert_eq!(escuro.spacing.interact_size, claro.spacing.interact_size);
        // Sem piscar: um campo com foco não redesenha a tela parada.
        assert!(!escuro.visuals.text_cursor.blink && !claro.visuals.text_cursor.blink);
    }

    #[test]
    fn notas_sao_legiveis_em_todos_os_temas() {
        for tema in 0..3u8 {
            for (chave, _, _) in NOTA_BASE {
                let c = cor_nota_em(tema, chave);
                let tinta = contraste(c.tinta, c.fundo);
                let secundaria = contraste(c.secundaria, c.fundo);
                assert!(tinta >= 7.0, "tinta na nota {chave} no tema {tema}: {tinta:.2}");
                assert!(secundaria >= 4.5, "secundária na nota {chave} no tema {tema}: {secundaria:.2}");
            }
        }
    }

    #[test]
    fn bolinhas_de_cor_aparecem_nos_temas_claros() {
        for (tema, p) in [(1u8, &CLARO), (2, &LEITURA)] {
            for (chave, _, _) in NOTA_BASE {
                let borda = borda_da_bolinha(&cor_nota_em(tema, chave));
                for (nome, fundo) in [("superfície", p.superficie), ("superfície alta", p.superficie_alta)] {
                    let r = contraste(borda, fundo);
                    assert!(r >= 1.5, "bolinha {chave} no tema {tema} sobre a {nome}: {r:.2}");
                }
            }
        }
    }

    #[test]
    fn nota_se_separa_do_fundo() {
        for (tema, p) in [(1u8, &CLARO), (2, &LEITURA)] {
            for (chave, _, _) in NOTA_BASE {
                let r = contraste(cor_nota_em(tema, chave).linha, p.fundo);
                assert!(r >= 1.35, "borda da nota {chave} no tema {tema}: {r:.2}");
            }
        }
        for (chave, _, _) in NOTA_BASE {
            let r = contraste(cor_nota_em(0, chave).fundo, ESCURO.fundo);
            assert!(r >= 7.0, "nota {chave} no escuro: {r:.2}");
        }
    }

    #[test]
    fn codigo_e_legivel_e_aparece_no_escuro() {
        assert!(contraste(CODIGO.texto, CODIGO.fundo) >= 4.5);
        assert!(contraste(CODIGO.suave, CODIGO.fundo) >= 4.5);
        let luz = |c: Color32| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
        assert!((luz(CODIGO.fundo) - luz(ESCURO.fundo)).abs() >= (luz(ESCURO.superficie) - luz(ESCURO.fundo)).abs());
    }

    #[test]
    fn selecao_aparece_no_fundo() {
        for (nome, p) in [("escuro", &ESCURO), ("claro", &CLARO), ("leitura", &LEITURA)] {
            let r = contraste(p.destaque, p.fundo);
            assert!(r >= 3.0, "destaque sobre o fundo no tema {nome}: {r:.2}");
        }
    }

    #[test]
    fn texto_e_fundo_tem_contraste_em_todos_os_temas() {
        // Diferença de luminosidade simples entre texto e fundo, para nenhum tema ficar ilegível.
        let luz = |c: Color32| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
        for p in [&ESCURO, &CLARO, &LEITURA] {
            assert!((luz(p.texto) - luz(p.fundo)).abs() > 150.0);
            assert!((luz(p.terminal_texto) - luz(p.terminal_fundo)).abs() > 150.0);
        }
    }
}
