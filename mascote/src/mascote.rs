//! A abelha-robô da Colmeia: desenhada só com formas vetoriais do egui, sem
//! imagens, com cinco estados, cada um ligado a uma situação real do trabalho
//! dos agentes. O acabamento segue a logo: corpo numa célula deitada, asas
//! de favo e a tela do rosto com moldura roxa.

use std::f32::consts::TAU;

use eframe::egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Estado {
    Dormindo,
    Trabalhando,
    Aguardando,
    Bugado,
    Comemorando,
}

impl Estado {
    pub const TODOS: [Estado; 5] = [Estado::Dormindo, Estado::Trabalhando, Estado::Aguardando, Estado::Bugado, Estado::Comemorando];

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

    /// A luz das antenas: as cores de estado dos temas (ok, alerta, erro, destaque).
    fn cor_luz(self) -> Color32 {
        match self {
            Estado::Dormindo => Color32::from_rgb(0x8a, 0x91, 0xa3),
            Estado::Trabalhando => VERDE,
            Estado::Aguardando => AMBAR,
            Estado::Bugado => Color32::from_rgb(0xf4, 0x70, 0x7c),
            Estado::Comemorando => ROXO_CLARO,
        }
    }
}

/// Tempo total e tempo desde que o estado atual começou, em segundos.
#[derive(Clone, Copy)]
pub struct Tempo {
    pub total: f32,
    pub no_estado: f32,
}

const MEL: Color32 = Color32::from_rgb(0xf4, 0xb5, 0x44);
const MEL_SOMBRA: Color32 = Color32::from_rgba_premultiplied(0x60, 0x43, 0x13, 115);
const LISTRA: Color32 = Color32::from_rgb(0x2a, 0x26, 0x38);
const TELA: Color32 = Color32::from_rgb(0x14, 0x12, 0x21);
const OLHO: Color32 = Color32::from_rgb(0x7f, 0xe6, 0xf2);
const ROXO: Color32 = Color32::from_rgb(0x8f, 0x72, 0xf6);
const ROXO_CLARO: Color32 = Color32::from_rgb(0xae, 0x8f, 0xf7);
const VERDE: Color32 = Color32::from_rgb(0x6f, 0xd4, 0x9b);
const AMBAR: Color32 = Color32::from_rgb(0xf1, 0xb6, 0x5f);

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

    /// Polígono convexo em coordenadas do quadro, com contorno opcional.
    fn poligono(&self, pontos: &[Pos2], cor: Color32, contorno: Option<(f32, Color32)>) {
        let pontos = pontos.iter().map(|q| self.pt(q.x, q.y)).collect();
        let traco = contorno.map_or(Stroke::NONE, |(e, c)| Stroke::new(e * self.u, self.cor(c)));
        self.p.add(Shape::convex_polygon(pontos, self.cor(cor), traco));
    }

    /// Contorno de um retângulo arredondado (a moldura da tela).
    fn moldura(&self, (x, y): (f32, f32), (l, a): (f32, f32), raio: f32, espessura: f32, cor: Color32) {
        let rect = Rect::from_center_size(self.pt(x, y), vec2(l * self.u, a * self.u));
        let raio = CornerRadius::same((raio * self.u).min(255.0) as u8);
        self.p.rect_stroke(rect, raio, Stroke::new(espessura * self.u, self.cor(cor)), eframe::egui::StrokeKind::Middle);
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
fn faixa(pontos: &[Pos2], y0: f32, y1: f32) -> Vec<Pos2> {
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
        let cor = if i % 2 == 0 { Color32::from_rgba_unmultiplied(127, 230, 242, 90) } else { Color32::from_rgba_unmultiplied(244, 112, 124, 90) };
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
    let comprimento = 13.0;
    let angulo: f32 = match estado {
        Estado::Dormindo => 1.22,
        Estado::Trabalhando => 0.7 + (tt * 12.0 + lado).sin() * 0.25,
        Estado::Aguardando if lado > 0.0 => -1.05 + (tt * 10.0).sin() * 0.42,
        Estado::Aguardando => 1.13,
        Estado::Bugado if lado > 0.0 => 0.35 + (aleatorio((tt * 14.0) as u32, 6) - 0.5) * 1.2,
        Estado::Bugado => 1.4 + (aleatorio((tt * 14.0) as u32, 5) - 0.5) * 1.2,
        Estado::Comemorando => -0.96 + (tt * 9.0 + lado).sin() * 0.42,
    };
    (angulo.cos() * comprimento * lado.signum(), angulo.sin() * comprimento)
}

/// O corpo: um hexágono deitado, como as células da logo.
const CORPO: [(f32, f32); 6] = [(-40.0, 6.0), (-23.0, -24.0), (23.0, -24.0), (40.0, 6.0), (23.0, 36.0), (-23.0, 36.0)];

fn abelha(p: Pincel, estado: Estado, t: Tempo) {
    let tt = t.total;
    let dorme = estado == Estado::Dormindo;

    // Asas de favo atrás do corpo; batem (achatam) rápido quando trabalha.
    let batida = match estado {
        Estado::Dormindo => 0.0,
        Estado::Trabalhando | Estado::Comemorando => (tt * 45.0).sin() * 0.5 + 0.5,
        Estado::Aguardando => (tt * 21.0).sin() * 0.5 + 0.5,
        Estado::Bugado => aleatorio((tt * 14.0) as u32, 7),
    };
    let asa = Color32::from_rgba_unmultiplied(190, 174, 255, 97);
    let (raio, x, y, giro) = if dorme { (12.0, 30.0, -20.0, 24.0_f32) } else { (15.0, 37.0, -27.0, 32.0) };
    for lado in [-1.0_f32, 1.0] {
        let (s, c) = (giro * lado).to_radians().sin_cos();
        let achatar = 1.0 - 0.45 * batida;
        let colocar = |q: Pos2| {
            let (qx, qy) = (q.x, q.y * achatar);
            pos2(x * lado + qx * c - qy * s, y + qx * s + qy * c)
        };
        let celula: Vec<Pos2> = hexagono(Pos2::ZERO, raio, false).into_iter().map(colocar).collect();
        p.poligono(&celula, asa, Some((1.6, ROXO.gamma_multiply(0.75))));
        let (a, b) = (colocar(pos2(-6.0 * lado, -5.0)), colocar(pos2(2.0 * lado, -9.0)));
        p.linha(a.x, a.y, b.x, b.y, 2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 115));
    }

    // Antenas com a luz do estado; mexem quando está esperando você.
    let balanco = if estado == Estado::Aguardando { (tt * 10.0).sin() * 3.0 } else { 0.0 };
    let luz = estado.cor_luz();
    for lado in [-1.0_f32, 1.0] {
        p.linha(9.0 * lado, -24.0, 17.0 * lado + balanco, -42.0, 2.6, LISTRA);
        if !dorme {
            p.circulo(17.5 * lado + balanco, -44.0, 7.5, luz.gamma_multiply(0.22));
        }
        p.circulo(17.5 * lado + balanco, -44.0, 4.4, luz);
    }

    // Bracinhos com a mão redonda.
    for lado in [-1.0_f32, 1.0] {
        let (ox, oy) = (39.0 * lado, 12.0);
        let (mx, my) = mao(lado, estado, tt);
        p.linha(ox, oy, ox + mx, oy + my, 5.0, LISTRA);
        p.circulo(ox + mx, oy + my, 3.4, LISTRA);
    }

    // Corpo de mel com cantos redondos; as listras e a sombra de baixo cortadas no contorno.
    let corpo = arredondar(&CORPO.map(|(x, y)| pos2(x, y)), 6.0);
    p.poligono(&corpo, MEL, None);
    for (y0, y1, cor) in [(18.0, 24.5, LISTRA), (29.5, 36.0, LISTRA), (38.0, 50.0, MEL_SOMBRA)] {
        let parte = faixa(&corpo, y0, y1);
        if parte.len() >= 3 {
            p.poligono(&parte, cor, None);
        }
    }

    rosto(p, 0.0, -4.5, estado, t);
}

/// A tela no rosto: é ela que mostra a expressão.
fn rosto(p: Pincel, x: f32, y: f32, estado: Estado, t: Tempo) {
    let tt = t.total;
    p.caixa(x, y, 52.0, 31.0, 10.0, TELA);
    p.moldura((x, y), (52.0, 31.0), 10.0, 2.0, ROXO);
    p.linha(x - 19.0, y - 12.5, x - 7.0, y - 12.5, 2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 30));

    let olho = if estado == Estado::Dormindo { Color32::from_rgba_unmultiplied(127, 230, 242, 128) } else { OLHO };
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
                p.circulo(x - 6.0 + i as f32 * 6.0, y + 10.0, 1.6, Color32::from_rgba_unmultiplied(127, 230, 242, fase as u8));
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
                p.texto(34.0 + fase * 16.0, -34.0 - fase * 26.0, 9.0 + fase * 7.0, "z", Color32::from_rgba_unmultiplied(170, 176, 194, alfa));
            }
        }
        Estado::Aguardando => {
            // O balão com o "!" desenhado (a fonte não entra na conta do tamanho).
            let salto = -(tt * 5.0).sin().abs() * 3.0;
            let tinta = Color32::from_rgb(0x1a, 0x1a, 0x1f);
            p.circulo(44.0, -46.0 + salto, 11.0, AMBAR);
            p.caixa(44.0, -48.5 + salto, 3.2, 9.0, 1.6, tinta);
            p.circulo(44.0, -39.6 + salto, 1.9, tinta);
        }
        Estado::Comemorando => {
            let cores = [ROXO_CLARO, VERDE, AMBAR, OLHO];
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
        let corpo = arredondar(&CORPO.map(|(x, y)| pos2(x, y)), 6.0);
        let listra = faixa(&corpo, 18.0, 24.5);
        assert!(listra.len() >= 4);
        assert!(listra.iter().all(|q| (18.0 - 1e-3..=24.5 + 1e-3).contains(&q.y)));
        assert!(faixa(&corpo, 100.0, 120.0).is_empty());
    }

    #[test]
    fn todo_estado_tem_nome_e_situacao() {
        for e in Estado::TODOS {
            assert!(!e.nome().is_empty() && !e.quando().is_empty());
        }
    }
}
