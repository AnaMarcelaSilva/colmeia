//! A frase de uma tarefa na reunião (a nota pronta, o agente escrevendo
//! ou o estado em cinza) e a edição da nota no lugar do painel (Enter
//! salva, Esc desfaz).
use eframe::egui::{self, CornerRadius, Id, Stroke, pos2};

use super::Registro;
use crate::api;
use crate::tema::{self, cores, forte};

const FONTE: f32 = 14.5;

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

pub(crate) enum Clique {
    Editar,
    AbrirTarefa,
    TirarDaDaily,
    TirarDaSprint,
    /// Refazer a montagem (com o montador com IA, se `true`).
    Remontar(bool),
}

impl Registro {
    /// A linha em edição, no mesmo lugar: o título e a frase. Na sprint, o
    /// título vale só nela; na daily, muda o da tarefa.
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
            let r = tema::campo_mensagem(ui, &mut e.descricao, 5, 220.0, id_frase, e.salvando);
            if std::mem::take(&mut e.focar) {
                r.request_focus();
            }
            ui.horizontal(|ui| {
                let dica = if e.salvando {
                    "Salvando…"
                } else {
                    "Enter salva · Shift+Enter nova linha · Esc desfaz · comece a linha com 1. ou - para fazer itens"
                };
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
