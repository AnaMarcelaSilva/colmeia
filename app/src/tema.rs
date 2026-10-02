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

/// Cantos: controles, superfícies (colunas, cartões, janelas) e chips.
pub const RAIO_CONTROLE: u8 = 8;
pub const RAIO_SUPERFICIE: u8 = 12;

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
    ok: rgb(0x23915a),
    alerta: rgb(0xc2780e),
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
    alerta: rgb(0xa86a0c),
    erro: rgb(0xb3372d),
    favo: Color32::from_rgba_premultiplied(0, 0, 0, 12),
    terminal_fundo: rgb(0xf7f1e3),
    terminal_texto: rgb(0x3a2f24),
    ansi: [
        rgb(0x3a2f24), rgb(0xb3372d), rgb(0x4d7a36), rgb(0x8a6410), rgb(0x3b5e8c), rgb(0x86457a), rgb(0x2f7470), rgb(0x77695a),
        rgb(0x8a7c6a), rgb(0xc7473c), rgb(0x5e8f45), rgb(0xa07418), rgb(0x4a70a3), rgb(0x9a568d), rgb(0x3a8781), rgb(0x3a2f24),
    ],
};

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
    ctx.global_style_mut(|s| {
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
        .fill(p.superficie)
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
        .corner_radius(CornerRadius::same(16))
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
    let p = cores();
    let galeria = ui.painter().layout_no_wrap(texto.to_owned(), FontId::proportional(13.0), p.texto);
    let (rect, resposta) = ui.allocate_exact_size(vec2(galeria.size().x + 28.0, 32.0), Sense::click());
    let fundo = if resposta.hovered() { p.realce } else { p.superficie_alta };
    ui.painter().rect(rect, CornerRadius::same(16), fundo, Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galeria.size() / 2.0, galeria, p.texto);
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
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
