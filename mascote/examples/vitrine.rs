//! Vitrine da abelha-robô: os cinco estados em ciclo automático ou escolhidos
//! nos botões. MASCOTE_ESTADO=0..4 abre num estado fixo, sem ciclo.

use eframe::egui::{self, Color32, RichText};
use mascote::{Estado, Tempo};

const FUNDO: Color32 = Color32::from_rgb(0x0e, 0x11, 0x17);
const PAINEL: Color32 = Color32::from_rgb(0x15, 0x1a, 0x22);
const BORDA: Color32 = Color32::from_rgb(0x26, 0x2d, 0x39);
const TEXTO: Color32 = Color32::from_rgb(0xe4, 0xe8, 0xef);
const SUAVE: Color32 = Color32::from_rgb(0x8e, 0x97, 0xa8);
const DESTAQUE: Color32 = Color32::from_rgb(0xb7, 0x94, 0xf6);
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

        egui::CentralPanel::default().frame(egui::Frame::new().fill(FUNDO).inner_margin(20)).show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let cartao = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), area.height() - 48.0));
            let pintor = ui.painter_at(cartao);
            pintor.rect_filled(cartao, 12.0, PAINEL);
            let tamanho = (cartao.height() * 0.6).min(cartao.width() * 0.5);
            mascote::desenhar(&pintor, cartao.center() - egui::vec2(0.0, 26.0), tamanho, self.estado, tempo);
            pintor.text(
                cartao.center_bottom() - egui::vec2(0.0, 52.0),
                egui::Align2::CENTER_CENTER,
                self.estado.nome(),
                egui::FontId::proportional(18.0),
                TEXTO,
            );
            pintor.text(
                cartao.center_bottom() - egui::vec2(0.0, 28.0),
                egui::Align2::CENTER_CENTER,
                self.estado.quando(),
                egui::FontId::proportional(13.5),
                SUAVE,
            );

            ui.allocate_space(egui::vec2(area.width(), cartao.height() + 12.0));
            ui.horizontal(|ui| {
                for estado in Estado::TODOS {
                    let ativo = estado == self.estado;
                    let botao = egui::Button::new(RichText::new(estado.nome()).color(if ativo { DESTAQUE } else { TEXTO }))
                        .stroke(egui::Stroke::new(1.0, if ativo { DESTAQUE } else { BORDA }))
                        .corner_radius(14);
                    if ui.add(botao).clicked() {
                        self.estado = estado;
                        self.desde = agora;
                        self.ciclo = false;
                    }
                }
                ui.add_space(12.0);
                ui.checkbox(&mut self.ciclo, RichText::new("Ciclo automático").color(TEXTO));
            });
        });

        // A vitrine anima sem parar; no app, a abelha só anima na troca de estado.
        ui.ctx().request_repaint();
    }
}

fn main() -> eframe::Result {
    let fixo = std::env::var("MASCOTE_ESTADO").ok().and_then(|v| v.parse::<usize>().ok()).and_then(|i| Estado::TODOS.get(i).copied());
    let (estado, ciclo) = fixo.map_or((Estado::Dormindo, true), |e| (e, false));
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Colmeia · mascote").with_inner_size([720.0, 520.0]),
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
