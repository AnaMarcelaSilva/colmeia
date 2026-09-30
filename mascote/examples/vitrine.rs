//! Vitrine dos dois mascotes: mostra os cinco estados, em ciclo automático
//! ou escolhidos nos botões. MASCOTE_ESTADO=0..4 abre num estado fixo, sem ciclo.

use eframe::egui::{self, Color32, RichText};
use mascote::{Estado, Modelo, Tempo};

const FUNDO: Color32 = Color32::from_rgb(0x0b, 0x0d, 0x10);
const PAINEL: Color32 = Color32::from_rgb(0x14, 0x17, 0x1c);
const TEXTO: Color32 = Color32::from_rgb(0xd7, 0xda, 0xe0);
const SUAVE: Color32 = Color32::from_rgb(0x8a, 0x90, 0xa0);
const DESTAQUE: Color32 = Color32::from_rgb(0xc7, 0x92, 0xea);
const SEGUNDOS_POR_ESTADO: f64 = 3.0;

struct Vitrine {
    estado: Estado,
    desde: f64,
    ciclo: bool,
}

impl eframe::App for Vitrine {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let agora = ui.input(|i| i.time);
        if self.ciclo && agora - self.desde >= SEGUNDOS_POR_ESTADO {
            let i = Estado::TODOS.iter().position(|e| *e == self.estado).unwrap_or(0);
            self.estado = Estado::TODOS[(i + 1) % Estado::TODOS.len()];
            self.desde = agora;
        }
        let tempo = Tempo { total: agora as f32, no_estado: (agora - self.desde) as f32 };

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(FUNDO).inner_margin(24))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mascote").color(TEXTO).strong().size(18.0));
                    ui.label(RichText::new(format!("{}: {}", self.estado.nome(), self.estado.quando())).color(SUAVE).size(15.0));
                });
                ui.add_space(12.0);

                let area = ui.available_rect_before_wrap();
                let altura_cartoes = area.height() - 56.0;
                let largura = (area.width() - 16.0) / 2.0;
                for (i, modelo) in [Modelo::Robo, Modelo::Abelha].into_iter().enumerate() {
                    let caixa = egui::Rect::from_min_size(area.min + egui::vec2(i as f32 * (largura + 16.0), 0.0), egui::vec2(largura, altura_cartoes));
                    let pintor = ui.painter_at(caixa);
                    pintor.rect_filled(caixa, 10.0, PAINEL);
                    let tamanho = (caixa.height() * 0.55).min(caixa.width() * 0.5);
                    mascote::desenhar(&pintor, caixa.center() - egui::vec2(0.0, 16.0), tamanho, modelo, self.estado, tempo);
                    pintor.text(
                        egui::pos2(caixa.center().x, caixa.bottom() - 28.0),
                        egui::Align2::CENTER_CENTER,
                        modelo.nome(),
                        egui::FontId::proportional(16.0),
                        TEXTO,
                    );
                }

                ui.allocate_space(egui::vec2(area.width(), altura_cartoes + 16.0));
                ui.horizontal(|ui| {
                    for estado in Estado::TODOS {
                        let ativo = estado == self.estado;
                        let botao = egui::Button::new(RichText::new(estado.nome()).color(if ativo { DESTAQUE } else { TEXTO }))
                            .stroke(egui::Stroke::new(1.0, if ativo { DESTAQUE } else { Color32::from_rgb(0x26, 0x2a, 0x33) }));
                        if ui.add(botao).clicked() {
                            self.estado = estado;
                            self.desde = agora;
                            self.ciclo = false;
                        }
                    }
                    ui.add_space(16.0);
                    ui.checkbox(&mut self.ciclo, RichText::new("Ciclo automático").color(TEXTO));
                });
            });

        // Vitrine anima sem parar; no app, o mascote só anima na troca de estado.
        ui.ctx().request_repaint();
    }
}

fn main() -> eframe::Result {
    let fixo = std::env::var("MASCOTE_ESTADO").ok().and_then(|v| v.parse::<usize>().ok()).and_then(|i| Estado::TODOS.get(i).copied());
    let (estado, ciclo) = fixo.map_or((Estado::Dormindo, true), |e| (e, false));
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Mascote · protótipo").with_inner_size([1100.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "mascote",
        opcoes,
        Box::new(move |cc| {
            cc.egui_ctx.global_style_mut(|s| s.interaction.selectable_labels = false);
            Ok(Box::new(Vitrine { estado, desde: 0.0, ciclo }))
        }),
    )
}
