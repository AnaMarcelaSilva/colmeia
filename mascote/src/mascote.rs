//! A abelha-robô da Colmeia: desenhada só com formas vetoriais do egui, sem
//! imagens, com cinco estados, cada um ligado a uma situação real do trabalho
//! dos agentes.

use std::f32::consts::TAU;

use eframe::egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Estado {
    Dormindo,
    Trabalhando,
    Aguardando,
    Bugado,
    Comemorando,
}

impl Estado {
    pub const TODOS: [Estado; 5] = [
        Estado::Dormindo,
        Estado::Trabalhando,
        Estado::Aguardando,
        Estado::Bugado,
        Estado::Comemorando,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Estado::Dormindo => "Dormindo",
            Estado::Trabalhando => "Trabalhando",
            Estado::Aguardando => "Aguardando você",
            Estado::Bugado => "Bugado",
            Estado::Comemorando => "Comemorando",
        }
    }

    /// A situação do app que leva a este estado.
    pub fn quando(self) -> &'static str {
        match self {
            Estado::Dormindo => "nenhum agente trabalhando",
            Estado::Trabalhando => "agentes em execução",
            Estado::Aguardando => "uma tarefa precisa da sua aprovação",
            Estado::Bugado => "um agente ou integração deu erro",
            Estado::Comemorando => "uma tarefa foi concluída",
        }
    }

    fn cor_luz(self) -> Color32 {
        match self {
            Estado::Dormindo => Color32::from_rgb(0x7a, 0x81, 0x90),
            Estado::Trabalhando => Color32::from_rgb(0x7f, 0xd1, 0x8b),
            Estado::Aguardando => Color32::from_rgb(0xf2, 0xb3, 0x5b),
            Estado::Bugado => Color32::from_rgb(0xf2, 0x5b, 0x6b),
            Estado::Comemorando => Color32::from_rgb(0xc7, 0x92, 0xea),
        }
    }
}

/// Tempo total e tempo desde que o estado atual começou, em segundos.
#[derive(Clone, Copy)]
pub struct Tempo {
    pub total: f32,
    pub no_estado: f32,
}

const TELA: Color32 = Color32::from_rgb(0x11, 0x14, 0x1b);
const OLHO: Color32 = Color32::from_rgb(0x6f, 0xe3, 0xf0);

/// Desenha em coordenadas de um quadro de 100 x 100 centrado em `c`.
/// `tinta` troca todas as cores por uma só, para o efeito de cores deslocadas.
#[derive(Clone, Copy)]
struct Pincel<'a> {
    p: &'a Painter,
    c: Pos2,
    u: f32,
    tinta: Option<Color32>,
}

impl<'a> Pincel<'a> {
    fn movido(self, dx: f32, dy: f32) -> Self {
        Self { c: self.c + vec2(dx * self.u, dy * self.u), ..self }
    }

    fn cor(&self, cor: Color32) -> Color32 {
        match self.tinta {
            Some(t) => Color32::from_rgba_unmultiplied(t.r(), t.g(), t.b(), (cor.a() as u16 * t.a() as u16 / 255) as u8),
            None => cor,
        }
    }

    fn pt(&self, x: f32, y: f32) -> Pos2 {
        self.c + vec2(x * self.u, y * self.u)
    }

    fn circulo(&self, x: f32, y: f32, r: f32, cor: Color32) {
        self.p.circle_filled(self.pt(x, y), r * self.u, self.cor(cor));
    }

    fn caixa(&self, x: f32, y: f32, l: f32, a: f32, raio: f32, cor: Color32) {
        let rect = Rect::from_center_size(self.pt(x, y), vec2(l * self.u, a * self.u));
        let raio = CornerRadius::same((raio * self.u).min(255.0) as u8);
        self.p.rect_filled(rect, raio, self.cor(cor));
    }

    /// Traço com pontas arredondadas.
    fn linha(&self, x1: f32, y1: f32, x2: f32, y2: f32, espessura: f32, cor: Color32) {
        let cor = self.cor(cor);
        let (a, b) = (self.pt(x1, y1), self.pt(x2, y2));
        self.p.line_segment([a, b], Stroke::new(espessura * self.u, cor));
        self.p.circle_filled(a, espessura * self.u / 2.0, cor);
        self.p.circle_filled(b, espessura * self.u / 2.0, cor);
    }

    fn elipse(&self, x: f32, y: f32, rx: f32, ry: f32, angulo: f32, cor: Color32) {
        let (s, c) = angulo.sin_cos();
        let pontos = (0..24)
            .map(|i| {
                let a = i as f32 / 24.0 * TAU;
                let (px, py) = (a.cos() * rx, a.sin() * ry);
                self.pt(x + px * c - py * s, y + px * s + py * c)
            })
            .collect();
        self.p.add(Shape::convex_polygon(pontos, self.cor(cor), Stroke::NONE));
    }

    fn texto(&self, x: f32, y: f32, tamanho: f32, texto: &str, cor: Color32) {
        self.p.text(self.pt(x, y), Align2::CENTER_CENTER, texto, FontId::proportional(tamanho * self.u), self.cor(cor));
    }
}

/// Número pseudoaleatório estável entre 0 e 1: o mesmo quadro desenha sempre igual.
fn aleatorio(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
}

pub fn desenhar(painter: &Painter, centro: Pos2, tamanho: f32, estado: Estado, t: Tempo) {
    let base = Pincel { p: painter, c: centro, u: tamanho / 100.0, tinta: None };
    if estado != Estado::Bugado {
        corpo(base, estado, t);
        extras(base, estado, t);
        return;
    }

    // Bugado: a imagem "quebra" em faixas, com cores deslocadas e chiado.
    // Rajadas fortes e curtas, com tremidas leves entre elas.
    let semente = (t.total * 14.0) as u32;
    let forte = t.total % 0.9 < 0.3;
    let intensidade = if forte { 1.0 } else { 0.25 };
    let deslocamento = (2.0 + 3.0 * aleatorio(semente, 0)) * intensidade;
    corpo(Pincel { tinta: Some(Color32::from_rgba_unmultiplied(255, 60, 90, 120)), ..base.movido(-deslocamento, 0.0) }, estado, t);
    corpo(Pincel { tinta: Some(Color32::from_rgba_unmultiplied(60, 230, 255, 120)), ..base.movido(deslocamento, 0.0) }, estado, t);

    let area = Rect::from_center_size(centro, vec2(tamanho * 1.6, tamanho * 1.4));
    let faixas = 9;
    let altura = area.height() / faixas as f32;
    for i in 0..faixas {
        let faixa = Rect::from_min_size(area.min + vec2(0.0, i as f32 * altura), vec2(area.width(), altura));
        let desvio = if aleatorio(semente, i + 10) > 0.55 { (aleatorio(semente, i + 30) - 0.5) * 18.0 * intensidade } else { 0.0 };
        let recorte = painter.with_clip_rect(faixa.intersect(painter.clip_rect()));
        corpo(Pincel { p: &recorte, ..base.movido(desvio, 0.0) }, estado, t);
    }

    // Linhas de chiado atravessando o mascote.
    for i in 0..(if forte { 5 } else { 1 }) {
        let y = (aleatorio(semente, i + 50) - 0.5) * 90.0;
        let largura = 30.0 + aleatorio(semente, i + 60) * 70.0;
        let x = (aleatorio(semente, i + 70) - 0.5) * 40.0;
        let cor = if i % 2 == 0 { Color32::from_rgba_unmultiplied(111, 227, 240, 90) } else { Color32::from_rgba_unmultiplied(242, 91, 107, 90) };
        base.caixa(x, y, largura, 1.6, 0.0, cor);
    }
}

fn corpo(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;
    let pulo = match estado {
        Estado::Dormindo => (tt * 1.2).sin() * 1.0,
        Estado::Trabalhando => (tt * 6.0).sin() * 2.0,
        Estado::Aguardando => -(tt * 5.0).sin().abs() * 5.0,
        Estado::Bugado => (aleatorio((tt * 14.0) as u32, 99) - 0.5) * 3.0,
        Estado::Comemorando => -(tt * 7.0).sin().abs() * 11.0,
    };

    // A sombra fica no chão e encolhe quando o mascote sobe.
    let escala = 1.0 + pulo / 40.0;
    p.elipse(0.0, 54.0, 26.0 * escala, 4.5 * escala, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 90));

    let p = p.movido(0.0, pulo);
    abelha(p, estado, t);
}

/// Posição da mão para um braço preso em `ombro`, em cada estado.
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

fn abelha(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;
    let amarelo = Color32::from_rgb(0xf6, 0xc4, 0x43);
    let listra = Color32::from_rgb(0x2b, 0x2b, 0x35);

    // Asas atrás do corpo; batem rápido quando trabalha.
    let batida = match estado {
        Estado::Dormindo => 0.0,
        Estado::Trabalhando | Estado::Comemorando => (tt * 45.0).sin() * 0.55,
        Estado::Aguardando => (tt * 18.0).sin() * 0.35,
        Estado::Bugado => (aleatorio((tt * 14.0) as u32, 7) - 0.5) * 0.9,
    };
    // Asas translúcidas, presas nas costas e abertas para os lados.
    let asa = Color32::from_rgba_unmultiplied(200, 228, 255, 85);
    let brilho = Color32::from_rgba_unmultiplied(235, 245, 255, 70);
    let (inclinacao, altura) = if estado == Estado::Dormindo { (0.35, -22.0) } else { (0.6, -26.0) };
    for lado in [-1.0_f32, 1.0] {
        let angulo = (inclinacao + batida) * lado;
        p.elipse(36.0 * lado, altura, 17.0, 10.0, angulo, asa);
        p.elipse(33.0 * lado, altura - 2.0, 9.0, 4.5, angulo, brilho);
    }

    // Antenas; mexem quando está esperando você.
    let balanco = if estado == Estado::Aguardando { (tt * 10.0).sin() * 3.0 } else { 0.0 };
    p.linha(-9.0, -26.0, -17.0 + balanco, -44.0, 2.4, listra);
    p.linha(9.0, -26.0, 17.0 + balanco, -44.0, 2.4, listra);
    let luz = estado.cor_luz();
    p.circulo(-18.0 + balanco, -46.0, 4.2, luz);
    p.circulo(18.0 + balanco, -46.0, 4.2, luz);

    // Bracinhos.
    for lado in [-1.0_f32, 1.0] {
        let (ox, oy) = (38.0 * lado, 12.0);
        let (mx, my) = mao(lado, estado, tt);
        p.linha(ox, oy, ox + mx * 0.7, oy + my * 0.7, 5.0, listra);
    }

    // Corpo redondo com listras que acompanham a curva.
    let (cy, raio, meio) = (4.0, 34.0, 8.0);
    p.caixa(0.0, cy, 84.0, 68.0, raio, amarelo);
    for y in [17.0_f32, 27.0] {
        let dy = y - cy;
        let meia = meio + (raio * raio - dy * dy).max(0.0).sqrt() - 2.0;
        p.caixa(0.0, y, meia * 2.0, 5.0, 2.5, listra);
    }
    p.circulo(-24.0, -14.0, 4.5, Color32::from_rgba_unmultiplied(255, 255, 255, 150));

    rosto(p, 0.0, -4.0, estado, t);
}

/// A tela no rosto: é ela que mostra a expressão.
fn rosto(p: Pincel, x: f32, y: f32, estado: Estado, t: Tempo) {
    let tt = t.total;
    p.caixa(x, y, 52.0, 32.0, 11.0, TELA);

    let olho = if estado == Estado::Dormindo { Color32::from_rgba_unmultiplied(111, 227, 240, 120) } else { OLHO };
    for lado in [-1.0_f32, 1.0] {
        let ox = x + 11.0 * lado;
        match estado {
            Estado::Dormindo => p.linha(ox - 5.0, y, ox + 5.0, y, 2.2, olho),
            Estado::Trabalhando => {
                // Olhos "lendo" a tela de um lado para o outro.
                let olhar = (tt * 2.2).sin() * 4.0;
                p.caixa(ox + olhar, y - 1.0, 8.0, 8.0, 2.0, olho);
            }
            Estado::Aguardando => {
                if tt % 3.0 < 0.12 {
                    p.linha(ox - 5.0, y, ox + 5.0, y, 2.2, olho);
                } else {
                    p.circulo(ox, y - 1.0, 5.0, olho);
                    p.circulo(ox + 1.5, y - 2.5, 1.6, TELA);
                }
            }
            Estado::Bugado => {
                p.linha(ox - 4.0, y - 4.0, ox + 4.0, y + 4.0, 2.2, olho);
                p.linha(ox - 4.0, y + 4.0, ox + 4.0, y - 4.0, 2.2, olho);
            }
            Estado::Comemorando => {
                p.linha(ox - 5.0, y + 2.0, ox, y - 3.0, 2.2, olho);
                p.linha(ox, y - 3.0, ox + 5.0, y + 2.0, 2.2, olho);
            }
        }
    }

    match estado {
        // Três pontinhos acendendo em sequência: "processando".
        Estado::Trabalhando => {
            for i in 0..3 {
                let fase = ((tt * 3.0 - i as f32 * 0.35).sin() * 0.5 + 0.5) * 200.0 + 55.0;
                p.circulo(x - 6.0 + i as f32 * 6.0, y + 10.0, 1.6, Color32::from_rgba_unmultiplied(111, 227, 240, fase as u8));
            }
        }
        Estado::Comemorando => {
            p.linha(x - 5.0, y + 8.0, x, y + 10.0, 1.8, OLHO);
            p.linha(x, y + 10.0, x + 5.0, y + 8.0, 1.8, OLHO);
        }
        Estado::Aguardando => p.circulo(x, y + 9.0, 2.2, OLHO),
        _ => {}
    }
}

/// O que aparece em volta do mascote: "zzz", o balão de aviso e os confetes.
fn extras(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;
    match estado {
        Estado::Dormindo => {
            for i in 0..3 {
                let fase = (tt * 0.45 + i as f32 / 3.0).fract();
                let alfa = ((1.0 - fase) * 220.0) as u8;
                p.texto(28.0 + fase * 16.0, -38.0 - fase * 26.0, 9.0 + fase * 7.0, "z", Color32::from_rgba_unmultiplied(200, 205, 215, alfa));
            }
        }
        Estado::Aguardando => {
            let salto = -(tt * 5.0).sin().abs() * 3.0;
            p.circulo(42.0, -46.0 + salto, 11.0, Color32::from_rgb(0xf2, 0xb3, 0x5b));
            p.texto(42.0, -46.5 + salto, 15.0, "!", Color32::from_rgb(0x1a, 0x1a, 0x1f));
        }
        Estado::Comemorando => {
            let cores = [
                Color32::from_rgb(0xc7, 0x92, 0xea),
                Color32::from_rgb(0x7f, 0xd1, 0x8b),
                Color32::from_rgb(0xf2, 0xb3, 0x5b),
                Color32::from_rgb(0x6f, 0xe3, 0xf0),
            ];
            for i in 0..26u32 {
                let x0 = (aleatorio(i, 1) - 0.5) * 150.0;
                let velocidade = 0.6 + aleatorio(i, 2) * 0.8;
                let y = -80.0 + (t.no_estado * 55.0 * velocidade + aleatorio(i, 3) * 160.0) % 160.0;
                let x = x0 + (tt * 3.0 + i as f32).sin() * 6.0;
                p.elipse(x, y, 2.6, 1.4, tt * 4.0 + i as f32, cores[i as usize % cores.len()]);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn aleatorio_e_estavel_e_fica_entre_zero_e_um() {
        for a in 0..200 {
            for b in 0..20 {
                let x = aleatorio(a, b);
                assert!((0.0..=1.0).contains(&x));
                assert_eq!(x, aleatorio(a, b), "o mesmo quadro tem que desenhar sempre igual");
            }
        }
    }

    #[test]
    fn todo_estado_tem_nome_e_situacao() {
        for e in Estado::TODOS {
            assert!(!e.nome().is_empty() && !e.quando().is_empty());
        }
    }
}
