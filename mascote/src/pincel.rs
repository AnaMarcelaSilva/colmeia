//! O pincel dos modelos: desenha num quadro de 100 x 100 centrado num ponto,
//! só com formas vetoriais do egui, e as contas de geometria que eles dividem.

use std::f32::consts::TAU;

use eframe::egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

/// Desenha em coordenadas de um quadro de 100 x 100 centrado em `c`.
/// `tinta` troca todas as cores por uma só, para o efeito de cores deslocadas.
#[derive(Clone, Copy)]
pub(crate) struct Pincel<'a> {
    pub p: &'a Painter,
    pub c: Pos2,
    pub u: f32,
    pub tinta: Option<Color32>,
}

impl<'a> Pincel<'a> {
    pub fn movido(self, dx: f32, dy: f32) -> Self {
        Self { c: self.c + vec2(dx * self.u, dy * self.u), ..self }
    }

    /// O mesmo quadro, com tudo `fator` vezes maior em volta do centro.
    pub fn ampliado(self, fator: f32) -> Self {
        Self { u: self.u * fator, ..self }
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

    pub fn circulo(&self, x: f32, y: f32, r: f32, cor: Color32) {
        self.p.circle_filled(self.pt(x, y), r * self.u, self.cor(cor));
    }

    pub fn caixa(&self, x: f32, y: f32, l: f32, a: f32, raio: f32, cor: Color32) {
        let rect = Rect::from_center_size(self.pt(x, y), vec2(l * self.u, a * self.u));
        let raio = CornerRadius::same((raio * self.u).min(255.0) as u8);
        self.p.rect_filled(rect, raio, self.cor(cor));
    }

    /// Traço com pontas arredondadas.
    pub fn linha(&self, x1: f32, y1: f32, x2: f32, y2: f32, espessura: f32, cor: Color32) {
        let cor = self.cor(cor);
        let (a, b) = (self.pt(x1, y1), self.pt(x2, y2));
        self.p.line_segment([a, b], Stroke::new(espessura * self.u, cor));
        self.p.circle_filled(a, espessura * self.u / 2.0, cor);
        self.p.circle_filled(b, espessura * self.u / 2.0, cor);
    }

    /// Curva de Bézier quadrática (de `a` a `b`, puxada por `c`), com pontas
    /// arredondadas: as antenas e as listras curvas.
    pub fn curva(&self, a: (f32, f32), c: (f32, f32), b: (f32, f32), espessura: f32, cor: Color32) {
        let cor = self.cor(cor);
        let pontos: Vec<Pos2> = (0..=12)
            .map(|i| {
                let t = i as f32 / 12.0;
                let (u, v, w) = ((1.0 - t) * (1.0 - t), 2.0 * (1.0 - t) * t, t * t);
                self.pt(u * a.0 + v * c.0 + w * b.0, u * a.1 + v * c.1 + w * b.1)
            })
            .collect();
        let raio = espessura * self.u / 2.0;
        self.p.circle_filled(pontos[0], raio, cor);
        self.p.circle_filled(pontos[12], raio, cor);
        self.p.add(Shape::line(pontos, Stroke::new(espessura * self.u, cor)));
    }

    pub fn elipse(&self, x: f32, y: f32, rx: f32, ry: f32, angulo: f32, cor: Color32) {
        self.elipse_com(x, y, (rx, ry), angulo, cor, None);
    }

    /// Elipse girada, com contorno opcional (espessura, cor).
    pub fn elipse_com(&self, x: f32, y: f32, (rx, ry): (f32, f32), angulo: f32, cor: Color32, contorno: Option<(f32, Color32)>) {
        let (s, c) = angulo.sin_cos();
        let pontos = (0..24)
            .map(|i| {
                let a = i as f32 / 24.0 * TAU;
                let (px, py) = (a.cos() * rx, a.sin() * ry);
                self.pt(x + px * c - py * s, y + px * s + py * c)
            })
            .collect();
        let traco = contorno.map_or(Stroke::NONE, |(e, k)| Stroke::new(e * self.u, self.cor(k)));
        self.p.add(Shape::convex_polygon(pontos, self.cor(cor), traco));
    }

    pub fn texto(&self, x: f32, y: f32, tamanho: f32, texto: &str, cor: Color32) {
        self.p.text(self.pt(x, y), Align2::CENTER_CENTER, texto, FontId::proportional(tamanho * self.u), self.cor(cor));
    }

    /// Polígono convexo em coordenadas do quadro, com contorno opcional.
    pub fn poligono(&self, pontos: &[Pos2], cor: Color32, contorno: Option<(f32, Color32)>) {
        let pontos = pontos.iter().map(|q| self.pt(q.x, q.y)).collect();
        let traco = contorno.map_or(Stroke::NONE, |(e, c)| Stroke::new(e * self.u, self.cor(c)));
        self.p.add(Shape::convex_polygon(pontos, self.cor(cor), traco));
    }

    /// Contorno de um retângulo arredondado (a moldura da tela).
    pub fn moldura(&self, (x, y): (f32, f32), (l, a): (f32, f32), raio: f32, espessura: f32, cor: Color32) {
        let rect = Rect::from_center_size(self.pt(x, y), vec2(l * self.u, a * self.u));
        let raio = CornerRadius::same((raio * self.u).min(255.0) as u8);
        self.p.rect_stroke(rect, raio, Stroke::new(espessura * self.u, self.cor(cor)), StrokeKind::Middle);
    }
}

/// Hexágono de raio `r` em volta de `c`; `deitado` põe uma aresta em cima
/// (o corpo), senão uma ponta em cima (as asas e a logo).
pub fn hexagono(c: Pos2, r: f32, deitado: bool) -> Vec<Pos2> {
    let giro = if deitado { 0.0 } else { -90.0 };
    (0..6)
        .map(|i| {
            let a = (60.0 * i as f32 + giro).to_radians();
            c + vec2(a.cos() * r, a.sin() * r)
        })
        .collect()
}

/// O polígono convexo com os cantos arredondados por um disco de `raio`: o
/// mesmo que um traço da mesma cor com junção redonda (o egui só faz junção
/// em ponta). Continua convexo, para desenhar com `convex_polygon`.
pub fn arredondar(pontos: &[Pos2], raio: f32) -> Vec<Pos2> {
    let n = pontos.len();
    if n < 3 || raio <= 0.0 {
        return pontos.to_vec();
    }
    let centro = pontos.iter().fold(vec2(0.0, 0.0), |s, q| s + q.to_vec2()) / n as f32;
    // A normal de fora de cada aresta (de i para i + 1).
    let normais: Vec<_> = (0..n)
        .map(|i| {
            let (a, b) = (pontos[i], pontos[(i + 1) % n]);
            let d = (b - a).normalized();
            let normal = vec2(d.y, -d.x);
            if normal.dot(a + (b - a) / 2.0 - pos2(centro.x, centro.y)) < 0.0 { -normal } else { normal }
        })
        .collect();
    let mut saida = Vec::with_capacity(n * 5);
    for i in 0..n {
        let (n1, n2) = (normais[(i + n - 1) % n], normais[i]);
        let a1 = n1.y.atan2(n1.x);
        let mut delta = n2.y.atan2(n2.x) - a1;
        if delta > std::f32::consts::PI {
            delta -= TAU;
        } else if delta < -std::f32::consts::PI {
            delta += TAU;
        }
        for passo in 0..=4 {
            let a = a1 + delta * passo as f32 / 4.0;
            saida.push(pontos[i] + vec2(a.cos(), a.sin()) * raio);
        }
    }
    saida
}

/// A parte de um polígono convexo entre duas alturas (y0 <= y <= y1): as
/// listras e a sombra cortadas no contorno do corpo.
pub(crate) fn faixa(pontos: &[Pos2], y0: f32, y1: f32) -> Vec<Pos2> {
    let cortar = |pontos: &[Pos2], dentro: &dyn Fn(Pos2) -> bool, y: f32| {
        let mut saida = Vec::new();
        for (i, &a) in pontos.iter().enumerate() {
            let b = pontos[(i + 1) % pontos.len()];
            if dentro(a) {
                saida.push(a);
            }
            if dentro(a) != dentro(b) {
                let t = (y - a.y) / (b.y - a.y);
                saida.push(pos2(a.x + t * (b.x - a.x), y));
            }
        }
        saida
    };
    let acima = cortar(pontos, &|q| q.y >= y0, y0);
    cortar(&acima, &|q| q.y <= y1, y1)
}

/// Número pseudoaleatório estável entre 0 e 1: o mesmo quadro desenha sempre igual.
pub(crate) fn aleatorio(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
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
    fn cantos_redondos_crescem_o_poligono_pelo_raio() {
        let hex = hexagono(Pos2::ZERO, 10.0, false);
        let redondo = arredondar(&hex, 2.0);
        assert_eq!(redondo.len(), 6 * 5);
        // Todo ponto fica a 2 de algum vértice ou além da aresta: nenhum entra no hexágono.
        for q in &redondo {
            let d = q.to_vec2().length();
            assert!((10.0 * 0.866 + 2.0 - 0.01..=12.0 + 0.01).contains(&d), "{q:?} a {d}");
        }
    }

    #[test]
    fn faixa_corta_entre_as_alturas() {
        let corpo = arredondar(&hexagono(Pos2::ZERO, 30.0, true), 6.0);
        let listra = faixa(&corpo, 18.0, 24.5);
        assert!(listra.len() >= 4);
        assert!(listra.iter().all(|q| (18.0 - 1e-3..=24.5 + 1e-3).contains(&q.y)));
        assert!(faixa(&corpo, 100.0, 120.0).is_empty());
    }
}
