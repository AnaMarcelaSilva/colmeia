//! Vitrine da abelha-robô: os cinco estados em ciclo automático ou escolhidos
//! nos botões, em cada modelo. MASCOTE_ESTADO=0..4 abre num estado fixo, sem
//! ciclo; MASCOTE_MODELO=antiga (ou meio-termo, favo, zangao: os códigos do
//! perfil) abre num modelo. Parados: MASCOTE_GALERIA=1 mostra todos os modelos
//! em todos os estados, MASCOTE_GALERIA=modelos os cinco lado a lado (grandes e
//! a 40 px) e MASCOTE_GALERIA=estados o modelo escolhido nos cinco estados.

use eframe::egui::{self, Color32, RichText};
use mascote::{Estado, Modelo, Tempo};

// As cores do tema escuro da Colmeia.
const FUNDO: Color32 = Color32::from_rgb(0x0b, 0x0d, 0x15);
const PAINEL: Color32 = Color32::from_rgb(0x12, 0x14, 0x1f);
const BORDA: Color32 = Color32::from_rgb(0x2c, 0x32, 0x48);
const TEXTO: Color32 = Color32::from_rgb(0xe7, 0xe9, 0xf2);
const SUAVE: Color32 = Color32::from_rgb(0x9d, 0xa4, 0xba);
const DESTAQUE: Color32 = Color32::from_rgb(0xae, 0x8f, 0xf7);
/// O fundo do diálogo da abelha no tema escuro, o das prévias em Ajustes.
const DIALOGO: Color32 = Color32::from_rgb(0x1e, 0x22, 0x33);
/// O instante das galerias, o mesmo das prévias em Ajustes.
const INSTANTE: Tempo = Tempo { total: 1.3, no_estado: 0.6 };
const SEGUNDOS_POR_ESTADO: f64 = 3.0;

#[derive(Clone, Copy, PartialEq)]
enum Galeria {
    Nenhuma,
    Todos,
    Modelos,
    Estados,
}

struct Vitrine {
    modelo: Modelo,
    estado: Estado,
    desde: f64,
    ciclo: bool,
    galeria: Galeria,
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
        match self.galeria {
            Galeria::Nenhuma => {}
            Galeria::Todos => return galeria(ui),
            Galeria::Modelos => return lado_a_lado(ui),
            Galeria::Estados => return estados(ui, self.modelo),
        }

        egui::CentralPanel::default().frame(egui::Frame::new().fill(FUNDO).inner_margin(20)).show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let cartao = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), area.height() - 48.0));
            let pintor = ui.painter_at(cartao);
            pintor.rect_filled(cartao, 12.0, PAINEL);
            let tamanho = (cartao.height() * 0.6).min(cartao.width() * 0.5);
            mascote::desenhar(&pintor, cartao.center() - egui::vec2(0.0, 26.0), tamanho, self.modelo, self.estado, tempo);
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

            ui.allocate_space(egui::vec2(area.width(), cartao.height() - 24.0));
            ui.horizontal(|ui| {
                for modelo in Modelo::TODOS {
                    let ativo = modelo == self.modelo;
                    let botao = egui::Button::new(RichText::new(modelo.nome()).color(if ativo { DESTAQUE } else { SUAVE })).frame(false);
                    if ui.add(botao).clicked() {
                        self.modelo = modelo;
                    }
                }
            });
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

/// Uma linha por modelo, uma coluna por estado, num instante fixo.
fn galeria(ui: &mut egui::Ui) {
    let area = ui.max_rect();
    let pintor = ui.painter();
    pintor.rect_filled(area, 0.0, FUNDO);
    let (lado, margem) = (100.0, 130.0);
    for (i, modelo) in Modelo::TODOS.into_iter().enumerate() {
        let y = area.top() + 70.0 + i as f32 * lado * 1.15;
        pintor.text(egui::pos2(area.left() + 12.0, y), egui::Align2::LEFT_CENTER, modelo.nome(), egui::FontId::proportional(14.0), TEXTO);
        for (j, estado) in Estado::TODOS.into_iter().enumerate() {
            let centro = egui::pos2(area.left() + margem + 60.0 + j as f32 * lado * 1.6, y);
            mascote::desenhar(pintor, centro, lado * 0.8, modelo, estado, INSTANTE);
        }
    }
}

/// Os cinco modelos trabalhando, grandes, e embaixo a 40 px, o tamanho da barra lateral.
fn lado_a_lado(ui: &mut egui::Ui) {
    let area = ui.max_rect();
    let pintor = ui.painter();
    pintor.rect_filled(area, 0.0, DIALOGO);
    let coluna = area.width() / 5.0;
    for (i, modelo) in Modelo::TODOS.into_iter().enumerate() {
        let x = area.left() + coluna * (i as f32 + 0.5);
        mascote::desenhar(pintor, egui::pos2(x, area.top() + 170.0), 220.0, modelo, Estado::Trabalhando, INSTANTE);
        pintor.text(egui::pos2(x, area.top() + 330.0), egui::Align2::CENTER_CENTER, modelo.nome(), egui::FontId::proportional(16.0), TEXTO);
        mascote::desenhar(pintor, egui::pos2(x, area.top() + 410.0), 40.0, modelo, Estado::Trabalhando, INSTANTE);
    }
}

/// O modelo escolhido nos cinco estados, grande.
fn estados(ui: &mut egui::Ui, modelo: Modelo) {
    let area = ui.max_rect();
    let pintor = ui.painter();
    pintor.rect_filled(area, 0.0, DIALOGO);
    let coluna = area.width() / 5.0;
    for (i, estado) in Estado::TODOS.into_iter().enumerate() {
        let x = area.left() + coluna * (i as f32 + 0.5);
        mascote::desenhar(pintor, egui::pos2(x, area.top() + 170.0), 200.0, modelo, estado, INSTANTE);
        pintor.text(egui::pos2(x, area.top() + 330.0), egui::Align2::CENTER_CENTER, estado.nome(), egui::FontId::proportional(16.0), TEXTO);
        mascote::desenhar(pintor, egui::pos2(x, area.top() + 410.0), 40.0, modelo, estado, INSTANTE);
    }
}

fn main() -> eframe::Result {
    let fixo = std::env::var("MASCOTE_ESTADO").ok().and_then(|v| v.parse::<usize>().ok()).and_then(|i| Estado::TODOS.get(i).copied());
    let (estado, ciclo) = fixo.map_or((Estado::Dormindo, true), |e| (e, false));
    let galeria = match std::env::var("MASCOTE_GALERIA").unwrap_or_default().as_str() {
        "1" => Galeria::Todos,
        "modelos" => Galeria::Modelos,
        "estados" => Galeria::Estados,
        _ => Galeria::Nenhuma,
    };
    let modelo = Modelo::do_codigo(&std::env::var("MASCOTE_MODELO").unwrap_or_default());
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Colmeia · mascote").with_inner_size(match galeria {
            Galeria::Nenhuma => [720.0, 520.0],
            Galeria::Todos => [1000.0, 700.0],
            Galeria::Modelos | Galeria::Estados => [1400.0, 470.0],
        }),
        ..Default::default()
    };
    eframe::run_native(
        "mascote",
        opcoes,
        Box::new(move |cc| {
            cc.egui_ctx.global_style_mut(|s| s.interaction.selectable_labels = false);
            Ok(Box::new(Vitrine { modelo, estado, desde: 0.0, ciclo, galeria }))
        }),
    )
}
