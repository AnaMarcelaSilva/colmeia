//! O Zangão: a mesma abelha-robô, mais robusta. Corpo largo e arredondado em
//! âmbar queimado com listras quase pretas, asas grandes, visor largo com
//! olhos maiores e mais juntos, e um tufo curto no topo, com a luz do estado
//! na ponta, no lugar das antenas finas. Lê diferente da Colmeia mesmo a 40 px.

use std::f32::consts::TAU;

use eframe::egui::{Color32, Pos2, pos2};

use crate::mascote::{Estado, OLHO, Rosto, Tempo, balanco, batida, expressao, mao};
use crate::pincel::{Pincel, faixa};

const AMBAR: Color32 = Color32::from_rgb(0xc9, 0x86, 0x2a);
const SOMBRA: Color32 = Color32::from_rgba_premultiplied(0x3a, 0x22, 0x08, 140);
const LISTRA: Color32 = Color32::from_rgb(0x1a, 0x15, 0x1c);
const TELA: Color32 = Color32::from_rgb(0x0e, 0x0c, 0x14);
const MOLDURA: Color32 = Color32::from_rgb(0xf0, 0xb7, 0x5a);

/// O corpo: um oval largo e baixo, mais largo que o da Colmeia.
fn corpo() -> Vec<Pos2> {
    (0..32)
        .map(|i| {
            let a = i as f32 / 32.0 * TAU;
            pos2(a.cos() * 48.0, 6.0 + a.sin() * 37.0)
        })
        .collect()
}

/// Os olhos ficam maiores que nos outros modelos.
const OLHOS_MAIORES: f32 = 1.35;

pub(crate) fn desenhar(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;
    let dorme = estado == Estado::Dormindo;

    // Asas grandes e esfumadas, abertas para cima; achatam quando batem.
    let achatar = 1.0 - 0.45 * batida(estado, tt);
    let (x, y, giro) = if dorme { (33.0, -24.0, 0.3) } else { (38.0, -30.0, 0.55) };
    for lado in [-1.0_f32, 1.0] {
        let asa = Color32::from_rgba_unmultiplied(0xe8, 0xd6, 0xb0, 80);
        p.elipse_com(x * lado, y, (21.0, 13.0 * achatar), -giro * lado, asa, Some((1.6, Color32::from_rgba_unmultiplied(0xf0, 0xc8, 0x8c, 170))));
        let (s, c) = (-giro * lado).sin_cos();
        let veia = |d: f32| (x * lado + d * c, y + d * s);
        let ((ax, ay), (bx, by)) = (veia(-8.0), veia(8.0));
        p.linha(ax, ay, bx, by, 1.4, Color32::from_rgba_unmultiplied(255, 240, 210, 90));
    }

    // O tufo: três penas curtas e grossas; a do meio leva a luz do estado.
    let balanco = balanco(estado, tt);
    let caido = if dorme { 4.0 } else { 0.0 };
    for lado in [-1.0_f32, 1.0] {
        p.curva((6.0 * lado, -26.0), (9.0 * lado, -34.0 + caido), (13.0 * lado + balanco * 0.6, -36.0 + caido * 1.2), 4.5, LISTRA);
    }
    let ponta = (balanco + caido, -42.0 + caido);
    p.curva((0.0, -26.0), (0.0, -36.0), ponta, 5.0, LISTRA);
    let luz = estado.cor_luz();
    if !dorme {
        p.circulo(ponta.0, ponta.1 - 3.0, 7.5, luz.gamma_multiply(0.22));
    }
    p.circulo(ponta.0, ponta.1 - 3.0, 4.3, luz);

    // Braços curtos e grossos.
    for lado in [-1.0_f32, 1.0] {
        let (ox, oy) = (45.0 * lado, 14.0);
        let (mx, my) = mao(lado, 12.0, estado, tt);
        p.linha(ox, oy, ox + mx, oy + my, 6.0, LISTRA);
        p.circulo(ox + mx, oy + my, 4.0, LISTRA);
    }

    // Corpo oval e largo; as listras e a sombra de baixo cortadas no contorno.
    let corpo = corpo();
    p.poligono(&corpo, AMBAR, None);
    for (y0, y1, cor) in [(13.0, 21.5, LISTRA), (27.0, 35.0, LISTRA), (37.0, 50.0, SOMBRA)] {
        let parte = faixa(&corpo, y0, y1);
        if parte.len() >= 3 {
            p.poligono(&parte, cor, None);
        }
    }
    p.elipse(-27.0, -17.0, 7.5, 4.0, -0.6, Color32::from_rgba_unmultiplied(255, 255, 255, 70));

    // Visor largo com aro escuro e moldura de mel; os olhos crescem com ele.
    let (x, y) = (0.0, -3.0);
    p.caixa(x, y, 62.0, 38.0, 14.0, LISTRA);
    p.caixa(x, y, 55.0, 31.0, 11.0, TELA);
    p.moldura((x, y), (55.0, 31.0), 11.0, 1.5, MOLDURA);
    let rosto = p.movido(x, y + 1.0).ampliado(OLHOS_MAIORES);
    expressao(rosto, &Rosto { x: 0.0, y: -1.0, olhos: 7.0, tela: TELA, olho: OLHO }, estado, t);
}
