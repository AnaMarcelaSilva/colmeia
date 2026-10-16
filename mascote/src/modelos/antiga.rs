//! A abelha de antes da identidade: corpo redondo e amarelo, asas cinza
//! translúcidas e a tela do rosto sem moldura.

use eframe::egui::Color32;

use crate::mascote::{Estado, Rosto, Tempo, balanco, expressao};
use crate::pincel::{Pincel, aleatorio};

const AMARELO: Color32 = Color32::from_rgb(0xf6, 0xc4, 0x43);
const LISTRA: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x35);
const TELA: Color32 = Color32::from_rgb(0x11, 0x14, 0x1b);
const OLHO: Color32 = Color32::from_rgb(0x6f, 0xe3, 0xf0);

/// A luz das antenas, nas cores daquela época.
fn luz(estado: Estado) -> Color32 {
    match estado {
        Estado::Dormindo => Color32::from_rgb(0x7a, 0x81, 0x90),
        Estado::Trabalhando => Color32::from_rgb(0x7f, 0xd1, 0x8b),
        Estado::Aguardando => Color32::from_rgb(0xf2, 0xb3, 0x5b),
        Estado::Bugado => Color32::from_rgb(0xf2, 0x5b, 0x6b),
        Estado::Comemorando => Color32::from_rgb(0xc7, 0x92, 0xea),
    }
}

/// A mão do braço, com os ângulos de antes.
fn mao(lado: f32, estado: Estado, tt: f32) -> (f32, f32) {
    let comprimento = 16.0;
    let angulo: f32 = match estado {
        Estado::Dormindo => 1.35,
        Estado::Trabalhando => 0.9 + (tt * 12.0 + lado).sin() * 0.25,
        Estado::Aguardando if lado > 0.0 => -1.1 + (tt * 10.0).sin() * 0.45,
        Estado::Aguardando => 1.2,
        Estado::Bugado => 1.0 + (aleatorio((tt * 14.0) as u32, lado as u32 + 5) - 0.5) * 1.2,
        Estado::Comemorando => -1.0 + (tt * 9.0 + lado).sin() * 0.4,
    };
    (angulo.cos() * comprimento * lado.signum(), angulo.sin() * comprimento)
}

pub(crate) fn desenhar(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;

    // Asas atrás do corpo, presas nas costas e abertas para os lados; batem
    // girando (as de antes não achatavam).
    let batida = match estado {
        Estado::Dormindo => 0.0,
        Estado::Trabalhando | Estado::Comemorando => (tt * 45.0).sin() * 0.55,
        Estado::Aguardando => (tt * 18.0).sin() * 0.35,
        Estado::Bugado => (aleatorio((tt * 14.0) as u32, 7) - 0.5) * 0.9,
    };
    let asa = Color32::from_rgba_unmultiplied(200, 228, 255, 85);
    let brilho = Color32::from_rgba_unmultiplied(235, 245, 255, 70);
    let (inclinacao, altura) = if estado == Estado::Dormindo { (0.35, -22.0) } else { (0.6, -26.0) };
    for lado in [-1.0_f32, 1.0] {
        let angulo = (inclinacao + batida) * lado;
        p.elipse(36.0 * lado, altura, 17.0, 10.0, angulo, asa);
        p.elipse(33.0 * lado, altura - 2.0, 9.0, 4.5, angulo, brilho);
    }

    let balanco = balanco(estado, tt);
    for lado in [-1.0_f32, 1.0] {
        p.linha(9.0 * lado, -26.0, 17.0 * lado + balanco, -44.0, 2.4, LISTRA);
        p.circulo(18.0 * lado + balanco, -46.0, 4.2, luz(estado));
    }

    for lado in [-1.0_f32, 1.0] {
        let (ox, oy) = (38.0 * lado, 12.0);
        let (mx, my) = mao(lado, estado, tt);
        p.linha(ox, oy, ox + mx * 0.7, oy + my * 0.7, 5.0, LISTRA);
    }

    // Corpo redondo com listras que acompanham a curva.
    let (cy, raio, meio) = (4.0, 34.0, 8.0);
    p.caixa(0.0, cy, 84.0, 68.0, raio, AMARELO);
    for y in [17.0_f32, 27.0] {
        let dy = y - cy;
        let meia = meio + (raio * raio - dy * dy).max(0.0).sqrt() - 2.0;
        p.caixa(0.0, y, meia * 2.0, 5.0, 2.5, LISTRA);
    }
    p.circulo(-24.0, -14.0, 4.5, Color32::from_rgba_unmultiplied(255, 255, 255, 150));

    let (x, y) = (0.0, -4.0);
    p.caixa(x, y, 52.0, 32.0, 11.0, TELA);
    expressao(p, &Rosto { x, y, olhos: 11.0, tela: TELA, olho: OLHO }, estado, t);
}
