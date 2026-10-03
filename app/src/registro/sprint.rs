//! Página da sprint: o período, os números (com o tempo por ferramenta), as
//! tarefas por projeto na ordem do deck e a galeria das capturas.

use eframe::egui::{self, CornerRadius, FontId, Rect, RichText, Sense, pos2, vec2};

use super::comum::{self, desenhar_miniatura};
use super::{Acao, Periodo, Registro};
use crate::api;
use crate::tema::{self, cores};

impl Registro {
    pub(super) fn pagina_sprint(&mut self, ui: &mut egui::Ui, projetos: &[String], acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let deck = self.deck_sprint.clone();
        match &deck {
            Some(d) => tema::cabecalho(ui, &d.titulo, &d.periodo),
            None => tema::cabecalho(ui, "Sprint", &super::daily::escopo(projetos)),
        }
        ui.add_space(12.0);
        let opcoes = [Periodo::Dias7, Periodo::Dias14, Periodo::Mes, Periodo::Escolher];
        let atual = opcoes.iter().position(|o| *o == self.periodo).unwrap_or(1);
        if let Some(i) = tema::segmentado(ui, &["7 dias", "14 dias", "Este mês", "Escolher…"], atual)
            && opcoes[i] != self.periodo
        {
            self.periodo = opcoes[i];
            self.sprint = None;
            self.deck_sprint = None;
            self.pedir_sprint(&ctx);
        }
        if self.periodo == Periodo::Escolher {
            ui.add_space(10.0);
            let (de, ate) = (self.de.clone(), self.ate.clone());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(180.0);
                    tema::campo(ui, "De", &mut self.de, "DD/MM/AAAA");
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(180.0);
                    tema::campo(ui, "Até", &mut self.ate, "DD/MM/AAAA");
                });
            });
            // Pede quando as duas datas estão completas e mudaram.
            if (de != self.de || ate != self.ate) && self.de.trim().len() == 10 && self.ate.trim().len() == 10 {
                self.sprint = None;
                self.deck_sprint = None;
                self.pedir_sprint(&ctx);
            }
        }
        ui.add_space(20.0);
        if let Some(e) = self.erro_sprint.clone() {
            ui.label(RichText::new(e).color(p.erro).size(13.5));
            ui.add_space(12.0);
        }
        let Some(deck) = deck else {
            if self.erro_sprint.is_none() && (self.periodo != Periodo::Escolher || self.periodo_atual().is_some()) {
                comum::esqueleto_pagina(ui);
            }
            return;
        };
        let topo = ui.cursor().top();
        let saida = egui::ScrollArea::vertical().id_salt("pagina-sprint").auto_shrink(false).show(ui, |ui| {
            // A barra de rolagem fica numa faixa só dela, sem nada por baixo.
            ui.set_max_width(ui.available_width() - comum::MARGEM_ROLAGEM);
            if deck.slides.is_empty() && deck.capa.novas.is_empty() {
                let periodo = deck.titulo.trim_start_matches("Sprint · ").replace(" a ", " e ");
                ui.label(RichText::new(format!("Nada aconteceu entre {periodo}.")).color(p.suave).size(13.5));
                ui.add_space(12.0);
                let (rotulo, proximo) = match self.periodo {
                    Periodo::Dias7 => (Some("Ver 14 dias"), Periodo::Dias14),
                    Periodo::Dias14 => (Some("Ver este mês"), Periodo::Mes),
                    _ => (None, self.periodo),
                };
                if let Some(rotulo) = rotulo
                    && tema::botao_secundario(ui, rotulo).clicked()
                {
                    self.periodo = proximo;
                    self.sprint = None;
                    self.deck_sprint = None;
                    self.pedir_sprint(&ctx);
                }
                return;
            }
            // A barra por ferramenta ao lado dos números quando cabe; senão, numa linha própria.
            let ferramentas = &deck.capa.numeros.por_ferramenta;
            let largura_numeros = comum::largura_numeros(ui.painter(), &deck.capa.numeros, 32.0, true);
            if ferramentas.is_empty() {
                comum::numeros(ui, &deck.capa.numeros, 32.0, true);
            } else if largura_numeros + 40.0 + LARGURA_BARRA <= ui.available_width() {
                ui.horizontal(|ui| {
                    comum::numeros(ui, &deck.capa.numeros, 32.0, true);
                    ui.add_space(40.0 - ui.spacing().item_spacing.x);
                    por_ferramenta(ui, ferramentas);
                });
            } else {
                comum::numeros(ui, &deck.capa.numeros, 32.0, true);
                ui.add_space(4.0);
                por_ferramenta(ui, ferramentas);
            }
            ui.add_space(32.0);
            let mut secoes: Vec<&str> = deck.slides.iter().map(|s| s.secao.as_str()).collect();
            secoes.dedup();
            let com_titulo = secoes.len() > 1;
            for secao in secoes {
                let slides: Vec<&api::Slide> = deck.slides.iter().filter(|s| s.secao == secao).collect();
                if com_titulo {
                    ui.horizontal(|ui| {
                        ui.label(tema::texto_forte(secao, 17.0).color(p.texto));
                        let n = slides.len();
                        ui.label(RichText::new(if n == 1 { "· 1 tarefa".to_string() } else { format!("· {n} tarefas") }).color(p.suave).size(13.5));
                    });
                    ui.add_space(8.0);
                }
                if let Some(tarefa) = comum::grade(ui, &slides, false, &mut self.miniaturas, &mut self.rolar_ate) {
                    acoes.push(Acao::Apresentar { deck: Box::new(deck.clone()), periodo: self.periodo_atual(), tarefa: Some(tarefa) });
                }
                ui.add_space(32.0 - 12.0);
            }
            if deck.mais > 0 {
                ui.label(RichText::new(format!("E mais {} tarefas que não cabem na apresentação.", deck.mais)).color(p.suave).size(13.5));
                ui.add_space(12.0);
            }
            self.galeria(ui, acoes);
            ui.add_space(24.0);
        });
        let area = egui::Rect::from_min_max(egui::pos2(saida.inner_rect.left(), topo), saida.inner_rect.right_bottom());
        comum::sombra_rolagem(ui.painter(), area, saida.state.offset.y);
    }

    /// Todas as capturas do período, recolhidas. O clique abre o visor.
    fn galeria(&mut self, ui: &mut egui::Ui, _acoes: &mut [Acao]) {
        let Some(sprint) = self.sprint.clone() else { return };
        if sprint.capturas.is_empty() {
            return;
        }
        let ctx = ui.ctx().clone();
        let mut aberta = self.galeria_aberta;
        tema::secao_recolhivel(ui, &format!("Galeria · {}", sprint.capturas.len()), &mut aberta);
        self.galeria_aberta = aberta;
        if !aberta {
            return;
        }
        ui.add_space(8.0);
        let largura = ui.available_width();
        let mut x = 0.0;
        let mut linha: Option<Rect> = None;
        let mut abrir = None;
        for c in &sprint.capturas {
            let w = match self.miniaturas.ver(c.anexo) {
                Some(comum::Miniatura::Pronta(t)) => (96.0 * t.size_vec2().x / t.size_vec2().y.max(1.0)).clamp(64.0, 192.0),
                _ => 128.0,
            };
            if linha.is_none() || x + w > largura {
                let (r, _) = ui.allocate_exact_size(vec2(largura, 96.0), Sense::hover());
                ui.add_space(8.0 - ui.spacing().item_spacing.y);
                linha = Some(r);
                x = 0.0;
            }
            let base = linha.expect("linha da galeria");
            let caixa = Rect::from_min_size(pos2(base.left() + x, base.top()), vec2(w, 96.0));
            let resposta = desenhar_miniatura(ui, &mut self.miniaturas, caixa, c.anexo, tema::RAIO_CONTROLE).on_hover_text(format!("{} · {}", c.texto, c.dia));
            if resposta.clicked() {
                abrir = Some((c.anexo, c.texto.clone(), c.dia.clone(), c.tarefa_id));
            }
            x += w + 8.0;
        }
        if let Some((anexo, texto, dia, tarefa)) = abrir {
            self.abrir_visor(&ctx, anexo, &texto, &dia, tarefa);
        }
    }
}

const LARGURA_BARRA: f32 = 320.0;

/// Barra fina empilhada com o tempo de cada ferramenta e a legenda embaixo.
/// As cores saem da paleta ANSI do tema (já ajustada nos três temas).
fn por_ferramenta(ui: &mut egui::Ui, lista: &[api::TempoFerramenta]) {
    let p = cores();
    let cores_barra = [p.ansi[4], p.ansi[5], p.ansi[6], p.ansi[3]];
    let total: i64 = lista.iter().map(|f| f.segundos).sum::<i64>().max(1);
    ui.vertical(|ui| {
        ui.set_max_width(LARGURA_BARRA.min(ui.available_width()).max(160.0));
        ui.add_space(14.0);
        let (barra, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
        ui.painter().rect_filled(barra, CornerRadius::same(4), p.superficie);
        let mut x = barra.left();
        for (i, f) in lista.iter().enumerate() {
            let cor = cores_barra.get(i).copied().unwrap_or(p.suave);
            let w = barra.width() * f.segundos as f32 / total as f32;
            let parte = Rect::from_min_size(pos2(x, barra.top()), vec2((w - 2.0).max(1.0), 8.0));
            ui.painter().rect_filled(parte, CornerRadius::same(4), cor);
            x += w;
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (i, f) in lista.iter().enumerate() {
                let cor = cores_barra.get(i).copied().unwrap_or(p.suave);
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, cor);
                ui.label(RichText::new(&f.nome).color(p.texto).size(12.5));
                ui.label(RichText::new(comum::duracao(f.segundos)).font(FontId::proportional(12.5)).color(p.suave));
                ui.add_space(8.0);
            }
        });
    });
}
