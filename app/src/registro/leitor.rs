//! A reunião como leitor: a pauta à esquerda (as tarefas por projeto na
//! daily, por assunto na sprint, na ordem em que você fala) e, à direita,
//! a tarefa escolhida inteira: o tópico, a frase para quem não é dev, os
//! cartões técnicos da montagem e a galeria com as imagens passando com
//! seta. Cada tarefa tem o visto de "apresentada", e a ordem da pauta é
//! sua (a alça arrasta). A mesma tela serve a página e a tela cheia (a
//! apresentação): muda a escala e o que fica em volta.

use std::collections::HashMap;

use eframe::egui::{self, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::comum::{self, EstiloNota, Miniatura, NotaMedida, texto_cortado};
use super::lista::Arrasto;
use super::sprint::Secao;
use super::{Registro, frase};
use crate::api;
use crate::tema::{self, Icone, cores, forte};

pub(super) const PAUTA: f32 = 380.0;
pub(super) const PAUTA_CHEIA: f32 = 320.0;
const ITEM: f32 = 46.0;
const ITEM_COMPACTO: f32 = 36.0;
const GRUPO: f32 = 32.0;
const VISTO: f32 = 18.0;

/// O estado do leitor de uma reunião (a daily tem o dele, a sprint o dela).
#[derive(Default)]
pub(crate) struct Leitor {
    /// A tarefa aberta no painel.
    pub escolhida: Option<i64>,
    /// A imagem da galeria em cada tarefa.
    pub imagem: HashMap<i64, usize>,
    /// Arrastando uma tarefa da pauta: a tarefa, a seção dela e a posição
    /// em que seria solta (índice dentro da seção).
    arrasto: Option<(i64, i64, usize)>,
}

/// O que a pauta e o painel pedem.
pub(crate) enum Pedido {
    Escolher(i64),
    Visto(i64, bool),
    /// A nova ordem das tarefas de uma seção.
    Ordem(Vec<i64>),
    /// Na sprint fixa: a tarefa vai para o assunto (0 tira do assunto).
    Assunto {
        tarefa: i64,
        assunto: i64,
    },
    Clique(frase::Clique, i64),
    /// Uma imagem da galeria, para abrir no visor.
    Imagem {
        tarefa: i64,
        anexo: i64,
        legenda: String,
    },
    /// O menu do cabeçalho de um assunto da sprint fixa.
    RenomearAssunto(i64, String),
    ApagarAssunto(i64),
    EscreverTexto(i64),
    GerarTexto(i64),
}

/// Como a pauta aparece.
pub(crate) struct OpcoesPauta {
    /// Em tela cheia: itens compactos, sem a segunda linha.
    pub compacta: bool,
    /// A ordem pode mudar e, na sprint fixa, a tarefa pode ir a outro assunto.
    pub arrastavel: bool,
    pub assuntos: bool,
    pub daily: bool,
    pub sprint_fixa: bool,
    pub conectado: bool,
    pub recorte: api::Recorte,
}

/// As imagens da tarefa na ordem da galeria: as que a montagem escolheu
/// primeiro (com a legenda dela), depois as outras, sem os vídeos.
pub(super) fn imagens_do_slide(s: &api::Slide) -> Vec<(i64, String)> {
    let mut lista: Vec<(i64, String)> = Vec::new();
    if let Some(m) = &s.montagem {
        for c in &m.cartoes {
            if c.imagem != 0 && s.anexos.iter().any(|a| a.id == c.imagem && !a.video()) && !lista.iter().any(|(id, _)| *id == c.imagem) {
                let legenda = if c.legenda.trim().is_empty() { c.titulo.clone() } else { c.legenda.clone() };
                lista.push((c.imagem, legenda));
            }
        }
    }
    for a in &s.anexos {
        if !a.video() && !lista.iter().any(|(id, _)| *id == a.id) {
            lista.push((a.id, a.nome.clone()));
        }
    }
    lista
}

/// O tópico da tarefa: o da montagem, senão o título.
pub(super) fn topico(s: &api::Slide) -> &str {
    s.montagem.as_ref().map(|m| m.topico.as_str()).filter(|t| !t.trim().is_empty()).unwrap_or(&s.titulo).trim_end_matches('.')
}

/// A frase da tarefa: a da montagem, senão a nota, senão o estado.
fn frase_do(s: &api::Slide) -> String {
    if let Some(m) = s.montagem.as_ref().filter(|m| !m.frase.trim().is_empty()) {
        return m.frase.trim().to_string();
    }
    if !s.nota.trim().is_empty() {
        return s.nota.lines().next().unwrap_or_default().trim().to_string();
    }
    match frase::frase_do_slide(s, false, true) {
        frase::Frase::Pronta(t) => t.lines().next().unwrap_or_default().to_string(),
        frase::Frase::Estado(t) => t.to_string(),
        frase::Frase::Escrevendo => "O agente está escrevendo.".into(),
    }
}

impl Registro {
    /// A pauta: um grupo por seção do deck, com o visto, a bolinha do
    /// estado, o tópico e o número da tarefa. Devolve o que foi pedido.
    pub(super) fn pauta(&mut self, ui: &mut egui::Ui, deck: &api::Deck, secoes: &[Secao], leitor: &mut Leitor, op: &OpcoesPauta) -> Vec<Pedido> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let mut pedidos = Vec::new();
        let alt = if op.compacta { ITEM_COMPACTO } else { ITEM };
        let arrastando = egui::DragAndDrop::payload::<Arrasto>(&ctx).map(|a| a.tarefas[0]);
        let ponteiro = ctx.pointer_hover_pos();
        let mut soltou_em = None;
        for secao in secoes {
            let chave = secao.chave;
            // "Sem assunto" vazio só aparece como lugar de soltar.
            if secao.slides.is_empty() && secao.assunto == Some(0) && arrastando.is_none() {
                continue;
            }
            let feitas = secao.slides.iter().filter(|s| s.apresentada).count();
            let aberta = !self.recolhidas.contains(&chave);
            // O cabeçalho: seta, nome, fio e "feitas de n". Na sprint fixa,
            // recebe uma tarefa arrastada (ela muda de assunto).
            let (rect, cabeca) = ui.allocate_exact_size(vec2(ui.available_width(), GRUPO), Sense::click());
            let sobre_cabeca =
                op.assuntos && arrastando.is_some_and(|t| !secao.slides.iter().any(|s| s.tarefa_id == t)) && ponteiro.is_some_and(|q| rect.contains(q));
            if sobre_cabeca {
                ui.painter().rect(
                    rect,
                    CornerRadius::same(8),
                    p.destaque.gamma_multiply(0.12),
                    Stroke::new(1.0, p.destaque.gamma_multiply(0.6)),
                    StrokeKind::Inside,
                );
            } else {
                tema::fundo_ao_passar(ui, &cabeca, rect, 8, true);
            }
            let giro = ctx.animate_bool_with_time(Id::new(("pauta-seta", chave)), aberta, tema::TRANSICAO);
            comum::seta(ui.painter(), pos2(rect.left() + 12.0, rect.center().y), giro, p.suave);
            let vazio = secao.assunto == Some(0);
            let nome = ui.painter().layout_no_wrap(secao.nome.clone(), forte(13.5), if vazio { p.suave } else { p.texto });
            let largura_nome = nome.size().x;
            ui.painter().galley(pos2(rect.left() + 26.0, rect.center().y - nome.size().y / 2.0), nome, p.texto);
            let conta = if sobre_cabeca { "Soltar aqui".to_string() } else { format!("{feitas} de {}", secao.slides.len()) };
            let g = ui.painter().layout_no_wrap(conta, FontId::proportional(11.5), if sobre_cabeca { p.destaque } else { p.suave });
            let x_conta = rect.right() - 8.0 - g.size().x;
            ui.painter().galley(pos2(x_conta, rect.center().y - g.size().y / 2.0), g, p.suave);
            let inicio = rect.left() + 26.0 + largura_nome + 10.0;
            if x_conta - 8.0 > inicio {
                ui.painter().line_segment([pos2(inicio, rect.center().y), pos2(x_conta - 8.0, rect.center().y)], Stroke::new(1.0, p.borda.gamma_multiply(0.7)));
            }
            if cabeca.clicked() && !self.recolhidas.remove(&chave) {
                self.recolhidas.insert(chave);
            }
            if let Some(assunto) = secao.assunto.filter(|_| op.sprint_fixa) {
                if let Some(a) = cabeca.dnd_release_payload::<Arrasto>()
                    && !secao.slides.iter().any(|s| s.tarefa_id == a.tarefas[0])
                {
                    pedidos.push(Pedido::Assunto { tarefa: a.tarefas[0], assunto });
                }
                if assunto > 0 && op.conectado {
                    let sem_texto = deck.assuntos.iter().any(|a| a.id == assunto && a.texto.trim().is_empty());
                    cabeca.context_menu(|ui| {
                        ui.set_min_width(200.0);
                        if sem_texto && tema::opcao_menu(ui, "Escrever a fala do assunto", false) {
                            pedidos.push(Pedido::EscreverTexto(assunto));
                            ui.close();
                        }
                        if sem_texto && tema::opcao_menu(ui, "Gerar a fala com IA", false) {
                            pedidos.push(Pedido::GerarTexto(assunto));
                            ui.close();
                        }
                        if tema::opcao_menu(ui, "Renomear", false) {
                            pedidos.push(Pedido::RenomearAssunto(assunto, secao.nome.clone()));
                            ui.close();
                        }
                        if tema::opcao_menu(ui, "Apagar assunto", false) {
                            pedidos.push(Pedido::ApagarAssunto(assunto));
                            ui.close();
                        }
                    });
                }
            }
            if !aberta {
                ui.add_space(4.0);
                continue;
            }
            // Na sprint fixa, a fala do assunto (ou a caixa para escrevê-la).
            if let Some(assunto) = secao.assunto.filter(|a| *a > 0 && op.sprint_fixa) {
                egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 6, ..Default::default() }).show(ui, |ui| {
                    self.texto_do_assunto(ui, deck, assunto, op.recorte);
                });
            }
            if secao.slides.is_empty() {
                ui.label(egui::RichText::new("Nenhuma tarefa. Arraste uma para cá.").color(p.suave).size(12.5));
            }
            // Os itens. Durante o arrasto dentro da seção, a linha de soltar
            // aparece entre dois itens.
            let topo_itens = ui.cursor().top();
            let mut alvo = None;
            if let (Some(t), Some(q)) = (arrastando, ponteiro)
                && secao.slides.iter().any(|s| s.tarefa_id == t)
                && op.arrastavel
            {
                let dentro = Rect::from_min_size(pos2(rect.left(), topo_itens), vec2(rect.width(), (alt + 2.0) * secao.slides.len() as f32));
                if dentro.expand2(vec2(0.0, 6.0)).contains(q) {
                    let indice = (((q.y - topo_itens) / (alt + 2.0)).round().max(0.0) as usize).min(secao.slides.len());
                    alvo = Some(indice);
                    leitor.arrasto = Some((t, chave, indice));
                }
            }
            for (i, s) in secao.slides.iter().enumerate() {
                let (r, resposta) = ui.allocate_exact_size(
                    vec2(ui.available_width(), alt),
                    if op.arrastavel && op.conectado && !s.removida { Sense::click_and_drag() } else { Sense::click() },
                );
                ui.add_space(2.0);
                if !ui.is_rect_visible(r) {
                    continue;
                }
                // Nomes estáveis para os testes de foto acharem o item e o visto.
                resposta.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("tarefa {}", s.tarefa_id)));
                let ativa = leitor.escolhida == Some(s.tarefa_id);
                let em_arrasto = arrastando == Some(s.tarefa_id);
                let tom = |c: Color32| if em_arrasto { c.gamma_multiply(0.4) } else { c };
                if ativa {
                    ui.painter().rect(
                        r,
                        CornerRadius::same(10),
                        p.destaque.gamma_multiply(0.13),
                        Stroke::new(1.0, p.destaque.gamma_multiply(0.5)),
                        StrokeKind::Inside,
                    );
                } else {
                    tema::fundo_ao_passar(ui, &resposta, r, 10, !em_arrasto);
                }
                if alvo == Some(i) {
                    let y = r.top() - 1.0;
                    ui.painter().line_segment([pos2(r.left() + 8.0, y), pos2(r.right() - 8.0, y)], Stroke::new(2.0, p.destaque));
                }
                let pintor = ui.painter();
                let meio = r.center().y;
                let por_cima = resposta.hovered() || resposta.contains_pointer();
                let mut x = r.left() + 8.0;
                if op.arrastavel && op.conectado {
                    let t = tema::transicao(ui, resposta.id.with("alca"), por_cima || ativa);
                    if t > 0.0 {
                        alca(pintor, pos2(x + 4.0, meio), p.suave.gamma_multiply(0.8 * t));
                    }
                    x += 14.0;
                }
                // O visto.
                let caixa_visto = Rect::from_center_size(pos2(x + VISTO / 2.0, meio), vec2(VISTO, VISTO));
                let visto = ui.interact(caixa_visto.expand(4.0), resposta.id.with("visto"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                visto.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, true, format!("apresentada {}", s.tarefa_id)));
                desenhar_visto(pintor, caixa_visto, s.apresentada, visto.hovered(), tom);
                if visto.on_hover_text(if s.apresentada { "Desmarcar" } else { "Marcar como apresentada" }).clicked() && op.conectado {
                    pedidos.push(Pedido::Visto(s.tarefa_id, !s.apresentada));
                }
                x += VISTO + 10.0;
                let (cor, _, marca) = comum::estado_do_slide(s);
                tema::marca(pintor, pos2(x + 4.0, if op.compacta { meio } else { meio - 9.0 }), 4.0, tom(cor), marca);
                x += 14.0;
                let direita = r.right() - 10.0;
                let mut largura = direita - x;
                if !s.anexos.iter().all(|a| a.video()) {
                    tema::desenhar_icone(pintor, pos2(direita - 7.0, meio), Icone::Imagem, tom(p.suave));
                    largura -= 20.0;
                }
                let apagado = s.apresentada && !ativa;
                let cor_titulo = tom(if apagado { p.suave } else { p.texto });
                let fonte = if ativa { forte(13.5) } else { FontId::proportional(13.5) };
                let y_titulo = if op.compacta { meio - 9.0 } else { meio - 16.0 };
                let cortou = texto_cortado(pintor, pos2(x, y_titulo), topico(s), fonte.clone(), cor_titulo, largura);
                if apagado {
                    let w = tema::cortar(pintor, topico(s), egui::TextFormat::simple(fonte, cor_titulo), largura, 1, true).size().x;
                    pintor.line_segment([pos2(x, y_titulo + 9.5), pos2(x + w, y_titulo + 9.5)], Stroke::new(1.0, p.suave.gamma_multiply(0.7)));
                }
                if !op.compacta {
                    let sub = format!("#{} · {}", s.tarefa_id, s.titulo.trim_end_matches('.'));
                    texto_cortado(pintor, pos2(x, meio + 3.0), &sub, FontId::proportional(11.5), tom(p.suave), largura);
                }
                let resposta = if cortou { resposta.on_hover_text(topico(s)) } else { resposta };
                if resposta.clicked() {
                    pedidos.push(Pedido::Escolher(s.tarefa_id));
                }
                if resposta.double_clicked() && !s.removida {
                    pedidos.push(Pedido::Clique(frase::Clique::AbrirTarefa, s.tarefa_id));
                }
                if op.arrastavel && op.conectado && !s.removida && resposta.drag_started() {
                    resposta.dnd_set_drag_payload(Arrasto { tarefas: vec![s.tarefa_id], rotulo: topico(s).to_string() });
                    leitor.arrasto = Some((s.tarefa_id, chave, i));
                }
                if resposta.hovered() && op.arrastavel && op.conectado {
                    resposta.clone().on_hover_cursor(egui::CursorIcon::Grab);
                }
                let mut clique = None;
                resposta.context_menu(|ui| {
                    ui.set_min_width(200.0);
                    if op.conectado && !s.removida && tema::opcao_menu(ui, "Editar a nota", false) {
                        clique = Some(frase::Clique::Editar);
                        ui.close();
                    }
                    if !s.removida && tema::opcao_menu(ui, "Abrir a tarefa", false) {
                        clique = Some(frase::Clique::AbrirTarefa);
                        ui.close();
                    }
                    if op.daily && op.conectado && tema::opcao_menu(ui, "Tirar desta daily", false) {
                        clique = Some(frase::Clique::TirarDaDaily);
                        ui.close();
                    }
                    if op.sprint_fixa && op.conectado && tema::opcao_menu(ui, "Tirar desta sprint", false) {
                        clique = Some(frase::Clique::TirarDaSprint);
                        ui.close();
                    }
                    if op.conectado && !s.removida {
                        if tema::opcao_menu(ui, "Remontar para a reunião", false) {
                            clique = Some(frase::Clique::Remontar(false));
                            ui.close();
                        }
                        if tema::opcao_menu(ui, "Remontar com IA", false) {
                            clique = Some(frase::Clique::Remontar(true));
                            ui.close();
                        }
                    }
                });
                if let Some(c) = clique {
                    pedidos.push(Pedido::Clique(c, s.tarefa_id));
                }
            }
            // Soltar dentro da seção: a ordem nova.
            let area = Rect::from_min_max(pos2(rect.left(), topo_itens), pos2(rect.right(), ui.cursor().top()));
            let dentro = ui.interact(area, Id::new(("pauta-soltar", chave)), Sense::hover());
            if let Some(a) = dentro.dnd_release_payload::<Arrasto>()
                && let Some((t, c, indice)) = leitor.arrasto
                && c == chave
                && a.tarefas[0] == t
            {
                soltou_em = Some((chave, t, indice));
            }
            ui.add_space(6.0);
        }
        if let Some((chave, t, indice)) = soltou_em {
            let secao = secoes.iter().find(|s| s.chave == chave);
            if let Some(secao) = secao {
                let mut ordem: Vec<i64> = secao.slides.iter().map(|s| s.tarefa_id).collect();
                let de = ordem.iter().position(|x| *x == t).unwrap_or(0);
                ordem.remove(de);
                let para = if indice > de { indice - 1 } else { indice }.min(ordem.len());
                ordem.insert(para, t);
                if ordem.iter().copied().ne(secao.slides.iter().map(|s| s.tarefa_id)) {
                    pedidos.push(Pedido::Ordem(ordem));
                }
            }
            leitor.arrasto = None;
        }
        if arrastando.is_none() {
            leitor.arrasto = None;
        }
        pedidos
    }

    /// O painel da tarefa escolhida. `escala` aumenta a letra na tela cheia;
    /// `acoes` mostra os botões (só na página).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn painel(
        &mut self,
        ui: &mut egui::Ui,
        s: &api::Slide,
        projeto: &str,
        indice: usize,
        total: usize,
        leitor: &mut Leitor,
        escala: f32,
        acoes: bool,
        op: &OpcoesPauta,
    ) -> Vec<Pedido> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let e = escala;
        let mut pedidos = Vec::new();
        let area = ui.max_rect();
        ui.painter().rect(area, CornerRadius::same(16), p.superficie.gamma_multiply(0.7), Stroke::new(1.0, p.borda.gamma_multiply(0.6)), StrokeKind::Inside);
        let interno = area.shrink2(vec2(20.0 * e, 16.0 * e));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(interno).layout(egui::Layout::top_down(egui::Align::Min)));
        let ui = &mut ui;
        let largura = interno.width();

        // A linha de cima: "3 de 8 · projeto" e, à direita, o estado do visto.
        let (linha, _) = ui.allocate_exact_size(vec2(largura, 18.0 * e), Sense::hover());
        let pintor = ui.painter();
        let mut cima = format!("{indice} de {total}");
        if !projeto.is_empty() {
            cima = format!("{cima} · {projeto}");
        }
        if self.montando.contains(&s.tarefa_id) {
            cima = format!("{cima} · montando com IA…");
        } else if s.montagem.as_ref().is_some_and(|m| m.com_ia()) {
            cima = format!("{cima} · montada com IA");
        }
        pintor.text(linha.left_center(), egui::Align2::LEFT_CENTER, cima, FontId::proportional(12.5 * e), p.suave);
        if s.apresentada {
            let g = pintor.layout_no_wrap("apresentada".into(), FontId::proportional(12.5 * e), p.destaque);
            let x = linha.right() - g.size().x;
            pintor.galley(pos2(x, linha.center().y - g.size().y / 2.0), g, p.destaque);
            tema::desenhar_icone(pintor, pos2(x - 12.0, linha.center().y), Icone::Check, p.destaque);
        } else if !acoes {
            let g = pintor.layout_no_wrap("espaço marca como apresentada".into(), FontId::proportional(12.5 * e), p.suave);
            pintor.galley(pos2(linha.right() - g.size().x, linha.center().y - g.size().y / 2.0), g, p.suave);
        }
        ui.add_space(10.0 * e);

        // O título: o visto grande, o tópico e a linha do estado.
        let (cab, _) = ui.allocate_exact_size(vec2(largura, 56.0 * e), Sense::hover());
        let lado = 24.0 * e;
        let caixa_visto = Rect::from_min_size(pos2(cab.left(), cab.top() + 4.0 * e), vec2(lado, lado));
        let visto =
            ui.interact(caixa_visto.expand(4.0), Id::new(("visto-painel", s.tarefa_id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        visto.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, true, "visto do painel"));
        desenhar_visto(ui.painter(), caixa_visto, s.apresentada, visto.hovered(), |c| c);
        if visto.on_hover_text(if s.apresentada { "Desmarcar" } else { "Marcar como apresentada" }).clicked() && op.conectado {
            pedidos.push(Pedido::Visto(s.tarefa_id, !s.apresentada));
        }
        let x = cab.left() + lado + 12.0 * e;
        let pintor = ui.painter();
        texto_cortado(pintor, pos2(x, cab.top()), topico(s), forte(22.0 * e), p.texto, cab.right() - x);
        let y = cab.top() + 36.0 * e;
        let (cor, nome_estado, marca) = comum::estado_do_slide(s);
        tema::marca(pintor, pos2(x + 4.0, y + 7.0 * e), 4.0 * e, cor, marca);
        let mut xx = x + 14.0 * e;
        let g = pintor.layout_no_wrap(nome_estado.to_string(), FontId::proportional(12.5 * e), p.suave);
        pintor.galley(pos2(xx, y), g.clone(), p.suave);
        xx += g.size().x + 10.0 * e;
        if !projeto.is_empty() {
            xx = tema::etiqueta(pintor, pos2(xx, y), projeto, FontId::monospace(11.0 * e), p.suave).right() + 10.0 * e;
        }
        let sub = format!("#{} · {}", s.tarefa_id, s.titulo.trim_end_matches('.'));
        texto_cortado(pintor, pos2(xx, y), &sub, FontId::proportional(12.5 * e), p.suave, cab.right() - xx);
        ui.add_space(8.0 * e);

        // O corpo rola: a frase, os cartões e a galeria.
        let altura_acoes = if acoes { 44.0 * e } else { 0.0 };
        let corpo = Rect::from_min_max(ui.cursor().left_top(), pos2(interno.right(), interno.bottom() - altura_acoes));
        let imagens = imagens_do_slide(s);
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(corpo).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().id_salt(("painel-leitor", s.tarefa_id)).auto_shrink(false).show(&mut filho, |ui| {
            let largura = ui.available_width() - comum::MARGEM_ROLAGEM;
            // Com imagem, a galeria fica ao lado do texto quando cabe.
            let lado_a_lado = !imagens.is_empty() && largura >= 900.0 * e;
            let largura_texto = if lado_a_lado { largura - 24.0 * e - (largura * 0.42).min(560.0 * e) } else { largura };
            let frase = frase_do(s);
            let g = ui.painter().layout(frase, FontId::proportional(16.0 * e), p.texto, largura_texto.min(760.0 * e));
            let (r, _) = ui.allocate_exact_size(vec2(largura, g.size().y), Sense::hover());
            ui.painter().galley(r.left_top(), g, p.texto);
            ui.add_space(12.0 * e);
            let topo_cartoes = ui.cursor().top();
            let x_cartoes = ui.cursor().left();
            let mut y = topo_cartoes;
            if let Some(m) = &s.montagem {
                for c in m.cartoes.iter().filter(|c| c.imagem == 0) {
                    y = cartao(ui, pos2(x_cartoes, y), largura_texto, c, e) + 8.0 * e;
                }
            }
            if s.montagem.as_ref().is_none_or(|m| m.cartoes.iter().all(|c| c.imagem != 0)) && !s.nota.trim().is_empty() {
                y = cartao(
                    ui,
                    pos2(x_cartoes, y),
                    largura_texto,
                    &api::Cartao { titulo: "Nota".into(), texto: s.nota.trim().to_string(), ..Default::default() },
                    e,
                ) + 8.0 * e;
            }
            let fim_texto = y;
            if !imagens.is_empty() {
                let (x, topo, largura_galeria) = if lado_a_lado {
                    (x_cartoes + largura_texto + 24.0 * e, topo_cartoes, largura - largura_texto - 24.0 * e)
                } else {
                    (x_cartoes, fim_texto + 4.0 * e, largura.min(680.0 * e))
                };
                let altura_galeria = (largura_galeria - 80.0) * 9.0 / 16.0 + 30.0 * e;
                let fim =
                    self.galeria_do_painel(ui, Rect::from_min_size(pos2(x, topo), vec2(largura_galeria, altura_galeria)), s, &imagens, leitor, e, &mut pedidos);
                y = y.max(fim);
            }
            ui.allocate_rect(Rect::from_min_max(pos2(x_cartoes, topo_cartoes), pos2(x_cartoes + largura, y)), Sense::hover());
            ui.add_space(8.0);
        });
        ui.allocate_rect(corpo, Sense::hover());

        // Os botões (só na página).
        if acoes {
            let (faixa, _) = ui.allocate_exact_size(vec2(largura, altura_acoes), Sense::hover());
            let mut barra =
                ui.new_child(egui::UiBuilder::new().max_rect(faixa.shrink2(vec2(0.0, 6.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            barra.spacing_mut().item_spacing.x = 8.0;
            let rotulo = if s.apresentada { "Desmarcar" } else { "Apresentada" };
            if tema::botao_principal(&mut barra, rotulo, op.conectado).clicked() {
                pedidos.push(Pedido::Visto(s.tarefa_id, !s.apresentada));
            }
            if tema::botao_secundario_com(&mut barra, "Editar a nota", op.conectado && !s.removida).clicked() {
                pedidos.push(Pedido::Clique(frase::Clique::Editar, s.tarefa_id));
            }
            if tema::botao_secundario_com(&mut barra, "Abrir a tarefa", !s.removida).clicked() {
                pedidos.push(Pedido::Clique(frase::Clique::AbrirTarefa, s.tarefa_id));
            }
            barra.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if op.daily && tema::link_com(ui, "Tirar desta daily", op.conectado).clicked() {
                    pedidos.push(Pedido::Clique(frase::Clique::TirarDaDaily, s.tarefa_id));
                }
                if op.sprint_fixa && tema::link_com(ui, "Tirar desta sprint", op.conectado).clicked() {
                    pedidos.push(Pedido::Clique(frase::Clique::TirarDaSprint, s.tarefa_id));
                }
            });
        }
        let _ = ctx;
        pedidos
    }

    /// A galeria: a imagem da vez, as setas, a legenda e "2 de 3". Devolve
    /// o fundo do que desenhou.
    #[allow(clippy::too_many_arguments)]
    fn galeria_do_painel(
        &mut self,
        ui: &mut egui::Ui,
        caixa: Rect,
        s: &api::Slide,
        imagens: &[(i64, String)],
        leitor: &mut Leitor,
        e: f32,
        pedidos: &mut Vec<Pedido>,
    ) -> f32 {
        let p = cores();
        let ctx = ui.ctx().clone();
        let n = imagens.len();
        let indice = leitor.imagem.get(&s.tarefa_id).copied().unwrap_or(0).min(n - 1);
        let (anexo, legenda) = &imagens[indice];
        let seta = 30.0;
        let moldura = Rect::from_min_max(pos2(caixa.left() + seta + 10.0, caixa.top()), pos2(caixa.right() - seta - 10.0, caixa.bottom() - 30.0 * e));
        let resposta = ui.interact(moldura, Id::new(("galeria", s.tarefa_id, *anexo)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().rect(moldura, CornerRadius::same(10), p.superficie, Stroke::new(1.0, p.borda), StrokeKind::Inside);
        match self.imagens.pedir(&ctx, *anexo) {
            Miniatura::Pronta(t) => {
                let tamanho = t.size_vec2();
                let escala = (moldura.width() / tamanho.x).min(moldura.height() / tamanho.y);
                let dentro = Rect::from_center_size(moldura.center(), tamanho * escala);
                egui::Image::new(t).corner_radius(CornerRadius::same(8)).paint_at(ui, dentro);
            }
            Miniatura::Falhou => {
                ui.painter().text(moldura.center(), egui::Align2::CENTER_CENTER, "imagem indisponível", FontId::proportional(13.0), p.suave);
            }
            Miniatura::Carregando => {}
        }
        if resposta.clicked() {
            pedidos.push(Pedido::Imagem { tarefa: s.tarefa_id, anexo: *anexo, legenda: legenda.clone() });
        }
        // As setas.
        let mut ir = None;
        for (lado, icone, pode, delta) in [(caixa.left(), Icone::Anterior, indice > 0, -1i64), (caixa.right() - seta, Icone::Proximo, indice + 1 < n, 1)] {
            let r = Rect::from_min_size(pos2(lado, moldura.center().y - seta / 2.0), vec2(seta, seta));
            let b = ui.interact(r, Id::new(("galeria-seta", s.tarefa_id, delta)), Sense::click());
            b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, if delta < 0 { "imagem anterior" } else { "próxima imagem" }));
            let cor = if pode { if b.hovered() { p.texto } else { p.suave } } else { p.suave.gamma_multiply(0.35) };
            ui.painter().circle(r.center(), seta / 2.0, p.superficie_alta, Stroke::new(1.0, p.borda));
            tema::desenhar_icone(ui.painter(), r.center(), icone, cor);
            if pode && b.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                ir = Some(delta);
            }
        }
        if let Some(delta) = ir {
            leitor.imagem.insert(s.tarefa_id, (indice as i64 + delta).max(0) as usize);
        }
        // A legenda e a conta.
        let y = moldura.bottom() + 8.0 * e;
        let conta = ui.painter().layout_no_wrap(format!("{} de {n}", indice + 1), FontId::proportional(12.0 * e), p.suave);
        let x_conta = moldura.right() - conta.size().x;
        ui.painter().galley(pos2(x_conta, y), conta, p.suave);
        texto_cortado(ui.painter(), pos2(moldura.left(), y), legenda, FontId::proportional(12.5 * e), p.texto, x_conta - moldura.left() - 12.0);
        caixa.bottom()
    }
}

/// Um cartão da montagem: o título em caixa alta e o texto com os itens.
/// Devolve o fundo.
fn cartao(ui: &mut egui::Ui, pos: egui::Pos2, largura: f32, c: &api::Cartao, e: f32) -> f32 {
    let p = cores();
    let falta = c.titulo.eq_ignore_ascii_case("falta");
    let estilo = EstiloNota {
        fonte: 13.5 * e,
        cor: p.texto,
        marcador: p.suave,
        largura: largura - 28.0 * e,
        altura_linha: 21.0 * e,
        recuo: 18.0 * e,
        vao_item: 3.0 * e,
        vao_bloco: 6.0 * e,
    };
    let nota = NotaMedida::nova(ui.painter(), c.texto.trim(), estilo);
    // Um título que é o começo do texto (a montagem simples corta o item na
    // vírgula) não se repete.
    let titulo = c.titulo.trim();
    let com_titulo = !titulo.is_empty() && !c.texto.trim().to_lowercase().starts_with(&titulo.to_lowercase());
    let altura_titulo = if com_titulo { 18.0 * e } else { 0.0 };
    let altura = 10.0 * e + altura_titulo + nota.altura + 10.0 * e;
    let r = Rect::from_min_size(pos, vec2(largura, altura));
    let (fundo, borda) = if falta { (p.erro.gamma_multiply(0.08), p.erro.gamma_multiply(0.3)) } else { (Color32::from_white_alpha(6), p.borda) };
    ui.painter().rect(r, CornerRadius::same(10), fundo, Stroke::new(1.0, borda), StrokeKind::Inside);
    let x = r.left() + 14.0 * e;
    if com_titulo {
        texto_cortado(
            ui.painter(),
            pos2(x, r.top() + 9.0 * e),
            &titulo.to_uppercase(),
            forte(11.0 * e),
            if falta { p.erro } else { p.suave },
            largura - 28.0 * e,
        );
    }
    nota.pintar(ui.painter(), pos2(x, r.top() + 10.0 * e + altura_titulo));
    ui.allocate_rect(r, Sense::hover());
    r.bottom()
}

/// O visto de apresentada: um círculo vazio que vira check.
fn desenhar_visto(pintor: &egui::Painter, caixa: Rect, feito: bool, hover: bool, tom: impl Fn(Color32) -> Color32) {
    let p = cores();
    let r = caixa.width() / 2.0;
    if feito {
        pintor.circle(caixa.center(), r, tom(p.destaque.gamma_multiply(0.2)), Stroke::new(1.5, tom(p.destaque)));
        tema::desenhar_icone(pintor, caixa.center(), Icone::Check, tom(p.destaque));
    } else {
        let cor = if hover { p.destaque.gamma_multiply(0.8) } else { p.borda };
        pintor.circle(caixa.center(), r, Color32::TRANSPARENT, Stroke::new(1.5, tom(cor)));
    }
}

/// A alça de arrastar: seis pontinhos.
fn alca(pintor: &egui::Painter, centro: egui::Pos2, cor: Color32) {
    for dy in [-5.0, 0.0, 5.0] {
        for dx in [-2.0, 2.0] {
            pintor.circle_filled(centro + vec2(dx, dy), 1.1, cor);
        }
    }
}

/// A barrinha "2 de 8 apresentadas".
pub(super) fn progresso(ui: &mut egui::Ui, feitas: usize, total: usize) {
    let p = cores();
    let texto = format!("{feitas} de {total} apresentadas");
    let g = ui.painter().layout_no_wrap(texto, FontId::proportional(12.5), p.suave);
    let (r, _) = ui.allocate_exact_size(vec2(90.0 + 8.0 + g.size().x, 20.0), Sense::hover());
    let barra = Rect::from_min_size(pos2(r.left(), r.center().y - 2.5), vec2(90.0, 5.0));
    ui.painter().rect_filled(barra, CornerRadius::same(3), p.realce);
    if total > 0 {
        let cheia = Rect::from_min_size(barra.min, vec2(90.0 * feitas as f32 / total as f32, 5.0));
        ui.painter().rect_filled(cheia, CornerRadius::same(3), p.destaque);
    }
    ui.painter().galley(pos2(barra.right() + 8.0, r.center().y - g.size().y / 2.0), g, p.suave);
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn galeria_segue_a_montagem_e_depois_os_anexos() {
        let s = api::Slide {
            anexos: vec![
                api::AnexoSlide { id: 1, tipo: "imagem".into(), nome: "a.png".into(), ..Default::default() },
                api::AnexoSlide { id: 2, tipo: "video".into(), nome: "b.webm".into(), ..Default::default() },
                api::AnexoSlide { id: 3, tipo: "imagem".into(), nome: "c.png".into(), ..Default::default() },
            ],
            montagem: Some(api::Montagem {
                cartoes: vec![api::Cartao { titulo: "Como ficou".into(), imagem: 3, legenda: "A tela nova".into(), ..Default::default() }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let lista = imagens_do_slide(&s);
        assert_eq!(lista, vec![(3, "A tela nova".to_string()), (1, "a.png".to_string())]);
        assert_eq!(topico(&api::Slide { titulo: "Título.".into(), ..Default::default() }), "Título");
    }
}
