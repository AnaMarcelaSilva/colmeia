//! O robô do favo: corpo redondo, a tela do rosto num hexágono como a logo,
//! asas em célula de favo só com o contorno e listras curvas. É o que lê
//! melhor pequeno.

use eframe::egui::{Color32, Pos2, pos2};

use crate::mascote::{Estado, Rosto, Tempo, balanco, batida, expressao, mao};
use crate::pincel::{Pincel, arredondar, hexagono};

const MEL: Color32 = Color32::from_rgb(0xf6, 0xc4, 0x43);
const LISTRA: Color32 = Color32::from_rgb(0x2a, 0x24, 0x38);
const TELA: Color32 = Color32::from_rgb(0x1b, 0x17, 0x30);
const BORDA: Color32 = Color32::from_rgb(0xa3, 0x94, 0xcc);
const OLHO: Color32 = Color32::from_rgb(0x7f, 0xe0, 0xea);

pub(crate) fn desenhar(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;
    let dorme = estado == Estado::Dormindo;

    // Asas: células com ponta em cima, quase só o contorno.
    let achatar = 1.0 - 0.45 * batida(estado, tt);
    let (x, y) = if dorme { (24.0, -15.0) } else { (27.0, -19.0) };
    for lado in [-1.0_f32, 1.0] {
        let celula: Vec<Pos2> = hexagono(Pos2::ZERO, 13.0, false).into_iter().map(|q| pos2(x * lado + q.x, y + q.y * achatar)).collect();
        p.poligono(&celula, Color32::from_rgba_unmultiplied(0xa3, 0x94, 0xcc, 46), Some((1.5, Color32::from_rgb(0xb9, 0xa9, 0xec))));
    }

    // Antenas curvas com a luz do estado.
    let balanco = balanco(estado, tt);
    let luz = estado.cor_luz();
    for lado in [-1.0_f32, 1.0] {
        p.curva((8.0 * lado, -27.0), (11.0 * lado, -37.0), (15.0 * lado + balanco, -40.0), 2.5, LISTRA);
        if !dorme {
            p.circulo(15.5 * lado + balanco, -41.0, 7.0, luz.gamma_multiply(0.22));
        }
        p.circulo(15.5 * lado + balanco, -41.0, 4.0, luz);
    }

    for lado in [-1.0_f32, 1.0] {
        let (ox, oy) = (32.0 * lado, 12.0);
        let (mx, my) = mao(lado, 10.0, estado, tt);
        p.linha(ox, oy, ox + mx, oy + my, 4.2, LISTRA);
    }

    p.circulo(0.0, 2.0, 33.0, MEL);
    p.curva((-28.0, 17.0), (0.0, 27.0), (28.0, 17.0), 4.5, LISTRA);
    p.curva((-20.0, 26.0), (0.0, 33.0), (20.0, 26.0), 4.0, LISTRA);

    let (x, y) = (0.0, -2.0);
    let visor = arredondar(&hexagono(pos2(x, y), 19.0, false), 1.5);
    p.poligono(&visor, TELA, Some((1.75, BORDA)));
    expressao(p, &Rosto { x, y, olhos: 8.0, tela: TELA, olho: OLHO }, estado, t);
}
