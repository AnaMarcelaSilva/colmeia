//! As peças da sprint fixa: a escolha da sprint (as setas e as datas), o
//! arrasto de uma tarefa até um assunto e o nome de um assunto sendo
//! escrito. Os assuntos juntam tarefas de projetos diferentes; o projeto da
//! tarefa não muda.

use eframe::egui::{self, CornerRadius, FontId, Id, RichText, Stroke, vec2};

use super::Registro;
use crate::tema::{self, Icone, cores};

/// AAAA-MM-DD vira DD/MM.
fn curta(data: &str) -> String {
    if data.len() == 10 { format!("{}/{}", &data[8..10], &data[5..7]) } else { data.to_string() }
}

/// AAAA-MM-DD vira DD/MM/AAAA (o formato dos campos).
fn da_tela(data: &str) -> String {
    if data.len() == 10 { format!("{}/{}/{}", &data[8..10], &data[5..7], &data[0..4]) } else { data.to_string() }
}

/// O que está sendo arrastado: as tarefas (uma ou um projeto inteiro) e o
/// rótulo que acompanha o ponteiro (também o nome de um assunto novo).
pub(super) struct Arrasto {
    pub tarefas: Vec<i64>,
    pub rotulo: String,
}

/// O rótulo que segue o ponteiro durante o arrasto.
pub(super) fn rotulo_do_arrasto(ctx: &egui::Context) {
    let p = cores();
    let (Some(a), Some(ponto)) = (egui::DragAndDrop::payload::<Arrasto>(ctx), ctx.pointer_interact_pos()) else { return };
    let pintor = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("arrasto-assunto")));
    let texto = if a.tarefas.len() == 1 { a.rotulo.clone() } else { format!("{} · {} tarefas", a.rotulo, a.tarefas.len()) };
    let galeria = pintor.layout_no_wrap(texto, FontId::proportional(13.5), p.texto);
    let caixa = egui::Rect::from_min_size(ponto + vec2(14.0, 10.0), galeria.size() + vec2(20.0, 12.0));
    pintor.rect(caixa, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie_alta, Stroke::new(1.0, p.destaque), egui::StrokeKind::Inside);
    pintor.galley(caixa.min + vec2(10.0, 6.0), galeria, p.texto);
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
}

/// Durante o arrasto, a lista rola: com a roda do mouse (que o egui ignora
/// enquanto algo é arrastado) e sozinha com o ponteiro perto da borda de
/// cima ou de baixo. Chamada dentro da área de rolagem.
pub(super) fn rolar_no_arrasto(ui: &egui::Ui) {
    let ctx = ui.ctx();
    let (true, Some(ponto)) = (egui::DragAndDrop::has_payload_of_type::<Arrasto>(ctx), ctx.pointer_hover_pos()) else { return };
    let visivel = ui.clip_rect();
    if !visivel.x_range().contains(ponto.x) {
        return;
    }
    let mut delta = ctx.input(|i| i.smooth_scroll_delta.y);
    const BORDA: f32 = 70.0;
    let perto_do_topo = BORDA - (ponto.y - visivel.top());
    let perto_do_fim = BORDA - (visivel.bottom() - ponto.y);
    if perto_do_topo > 0.0 {
        delta += perto_do_topo.min(BORDA * 1.5) * 0.3;
    } else if perto_do_fim > 0.0 {
        delta -= perto_do_fim.min(BORDA * 1.5) * 0.3;
    }
    if delta != 0.0 {
        ui.scroll_with_delta_animation(vec2(0.0, delta), egui::style::ScrollAnimation::none());
        ctx.request_repaint();
    }
}

impl Registro {
    /// As datas da sprint escolhida em edição (no lugar das setas). Devolve
    /// se está editando.
    pub(super) fn datas_em_edicao(&mut self, ui: &mut egui::Ui) -> bool {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some((de, ate)) = self.datas_sprint.as_mut() else { return false };
        let (mut salvar, mut cancelar) = (false, false);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(150.0);
                tema::campo(ui, "Começa em", de, "DD/MM/AAAA");
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_width(150.0);
                salvar = tema::campo(ui, "Termina em", ate, "DD/MM/AAAA").lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.add_space(19.0);
                ui.horizontal(|ui| {
                    salvar |= tema::botao_principal(ui, "Salvar", self.conectado).clicked();
                    cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
                });
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new("A próxima sprint segue estas datas: começa no dia seguinte, com a mesma duração.").color(p.suave).size(12.5));
        if let Some(e) = &self.erro_datas {
            ui.label(RichText::new(e).color(p.erro).size(13.0));
        }
        if cancelar || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.datas_sprint = None;
            self.erro_datas = None;
        } else if salvar {
            self.salvar_datas(&ctx);
        }
        true
    }

    /// As setas entre as sprints, as datas da escolhida e "Editar datas",
    /// dentro de uma linha de quem chama.
    pub(super) fn setas_da_sprint(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(lista) = self.sprints.as_ref() else { return };
        let Some(pos) = self.sprint_escolhida.and_then(|id| lista.sprints.iter().position(|s| s.id == id)) else { return };
        let s = lista.sprints[pos].clone();
        let anterior = pos.checked_sub(1).map(|i| lista.sprints[i].id);
        let proxima = lista.sprints.get(pos + 1).map(|x| x.id);
        let atual = lista.atual;
        let quando = if s.id == atual {
            "sprint atual"
        } else if proxima.is_none() || s.inicio > lista.sprints.iter().find(|x| x.id == atual).map(|x| x.inicio.clone()).unwrap_or_default() {
            "sprint futura"
        } else {
            "sprint passada"
        };
        let mut ir = None;
        if tema::botao_icone_com(ui, Icone::Anterior, 30.0, anterior.is_some()).on_hover_text("Sprint anterior").clicked() {
            ir = anterior;
        }
        ui.label(tema::texto_forte(format!("{} a {}", curta(&s.inicio), curta(&s.fim)), 15.0).color(p.texto));
        if tema::botao_icone_com(ui, Icone::Proximo, 30.0, proxima.is_some()).on_hover_text("Próxima sprint").clicked() {
            ir = proxima;
        }
        ui.label(RichText::new(quando).color(p.suave).size(13.0));
        ui.add_space(8.0);
        if tema::link_com(ui, "Editar datas", self.conectado).on_hover_text("Mudar o começo e o fim desta sprint").clicked() {
            self.datas_sprint = Some((da_tela(&s.inicio), da_tela(&s.fim)));
            self.erro_datas = None;
        }
        if s.id != atual && tema::link_com(ui, "Ir para a atual", true).clicked() {
            ir = Some(atual);
        }
        if let Some(id) = ir {
            self.escolher_sprint(&ctx, id);
        }
    }
}

/// O nome de um assunto sendo escrito: `Some(true)` com Enter (salva),
/// `Some(false)` ao sair do campo de outro jeito (Esc ou clique fora).
pub(super) fn campo_nome(ui: &mut egui::Ui, nome: &mut String, id: i64) -> Option<bool> {
    let campo = egui::TextEdit::singleline(nome).id(Id::new(("nome-assunto", id))).hint_text("Nome do assunto").desired_width(200.0).char_limit(80);
    let r = ui.add(campo);
    if r.lost_focus() {
        return Some(ui.input(|i| i.key_pressed(egui::Key::Enter)));
    }
    if !r.has_focus() && ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_da_sprint_na_tela() {
        assert_eq!(curta("2026-10-02"), "02/10");
        assert_eq!(da_tela("2026-10-08"), "08/10/2026");
        assert_eq!(curta("x"), "x");
    }
}
