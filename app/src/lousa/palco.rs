//! O palco: a lousa em tela cheia, de cartão em cartão (notas, código,
//! imagens, vídeos e tarefas, seguindo as ligações e, sem elas, na ordem de
//! leitura), com a visão geral. A câmera desliza 300 ms até o cartão; só
//! nesse intervalo há redesenho.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Align2, CornerRadius, FontId, Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::camera::{self, Camera};
use super::desenho::{self, Desenho, Marcas, caixa_quadro};
use crate::api::{ElementoLousa as Elemento, TipoElemento};
use crate::dados;
use crate::tema::{self, Icone, cores, forte};

/// Duração da transição entre cartões.
const TRANSICAO: f64 = 0.3;
/// Altura do rodapé (a navegação), fixa como na apresentação.
const RODAPE: f32 = 64.0;

/// O que entra no palco: textos soltos e ligações ficam de fora.
pub fn vai_ao_palco(e: &Elemento) -> bool {
    matches!(e.tipo, TipoElemento::Nota | TipoElemento::Codigo | TipoElemento::Imagem | TipoElemento::Video | TipoElemento::Tarefa)
}

/// A ordem do palco: segue as ligações. Cada grupo de cartões ligados entre
/// si é percorrido em profundidade a partir de quem não tem entrada (o
/// fluxo nota → código → tabela fica junto); os grupos, e os filhos de um
/// mesmo cartão, vão na ordem de leitura. Sem ligações, é a ordem de leitura.
pub fn ordem_do_palco(elementos: &[Elemento]) -> Vec<i64> {
    let leitura = ordem_de_leitura(elementos);
    let posicao: HashMap<i64, usize> = leitura.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut saidas: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut vizinhos: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut com_entrada: HashSet<i64> = HashSet::new();
    for l in elementos.iter().filter(|e| e.tipo == TipoElemento::Ligacao && e.de != e.para) {
        if !(posicao.contains_key(&l.de) && posicao.contains_key(&l.para)) {
            continue;
        }
        saidas.entry(l.de).or_default().push(l.para);
        vizinhos.entry(l.de).or_default().push(l.para);
        vizinhos.entry(l.para).or_default().push(l.de);
        com_entrada.insert(l.para);
    }
    for filhos in saidas.values_mut() {
        filhos.sort_by_key(|id| posicao[id]);
    }
    let mut ordem = Vec::with_capacity(leitura.len());
    let mut visto: HashSet<i64> = HashSet::new();
    let mut no_grupo: HashSet<i64> = HashSet::new();
    for inicio in &leitura {
        if no_grupo.contains(inicio) {
            continue;
        }
        // O grupo do cartão (ligações nos dois sentidos).
        let mut grupo = vec![*inicio];
        no_grupo.insert(*inicio);
        let mut i = 0;
        while i < grupo.len() {
            for v in vizinhos.get(&grupo[i]).into_iter().flatten() {
                if no_grupo.insert(*v) {
                    grupo.push(*v);
                }
            }
            i += 1;
        }
        grupo.sort_by_key(|id| posicao[id]);
        // Raízes primeiro; um ciclo sem raiz começa pelo primeiro na leitura.
        let raizes = grupo.iter().filter(|id| !com_entrada.contains(id)).chain(grupo.iter());
        for raiz in raizes {
            let mut pilha = vec![*raiz];
            while let Some(id) = pilha.pop() {
                if !visto.insert(id) {
                    continue;
                }
                ordem.push(id);
                for filho in saidas.get(&id).into_iter().flatten().rev() {
                    if !visto.contains(filho) {
                        pilha.push(*filho);
                    }
                }
            }
        }
    }
    ordem
}

/// A ordem de leitura: faixas de cima para baixo (um cartão entra na faixa
/// se começa antes da metade da altura do primeiro dela) e, em cada faixa,
/// da esquerda para a direita.
pub fn ordem_de_leitura(elementos: &[Elemento]) -> Vec<i64> {
    let mut cartoes: Vec<&Elemento> = elementos.iter().filter(|e| vai_ao_palco(e)).collect();
    cartoes.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut faixas: Vec<Vec<&Elemento>> = Vec::new();
    for e in cartoes {
        match faixas.last_mut() {
            Some(faixa) if e.y < faixa[0].y + faixa[0].altura / 2.0 => faixa.push(e),
            _ => faixas.push(vec![e]),
        }
    }
    faixas
        .into_iter()
        .flat_map(|mut f| {
            f.sort_by(|a, b| a.x.total_cmp(&b.x));
            f.into_iter().map(|e| e.id)
        })
        .collect()
}

/// O vídeo que não abriu: o item e o erro.
type FalhaDoVideo = (i64, String);

pub enum PedidoPalco {
    Sair,
    AbrirTarefa(i64),
}

pub struct Palco {
    elementos: Vec<Elemento>,
    ordem: Vec<i64>,
    atual: usize,
    visao_geral: bool,
    camera: Camera,
    de: Camera,
    para: Camera,
    inicio: Option<f64>,
    desenho: Desenho,
    grade: tema::Grade,
    atalhos: bool,
    area: Rect,
    tema_base: tema::Escolha,
    tema_sessao: Option<tema::Escolha>,
    /// A primeira vez enquadra sem transição.
    posta: bool,
    /// Vídeo abrindo ou com erro: o item, o texto, se é erro e quando.
    video: Option<(i64, String, bool, f64)>,
    falhas: (Sender<FalhaDoVideo>, Receiver<FalhaDoVideo>),
}

impl Palco {
    /// `desde`: o item por onde começar (Shift+F5), se for um cartão.
    pub fn novo(elementos: Vec<Elemento>, desde: Option<i64>, tema_base: tema::Escolha) -> Palco {
        let ordem = ordem_do_palco(&elementos);
        let atual = desde.and_then(|d| ordem.iter().position(|id| *id == d)).unwrap_or(0);
        Palco {
            elementos,
            ordem,
            atual,
            visao_geral: false,
            camera: Camera::default(),
            de: Camera::default(),
            para: Camera::default(),
            inicio: None,
            desenho: Desenho::novo("palco"),
            grade: tema::Grade::default(),
            atalhos: false,
            area: Rect::NOTHING,
            tema_base,
            tema_sessao: None,
            posta: false,
            video: None,
            falhas: mpsc::channel(),
        }
    }

    /// O cartão atual, se houver.
    fn cartao_atual(&self) -> Option<&Elemento> {
        let id = *self.ordem.get(self.atual)?;
        self.elementos.iter().find(|e| e.id == id)
    }

    /// Abre o vídeo no reprodutor, como na lousa: "Abrindo no reprodutor…" no
    /// cartão e, se falhar, o erro no próprio cartão.
    fn abrir_video(&mut self, ctx: &egui::Context, id: i64, anexo: i64, agora: f64) {
        self.video = Some((id, "Abrindo no reprodutor…".into(), false, agora));
        ctx.request_repaint_after(std::time::Duration::from_secs(3));
        let envio = self.falhas.0.clone();
        super::abrir_no_reprodutor(ctx, anexo, move |e| {
            let _ = envio.send((id, e));
        });
    }

    #[cfg(test)]
    pub fn vazio(&self) -> bool {
        self.ordem.is_empty()
    }

    /// Volta o tema da sessão (T) ao do perfil, ao sair.
    pub fn encerrar(&mut self, ctx: &egui::Context) {
        if self.tema_sessao.take().is_some() {
            self.tema_base.aplicar(ctx);
        }
    }

    fn util(&self) -> Rect {
        Rect::from_min_max(self.area.min, pos2(self.area.right(), self.area.bottom() - RODAPE))
    }

    /// A câmera do cartão atual (ou de tudo, na visão geral).
    fn alvo(&self) -> Camera {
        let util = self.util();
        let s = crate::apresentacao::escala(self.area.size());
        if self.visao_geral || self.ordem.is_empty() {
            let caixas = self.elementos.iter().filter(|e| e.tipo != TipoElemento::Ligacao).map(caixa_quadro);
            let tudo = camera::envolver(caixas).unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)));
            return Camera::enquadrar(util, tudo, 64.0 * s, 1.0, false);
        }
        let id = self.ordem[self.atual.min(self.ordem.len() - 1)];
        let caixa = self.elementos.iter().find(|e| e.id == id).map_or(Rect::NOTHING, caixa_quadro);
        Camera::enquadrar(util, caixa, 64.0 * s, 3.0, false)
    }

    fn ir(&mut self, atual: usize, agora: f64) {
        self.atual = atual.min(self.ordem.len().saturating_sub(1));
        self.visao_geral = false;
        self.transicao(agora);
    }

    fn transicao(&mut self, agora: f64) {
        self.de = self.camera;
        self.para = self.alvo();
        self.inicio = Some(agora);
    }

    fn teclado(&mut self, ctx: &egui::Context, agora: f64) -> Option<PedidoPalco> {
        let tecla = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if self.atalhos {
            if tecla(Key::Escape) || tecla(Key::Questionmark) {
                self.atalhos = false;
            }
            return None;
        }
        let interrogacao = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "?")));
        let total = self.ordem.len();
        if tecla(Key::Escape) {
            return Some(PedidoPalco::Sair);
        }
        // Enter no cartão de vídeo abre o vídeo.
        if !self.visao_geral
            && let Some((id, anexo)) = self.cartao_atual().filter(|e| e.tipo == TipoElemento::Video).map(|e| (e.id, e.anexo_id))
            && tecla(Key::Enter)
        {
            self.abrir_video(ctx, id, anexo, agora);
        }
        if tecla(Key::ArrowRight) || tecla(Key::Space) || tecla(Key::PageDown) {
            self.ir((self.atual + 1).min(total.saturating_sub(1)), agora);
        } else if tecla(Key::ArrowLeft) || tecla(Key::PageUp) {
            self.ir(self.atual.saturating_sub(1), agora);
        } else if tecla(Key::Home) {
            self.ir(0, agora);
        } else if tecla(Key::End) {
            self.ir(total.saturating_sub(1), agora);
        } else if tecla(Key::O) {
            self.visao_geral = !self.visao_geral;
            self.transicao(agora);
        } else if tecla(Key::T) {
            let atual = self.tema_sessao.unwrap_or(self.tema_base);
            let outro = if atual == tema::Escolha::Escuro { tema::Escolha::Claro } else { tema::Escolha::Escuro };
            outro.aplicar(ctx);
            self.tema_sessao = Some(outro);
        } else if interrogacao || tecla(Key::Questionmark) {
            self.atalhos = true;
        }
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
        None
    }

    /// Desenha o palco na janela inteira.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, modelo: &dados::Modelo, agora: f64) -> Option<PedidoPalco> {
        let ctx = ui.ctx().clone();
        let p = cores();
        let tela = ui.max_rect();
        let mudou_area = tela != self.area;
        self.area = tela;
        self.desenho.receber(&ctx);
        while let Ok((id, erro)) = self.falhas.1.try_recv() {
            self.video = Some((id, erro, true, agora));
        }
        if !self.posta || mudou_area {
            self.posta = true;
            self.camera = self.alvo();
            self.para = self.camera;
        }
        let mut pedido = self.teclado(&ctx, agora);
        // A transição de 300 ms: o único redesenho seguido do palco.
        if let Some(inicio) = self.inicio {
            let t = ((agora - inicio) / TRANSICAO) as f32;
            if t >= 1.0 {
                self.camera = self.para;
                self.inicio = None;
            } else {
                self.camera = Camera::entre(self.de, self.para, self.util(), t);
                ctx.request_repaint();
            }
        }
        ui.painter().rect_filled(tela, 0, p.fundo);
        let util = self.util();
        self.grade.desenhar(&ui.painter_at(util), util, self.camera.origem, self.camera.zoom);
        let tarefas = |id: i64| super::info_tarefa(modelo, id);
        let atual = self.ordem.get(self.atual).copied();
        let video = self.video.as_ref().filter(|(_, _, erro, desde)| *erro || agora - desde < 3.0).map(|(id, t, erro, _)| (*id, t.clone(), *erro));
        let marcas = Marcas { limpo: true, video, foco: atual.filter(|_| !self.visao_geral), ..Default::default() };
        self.desenho.desenhar(ui, util, &self.camera, &self.elementos, &tarefas, &marcas);
        let pintor = ui.painter_at(util);
        let mut clicado = None;
        let mut abrir_video = None;
        for (n, id) in self.ordem.iter().enumerate() {
            let Some(e) = self.elementos.iter().find(|e| e.id == *id) else { continue };
            let r = self.camera.retangulo_na_tela(util, caixa_quadro(e));
            if self.visao_geral {
                let resposta = ui.interact(r, Id::new(("palco-cartao", *id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                if resposta.hovered() {
                    pintor.rect_stroke(r.expand(3.0), CornerRadius::same(tema::RAIO_CARTAO + 3), Stroke::new(2.0, p.destaque), StrokeKind::Outside);
                }
                let centro = r.left_top() - vec2(8.0, 8.0);
                pintor.circle_filled(centro, 11.0, p.destaque);
                pintor.text(centro, Align2::CENTER_CENTER, (n + 1).to_string(), forte(12.0), tema::sobre_destaque());
                if resposta.clicked() {
                    clicado = Some(n);
                }
            } else if Some(*id) != atual {
                // Véu nos que não são o atual.
                pintor.rect_filled(
                    r.expand(1.0),
                    CornerRadius::same(desenho::canto(tema::RAIO_CARTAO as f32, self.camera.zoom) as u8),
                    p.fundo.gamma_multiply(0.6),
                );
            } else if e.tipo == TipoElemento::Tarefa && e.tarefa_ref != 0 {
                let resposta = ui.interact(r, Id::new(("palco-tarefa", *id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                if resposta.on_hover_text("Abrir a tarefa").double_clicked() {
                    pedido = Some(PedidoPalco::AbrirTarefa(e.tarefa_ref));
                }
            } else if e.tipo == TipoElemento::Video {
                // O play do cartão atual abre o vídeo, como na lousa.
                let (centro, raio) = desenho::play_do_video(r, self.camera.zoom);
                let alvo = Rect::from_center_size(centro, Vec2::splat(raio.max(12.0) * 2.0));
                let resposta = ui.interact(alvo, Id::new(("palco-video", *id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                if resposta.on_hover_text("Abrir no reprodutor (Enter)").clicked() {
                    abrir_video = Some((e.id, e.anexo_id));
                }
            }
        }
        // O cartão atual fica por cima de tudo (um vizinho sobreposto não o cobre), com sombra.
        if !self.visao_geral
            && let Some(e) = atual.and_then(|id| self.elementos.iter().find(|e| e.id == id)).cloned()
        {
            let r = self.camera.retangulo_na_tela(util, caixa_quadro(&e));
            let canto = desenho::canto(tema::RAIO_CARTAO as f32, self.camera.zoom) as u8;
            pintor.add(tema::sombra(6, 18).as_shape(r, CornerRadius::same(canto)));
            self.desenho.desenhar(ui, util, &self.camera, std::slice::from_ref(&e), &tarefas, &marcas);
        }
        if let Some(n) = clicado {
            self.ir(n, agora);
        }
        if let Some((id, anexo)) = abrir_video {
            self.abrir_video(&ctx, id, anexo, agora);
        }
        if self.ordem.is_empty() {
            ui.painter().text(util.center(), Align2::CENTER_CENTER, "A lousa não tem cartões para apresentar", FontId::proportional(18.0), p.suave);
        }
        if let Some(p) = self.rodape(ui, tela, agora) {
            pedido = Some(p);
        }
        if self.atalhos {
            self.painel_de_atalhos(&ctx, tela);
        }
        if matches!(pedido, Some(PedidoPalco::Sair)) {
            self.encerrar(&ctx);
        }
        pedido
    }

    fn rodape(&mut self, ui: &mut egui::Ui, tela: Rect, agora: f64) -> Option<PedidoPalco> {
        let p = cores();
        let mut pedido = None;
        let faixa = Rect::from_min_max(pos2(tela.left(), tela.bottom() - RODAPE), tela.max);
        let margem = (64.0 * crate::apresentacao::escala(tela.size())).round();
        let mut filho = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(faixa.left() + margem, faixa.center().y - 16.0), pos2(faixa.right() - margem, faixa.center().y + 16.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let total = self.ordem.len();
        if tema::botao_icone(&mut filho, Icone::Anterior, 32.0).on_hover_text("Anterior (←)").clicked() {
            self.ir(self.atual.saturating_sub(1), agora);
        }
        // O contador tem a largura de "88 / 88" (ou mais, com mais cartões): os
        // botões não andam ao navegar.
        let posicao = if total == 0 { "0 / 0".to_string() } else { format!("{} / {total}", self.atual + 1) };
        let modelo_largura = format!("{0} / {0}", "8".repeat(total.max(10).to_string().len()));
        let largura = filho.painter().layout_no_wrap(modelo_largura, forte(15.0), p.suave).size().x + 8.0;
        let (rect, _) = filho.allocate_exact_size(vec2(largura, 32.0), Sense::hover());
        filho.painter().text(rect.center(), Align2::CENTER_CENTER, posicao, forte(15.0), p.suave);
        if tema::botao_icone(&mut filho, Icone::Proximo, 32.0).on_hover_text("Próximo (→ ou espaço)").clicked() {
            self.ir((self.atual + 1).min(total.saturating_sub(1)), agora);
        }
        filho.add_space(16.0);
        // A tecla desenhada antes do nome (como na tela de atalhos), não "O visão geral".
        tema::tecla(&mut filho, "O");
        filho.add_space(6.0);
        if tema::chip_alternar(&mut filho, "Visão geral", self.visao_geral).clicked() {
            self.visao_geral = !self.visao_geral;
            self.transicao(agora);
        }
        filho.add_space(12.0);
        tema::tecla(&mut filho, "?");
        filho.add_space(6.0);
        if tema::botao_secundario(&mut filho, "Atalhos").clicked() {
            self.atalhos = true;
        }
        filho.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tema::botao_icone(ui, Icone::Fechar, 32.0).on_hover_text("Sair (Esc)").clicked() {
                pedido = Some(PedidoPalco::Sair);
            }
        });
        pedido
    }

    fn painel_de_atalhos(&mut self, ctx: &egui::Context, tela: Rect) {
        let p = cores();
        egui::Area::new(Id::new("palco-atalhos")).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(tela.center()).show(ctx, |ui| {
            tema::moldura_janela().show(ui, |ui| {
                ui.label(tema::texto_forte("Atalhos do palco", 17.0).color(p.texto));
                ui.add_space(10.0);
                for (teclas, texto) in [
                    ("→  espaço", "próximo cartão"),
                    ("←", "cartão anterior"),
                    ("Home  End", "primeiro e último"),
                    ("O", "visão geral"),
                    ("T", "trocar o tema"),
                    ("Esc", "sair"),
                ] {
                    ui.horizontal(|ui| {
                        ui.set_min_width(300.0);
                        tema::tecla(ui, teclas);
                        ui.label(egui::RichText::new(texto).color(p.texto).size(13.5));
                    });
                }
            });
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn e(id: i64, tipo: TipoElemento, x: f32, y: f32, altura: f32) -> Elemento {
        Elemento { id, tipo, x, y, largura: 200.0, altura, ..Default::default() }
    }

    #[test]
    fn ordem_de_leitura_por_faixas() {
        let lista = vec![
            e(1, TipoElemento::Nota, 500.0, 10.0, 200.0),
            e(2, TipoElemento::Nota, 0.0, 60.0, 200.0),
            e(3, TipoElemento::Codigo, 250.0, 0.0, 100.0),
            e(4, TipoElemento::Imagem, 0.0, 400.0, 100.0),
            e(5, TipoElemento::Texto, 0.0, 0.0, 40.0),
            e(6, TipoElemento::Ligacao, 0.0, 0.0, 16.0),
            e(7, TipoElemento::Tarefa, 300.0, 430.0, 96.0),
        ];
        // Faixa 1: 3 (y 0), 1 (10) e 2 (60 < 0 + 50? não): o 2 abre a faixa seguinte.
        assert_eq!(ordem_de_leitura(&lista), vec![3, 1, 2, 4, 7]);
    }

    #[test]
    fn ordem_do_palco_segue_as_ligacoes() {
        // Três colunas ligadas de cima para baixo: nota → código → tabela em cada uma.
        let mut lista = Vec::new();
        for col in 0..3i64 {
            for lin in 0..3i64 {
                lista.push(e(10 * col + lin + 1, TipoElemento::Nota, col as f32 * 300.0, lin as f32 * 300.0, 200.0));
            }
        }
        let mut lig = 100;
        for col in 0..3i64 {
            for lin in 0..2i64 {
                lig += 1;
                lista.push(Elemento { id: lig, tipo: TipoElemento::Ligacao, de: 10 * col + lin + 1, para: 10 * col + lin + 2, ..Default::default() });
            }
        }
        assert_eq!(ordem_do_palco(&lista), vec![1, 2, 3, 11, 12, 13, 21, 22, 23]);
        // Sem ligações, a ordem de leitura (por linhas).
        let sem: Vec<Elemento> = lista.iter().filter(|e| e.tipo != TipoElemento::Ligacao).cloned().collect();
        assert_eq!(ordem_do_palco(&sem), vec![1, 11, 21, 2, 12, 22, 3, 13, 23]);
        // Um ciclo também passa por todos, uma vez cada.
        let mut ciclo = sem.clone();
        ciclo.push(Elemento { id: 200, tipo: TipoElemento::Ligacao, de: 1, para: 11, ..Default::default() });
        ciclo.push(Elemento { id: 201, tipo: TipoElemento::Ligacao, de: 11, para: 1, ..Default::default() });
        let ordem = ordem_do_palco(&ciclo);
        assert_eq!(ordem.len(), 9);
        assert_eq!(&ordem[..2], &[1, 11]);
    }

    #[test]
    fn so_cartoes_vao_ao_palco() {
        assert!(vai_ao_palco(&e(1, TipoElemento::Video, 0.0, 0.0, 10.0)));
        assert!(!vai_ao_palco(&e(1, TipoElemento::Texto, 0.0, 0.0, 10.0)));
        assert!(!vai_ao_palco(&e(1, TipoElemento::Ligacao, 0.0, 0.0, 10.0)));
        let p = Palco::novo(vec![e(1, TipoElemento::Nota, 0.0, 0.0, 10.0), e(2, TipoElemento::Nota, 300.0, 0.0, 10.0)], Some(2), tema::Escolha::Escuro);
        assert_eq!(p.atual, 1);
        assert!(!p.vazio());
    }
}
