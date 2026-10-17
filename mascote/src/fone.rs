//! O fone de ouvido com microfone (headset) que a abelha põe quando o
//! microfone está aberto: o arco por cima da cabeça, as conchas dos lados da
//! tela do rosto e a haste com o microfone aceso em vermelho. Encaixa em cada
//! modelo pela largura do corpo na altura do rosto; legível a 40 px.

use eframe::egui::Color32;

use crate::mascote::Modelo;
use crate::pincel::Pincel;

/// Como o microfone está.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fone {
    /// Ouvindo um comando (ou o ditado): o microfone grande e brilhando.
    Ouvindo,
    /// Esperando o nome: o fone mais discreto, o microfone pequeno e aceso.
    Esperando,
}

const ARCO: Color32 = Color32::from_rgb(0x26, 0x22, 0x33);
const REALCE: Color32 = Color32::from_rgb(0x8a, 0x80, 0xb8);
const ALMOFADA: Color32 = Color32::from_rgb(0x4a, 0x44, 0x66);
const ACESO: Color32 = Color32::from_rgb(0xf4, 0x47, 0x5b);

/// Onde o fone encaixa em cada modelo (no quadro de 100): a meia largura do
/// corpo na altura do rosto, a altura do rosto e o topo do corpo.
fn encaixe(modelo: Modelo) -> (f32, f32, f32) {
    match modelo {
        Modelo::Colmeia => (35.0, -4.5, -24.0),
        Modelo::Classica => (41.0, -4.0, -30.0),
        Modelo::Redonda => (35.0, -1.5, -30.0),
        Modelo::Favo => (32.0, -2.0, -31.0),
        Modelo::Zangao => (46.0, -3.0, -31.0),
    }
}

pub(crate) fn desenhar(p: Pincel, modelo: Modelo, fone: Fone) {
    let (lado, rosto, topo) = encaixe(modelo);
    let esperando = fone == Fone::Esperando;
    // O arco: sai das conchas e passa rente ao topo do corpo.
    let (y0, pico) = (rosto - 7.0, topo - 4.0);
    let controle = 2.0 * pico - y0;
    let grossura = if esperando { 4.2 } else { 5.0 };
    p.curva((-lado, y0), (0.0, controle), (lado, y0), grossura, ARCO);
    p.curva((-lado + 1.5, y0 - 1.0), (0.0, controle + 1.6), (lado - 1.5, y0 - 1.0), 1.4, REALCE);
    // As conchas, por cima da borda do corpo.
    for l in [-1.0_f32, 1.0] {
        p.caixa(lado * l, rosto, 11.0, 19.0, 4.5, ARCO);
        p.caixa((lado - 1.0) * l, rosto, 6.0, 12.0, 3.0, ALMOFADA);
    }
    // A haste, da concha da direita até a frente da boca, e o microfone.
    let (bx, by) = (13.0, rosto + 20.0);
    p.curva((lado - 2.0, rosto + 7.0), (lado - 4.0, by + 2.0), (bx + 3.0, by), 3.0, ARCO);
    if esperando {
        p.circulo(bx, by, 4.6, ARCO);
        p.circulo(bx, by, 3.0, ACESO);
    } else {
        p.circulo(bx, by, 11.0, ACESO.gamma_multiply(0.3));
        p.circulo(bx, by, 6.8, ARCO);
        p.circulo(bx, by, 5.2, ACESO);
        p.circulo(bx - 1.5, by - 1.5, 1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 210));
    }
}
