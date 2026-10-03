//! Página da daily: a prévia do deck. Os números do período, os cartões das
//! tarefas agrupados como nos slides (concluídas, em revisão, aguardando
//! você, trabalhando, com erro) e o texto para copiar, recolhido.

use eframe::egui::{self, Id, RichText, vec2};

use super::comum;
use super::{Acao, Registro};
use crate::api;
use crate::tema::{self, TipoAviso, cores};

impl Registro {
    pub(super) fn pagina_daily(&mut self, ui: &mut egui::Ui, projetos: &[String], acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let deck = self.deck_daily.clone();
        match &deck {
            Some(d) => tema::cabecalho(ui, &d.titulo, &d.periodo),
            None => tema::cabecalho(ui, "Daily", &escopo(projetos)),
        }
        ui.add_space(20.0);
        if let Some(e) = self.erro_daily.clone() {
            if comum::faixa_erro(ui, &format!("Não consegui montar a daily: {e}")) {
                self.erro_daily = None;
                self.pedir_daily(&ctx);
            }
            ui.add_space(16.0);
        }
        let Some(deck) = deck else {
            if self.erro_daily.is_none() {
                comum::esqueleto_pagina(ui);
            }
            return;
        };
        let topo = ui.cursor().top();
        let saida = egui::ScrollArea::vertical().id_salt("pagina-daily").auto_shrink(false).show(ui, |ui| {
            // A barra de rolagem fica numa faixa só dela, sem nada por baixo.
            ui.set_max_width(ui.available_width() - comum::MARGEM_ROLAGEM);
            if deck.slides.is_empty() && deck.capa.novas.is_empty() {
                let desde = deck.periodo.split(" · ").next().unwrap_or_default().to_string();
                let texto = format!("Nenhuma tarefa trabalhada ({desde}). Quando um agente trabalhar numa tarefa, ela aparece aqui como um slide.");
                comum::vazio(ui, "Nada para a daily", &texto, None);
                return;
            }
            comum::numeros(ui, &deck.capa.numeros, 32.0, true);
            ui.add_space(28.0);
            let com_projeto = projetos.len() > 1;
            for (chave, titulo, cor, marca) in comum::grupos() {
                let slides: Vec<&api::Slide> = deck.slides.iter().filter(|s| s.grupo == chave).collect();
                if slides.is_empty() {
                    continue;
                }
                rotulo_grupo(ui, titulo, slides.len(), cor, marca);
                ui.add_space(8.0);
                let clique = comum::grade(ui, &slides, com_projeto, &mut self.miniaturas, &mut self.rolar_ate, &self.pedidos, self.caixa_aberta);
                if let Some(c) = clique {
                    acoes.push(super::acao_do_clique(c, &deck, None));
                }
                ui.add_space(16.0);
            }
            if deck.slides.is_empty() {
                let novas = deck.capa.novas.join(", ");
                ui.label(RichText::new(format!("Só tarefas novas: {novas}.")).color(p.suave).size(13.5));
            }
            if deck.mais > 0 {
                ui.label(RichText::new(format!("E mais {} tarefas que não cabem na apresentação.", deck.mais)).color(p.suave).size(13.5));
            }
            ui.add_space(12.0);
            self.texto_da_daily(ui, acoes);
            ui.add_space(24.0);
        });
        let area = egui::Rect::from_min_max(egui::pos2(saida.inner_rect.left(), topo), saida.inner_rect.right_bottom());
        comum::sombra_rolagem(ui.painter(), area, saida.state.offset.y);
    }

    fn texto_da_daily(&mut self, ui: &mut egui::Ui, acoes: &mut Vec<Acao>) {
        let Some(daily) = self.daily.clone() else { return };
        let mut aberta = self.texto_daily_aberto;
        ui.horizontal(|ui| {
            tema::secao_recolhivel(ui, "Texto da daily", &mut aberta);
            if aberta {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_secundario_com(ui, "Copiar", !daily.vazio && self.conectado).clicked() {
                        ui.ctx().copy_text(self.texto_daily.clone());
                        acoes.push(Acao::Avisar(TipoAviso::Neutro, "Copiado".into()));
                    }
                    if self.texto_daily != daily.texto {
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Refazer texto").clicked() {
                            self.texto_daily = daily.texto.clone();
                        }
                    }
                });
            }
        });
        self.texto_daily_aberto = aberta;
        if !aberta {
            return;
        }
        ui.add_space(8.0);
        let resposta = tema::campo_multilinha(ui, &mut self.texto_daily, 6, 260.0, Id::new("texto-daily"), daily.vazio);
        // Foco no texto uma vez só (pedir em todo quadro trava os eventos).
        if std::mem::take(&mut self.focar_daily) && !daily.vazio {
            resposta.request_focus();
        }
    }
}

/// "loja-web" ou "todos os projetos".
pub(super) fn escopo(projetos: &[String]) -> String {
    match projetos {
        [um] => um.clone(),
        _ => "todos os projetos".into(),
    }
}

/// Rótulo de um grupo de cartões: a marca do estado, nome e quantos.
pub(super) fn rotulo_grupo(ui: &mut egui::Ui, titulo: &str, n: usize, cor: egui::Color32, marca: tema::Marca) {
    let p = cores();
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), egui::Sense::hover());
        tema::marca(ui.painter(), r.center(), 4.5, cor, marca);
        ui.label(tema::texto_forte(format!("{titulo} · {n}"), 13.0).color(p.suave));
    });
}
