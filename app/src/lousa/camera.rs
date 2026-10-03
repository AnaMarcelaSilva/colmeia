//! A câmera da lousa: que ponto do quadro fica no canto da área e com que
//! zoom. Funções puras, sem egui além dos tipos de ponto e retângulo.
//!
//! O zoom anda em níveis fixos (a Ctrl+rodinha, os botões e a pinça): menos
//! tamanhos de fonte no atlas e menos layout refeito. Só o "ajustar" do
//! slide e a transição do palco usam valores livres.

use eframe::egui::{Pos2, Rect, Vec2, pos2};

/// Níveis de zoom, do menor ao maior.
pub const NIVEIS: [f32; 14] = [0.10, 0.25, 0.33, 0.50, 0.67, 0.75, 0.90, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0, 4.0];

pub const ZOOM_MINIMO: f32 = NIVEIS[0];
pub const ZOOM_MAXIMO: f32 = NIVEIS[NIVEIS.len() - 1];

/// Quanto a pinça (ou a Ctrl+rodinha) precisa acumular para trocar um nível.
const LIMIAR_PINCA: f32 = 0.18;
/// No máximo um nível nesse intervalo: o egui espalha um dente da rodinha
/// por vários quadros, e cada um passaria do limiar sozinho.
const INTERVALO_PINCA: f64 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// O ponto do quadro no canto de cima à esquerda da área.
    pub origem: Pos2,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { origem: pos2(-40.0, -40.0), zoom: 1.0 }
    }
}

impl Camera {
    /// Do quadro para a tela (`area` é onde a lousa está na janela).
    pub fn para_tela(&self, area: Rect, p: Pos2) -> Pos2 {
        area.min + (p - self.origem) * self.zoom
    }

    pub fn para_quadro(&self, area: Rect, t: Pos2) -> Pos2 {
        self.origem + (t - area.min) / self.zoom
    }

    pub fn retangulo_na_tela(&self, area: Rect, r: Rect) -> Rect {
        Rect::from_min_max(self.para_tela(area, r.min), self.para_tela(area, r.max))
    }

    /// O pedaço do quadro que aparece na área.
    pub fn visivel(&self, area: Rect) -> Rect {
        Rect::from_min_size(self.origem, area.size() / self.zoom)
    }

    /// Move a vista: `delta` em pixels de tela.
    pub fn mover(&mut self, delta: Vec2) {
        self.origem -= delta / self.zoom;
    }

    /// Troca o zoom mantendo parado o ponto do quadro sob `ancora` (na tela).
    pub fn zoom_em(&mut self, area: Rect, ancora: Pos2, zoom: f32) {
        let zoom = zoom.clamp(ZOOM_MINIMO, ZOOM_MAXIMO);
        let fixo = self.para_quadro(area, ancora);
        self.zoom = zoom;
        self.origem = fixo - (ancora - area.min) / zoom;
    }

    /// Um nível acima (`passo` 1) ou abaixo (-1) do zoom atual, no ponto dado.
    pub fn passo_de_zoom(&mut self, area: Rect, ancora: Pos2, passo: i32) {
        let novo = if passo > 0 { nivel_acima(self.zoom) } else { nivel_abaixo(self.zoom) };
        self.zoom_em(area, ancora, novo);
    }

    /// Enquadra `conteudo` (no quadro) na área com `margem` de tela, com zoom
    /// até `maximo`. `em_niveis` escolhe o maior nível que cabe (a lousa);
    /// sem ele, o zoom é o exato (o slide e o palco).
    pub fn enquadrar(area: Rect, conteudo: Rect, margem: f32, maximo: f32, em_niveis: bool) -> Camera {
        let livre = (area.size() - Vec2::splat(2.0 * margem)).max(Vec2::splat(1.0));
        let tamanho = conteudo.size().max(Vec2::splat(1.0));
        let exato = (livre.x / tamanho.x).min(livre.y / tamanho.y).min(maximo);
        let zoom = if em_niveis { nivel_que_cabe(exato) } else { exato.clamp(0.02, ZOOM_MAXIMO) };
        let centro = conteudo.center();
        Camera { origem: centro - area.size() / (2.0 * zoom), zoom }
    }

    /// Mistura entre duas câmeras (a transição do palco): `t` de 0 a 1, com
    /// o zoom interpolado em escala logarítmica e o centro em linha reta.
    pub fn entre(a: Camera, b: Camera, area: Rect, t: f32) -> Camera {
        let t = t.clamp(0.0, 1.0);
        let suave = t * t * (3.0 - 2.0 * t);
        let zoom = (a.zoom.ln() + (b.zoom.ln() - a.zoom.ln()) * suave).exp();
        let meio = area.size() / 2.0;
        let (ca, cb) = (a.origem + meio / a.zoom, b.origem + meio / b.zoom);
        let centro = ca + (cb - ca) * suave;
        Camera { origem: centro - meio / zoom, zoom }
    }

    /// O texto do botão de zoom: "100%".
    pub fn porcentagem(&self) -> String {
        format!("{}%", (self.zoom * 100.0).round() as i32)
    }
}

/// O maior nível que não passa de `zoom` (o menor nível, se nenhum couber).
pub fn nivel_que_cabe(zoom: f32) -> f32 {
    NIVEIS.iter().rev().copied().find(|n| *n <= zoom + 1e-4).unwrap_or(ZOOM_MINIMO)
}

pub fn nivel_acima(zoom: f32) -> f32 {
    NIVEIS.iter().copied().find(|n| *n > zoom + 1e-4).unwrap_or(ZOOM_MAXIMO)
}

pub fn nivel_abaixo(zoom: f32) -> f32 {
    NIVEIS.iter().rev().copied().find(|n| *n < zoom - 1e-4).unwrap_or(ZOOM_MINIMO)
}

/// O zoom em que o texto é montado: os níveis ficam como estão; um zoom
/// livre (ajuste do slide, transição do palco) desce para o múltiplo de 5%
/// abaixo dele, para não encher o atlas de tamanhos de fonte.
pub fn zoom_do_texto(zoom: f32) -> f32 {
    if NIVEIS.iter().any(|n| (n - zoom).abs() < 1e-3) {
        return zoom;
    }
    ((zoom * 20.0).floor() / 20.0).max(0.05)
}

/// Pinça e Ctrl+rodinha acumuladas: o nível só muda quando a soma cruza o
/// limiar, e um gesto troca no máximo um nível por quadro (sem atravessar os
/// 14 de uma vez).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pinca {
    acumulado: f32,
    ultimo: f64,
}

impl Pinca {
    /// Recebe o fator do quadro (1 = nada) e o momento, e diz se é para
    /// subir (1), descer (-1) ou ficar (0).
    pub fn passo(&mut self, fator: f32, agora: f64) -> i32 {
        if fator <= 0.0 || (fator - 1.0).abs() < 1e-6 {
            return 0;
        }
        if agora - self.ultimo < INTERVALO_PINCA {
            // Ainda no mesmo dente: o resto do gesto não conta.
            return 0;
        }
        self.acumulado += fator.ln();
        let passo = if self.acumulado >= LIMIAR_PINCA {
            1
        } else if self.acumulado <= -LIMIAR_PINCA {
            -1
        } else {
            return 0;
        };
        self.acumulado = 0.0;
        self.ultimo = agora;
        passo
    }
}

/// O retângulo que envolve todos, ou None se a lista está vazia.
pub fn envolver(caixas: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    caixas.into_iter().reduce(|a, b| a.union(b))
}

#[cfg(test)]
mod testes {
    use super::*;
    use eframe::egui::vec2;

    fn area() -> Rect {
        Rect::from_min_size(pos2(236.0, 100.0), vec2(1200.0, 700.0))
    }

    #[test]
    fn ida_e_volta_entre_tela_e_quadro() {
        let c = Camera { origem: pos2(-300.0, 50.0), zoom: 0.67 };
        for p in [pos2(0.0, 0.0), pos2(1234.5, -987.25), pos2(-40000.0, 80000.0)] {
            let volta = c.para_quadro(area(), c.para_tela(area(), p));
            assert!((volta - p).length() < 0.05, "{p:?} voltou {volta:?}");
        }
        assert_eq!(c.para_tela(area(), c.origem), area().min);
    }

    #[test]
    fn zoom_fica_preso_no_ponto_do_cursor() {
        let mut c = Camera::default();
        let cursor = pos2(700.0, 420.0);
        let antes = c.para_quadro(area(), cursor);
        c.passo_de_zoom(area(), cursor, 1);
        assert_eq!(c.zoom, 1.1);
        assert!((c.para_quadro(area(), cursor) - antes).length() < 1e-3);
        c.passo_de_zoom(area(), cursor, -1);
        c.passo_de_zoom(area(), cursor, -1);
        assert_eq!(c.zoom, 0.9);
        assert!((c.para_quadro(area(), cursor) - antes).length() < 1e-3);
        // Nos extremos, fica.
        for _ in 0..30 {
            c.passo_de_zoom(area(), cursor, -1);
        }
        assert_eq!(c.zoom, ZOOM_MINIMO);
        for _ in 0..30 {
            c.passo_de_zoom(area(), cursor, 1);
        }
        assert_eq!(c.zoom, ZOOM_MAXIMO);
    }

    #[test]
    fn mover_a_vista_acompanha_o_mouse() {
        let mut c = Camera { origem: pos2(0.0, 0.0), zoom: 2.0 };
        let p = pos2(10.0, 10.0);
        let antes = c.para_tela(area(), p);
        c.mover(vec2(30.0, -20.0));
        assert_eq!(c.para_tela(area(), p), antes + vec2(30.0, -20.0));
    }

    #[test]
    fn ajustar_enquadra_tudo_com_margem() {
        let conteudo = Rect::from_min_size(pos2(-500.0, 200.0), vec2(3000.0, 1000.0));
        let c = Camera::enquadrar(area(), conteudo, 48.0, 1.0, true);
        assert!(NIVEIS.contains(&c.zoom) && c.zoom <= 1.0);
        let na_tela = c.retangulo_na_tela(area(), conteudo);
        assert!(area().shrink(47.0).contains_rect(na_tela), "{na_tela:?}");
        assert!((na_tela.center() - area().center()).length() < 0.5);
        // Pouca coisa: no máximo 100%.
        let pequeno = Camera::enquadrar(area(), Rect::from_min_size(Pos2::ZERO, vec2(100.0, 50.0)), 48.0, 1.0, true);
        assert_eq!(pequeno.zoom, 1.0);
        // O ajuste do slide é exato (sem níveis).
        let exato = Camera::enquadrar(area(), conteudo, 0.0, 1.0, false);
        assert!((exato.zoom - 0.4).abs() < 1e-4, "{}", exato.zoom);
    }

    #[test]
    fn niveis_e_zoom_do_texto() {
        assert_eq!(nivel_acima(1.0), 1.1);
        assert_eq!(nivel_abaixo(1.0), 0.9);
        assert_eq!(nivel_acima(0.98), 1.0);
        assert_eq!(nivel_que_cabe(0.98), 0.9);
        assert_eq!(nivel_que_cabe(0.01), ZOOM_MINIMO);
        assert_eq!(zoom_do_texto(0.33), 0.33);
        assert_eq!(zoom_do_texto(0.437), 0.40);
        assert_eq!(zoom_do_texto(0.01), 0.05);
    }

    #[test]
    fn pinca_acumulada_nao_atravessa_todos_os_niveis() {
        let mut p = Pinca::default();
        // Um gesto de pinça em vinte quadros (de 16 ms), cada um 3% maior.
        let passos: i32 = (0..20).map(|i| p.passo(1.03, 1.0 + i as f64 * 0.016)).sum();
        assert!((1..=3).contains(&passos), "{passos} níveis num gesto");
        assert_eq!(p.passo(1.0, 2.0), 0);
        // Um dente da rodinha espalhado em três quadros: um nível só.
        let mut p = Pinca::default();
        let passos: i32 = [0.8, 0.8, 0.8].iter().enumerate().map(|(i, f)| p.passo(*f, 5.0 + i as f64 * 0.016)).sum();
        assert_eq!(passos, -1);
        assert_eq!(p.passo(1.25, 6.0), 1);
    }

    #[test]
    fn transicao_vai_de_uma_camera_a_outra() {
        let a = Camera { origem: pos2(0.0, 0.0), zoom: 0.5 };
        let b = Camera { origem: pos2(2000.0, 900.0), zoom: 2.0 };
        assert_eq!(Camera::entre(a, b, area(), 0.0), a);
        let fim = Camera::entre(a, b, area(), 1.0);
        assert!((fim.zoom - 2.0).abs() < 1e-4 && (fim.origem - b.origem).length() < 0.1);
        let meio = Camera::entre(a, b, area(), 0.5);
        assert!((meio.zoom - 1.0).abs() < 1e-3);
    }
}
