//! A abelha da identidade: corpo numa célula deitada, asas de favo e a tela
//! do rosto com moldura roxa. O zangão é a mesma, maior e mais escura.

use eframe::egui::{Color32, Pos2, pos2};

use crate::mascote::{Estado, OLHO, ROXO, Rosto, Tempo, balanco, batida, expressao, mao};
use crate::pincel::{Pincel, arredondar, faixa, hexagono};

pub(crate) struct Paleta {
    corpo: Color32,
    sombra: Color32,
    listra: Color32,
    asa: Color32,
    tela: Color32,
}

pub(crate) const MEL: Paleta = Paleta {
    corpo: Color32::from_rgb(0xf4, 0xb5, 0x44),
    sombra: Color32::from_rgba_premultiplied(0x60, 0x43, 0x13, 115),
    listra: Color32::from_rgb(0x2a, 0x26, 0x38),
    asa: Color32::from_rgba_premultiplied(72, 66, 97, 97),
    tela: Color32::from_rgb(0x14, 0x12, 0x21),
};

/// O zangão: âmbar queimado, listras quase pretas e asas mais fechadas.
pub(crate) const ZANGAO: Paleta = Paleta {
    corpo: Color32::from_rgb(0xc9, 0x86, 0x2a),
    sombra: Color32::from_rgba_premultiplied(0x3a, 0x22, 0x08, 140),
    listra: Color32::from_rgb(0x1a, 0x15, 0x22),
    asa: Color32::from_rgba_premultiplied(58, 50, 82, 120),
    tela: Color32::from_rgb(0x0e, 0x0c, 0x18),
};

/// O corpo: um hexágono deitado, como as células da logo.
const CORPO: [(f32, f32); 6] = [(-40.0, 6.0), (-23.0, -24.0), (23.0, -24.0), (40.0, 6.0), (23.0, 36.0), (-23.0, 36.0)];

pub(crate) fn desenhar(p: Pincel, cores: &Paleta, estado: Estado, t: Tempo) {
    let tt = t.total;
    let dorme = estado == Estado::Dormindo;

    // Asas de favo atrás do corpo; batem (achatam) rápido quando trabalha.
    let bate = batida(estado, tt);
    let (raio, x, y, giro) = if dorme { (12.0, 30.0, -20.0, 24.0_f32) } else { (15.0, 37.0, -27.0, 32.0) };
    for lado in [-1.0_f32, 1.0] {
        let (s, c) = (giro * lado).to_radians().sin_cos();
        let achatar = 1.0 - 0.45 * bate;
        let colocar = |q: Pos2| {
            let (qx, qy) = (q.x, q.y * achatar);
            pos2(x * lado + qx * c - qy * s, y + qx * s + qy * c)
        };
        let celula: Vec<Pos2> = hexagono(Pos2::ZERO, raio, false).into_iter().map(colocar).collect();
        p.poligono(&celula, cores.asa, Some((1.6, ROXO.gamma_multiply(0.75))));
        let (a, b) = (colocar(pos2(-6.0 * lado, -5.0)), colocar(pos2(2.0 * lado, -9.0)));
        p.linha(a.x, a.y, b.x, b.y, 2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 115));
    }

    // Antenas com a luz do estado.
    let balanco = balanco(estado, tt);
    let luz = estado.cor_luz();
    for lado in [-1.0_f32, 1.0] {
        p.linha(9.0 * lado, -24.0, 17.0 * lado + balanco, -42.0, 2.6, cores.listra);
        if !dorme {
            p.circulo(17.5 * lado + balanco, -44.0, 7.5, luz.gamma_multiply(0.22));
        }
        p.circulo(17.5 * lado + balanco, -44.0, 4.4, luz);
    }

    // Bracinhos com a mão redonda.
    for lado in [-1.0_f32, 1.0] {
        let (ox, oy) = (39.0 * lado, 12.0);
        let (mx, my) = mao(lado, 13.0, estado, tt);
        p.linha(ox, oy, ox + mx, oy + my, 5.0, cores.listra);
        p.circulo(ox + mx, oy + my, 3.4, cores.listra);
    }

    // Corpo com cantos redondos; as listras e a sombra de baixo cortadas no contorno.
    let corpo = arredondar(&CORPO.map(|(x, y)| pos2(x, y)), 6.0);
    p.poligono(&corpo, cores.corpo, None);
    for (y0, y1, cor) in [(18.0, 24.5, cores.listra), (29.5, 36.0, cores.listra), (38.0, 50.0, cores.sombra)] {
        let parte = faixa(&corpo, y0, y1);
        if parte.len() >= 3 {
            p.poligono(&parte, cor, None);
        }
    }

    // A tela do rosto, com moldura roxa e um reflexo.
    let (x, y) = (0.0, -4.5);
    p.caixa(x, y, 52.0, 31.0, 10.0, cores.tela);
    p.moldura((x, y), (52.0, 31.0), 10.0, 2.0, ROXO);
    p.linha(x - 19.0, y - 12.5, x - 7.0, y - 12.5, 2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 30));
    expressao(p, &Rosto { x, y, olhos: 11.0, tela: cores.tela, olho: OLHO }, estado, t);
}
