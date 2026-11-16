//! O que a abelha desenha além dela mesma: o ponto ou o contador, o cartão
//! do resumo e o balão. Sempre escuros, com a borda roxa da marca: ficam por
//! cima de qualquer janela, em qualquer tema.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};

use crate::conversa::{self, Bloco, Ferramenta};
use crate::resumo::{Aviso, Contagem, Escolha, Tipo};

/// As cores da abelha, escolhidas por ela mesma (independente do tema da
/// Colmeia): o mesmo jogo de quatro da Colmeia, nos tons que a abelha usa.
pub struct Paleta {
    pub nome: &'static str,
    /// Tema claro: os véus (fundos e fios sutis) ficam escuros em vez de brancos.
    pub clara: bool,
    pub fundo: Color32,
    pub borda: Color32,
    pub borda_clara: Color32,
    pub destaque: Color32,
    pub destaque_claro: Color32,
    pub texto: Color32,
    pub suave: Color32,
    pub erro: Color32,
    pub alerta: Color32,
    pub ok: Color32,
    /// O azul do Concluído, como na Colmeia (longe do roxo da Revisão).
    pub concluido: Color32,
    /// O fundo das listas que abrem por cima do cartão.
    pub fundo_lista: Color32,
    pub erro_selo: Color32,
    /// A tinta do texto sobre os botões cheios (destaque, alerta).
    pub tinta: Color32,
}

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

#[rustfmt::skip]
pub const TEMAS: [Paleta; 4] = [
    Paleta { nome: "Escuro", clara: false, fundo: Color32::from_rgba_premultiplied(0x16, 0x14, 0x1f, 0xf5), borda: rgb(0x3a3550), borda_clara: rgb(0x5a556e),
        destaque: rgb(0xae8ff7), destaque_claro: rgb(0xc0a7fa), texto: rgb(0xf2f0f8), suave: rgb(0xa9a4bb), erro: rgb(0xf4707c), alerta: rgb(0xf1b65f),
        ok: rgb(0x6fd49b), concluido: rgb(0x72b7f5), fundo_lista: rgb(0x1f1c2b), erro_selo: rgb(0xc8344a), tinta: rgb(0x1a1722) },
    Paleta { nome: "Claro", clara: true, fundo: Color32::from_rgba_premultiplied(0xfb, 0xfb, 0xfe, 0xf7), borda: rgb(0xd8dbe8), borda_clara: rgb(0xb9bdd0),
        destaque: rgb(0x6a43d4), destaque_claro: rgb(0x7d5ae0), texto: rgb(0x181b26), suave: rgb(0x596075), erro: rgb(0xc22f45), alerta: rgb(0x8f5600),
        ok: rgb(0x14703f), concluido: rgb(0x1f5fb0), fundo_lista: rgb(0xffffff), erro_selo: rgb(0xc22f45), tinta: rgb(0xffffff) },
    Paleta { nome: "Leitura", clara: true, fundo: Color32::from_rgba_premultiplied(0xfa, 0xf5, 0xea, 0xf7), borda: rgb(0xd8c9a8), borda_clara: rgb(0xbfae8c),
        destaque: rgb(0x7a4486), destaque_claro: rgb(0x8d569a), texto: rgb(0x36291e), suave: rgb(0x6a5b4a), erro: rgb(0xad3429), alerta: rgb(0x7a5c00),
        ok: rgb(0x3f6829), concluido: rgb(0x2f5c8a), fundo_lista: rgb(0xfffcf4), erro_selo: rgb(0xad3429), tinta: rgb(0xffffff) },
    Paleta { nome: "Fosco", clara: false, fundo: Color32::from_rgba_premultiplied(0x21, 0x21, 0x26, 0xf5), borda: rgb(0x3b3b44), borda_clara: rgb(0x55555f),
        destaque: rgb(0xab9ad9), destaque_claro: rgb(0xbcaee3), texto: rgb(0xdbd9d2), suave: rgb(0xa6a49c), erro: rgb(0xdc8e95), alerta: rgb(0xd8b277),
        ok: rgb(0x93c29b), concluido: rgb(0x8fb9e6), fundo_lista: rgb(0x2a2a31), erro_selo: rgb(0xb84a58), tinta: rgb(0x1a1a1e) },
];

/// O tema em uso (o índice em TEMAS), lido a cada desenho.
static TEMA: AtomicUsize = AtomicUsize::new(0);

pub fn tema() -> &'static Paleta {
    &TEMAS[TEMA.load(Ordering::Relaxed).min(TEMAS.len() - 1)]
}

pub fn escolher_tema(i: usize) {
    TEMA.store(i.min(TEMAS.len() - 1), Ordering::Relaxed);
}

/// O índice do tema com este nome, se existe.
pub fn tema_chamado(nome: &str) -> Option<usize> {
    TEMAS.iter().position(|t| t.nome == nome)
}

/// Um véu para fundos e fios sutis: branco nos temas escuros, preto nos claros.
fn veu(alpha: u8) -> Color32 {
    if tema().clara { Color32::from_black_alpha(alpha) } else { Color32::from_white_alpha(alpha) }
}

const RAIO: u8 = 12;
const RAIO_BOTAO: u8 = 8;
/// A margem de dentro do resumo.
const MARGEM: f32 = 16.0;
/// Um item do resumo: duas linhas, como uma tarefa no cartão da lateral.
const ALTURA_DESTAQUE: f32 = 46.0;
/// Quantos itens o resumo mostra; os outros viram "e mais N" (estão no quadro).
pub const MAX_DESTAQUES: usize = 3;
const VAO_DESTAQUE: f32 = 2.0;
/// A bolinha do agente no item do resumo.
const RAIO_BOLINHA: f32 = 9.0;
const LINHA_MAIS: f32 = 24.0;

pub const LARGURA_CARTAO: f32 = 360.0;

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
        let mut job = egui::text::LayoutJob::simple(texto.trim().to_owned(), fonte, tema().texto, largura);
        job.wrap.max_rows = 2;
        ctx.fonts_mut(|f| f.layout_job(job)).rows.len().clamp(1, 2)
    };
    let titulo = medir(&aviso.titulo, forte(14.0));
    let detalhe = (!aviso.detalhe.trim().is_empty()).then(|| medir(&aviso.detalhe, FontId::proportional(12.5)));
    altura_com(titulo, detalhe)
}

/// A altura do resumo: com quantos itens pedem você e a caixa para escrever à abelha.
/// Cabeçalho e contagem (60), o meio, o fio (16 + 1 + 12) e os botões com a margem de baixo.
pub fn altura_resumo(itens: usize, com_caixa: bool) -> f32 {
    let mostrados = itens.min(MAX_DESTAQUES);
    let meio = if mostrados == 0 {
        18.0
    } else {
        let mais = if itens > MAX_DESTAQUES { LINHA_MAIS } else { 0.0 };
        mostrados as f32 * ALTURA_DESTAQUE + (mostrados - 1) as f32 * VAO_DESTAQUE + mais
    };
    let caixa = if com_caixa { 10.0 + ALTURA_CAIXA } else { 0.0 };
    MARGEM + 60.0 + meio + caixa + 29.0 + ALTURA_BOTAO + 12.0
}

/// A caixa do resumo para escrever à abelha (com os comandos ou a pergunta livre).
const ALTURA_CAIXA: f32 = 34.0;

/// A mesma Inter da Colmeia, regular e seminegrito. As fontes do egui ficam
/// de reserva para os símbolos que ela não tem.
pub fn instalar_fontes(ctx: &egui::Context) {
    let mut fontes = egui::FontDefinitions::default();
    fontes.font_data.insert("inter".into(), Arc::new(egui::FontData::from_static(include_bytes!("../../app/fontes/Inter-Regular.ttf"))));
    fontes.font_data.insert("inter-forte".into(), Arc::new(egui::FontData::from_static(include_bytes!("../../app/fontes/Inter-SemiBold.ttf"))));
    let proporcional = fontes.families.entry(FontFamily::Proportional).or_default();
    proporcional.insert(0, "inter".into());
    // Os traços de caixa (─ │ ┼) que sobram num texto: a mono do egui os tem.
    proporcional.push("Hack".into());
    let reserva = fontes.families[&FontFamily::Proportional].clone();
    fontes.families.insert(FontFamily::Name("forte".into()), std::iter::once("inter-forte".to_owned()).chain(reserva).collect());
    ctx.set_fonts(fontes);
}

fn forte(tamanho: f32) -> FontId {
    FontId::new(tamanho, FontFamily::Name("forte".into()))
}

fn cor(tipo: Tipo) -> Color32 {
    match tipo {
        Tipo::Erro => tema().erro,
        Tipo::Espera => tema().alerta,
        Tipo::Concluiu => tema().concluido,
        Tipo::Terminou => tema().suave,
    }
}

/// O vermelho dos selos: escuro o bastante para o número branco (contraste AA).
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
            pintor.circle_filled(canto, 5.0, tema().ok);
        }
        return;
    };
    let (fundo, letra) = if c.com_erro > 0 { (tema().erro_selo, Color32::WHITE) } else { (tema().alerta, tema().tinta) };
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
    pintor.circle_filled(c, r, tema().destaque.gamma_multiply(0.16 * t));
    pintor.circle_stroke(c, r, Stroke::new(1.5, tema().destaque.gamma_multiply(0.55 * t)));
}

fn cartao(pintor: &egui::Painter, rect: Rect, borda: Color32) {
    // Sombra curta embaixo, para descolar da janela de trás.
    pintor.rect_filled(rect.translate(vec2(0.0, 3.0)), CornerRadius::same(RAIO), Color32::from_black_alpha(60));
    pintor.rect(rect, CornerRadius::same(RAIO), tema().fundo, Stroke::new(1.0, borda), StrokeKind::Inside);
}

#[derive(Clone, Copy, PartialEq)]
enum Estilo {
    /// Cheio, na cor da marca: a ação principal.
    Principal,
    /// Um fundo claro e baixo.
    Secundario,
    /// Só o texto, com fundo ao passar o mouse.
    Discreto,
}

fn largura_botao(ui: &Ui, texto: &str) -> f32 {
    let g = ui.painter().layout_no_wrap(texto.to_owned(), forte(13.0), Color32::PLACEHOLDER);
    (g.size().x + 28.0).max(64.0).round()
}

/// Um botão desenhado aqui: 14 px de cada lado do texto e os cantos de 8.
fn botao(ui: &mut Ui, texto: &str, estilo: Estilo, altura: f32) -> egui::Response {
    let largura = largura_botao(ui, texto);
    let (rect, r) = ui.allocate_exact_size(vec2(largura, altura), Sense::click());
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    let em_cima = r.hovered();
    let (fundo, letra) = match estilo {
        Estilo::Principal => (if em_cima { tema().destaque_claro } else { tema().destaque }, tema().tinta),
        Estilo::Secundario => (veu(if em_cima { 26 } else { 14 }), tema().texto),
        Estilo::Discreto => (if em_cima { veu(14) } else { Color32::TRANSPARENT }, if em_cima { tema().texto } else { tema().suave }),
    };
    let pintor = ui.painter();
    pintor.rect_filled(rect, CornerRadius::same(RAIO_BOTAO), fundo);
    if r.has_focus() {
        pintor.rect_stroke(rect, CornerRadius::same(RAIO_BOTAO), Stroke::new(1.5, tema().destaque_claro), StrokeKind::Outside);
    }
    let g = pintor.layout_no_wrap(texto.to_owned(), forte(13.0), Color32::PLACEHOLDER);
    pintor.galley((rect.center() - g.size() / 2.0).round(), g, letra);
    r
}

/// O Esc num campo com foco só sai do campo (o egui tira o foco); consumido
/// aqui, ele não fecha o cartão junto. O próximo Esc fecha.
fn esc_so_sai_do_campo(ui: &mut Ui, campo: &egui::Response) {
    if campo.lost_focus() {
        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    }
}

/// O "x" de fechar no canto do cartão (28 px de área para o clique).
fn fechar(ui: &mut Ui, canto: Pos2, id: egui::Id) -> bool {
    let rect = Rect::from_center_size(canto, vec2(28.0, 28.0));
    let r = ui.interact(rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let cor = if r.hovered() { tema().texto } else { tema().suave };
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), veu(14));
    }
    let d = 4.0;
    ui.painter().line_segment([rect.center() - vec2(d, d), rect.center() + vec2(d, d)], Stroke::new(1.5, cor));
    ui.painter().line_segment([rect.center() + vec2(-d, d), rect.center() + vec2(d, -d)], Stroke::new(1.5, cor));
    r.clicked()
}

/// A inicial de quem é o agente: "Claude Code (dev)" vira "C" (a primeira
/// letra ou número, como na lateral da Colmeia).
fn inicial(quem: &str) -> String {
    quem.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_default()
}

/// A bolinha de um agente, como na lateral da Colmeia: a inicial da
/// ferramenta com o anel na cor do que ele pede. Aqui ela leva para fora,
/// para a Colmeia, e por isso ganha a marca de "abrir fora" no canto.
fn bolinha(pintor: &egui::Painter, centro: Pos2, quem: &str, anel: Color32, realce: bool) {
    let fundo = veu(if realce { 30 } else { 14 });
    pintor.circle(centro, RAIO_BOLINHA, fundo, Stroke::new(2.0, anel));
    pintor.text(centro, Align2::CENTER_CENTER, inicial(quem), forte(10.0), tema().texto);
    let canto = centro + vec2(6.5, -6.5);
    pintor.circle(canto, 5.0, tema().fundo, Stroke::new(1.0, tema().borda));
    let cor = if realce { tema().texto } else { tema().suave };
    let traco = Stroke::new(1.2, cor);
    pintor.line_segment([canto + vec2(-2.0, 2.0), canto + vec2(2.0, -2.0)], traco);
    pintor.line_segment([canto + vec2(-0.5, -2.0), canto + vec2(2.0, -2.0)], traco);
    pintor.line_segment([canto + vec2(2.0, -2.0), canto + vec2(2.0, 0.5)], traco);
}

/// A bolinha vazia da tarefa sem agente: só o anel, para a coluna da direita
/// ficar alinhada em todas as linhas.
fn bolinha_vazia(pintor: &egui::Painter, centro: Pos2) {
    pintor.circle_stroke(centro, RAIO_BOLINHA, Stroke::new(1.0, tema().borda_clara));
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
    /// O item foi clicado: conversar com o agente dele aqui mesmo (sem
    /// agente rodando, a conversa abre parada, com o botão de ligar).
    Conversar {
        tarefa: i64,
        agente: Option<i64>,
    },
    AbrirColmeia,
    NovaTarefa,
    Desafixar,
}

/// Um item do resumo (e do cartão da volta), como as tarefas no cartão da
/// lateral: o ponto de estado, o título e embaixo o projeto com o que
/// aconteceu; à direita a bolinha do agente. A linha abre a conversa aqui;
/// a bolinha, a Colmeia no agente.
fn destaque(ui: &mut Ui, caixa: Rect, id: (&str, usize), d: &Aviso) -> Option<Pedido> {
    let mut pedido = None;
    let r = ui.interact(caixa, egui::Id::new((id, "caixa")), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    if r.hovered() {
        ui.painter().rect_filled(caixa, CornerRadius::same(8), veu(18));
    }
    // O ponto de estado como no app: o erro ganha um anel de leve em volta.
    let ponto = pos2(caixa.left() + 10.0, caixa.center().y);
    ui.painter().circle_filled(ponto, 3.5, cor(d.tipo));
    if d.tipo == Tipo::Erro {
        ui.painter().circle_stroke(ponto, 6.0, Stroke::new(1.0, cor(d.tipo).gamma_multiply(0.4)));
    }

    let centro = pos2(caixa.right() - 6.0 - RAIO_BOLINHA, caixa.center().y);
    let area = Rect::from_center_size(centro, vec2(RAIO_BOLINHA * 2.0 + 6.0, RAIO_BOLINHA * 2.0 + 6.0));
    let com_agente = d.agente.is_some() && !d.quem.is_empty();
    if com_agente {
        let b = ui.interact(area, egui::Id::new((id, "bolinha")), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        bolinha(ui.painter(), centro, &d.quem, cor(d.tipo), b.hovered());
        if b.on_hover_text(format!("Abrir na Colmeia, no {}", d.quem)).clicked() {
            pedido = Some(Pedido::Abrir { tarefa: d.tarefa, agente: d.agente });
        }
    } else {
        bolinha_vazia(ui.painter(), centro);
    }
    let direita = area.left() - 4.0;

    let x = caixa.left() + 22.0;
    let texto = direita - x;
    let (tarefa, embaixo) = if d.tarefa_titulo.is_empty() {
        (d.titulo.clone(), d.detalhe.clone())
    } else if d.projeto.is_empty() {
        (d.tarefa_titulo.clone(), d.titulo.clone())
    } else {
        (d.tarefa_titulo.clone(), format!("{} · {}", d.projeto, d.titulo))
    };
    linha(ui, pos2(x, caixa.top() + 6.0), &tarefa, forte(13.0), tema().texto, texto);
    linha(ui, pos2(x, caixa.top() + 25.0), &embaixo, FontId::proportional(12.0), tema().suave, texto);
    let dica = if com_agente { "Conversar com o agente aqui" } else { "Abrir a tarefa aqui (sem agente)" };
    if r.on_hover_text(dica).clicked() && pedido.is_none() {
        pedido = Some(Pedido::Conversar { tarefa: d.tarefa, agente: d.agente });
    }
    pedido
}

/// O resumo, fixo até fechar: o nome, a contagem, o que mais pede você, a
/// caixa para escrever à abelha (com `rascunho`) e os botões.
pub fn resumo(ui: &mut Ui, rect: Rect, nome: &str, ligada: bool, c: Contagem, destaques: &[Aviso], rascunho: Option<&mut String>) -> Option<Pedido> {
    let mut pedido = None;
    cartao(ui.painter(), rect, tema().borda);
    let dentro = rect.shrink(MARGEM);
    let largura = dentro.width();
    linha(ui, dentro.left_top(), nome, forte(14.5), tema().texto, largura - 32.0);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 9.0), egui::Id::new("fechar-resumo")) {
        pedido = Some(Pedido::Fechar);
    }

    // "2 esperam você · 1 trabalhando · 0 com erro", o número na cor dele quando passa de zero.
    let y = dentro.top() + 28.0;
    if ligada {
        let partes = [
            (c.esperam, if c.esperam == 1 { "espera você" } else { "esperam você" }, tema().alerta),
            (c.trabalhando, "trabalhando", tema().ok),
            (c.com_erro, "com erro", tema().erro),
        ];
        let mut job = egui::text::LayoutJob::default();
        for (i, (n, texto, cor)) in partes.into_iter().enumerate() {
            if i > 0 {
                job.append("  ·  ", 0.0, egui::TextFormat::simple(FontId::proportional(12.5), tema().borda_clara));
            }
            let numero = if n > 0 { egui::TextFormat::simple(forte(12.5), cor) } else { egui::TextFormat::simple(FontId::proportional(12.5), tema().suave) };
            job.append(&n.to_string(), 0.0, numero);
            job.append(&format!(" {texto}"), 0.0, egui::TextFormat::simple(FontId::proportional(12.5), tema().suave));
        }
        let g = ui.painter().layout_job(job);
        ui.painter().galley(pos2(dentro.left(), y), g, tema().suave);
    } else {
        linha(ui, pos2(dentro.left(), y), "Sem ligação com o núcleo", FontId::proportional(12.5), tema().suave, largura);
    }

    let mut topo = dentro.top() + 60.0;
    for (i, d) in destaques.iter().take(MAX_DESTAQUES).enumerate() {
        if i > 0 {
            topo += VAO_DESTAQUE;
        }
        let caixa = Rect::from_min_size(pos2(dentro.left(), topo), vec2(largura, ALTURA_DESTAQUE));
        if let Some(p) = destaque(ui, caixa, ("destaque", i), d) {
            pedido = Some(p);
        }
        topo = caixa.bottom();
    }
    if destaques.len() > MAX_DESTAQUES {
        let resto = destaques.len() - MAX_DESTAQUES;
        let texto = if resto == 1 { "e mais 1 no quadro".to_string() } else { format!("e mais {resto} no quadro") };
        linha(ui, pos2(dentro.left() + 2.0, topo + 7.0), &texto, FontId::proportional(12.0), tema().suave, largura);
        topo += LINHA_MAIS;
    }
    if destaques.is_empty() {
        linha(ui, pos2(dentro.left(), topo), "Nada precisa de você agora.", FontId::proportional(12.5), tema().suave, largura);
        topo += 18.0;
    }

    if let Some(texto) = rascunho {
        let caixa = Rect::from_min_size(pos2(dentro.left(), topo + 10.0), vec2(largura, ALTURA_CAIXA));
        if caixa_para_a_abelha(ui, caixa, nome, texto) {
            pedido = Some(Pedido::Escrever);
        }
        topo = caixa.bottom();
    }

    // Um fio de ponta a ponta separa os botões do que está acima.
    let fio = topo + 16.0;
    ui.painter().hline(rect.x_range().shrink(1.0), fio + 0.5, Stroke::new(1.0, veu(10)));
    let rodape = Rect::from_min_max(pos2(dentro.left(), fio + 13.0), pos2(dentro.right(), fio + 13.0 + ALTURA_BOTAO));
    ui.scope_builder(UiBuilder::new().max_rect(rodape).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        if botao(ui, "Abrir a Colmeia", Estilo::Principal, ALTURA_BOTAO).clicked() {
            pedido = Some(Pedido::AbrirColmeia);
        }
        ui.add_space(6.0);
        if botao(ui, "+ Tarefa", Estilo::Secundario, ALTURA_BOTAO).on_hover_text("Criar uma tarefa daqui, com ou sem agente").clicked() {
            pedido = Some(Pedido::NovaTarefa);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if botao(ui, "Desafixar", Estilo::Discreto, ALTURA_BOTAO).on_hover_text("A abelha sai da área de trabalho").clicked() {
                pedido = Some(Pedido::Desafixar);
            }
        });
    });
    pedido
}

// ---- Enquanto você esteve fora ----

/// Quantos itens o cartão da volta mostra.
pub const MAX_VOLTA: usize = 5;

/// A altura do cartão da volta: o nome, o título com o tempo (50), os itens,
/// o fio e os botões.
pub fn altura_volta(itens: usize) -> f32 {
    let mostrados = itens.clamp(1, MAX_VOLTA);
    let mais = if itens > MAX_VOLTA { LINHA_MAIS } else { 0.0 };
    MARGEM + 50.0 + mostrados as f32 * ALTURA_DESTAQUE + (mostrados - 1) as f32 * VAO_DESTAQUE + mais + 29.0 + ALTURA_BOTAO + 12.0
}

/// "47 min", "1 h 20", "2 dias": quanto tempo você ficou fora.
pub fn tempo_fora(d: Duration) -> String {
    let minutos = d.as_secs() / 60;
    match minutos {
        0..=59 => format!("{} min", minutos.max(1)),
        60..=1439 => {
            if minutos.is_multiple_of(60) {
                format!("{} h", minutos / 60)
            } else {
                format!("{} h {:02}", minutos / 60, minutos % 60)
            }
        }
        _ => {
            let dias = minutos / 1440;
            if dias == 1 { "1 dia".to_string() } else { format!("{dias} dias") }
        }
    }
}

/// O cartão de quando você volta de uma pausa: o que aconteceu enquanto
/// esteve fora, nos mesmos itens do resumo. Fica até você fechar.
pub fn volta(ui: &mut Ui, rect: Rect, nome: &str, fora: Duration, avisos: &[Aviso]) -> Option<Pedido> {
    let mut pedido = None;
    cartao(ui.painter(), rect, tema().destaque.gamma_multiply(0.6));
    let dentro = rect.shrink(MARGEM);
    let largura = dentro.width();
    linha(ui, dentro.left_top() + vec2(0.0, 1.0), nome, FontId::proportional(12.0), tema().suave, largura - 40.0);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 7.0), egui::Id::new("fechar-volta")) {
        pedido = Some(Pedido::Fechar);
    }
    let titulo = format!("Enquanto você esteve fora, {}", tempo_fora(fora));
    linha(ui, pos2(dentro.left(), dentro.top() + 21.0), &titulo, forte(14.0), tema().texto, largura);

    let mut topo = dentro.top() + 50.0;
    for (i, d) in avisos.iter().take(MAX_VOLTA).enumerate() {
        if i > 0 {
            topo += VAO_DESTAQUE;
        }
        let caixa = Rect::from_min_size(pos2(dentro.left(), topo), vec2(largura, ALTURA_DESTAQUE));
        if let Some(p) = destaque(ui, caixa, ("volta", i), d) {
            pedido = Some(p);
        }
        topo = caixa.bottom();
    }
    if avisos.len() > MAX_VOLTA {
        let resto = avisos.len() - MAX_VOLTA;
        let texto = if resto == 1 { "e mais 1 no quadro".to_string() } else { format!("e mais {resto} no quadro") };
        linha(ui, pos2(dentro.left() + 2.0, topo + 7.0), &texto, FontId::proportional(12.0), tema().suave, largura);
        topo += LINHA_MAIS;
    }

    let fio = topo + 16.0;
    ui.painter().hline(rect.x_range().shrink(1.0), fio + 0.5, Stroke::new(1.0, veu(10)));
    let rodape = Rect::from_min_max(pos2(dentro.left(), fio + 13.0), pos2(dentro.right(), fio + 13.0 + ALTURA_BOTAO));
    ui.scope_builder(UiBuilder::new().max_rect(rodape).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        if botao(ui, "Abrir a Colmeia", Estilo::Principal, ALTURA_BOTAO).clicked() {
            pedido = Some(Pedido::AbrirColmeia);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if botao(ui, "Entendi", Estilo::Discreto, ALTURA_BOTAO).clicked() {
                pedido = Some(Pedido::Fechar);
            }
        });
    });
    pedido
}

// ---- A conversa com um agente, aqui na abelha ----

/// O tamanho mínimo da conversa; ela cresce pelo canto livre e o tamanho fica guardado.
pub const PAPO_MINIMO: egui::Vec2 = vec2(360.0, 470.0);
/// As escalas da letra da conversa, da menor à maior (1 é a normal).
pub const LETRAS: [f32; 6] = [0.9, 1.0, 1.15, 1.3, 1.5, 1.75];
pub const LINHAS_PAPO: usize = 80;

/// O que a conversa mostra.
pub struct Papo<'a> {
    pub titulo: &'a str,
    /// A tarefa desta conversa, marcada na lista de trocar.
    pub tarefa: i64,
    /// As tarefas abertas, para trocar a conversa pelo chip do topo.
    pub tarefas: &'a [Escolha],
    pub quem: &'a str,
    pub estado: &'a str,
    pub motivo: &'a str,
    pub rodando: bool,
    /// A tarefa tem um agente (parado ou não); sem nenhum, "Rodar" cria um.
    pub com_agente: bool,
    /// O agente está sendo ligado.
    pub iniciando: bool,
    /// As linhas do terminal já interpretadas.
    pub blocos: &'a [Bloco],
    /// Os grupos de ferramentas abertos (pelo índice do bloco).
    pub abertos: &'a mut HashSet<usize>,
    pub texto: &'a mut String,
    pub enviando: bool,
    pub erro: Option<&'a str>,
    /// Quantas outras tarefas esperam você.
    pub outras: usize,
    /// O canto livre do cartão, por onde ele cresce: (à esquerda, em cima).
    pub canto: (bool, bool),
    /// A escala da letra da conversa (uma de LETRAS).
    pub letra: f32,
}

pub enum PedidoPapo {
    Fechar,
    Enviar,
    /// Ir à próxima tarefa que espera você.
    Proximo,
    /// Trocar para esta tarefa (e o agente dela), pela lista do chip.
    Trocar(i64, Option<i64>),
    /// Ligar o agente parado (ou criar um, se a tarefa não tem).
    Iniciar,
    /// Uma das opções numeradas que o agente ofereceu.
    Opcao(u8),
    /// Começou a puxar o canto para redimensionar.
    Puxar,
    /// Mudou a letra da conversa para esta escala.
    Letra(f32),
    /// Escolheu este tema (índice em TEMAS) para a abelha.
    Tema(usize),
    AbrirColmeia,
    NovaTarefa,
    Desafixar,
}

/// Um nome curto para o botão da opção ("Yes, and don't ask again" vira "Yes, and don't…").
fn opcao_curta(texto: &str) -> String {
    let limpo = texto.split(" (").next().unwrap_or(texto).trim();
    let mut curto: String = limpo.chars().take(18).collect();
    if limpo.chars().count() > 18 {
        curto.push('…');
    }
    curto
}

pub fn papo(ui: &mut Ui, rect: Rect, nome: &str, p: &mut Papo) -> Option<PedidoPapo> {
    let mut pedido = None;
    cartao(ui.painter(), rect, tema().borda);
    let dentro = rect.shrink(MARGEM);
    let largura = dentro.width();
    // Cabeçalho: o nome da abelha, o título da tarefa num chip e o fechar.
    let g = ui.painter().layout_no_wrap(nome.to_owned(), forte(14.5), tema().texto);
    ui.painter().galley(dentro.left_top(), g.clone(), tema().texto);
    let chip_x = dentro.left() + g.size().x + 10.0;
    let chip = Rect::from_min_size(pos2(chip_x, dentro.top() - 2.0), vec2((dentro.right() - 30.0 - chip_x).max(60.0), 24.0));
    // O chip da tarefa abre a lista das outras: o clique troca a conversa
    // sem passar pelo "Próximo".
    let id_lista = egui::Id::new("lista-papo");
    let mut lista_aberta = ui.data(|d| d.get_temp::<bool>(id_lista).unwrap_or(false));
    let r_chip =
        ui.interact(chip, egui::Id::new("chip-papo"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Trocar de tarefa");
    if r_chip.clicked() {
        lista_aberta = !lista_aberta;
    }
    let fundo_chip = veu(if r_chip.hovered() || lista_aberta { 18 } else { 10 });
    ui.painter().rect(chip, CornerRadius::same(12), fundo_chip, Stroke::new(1.0, tema().borda), StrokeKind::Inside);
    linha(ui, pos2(chip.left() + 10.0, chip.top() + 4.0), p.titulo, FontId::proportional(12.5), tema().texto, chip.width() - 34.0);
    seta(ui.painter(), pos2(chip.right() - 13.0, chip.center().y), if lista_aberta { tema().texto } else { tema().suave }, lista_aberta);
    if lista_aberta {
        match lista_de_tarefas(ui, id_lista, chip, dentro, p.tarefa, p.tarefas) {
            Escolhido::Trocar(t, a) => {
                pedido = Some(PedidoPapo::Trocar(t, a));
                lista_aberta = false;
            }
            Escolhido::Fechar => lista_aberta = false,
            Escolhido::Nada => {}
        }
    }
    ui.data_mut(|d| d.insert_temp(id_lista, lista_aberta));
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 9.0), egui::Id::new("fechar-papo")) {
        pedido = Some(PedidoPapo::Fechar);
    }
    // Quem é o agente e o que ele faz agora.
    let (estado, cor_estado) = match p.estado {
        _ if p.iniciando => ("ligando…", tema().suave),
        _ if !p.com_agente => ("ainda sem agente", tema().suave),
        _ if !p.rodando => ("parou", tema().suave),
        "aguardando" => (acao_do_motivo(p.motivo), tema().alerta),
        "trabalhando" => ("trabalhando", tema().ok),
        outro => (outro, tema().suave),
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(p.quem, 0.0, egui::TextFormat::simple(FontId::proportional(12.5), tema().suave));
    job.append("  ·  ", 0.0, egui::TextFormat::simple(FontId::proportional(12.5), tema().borda_clara));
    job.append(estado, 0.0, egui::TextFormat::simple(forte(12.5), cor_estado));
    // Com outras tarefas esperando, "Próximo ›" à direita leva à seguinte.
    let mut largura_estado = largura;
    if p.outras > 0 {
        let texto = "Próximo ›";
        let w = largura_botao(ui, texto);
        let lugar = Rect::from_min_size(pos2(dentro.right() - w, dentro.top() + 24.0), vec2(w, 24.0));
        let dica = if p.outras == 1 { "Mais 1 tarefa espera você".to_string() } else { format!("Mais {} tarefas esperam você", p.outras) };
        let clicou = ui
            .scope_builder(UiBuilder::new().max_rect(lugar).id_salt("proximo"), |ui| botao(ui, texto, Estilo::Discreto, 24.0).on_hover_text(dica).clicked())
            .inner;
        if clicou {
            pedido = Some(PedidoPapo::Proximo);
        }
        largura_estado = largura - w - 8.0;
    }
    job.wrap.max_width = largura_estado;
    job.wrap.max_rows = 1;
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(dentro.left(), dentro.top() + 28.0), g, tema().suave);

    // O rodapé e a caixa, de baixo para cima, para saber o que sobra para o texto.
    let fio = dentro.bottom() - ALTURA_BOTAO - 13.0;
    let caixa = Rect::from_min_size(pos2(dentro.left(), fio - 16.0 - ALTURA_CAIXA), vec2(largura, ALTURA_CAIXA));
    let opcoes = if p.rodando { conversa::opcoes(p.blocos) } else { Vec::new() };
    // Até duas por linha; com mais, os botões quebram em duas linhas. Parado,
    // o lugar das opções é do botão de ligar o agente.
    let altura_opcoes = match opcoes.len() {
        0 if p.rodando => 0.0,
        0..=2 => 34.0,
        _ => 68.0,
    };
    let corpo = Rect::from_min_max(pos2(dentro.left(), dentro.top() + 54.0), pos2(dentro.right(), caixa.top() - 8.0 - altura_opcoes));
    // Ctrl + e Ctrl − (ou Ctrl e a rodinha) mudam a letra; Ctrl 0 volta à normal.
    let (mais, menos, normal) = ui.input(|i| {
        let ctrl = i.modifiers.command;
        // Ctrl com a rodinha (ou a pinça no touchpad) chega como zoom.
        let zoom = i.zoom_delta();
        (
            ctrl && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)) || zoom > 1.0,
            ctrl && i.key_pressed(egui::Key::Minus) || zoom < 1.0,
            ctrl && i.key_pressed(egui::Key::Num0),
        )
    });
    if let Some(nova) = letra_seguinte(p.letra, mais, menos, normal) {
        pedido = Some(PedidoPapo::Letra(nova));
    }
    if let Some(i) = conversa_desenhar(ui, corpo, p.blocos, p.abertos, p.rodando, p.com_agente, p.letra)
        && !p.abertos.remove(&i)
    {
        p.abertos.insert(i);
    }
    if !p.rodando {
        let lugar = Rect::from_min_size(pos2(dentro.left(), caixa.top() - altura_opcoes), vec2(largura, altura_opcoes - 6.0));
        ui.scope_builder(UiBuilder::new().max_rect(lugar).layout(egui::Layout::left_to_right(egui::Align::TOP)), |ui| {
            let texto = if p.iniciando {
                "Ligando…"
            } else if p.com_agente {
                "Retomar o agente"
            } else {
                "Rodar com o Claude Code"
            };
            let dica = if p.com_agente { "Liga o agente de novo, na sessão em que parou" } else { "Sobe um Claude Code nesta tarefa" };
            if botao(ui, texto, if p.iniciando { Estilo::Secundario } else { Estilo::Principal }, 28.0).on_hover_text(dica).clicked() && !p.iniciando {
                pedido = Some(PedidoPapo::Iniciar);
            }
        });
    } else if !opcoes.is_empty() {
        let lugar = Rect::from_min_size(pos2(dentro.left(), caixa.top() - altura_opcoes), vec2(largura, altura_opcoes - 6.0));
        ui.scope_builder(UiBuilder::new().max_rect(lugar).layout(egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true)), |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            // Todas iguais: a primeira costuma aprovar um comando, e a cor da
            // marca puxaria o clique sem leitura.
            for (n, texto) in opcoes {
                if botao(ui, &format!("{n} · {}", opcao_curta(&texto)), Estilo::Secundario, 28.0).on_hover_text(&texto).clicked() {
                    pedido = Some(PedidoPapo::Opcao(n));
                }
            }
        });
    }
    if let Some(e) = p.erro {
        linha(ui, pos2(dentro.left(), caixa.top() - 18.0), e, FontId::proportional(11.5), tema().erro, largura);
    }
    let dica = if p.rodando { "Responder ou pedir…" } else { "O agente não está rodando" };
    if caixa_do_papo(ui, caixa, dica, p.texto, p.rodando && !p.enviando, p.letra) {
        pedido = Some(PedidoPapo::Enviar);
    }
    ui.painter().hline(rect.x_range().shrink(1.0), fio + 0.5, Stroke::new(1.0, veu(10)));
    let rodape = Rect::from_min_max(pos2(dentro.left(), fio + 13.0), pos2(dentro.right(), fio + 13.0 + ALTURA_BOTAO));
    ui.scope_builder(UiBuilder::new().max_rect(rodape).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        if botao(ui, "Abrir na Colmeia", Estilo::Discreto, ALTURA_BOTAO).on_hover_text("A tarefa, com o terminal inteiro").clicked() {
            pedido = Some(PedidoPapo::AbrirColmeia);
        }
        if botao(ui, "+ Tarefa", Estilo::Discreto, ALTURA_BOTAO).clicked() {
            pedido = Some(PedidoPapo::NovaTarefa);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if botao(ui, "Desafixar", Estilo::Discreto, ALTURA_BOTAO).clicked() {
                pedido = Some(PedidoPapo::Desafixar);
            }
            // "Aa" abre os ajustes da conversa: a letra (menor, maior) e o tema da abelha.
            let id_ajustes = egui::Id::new("ajustes-papo");
            let mut ajustes_abertos = ui.data(|d| d.get_temp::<bool>(id_ajustes).unwrap_or(false));
            let r_aa = botao(ui, "Aa", Estilo::Discreto, ALTURA_BOTAO).on_hover_text("Letra e tema da abelha (Ctrl + e Ctrl − mudam a letra)");
            if r_aa.clicked() {
                ajustes_abertos = !ajustes_abertos;
            }
            if ajustes_abertos {
                match ajustes(ui, id_ajustes, r_aa.rect, dentro, p.letra) {
                    Ajuste::Letra(escala) => pedido = Some(PedidoPapo::Letra(escala)),
                    Ajuste::Tema(i) => pedido = Some(PedidoPapo::Tema(i)),
                    Ajuste::Fechar => ajustes_abertos = false,
                    Ajuste::Nada => {}
                }
            }
            ui.data_mut(|d| d.insert_temp(id_ajustes, ajustes_abertos));
        });
    });
    // A alça no canto livre: três risquinhos; puxar redimensiona.
    if alca(ui, rect, p.canto) {
        pedido = Some(PedidoPapo::Puxar);
    }
    pedido
}

enum Escolhido {
    Nada,
    Fechar,
    Trocar(i64, Option<i64>),
}

enum Ajuste {
    Nada,
    Fechar,
    Letra(f32),
    Tema(usize),
}

/// Os ajustes da conversa, acima do "Aa": a letra (− e +, com a escala no
/// meio) e os quatro temas da abelha, o atual marcado. Clicar fora ou Esc fecha.
fn ajustes(ui: &mut Ui, id: egui::Id, aa: Rect, dentro: Rect, letra: f32) -> Ajuste {
    let largura = 236.0_f32.min(dentro.width());
    let alto = 96.0;
    let esquerda = (aa.right() - largura).max(dentro.left());
    let mut ajuste = Ajuste::Nada;
    let area = egui::Area::new(id.with("area")).order(egui::Order::Foreground).fixed_pos(pos2(esquerda, aa.top() - alto - 6.0)).show(ui.ctx(), |ui| {
        ui.set_width(largura);
        egui::Frame::NONE
            .fill(tema().fundo_lista)
            .stroke(Stroke::new(1.0, tema().borda))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                let i = indice_da_letra(letra);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Letra").color(tema().suave).size(12.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if botao(ui, "+", Estilo::Secundario, 26.0).on_hover_text("Maior (Ctrl +)").clicked() && i + 1 < LETRAS.len() {
                            ajuste = Ajuste::Letra(LETRAS[i + 1]);
                        }
                        ui.label(egui::RichText::new(format!("{:.0}%", letra * 100.0)).color(tema().texto).size(12.5));
                        if botao(ui, "−", Estilo::Secundario, 26.0).on_hover_text("Menor (Ctrl −)").clicked() && i > 0 {
                            ajuste = Ajuste::Letra(LETRAS[i - 1]);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Tema da abelha").color(tema().suave).size(12.0));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let atual = TEMA.load(Ordering::Relaxed);
                    for (n, t) in TEMAS.iter().enumerate() {
                        let (rect, r) = ui.allocate_exact_size(vec2(50.0, 26.0), Sense::click());
                        let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
                        let escolhido = n == atual;
                        let borda = if escolhido {
                            tema().destaque
                        } else if r.hovered() {
                            tema().borda_clara
                        } else {
                            tema().borda
                        };
                        ui.painter().rect(rect, CornerRadius::same(8), t.fundo, Stroke::new(if escolhido { 1.5 } else { 1.0 }, borda), StrokeKind::Inside);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            t.nome,
                            if escolhido { forte(11.0) } else { FontId::proportional(11.0) },
                            t.texto,
                        );
                        if r.clicked() {
                            ajuste = Ajuste::Tema(n);
                        }
                    }
                });
            });
    });
    let fora = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|pos| !area.response.rect.contains(pos) && !aa.contains(pos)));
    // O Esc fecha só o menu (consumido, não chega ao cartão por trás).
    let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if matches!(ajuste, Ajuste::Nada) && (fora || esc) { Ajuste::Fechar } else { ajuste }
}

/// Uma seta pequena para baixo (ou para cima, quando a lista está aberta).
fn seta(pintor: &egui::Painter, c: Pos2, cor: Color32, aberta: bool) {
    let s = Stroke::new(1.4, cor);
    let dy = if aberta { -2.0 } else { 2.0 };
    pintor.line_segment([c + vec2(-3.5, -dy), c + vec2(0.0, dy)], s);
    pintor.line_segment([c + vec2(0.0, dy), c + vec2(3.5, -dy)], s);
}

/// A lista das tarefas abertas debaixo do chip: a atual marcada, as que
/// esperam você primeiro, cada uma com o projeto e o estado. O clique troca;
/// clicar fora ou Esc fecha.
fn lista_de_tarefas(ui: &mut Ui, id: egui::Id, chip: Rect, dentro: Rect, atual: i64, tarefas: &[Escolha]) -> Escolhido {
    let largura = (dentro.right() - chip.left()).max(240.0);
    let alto_max = (dentro.bottom() - chip.bottom() - 90.0).max(120.0);
    let mut escolhido = Escolhido::Nada;
    let area = egui::Area::new(id.with("area")).order(egui::Order::Foreground).fixed_pos(pos2(chip.left(), chip.bottom() + 4.0)).show(ui.ctx(), |ui| {
        ui.set_width(largura);
        egui::Frame::NONE
            .fill(tema().fundo_lista)
            .stroke(Stroke::new(1.0, tema().borda))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(alto_max).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if tarefas.is_empty() {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Nenhuma tarefa aberta").color(tema().suave).size(12.0));
                    }
                    for e in tarefas {
                        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                        let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
                        let esta = e.tarefa == atual;
                        if r.hovered() || esta {
                            ui.painter().rect_filled(rect, CornerRadius::same(8), veu(if r.hovered() { 16 } else { 8 }));
                        }
                        let cor_estado = if e.espera {
                            tema().alerta
                        } else if e.estado == "trabalhando" {
                            tema().ok
                        } else {
                            tema().borda_clara
                        };
                        ui.painter().circle_filled(pos2(rect.left() + 11.0, rect.top() + 14.0), 3.5, cor_estado);
                        let fonte = if esta { forte(12.5) } else { FontId::proportional(12.5) };
                        linha(ui, pos2(rect.left() + 22.0, rect.top() + 4.0), &e.titulo, fonte, tema().texto, rect.width() - 30.0);
                        let detalhe = if e.projeto.is_empty() { e.estado.clone() } else { format!("{} · {}", e.projeto, e.estado) };
                        linha(ui, pos2(rect.left() + 22.0, rect.top() + 21.0), &detalhe, FontId::proportional(11.0), tema().suave, rect.width() - 30.0);
                        if r.clicked() {
                            escolhido = Escolhido::Trocar(e.tarefa, e.agente);
                        }
                    }
                });
            });
    });
    let fora = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|pos| !area.response.rect.contains(pos) && !chip.contains(pos)));
    let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if matches!(escolhido, Escolhido::Nada) && (fora || esc) { Escolhido::Fechar } else { escolhido }
}

/// Uma linha curta sem pontuação no fim, numa frase de várias linhas: o
/// agente a escreveu como título ("O que foi feito").
fn titulo_curto(l: &str) -> bool {
    l.chars().count() <= 60
        && l.chars().next().is_some_and(char::is_uppercase)
        && !l.ends_with(['.', ',', ';', ':', '!', '?', '…', ')'])
        && !l.contains('`')
        && !l.contains("**")
}

/// Os trechos de uma linha: o comum, o **negrito** e o `código`. Um marcador
/// sem par fica como veio.
fn trechos(l: &str, job: &mut egui::text::LayoutJob, comum: &egui::text::TextFormat, negrito: &egui::text::TextFormat, codigo: &egui::text::TextFormat) {
    let mut resto = l;
    while !resto.is_empty() {
        let (i, marca) = match (resto.find("**"), resto.find('`')) {
            (Some(n), Some(c)) if n <= c => (n, "**"),
            (Some(n), None) => (n, "**"),
            (_, Some(c)) => (c, "`"),
            (None, None) => {
                job.append(resto, 0.0, comum.clone());
                break;
            }
        };
        let depois = &resto[i + marca.len()..];
        match depois.find(marca) {
            Some(fim) if fim > 0 => {
                if i > 0 {
                    job.append(&resto[..i], 0.0, comum.clone());
                }
                job.append(&depois[..fim], 0.0, if marca == "**" { negrito.clone() } else { codigo.clone() });
                resto = &depois[fim + marca.len()..];
            }
            _ => {
                job.append(&resto[..i + marca.len()], 0.0, comum.clone());
                resto = depois;
            }
        }
    }
}

/// Uma frase como o agente escreveu, com a estrutura que o terminal mostrou:
/// parágrafos separados, itens de lista com a bolinha, um título curto em
/// seminegrito, **negrito** e `código`.
fn texto_rico(texto: &str, normal: f32, cor: Color32, largura: f32) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.wrap.max_width = largura;
    let linhas: Vec<&str> = texto.split('\n').collect();
    let varias = linhas.len() > 1;
    let comum = TextFormat { font_id: FontId::proportional(normal), color: cor, line_height: Some(normal * 1.45), ..Default::default() };
    let negrito = TextFormat { font_id: forte(normal), ..comum.clone() };
    let codigo = TextFormat { font_id: FontId::new(normal * 0.92, FontFamily::Monospace), background: veu(14), ..comum.clone() };
    // A linha vazia entre parágrafos: uma linha baixa.
    let vazia = TextFormat { font_id: FontId::proportional(normal * 0.5), line_height: Some(normal * 0.6), ..comum.clone() };
    let marcador = TextFormat { color: tema().destaque_claro, ..comum.clone() };
    for (i, linha) in linhas.iter().enumerate() {
        let l = linha.trim();
        let ultima = i + 1 == linhas.len();
        if l.is_empty() {
            if !ultima {
                job.append("\n", 0.0, vazia.clone());
            }
            continue;
        }
        let (item, l) = match conversa::marcador_de_lista(l) {
            Some((m, resto)) => {
                job.append(&format!("{m} "), 6.0, marcador.clone());
                (true, resto)
            }
            None => (false, l),
        };
        if !item && varias && titulo_curto(l) {
            job.append(l, 0.0, negrito.clone());
        } else {
            trechos(l, &mut job, &comum, &negrito, &codigo);
        }
        if !ultima {
            job.append("\n", 0.0, comum.clone());
        }
    }
    job
}

/// Uma parte da frase pronta para desenhar: o texto corrido ou uma tabela.
enum Parte {
    Texto(Arc<egui::Galley>),
    Tabela(TabelaPronta),
}

impl Parte {
    fn tamanho(&self) -> egui::Vec2 {
        match self {
            Parte::Texto(g) => g.size(),
            Parte::Tabela(t) => vec2(t.largura, t.altura),
        }
    }
}

/// A tabela medida: a largura de cada coluna, as células de cada linha com a
/// altura dela, e o tamanho todo.
struct TabelaPronta {
    colunas: Vec<f32>,
    linhas: Vec<(Vec<Arc<egui::Galley>>, f32)>,
    largura: f32,
    altura: f32,
}

/// As folgas dentro da célula, dos lados e em cima e embaixo.
const FOLGA_CELULA: egui::Vec2 = vec2(8.0, 5.0);
/// O espaço entre as partes de uma frase (um texto e uma tabela).
const ENTRE_PARTES: f32 = 8.0;

/// As partes da frase: os trechos de texto corrido e, entre eles, as tabelas
/// (as linhas "│a│b│" e "├" que a conversa guardou).
fn partes_da_frase(texto: &str) -> Vec<Result<String, Vec<Vec<String>>>> {
    let mut partes: Vec<Result<String, Vec<Vec<String>>>> = Vec::new();
    let mut corrido: Vec<&str> = Vec::new();
    let mut tabela: Vec<&str> = Vec::new();
    let fechar_texto = |corrido: &mut Vec<&str>, partes: &mut Vec<_>| {
        let texto = corrido.join("\n");
        let texto = texto.trim_matches('\n');
        if !texto.is_empty() {
            partes.push(Ok(texto.to_string()));
        }
        corrido.clear();
    };
    let fechar_tabela = |tabela: &mut Vec<&str>, partes: &mut Vec<_>| {
        let linhas = conversa::tabela(tabela);
        if !linhas.is_empty() {
            partes.push(Err(linhas));
        }
        tabela.clear();
    };
    for l in texto.split('\n') {
        let de_tabela = l == "├" || (l.starts_with('│') && l.ends_with('│') && l.len() > 1);
        if de_tabela {
            if tabela.is_empty() {
                fechar_texto(&mut corrido, &mut partes);
            }
            tabela.push(l);
        } else {
            if !tabela.is_empty() {
                fechar_tabela(&mut tabela, &mut partes);
            }
            corrido.push(l);
        }
    }
    fechar_texto(&mut corrido, &mut partes);
    fechar_tabela(&mut tabela, &mut partes);
    partes
}

/// Uma célula da tabela: o texto com **negrito** e `código`, em seminegrito
/// inteiro no cabeçalho.
fn celula(texto: &str, tamanho: f32, cor: Color32, largura: f32, cabecalho: bool) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.wrap.max_width = largura;
    let comum = TextFormat { font_id: FontId::proportional(tamanho), color: cor, line_height: Some(tamanho * 1.4), ..Default::default() };
    let negrito = TextFormat { font_id: forte(tamanho), ..comum.clone() };
    let codigo = TextFormat { font_id: FontId::new(tamanho * 0.92, FontFamily::Monospace), background: veu(14), ..comum.clone() };
    trechos(texto, &mut job, if cabecalho { &negrito } else { &comum }, &negrito, &codigo);
    job
}

/// Mede a tabela: cada coluna fica na largura do que tem de mais comprido.
/// Se não cabe tudo, as colunas encolhem na proporção do que têm de sobra
/// (nenhuma fica mais estreita que a sua palavra mais comprida) e o texto
/// quebra dentro da célula; se nem assim cabe, dividem a largura.
fn medir_tabela(pintor: &egui::Painter, linhas: &[Vec<String>], tamanho: f32, cor: Color32, largura_max: f32) -> TabelaPronta {
    let n = linhas.iter().map(Vec::len).max().unwrap_or(0);
    let folga = FOLGA_CELULA.x * 2.0;
    let mut colunas = vec![0.0f32; n];
    let mut minimas = vec![0.0f32; n];
    for (i, l) in linhas.iter().enumerate() {
        for (c, texto) in l.iter().enumerate() {
            let g = pintor.layout_job(celula(texto, tamanho, cor, f32::INFINITY, i == 0));
            colunas[c] = colunas[c].max(g.size().x + folga);
            for palavra in texto.split_whitespace() {
                let g = pintor.layout_job(celula(palavra, tamanho, cor, f32::INFINITY, i == 0));
                minimas[c] = minimas[c].max(g.size().x + folga);
            }
        }
    }
    let total: f32 = colunas.iter().sum();
    if total > largura_max && total > 0.0 {
        let falta = total - largura_max;
        let sobra: f32 = colunas.iter().zip(&minimas).map(|(c, m)| (c - m).max(0.0)).sum();
        if sobra >= falta {
            for (c, m) in colunas.iter_mut().zip(&minimas) {
                *c -= falta * (*c - *m).max(0.0) / sobra;
            }
        } else {
            let total_min: f32 = minimas.iter().sum::<f32>().max(1.0);
            for (c, m) in colunas.iter_mut().zip(&minimas) {
                *c = (*m / total_min * largura_max).max(folga + 16.0);
            }
        }
    }
    let mut pronta = TabelaPronta { linhas: Vec::new(), largura: colunas.iter().sum(), altura: 0.0, colunas };
    for (i, l) in linhas.iter().enumerate() {
        let celulas: Vec<Arc<egui::Galley>> =
            l.iter().enumerate().map(|(c, texto)| pintor.layout_job(celula(texto, tamanho, cor, pronta.colunas[c] - folga, i == 0))).collect();
        let alto = celulas.iter().map(|g| g.size().y).fold(0.0, f32::max) + FOLGA_CELULA.y * 2.0;
        pronta.altura += alto;
        pronta.linhas.push((celulas, alto));
    }
    pronta
}

/// Desenha a tabela medida: o cabeçalho num fundo leve, os fios finos entre
/// as linhas e as colunas, a borda arredondada em volta.
fn desenhar_tabela(pintor: &egui::Painter, pos: Pos2, t: &TabelaPronta, cor: Color32) {
    let caixa = Rect::from_min_size(pos, vec2(t.largura, t.altura));
    if let Some((_, alto)) = t.linhas.first() {
        let cabecalho = Rect::from_min_size(pos, vec2(t.largura, *alto));
        pintor.rect_filled(cabecalho, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, veu(10));
    }
    let fio = Stroke::new(1.0, veu(18));
    let mut y = pos.y;
    for (i, (celulas, alto)) in t.linhas.iter().enumerate() {
        if i > 0 {
            pintor.hline(caixa.x_range(), y, fio);
        }
        let mut x = pos.x;
        for (c, g) in celulas.iter().enumerate() {
            if c > 0 {
                pintor.vline(x, egui::Rangef::new(y, y + alto), fio);
            }
            pintor.galley(pos2(x + FOLGA_CELULA.x, y + FOLGA_CELULA.y), g.clone(), cor);
            x += t.colunas[c];
        }
        y += alto;
    }
    pintor.rect_stroke(caixa, CornerRadius::same(6), Stroke::new(1.0, veu(24)), StrokeKind::Inside);
}

/// A frase medida em partes, na largura dada: o texto corrido e as tabelas.
fn montar_frase(pintor: &egui::Painter, texto: &str, normal: f32, cor: Color32, largura: f32) -> Vec<Parte> {
    partes_da_frase(texto)
        .into_iter()
        .map(|p| match p {
            Ok(t) => Parte::Texto(pintor.layout_job(texto_rico(&t, normal, cor, largura))),
            Err(linhas) => Parte::Tabela(medir_tabela(pintor, &linhas, normal * 0.93, cor, largura)),
        })
        .collect()
}

/// Qual das LETRAS é a escala (a normal, se não for nenhuma).
fn indice_da_letra(escala: f32) -> usize {
    LETRAS.iter().position(|l| (l - escala).abs() < 0.01).unwrap_or(1)
}

/// A letra seguinte: um passo para cima ou para baixo, ou a normal. None
/// quando não muda (já está no fim, ou nada foi apertado).
fn letra_seguinte(atual: f32, mais: bool, menos: bool, normal: bool) -> Option<f32> {
    let i = indice_da_letra(atual);
    let nova = if normal {
        0
    } else if mais {
        (i + 1).min(LETRAS.len() - 1)
    } else if menos {
        i.saturating_sub(1)
    } else {
        return None;
    };
    (nova != i).then_some(LETRAS[nova])
}

/// A alça de redimensionar no canto livre do cartão; diz se começou a puxar.
fn alca(ui: &mut Ui, rect: Rect, (esquerda, cima): (bool, bool)) -> bool {
    let canto = pos2(if esquerda { rect.left() + 9.0 } else { rect.right() - 9.0 }, if cima { rect.top() + 9.0 } else { rect.bottom() - 9.0 });
    let area = Rect::from_center_size(canto, vec2(22.0, 22.0));
    let cursor = if esquerda == cima { egui::CursorIcon::ResizeNwSe } else { egui::CursorIcon::ResizeNeSw };
    let r = ui.interact(area, egui::Id::new("alca-papo"), Sense::drag()).on_hover_cursor(cursor);
    let cor = if r.hovered() || r.dragged() { tema().texto } else { tema().borda_clara };
    let (sx, sy) = (if esquerda { -1.0 } else { 1.0 }, if cima { -1.0 } else { 1.0 });
    // Risquinhos em diagonal, encostados no canto, apontando para fora.
    for (i, tam) in [(0.0, 10.0), (3.5, 6.5), (7.0, 3.0)] {
        let a = canto + vec2(sx * (tam - i), -sy * i);
        let b = canto + vec2(sx * -i, sy * (tam - i));
        ui.painter().line_segment([a, b], Stroke::new(1.2, cor));
    }
    r.drag_started()
}

/// "espera sua resposta", "pede aprovação"…, pelo motivo do núcleo.
fn acao_do_motivo(motivo: &str) -> &'static str {
    match motivo {
        "aprovacao" | "permissao" => "pede aprovação",
        "pergunta" => "fez uma pergunta",
        "prompt" | "sua_vez" | "" => "espera sua resposta",
        _ => "espera você",
    }
}

/// A conversa em balões, do fim para o começo até encher a área: o que você
/// mandou à direita, as frases do agente à esquerda, as ferramentas numa
/// linha cinza que abre no clique, a pergunta no balão com a borda âmbar.
/// Devolve o índice do grupo de ferramentas clicado.
fn conversa_desenhar(ui: &mut Ui, area: Rect, blocos: &[Bloco], abertos: &HashSet<usize>, rodando: bool, com_agente: bool, escala: f32) -> Option<usize> {
    let pintor = ui.painter_at(area);
    // Os tamanhos da letra, todos na escala escolhida.
    let normal = 13.5 * escala;
    let pequena = 12.0 * escala;
    let miuda = 11.0 * escala;
    if blocos.is_empty() {
        let texto = if rodando {
            "Esperando o agente escrever…"
        } else if com_agente {
            "O agente parou. Ligue de novo para conversar por aqui."
        } else {
            "Esta tarefa ainda não tem agente. Suba um para conversar por aqui."
        };
        paragrafo(ui, pos2(area.left(), area.top()), texto, FontId::proportional(normal), tema().suave, area.width());
        return None;
    }
    let mut clicado = None;
    let largura = area.width();
    let fonte = FontId::proportional(normal);
    let mono = FontId::new(pequena, FontFamily::Monospace);
    let mut fundo_y = area.bottom();
    for (i, bloco) in blocos.iter().enumerate().rev() {
        if fundo_y < area.top() {
            break;
        }
        match bloco {
            Bloco::Seu(t) | Bloco::Frase(t) => {
                let meu = matches!(bloco, Bloco::Seu(_));
                let partes = montar_frase(&pintor, t, normal, tema().texto, (if meu { largura * 0.84 } else { largura - 12.0 }) - 20.0);
                let largo = partes.iter().map(|p| p.tamanho().x).fold(0.0, f32::max);
                let alto: f32 = partes.iter().map(|p| p.tamanho().y).sum::<f32>() + ENTRE_PARTES * partes.len().saturating_sub(1) as f32;
                let altura = alto + 14.0;
                let topo = fundo_y - altura;
                let x = if meu { area.right() - largo - 20.0 } else { area.left() };
                let balao = Rect::from_min_size(pos2(x, topo), vec2(largo + 20.0, altura));
                let (fundo, borda) = if meu { (tema().destaque.gamma_multiply(0.22), tema().destaque.gamma_multiply(0.5)) } else { (veu(9), veu(14)) };
                pintor.rect(balao, CornerRadius::same(10), fundo, Stroke::new(1.0, borda), StrokeKind::Inside);
                let mut y = balao.top() + 7.0;
                for p in partes {
                    let pos = pos2(balao.left() + 10.0, y);
                    y += p.tamanho().y + ENTRE_PARTES;
                    match p {
                        Parte::Texto(g) => pintor.galley(pos, g, tema().texto),
                        Parte::Tabela(t) => desenhar_tabela(&pintor, pos, &t, tema().texto),
                    }
                }
                fundo_y = topo - 6.0;
            }
            Bloco::Nota(t) => {
                let g = pintor.layout(t.clone(), FontId::proportional(miuda), tema().suave, largura - 8.0);
                let topo = fundo_y - g.size().y;
                pintor.galley(pos2(area.left() + 4.0, topo), g, tema().suave);
                fundo_y = topo - 6.0;
            }
            Bloco::Ferramentas(lista) => {
                let aberto = abertos.contains(&i);
                // Aberto, cada ferramenta na sua linha com o começo do resultado.
                if aberto {
                    for f in lista.iter().rev() {
                        for r in f.resultado.iter().take(3).rev() {
                            let g = pintor.layout(r.clone(), mono.clone(), tema().suave, largura - 36.0);
                            let topo = fundo_y - g.size().y;
                            pintor.galley(pos2(area.left() + 32.0, topo), g, tema().suave.gamma_multiply(0.8));
                            fundo_y = topo - 2.0;
                        }
                        let mut job = egui::text::LayoutJob::default();
                        job.append(&f.resumo(), 0.0, egui::TextFormat::simple(FontId::proportional(pequena), tema().suave));
                        if let Some((mais, menos)) = f.contagem() {
                            job.append(&format!("  +{mais}"), 0.0, egui::TextFormat::simple(FontId::proportional(pequena), tema().ok));
                            job.append(&format!(" −{menos}"), 0.0, egui::TextFormat::simple(FontId::proportional(pequena), tema().erro));
                        }
                        job.wrap.max_width = largura - 24.0;
                        job.wrap.max_rows = 1;
                        let g = pintor.layout_job(job);
                        let topo = fundo_y - g.size().y;
                        pintor.galley(pos2(area.left() + 20.0, topo), g, tema().suave);
                        fundo_y = topo - 3.0;
                    }
                }
                // A linha do grupo: a seta e os resumos juntos.
                let resumo = lista.iter().map(Ferramenta::resumo).collect::<Vec<_>>().join(" · ");
                let mut job = egui::text::LayoutJob::simple_singleline(resumo, FontId::proportional(pequena), tema().suave);
                job.wrap = egui::text::TextWrapping::truncate_at_width(largura - 24.0);
                let g = pintor.layout_job(job);
                let alto = (18.0 * escala).round();
                let topo = fundo_y - alto;
                let faixa = Rect::from_min_size(pos2(area.left(), topo), vec2(largura, alto));
                // Só abre quando há o que mostrar (o resumo do Claude Code atual não tem).
                let tem_detalhe = lista.iter().any(Ferramenta::tem_detalhe);
                if faixa.top() >= area.top() && tem_detalhe {
                    let r = ui.interact(faixa, egui::Id::new(("ferramentas", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    let dica = if aberto { "Esconder o que cada ferramenta fez" } else { "Ver o que cada ferramenta fez" };
                    if r.on_hover_text(dica).clicked() {
                        clicado = Some(i);
                    }
                }
                let c = pos2(area.left() + 8.0, faixa.center().y);
                let seta = Stroke::new(1.3, tema().suave);
                if !tem_detalhe {
                    pintor.circle_filled(c, 2.0, tema().suave);
                } else if aberto {
                    pintor.line_segment([c + vec2(-3.5, -2.0), c + vec2(0.0, 2.0)], seta);
                    pintor.line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -2.0)], seta);
                } else {
                    pintor.line_segment([c + vec2(-2.0, -3.5), c + vec2(2.0, 0.0)], seta);
                    pintor.line_segment([c + vec2(2.0, 0.0), c + vec2(-2.0, 3.5)], seta);
                }
                pintor.galley(pos2(area.left() + 20.0, faixa.center().y - g.size().y / 2.0), g, tema().suave);
                fundo_y = topo - 4.0;
            }
            Bloco::Pergunta(q) => {
                // Mede de baixo para cima: as opções não entram (viram botões).
                let largura_texto = largura - 12.0 - 22.0;
                let rotulo = if q.aprovacao() { "PEDE APROVAÇÃO" } else { "PERGUNTA" };
                let mut partes: Vec<(std::sync::Arc<egui::Galley>, f32)> = Vec::new();
                partes.push((pintor.layout(rotulo.to_string(), forte(10.5 * escala), tema().alerta, largura_texto), 4.0));
                if !q.titulo.is_empty() {
                    partes.push((pintor.layout(q.titulo.clone(), forte(normal), tema().texto, largura_texto), 3.0));
                }
                for c in &q.comando {
                    partes.push((pintor.layout(c.clone(), mono.clone(), tema().texto, largura_texto), 2.0));
                }
                for t in &q.texto {
                    partes.push((pintor.layout(t.clone(), fonte.clone(), tema().suave, largura_texto), 2.0));
                }
                if q.opcoes.is_empty() && q.titulo.is_empty() && q.comando.is_empty() && q.texto.is_empty() {
                    continue;
                }
                let altura: f32 = partes.iter().map(|(g, v)| g.size().y + v).sum::<f32>() + 14.0;
                let largura_balao = partes.iter().map(|(g, _)| g.size().x).fold(0.0, f32::max) + 22.0;
                let topo = fundo_y - altura;
                let balao = Rect::from_min_size(pos2(area.left(), topo), vec2(largura_balao.min(largura - 12.0), altura));
                pintor.rect(balao, CornerRadius::same(10), tema().alerta.gamma_multiply(0.08), Stroke::new(1.0, tema().alerta), StrokeKind::Inside);
                let mut y = balao.top() + 8.0;
                for (g, v) in partes {
                    let cor = g.job.sections.first().map(|s| s.format.color).unwrap_or(tema().texto);
                    let h = g.size().y;
                    pintor.galley(pos2(balao.left() + 11.0, y), g, cor);
                    y += h + v;
                }
                fundo_y = topo - 6.0;
            }
        }
    }
    clicado
}

/// A caixa de responder ao agente; Enter (ou a seta) manda.
fn caixa_do_papo(ui: &mut Ui, caixa: Rect, dica: &str, texto: &mut String, ativa: bool, escala: f32) -> bool {
    ui.painter().rect(caixa, CornerRadius::same(RAIO_BOTAO), veu(6), Stroke::new(1.0, tema().borda), StrokeKind::Inside);
    let campo = Rect::from_min_max(caixa.min + vec2(10.0, 0.0), pos2(caixa.right() - 34.0, caixa.bottom()));
    let resposta = ui
        .scope_builder(UiBuilder::new().max_rect(campo).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.add_enabled(
                ativa,
                egui::TextEdit::singleline(texto)
                    .id(egui::Id::new("resposta-papo"))
                    .frame(egui::Frame::NONE)
                    .desired_width(campo.width())
                    .char_limit(4000)
                    .text_color(tema().texto)
                    .font(FontId::proportional(12.5 * escala))
                    .hint_text(egui::RichText::new(dica).color(tema().suave).size(12.5 * escala)),
            )
        })
        .inner;
    if resposta.clicked() && !ui.input(|i| i.viewport().focused.unwrap_or(false)) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    let enter = resposta.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    // O campo já vem com o foco (e volta a ele depois de um clique fora),
    // para escrever sem procurar onde. O Esc não fica aqui: ele fecha a
    // abelha, e quem trata é o laço principal.
    if ativa && ui.memory(|m| m.focused().is_none()) {
        resposta.request_focus();
    }
    let seta = Rect::from_center_size(pos2(caixa.right() - 16.0, caixa.center().y), vec2(24.0, 24.0));
    let botao = ui.interact(seta, egui::Id::new("mandar-papo"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let cor = if !ativa {
        tema().borda_clara
    } else if botao.hovered() {
        tema().texto
    } else {
        tema().destaque
    };
    let c = seta.center();
    ui.painter().line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(1.6, cor));
    ui.painter().line_segment([c + vec2(1.0, -4.0), c + vec2(5.0, 0.0)], Stroke::new(1.6, cor));
    ui.painter().line_segment([c + vec2(1.0, 4.0), c + vec2(5.0, 0.0)], Stroke::new(1.6, cor));
    ativa && (enter || botao.clicked()) && !texto.trim().is_empty()
}

// ---- Uma tarefa nova, daqui ----

/// Cabeçalho (30), o projeto (30 + 8), a caixa de marcar (22 + 10), a caixa
/// do título (34), a linha do erro (24), o fio e os botões (13 + 1 + 12 + 32)
/// e as margens.
pub const ALTURA_NOVA: f32 = 240.0;

pub struct NovaVista<'a> {
    pub titulo: &'a mut String,
    pub projeto: &'a str,
    /// Há mais de um projeto para escolher.
    pub varios_projetos: bool,
    pub com_agente: bool,
    pub enviando: bool,
    pub erro: Option<&'a str>,
}

pub enum PedidoNova {
    Cancelar,
    TrocarProjeto,
    AlternarAgente,
    Criar,
}

pub fn nova_tarefa(ui: &mut Ui, rect: Rect, nome: &str, n: &mut NovaVista) -> Option<PedidoNova> {
    let mut pedido = None;
    cartao(ui.painter(), rect, tema().borda);
    let dentro = rect.shrink(MARGEM);
    let largura = dentro.width();
    linha(ui, dentro.left_top(), &format!("Nova tarefa · {nome}"), forte(14.5), tema().texto, largura - 32.0);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 9.0), egui::Id::new("fechar-nova")) {
        pedido = Some(PedidoNova::Cancelar);
    }
    // O projeto, numa linha inteira: a palavra em cinza, o nome (cortado se
    // for comprido) e a seta quando há mais de um para passar.
    let topo = dentro.top() + 30.0;
    let seletor = Rect::from_min_size(pos2(dentro.left(), topo), vec2(largura, 30.0));
    let r = ui.interact(seletor, egui::Id::new("projeto-nova"), Sense::click());
    let r = if n.varios_projetos { r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Clique para passar ao próximo projeto") } else { r };
    let fundo = veu(if r.hovered() && n.varios_projetos { 22 } else { 12 });
    ui.painter().rect(seletor, CornerRadius::same(RAIO_BOTAO), fundo, Stroke::new(1.0, tema().borda), StrokeKind::Inside);
    let rotulo = ui.painter().layout_no_wrap("Projeto".to_owned(), FontId::proportional(12.5), tema().suave);
    let rotulo_x = rotulo.size().x;
    ui.painter().galley(pos2(seletor.left() + 12.0, seletor.top() + 7.0), rotulo, tema().suave);
    let nome = if n.projeto.is_empty() { "nenhum" } else { n.projeto };
    let largura_nome = largura - 12.0 - rotulo_x - 10.0 - 12.0 - if n.varios_projetos { 18.0 } else { 0.0 };
    linha(ui, pos2(seletor.left() + 12.0 + rotulo_x + 10.0, seletor.top() + 7.0), nome, forte(12.5), tema().texto, largura_nome);
    if n.varios_projetos {
        // A seta para baixo, um "v" de duas linhas.
        let c = pos2(seletor.right() - 16.0, seletor.center().y);
        let cor = if r.hovered() { tema().texto } else { tema().suave };
        ui.painter().line_segment([c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0)], Stroke::new(1.5, cor));
        ui.painter().line_segment([c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)], Stroke::new(1.5, cor));
    }
    if r.clicked() && n.varios_projetos {
        pedido = Some(PedidoNova::TrocarProjeto);
    }
    // Com ou sem agente: uma caixa de marcar, clicável na linha inteira.
    let marca = Rect::from_min_size(pos2(dentro.left(), seletor.bottom() + 8.0), vec2(largura, 22.0));
    let r = ui.interact(marca, egui::Id::new("agente-nova"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let quadrado = Rect::from_center_size(pos2(marca.left() + 9.0, marca.center().y), vec2(16.0, 16.0));
    if n.com_agente {
        ui.painter().rect_filled(quadrado, CornerRadius::same(4), if r.hovered() { tema().destaque_claro } else { tema().destaque });
        let c = quadrado.center();
        ui.painter().line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.0)], Stroke::new(1.8, tema().tinta));
        ui.painter().line_segment([c + vec2(-1.0, 3.0), c + vec2(4.0, -3.0)], Stroke::new(1.8, tema().tinta));
    } else {
        ui.painter().rect(quadrado, CornerRadius::same(4), veu(if r.hovered() { 20 } else { 8 }), Stroke::new(1.0, tema().borda_clara), StrokeKind::Inside);
    }
    linha(
        ui,
        pos2(quadrado.right() + 10.0, marca.top() + 3.0),
        "Já com um Claude Code na tarefa",
        FontId::proportional(12.5),
        if n.com_agente { tema().texto } else { tema().suave },
        largura - 30.0,
    );
    if r.clicked() {
        pedido = Some(PedidoNova::AlternarAgente);
    }
    let caixa = Rect::from_min_size(pos2(dentro.left(), marca.bottom() + 10.0), vec2(largura, ALTURA_CAIXA));
    ui.painter().rect(caixa, CornerRadius::same(RAIO_BOTAO), veu(6), Stroke::new(1.0, tema().borda), StrokeKind::Inside);
    let campo = Rect::from_min_max(caixa.min + vec2(10.0, 0.0), pos2(caixa.right() - 10.0, caixa.bottom()));
    let resposta = ui
        .scope_builder(UiBuilder::new().max_rect(campo).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.add_enabled(
                !n.enviando,
                egui::TextEdit::singleline(n.titulo)
                    .id(egui::Id::new("titulo-nova"))
                    .frame(egui::Frame::NONE)
                    .desired_width(campo.width())
                    .char_limit(200)
                    .text_color(tema().texto)
                    .font(FontId::proportional(12.5))
                    .hint_text(egui::RichText::new("O que precisa ser feito").color(tema().suave)),
            )
        })
        .inner;
    if resposta.clicked() && !ui.input(|i| i.viewport().focused.unwrap_or(false)) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    let enter = resposta.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    esc_so_sai_do_campo(ui, &resposta);
    if let Some(e) = n.erro {
        linha(ui, pos2(dentro.left(), caixa.bottom() + 8.0), e, FontId::proportional(11.5), tema().erro, largura);
    }
    let fio = dentro.bottom() - ALTURA_BOTAO - 13.0;
    ui.painter().hline(rect.x_range().shrink(1.0), fio + 0.5, Stroke::new(1.0, veu(10)));
    let rodape = Rect::from_min_max(pos2(dentro.left(), fio + 13.0), pos2(dentro.right(), fio + 13.0 + ALTURA_BOTAO));
    let pode = !n.enviando && !n.titulo.trim().is_empty() && !n.projeto.is_empty();
    ui.scope_builder(UiBuilder::new().max_rect(rodape).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        if botao(ui, "Cancelar", Estilo::Discreto, ALTURA_BOTAO).clicked() {
            pedido = Some(PedidoNova::Cancelar);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let texto = if n.enviando {
                "Criando…"
            } else if n.com_agente {
                "Criar e rodar"
            } else {
                "Criar"
            };
            if botao(ui, texto, if pode { Estilo::Principal } else { Estilo::Secundario }, ALTURA_BOTAO).clicked() && pode {
                pedido = Some(PedidoNova::Criar);
            }
        });
    });
    if enter && pode {
        pedido = Some(PedidoNova::Criar);
    }
    pedido
}

fn caixa_para_a_abelha(ui: &mut Ui, caixa: Rect, nome: &str, texto: &mut String) -> bool {
    ui.painter().rect(caixa, CornerRadius::same(RAIO_BOTAO), veu(6), Stroke::new(1.0, tema().borda), StrokeKind::Inside);
    let campo = Rect::from_min_max(caixa.min + vec2(10.0, 0.0), pos2(caixa.right() - 34.0, caixa.bottom()));
    let resposta = ui
        .scope_builder(UiBuilder::new().max_rect(campo).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.add(
                egui::TextEdit::singleline(texto)
                    .id(egui::Id::new("pergunta-abelha"))
                    .frame(egui::Frame::NONE)
                    .desired_width(campo.width())
                    .char_limit(500)
                    .text_color(tema().texto)
                    .font(FontId::proportional(12.5))
                    .hint_text(egui::RichText::new(format!("Pergunte ou peça à {nome}…")).color(tema().suave)),
            )
        })
        .inner;
    // Sem foco na janela, o teclado não chega: o clique na caixa pede o foco.
    if resposta.clicked() && !ui.input(|i| i.viewport().focused.unwrap_or(false)) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    let enter = resposta.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    esc_so_sai_do_campo(ui, &resposta);
    let seta = Rect::from_center_size(pos2(caixa.right() - 16.0, caixa.center().y), vec2(24.0, 24.0));
    let botao = ui.interact(seta, egui::Id::new("mandar-pergunta"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let cor = if botao.hovered() { tema().texto } else { tema().destaque };
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
    cartao(ui.painter(), rect, tema().destaque.gamma_multiply(0.6));
    let dentro = rect.shrink2(vec2(14.0, 14.0));
    let largura = dentro.width();
    linha(ui, dentro.left_top() + vec2(0.0, 1.0), nome, FontId::proportional(12.0), tema().suave, largura - 40.0);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 7.0), egui::Id::new("fechar-balao")) {
        pedido = Some(Pedido::Fechar);
    }
    let mut y = dentro.top() + 24.0;
    ui.painter().circle_filled(pos2(dentro.left() + 5.0, y + 9.5), 4.5, cor(aviso.tipo));
    y += paragrafo(ui, pos2(dentro.left() + 16.0, y), &aviso.titulo, forte(14.0), tema().texto, largura - 16.0);
    if !aviso.detalhe.trim().is_empty() {
        y += 4.0 + paragrafo(ui, pos2(dentro.left() + 16.0, y + 4.0), &aviso.detalhe, FontId::proportional(12.5), tema().suave, largura - 16.0);
    }
    let topo = (y + 12.0).min(dentro.bottom() - ALTURA_BOTAO - 4.0);
    let botoes = Rect::from_min_size(pos2(dentro.left() + 16.0, topo), vec2(largura - 16.0, ALTURA_BOTAO));
    ui.scope_builder(UiBuilder::new().max_rect(botoes).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let acao = if aviso.tipo == Tipo::Erro { "Ver o erro" } else { "Abrir" };
        if botao(ui, acao, Estilo::Principal, ALTURA_BOTAO).clicked() {
            pedido = Some(Pedido::Abrir { tarefa: aviso.tarefa, agente: aviso.agente });
        }
        if botao(ui, "Depois", Estilo::Discreto, ALTURA_BOTAO).clicked() {
            pedido = Some(Pedido::Fechar);
        }
    });
    let fio = Rect::from_min_size(pos2(rect.left() + 14.0, rect.bottom() - 5.0), vec2((rect.width() - 28.0) * resta.clamp(0.0, 1.0), 2.0));
    ui.painter().rect_filled(fio, CornerRadius::same(1), tema().destaque.gamma_multiply(0.7));
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
    cartao(ui.painter(), rect, if f.erro { tema().erro.gamma_multiply(0.6) } else { tema().destaque.gamma_multiply(0.6) });
    let dentro = rect.shrink2(vec2(13.0, 12.0));
    let largura = dentro.width();
    let mut x = dentro.left();
    if f.microfone {
        ui.painter().circle_filled(pos2(x + 5.0, dentro.top() + 9.0), 4.5, tema().erro);
        x += 16.0;
    }
    linha(ui, pos2(x, dentro.top()), nome, FontId::proportional(13.0), tema().suave, largura - 40.0);
    if fechar(ui, pos2(dentro.right() - 8.0, dentro.top() + 9.0), egui::Id::new("fechar-voz")) {
        pedido = Some(PedidoVoz::Fechar);
    }
    // A resposta em duas linhas, ou mais (a da pergunta livre).
    let linhas = linhas_da_resposta(f.texto);
    let mut job = egui::text::LayoutJob::simple(f.texto.to_owned(), FontId::proportional(13.5), if f.erro { tema().erro } else { tema().texto }, largura);
    job.wrap.max_rows = linhas;
    let g = ui.painter().layout_job(job);
    // O que foi dito vem logo abaixo da resposta (a altura do balão é estimada antes).
    let fim = dentro.top() + 22.0 + g.size().y.max(2.0 * ALTURA_LINHA);
    ui.painter().galley(pos2(dentro.left(), dentro.top() + 22.0), g, tema().texto);
    if !f.rodape.is_empty() {
        let job = egui::text::LayoutJob::simple(f.rodape.to_owned(), FontId::proportional(11.5), tema().suave, largura);
        let g = ui.painter().layout_job(job);
        ui.painter().galley(pos2(dentro.left(), fim + 4.0), g, tema().suave);
    } else if !f.ouvido.is_empty() {
        let quem = if f.escrito { "Você escreveu" } else { "Você disse" };
        let y = fim + 4.0;
        linha(ui, pos2(dentro.left(), y), &format!("{quem}: \u{201c}{}\u{201d}", f.ouvido), FontId::proportional(11.5), tema().suave, largura);
    }
    if f.pergunta || f.abrir.is_some() {
        let botoes = Rect::from_min_size(pos2(dentro.left(), dentro.bottom() - ALTURA_BOTAO), vec2(largura, ALTURA_BOTAO));
        ui.scope_builder(UiBuilder::new().max_rect(botoes).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if f.pergunta {
                if botao(ui, "Sim", Estilo::Principal, ALTURA_BOTAO).clicked() {
                    pedido = Some(PedidoVoz::Sim);
                }
                if botao(ui, "Não", Estilo::Secundario, ALTURA_BOTAO).clicked() {
                    pedido = Some(PedidoVoz::Nao);
                }
            } else if let Some(t) = f.abrir
                && botao(ui, "Abrir", Estilo::Principal, ALTURA_BOTAO).clicked()
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
    fn tempo_fora_em_minutos_horas_e_dias() {
        let m = |n: u64| Duration::from_secs(n * 60);
        assert_eq!(tempo_fora(m(0)), "1 min");
        assert_eq!(tempo_fora(m(47)), "47 min");
        assert_eq!(tempo_fora(m(120)), "2 h");
        assert_eq!(tempo_fora(m(80)), "1 h 20");
        assert_eq!(tempo_fora(m(1440)), "1 dia");
        assert_eq!(tempo_fora(m(3000)), "2 dias");
    }

    #[test]
    fn selo_com_um_aviso_e_o_numero_com_mais() {
        assert_eq!(texto_do_selo(0), None);
        assert_eq!(texto_do_selo(1).as_deref(), Some("!"));
        assert_eq!(texto_do_selo(3).as_deref(), Some("3"));
        assert_eq!(texto_do_selo(140).as_deref(), Some("99+"));
    }

    #[test]
    fn titulo_curto_e_so_a_linha_sem_pontuacao() {
        assert!(titulo_curto("O que foi feito na Colmeia"));
        assert!(!titulo_curto("Rotina criada e já rodada uma vez como marco."));
        assert!(!titulo_curto("Exemplo do que saiu:"));
        assert!(!titulo_curto("veja o `arquivo`"));
    }

    #[test]
    fn trechos_separam_negrito_e_codigo() {
        use egui::text::{LayoutJob, TextFormat};
        let (comum, negrito, codigo) = (
            TextFormat::default(),
            TextFormat { font_id: forte(12.0), ..Default::default() },
            TextFormat { font_id: FontId::monospace(12.0), ..Default::default() },
        );
        let mut job = LayoutJob::default();
        trechos("a **b** `c` d **sem par", &mut job, &comum, &negrito, &codigo);
        let textos: Vec<&str> = job.sections.iter().map(|s| &job.text[usize::from(s.byte_range.start)..usize::from(s.byte_range.end)]).collect();
        // O egui junta trechos vizinhos de mesmo formato: o marcador sem par fica no texto comum.
        assert_eq!(textos, vec!["a ", "b", " ", "c", " d **sem par"]);
        assert_eq!(job.sections[1].format.font_id, forte(12.0));
        assert_eq!(job.sections[3].format.font_id, FontId::monospace(12.0));
    }

    #[test]
    fn frase_se_parte_em_texto_e_tabela() {
        let p = partes_da_frase("Sim, é o DNS:\n│a│b│\n├\n│1│2│\nPronto.\n\nFim");
        assert_eq!(
            p,
            vec![
                Ok("Sim, é o DNS:".to_string()),
                Err(vec![vec!["a".to_string(), "b".to_string()], vec!["1".to_string(), "2".to_string()]]),
                Ok("Pronto.\n\nFim".to_string())
            ]
        );
        assert_eq!(partes_da_frase("│a│"), vec![Err(vec![vec!["a".to_string()]])]);
        assert_eq!(partes_da_frase("só texto"), vec![Ok("só texto".to_string())]);
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
