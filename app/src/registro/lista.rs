//! A sprint para a reunião: a escolha da sprint fixa (as setas e as datas) e
//! a lista em resumo, uma linha por tarefa com o título e a descrição da
//! sprint. Os dois só valem na sprint: a tarefa, o quadro e a daily
//! continuam como estão. O clique no título abre o cartão completo embaixo.

use eframe::egui::{self, CornerRadius, FontId, RichText, Sense, Stroke, vec2};

use super::comum::{self, CliqueCartao, PedidoNoCartao};
use super::{EdicaoItem, Registro};
use crate::api;
use crate::tema::{self, Icone, cores};

/// AAAA-MM-DD vira DD/MM.
fn curta(data: &str) -> String {
    if data.len() == 10 { format!("{}/{}", &data[8..10], &data[5..7]) } else { data.to_string() }
}

/// AAAA-MM-DD vira DD/MM/AAAA (o formato dos campos).
fn da_tela(data: &str) -> String {
    if data.len() == 10 { format!("{}/{}/{}", &data[8..10], &data[5..7], &data[0..4]) } else { data.to_string() }
}

impl Registro {
    /// As setas entre as sprints, as datas da escolhida e "Editar datas".
    pub(super) fn escolha_da_sprint(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(lista) = self.sprints.as_ref() else { return };
        let Some(pos) = self.sprint_escolhida.and_then(|id| lista.sprints.iter().position(|s| s.id == id)) else { return };
        let s = lista.sprints[pos].clone();
        let anterior = pos.checked_sub(1).map(|i| lista.sprints[i].id);
        let proxima = lista.sprints.get(pos + 1).map(|x| x.id);
        let atual = lista.atual;

        if let Some((de, ate)) = self.datas_sprint.as_mut() {
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
            return;
        }

        let mut ir = None;
        ui.horizontal(|ui| {
            if tema::botao_icone_com(ui, Icone::Anterior, 30.0, anterior.is_some()).on_hover_text("Sprint anterior").clicked() {
                ir = anterior;
            }
            ui.label(tema::texto_forte(format!("{} a {}", curta(&s.inicio), curta(&s.fim)), 15.0).color(p.texto));
            if tema::botao_icone_com(ui, Icone::Proximo, 30.0, proxima.is_some()).on_hover_text("Próxima sprint").clicked() {
                ir = proxima;
            }
            let quando = if s.id == atual {
                "sprint atual"
            } else if proxima.is_none() || s.inicio > lista.sprints.iter().find(|x| x.id == atual).map(|x| x.inicio.clone()).unwrap_or_default() {
                "sprint futura"
            } else {
                "sprint passada"
            };
            ui.label(RichText::new(quando).color(p.suave).size(13.0));
            ui.add_space(12.0);
            if tema::link_com(ui, "Editar datas", self.conectado).on_hover_text("Mudar o começo e o fim desta sprint").clicked() {
                self.datas_sprint = Some((da_tela(&s.inicio), da_tela(&s.fim)));
                self.erro_datas = None;
            }
            if s.id != atual {
                ui.add_space(8.0);
                if tema::link_com(ui, "Ir para a atual", true).clicked() {
                    ir = Some(atual);
                }
            }
        });
        if let Some(id) = ir {
            self.escolher_sprint(&ctx, id);
        }
    }

    /// A lista em resumo de uma seção: título, estado e descrição de cada
    /// tarefa. Na sprint fixa, o lápis edita o título e a descrição dela.
    pub(super) fn lista(&mut self, ui: &mut egui::Ui, deck: &api::Deck, slides: &[&api::Slide], com_projeto: bool) -> Option<CliqueCartao> {
        let p = cores();
        let editavel = deck.sprint_id != 0 && self.conectado;
        let mut clique = None;
        for s in slides {
            let id = s.tarefa_id;
            let aberto = self.abertos.contains(&id);
            let editando = self.edicao.as_ref().is_some_and(|e| e.tarefa == id);
            let (mut alternar, mut editar) = (false, false);
            let moldura = egui::Frame::new()
                .fill(p.superficie_alta)
                .stroke(Stroke::new(1.0, if editando { p.destaque.gamma_multiply(0.55) } else { p.borda }))
                .corner_radius(CornerRadius::same(tema::RAIO_CARTAO))
                .inner_margin(egui::Margin::symmetric(16, 12));
            let resposta = moldura.show(ui, |ui| {
                ui.set_width(ui.available_width());
                if editando {
                    self.formulario(ui, id);
                    return;
                }
                // O título, com o lápis e "Ver detalhes" à direita.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let rotulo = if aberto { "Esconder detalhes" } else { "Ver detalhes" };
                        alternar = tema::link_com(ui, rotulo, true).clicked();
                        if editavel && !s.removida {
                            ui.add_space(4.0);
                            editar = tema::botao_icone(ui, Icone::Lapis, 28.0).on_hover_text("Editar o título e a descrição só nesta sprint").clicked();
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let titulo = egui::Label::new(tema::texto_forte(&s.titulo, 15.0).color(p.texto)).truncate().sense(Sense::click());
                            alternar |= ui.add(titulo).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
                        });
                    });
                });
                // Estado, projeto e, se o título mudou, o da tarefa.
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let pilula = comum::pilula_do_slide(s, false);
                    let (r, _) = ui.allocate_exact_size(vec2(pilula.largura(ui.painter()), 22.0), Sense::hover());
                    pilula.pintar(ui.painter(), r.min);
                    if com_projeto && !s.projeto.is_empty() {
                        tema::etiqueta_ui(ui, &s.projeto, tema::fonte_etiqueta(), p.suave);
                    }
                    if !s.titulo_original.is_empty() {
                        ui.add(egui::Label::new(RichText::new(format!("Na tarefa: {}", s.titulo_original)).color(p.suave).size(12.5)).truncate());
                    }
                });
                ui.add_space(6.0);
                let nota = comum::sem_linhas_vazias(&s.nota);
                if nota.trim().is_empty() {
                    let texto = if editavel { "Sem descrição. Use o lápis para escrever o que contar na reunião." } else { "Sem descrição." };
                    ui.label(RichText::new(texto).color(p.suave).italics().size(13.5));
                } else {
                    let largura = ui.available_width();
                    let formato = egui::TextFormat { font_id: FontId::proportional(13.5), color: p.texto, ..Default::default() };
                    let galeria = tema::cortar(ui.painter(), &nota, formato, largura, 3, false);
                    let (r, _) = ui.allocate_exact_size(vec2(largura, galeria.size().y), Sense::hover());
                    ui.painter().galley(r.min, galeria, p.texto);
                }
            });
            if editar {
                self.abrir_edicao(s);
            }
            if alternar && !self.abertos.remove(&id) {
                self.abertos.insert(id);
            }
            // O cartão completo, embaixo do item aberto.
            if aberto && !editando {
                ui.add_space(6.0);
                let largura = ui.available_width();
                let altura = comum::altura_cartao(ui, s, largura);
                let (rect, _) = ui.allocate_exact_size(vec2(largura, altura), Sense::hover());
                let resumo = self.pedidos.get(&id);
                let info = PedidoNoCartao {
                    estado: resumo.map(|r| &r.estado),
                    caixa_aberta: self.caixa_aberta == Some(id),
                    respondido: resumo.is_some_and(|r| r.pedido.estado == "respondido"),
                };
                if let Some(c) = comum::cartao(ui, rect, s, com_projeto, &mut self.miniaturas, &mut self.rolar_ate, &info, false) {
                    clique = Some(c);
                }
            }
            if self.rolar_ate == Some(id) {
                ui.scroll_to_rect(resposta.response.rect, Some(egui::Align::Center));
                self.rolar_ate = None;
            }
            ui.add_space(8.0);
        }
        clique
    }

    fn abrir_edicao(&mut self, s: &api::Slide) {
        let titulo_tarefa = if s.titulo_original.is_empty() { s.titulo.clone() } else { s.titulo_original.clone() };
        self.edicao = Some(EdicaoItem {
            tarefa: s.tarefa_id,
            titulo: s.titulo.clone(),
            descricao: s.nota.clone(),
            titulo_tarefa,
            titulo_antes: s.titulo.clone(),
            descricao_antes: s.nota.clone(),
            versao: s.nota_versao.clone(),
            salvando: false,
            focar: true,
            aviso: None,
        });
    }

    /// O título e a descrição da tarefa na sprint, com Salvar e Cancelar.
    fn formulario(&mut self, ui: &mut egui::Ui, tarefa: i64) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(e) = self.edicao.as_mut() else { return };
        let (mut salvar, mut cancelar) = (false, false);
        let r = tema::campo(ui, "Título na sprint", &mut e.titulo, &e.titulo_tarefa.clone());
        if std::mem::take(&mut e.focar) {
            r.request_focus();
        }
        salvar |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(8.0);
        ui.label(RichText::new("Descrição").color(p.suave).size(12.5));
        ui.add_space(2.0);
        tema::campo_multilinha(ui, &mut e.descricao, 3, 220.0, egui::Id::new(("descricao-sprint", tarefa)), e.salvando);
        ui.add_space(6.0);
        ui.label(
            RichText::new("Vale só para esta sprint: a tarefa, o quadro e a daily continuam como estão. A descrição também é a nota do slide.")
                .color(p.suave)
                .size(12.5),
        );
        if let Some(aviso) = &e.aviso {
            ui.add_space(4.0);
            ui.label(RichText::new(aviso).color(p.erro).size(13.0));
        }
        ui.add_space(10.0);
        let mut usar_original = false;
        ui.horizontal(|ui| {
            salvar |= tema::botao_principal(ui, if e.salvando { "Salvando…" } else { "Salvar" }, !e.salvando).clicked();
            cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
            if e.titulo.trim() != e.titulo_tarefa.trim() {
                ui.add_space(8.0);
                usar_original = tema::link_com(ui, "Usar o título da tarefa", !e.salvando).clicked();
            }
        });
        if usar_original {
            e.titulo = e.titulo_tarefa.clone();
        }
        if cancelar || (!e.salvando && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.edicao = None;
        } else if salvar {
            self.salvar_item(&ctx);
        }
    }
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
