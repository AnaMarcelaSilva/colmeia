//! Gaveta de arquivos do painel da tarefa: a árvore da pasta (só leitura,
//! um nível por pedido, em cache por tarefa) e a pré-visualização de texto e
//! imagem. Fica por cima do terminal, sem mudar o tamanho dele: um terminal
//! que muda de tamanho faz o Claude Code redesenhar a tela inteira.
//!
//! Tudo é buscado no núcleo, que confina os caminhos à pasta da tarefa. A
//! gaveta só redesenha com entrada ou resposta: nada roda por tempo.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Key, Modifiers, Rect, Sense, Stroke, StrokeKind, TextureHandle, pos2, vec2};

use crate::api;
use crate::registro;
use crate::sistema;
use crate::tema::{self, cores, forte};

const ALTURA_LINHA: f32 = 26.0;
const LARGURA_ARVORE: f32 = 320.0;
const ALTURA_LINHA_TEXTO: f32 = 18.0;

enum EstadoPasta {
    Carregando,
    Pronta(api::ListaArquivos),
    Erro(String),
}

enum EstadoPrevia {
    Carregando,
    Pronta { previa: api::Previa, linhas: Vec<String>, imagem: Option<Result<TextureHandle, String>> },
    Erro(String),
}

struct PreviaAberta {
    caminho: String,
    estado: EstadoPrevia,
}

enum Resposta {
    Lista(String, Result<api::ListaArquivos, String>),
    Previa(String, Result<api::Previa, String>),
    Imagem(String, Result<egui::ColorImage, String>),
}

/// O que a gaveta pede ao painel.
pub enum PedidoGaveta {
    Fechar,
    AbrirPasta,
    AbrirNoEditor(String),
    AbrirNoSistema(String),
    Citar(String),
}

/// Uma linha da árvore, já achatada na ordem em que aparece.
#[derive(Clone, Debug, PartialEq)]
pub enum Linha {
    Entrada { caminho: String, nivel: usize, entrada: api::Entrada, aberta: bool },
    Carregando { nivel: usize },
    Vazia { nivel: usize },
    Mais { nivel: usize },
    Erro { nivel: usize, texto: String },
}

/// A árvore de uma tarefa, em cache enquanto a tela roda.
pub struct Arvore {
    pub tarefa: i64,
    pastas: HashMap<String, EstadoPasta>,
    abertas: HashSet<String>,
    cursor: Option<String>,
    previa: Option<PreviaAberta>,
    /// Pedir o teclado para a gaveta (uma vez só).
    pub focar: bool,
    canal: (Sender<Resposta>, Receiver<Resposta>),
}

fn juntar(pasta: &str, nome: &str) -> String {
    if pasta.is_empty() { nome.to_string() } else { format!("{pasta}/{nome}") }
}

fn pai(caminho: &str) -> Option<&str> {
    caminho.rsplit_once('/').map(|(p, _)| p).or(if caminho.is_empty() { None } else { Some("") })
}

impl Arvore {
    pub fn nova(ctx: &egui::Context, tarefa: i64) -> Arvore {
        let mut a = Arvore { tarefa, pastas: HashMap::new(), abertas: HashSet::new(), cursor: None, previa: None, focar: true, canal: mpsc::channel() };
        a.pedir_pasta(ctx, String::new());
        a
    }

    fn pedir_pasta(&mut self, ctx: &egui::Context, caminho: String) {
        self.pastas.insert(caminho.clone(), EstadoPasta::Carregando);
        let tarefa = self.tarefa;
        registro::em_segundo_plano(&self.canal.0, ctx, move || {
            let r = api::arquivos(tarefa, &caminho);
            Resposta::Lista(caminho, r)
        });
    }

    /// "Atualizar": esquece o cache e busca de novo a raiz e as pastas abertas.
    pub fn atualizar(&mut self, ctx: &egui::Context) {
        self.pastas.clear();
        self.pedir_pasta(ctx, String::new());
        let abertas: Vec<String> = self.abertas.iter().cloned().collect();
        for p in abertas {
            self.pedir_pasta(ctx, p);
        }
    }

    fn alternar(&mut self, ctx: &egui::Context, caminho: &str) {
        if !self.abertas.remove(caminho) {
            self.abertas.insert(caminho.to_string());
            if !self.pastas.contains_key(caminho) {
                self.pedir_pasta(ctx, caminho.to_string());
            }
        }
    }

    fn abrir_previa(&mut self, ctx: &egui::Context, caminho: &str, mostrar: bool) {
        self.previa = Some(PreviaAberta { caminho: caminho.to_string(), estado: EstadoPrevia::Carregando });
        let (tarefa, caminho) = (self.tarefa, caminho.to_string());
        registro::em_segundo_plano(&self.canal.0, ctx, move || {
            let r = api::ver_arquivo(tarefa, &caminho, mostrar);
            Resposta::Previa(caminho, r)
        });
    }

    fn receber(&mut self, ctx: &egui::Context) {
        while let Ok(r) = self.canal.1.try_recv() {
            match r {
                Resposta::Lista(caminho, r) => {
                    let estado = match r {
                        Ok(l) => EstadoPasta::Pronta(l),
                        Err(e) => EstadoPasta::Erro(e),
                    };
                    self.pastas.insert(caminho, estado);
                }
                Resposta::Previa(caminho, r) => {
                    let Some(aberta) = self.previa.as_mut().filter(|p| p.caminho == caminho) else { continue };
                    aberta.estado = match r {
                        Ok(previa) => {
                            if previa.tipo == "imagem" {
                                let (tarefa, c) = (self.tarefa, caminho.clone());
                                registro::em_segundo_plano(&self.canal.0, ctx, move || {
                                    let img = api::imagem_do_arquivo(tarefa, &c).and_then(|png| registro::decodificar(&png, None));
                                    Resposta::Imagem(c, img)
                                });
                            }
                            let linhas = if previa.tipo == "texto" { previa.texto.lines().map(|l| l.replace('\t', "    ")).collect() } else { Vec::new() };
                            EstadoPrevia::Pronta { previa, linhas, imagem: None }
                        }
                        Err(e) => EstadoPrevia::Erro(e),
                    };
                }
                Resposta::Imagem(caminho, r) => {
                    if let Some(PreviaAberta { estado: EstadoPrevia::Pronta { imagem, .. }, .. }) = self.previa.as_mut().filter(|p| p.caminho == caminho) {
                        // Uma textura por arquivo aberto: sai junto com a pré-visualização.
                        *imagem = Some(r.map(|img| ctx.load_texture(format!("previa-{caminho}"), img, egui::TextureOptions::LINEAR)));
                    }
                }
            }
        }
    }

    /// As linhas que aparecem, na ordem: pastas abertas mostram o conteúdo
    /// (ou "carregando", "vazia", o erro e o aviso de lista cortada).
    pub fn linhas(&self) -> Vec<Linha> {
        let mut saida = Vec::new();
        self.achatar("", 0, &mut saida);
        saida
    }

    fn achatar(&self, pasta: &str, nivel: usize, saida: &mut Vec<Linha>) {
        match self.pastas.get(pasta) {
            None | Some(EstadoPasta::Carregando) => saida.push(Linha::Carregando { nivel }),
            Some(EstadoPasta::Erro(e)) => saida.push(Linha::Erro { nivel, texto: e.clone() }),
            Some(EstadoPasta::Pronta(lista)) => {
                if lista.entradas.is_empty() {
                    saida.push(Linha::Vazia { nivel });
                }
                for e in &lista.entradas {
                    let caminho = juntar(pasta, &e.nome);
                    let aberta = e.pasta && !e.ignorada && !e.link && self.abertas.contains(&caminho);
                    saida.push(Linha::Entrada { caminho: caminho.clone(), nivel, entrada: e.clone(), aberta });
                    if aberta {
                        self.achatar(&caminho, nivel + 1, saida);
                    }
                }
                if lista.mais {
                    saida.push(Linha::Mais { nivel });
                }
            }
        }
    }

    /// O teclado da gaveta: ↑↓ andam, → abre, ← fecha (ou sobe), Enter
    /// pré-visualiza, Esc fecha a pré-visualização e, depois, a gaveta.
    fn teclado(&mut self, ctx: &egui::Context, linhas: &[Linha]) -> Option<PedidoGaveta> {
        let tecla = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if tecla(Key::Escape) {
            if self.previa.take().is_some() {
                return None;
            }
            return Some(PedidoGaveta::Fechar);
        }
        let entradas: Vec<(&String, &api::Entrada, bool)> = linhas
            .iter()
            .filter_map(|l| match l {
                Linha::Entrada { caminho, entrada, aberta, .. } => Some((caminho, entrada, *aberta)),
                _ => None,
            })
            .collect();
        if entradas.is_empty() {
            return None;
        }
        let atual = self.cursor.as_ref().and_then(|c| entradas.iter().position(|(cam, _, _)| *cam == c));
        if tecla(Key::ArrowDown) {
            let i = atual.map_or(0, |i| (i + 1).min(entradas.len() - 1));
            self.cursor = Some(entradas[i].0.clone());
        } else if tecla(Key::ArrowUp) {
            let i = atual.map_or(0, |i| i.saturating_sub(1));
            self.cursor = Some(entradas[i].0.clone());
        } else if let Some(i) = atual {
            let (caminho, entrada, aberta) = (entradas[i].0.clone(), entradas[i].1.clone(), entradas[i].2);
            let pasta = entrada.pasta && !entrada.ignorada && !entrada.link;
            if tecla(Key::ArrowRight) && pasta && !aberta {
                self.alternar(ctx, &caminho);
            } else if tecla(Key::ArrowLeft) {
                if aberta {
                    self.abertas.remove(&caminho);
                } else if let Some(p) = pai(&caminho).filter(|p| !p.is_empty()) {
                    self.cursor = Some(p.to_string());
                }
            } else if tecla(Key::Enter) {
                if pasta {
                    self.alternar(ctx, &caminho);
                } else if !entrada.pasta && !entrada.link {
                    self.abrir_previa(ctx, &caminho, false);
                }
            }
        }
        None
    }

    /// Desenha a gaveta presa ao canto de cima à esquerda de `area` (o
    /// terminal em foco), com a altura dele. `largura_coluna`: a largura que
    /// a pré-visualização pode ocupar.
    pub fn mostrar(&mut self, ctx: &egui::Context, area: Rect, largura_coluna: f32, pasta: &str, editor: Option<&str>) -> Option<PedidoGaveta> {
        self.receber(ctx);
        let p = cores();
        let mut pedido = None;
        let linhas = self.linhas();
        let id_foco = Id::new(("gaveta-arquivos", self.tarefa));
        let com_teclado = ctx.memory(|m| m.has_focus(id_foco));
        if com_teclado {
            let filtro = egui::EventFilter { tab: false, horizontal_arrows: true, vertical_arrows: true, escape: true };
            ctx.memory_mut(|m| m.set_focus_lock_filter(id_foco, filtro));
            pedido = self.teclado(ctx, &linhas);
        }
        let largura_total =
            if self.previa.is_some() { (largura_coluna * 0.6).max(780.0).min(largura_coluna - 24.0).max(LARGURA_ARVORE) } else { LARGURA_ARVORE };
        egui::Area::new(Id::new(("gaveta", self.tarefa))).order(egui::Order::Foreground).fixed_pos(area.min).show(ctx, |ui| {
            tema::moldura_flutuante().inner_margin(egui::Margin::ZERO).show(ui, |ui| {
                ui.set_min_size(vec2(largura_total, area.height() - 2.0));
                ui.set_max_size(vec2(largura_total, area.height() - 2.0));
                let caixa = ui.max_rect();
                // O foco da gaveta: um retângulo que pega o teclado (uma vez só).
                let foco = ui.interact(caixa, id_foco, Sense::focusable_noninteractive());
                if std::mem::take(&mut self.focar) {
                    foco.request_focus();
                }
                let arvore = Rect::from_min_size(caixa.min, vec2(LARGURA_ARVORE.min(caixa.width()), caixa.height()));
                let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(arvore).layout(egui::Layout::top_down(egui::Align::Min)));
                if let Some(pg) = self.coluna_arvore(&mut filho, &linhas, pasta, id_foco) {
                    pedido = Some(pg);
                }
                if self.previa.is_some() {
                    let direita = Rect::from_min_max(pos2(arvore.right() + 12.0, caixa.top()), caixa.max);
                    let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(direita).layout(egui::Layout::top_down(egui::Align::Min)));
                    if let Some(pg) = self.coluna_previa(&mut filho, editor) {
                        pedido = Some(pg);
                    }
                }
                let _ = p;
            });
        });
        pedido
    }

    fn coluna_arvore(&mut self, ui: &mut egui::Ui, linhas: &[Linha], pasta: &str, id_foco: Id) -> Option<PedidoGaveta> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let mut pedido = None;
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(28.0);
                ui.label(tema::texto_forte("Arquivos", 14.0).color(p.texto));
                let largura = ui.available_width() - 28.0 - 8.0;
                let (r, _) = ui.allocate_exact_size(vec2(largura.max(10.0), 20.0), Sense::hover());
                tema::texto_sem_inicio(ui.painter(), r.min + vec2(4.0, 2.0), pasta, FontId::proportional(12.5), p.suave, largura - 4.0);
                if tema::botao_icone(ui, tema::Icone::Fechar, 28.0).on_hover_text("Fechar (Esc)").clicked() {
                    pedido = Some(PedidoGaveta::Fechar);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if tema::botao_secundario(ui, "Abrir pasta").on_hover_text(pasta).clicked() {
                    pedido = Some(PedidoGaveta::AbrirPasta);
                }
                ui.add_space(6.0);
                if tema::botao_secundario(ui, "Atualizar").clicked() {
                    self.atualizar(&ctx);
                }
            });
        });
        ui.add_space(8.0 - ui.spacing().item_spacing.y);
        let topo = ui.cursor().top();
        let saida = egui::ScrollArea::vertical().id_salt(("arvore", self.tarefa)).auto_shrink(false).show_rows(ui, ALTURA_LINHA, linhas.len(), |ui, faixa| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for linha in &linhas[faixa] {
                if let Some(pg) = self.linha(ui, linha, &ctx, id_foco) {
                    pedido = Some(pg);
                }
            }
        });
        let area = Rect::from_min_max(pos2(saida.inner_rect.left(), topo), saida.inner_rect.right_bottom());
        registro::sombra_rolagem(ui.painter(), area, saida.state.offset.y);
        pedido
    }

    fn linha(&mut self, ui: &mut egui::Ui, linha: &Linha, ctx: &egui::Context, id_foco: Id) -> Option<PedidoGaveta> {
        let p = cores();
        let largura = ui.available_width() - registro::MARGEM_ROLAGEM;
        let (rect, _) = ui.allocate_exact_size(vec2(largura, ALTURA_LINHA), Sense::hover());
        let pintor = ui.painter().clone();
        let recuo = |nivel: usize| rect.left() + 10.0 + nivel as f32 * 16.0;
        let erro_bloco = |texto: &str, nivel: usize| {
            let r = Rect::from_min_max(pos2(recuo(nivel), rect.top() + 1.0), pos2(rect.right() - 4.0, rect.bottom() - 1.0));
            pintor.rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), tema::fundo_tingido(p, p.erro, tema::claro()));
            pintor.circle_filled(pos2(r.left() + 10.0, r.center().y), 3.5, p.erro);
            registro::texto_cortado(&pintor, pos2(r.left() + 20.0, r.center().y), texto, FontId::proportional(13.0), p.texto, r.width() - 24.0);
        };
        match linha {
            Linha::Carregando { nivel } => {
                tema::esqueleto(&pintor, Rect::from_min_size(pos2(recuo(*nivel) + 16.0, rect.center().y - 5.0), vec2(120.0, 10.0)), tema::RAIO_ETIQUETA);
                None
            }
            Linha::Vazia { nivel } => {
                let formato = egui::TextFormat { font_id: FontId::proportional(12.5), color: p.suave, italics: true, ..Default::default() };
                let g = pintor.layout_job(egui::text::LayoutJob::single_section("Pasta vazia".into(), formato));
                pintor.galley(pos2(recuo(*nivel) + 16.0, rect.center().y - g.size().y / 2.0), g, p.suave);
                None
            }
            Linha::Erro { nivel, texto } => {
                erro_bloco(texto, *nivel);
                None
            }
            Linha::Mais { nivel } => {
                let g = pintor.layout_no_wrap(format!("Mostrando {} itens · ", "2.000"), FontId::proportional(12.5), p.suave);
                let x = recuo(*nivel) + 16.0;
                let w = g.size().x;
                pintor.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.suave);
                let g = pintor.layout_no_wrap("Abrir pasta para ver todos".into(), FontId::proportional(12.5), p.destaque);
                let link = Rect::from_min_size(pos2(x + w, rect.center().y - g.size().y / 2.0), g.size());
                pintor.galley(link.min, g, p.destaque);
                let r = ui.interact(link, Id::new(("mais-arquivos", self.tarefa, *nivel)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                r.clicked().then_some(PedidoGaveta::AbrirPasta)
            }
            Linha::Entrada { caminho, nivel, entrada, aberta } => {
                let x = recuo(*nivel);
                let pasta_que_abre = entrada.pasta && !entrada.ignorada && !entrada.link;
                let sentido = if entrada.ignorada { Sense::hover() } else { Sense::click() };
                let resposta = ui.interact(rect, Id::new(("linha-arquivo", self.tarefa, caminho.as_str())), sentido);
                let escolhida = self.cursor.as_deref() == Some(caminho.as_str()) || self.previa.as_ref().is_some_and(|pv| pv.caminho == *caminho);
                let fundo = rect.shrink2(vec2(4.0, 1.0));
                if escolhida {
                    pintor.rect_filled(fundo, CornerRadius::same(tema::RAIO_ETIQUETA), tema::fundo_escolhido());
                    // A barrinha distingue a escolhida do realce do mouse sem depender da cor.
                    let barra = Rect::from_min_size(fundo.min + vec2(0.0, 3.0), vec2(2.0, fundo.height() - 6.0));
                    pintor.rect_filled(barra, CornerRadius::same(1), p.destaque);
                } else if resposta.hovered() && !entrada.ignorada {
                    pintor.rect_filled(fundo, CornerRadius::same(tema::RAIO_ETIQUETA), p.realce);
                }
                let realcada = escolhida || (resposta.hovered() && !entrada.ignorada);
                if pasta_que_abre {
                    let c = pos2(x + 6.0, rect.center().y);
                    let cor = if resposta.hovered() { p.texto } else { p.suave };
                    let pontos = if *aberta {
                        vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
                    } else {
                        vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
                    };
                    pintor.add(egui::Shape::convex_polygon(pontos, cor, Stroke::NONE));
                }
                let fonte = if entrada.pasta { forte(13.0) } else { FontId::proportional(13.0) };
                let cor_nome = if entrada.ignorada { p.suave } else { p.texto };
                let tamanho = if entrada.pasta || entrada.link { String::new() } else { registro::tamanho(entrada.bytes) };
                let g_tam = pintor.layout_no_wrap(tamanho, FontId::proportional(11.5), if realcada { p.texto } else { p.suave });
                let largura_nome = rect.right() - (x + 16.0) - g_tam.size().x - 12.0 - if entrada.link { 16.0 } else { 0.0 };
                let g = tema::cortar(&pintor, &entrada.nome, egui::TextFormat::simple(fonte, cor_nome), largura_nome.max(20.0), 1, true);
                let fim_nome = x + 16.0 + g.size().x;
                pintor.galley(pos2(x + 16.0, rect.center().y - g.size().y / 2.0), g, cor_nome);
                if entrada.link {
                    tema::externo(&pintor, pos2(fim_nome + 6.0 + 5.0, rect.center().y), p.suave);
                }
                pintor.galley(pos2(rect.right() - 8.0 - g_tam.size().x, rect.center().y - g_tam.size().y / 2.0), g_tam, p.suave);
                let resposta = if entrada.ignorada {
                    resposta.on_hover_text("Não listada para não pesar")
                } else if entrada.link {
                    resposta.on_hover_text("Link: não é seguido")
                } else {
                    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
                };
                if resposta.clicked() {
                    self.cursor = Some(caminho.clone());
                    ctx.memory_mut(|m| m.request_focus(id_foco));
                    if pasta_que_abre {
                        self.alternar(ctx, caminho);
                    } else if !entrada.pasta && !entrada.link {
                        self.abrir_previa(ctx, caminho, false);
                    }
                }
                None
            }
        }
    }

    fn coluna_previa(&mut self, ui: &mut egui::Ui, editor: Option<&str>) -> Option<PedidoGaveta> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let mut pedido = None;
        let Some(aberta) = &self.previa else { return None };
        let caminho = aberta.caminho.clone();
        let nome = caminho.rsplit('/').next().unwrap_or(&caminho).to_string();
        let mut fechar = false;
        let mut mostrar_mesmo_assim = false;
        ui.add_space(10.0);
        // Cabeçalho: o caminho, o nome, o que é e o ×.
        ui.horizontal(|ui| {
            ui.set_height(40.0);
            let largura = ui.available_width() - 28.0 - 24.0;
            let (r, _) = ui.allocate_exact_size(vec2(largura.max(40.0), 40.0), Sense::hover());
            let pintor = ui.painter();
            let g = pintor.layout_no_wrap(nome.clone(), forte(14.0), p.texto);
            let largura_nome = g.size().x.min(r.width());
            let antes = caminho.strip_suffix(&nome).unwrap_or_default();
            if !antes.is_empty() && r.width() - largura_nome > 30.0 {
                tema::texto_sem_inicio(pintor, r.min, antes, FontId::proportional(12.5), p.suave, r.width() - largura_nome - 4.0);
            }
            let w_antes = if antes.is_empty() {
                0.0
            } else {
                pintor.layout_no_wrap(antes.into(), FontId::proportional(12.5), p.suave).size().x.min(r.width() - largura_nome - 4.0)
            };
            pintor.galley(r.min + vec2(w_antes, -1.0), g, p.texto);
            let meta = match &aberta.estado {
                EstadoPrevia::Pronta { previa, .. } if previa.tipo == "texto" => {
                    format!("{} {} · {}", previa.linhas, if previa.linhas == 1 { "linha" } else { "linhas" }, registro::tamanho(previa.bytes))
                }
                EstadoPrevia::Pronta { previa, .. } if previa.tipo == "imagem" => format!("{} × {} · {}", previa.largura, previa.altura, previa.formato),
                EstadoPrevia::Pronta { previa, .. } => registro::tamanho(previa.bytes),
                _ => String::new(),
            };
            pintor.text(r.min + vec2(0.0, 22.0), Align2::LEFT_TOP, meta, FontId::proportional(12.0), p.suave);
            if tema::botao_icone(ui, tema::Icone::Fechar, 28.0).on_hover_text("Fechar a pré-visualização (Esc)").clicked() {
                fechar = true;
            }
        });
        let rodape = 48.0;
        let corpo =
            Rect::from_min_max(pos2(ui.max_rect().left(), ui.cursor().top() + 4.0), pos2(ui.max_rect().right() - 12.0, ui.max_rect().bottom() - rodape));
        let mut absoluto = String::new();
        let mut cortado = false;
        match &aberta.estado {
            EstadoPrevia::Carregando => tema::esqueleto(ui.painter(), corpo, tema::RAIO_CONTROLE),
            EstadoPrevia::Erro(e) => {
                let r = Rect::from_min_size(corpo.min, vec2(corpo.width(), 36.0));
                ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), tema::fundo_tingido(p, p.erro, tema::claro()));
                ui.painter().circle_filled(pos2(r.left() + 12.0, r.center().y), 3.5, p.erro);
                registro::texto_cortado(ui.painter(), pos2(r.left() + 24.0, r.center().y), e, FontId::proportional(13.0), p.texto, r.width() - 28.0);
            }
            EstadoPrevia::Pronta { previa, linhas, imagem } => {
                absoluto = previa.caminho_absoluto.clone();
                cortado = previa.cortado;
                match previa.tipo.as_str() {
                    "texto" => texto(ui, corpo, linhas, previa.cortado, &caminho),
                    "imagem" => {
                        ui.painter().rect(corpo, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie, Stroke::NONE, StrokeKind::Inside);
                        match imagem {
                            Some(Ok(t)) => {
                                let tamanho = t.size_vec2();
                                let escala = ((corpo.width() - 16.0) / tamanho.x).min((corpo.height() - 16.0) / tamanho.y).min(1.0);
                                let caixa = Rect::from_center_size(corpo.center(), tamanho * escala);
                                egui::Image::new(t).corner_radius(CornerRadius::same(tema::RAIO_CONTROLE)).paint_at(ui, caixa);
                                ui.painter().rect_stroke(caixa, CornerRadius::same(tema::RAIO_CONTROLE), Stroke::new(1.0, p.borda), StrokeKind::Inside);
                            }
                            Some(Err(e)) => {
                                ui.painter().text(
                                    corpo.center(),
                                    Align2::CENTER_CENTER,
                                    format!("Não consegui mostrar a imagem: {e}"),
                                    FontId::proportional(13.0),
                                    p.texto,
                                );
                            }
                            None => tema::esqueleto(ui.painter(), corpo.shrink(16.0), tema::RAIO_CONTROLE),
                        }
                    }
                    "sensivel" => {
                        let c = corpo.center();
                        let pintor = ui.painter();
                        pintor.circle_stroke(c - vec2(0.0, 44.0), 4.25, Stroke::new(1.5, p.alerta));
                        pintor.text(c - vec2(0.0, 22.0), Align2::CENTER_CENTER, "Este arquivo pode ter senhas ou chaves", forte(14.0), p.texto);
                        pintor.text(c, Align2::CENTER_CENTER, "A tela pode estar compartilhada.", FontId::proportional(13.0), p.suave);
                        let largura = ui.painter().layout_no_wrap("Mostrar mesmo assim".into(), FontId::proportional(13.0), p.texto).size().x + 28.0;
                        let r = Rect::from_center_size(c + vec2(0.0, 40.0), vec2(largura, 32.0));
                        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(r));
                        mostrar_mesmo_assim = tema::botao_secundario(&mut filho, "Mostrar mesmo assim").clicked();
                    }
                    _ => {
                        let c = corpo.center();
                        ui.painter().rect(corpo, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie_alta, Stroke::new(1.0, p.borda), StrokeKind::Inside);
                        ui.painter().text(c - vec2(0.0, 10.0), Align2::CENTER_CENTER, "Arquivo binário", forte(14.0), p.texto);
                        let texto = format!("{}, sem pré-visualização", registro::tamanho(previa.bytes));
                        ui.painter().text(c + vec2(0.0, 12.0), Align2::CENTER_CENTER, texto, FontId::proportional(13.0), p.suave);
                    }
                }
            }
        }
        // Rodapé: abrir no editor, no sistema (só os tipos da lista) e citar na mensagem.
        let pe = Rect::from_min_max(pos2(corpo.left(), corpo.bottom() + 8.0), pos2(corpo.right(), corpo.bottom() + rodape - 4.0));
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(pe).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let pronta = !absoluto.is_empty();
        // Sem espaço para os três botões inteiros, "Abrir no IntelliJ" vira "IntelliJ".
        let medir = |t: &str| ui.painter().layout_no_wrap(t.to_owned(), FontId::proportional(13.0), p.texto).size().x + 28.0 + 8.0;
        let necessario = editor.map_or(0.0, |e| medir(&format!("Abrir no {e}"))) + medir("Abrir no sistema") + medir("Citar na mensagem");
        let curto = necessario > pe.width();
        if let Some(nome_editor) = editor {
            let rotulo = if curto { nome_editor.to_string() } else { format!("Abrir no {nome_editor}") };
            let dica = if cortado { "Abrir o arquivo inteiro no editor" } else { "Abrir no editor" };
            if tema::botao_secundario_com(&mut filho, &rotulo, pronta).on_hover_text(dica).clicked() {
                pedido = Some(PedidoGaveta::AbrirNoEditor(absoluto.clone()));
            }
            filho.add_space(6.0);
        }
        if sistema::abre_no_sistema(&nome) {
            if tema::botao_secundario_com(&mut filho, if curto { "Sistema" } else { "Abrir no sistema" }, pronta).clicked() {
                pedido = Some(PedidoGaveta::AbrirNoSistema(absoluto.clone()));
            }
            filho.add_space(6.0);
        }
        if tema::botao_secundario(&mut filho, "Citar na mensagem").on_hover_text(format!("@{caminho}")).clicked() {
            pedido = Some(PedidoGaveta::Citar(format!("@{caminho}")));
        }
        if fechar {
            self.previa = None;
        }
        if mostrar_mesmo_assim {
            self.abrir_previa(&ctx, &caminho, true);
        }
        pedido
    }
}

/// O texto do arquivo: monoespaçado, com número de linha, sem quebra e só as
/// linhas visíveis desenhadas.
fn texto(ui: &mut egui::Ui, corpo: Rect, linhas: &[String], cortado: bool, caminho: &str) {
    let p = cores();
    // O contorno marca onde o arquivo começa e acaba (nos temas claros, o
    // fundo do terminal quase não se separa da gaveta).
    ui.painter().rect(corpo, CornerRadius::same(tema::RAIO_CONTROLE), p.terminal_fundo, Stroke::new(1.0, p.borda), StrokeKind::Inside);
    let faixa_corte = if cortado { 32.0 } else { 0.0 };
    let area = Rect::from_min_max(corpo.min + vec2(0.0, 6.0), corpo.max - vec2(0.0, faixa_corte + 6.0));
    let digitos = linhas.len().max(1).to_string().len() as f32;
    let calha = digitos * 7.5 + 16.0;
    let fonte = FontId::monospace(12.5);
    let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
    // Barras sólidas: a horizontal aparece sempre que uma linha passa da
    // largura (as flutuantes só surgiam com o mouse em cima, e parecia que a
    // linha longa não tinha fim). Shift+rodinha também anda para o lado.
    filho.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    egui::ScrollArea::both().id_salt(("previa-texto", caminho)).auto_shrink(false).show_rows(&mut filho, ALTURA_LINHA_TEXTO, linhas.len(), |ui, faixa| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for n in faixa {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().max(calha + 8.0), ALTURA_LINHA_TEXTO), Sense::hover());
            ui.painter().text(pos2(r.left() + calha - 8.0, r.center().y), Align2::RIGHT_CENTER, (n + 1).to_string(), fonte.clone(), p.suave);
            let g = ui.painter().layout_no_wrap(linhas[n].clone(), fonte.clone(), p.terminal_texto);
            let largura = g.size().x;
            ui.painter().galley(pos2(r.left() + calha, r.center().y - g.size().y / 2.0), g, p.terminal_texto);
            // Reserva a largura da linha para a rolagem horizontal.
            if largura + calha > r.width() {
                ui.allocate_exact_size(vec2(largura + calha, 0.0), Sense::hover());
            }
        }
    });
    if cortado {
        let faixa = Rect::from_min_max(pos2(corpo.left(), corpo.bottom() - faixa_corte), corpo.max);
        ui.painter().rect_filled(faixa, CornerRadius { nw: 0, ne: 0, sw: tema::RAIO_CONTROLE, se: tema::RAIO_CONTROLE }, p.superficie);
        let g = ui.painter().layout_no_wrap("Mostrando os primeiros 256 KB · abra no editor para ver tudo".into(), FontId::proportional(12.5), p.texto);
        ui.painter().galley(pos2(faixa.left() + 12.0, faixa.center().y - g.size().y / 2.0), g, Color32::WHITE);
    }
}

/// Menor sobra ao lado da Colmeia que ainda serve para o navegador.
const SOBRA_MINIMA: f32 = 360.0;

/// A geometria da janela do navegador: à direita da Colmeia se sobrar pelo
/// menos 360 pontos; senão, na metade direita do monitor, começando abaixo
/// do cabeçalho da tarefa (`topo`, em pontos da tela), para os botões dele
/// continuarem à vista. Em pixels da tela (pontos × escala), nunca mais alta
/// que o monitor. Medidas absurdas são ignoradas: o X11 sem gerenciador de
/// janelas já informou um monitor de 1x1 (e a janela do navegador nascia
/// numa tirinha de 500x88 no canto). Sem monitor, a metade direita da
/// própria Colmeia, abaixo do cabeçalho.
pub fn geometria_ao_lado(janela: Option<Rect>, monitor: Option<egui::Vec2>, escala: f32, topo: Option<f32>) -> Option<[i32; 4]> {
    let px = |v: f32| (v * escala).round() as i32;
    let janela = janela.filter(|j| j.width() >= 200.0 && j.height() >= 200.0 && j.right() > 0.0);
    let Some(monitor) = monitor.filter(|m| m.x >= 640.0 && m.y >= 480.0) else {
        let j = janela?;
        let y = topo.unwrap_or(j.top()).clamp(j.top(), j.bottom() - 200.0);
        return Some([px(j.center().x), px(y), px(j.width() / 2.0), px(j.bottom() - y)]);
    };
    let janela = janela.filter(|j| j.left() < monitor.x);
    let metade = monitor.x / 2.0;
    if let Some(j) = janela {
        let sobra = monitor.x - j.right();
        if sobra >= SOBRA_MINIMA {
            let y = j.top().clamp(0.0, monitor.y - 300.0);
            return Some([px(j.right()), px(y), px(sobra), px(j.height().max(480.0).min(monitor.y - y))]);
        }
        // A Colmeia ocupa a metade direita: o navegador fica abaixo do cabeçalho.
        if let Some(t) = topo
            && j.right() > metade
        {
            let y = t.clamp(0.0, monitor.y - 300.0);
            return Some([px(metade), px(y), px(metade), px(monitor.y - y)]);
        }
    }
    Some([px(metade), 0, px(metade), px(monitor.y)])
}

/// O endereço como o usuário digitou vira um endereço aceito pelo núcleo:
/// "localhost:5173" vira http, um nome de site vira https e um caminho
/// absoluto vira file://. O núcleo continua conferindo.
pub fn normalizar_endereco(texto: &str) -> String {
    let t = texto.trim();
    if t.is_empty() || t.contains("://") {
        return t.to_string();
    }
    if t.starts_with('/') {
        return format!("file://{t}");
    }
    let host = t.split(['/', '?', '#']).next().unwrap_or(t);
    let nome = host.rsplit_once(':').map_or(host, |(h, porta)| if porta.chars().all(|c| c.is_ascii_digit()) { h } else { host });
    let local =
        nome == "localhost" || nome.ends_with(".localhost") || nome.split('.').all(|parte| !parte.is_empty() && parte.chars().all(|c| c.is_ascii_digit()));
    if local { format!("http://{t}") } else { format!("https://{t}") }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn entrada(nome: &str, pasta: bool) -> api::Entrada {
        api::Entrada { nome: nome.into(), pasta, ..Default::default() }
    }

    #[test]
    fn arvore_achata_so_as_pastas_abertas() {
        let mut a = Arvore { tarefa: 1, pastas: HashMap::new(), abertas: HashSet::new(), cursor: None, previa: None, focar: false, canal: mpsc::channel() };
        assert_eq!(a.linhas(), vec![Linha::Carregando { nivel: 0 }]);
        let mut git = entrada(".git", true);
        git.ignorada = true;
        a.pastas.insert(
            String::new(),
            EstadoPasta::Pronta(api::ListaArquivos { entradas: vec![git, entrada("src", true), entrada("README.md", false)], mais: false }),
        );
        a.abertas.insert("src".into());
        a.abertas.insert(".git".into());
        let linhas = a.linhas();
        // .git aberta não mostra nada (ignorada); src aberta ainda carregando.
        assert_eq!(linhas.len(), 4);
        assert!(matches!(&linhas[1], Linha::Entrada { caminho, aberta: true, .. } if caminho == "src"));
        assert_eq!(linhas[2], Linha::Carregando { nivel: 1 });
        a.pastas.insert("src".into(), EstadoPasta::Pronta(api::ListaArquivos { entradas: vec![], mais: false }));
        assert_eq!(a.linhas()[2], Linha::Vazia { nivel: 1 });
        assert_eq!(pai("src/main.rs"), Some("src"));
        assert_eq!(pai("README.md"), Some(""));
    }

    #[test]
    fn endereco_sem_esquema_ganha_um() {
        assert_eq!(normalizar_endereco("localhost:5173/pedidos"), "http://localhost:5173/pedidos");
        assert_eq!(normalizar_endereco("127.0.0.1:8080"), "http://127.0.0.1:8080");
        assert_eq!(normalizar_endereco("exemplo.com.br/a"), "https://exemplo.com.br/a");
        assert_eq!(normalizar_endereco("https://x.com"), "https://x.com");
        assert_eq!(normalizar_endereco(" /tmp/loja-web/index.html "), "file:///tmp/loja-web/index.html");
    }

    #[test]
    fn navegador_ao_lado_ou_na_metade_direita() {
        let monitor = Some(vec2(1920.0, 1080.0));
        let janela = |x: f32, y: f32, l: f32, a: f32| Some(Rect::from_min_size(pos2(x, y), vec2(l, a)));
        // Cabe à direita da janela.
        assert_eq!(geometria_ao_lado(janela(0.0, 0.0, 1200.0, 900.0), monitor, 1.0, None), Some([1200, 0, 720, 900]));
        // Sobra estreita, mas de pelo menos 360: usa a sobra.
        assert_eq!(geometria_ao_lado(janela(0.0, 0.0, 1240.0, 720.0), Some(vec2(1600.0, 900.0)), 1.0, Some(60.0)), Some([1240, 0, 360, 720]));
        // Janela maximizada sem cabeçalho de tarefa: a metade direita do monitor, por cima.
        assert_eq!(geometria_ao_lado(janela(0.0, 0.0, 1920.0, 1080.0), monitor, 1.0, None), Some([960, 0, 960, 1080]));
        // Com o cabeçalho da tarefa: abaixo dele, sem esconder os botões.
        assert_eq!(geometria_ao_lado(janela(0.0, 0.0, 1280.0, 720.0), Some(vec2(1600.0, 900.0)), 1.0, Some(96.0)), Some([800, 96, 800, 804]));
        // Nunca mais alta que o monitor.
        assert_eq!(geometria_ao_lado(janela(0.0, 200.0, 1000.0, 900.0), Some(vec2(1600.0, 900.0)), 1.0, None), Some([1000, 200, 600, 700]));
        // Tamanho absurdo (1 px de largura): como se não soubesse da janela.
        assert_eq!(geometria_ao_lado(janela(0.0, 0.0, 1.0, 88.0), Some(vec2(1600.0, 900.0)), 1.0, None), Some([800, 0, 800, 900]));
        assert_eq!(geometria_ao_lado(None, monitor, 2.0, None), Some([1920, 0, 1920, 2160]));
        assert_eq!(geometria_ao_lado(None, None, 1.0, None), None);
        assert_eq!(geometria_ao_lado(None, Some(vec2(1.0, 1.0)), 1.0, None), None);
        // Monitor de 1x1 (o X11 já mandou): a metade direita da Colmeia, abaixo do cabeçalho.
        assert_eq!(geometria_ao_lado(janela(0.0, 0.0, 1280.0, 720.0), Some(vec2(1.0, 1.0)), 1.0, Some(108.0)), Some([640, 108, 640, 612]));
    }
}
