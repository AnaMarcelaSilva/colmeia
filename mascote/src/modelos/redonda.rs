//! A Redonda: o corpo redondo e o jeito da Clássica com o acabamento da
//! marca (tela com borda roxa, asas lilás translúcidas, mel nas antenas e
//! um brilho suave no corpo).

use eframe::egui::Color32;

use crate::mascote::{Estado, OLHO, Rosto, Tempo, balanco, batida, expressao, mao};
use crate::pincel::Pincel;

const MEL_CLARO: Color32 = Color32::from_rgb(0xfb, 0xcf, 0x5c);
const MEL_ESCURO: Color32 = Color32::from_rgb(0xf0, 0xae, 0x2e);
const MEL_ANTENA: Color32 = Color32::from_rgb(0xf6, 0xb6, 0x5a);
const LISTRA: Color32 = Color32::from_rgb(0x2a, 0x24, 0x38);
const TELA: Color32 = Color32::from_rgb(0x1b, 0x17, 0x30);
const BORDA: Color32 = Color32::from_rgb(0xa3, 0x94, 0xcc);

pub(crate) fn desenhar(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;
    let dorme = estado == Estado::Dormindo;

    // Asas lilás, abertas para cima; achatam quando batem.
    let achatar = 1.0 - 0.45 * batida(estado, tt);
    let (x, y, giro) = if dorme { (24.0, -17.0, 0.35) } else { (26.0, -21.0, 0.49) };
    for lado in [-1.0_f32, 1.0] {
        let asa = Color32::from_rgba_unmultiplied(0xc9, 0xbc, 0xf6, 130);
        p.elipse_com(x * lado, y, (17.0, 11.0 * achatar), -giro * lado, asa, Some((1.0, Color32::from_rgba_unmultiplied(0xc4, 0xb7, 0xf2, 180))));
    }

    // Antenas de mel com a luz do estado no meio.
    let balanco = balanco(estado, tt);
    let luz = estado.cor_luz();
    for lado in [-1.0_f32, 1.0] {
        p.linha(11.0 * lado, -26.0, 16.0 * lado + balanco, -40.0, 2.5, LISTRA);
        if !dorme {
            p.circulo(16.5 * lado + balanco, -41.5, 7.5, luz.gamma_multiply(0.22));
        }
        p.circulo(16.5 * lado + balanco, -41.5, 4.5, MEL_ANTENA);
        p.circulo(16.5 * lado + balanco, -41.5, 2.4, luz);
    }

    for lado in [-1.0_f32, 1.0] {
        let (ox, oy) = (34.0 * lado, 12.0);
        let (mx, my) = mao(lado, 11.0, estado, tt);
        p.linha(ox, oy, ox + mx, oy + my, 4.6, LISTRA);
        p.circulo(ox + mx, oy + my, 3.0, LISTRA);
    }

    // Corpo redondo em duas camadas de mel (o gradiente do desenho) e o brilho.
    p.elipse(0.0, 2.0, 36.0, 32.0, 0.0, MEL_ESCURO);
    p.elipse(-5.0, -3.0, 29.0, 25.0, 0.0, MEL_CLARO);
    p.elipse(-13.0, -16.0, 7.0, 4.5, -0.44, Color32::from_rgba_unmultiplied(255, 255, 255, 90));
    p.caixa(0.0, 20.5, 50.0, 4.5, 2.25, LISTRA);
    p.caixa(0.0, 28.0, 36.0, 4.0, 2.0, LISTRA);

    let (x, y) = (0.0, -1.5);
    p.caixa(x, y, 50.0, 27.0, 12.0, TELA);
    p.moldura((x, y), (50.0, 27.0), 12.0, 1.75, BORDA);
    expressao(p, &Rosto { x, y, olhos: 10.0, tela: TELA, olho: OLHO }, estado, t);
}
