//! O desenho da lousa, o mesmo na lousa editável, no slide e no palco: só
//! o que aparece na área, com o layout dos textos guardado por item.
//!
//! O conteúdo (texto, cantos, recuos) está em unidades do quadro e escala
//! com o zoom; a moldura da interação (seleção, alças, barras) fica em
//! pixels de tela e é desenhada por quem chama. Quando a letra fica pequena
//! demais na tela (menos de 6 px), o texto vira barras: nada de rasterizar
//! letras de 2 px nem de montar layout à toa no zoom baixo.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use eframe::egui::epaint::{CubicBezierShape, Galley};
use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, TextFormat, Vec2, pos2, vec2};

use super::camera::{self, Camera};
use super::markdown::{self, Alinhamento, Bloco, Marcador, Trecho};
use crate::api::{ElementoLousa as Elemento, TipoElemento};
use crate::registro::{CacheImagens, Miniatura, tamanho};
use crate::tema::{self, CODIGO, ESCURO, cores, forte};

/// Recuo interno da nota e do texto solto (unidades do quadro).
pub const RECUO: f32 = 14.0;
/// Faixa do título da nota e do código.
pub const FAIXA: f32 = 24.0;
const RECUO_CODIGO: f32 = 12.0;
const LINHA_CODIGO: f32 = 18.0;
/// Abaixo disso (px de tela), o texto vira barra.
const MENOR_LETRA: f32 = 6.0;
/// Títulos ficam como texto até este tamanho.
const MENOR_TITULO: f32 = 4.0;
/// Itens guardados no cache de layout.
const MAX_CACHE: usize = 400;

/// Uma peça do conteúdo de um item, em px de tela relativos ao canto do conteúdo.
#[derive(Clone)]
pub enum Peca {
    Texto { pos: Vec2, galeria: Arc<Galley>, letra: f32, titulo: bool },
    Barra { rect: Rect, cor: Color32 },
    Fundo { rect: Rect, cor: Color32, raio: f32 },
    Linha { a: Pos2, b: Pos2, cor: Color32 },
    Caixa { rect: Rect, marcada: bool, tinta: Color32, papel: Color32 },
}

/// O conteúdo montado de um item num zoom: as peças e a altura total.
#[derive(Clone, Default)]
pub struct Composicao {
    pub pecas: Vec<Peca>,
    pub altura: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Chave {
    id: i64,
    rev: u64,
    zoom: u32,
    largura: u32,
    altura: u32,
    tema: u8,
    barras: bool,
}

/// As cores do conteúdo de um item.
#[derive(Clone, Copy)]
pub struct Tinta {
    pub texto: Color32,
    pub secundaria: Color32,
    pub linha: Color32,
    pub recuo: Color32,
    pub papel: Color32,
}

pub fn tinta(e: &Elemento) -> Tinta {
    let p = cores();
    match e.tipo {
        TipoElemento::Nota => {
            let c = tema::cor_nota(&e.cor);
            Tinta { texto: c.tinta, secundaria: c.secundaria, linha: c.linha, recuo: c.recuo, papel: c.fundo }
        }
        TipoElemento::Codigo => Tinta { texto: CODIGO.texto, secundaria: CODIGO.suave, linha: CODIGO.borda, recuo: CODIGO.fundo, papel: CODIGO.fundo },
        _ => Tinta { texto: p.texto, secundaria: p.suave, linha: p.borda, recuo: p.superficie, papel: p.fundo },
    }
}

/// O tema atual, para a chave do cache (as cores ficam dentro do layout).
fn tema_atual() -> u8 {
    if !tema::claro() {
        0
    } else if cores().fundo == tema::LEITURA.fundo {
        2
    } else {
        1
    }
}

/// O que o desenho precisa saber de uma tarefa para o cartão.
#[derive(Clone, Debug, Default)]
pub struct InfoTarefa {
    pub titulo: String,
    pub projeto: String,
    pub coluna: String,
    pub erro: bool,
    pub estado: Option<tema::EstadoVisual>,
}

/// O estado da tela que muda o desenho de um item.
#[derive(Default)]
pub struct Marcas<'a> {
    /// O item com o texto aberto no editor: só o papel é desenhado.
    pub editando: Option<i64>,
    pub novos: Option<&'a std::collections::HashSet<i64>>,
    pub nao_salvos: Option<&'a std::collections::HashSet<i64>>,
    /// Vídeo abrindo ou com erro (o id e o texto).
    pub video: Option<(i64, String, bool)>,
    /// Ligações destacadas (selecionadas ou com o mouse em cima).
    pub ligacoes_destacadas: Vec<i64>,
    /// Itens com o mouse em cima (o cartão de tarefa muda a borda).
    pub em_cima: Option<i64>,
    /// Sem sombra, selo nem marca de "novo" (o palco e o slide).
    pub limpo: bool,
    /// O cartão em destaque no palco: as ligações que não o tocam ficam apagadas.
    pub foco: Option<i64>,
}

/// Quem vigia o atlas das letras do egui num contexto (veja `geracao_das_fontes`).
#[derive(Clone, Copy)]
struct VigiaFontes {
    passada: u64,
    opcoes: egui::epaint::text::TextOptions,
    pontos: f32,
    cheio: f32,
    geracao: u64,
}

static GERACOES: AtomicU64 = AtomicU64::new(1);

/// Um número que muda quando o egui refaz o atlas das letras: troca de tema
/// (as opções do texto mudam entre claro e escuro), de escala ou atlas quase
/// cheio. As galerias guardadas apontam para o atlas em que foram montadas;
/// com outro atlas, sairiam letras trocadas. Medido uma vez por passada, no
/// começo de cada quadro (main) e de novo por quem desenha.
pub fn geracao_das_fontes(ctx: &egui::Context) -> u64 {
    let id = egui::Id::new("lousa-geracao-das-fontes");
    let passada = ctx.cumulative_pass_nr();
    if let Some(v) = ctx.data(|d| d.get_temp::<VigiaFontes>(id))
        && v.passada == passada
    {
        return v.geracao;
    }
    let (opcoes, cheio) = ctx.fonts(|f| (*f.options(), f.font_atlas_fill_ratio()));
    let pontos = ctx.pixels_per_point();
    ctx.data_mut(|d| {
        let anterior = d.get_temp::<VigiaFontes>(id);
        // O atlas só esvazia quando é refeito.
        let refeito = anterior.is_none_or(|v| v.opcoes != opcoes || v.pontos != pontos || cheio < v.cheio);
        let geracao = match anterior {
            Some(v) if !refeito => v.geracao,
            _ => GERACOES.fetch_add(1, Ordering::Relaxed),
        };
        d.insert_temp(id, VigiaFontes { passada, opcoes, pontos, cheio, geracao });
        geracao
    })
}

/// O desenho de uma lousa: o cache de layout e as imagens.
pub struct Desenho {
    cache: HashMap<Chave, (Arc<Composicao>, u64)>,
    quadro: u64,
    /// A geração do atlas das letras em que o cache foi montado.
    fontes: u64,
    pub imagens: CacheImagens,
    pub miniaturas: CacheImagens,
}

impl Desenho {
    pub fn novo(prefixo: &'static str) -> Desenho {
        Desenho {
            cache: HashMap::new(),
            quadro: 0,
            fontes: 0,
            imagens: CacheImagens::new(Some(1600), 24, prefixo),
            miniaturas: CacheImagens::new(Some(256), 200, prefixo),
        }
    }

    pub fn receber(&mut self, ctx: &egui::Context) {
        self.imagens.receber(ctx);
        self.miniaturas.receber(ctx);
    }

    /// Quantos layouts estão guardados (para os testes e a medição).
    #[cfg(test)]
    pub fn guardados(&self) -> usize {
        self.cache.len()
    }

    /// O conteúdo de um item no zoom, do cache ou montado agora.
    pub fn composicao(&mut self, pintor: &Painter, e: &Elemento, zoom: f32) -> Arc<Composicao> {
        let fontes = geracao_das_fontes(pintor.ctx());
        if fontes != self.fontes {
            // Atlas novo: as galerias guardadas não servem mais.
            self.cache.clear();
            self.fontes = fontes;
        }
        let zt = camera::zoom_do_texto(zoom);
        let barras = letra_base(e) * zt < MENOR_LETRA;
        let chave = Chave {
            id: e.id,
            rev: e.rev,
            zoom: zt.to_bits(),
            largura: e.largura.to_bits(),
            altura: if e.tipo == TipoElemento::Codigo { e.altura.to_bits() } else { 0 },
            tema: tema_atual(),
            barras,
        };
        self.quadro += 1;
        if let Some((c, usado)) = self.cache.get_mut(&chave) {
            *usado = self.quadro;
            return c.clone();
        }
        let composicao = if barras {
            // A geometria vem do layout a 50% (letra de 7 px), reduzida; os
            // títulos que ainda se leem são montados de verdade.
            let base = self.composicao(pintor, e, 0.5);
            Arc::new(reduzir(pintor, &base, zt / 0.5))
        } else {
            Arc::new(compor(pintor, e, zt))
        };
        if self.cache.len() >= MAX_CACHE {
            // Sai a metade menos usada de uma vez (não a cada item novo).
            let mut usos: Vec<u64> = self.cache.values().map(|(_, u)| *u).collect();
            usos.sort_unstable();
            let corte = usos[usos.len() / 2];
            self.cache.retain(|_, (_, u)| *u > corte);
        }
        self.cache.insert(chave, (composicao.clone(), self.quadro));
        composicao
    }

    /// A altura que o conteúdo pede (unidades do quadro), para a nota crescer.
    pub fn altura_do_conteudo(&mut self, pintor: &Painter, e: &Elemento) -> f32 {
        let c = self.composicao(pintor, e, 1.0);
        let faixa = if e.titulo.is_empty() || e.tipo == TipoElemento::Texto { 0.0 } else { FAIXA };
        match e.tipo {
            TipoElemento::Codigo => faixa + RECUO_CODIGO * 2.0 + e.texto.lines().count().max(1) as f32 * LINHA_CODIGO,
            TipoElemento::Texto => c.altura + 4.0,
            _ => faixa + c.altura + RECUO * 2.0,
        }
    }

    /// Desenha os itens visíveis na área, na ordem (a lista já vem ordenada).
    #[allow(clippy::too_many_arguments)]
    pub fn desenhar(
        &mut self,
        ui: &egui::Ui,
        area: Rect,
        camera: &Camera,
        elementos: &[Elemento],
        tarefas: &dyn Fn(i64) -> Option<InfoTarefa>,
        marcas: &Marcas,
    ) {
        let pintor = ui.painter_at(area);
        let ctx = ui.ctx().clone();
        let visivel = camera.visivel(area).expand(40.0 / camera.zoom);
        let por_id: HashMap<i64, &Elemento> = elementos.iter().map(|e| (e.id, e)).collect();
        // As linhas das ligações vão por baixo dos cartões (não cruzam o
        // texto); a ponta da seta e o rótulo, por cima.
        let mut ligacoes = Vec::new();
        for e in elementos.iter().filter(|e| e.tipo == TipoElemento::Ligacao) {
            let (Some(de), Some(para)) = (por_id.get(&e.de), por_id.get(&e.para)) else { continue };
            let caixa = caixa_quadro(de).union(caixa_quadro(para));
            if !caixa.intersects(visivel) {
                continue;
            }
            let apagada = marcas.foco.is_some_and(|f| e.de != f && e.para != f);
            let ligacao = Ligacao::nova(area, camera, e, de, para, marcas.ligacoes_destacadas.contains(&e.id), apagada);
            ligacao.linha(&pintor);
            ligacoes.push(ligacao);
        }
        for e in elementos {
            if e.tipo == TipoElemento::Ligacao || !caixa_quadro(e).intersects(visivel) {
                continue;
            }
            let r = camera.retangulo_na_tela(area, caixa_quadro(e));
            self.desenhar_item(&pintor, &ctx, r, camera.zoom, e, tarefas, marcas);
        }
        let com_rotulo = !marcas.limpo || camera.zoom >= 0.4;
        for ligacao in &ligacoes {
            ligacao.ponta_e_rotulo(&pintor, camera.zoom, com_rotulo);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn desenhar_item(
        &mut self,
        pintor: &Painter,
        ctx: &egui::Context,
        r: Rect,
        z: f32,
        e: &Elemento,
        tarefas: &dyn Fn(i64) -> Option<InfoTarefa>,
        marcas: &Marcas,
    ) {
        let p = cores();
        let reduzido = letra_base(e) * z < MENOR_LETRA;
        let canto = canto(tema::RAIO_CARTAO as f32, z);
        let editando = marcas.editando == Some(e.id);
        match e.tipo {
            TipoElemento::Nota => {
                let t = tinta(e);
                if !reduzido && !marcas.limpo {
                    pintor.add(tema::sombra(2, 8).as_shape(r, CornerRadius::same(canto as u8)));
                }
                pintor.rect(r, CornerRadius::same(canto as u8), t.papel, Stroke::new(1.0, t.linha), StrokeKind::Inside);
                let mut topo = r.top();
                if !e.titulo.is_empty() {
                    let faixa = Rect::from_min_max(r.min, pos2(r.right(), r.top() + FAIXA * z));
                    let c = canto as u8;
                    pintor.rect_filled(faixa.shrink(0.5), CornerRadius { nw: c, ne: c, sw: 0, se: 0 }, t.recuo);
                    if 11.5 * z >= MENOR_TITULO {
                        let g = tema::cortar(
                            pintor,
                            &e.titulo,
                            TextFormat::simple(forte(11.5 * camera::zoom_do_texto(z)), t.secundaria),
                            faixa.width() - 2.0 * RECUO * z,
                            1,
                            false,
                        );
                        pintor.galley(pos2(faixa.left() + RECUO * z, faixa.center().y - g.size().y / 2.0), g, t.secundaria);
                    }
                    topo = faixa.bottom();
                }
                if !editando {
                    let conteudo = Rect::from_min_max(pos2(r.left() + RECUO * z, topo + RECUO * z), pos2(r.right() - RECUO * z, r.bottom() - RECUO * z));
                    let c = self.composicao(pintor, e, z);
                    self.pintar_conteudo(pintor, r, conteudo, &c, t, reduzido);
                }
            }
            TipoElemento::Texto => {
                if !editando {
                    let t = tinta(e);
                    let c = self.composicao(pintor, e, z);
                    self.pintar_conteudo(pintor, r, r, &c, t, reduzido);
                }
            }
            TipoElemento::Codigo => {
                if !reduzido && !marcas.limpo {
                    pintor.add(tema::sombra(2, 8).as_shape(r, CornerRadius::same(canto as u8)));
                }
                pintor.rect(r, CornerRadius::same(canto as u8), CODIGO.fundo, Stroke::new(1.0, CODIGO.borda), StrokeKind::Inside);
                let mut topo = r.top();
                if !e.titulo.is_empty() {
                    let faixa = Rect::from_min_max(r.min, pos2(r.right(), r.top() + FAIXA * z));
                    if 11.0 * z >= MENOR_TITULO {
                        let g = tema::cortar(
                            pintor,
                            &e.titulo,
                            TextFormat::simple(FontId::monospace(11.0 * camera::zoom_do_texto(z)), CODIGO.suave),
                            faixa.width() - 2.0 * RECUO_CODIGO * z,
                            1,
                            true,
                        );
                        pintor.galley(pos2(faixa.left() + RECUO_CODIGO * z, faixa.center().y - g.size().y / 2.0), g, CODIGO.suave);
                    }
                    pintor.line_segment([faixa.left_bottom(), faixa.right_bottom()], Stroke::new(1.0, CODIGO.borda));
                    topo = faixa.bottom();
                }
                if !editando {
                    let conteudo = Rect::from_min_max(
                        pos2(r.left() + RECUO_CODIGO * z, topo + RECUO_CODIGO * z),
                        pos2(r.right() - RECUO_CODIGO * z, r.bottom() - RECUO_CODIGO * z / 2.0),
                    );
                    let c = self.composicao(pintor, e, z);
                    self.pintar_conteudo(pintor, r, conteudo, &c, tinta(e), reduzido);
                }
            }
            TipoElemento::Imagem => self.desenhar_imagem(pintor, ctx, r, z, e),
            TipoElemento::Video => {
                desenhar_video(pintor, r, z, e, marcas.video.as_ref().filter(|(id, _, _)| *id == e.id).map(|(_, t, erro)| (t.as_str(), *erro)))
            }
            TipoElemento::Tarefa => desenhar_cartao_tarefa(pintor, r, z, e, tarefas(e.tarefa_ref).filter(|_| e.tarefa_ref != 0), marcas.em_cima == Some(e.id)),
            TipoElemento::Ligacao => {}
        }
        if marcas.limpo {
            return;
        }
        // O selo do agente, a marca de "novo" e o "não salvo": em px de tela.
        if e.do_agente() && !reduzido {
            let cor = match e.tipo {
                TipoElemento::Nota => tinta(e).secundaria,
                TipoElemento::Codigo => CODIGO.suave,
                _ => p.suave,
            };
            hexagono(pintor, r.right_top() + vec2(-8.0 - 6.0, 8.0 + 6.0), 6.0, cor);
        }
        if marcas.novos.is_some_and(|n| n.contains(&e.id)) {
            pintor.rect_stroke(r.expand(4.0), CornerRadius::same(tema::RAIO_CARTAO + 4), Stroke::new(1.5, p.ok), StrokeKind::Outside);
        }
        if marcas.nao_salvos.is_some_and(|n| n.contains(&e.id)) {
            pintor.circle(r.right_top(), 4.0, p.erro, Stroke::new(1.5, p.fundo));
        }
    }

    /// Pinta o conteúdo recortado ao item; se não coube, degradê e "⋯".
    fn pintar_conteudo(&self, pintor: &Painter, caixa: Rect, conteudo: Rect, c: &Composicao, t: Tinta, reduzido: bool) {
        let recorte = pintor.with_clip_rect(caixa.intersect(pintor.clip_rect()));
        let origem = conteudo.min.to_vec2();
        for peca in &c.pecas {
            match peca {
                Peca::Texto { pos, galeria, .. } => {
                    if conteudo.top() + pos.y > caixa.bottom() {
                        break;
                    }
                    recorte.galley(Pos2::ZERO + origem + *pos, galeria.clone(), t.texto);
                }
                Peca::Barra { rect, cor } => {
                    let r = rect.translate(origem);
                    if r.top() > caixa.bottom() {
                        break;
                    }
                    recorte.rect_filled(r, CornerRadius::same((r.height() / 2.0).min(255.0) as u8), *cor);
                }
                Peca::Fundo { rect, cor, raio } => {
                    recorte.rect_filled(rect.translate(origem), CornerRadius::same(*raio as u8), *cor);
                }
                Peca::Linha { a, b, cor } => {
                    recorte.line_segment([*a + origem, *b + origem], Stroke::new(1.0, *cor));
                }
                Peca::Caixa { rect, marcada, tinta, papel } => {
                    let r = rect.translate(origem);
                    let raio = CornerRadius::same((3.0 * r.width() / 12.0) as u8);
                    if *marcada {
                        recorte.rect_filled(r, raio, *tinta);
                        let visto = vec![
                            r.left_center() + vec2(r.width() * 0.22, 0.0),
                            r.center() + vec2(-r.width() * 0.06, r.height() * 0.22),
                            r.right_center() + vec2(-r.width() * 0.22, -r.height() * 0.22),
                        ];
                        recorte.add(Shape::line(visto, Stroke::new((r.width() / 7.0).max(1.0), *papel)));
                    } else {
                        recorte.rect_stroke(r, raio, Stroke::new((r.width() / 10.0).max(1.0), *tinta), StrokeKind::Inside);
                    }
                }
            }
        }
        // Não coube (a dona encolheu o cartão): degradê na base e "⋯".
        if conteudo.top() + c.altura > conteudo.bottom() + 1.0 && !reduzido {
            let altura = (24.0 * conteudo.height() / conteudo.height().max(1.0)).min(caixa.height() / 2.0);
            let base = Rect::from_min_max(pos2(caixa.left() + 1.0, caixa.bottom() - altura - 1.0), pos2(caixa.right() - 1.0, caixa.bottom() - 1.0));
            let mut malha = egui::Mesh::default();
            let transparente = Color32::from_rgba_unmultiplied(t.papel.r(), t.papel.g(), t.papel.b(), 0);
            malha.colored_vertex(base.left_top(), transparente);
            malha.colored_vertex(base.right_top(), transparente);
            malha.colored_vertex(base.right_bottom(), t.papel);
            malha.colored_vertex(base.left_bottom(), t.papel);
            malha.add_triangle(0, 1, 2);
            malha.add_triangle(0, 2, 3);
            pintor.add(Shape::mesh(malha));
            // "⋯" desenhado (a fonte não garante o símbolo).
            let centro = caixa.right_bottom() - vec2(10.0 + 7.0, 10.0);
            for dx in [-5.0, 0.0, 5.0] {
                pintor.circle_filled(centro + vec2(dx, 0.0), 1.6, t.secundaria);
            }
        }
    }

    fn desenhar_imagem(&mut self, pintor: &Painter, ctx: &egui::Context, r: Rect, z: f32, e: &Elemento) {
        let p = cores();
        let canto = CornerRadius::same(canto(tema::RAIO_CARTAO as f32, z) as u8);
        let miniatura = r.width() < 256.0;
        let cache = if miniatura { &mut self.miniaturas } else { &mut self.imagens };
        match cache.pedir(ctx, e.anexo_id) {
            Miniatura::Pronta(textura) => {
                let tamanho = textura.size_vec2();
                let escala = (r.width() / tamanho.x).max(r.height() / tamanho.y);
                let visivel = vec2(r.width() / (tamanho.x * escala), r.height() / (tamanho.y * escala));
                let uv = Rect::from_center_size(pos2(0.5, 0.5), visivel);
                pintor.add(egui::epaint::RectShape::filled(r, canto, Color32::WHITE).with_texture(textura.id(), uv));
            }
            Miniatura::Falhou => {
                pintor.rect_filled(r, canto, p.superficie);
                if r.width() > 120.0 {
                    pintor.text(r.center(), Align2::CENTER_CENTER, "imagem indisponível", FontId::proportional(12.5), p.suave);
                }
            }
            Miniatura::Carregando => tema::esqueleto(pintor, r, canto.nw),
        }
        pintor.rect_stroke(r, canto, Stroke::new(1.0, p.borda), StrokeKind::Inside);
        if !e.titulo.is_empty() && 12.0 * z >= MENOR_LETRA {
            let g = tema::cortar(pintor, &e.titulo, TextFormat::simple(FontId::proportional(12.0 * camera::zoom_do_texto(z)), p.suave), r.width(), 1, false);
            pintor.galley(pos2(r.left(), r.bottom() + 6.0 * z), g, p.suave);
        }
    }
}

/// O tamanho base da letra de um item (o parágrafo), para a regra das barras.
fn letra_base(e: &Elemento) -> f32 {
    match e.tipo {
        TipoElemento::Codigo => 12.5,
        _ => 14.0,
    }
}

/// O canto em px de tela: `(raio · z)` entre 2 e o raio da janela.
pub fn canto(raio: f32, z: f32) -> f32 {
    (raio * z).clamp(2.0, tema::RAIO_JANELA as f32)
}

pub fn caixa_quadro(e: &Elemento) -> Rect {
    Rect::from_min_size(pos2(e.x, e.y), vec2(e.largura, e.altura))
}

/// Folga do texto solto (sem papel nem recuo) na seleção e no clique, em
/// unidades do quadro: o contorno não encosta nas letras.
pub const FOLGA_TEXTO: f32 = 7.0;

/// A caixa de seleção e de clique do item: a do desenho e, no texto solto,
/// com a folga em volta (o texto continua desenhado no mesmo lugar).
pub fn caixa_de_toque(e: &Elemento) -> Rect {
    if e.tipo == TipoElemento::Texto { caixa_quadro(e).expand(FOLGA_TEXTO) } else { caixa_quadro(e) }
}

/// Hexágono vazado (a forma do logo), o selo do agente.
pub fn hexagono(pintor: &Painter, centro: Pos2, raio: f32, cor: Color32) {
    let pontos: Vec<Pos2> = (0..6)
        .map(|i| {
            let a = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 90.0);
            centro + vec2(a.cos() * raio, a.sin() * raio)
        })
        .collect();
    pintor.add(Shape::closed_line(pontos, Stroke::new(1.2, cor)));
}

/// A versão em barras: cada linha de texto vira uma barra arredondada da
/// largura dela; os títulos que ainda se leem (4 px ou mais) ficam texto.
fn reduzir(pintor: &Painter, base: &Composicao, fator: f32) -> Composicao {
    let mut pecas = Vec::new();
    for peca in &base.pecas {
        match peca {
            Peca::Texto { pos, galeria, letra, titulo } => {
                let letra_aqui = letra * fator;
                if *titulo && letra_aqui >= MENOR_TITULO {
                    // O título montado no tamanho de agora (uma linha, cortado).
                    let texto: String = galeria.rows.iter().flat_map(|r| r.glyphs.iter().map(|g| g.chr)).collect();
                    let cor = galeria.job.sections.first().map_or(cores().texto, |s| s.format.color);
                    let g = tema::cortar(
                        pintor,
                        &texto,
                        TextFormat::simple(forte(letra_aqui), cor),
                        galeria.size().x * fator + 4.0,
                        galeria.rows.len().max(1),
                        false,
                    );
                    pecas.push(Peca::Texto { pos: *pos * fator, galeria: g, letra: letra_aqui, titulo: true });
                    continue;
                }
                let cor = cor_da_galeria(galeria);
                for linha in &galeria.rows {
                    let r = linha.rect();
                    if r.width() < 1.0 {
                        continue;
                    }
                    let altura = r.height() * 0.55 * fator;
                    let topo = (pos.y + r.center().y) * fator - altura / 2.0;
                    pecas.push(Peca::Barra {
                        rect: Rect::from_min_size(pos2((pos.x + r.left()) * fator, topo), vec2((r.width() * fator).max(1.0), altura.max(1.0))),
                        cor,
                    });
                }
            }
            Peca::Barra { rect, cor } => pecas.push(Peca::Barra { rect: escalar(*rect, fator), cor: *cor }),
            Peca::Fundo { rect, cor, raio } => pecas.push(Peca::Fundo { rect: escalar(*rect, fator), cor: *cor, raio: raio * fator }),
            Peca::Linha { a, b, cor } => pecas.push(Peca::Linha { a: (a.to_vec2() * fator).to_pos2(), b: (b.to_vec2() * fator).to_pos2(), cor: *cor }),
            Peca::Caixa { rect, marcada, tinta, papel } => {
                pecas.push(Peca::Caixa { rect: escalar(*rect, fator), marcada: *marcada, tinta: *tinta, papel: *papel })
            }
        }
    }
    Composicao { pecas, altura: base.altura * fator }
}

/// A cor da barra de uma galeria: a do primeiro trecho, mais apagada.
fn cor_da_galeria(g: &Galley) -> Color32 {
    let cor = g.job.sections.first().map_or(cores().suave, |s| s.format.color);
    cor.gamma_multiply(0.35)
}

fn escalar(r: Rect, f: f32) -> Rect {
    Rect::from_min_max((r.min.to_vec2() * f).to_pos2(), (r.max.to_vec2() * f).to_pos2())
}

/// Monta o conteúdo do item no zoom do texto `zt` (px de tela).
pub fn compor(pintor: &Painter, e: &Elemento, zt: f32) -> Composicao {
    match e.tipo {
        TipoElemento::Codigo => compor_codigo(pintor, e, zt),
        TipoElemento::Nota | TipoElemento::Texto => {
            let recuo = if e.tipo == TipoElemento::Nota { RECUO * 2.0 } else { 0.0 };
            compor_markdown(pintor, &markdown::ler(&e.texto), tinta(e), ((e.largura - recuo) * zt).max(8.0), zt)
        }
        _ => Composicao::default(),
    }
}

fn compor_codigo(pintor: &Painter, e: &Elemento, zt: f32) -> Composicao {
    let mut pecas = Vec::new();
    let largura = ((e.largura - 2.0 * RECUO_CODIGO) * zt).max(8.0);
    let faixa = if e.titulo.is_empty() { 0.0 } else { FAIXA };
    let cabem = (((e.altura - faixa - RECUO_CODIGO * 1.5) / LINHA_CODIGO).floor().max(1.0)) as usize;
    let letra = 12.5 * zt;
    let mut y = 0.0;
    for linha in e.texto.lines().take(cabem) {
        let linha = linha.replace('\t', "    ");
        if !linha.is_empty() {
            let g = tema::cortar(pintor, &linha, TextFormat::simple(FontId::monospace(letra), CODIGO.texto), largura, 1, true);
            pecas.push(Peca::Texto { pos: vec2(0.0, y), galeria: g, letra, titulo: false });
        }
        y += LINHA_CODIGO * zt;
    }
    let total = e.texto.lines().count().max(1) as f32 * LINHA_CODIGO * zt;
    Composicao { pecas, altura: total }
}

/// O formato de um trecho no tamanho dado. O fundo do código na linha não
/// vai aqui (o do egui é reto e da altura da linha): vem de `fundos_do_codigo`.
fn formato(t: &Trecho, tamanho: f32, base_forte: bool, cor: Color32) -> TextFormat {
    let fonte = if t.codigo {
        // O código na linha fica um pouco menor que o texto em volta (12,5 para 14).
        FontId::monospace(tamanho * 12.5 / 14.0)
    } else if t.negrito || base_forte {
        forte(tamanho)
    } else {
        FontId::proportional(tamanho)
    };
    let mut f = TextFormat::simple(fonte, cor);
    f.italics = t.italico;
    f
}

/// A linha de base de uma letra sozinha e a altura da linha dela (px).
fn base_e_altura(pintor: &Painter, fonte: FontId) -> (f32, f32) {
    let g = pintor.layout_no_wrap("x".into(), fonte, Color32::WHITE);
    let base = g.rows.first().and_then(|r| r.glyphs.first()).map_or(0.0, |l| l.pos.y);
    (base, g.size().y)
}

/// Respiro do código na linha de cada lado (px no zoom 100%).
const RESPIRO_CODIGO: f32 = 3.0;

#[allow(clippy::too_many_arguments)]
fn job(pintor: &Painter, trechos: &[Trecho], tamanho: f32, forte_base: bool, cor: Color32, zt: f32, largura: f32, entrelinha: Option<f32>) -> LayoutJob {
    let mut j = LayoutJob { wrap: TextWrapping { max_width: largura.max(8.0), ..Default::default() }, ..Default::default() };
    if trechos.is_empty() {
        j.append(" ", 0.0, TextFormat::simple(FontId::proportional(tamanho), cor));
    }
    // O código na linha fica na linha de base do texto: a altura da linha
    // dele é a do texto menos a diferença das linhas de base (o egui alinha
    // as letras de uma linha por baixo).
    let altura_do_codigo = trechos.iter().any(|t| t.codigo).then(|| {
        let (base_texto, altura_texto) = base_e_altura(pintor, FontId::proportional(tamanho));
        let (base_codigo, _) = base_e_altura(pintor, FontId::monospace(tamanho * 12.5 / 14.0));
        entrelinha.unwrap_or(altura_texto) - (base_texto - base_codigo)
    });
    let mut depois_do_codigo = false;
    for t in trechos {
        let mut f = formato(t, tamanho, forte_base, cor);
        f.line_height = if t.codigo { altura_do_codigo } else { entrelinha };
        // O respiro: o fundo do código não encosta na palavra vizinha.
        let respiro = if t.codigo || depois_do_codigo { RESPIRO_CODIGO * zt } else { 0.0 };
        j.append(&t.texto, respiro, f);
        depois_do_codigo = t.codigo;
    }
    j
}

/// Os fundos do código na linha de uma galeria: um retângulo de canto 4 por
/// trecho e por linha quebrada, com o respiro dos lados e a altura da letra
/// do código (não a da linha). `origem`: onde a galeria é desenhada.
fn fundos_do_codigo(g: &Galley, origem: Vec2, cor: Color32, zt: f32) -> Vec<Peca> {
    let job = &g.job;
    if !job.sections.iter().any(|s| s.format.font_id.family == egui::FontFamily::Monospace) {
        return Vec::new();
    }
    // De que trecho é cada caractere (as letras da galeria vêm na mesma ordem).
    let mut trecho_do_caractere = Vec::with_capacity(job.text.len());
    for (i, s) in job.sections.iter().enumerate() {
        trecho_do_caractere.extend(job.text[s.byte_range.start.0..s.byte_range.end.0].chars().map(|_| i));
    }
    let letras: usize = g.rows.iter().map(|r| r.glyphs.len() + usize::from(r.ends_with_newline)).sum();
    if letras != trecho_do_caractere.len() {
        return Vec::new();
    }
    let respiro = RESPIRO_CODIGO * zt;
    let raio = (4.0 * zt).max(2.0);
    let mut pecas = Vec::new();
    let mut n = 0;
    for linha in &g.rows {
        let mut aberto: Option<Rect> = None;
        for letra in &linha.glyphs {
            let codigo = job.sections[trecho_do_caractere[n]].format.font_id.family == egui::FontFamily::Monospace;
            n += 1;
            if codigo {
                let r = Rect::from_min_size(pos2(letra.pos.x, letra.pos.y - letra.font_ascent), vec2(letra.advance_width, letra.font_height))
                    .translate(linha.pos.to_vec2() + origem);
                aberto = Some(aberto.map_or(r, |a| a.union(r)));
            } else if let Some(a) = aberto.take() {
                pecas.push(Peca::Fundo { rect: a.expand2(vec2(respiro, zt)), cor, raio });
            }
        }
        if let Some(a) = aberto.take() {
            pecas.push(Peca::Fundo { rect: a.expand2(vec2(respiro, zt)), cor, raio });
        }
        n += usize::from(linha.ends_with_newline);
    }
    pecas
}

/// A galeria do texto e, antes dela, os fundos do código na linha.
fn texto_com_codigo(pecas: &mut Vec<Peca>, g: Arc<Galley>, pos: Vec2, letra: f32, titulo: bool, t: &Tinta, zt: f32) {
    pecas.extend(fundos_do_codigo(&g, pos, t.recuo, zt));
    pecas.push(Peca::Texto { pos, galeria: g, letra, titulo });
}

/// Monta os blocos de markdown numa coluna de `largura` px.
pub fn compor_markdown(pintor: &Painter, blocos: &[Bloco], t: Tinta, largura: f32, zt: f32) -> Composicao {
    let mut pecas = Vec::new();
    let mut y = 0.0;
    let px = |v: f32| v * zt;
    let mut primeiro = true;
    for bloco in blocos {
        match bloco {
            Bloco::Titulo(nivel, trechos) => {
                let (tamanho, antes, depois) = match nivel {
                    1 => (20.0, 10.0, 6.0),
                    2 => (17.0, 8.0, 4.0),
                    _ => (15.0, 6.0, 2.0),
                };
                if !primeiro {
                    y += px(antes);
                }
                let g = pintor.layout_job(job(pintor, trechos, px(tamanho), true, t.texto, zt, largura, None));
                let altura = g.size().y;
                texto_com_codigo(&mut pecas, g, vec2(0.0, y), px(tamanho), *nivel <= 2, &t, zt);
                y += altura + px(depois);
            }
            Bloco::Paragrafo(linhas) => {
                for linha in linhas {
                    let g = pintor.layout_job(job(pintor, linha, px(14.0), false, t.texto, zt, largura, Some(px(14.0 * 1.35))));
                    let altura = g.size().y;
                    texto_com_codigo(&mut pecas, g, vec2(0.0, y), px(14.0), false, &t, zt);
                    y += altura;
                }
                y += px(6.0);
            }
            Bloco::Item { nivel, marcador, trechos } => {
                let x = px(18.0) * *nivel as f32;
                let texto_x = x + px(18.0);
                let cor = if matches!(marcador, Marcador::Caixa(true)) { t.secundaria } else { t.texto };
                let g = pintor.layout_job(job(pintor, trechos, px(14.0), false, cor, zt, largura - texto_x, Some(px(14.0 * 1.35))));
                let primeira = g.rows.first().map_or(px(19.0), |r| r.rect().height());
                match marcador {
                    Marcador::Ponto => {
                        let m = pintor.layout_no_wrap("•".into(), FontId::proportional(px(14.0)), t.secundaria);
                        pecas.push(Peca::Texto { pos: vec2(x + px(3.0), y), galeria: m, letra: px(14.0), titulo: false });
                    }
                    Marcador::Numero(n) => {
                        let m = pintor.layout_no_wrap(format!("{n}."), FontId::proportional(px(13.0)), t.secundaria);
                        pecas.push(Peca::Texto { pos: vec2(x, y + px(1.0)), galeria: m, letra: px(13.0), titulo: false });
                    }
                    Marcador::Caixa(marcada) => {
                        let lado = px(12.0);
                        let caixa = Rect::from_min_size(pos2(x, y + (primeira - lado) / 2.0), vec2(lado, lado));
                        pecas.push(Peca::Caixa { rect: caixa, marcada: *marcada, tinta: t.texto, papel: t.papel });
                    }
                }
                let altura = g.size().y;
                texto_com_codigo(&mut pecas, g, vec2(texto_x, y), px(14.0), false, &t, zt);
                y += altura + px(2.0);
            }
            Bloco::Codigo(texto) => {
                y += if primeiro { 0.0 } else { px(4.0) };
                let mut j = LayoutJob {
                    wrap: TextWrapping { max_width: largura - px(16.0), max_rows: 200, break_anywhere: true, ..Default::default() },
                    ..Default::default()
                };
                j.append(if texto.is_empty() { " " } else { texto }, 0.0, TextFormat::simple(FontId::monospace(px(12.5)), t.texto));
                let g = pintor.layout_job(j);
                let caixa = Rect::from_min_size(pos2(0.0, y), vec2(largura, g.size().y + px(12.0)));
                pecas.push(Peca::Fundo { rect: caixa, cor: t.recuo, raio: canto(tema::RAIO_ETIQUETA as f32, zt) });
                pecas.push(Peca::Texto { pos: vec2(px(8.0), y + px(6.0)), galeria: g, letra: px(12.5), titulo: false });
                y += caixa.height() + px(6.0);
            }
            Bloco::Tabela { alinhamentos, cabecalho, linhas } => {
                y += if primeiro { 0.0 } else { px(4.0) };
                y = compor_tabela(pintor, &mut pecas, y, alinhamentos, cabecalho.as_ref(), linhas, t, largura, zt) + px(6.0);
            }
            Bloco::Vazio => y += px(8.0),
        }
        primeiro = false;
    }
    Composicao { pecas, altura: y }
}

#[allow(clippy::too_many_arguments)]
fn compor_tabela(
    pintor: &Painter,
    pecas: &mut Vec<Peca>,
    topo: f32,
    alinhamentos: &[Alinhamento],
    cabecalho: Option<&Vec<Vec<Trecho>>>,
    linhas: &[Vec<Vec<Trecho>>],
    t: Tinta,
    largura: f32,
    zt: f32,
) -> f32 {
    let px = |v: f32| v * zt;
    let colunas = alinhamentos.len().max(1);
    let (rx, ry) = (px(6.0), px(4.0));
    let todas: Vec<(bool, &Vec<Vec<Trecho>>)> = cabecalho.map(|c| (true, c)).into_iter().chain(linhas.iter().map(|l| (false, l))).collect();
    // Largura de cada coluna: a maior célula; se não couber, todas encolhem na mesma proporção.
    let mut larguras = vec![px(24.0); colunas];
    for (forte_linha, linha) in &todas {
        for (i, celula) in linha.iter().enumerate().take(colunas) {
            let g = pintor.layout_job(job(pintor, celula, px(13.0), *forte_linha, t.texto, zt, f32::INFINITY, None));
            larguras[i] = larguras[i].max(g.size().x + 2.0 * rx);
        }
    }
    let total: f32 = larguras.iter().sum();
    if total > largura {
        let f = largura / total;
        for l in &mut larguras {
            *l *= f;
        }
    }
    let largura_tabela = larguras.iter().sum::<f32>();
    let mut y = topo;
    let altura_linha = pintor.layout_no_wrap("Ág".into(), FontId::proportional(px(13.0)), t.texto).size().y + 2.0 * ry;
    for (n, (forte_linha, linha)) in todas.iter().enumerate() {
        if *forte_linha {
            pecas.push(Peca::Fundo { rect: Rect::from_min_size(pos2(0.0, y), vec2(largura_tabela, altura_linha)), cor: t.recuo, raio: 0.0 });
        }
        let mut x = 0.0;
        for (i, l) in larguras.iter().enumerate() {
            if let Some(celula) = linha.get(i) {
                let texto = markdown::texto_simples(celula);
                if !texto.is_empty() {
                    let fonte = if *forte_linha { forte(px(13.0)) } else { FontId::proportional(px(13.0)) };
                    let g = tema::cortar(pintor, &texto, TextFormat::simple(fonte, t.texto), (l - 2.0 * rx).max(4.0), 1, true);
                    let dx = match alinhamentos.get(i).copied().unwrap_or_default() {
                        Alinhamento::Esquerda => rx,
                        Alinhamento::Centro => (l - g.size().x) / 2.0,
                        Alinhamento::Direita => l - rx - g.size().x,
                    };
                    pecas.push(Peca::Texto { pos: vec2(x + dx, y + ry), galeria: g, letra: px(13.0), titulo: false });
                }
            }
            x += l;
        }
        y += altura_linha;
        if n + 1 < todas.len() {
            pecas.push(Peca::Linha { a: pos2(0.0, y), b: pos2(largura_tabela, y), cor: t.linha });
        }
    }
    // Moldura e divisões das colunas.
    let mut x = 0.0;
    for l in &larguras[..larguras.len() - 1] {
        x += l;
        pecas.push(Peca::Linha { a: pos2(x, topo), b: pos2(x, y), cor: t.linha });
    }
    for (a, b) in [
        (pos2(0.0, topo), pos2(largura_tabela, topo)),
        (pos2(0.0, y), pos2(largura_tabela, y)),
        (pos2(0.0, topo), pos2(0.0, y)),
        (pos2(largura_tabela, topo), pos2(largura_tabela, y)),
    ] {
        pecas.push(Peca::Linha { a, b, cor: t.linha });
    }
    y
}

/// O centro e o raio do play do cartão de vídeo (na tela), para o desenho e o clique.
pub fn play_do_video(r: Rect, z: f32) -> (Pos2, f32) {
    let faixa = 36.0 * z;
    (pos2(r.center().x, r.top() + (r.height() - faixa) / 2.0), (22.0 * z).min(r.height() / 4.0))
}

/// O ponto `p` (tela) cai no play do cartão de vídeo em `r`.
pub fn no_play(r: Rect, z: f32, p: Pos2) -> bool {
    let (centro, raio) = play_do_video(r, z);
    (centro - p).length() <= raio.max(12.0)
}

/// O cartão de vídeo: 16:9, o play e a faixa com o título (ou o nome) e,
/// embaixo, o nome e o tamanho.
fn desenhar_video(pintor: &Painter, r: Rect, z: f32, e: &Elemento, estado: Option<(&str, bool)>) {
    let raio = CornerRadius::same(canto(tema::RAIO_CARTAO as f32, z) as u8);
    pintor.rect(r, raio, ESCURO.superficie_alta, Stroke::new(1.0, ESCURO.realce), StrokeKind::Inside);
    let faixa = 36.0 * z;
    let (centro, raio_play) = play_do_video(r, z);
    pintor.circle_filled(centro, raio_play, Color32::from_white_alpha(230));
    tema::play(pintor, centro, raio_play * 0.4, ESCURO.fundo);
    if 12.0 * z < MENOR_LETRA {
        return;
    }
    let zt = camera::zoom_do_texto(z);
    let info = e.anexo.clone().unwrap_or_default();
    let nome = if info.nome.is_empty() { "vídeo".to_string() } else { info.nome };
    let titulo = e.titulo.trim();
    let y = r.bottom() - faixa;
    let principal = if titulo.is_empty() { nome.as_str() } else { titulo };
    let g = tema::cortar(pintor, principal, TextFormat::simple(forte(13.0 * zt), ESCURO.texto), r.width() - 24.0 * z, 1, false);
    pintor.galley(pos2(r.left() + 12.0 * z, y + 2.0 * z), g, ESCURO.texto);
    let (linha, cor) = match estado {
        Some((texto, true)) => (texto.to_string(), ESCURO.erro),
        Some((texto, false)) => (texto.to_string(), ESCURO.suave),
        None if titulo.is_empty() => (tamanho(info.bytes), ESCURO.suave),
        None => (format!("{nome} · {}", tamanho(info.bytes)), ESCURO.suave),
    };
    let g = tema::cortar(pintor, &linha, TextFormat::simple(FontId::proportional(12.0 * zt), cor), r.width() - 24.0 * z, 1, false);
    pintor.galley(pos2(r.left() + 12.0 * z, y + 18.0 * z), g, cor);
}

/// O cartão de uma tarefa (o do kanban, compacto): título, número e projeto,
/// a etiqueta da coluna e o ponto do agente, lidos ao vivo do quadro.
fn desenhar_cartao_tarefa(pintor: &Painter, r: Rect, z: f32, e: &Elemento, info: Option<InfoTarefa>, em_cima: bool) {
    let p = cores();
    let raio = CornerRadius::same(canto(tema::RAIO_CARTAO as f32, z) as u8);
    let removida = info.is_none();
    let fundo = if removida { p.superficie } else { p.superficie_alta };
    let borda = if em_cima { p.destaque.gamma_multiply(0.55) } else { p.borda };
    pintor.rect(r, raio, fundo, Stroke::new(1.0, borda), StrokeKind::Inside);
    let info = info.unwrap_or_default();
    if info.erro {
        let faixa = Rect::from_min_size(r.min + vec2(0.0, 6.0 * z), vec2(3.0, (r.height() - 12.0 * z).max(1.0)));
        pintor.rect_filled(faixa, CornerRadius::same(2), p.erro);
    }
    if 13.5 * z < MENOR_LETRA {
        pintor.rect_filled(
            Rect::from_min_size(r.min + vec2(14.0, 14.0) * z, vec2(r.width() * 0.6, 8.0 * z)),
            CornerRadius::same(2),
            p.suave.gamma_multiply(0.35),
        );
        return;
    }
    let zt = camera::zoom_do_texto(z);
    let recuo = 14.0 * z;
    let largura = r.width() - 2.0 * recuo;
    let (titulo, cor_titulo) = if removida { ("Tarefa removida".to_string(), p.suave) } else { (info.titulo.clone(), p.texto) };
    let g = tema::cortar(pintor, &titulo, TextFormat::simple(forte(13.5 * zt), cor_titulo), largura - 14.0 * z, 2, false);
    let altura_titulo = g.size().y;
    pintor.galley(r.min + vec2(recuo, recuo), g, cor_titulo);
    if !removida {
        let linha = if info.projeto.is_empty() { format!("#{}", e.tarefa_ref) } else { format!("#{} · {}", e.tarefa_ref, info.projeto) };
        let g = tema::cortar(pintor, &linha, TextFormat::simple(FontId::proportional(11.5 * zt), p.suave), largura, 1, false);
        pintor.galley(r.min + vec2(recuo, recuo + altura_titulo + 2.0 * z), g, p.suave);
    }
    // Rodapé: a pílula do estado e o ponto do agente.
    let (cor, rotulo, marca) = tema::estado_da_tarefa(&info.coluna, info.erro, removida);
    let fonte = forte(12.0 * zt);
    let gr = pintor.layout_no_wrap(rotulo.to_string(), fonte, p.texto);
    let altura = 22.0 * z;
    let pilula = Rect::from_min_size(pos2(r.left() + recuo, r.bottom() - recuo - altura + 4.0 * z), vec2(gr.size().x + 30.0 * z, altura));
    pintor.rect_filled(pilula, CornerRadius::same((altura / 2.0) as u8), tema::fundo_tingido(p, cor, tema::claro()));
    tema::marca(pintor, pos2(pilula.left() + 13.5 * z, pilula.center().y), 3.5 * z, cor, marca);
    pintor.galley(pos2(pilula.left() + 22.0 * z, pilula.center().y - gr.size().y / 2.0), gr, p.texto);
    if let Some(estado) = info.estado {
        tema::ponto(pintor, pos2(r.right() - recuo - 4.0 * z, pilula.center().y), 3.5 * z, estado);
    }
    if em_cima {
        tema::externo(pintor, r.right_top() + vec2(-15.0, 15.0), p.suave);
    }
}

// Ligações

/// As pontas e os controles da curva de uma ligação, entre as bordas mais
/// próximas dos dois itens (em coordenadas do quadro).
pub fn curva(de: Rect, para: Rect) -> [Pos2; 4] {
    let d = para.center() - de.center();
    let horizontal = d.x.abs() / ((de.width() + para.width()) / 2.0).max(1.0) >= d.y.abs() / ((de.height() + para.height()) / 2.0).max(1.0);
    if horizontal {
        let (a, b) = if d.x >= 0.0 { (de.right_center(), para.left_center()) } else { (de.left_center(), para.right_center()) };
        let sinal = if d.x >= 0.0 { 1.0 } else { -1.0 };
        let puxar = ((b.x - a.x).abs() * 0.45).max(30.0) * sinal;
        [a, a + vec2(puxar, 0.0), b - vec2(puxar, 0.0), b]
    } else {
        let (a, b) = if d.y >= 0.0 { (de.center_bottom(), para.center_top()) } else { (de.center_top(), para.center_bottom()) };
        let sinal = if d.y >= 0.0 { 1.0 } else { -1.0 };
        let puxar = ((b.y - a.y).abs() * 0.45).max(30.0) * sinal;
        [a, a + vec2(0.0, puxar), b - vec2(0.0, puxar), b]
    }
}

/// Os pontos da curva na tela (achatada), para desenhar e para o clique.
pub fn pontos_na_tela(area: Rect, camera: &Camera, controles: [Pos2; 4]) -> Vec<Pos2> {
    let tela = controles.map(|p| camera.para_tela(area, p));
    CubicBezierShape::from_points_stroke(tela, false, Color32::TRANSPARENT, Stroke::NONE).flatten(Some(0.5))
}

/// O ponto do meio da curva (onde vai o rótulo).
pub fn meio(pontos: &[Pos2]) -> Pos2 {
    pontos.get(pontos.len() / 2).copied().unwrap_or(Pos2::ZERO)
}

/// Distância de um ponto à linha de pontos.
pub fn distancia(pontos: &[Pos2], p: Pos2) -> f32 {
    pontos
        .windows(2)
        .map(|s| {
            let (a, b) = (s[0], s[1]);
            let ab = b - a;
            let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
            (a + ab * t - p).length()
        })
        .fold(f32::INFINITY, f32::min)
}

/// A seta encosta na borda; o tracejado para antes dela. Devolve a ponta,
/// a base da seta e a direção.
fn fim_da_seta(pontos: &[Pos2]) -> (Pos2, Pos2, Vec2) {
    let fim = pontos[pontos.len() - 1];
    let antes = pontos.iter().rev().find(|p| (fim - **p).length() > 9.0).copied().unwrap_or(pontos[0]);
    let direcao = (fim - antes).normalized();
    (fim, fim - direcao * 10.0, direcao)
}

/// Só o tracejado, até a base da seta.
fn tracejado(pintor: &Painter, pontos: &[Pos2], cor: Color32, grossura: f32) {
    if pontos.len() < 2 {
        return;
    }
    let (fim, base, _) = fim_da_seta(pontos);
    let mut caminho: Vec<Pos2> = pontos.iter().copied().take_while(|p| (*p - fim).length() > 10.0 || *p == pontos[0]).collect();
    caminho.push(base);
    for forma in Shape::dashed_line(&caminho, Stroke::new(grossura, cor), 6.0, 5.0) {
        pintor.add(forma);
    }
}

/// Só a ponta da seta.
fn seta(pintor: &Painter, pontos: &[Pos2], cor: Color32) {
    if pontos.len() < 2 {
        return;
    }
    let (fim, base, direcao) = fim_da_seta(pontos);
    let normal = vec2(-direcao.y, direcao.x);
    pintor.add(Shape::convex_polygon(vec![fim, base + normal * 4.0, base - normal * 4.0], cor, Stroke::NONE));
}

/// Desenha a linha tracejada com a ponta de seta no destino.
pub fn tracejado_com_seta(pintor: &Painter, pontos: &[Pos2], cor: Color32, grossura: f32) {
    tracejado(pintor, pontos, cor, grossura);
    seta(pintor, pontos, cor);
}

/// Uma ligação pronta para desenhar em duas vezes: a linha antes dos
/// cartões, a ponta e o rótulo depois.
struct Ligacao<'a> {
    e: &'a Elemento,
    pontos: Vec<Pos2>,
    cor: Color32,
    grossura: f32,
    /// No palco, longe do cartão atual: tudo mais fraco, o rótulo também.
    apagada: bool,
}

impl<'a> Ligacao<'a> {
    fn nova(area: Rect, camera: &Camera, e: &'a Elemento, de: &Elemento, para: &Elemento, destaque: bool, apagada: bool) -> Ligacao<'a> {
        let p = cores();
        let pontos = pontos_na_tela(area, camera, curva(caixa_quadro(de), caixa_quadro(para)));
        let (cor, grossura) = if destaque { (p.destaque, 2.0) } else { (p.suave, (1.5 * camera.zoom).clamp(1.0, 2.5)) };
        let cor = if apagada { cor.gamma_multiply(0.35) } else { cor };
        Ligacao { e, pontos, cor, grossura, apagada }
    }

    fn linha(&self, pintor: &Painter) {
        tracejado(pintor, &self.pontos, self.cor, self.grossura);
    }

    fn ponta_e_rotulo(&self, pintor: &Painter, zoom: f32, com_rotulo: bool) {
        seta(pintor, &self.pontos, self.cor);
        if self.e.texto.is_empty() || !com_rotulo || 12.0 * zoom < MENOR_LETRA {
            return;
        }
        let p = cores();
        let fraco = |c: Color32| if self.apagada { c.gamma_multiply(0.35) } else { c };
        let letra = (12.0 * camera::zoom_do_texto(zoom)).max(10.0);
        let g = tema::cortar(pintor, &self.e.texto, TextFormat::simple(FontId::proportional(letra), fraco(p.texto)), 240.0 * zoom.max(0.5), 1, false);
        let caixa = Rect::from_center_size(meio(&self.pontos), g.size() + vec2(16.0, 6.0));
        // O fundo do rótulo apagado é misturado ao fundo (não transparente: a linha não aparece por trás).
        let papel = if self.apagada { tema::misturar(p.fundo, p.superficie_alta, 0.35) } else { p.superficie_alta };
        pintor.rect(caixa, CornerRadius::same(tema::RAIO_ETIQUETA), papel, Stroke::new(1.0, fraco(p.borda)), StrokeKind::Inside);
        pintor.galley(caixa.center() - g.size() / 2.0, g, fraco(p.texto));
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn nota(id: i64, texto: &str) -> Elemento {
        Elemento { id, tipo: TipoElemento::Nota, largura: 240.0, altura: 200.0, texto: texto.into(), cor: "amarelo".into(), rev: 1, ..Default::default() }
    }

    fn com_pintor(f: impl FnOnce(&Painter)) {
        let ctx = egui::Context::default();
        tema::instalar(&ctx);
        let mut f = Some(f);
        let mut saida = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(f) = f.take() {
                f(ui.painter())
            }
        });
        saida.textures_delta.clear();
    }

    #[test]
    fn cache_por_revisao_e_zoom() {
        com_pintor(|pintor| {
            let mut d = Desenho::novo("teste");
            let mut e = nota(1, "# Título\n- um\n- dois");
            let a = d.composicao(pintor, &e, 1.0);
            let b = d.composicao(pintor, &e, 1.0);
            assert!(Arc::ptr_eq(&a, &b), "a mesma revisão no mesmo zoom vem do cache");
            e.rev = 2;
            e.texto.push_str("\n- três");
            let c = d.composicao(pintor, &e, 1.0);
            assert!(!Arc::ptr_eq(&a, &c) && c.altura > a.altura);
            let _ = d.composicao(pintor, &e, 2.0);
            assert_eq!(d.guardados(), 3);
        });
    }

    #[test]
    fn atlas_novo_limpa_o_cache() {
        // Trocar de tema muda as opções do texto: o egui refaz o atlas das
        // letras, e as galerias guardadas apontariam para o atlas velho.
        let ctx = egui::Context::default();
        tema::instalar(&ctx);
        let mut d = Desenho::novo("teste");
        let e = nota(1, "# Título\ntexto");
        let quadro = |d: &mut Desenho| {
            let mut guardada = None;
            let mut saida = ctx.run_ui(egui::RawInput::default(), |ui| guardada = Some(d.composicao(ui.painter(), &e, 1.0)));
            // Como em `com_pintor`: a saída não vai para nenhum pintor de
            // verdade, e o egui reclama (no debug) de entregas de atlas largadas.
            saida.textures_delta.clear();
            guardada.unwrap()
        };
        let a = quadro(&mut d);
        let b = quadro(&mut d);
        assert!(Arc::ptr_eq(&a, &b), "sem troca, vem do cache");
        let escuro = ctx.global_style().visuals.dark_mode;
        ctx.set_theme(if escuro { egui::Theme::Light } else { egui::Theme::Dark });
        let _ = quadro(&mut d);
        let c = quadro(&mut d);
        assert!(!Arc::ptr_eq(&a, &c), "depois da troca, montada de novo");
    }

    #[test]
    fn letra_pequena_vira_barra_e_titulo_continua_texto() {
        com_pintor(|pintor| {
            let mut d = Desenho::novo("teste");
            let e = nota(1, "# Fluxo\nparágrafo que vira barra no zoom baixo");
            let c = d.composicao(pintor, &e, 0.25);
            assert!(c.pecas.iter().any(|p| matches!(p, Peca::Barra { .. })));
            assert!(c.pecas.iter().any(|p| matches!(p, Peca::Texto { titulo: true, .. })));
            // A 10%, nem o título se lê.
            let c = d.composicao(pintor, &e, 0.1);
            assert!(c.pecas.iter().all(|p| !matches!(p, Peca::Texto { .. })));
        });
    }

    #[test]
    fn codigo_na_linha_tem_fundo_arredondado_na_linha_de_base() {
        com_pintor(|pintor| {
            let mut d = Desenho::novo("teste");
            let c = d.composicao(pintor, &nota(1, "exportar `CSV` agora"), 1.0);
            let fundos: Vec<(Rect, f32)> =
                c.pecas.iter().filter_map(|p| if let Peca::Fundo { rect, raio, .. } = p { Some((*rect, *raio)) } else { None }).collect();
            assert_eq!(fundos.len(), 1, "um fundo para o trecho de código");
            let (fundo, raio) = fundos[0];
            assert!(raio >= 4.0);
            let Some(Peca::Texto { galeria, pos, .. }) = c.pecas.iter().find(|p| matches!(p, Peca::Texto { .. })) else { panic!() };
            let letras = &galeria.rows[0].glyphs;
            // "exportar " tem 9 letras; o C de CSV é a décima.
            let (texto, codigo) = (&letras[0], &letras[9]);
            assert_eq!(codigo.chr, 'C');
            assert!((texto.pos.y - codigo.pos.y).abs() <= 1.0, "linha de base: {} e {}", texto.pos.y, codigo.pos.y);
            // O fundo começa depois do espaço (respiro) e não passa da altura da linha.
            assert!(fundo.left() + pos.x > letras[7].pos.x + letras[7].advance_width);
            assert!(fundo.height() <= galeria.rows[0].rect().height() + 0.5, "{} > {}", fundo.height(), galeria.rows[0].rect().height());
        });
    }

    #[test]
    fn altura_do_conteudo_cresce_com_o_texto() {
        com_pintor(|pintor| {
            let mut d = Desenho::novo("teste");
            let curta = d.altura_do_conteudo(pintor, &nota(1, "uma linha"));
            let longa = d.altura_do_conteudo(pintor, &nota(2, &"linha\n".repeat(15)));
            assert!(curta < 80.0 && longa > curta * 4.0, "{curta} {longa}");
            let tabela = d.altura_do_conteudo(pintor, &nota(3, "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |"));
            assert!(tabela > curta);
        });
    }

    #[test]
    fn ligacao_sai_das_bordas_mais_proximas() {
        let a = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        let b = Rect::from_min_size(pos2(300.0, 20.0), vec2(100.0, 100.0));
        let c = curva(a, b);
        assert_eq!((c[0], c[3]), (a.right_center(), b.left_center()));
        let c = curva(b, a);
        assert_eq!((c[0], c[3]), (b.left_center(), a.right_center()));
        let embaixo = Rect::from_min_size(pos2(0.0, 400.0), vec2(100.0, 100.0));
        let c = curva(a, embaixo);
        assert_eq!((c[0], c[3]), (a.center_bottom(), embaixo.center_top()));
        let camera = Camera { origem: Pos2::ZERO, zoom: 1.0 };
        let area = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let pontos = pontos_na_tela(area, &camera, curva(a, b));
        assert!(distancia(&pontos, meio(&pontos)) < 0.01);
        assert!(distancia(&pontos, pos2(200.0, 300.0)) > 100.0);
    }
}

#[cfg(test)]
mod medicao {
    use super::*;

    /// 500 itens (notas com markdown, código, texto e ligações) desenhados num
    /// contexto sem janela: imprime o tempo médio por quadro. `cargo test
    /// --release -- --ignored --nocapture quinhentos`.
    #[test]
    #[ignore]
    fn quinhentos_itens() {
        let mut elementos = Vec::new();
        for i in 0..400i64 {
            let tipo = [TipoElemento::Nota, TipoElemento::Codigo, TipoElemento::Texto, TipoElemento::Nota][i as usize % 4];
            let texto = match tipo {
                TipoElemento::Codigo => format!("$ comando {i}\nsaida 1\nsaida 2"),
                TipoElemento::Texto => format!("Texto solto {i}"),
                _ => format!("# Item {i}\n- ponto **um**\n- ponto dois\n| a | b |\n|--|--|\n| {i} | x |"),
            };
            elementos.push(Elemento {
                id: i + 1,
                tipo,
                x: (i % 25) as f32 * 300.0,
                y: (i / 25) as f32 * 260.0,
                largura: 240.0,
                altura: 200.0,
                z: i,
                cor: "amarelo".into(),
                texto,
                rev: 1,
                ..Default::default()
            });
        }
        for i in 0..100i64 {
            elementos.push(Elemento {
                id: 1000 + i,
                tipo: TipoElemento::Ligacao,
                de: i * 4 + 1,
                para: i * 4 + 2,
                texto: "liga".into(),
                z: 1000 + i,
                ..Default::default()
            });
        }
        let ctx = egui::Context::default();
        tema::instalar(&ctx);
        let mut desenho = Desenho::novo("medicao");
        let area = Rect::from_min_size(pos2(236.0, 60.0), vec2(1364.0, 840.0));
        let nada = |_: i64| None;
        for (zoom, nome) in [(0.67, "67%"), (0.25, "25%"), (1.0, "100%")] {
            let mut camera = Camera { origem: pos2(0.0, 0.0), zoom };
            let mut tempos = Vec::new();
            for quadro in 0..60 {
                camera.origem.x = quadro as f32 * 7.0;
                let inicio = std::time::Instant::now();
                let mut saida =
                    ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))), ..Default::default() }, |ui| {
                        desenho.desenhar(ui, area, &camera, &elementos, &nada, &Marcas::default());
                    });
                let formas = ctx.tessellate(std::mem::take(&mut saida.shapes), 1.0);
                saida.textures_delta.clear();
                tempos.push(inicio.elapsed().as_secs_f64() * 1000.0);
                std::hint::black_box(formas);
            }
            tempos.sort_by(f64::total_cmp);
            println!("zoom {nome}: mediana {:.2} ms, p90 {:.2} ms (layout e tesselação, sem GPU)", tempos[30], tempos[54]);
        }
    }
}
