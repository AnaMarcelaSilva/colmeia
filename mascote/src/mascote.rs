//! A abelha-robô da Colmeia: desenhada só com formas vetoriais do egui, sem
//! imagens, com cinco estados, cada um ligado a uma situação real do trabalho
//! dos agentes. Os modelos mudam o acabamento; os estados, o pulo, a sombra,
//! o rosto e o que aparece em volta são os mesmos em todos.

use eframe::egui::{Color32, Painter, Pos2, Rect, vec2};

use crate::fone::{self, Fone};
use crate::modelos;
use crate::pincel::{Pincel, aleatorio};

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
    pub(crate) fn cor_luz(self) -> Color32 {
        match self {
            Estado::Dormindo => Color32::from_rgb(0x8a, 0x91, 0xa3),
            Estado::Trabalhando => VERDE,
            Estado::Aguardando => AMBAR,
            Estado::Bugado => Color32::from_rgb(0xf4, 0x70, 0x7c),
            Estado::Comemorando => ROXO_CLARO,
        }
    }
}

/// O acabamento da abelha. A personagem e os estados são os mesmos; o código
/// é o que o núcleo guarda no perfil.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Modelo {
    /// A da identidade: corpo numa célula deitada, como a logo.
    #[default]
    Colmeia,
    /// A de antes da identidade: redonda, visor escuro e asas cinza.
    Classica,
    /// O corpo redondo da clássica com o acabamento da marca.
    Redonda,
    /// Visor em hexágono e asas de favo só no contorno.
    Favo,
    /// Corpo largo âmbar queimado, olhos grandes e um tufo no lugar das antenas.
    Zangao,
}

impl Modelo {
    pub const TODOS: [Modelo; 5] = [Modelo::Colmeia, Modelo::Classica, Modelo::Redonda, Modelo::Favo, Modelo::Zangao];

    pub fn nome(self) -> &'static str {
        match self {
            Modelo::Colmeia => "Colmeia",
            Modelo::Classica => "Clássica",
            Modelo::Redonda => "Redonda",
            Modelo::Favo => "Favo",
            Modelo::Zangao => "Zangão",
        }
    }

    /// O código guardado no perfil (a mesma lista do núcleo). Ficou o dos
    /// nomes de antes, para não mexer nos perfis gravados.
    pub fn codigo(self) -> &'static str {
        match self {
            Modelo::Colmeia => "identidade",
            Modelo::Classica => "antiga",
            Modelo::Redonda => "meio-termo",
            Modelo::Favo => "favo",
            Modelo::Zangao => "zangao",
        }
    }

    /// Um código desconhecido (de um núcleo mais novo) vira o padrão.
    pub fn do_codigo(codigo: &str) -> Modelo {
        Modelo::TODOS.into_iter().find(|m| m.codigo() == codigo).unwrap_or_default()
    }
}

/// Tempo total e tempo desde que o estado atual começou, em segundos.
#[derive(Clone, Copy)]
pub struct Tempo {
    pub total: f32,
    pub no_estado: f32,
}

pub(crate) const OLHO: Color32 = Color32::from_rgb(0x7f, 0xe6, 0xf2);
pub(crate) const ROXO: Color32 = Color32::from_rgb(0x8f, 0x72, 0xf6);
pub(crate) const ROXO_CLARO: Color32 = Color32::from_rgb(0xae, 0x8f, 0xf7);
pub(crate) const VERDE: Color32 = Color32::from_rgb(0x6f, 0xd4, 0x9b);
pub(crate) const AMBAR: Color32 = Color32::from_rgb(0xf1, 0xb6, 0x5f);

/// O que a abelha veste além dela: o fone com o microfone aberto e, onde
/// quem desenha já mostra o aviso (a abelha fixa), sem o balão do "!".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Acessorios {
    pub fone: Option<Fone>,
    pub sem_aviso: bool,
}

/// Desenha a abelha `modelo` no estado `estado`, centrada em `centro`, num
/// quadrado de lado `tamanho`.
pub fn desenhar(painter: &Painter, centro: Pos2, tamanho: f32, modelo: Modelo, estado: Estado, t: Tempo) {
    desenhar_com(painter, centro, tamanho, modelo, estado, t, Acessorios::default());
}

/// Como `desenhar`, com os acessórios.
pub fn desenhar_com(painter: &Painter, centro: Pos2, tamanho: f32, modelo: Modelo, estado: Estado, t: Tempo, a: Acessorios) {
    let base = Pincel { p: painter, c: centro, u: tamanho / 100.0, tinta: None };
    let corpo = |p: Pincel| corpo(p, modelo, estado, t, a.fone);
    if estado != Estado::Bugado {
        corpo(base);
        if !(a.sem_aviso && estado == Estado::Aguardando) {
            extras(base, estado, t);
        }
        return;
    }

    // Bugado: a imagem "quebra" em faixas, com cores deslocadas e chiado.
    // Rajadas fortes e curtas, com tremidas leves entre elas.
    let semente = (t.total * 14.0) as u32;
    let forte = t.total % 0.9 < 0.3;
    let intensidade = if forte { 1.0 } else { 0.25 };
    let deslocamento = (2.0 + 3.0 * aleatorio(semente, 0)) * intensidade;
    corpo(Pincel { tinta: Some(Color32::from_rgba_unmultiplied(255, 60, 90, 120)), ..base.movido(-deslocamento, 0.0) });
    corpo(Pincel { tinta: Some(Color32::from_rgba_unmultiplied(60, 230, 255, 120)), ..base.movido(deslocamento, 0.0) });

    let area = Rect::from_center_size(centro, vec2(tamanho * 1.6, tamanho * 1.4));
    let faixas = 9;
    let altura = area.height() / faixas as f32;
    for i in 0..faixas {
        let faixa = Rect::from_min_size(area.min + vec2(0.0, i as f32 * altura), vec2(area.width(), altura));
        let desvio = if aleatorio(semente, i + 10) > 0.55 { (aleatorio(semente, i + 30) - 0.5) * 18.0 * intensidade } else { 0.0 };
        let recorte = painter.with_clip_rect(faixa.intersect(painter.clip_rect()));
        corpo(Pincel { p: &recorte, ..base.movido(desvio, 0.0) });
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

fn corpo(p: Pincel, modelo: Modelo, estado: Estado, t: Tempo, fone: Option<Fone>) {
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
    match modelo {
        Modelo::Colmeia => modelos::colmeia::desenhar(p, estado, t),
        Modelo::Classica => modelos::classica::desenhar(p, estado, t),
        Modelo::Redonda => modelos::redonda::desenhar(p, estado, t),
        Modelo::Favo => modelos::favo::desenhar(p, estado, t),
        Modelo::Zangao => modelos::zangao::desenhar(p, estado, t),
    }
    // O fone vai junto com o corpo (no pulo e na quebra do bugado).
    if let Some(f) = fone {
        fone::desenhar(p, modelo, f);
    }
}

/// Quanto as asas batem (0 a 1) em cada estado.
pub(crate) fn batida(estado: Estado, tt: f32) -> f32 {
    match estado {
        Estado::Dormindo => 0.0,
        Estado::Trabalhando | Estado::Comemorando => (tt * 45.0).sin() * 0.5 + 0.5,
        Estado::Aguardando => (tt * 21.0).sin() * 0.5 + 0.5,
        Estado::Bugado => aleatorio((tt * 14.0) as u32, 7),
    }
}

/// O balanço das antenas: mexem quando está esperando você.
pub(crate) fn balanco(estado: Estado, tt: f32) -> f32 {
    if estado == Estado::Aguardando { (tt * 10.0).sin() * 3.0 } else { 0.0 }
}

/// A posição da mão de um braço de `comprimento`, em cada estado (`lado` é -1 ou 1).
pub(crate) fn mao(lado: f32, comprimento: f32, estado: Estado, tt: f32) -> (f32, f32) {
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

/// Onde fica o rosto e as cores dele: cada modelo desenha a tela do seu
/// jeito, e os olhos e a boca (a expressão) são os mesmos.
pub(crate) struct Rosto {
    pub x: f32,
    pub y: f32,
    /// Distância de cada olho ao meio.
    pub olhos: f32,
    pub tela: Color32,
    pub olho: Color32,
}

/// Os olhos e a boca na tela do rosto: é ela que mostra a expressão.
pub(crate) fn expressao(p: Pincel, r: &Rosto, estado: Estado, t: Tempo) {
    let (x, y, tt) = (r.x, r.y, t.total);
    let olho = if estado == Estado::Dormindo { r.olho.gamma_multiply(0.5) } else { r.olho };
    for lado in [-1.0_f32, 1.0] {
        let ox = x + r.olhos * lado;
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
                    p.circulo(ox + 1.5, y - 2.5, 1.6, r.tela);
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
                let cor = Color32::from_rgba_unmultiplied(r.olho.r(), r.olho.g(), r.olho.b(), fase as u8);
                p.circulo(x - 6.0 + i as f32 * 6.0, y + 10.0, 1.6, cor);
            }
        }
        Estado::Comemorando => {
            p.linha(x - 5.0, y + 8.0, x, y + 10.0, 1.8, r.olho);
            p.linha(x, y + 10.0, x + 5.0, y + 8.0, 1.8, r.olho);
        }
        Estado::Aguardando => p.circulo(x, y + 9.0, 2.2, r.olho),
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
    use eframe::egui::{Context, pos2};

    #[test]
    fn todo_estado_tem_nome_e_situacao() {
        for e in Estado::TODOS {
            assert!(!e.nome().is_empty() && !e.quando().is_empty());
        }
    }

    #[test]
    fn codigo_do_modelo_vai_e_volta_e_o_desconhecido_vira_o_padrao() {
        for m in Modelo::TODOS {
            assert_eq!(Modelo::do_codigo(m.codigo()), m);
            assert!(!m.nome().is_empty());
        }
        assert_eq!(Modelo::do_codigo("unicornio"), Modelo::Colmeia);
        assert_eq!(Modelo::default(), Modelo::Colmeia);
    }

    /// Os nomes mudaram; os códigos não, para os perfis gravados continuarem valendo.
    #[test]
    fn codigos_gravados_continuam_os_de_antes() {
        let codigos: Vec<_> = Modelo::TODOS.iter().map(|m| m.codigo()).collect();
        assert_eq!(codigos, ["identidade", "antiga", "meio-termo", "favo", "zangao"]);
        let nomes: Vec<_> = Modelo::TODOS.iter().map(|m| m.nome()).collect();
        assert_eq!(nomes, ["Colmeia", "Clássica", "Redonda", "Favo", "Zangão"]);
    }

    /// Todo modelo desenha todo estado dentro do quadro (com folga para o
    /// pulo, os confetes e a quebra do bugado) e sem pontos inválidos.
    #[test]
    fn todo_modelo_desenha_todo_estado_dentro_do_quadro() {
        let ctx = Context::default();
        let limite = Rect::from_center_size(pos2(200.0, 200.0), vec2(172.0, 172.0));
        let acessorios =
            [Acessorios::default(), Acessorios { fone: Some(Fone::Ouvindo), sem_aviso: true }, Acessorios { fone: Some(Fone::Esperando), sem_aviso: false }];
        for (modelo, a) in Modelo::TODOS.into_iter().flat_map(|m| acessorios.map(|a| (m, a))) {
            for estado in Estado::TODOS {
                for total in [0.0, 0.4, 1.7, 3.3] {
                    let mut saida = ctx.run_ui(Default::default(), |ui| {
                        desenhar_com(ui.painter(), pos2(200.0, 200.0), 100.0, modelo, estado, Tempo { total, no_estado: total }, a);
                    });
                    saida.textures_delta.clear();
                    assert!(saida.shapes.len() > 10, "{modelo:?} {estado:?}: só {} formas", saida.shapes.len());
                    for forma in &saida.shapes {
                        let r = forma.shape.visual_bounding_rect();
                        if r.is_positive() {
                            assert!(limite.contains_rect(r), "{modelo:?} {estado:?} em {total}: forma fora do quadro {r:?}");
                        }
                    }
                }
            }
        }
    }
}
