//! A linha de uma tarefa na daily e na sprint: o ícone do estado, o título
//! em negrito e a frase da reunião, já pronta. O lápis aparece só com o
//! mouse em cima e edita na própria linha (Enter salva, Esc desfaz); o resto
//! (abrir a tarefa, ver na apresentação, tirar da daily) fica no botão direito.
//!
//! Desempenho: a linha é um galley só (título e frase), que o egui guarda em
//! cache; o lápis entra e sai com a transição de 150 ms e nada mais anima.

use eframe::egui::{self, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::Registro;
use super::lista::Arrasto;
use crate::api;
use crate::tema::{self, Icone, cores, forte};

const FONTE: f32 = 14.5;
const LAPIS: f32 = 26.0;
const ALCA: f32 = 16.0;

/// O que aparece depois do título.
pub(super) enum Frase<'a> {
    /// A frase da reunião (a nota), pronta.
    Pronta(&'a str),
    /// O agente está escrevendo a frase agora.
    Escrevendo,
    /// Sem frase: uma linha cinza tirada do estado (nada é gravado).
    Estado(&'static str),
}

/// A frase do slide. Na daily, sem a frase de hoje, vale a de ontem
/// (`anterior`); na sprint, a de outra sprint não serve.
pub(super) fn frase_do_slide(s: &api::Slide, escrevendo: bool, anterior: bool) -> Frase<'_> {
    if !s.nota.trim().is_empty() {
        return Frase::Pronta(s.nota.trim());
    }
    if escrevendo {
        return Frase::Escrevendo;
    }
    if let Some(a) = s.nota_anterior.as_ref().filter(|a| anterior && !a.texto.trim().is_empty()) {
        return Frase::Pronta(a.texto.trim());
    }
    Frase::Estado(estado_cinza(s))
}

/// A linha cinza de quem ainda não tem frase.
fn estado_cinza(s: &api::Slide) -> &'static str {
    if s.removida {
        return "A tarefa foi removida.";
    }
    match s.grupo.as_str() {
        "trabalhando" => "Segue em andamento. A frase sai quando for para Revisão.",
        "aguardando" => "Espera minha resposta.",
        "erros" => "Parou com erro.",
        "revisao" => "Em revisão, ainda sem frase.",
        "concluidas" => "Concluída, ainda sem frase.",
        _ => "Ainda sem frase.",
    }
}

/// Como a linha aparece.
pub(super) struct Opcoes<'a> {
    pub projeto: Option<&'a str>,
    pub editada: bool,
    /// O lápis aparece (a nota pode mudar).
    pub editavel: bool,
    /// Arrastar até um assunto (a sprint fixa): a alça no hover.
    pub arrastavel: bool,
    /// Na daily, o botão direito tem "Tirar desta daily".
    pub daily: bool,
}

pub(super) enum Clique {
    Editar,
    AbrirTarefa,
    Apresentar,
    TirarDaDaily,
}

/// O título e a frase num galley só, quebrado na largura.
fn texto(ui: &egui::Ui, s: &api::Slide, frase: &Frase, editada: bool, largura: f32, apagada: bool) -> std::sync::Arc<egui::Galley> {
    let p = cores();
    let tom = |c: Color32| if apagada { c.gamma_multiply(0.4) } else { c };
    let mut job = egui::text::LayoutJob { wrap: egui::text::TextWrapping { max_width: largura, ..Default::default() }, ..Default::default() };
    let normal = |cor: Color32, tamanho: f32| egui::TextFormat {
        font_id: FontId::proportional(tamanho),
        color: tom(cor),
        line_height: Some(22.0),
        ..Default::default()
    };
    let titulo = s.titulo.trim_end_matches('.');
    job.append(&format!("{titulo}. "), 0.0, egui::TextFormat { font_id: forte(FONTE), ..normal(p.texto, FONTE) });
    match frase {
        Frase::Pronta(t) => job.append(&super::comum::sem_linhas_vazias(t).replace('\n', " "), 0.0, normal(p.texto, FONTE)),
        Frase::Escrevendo => job.append("Escrevendo a frase…", 0.0, normal(p.destaque, 13.5)),
        Frase::Estado(t) => job.append(t, 0.0, normal(p.suave, FONTE)),
    }
    if editada {
        job.append("·\u{a0}editada\u{a0}por\u{a0}você", 8.0, normal(p.suave, 12.0));
    }
    ui.painter().layout_job(job)
}

/// Desenha a linha e devolve o que foi pedido nela.
pub(super) fn linha(ui: &mut egui::Ui, s: &api::Slide, frase: &Frase, op: &Opcoes) -> Option<Clique> {
    let p = cores();
    let largura = ui.available_width();
    let arrastando = egui::DragAndDrop::payload::<Arrasto>(ui.ctx()).is_some_and(|a| a.tarefas.contains(&s.tarefa_id));
    let etiqueta = op.projeto.filter(|n| !n.is_empty()).map(|n| ui.painter().layout_no_wrap(n.to_string(), tema::fonte_etiqueta(), p.suave));
    let esquerda = 12.0 + if op.arrastavel { ALCA } else { 0.0 } + 26.0;
    let direita = 10.0 + LAPIS + etiqueta.as_ref().map_or(0.0, |g| g.size().x + 12.0 + 10.0);
    let galeria = texto(ui, s, frase, op.editada, (largura - esquerda - direita).max(120.0), arrastando);
    let altura = galeria.size().y.max(22.0) + 16.0;
    let sentido = if op.arrastavel { Sense::click_and_drag() } else { Sense::click() };
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura, altura), sentido);
    let resposta = if op.arrastavel { resposta.on_hover_cursor(egui::CursorIcon::Grab) } else { resposta };
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let por_cima = resposta.hovered() || resposta.contains_pointer();
    tema::fundo_ao_passar(ui, &resposta, rect, 12, !arrastando);
    let pintor = ui.painter();
    let topo = rect.top() + 8.0;
    // O ícone na altura da primeira linha.
    let mut x = rect.left() + 12.0;
    if op.arrastavel {
        let t = tema::transicao(ui, resposta.id.with("alca"), por_cima);
        if t > 0.0 {
            alca(pintor, pos2(x + 5.0, topo + 11.0), p.suave.gamma_multiply(0.8 * t));
        }
        x += ALCA;
    }
    let (cor, _, marca) = super::comum::estado_do_slide(s);
    tema::marca(pintor, pos2(x + 7.0, topo + 11.0), 5.0, if arrastando { cor.gamma_multiply(0.4) } else { cor }, marca);
    pintor.galley(pos2(rect.left() + esquerda, topo), galeria, p.texto);
    let x_lapis = rect.right() - 6.0 - LAPIS;
    if let Some(g) = etiqueta {
        let pos = pos2(x_lapis - 10.0 - g.size().x - 12.0, topo + 1.5);
        tema::etiqueta(pintor, pos, op.projeto.unwrap_or_default(), tema::fonte_etiqueta(), p.suave);
    }
    let mut clique = None;
    if op.editavel && !arrastando {
        let caixa = Rect::from_min_size(pos2(x_lapis, topo - 2.0), vec2(LAPIS, LAPIS));
        let lapis = ui.interact(caixa, resposta.id.with("lapis"), Sense::click());
        let t = tema::transicao(ui, resposta.id.with("lapis-aparece"), por_cima || lapis.hovered());
        if t > 0.0 {
            if lapis.hovered() {
                ui.painter().rect_filled(caixa, CornerRadius::same(8), p.realce);
            }
            let cor = if lapis.hovered() { p.texto } else { p.suave };
            tema::desenhar_icone(ui.painter(), caixa.center(), Icone::Lapis, cor.gamma_multiply(t));
        }
        if lapis.on_hover_text("Editar a frase").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            clique = Some(Clique::Editar);
        }
    }
    if op.arrastavel && resposta.dragged() {
        resposta.dnd_set_drag_payload(Arrasto { tarefas: vec![s.tarefa_id], rotulo: s.titulo.clone() });
    }
    if resposta.double_clicked() && op.editavel {
        clique = Some(Clique::Editar);
    }
    resposta.context_menu(|ui| {
        ui.set_min_width(200.0);
        if op.editavel && tema::opcao_menu(ui, "Editar a frase", false) {
            clique = Some(Clique::Editar);
            ui.close();
        }
        if !s.removida && tema::opcao_menu(ui, "Abrir a tarefa", false) {
            clique = Some(Clique::AbrirTarefa);
            ui.close();
        }
        if tema::opcao_menu(ui, "Ver na apresentação", false) {
            clique = Some(Clique::Apresentar);
            ui.close();
        }
        if op.daily && tema::opcao_menu(ui, "Tirar desta daily", false) {
            clique = Some(Clique::TirarDaDaily);
            ui.close();
        }
    });
    clique
}

/// Seis pontos: a alça de arrastar.
fn alca(pintor: &egui::Painter, centro: egui::Pos2, cor: Color32) {
    for linha in 0..3 {
        for coluna in 0..2 {
            pintor.circle_filled(centro + vec2(-2.5 + 5.0 * coluna as f32, -5.0 + 5.0 * linha as f32), 1.3, cor);
        }
    }
}

impl Registro {
    /// A linha em edição, no mesmo lugar: o título (só na sprint) e a frase.
    /// Enter salva, Shift+Enter quebra a linha, Esc desfaz.
    pub(super) fn linha_em_edicao(&mut self, ui: &mut egui::Ui, com_titulo: bool) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(e) = self.edicao.as_mut() else { return };
        let id_titulo = Id::new(("titulo-na-linha", e.tarefa));
        let id_frase = Id::new(("frase-na-linha", e.tarefa));
        let foco = ctx.memory(|m| m.has_focus(id_titulo) || m.has_focus(id_frase));
        let enter = foco && ctx.input_mut(|i| self.composicao.tirar_enters(&mut i.events));
        let esc = foco && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let moldura = egui::Frame::new()
            .fill(p.destaque.gamma_multiply(0.07))
            .stroke(Stroke::new(1.0, p.destaque.gamma_multiply(0.45)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 38, right: 12, top: 8, bottom: 8 });
        let resposta = moldura.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            if com_titulo {
                let campo = egui::TextEdit::singleline(&mut e.titulo)
                    .id(id_titulo)
                    .font(forte(FONTE))
                    .hint_text(e.titulo_tarefa.as_str())
                    .desired_width(ui.available_width())
                    .char_limit(200)
                    .interactive(!e.salvando);
                ui.add(campo);
            } else {
                ui.label(tema::texto_forte(format!("{}.", e.titulo.trim_end_matches('.')), FONTE).color(p.texto));
            }
            let r = tema::campo_mensagem(ui, &mut e.descricao, 2, 160.0, id_frase, e.salvando);
            if std::mem::take(&mut e.focar) {
                r.request_focus();
            }
            ui.horizontal(|ui| {
                let dica = if e.salvando { "Salvando…" } else { "Enter salva · Shift+Enter nova linha · Esc desfaz" };
                ui.label(egui::RichText::new(dica).color(p.suave).size(11.5));
                if com_titulo && e.titulo.trim() != e.titulo_tarefa.trim() && !e.salvando && tema::link_com(ui, "Usar o título da tarefa", true).clicked() {
                    e.titulo = e.titulo_tarefa.clone();
                }
            });
            if let Some(aviso) = &e.aviso {
                ui.label(egui::RichText::new(aviso).color(p.erro).size(12.5));
            }
        });
        let marca = resposta.response.rect;
        let (cor, _, forma) = tema::estado_da_tarefa(&e.coluna, e.erro, false);
        tema::marca(ui.painter(), pos2(marca.left() + 19.0, marca.top() + 19.0), 5.0, cor, forma);
        let salvando = e.salvando;
        if esc && !salvando {
            self.edicao = None;
            ui.ctx().request_repaint();
        } else if enter && !salvando {
            self.salvar_item(&ctx);
        }
    }
}

/// O fundo do lugar em que se solta uma tarefa arrastada.
pub(super) fn realce_de_soltar(rect: Rect) -> egui::Shape {
    let p = cores();
    egui::Shape::Rect(egui::epaint::RectShape::new(
        rect,
        CornerRadius::same(14),
        p.destaque.gamma_multiply(0.08),
        Stroke::new(1.0, p.destaque.gamma_multiply(0.45)),
        StrokeKind::Inside,
    ))
}
