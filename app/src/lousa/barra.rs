//! As peças que flutuam sobre a lousa (não escalam com o zoom): a barra de
//! ferramentas, o zoom, a barra da seleção, o editor de texto com a barra de
//! formatação, a busca de tarefas, o menu de contexto e o estado vazio.

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Key, Modifiers, Order, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::desenho::{self, FAIXA, RECUO, caixa_quadro};
use super::modelo::{Comando, Mudou};
use super::{Acao, Contexto, Lousa, camera};
use crate::api::{ElementoLousa as Elemento, TipoElemento};
use crate::tema::{self, CODIGO, Icone, cores, forte};

/// O texto de um item e o título/rótulo, como o núcleo limita.
pub const MAX_TEXTO: usize = 8000;
pub const MAX_TITULO: usize = 120;
/// O contador aparece a partir daqui.
const AVISO_TEXTO: usize = 7500;

/// O pedido que o chip do estado vazio põe na caixa de mensagem (sem enviar).
pub const PEDIDO_FLUXO: &str = "Monte na lousa da tarefa um diagrama do fluxo desta tarefa, com notas curtas e ligações.";

/// A busca do cartão de tarefa (K ou o botão).
pub struct Busca {
    pub texto: String,
    pub escolhida: usize,
    /// Onde o cartão nasce (no quadro).
    pub posicao: Pos2,
    focar: bool,
}

impl Busca {
    pub fn nova(posicao: Pos2) -> Busca {
        Busca { texto: String::new(), escolhida: 0, posicao, focar: true }
    }
}

/// O texto que vai para a área de transferência do sistema ao copiar itens:
/// os textos das notas (assim colar fora da Colmeia também serve).
pub fn texto_da_area(itens: &[Elemento]) -> String {
    let textos: Vec<&str> = itens.iter().filter(|e| e.tipo.de_texto() && !e.texto.is_empty()).map(|e| e.texto.as_str()).collect();
    if textos.is_empty() { format!("{} itens da lousa", itens.len()) } else { textos.join("\n\n") }
}

/// O seletor de arquivos do sistema, numa thread; os arquivos voltam como soltos.
pub fn escolher_arquivos(l: &Lousa, ctx: &egui::Context, video: bool) {
    let chave = Id::new(("lousa-arquivos", l.dono));
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let (titulo, filtro, extensoes): (&str, &str, &[&str]) =
            if video { ("Inserir vídeo", "Vídeos", &["mp4", "webm", "mkv", "mov"]) } else { ("Inserir imagem", "Imagens", &["png", "jpg", "jpeg"]) };
        let escolhidos = rfd::FileDialog::new().set_title(titulo).add_filter(filtro, extensoes).pick_files().unwrap_or_default();
        if !escolhidos.is_empty() {
            ctx.data_mut(|d| d.insert_temp(chave, escolhidos));
            ctx.request_repaint();
        }
    });
}

/// A faixa "só para leitura" da própria lousa: sem o núcleo, e só quando a
/// faixa do núcleo desconectado não está no topo da janela (uma basta).
pub fn com_faixa(c: &Contexto) -> bool {
    !c.pode_mudar && !c.faixa_global
}

/// O topo das peças de cima (abaixo da faixa, quando ela aparece).
fn topo(c: &Contexto) -> f32 {
    if com_faixa(c) { 44.0 } else { 12.0 }
}

/// Altura da barra de ferramentas (botões de 32 e a moldura de 6).
const ALTURA_FERRAMENTAS: f32 = 44.0;

fn moldura() -> egui::Frame {
    tema::moldura_flutuante().inner_margin(egui::Margin::same(6))
}

/// O campo de texto das peças flutuantes (o título na barra da seleção, a
/// busca de tarefa): a moldura do `campo` do app, 28 de altura e recuo de 10.
fn campo_da_barra(ui: &mut egui::Ui, texto: &mut String, dica: &str, largura: f32, id: Option<Id>, limite: usize, letra: f32) -> egui::Response {
    let p = cores();
    let fundo = if tema::claro() { p.superficie_alta } else { p.superficie };
    egui::Frame::new()
        .fill(fundo)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 18.0;
            let mut campo = egui::TextEdit::singleline(texto)
                .frame(egui::Frame::NONE)
                .desired_width(largura)
                .char_limit(limite)
                .font(FontId::proportional(letra))
                .hint_text(RichText::new(dica).color(p.suave).size(letra));
            if let Some(id) = id {
                campo = campo.id(id);
            }
            ui.add(campo)
        })
        .inner
}

fn separador(ui: &mut egui::Ui) {
    ui.add_space(2.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 16.0), Sense::hover());
    ui.painter().rect_filled(r, 0, cores().borda);
    ui.add_space(2.0);
}

/// Tudo o que flutua sobre a lousa.
pub fn mostrar(l: &mut Lousa, ui: &mut egui::Ui, area: Rect, c: &Contexto) -> Vec<Acao> {
    let ctx = ui.ctx().clone();
    let mut acoes = Vec::new();
    acoes.extend(ferramentas(l, &ctx, area, c));
    zoom(l, &ctx, area);
    acoes.extend(apresentar(l, &ctx, area, c));
    if l.edicao.is_some() {
        editor(l, ui, area, c);
    } else if l.arrasto.is_none() && !l.selecao.is_empty() {
        acoes.extend(barra_da_selecao(l, &ctx, area, c));
    } else {
        l.titulo = None;
    }
    if l.busca.is_some() {
        busca(l, &ctx, area, c);
    }
    if l.modelo.elementos.is_empty() && l.envios.is_empty() {
        acoes.extend(vazio(l, &ctx, area, c));
    }
    envios_com_erro(l, ui, c);
    acoes
}

fn ferramentas(l: &mut Lousa, ctx: &egui::Context, area: Rect, c: &Contexto) -> Vec<Acao> {
    let mut acoes = Vec::new();
    let pode = c.pode_mudar;
    let mut criar = None;
    let mut ligar = false;
    let mut arquivo = None;
    let mut tarefa = false;
    // Sem o núcleo, a faixa "só para leitura" ocupa os 32 de cima: a barra desce.
    let topo = topo(c);
    egui::Area::new(Id::new(("lousa-ferramentas", l.dono)))
        .order(Order::Foreground)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(area.center_top() + vec2(0.0, topo))
        .show(ctx, |ui| {
            moldura().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (icone, tipo, dica) in [
                        (Icone::Nota, TipoElemento::Nota, "Nota (N)"),
                        (Icone::Texto, TipoElemento::Texto, "Texto (T)"),
                        (Icone::Codigo, TipoElemento::Codigo, "Código (C)"),
                    ] {
                        if tema::botao_icone_com(ui, icone, 32.0, pode).on_hover_text(dica).clicked() {
                            criar = Some(tipo);
                        }
                    }
                    separador(ui);
                    if tema::botao_icone_com(ui, Icone::Imagem, 32.0, pode).on_hover_text("Imagem (I)").clicked() {
                        arquivo = Some(false);
                    }
                    if tema::botao_icone_com(ui, Icone::Video, 32.0, pode).on_hover_text("Vídeo").clicked() {
                        arquivo = Some(true);
                    }
                    if tema::botao_icone_com(ui, Icone::Tarefa, 32.0, pode).on_hover_text("Cartão de tarefa (K)").clicked() {
                        tarefa = true;
                    }
                    separador(ui);
                    let dois = l.selecao.iter().filter(|id| l.modelo.buscar(**id).is_some_and(|e| e.tipo != TipoElemento::Ligacao)).count() == 2;
                    let r = tema::botao_icone_com(ui, Icone::Ligacao, 32.0, pode && dois);
                    let r = if dois { r.on_hover_text("Ligar o primeiro item ao segundo") } else { r.on_hover_text("Selecione dois itens (Shift+clique)") };
                    ligar = r.clicked();
                });
            });
        });
    let centro = l.centro_da_vista();
    if let Some(tipo) = criar {
        l.criar(ctx, tipo, centro, |_| {});
        l.ativa = true;
    }
    if let Some(video) = arquivo {
        escolher_arquivos(l, ctx, video);
    }
    if tarefa {
        l.busca = Some(Busca::nova(centro));
    }
    if ligar
        && let [de, para] = l.selecao.iter().copied().filter(|id| l.modelo.buscar(*id).is_some_and(|e| e.tipo != TipoElemento::Ligacao)).collect::<Vec<_>>()[..]
    {
        l.ligar(ctx, de, para);
    }
    let _ = &mut acoes;
    acoes
}

fn zoom(l: &mut Lousa, ctx: &egui::Context, area: Rect) {
    let p = cores();
    egui::Area::new(Id::new(("lousa-zoom", l.dono)))
        .order(Order::Foreground)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(area.right_bottom() - vec2(12.0, 12.0))
        .show(ctx, |ui| {
            tema::moldura_flutuante().inner_margin(egui::Margin::symmetric(4, 4)).corner_radius(CornerRadius::same(18)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let centro = l.area.center();
                    if tema::botao_icone(ui, Icone::Menos, 28.0).on_hover_text("Diminuir (Ctrl+rodinha)").clicked() {
                        l.camera.passo_de_zoom(l.area, centro, -1);
                    }
                    let (r, resposta) = ui.allocate_exact_size(vec2(52.0, 28.0), Sense::click());
                    let resposta =
                        resposta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("100% (Shift+0) · Ajustar (Shift+1) · Seleção (Shift+2)");
                    if resposta.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
                    }
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, l.camera.porcentagem(), FontId::proportional(13.0), p.texto);
                    if resposta.clicked() {
                        l.zoom_100();
                    }
                    if tema::botao_icone(ui, Icone::MaisZoom, 28.0).on_hover_text("Aumentar (Ctrl+rodinha)").clicked() {
                        l.camera.passo_de_zoom(l.area, centro, 1);
                    }
                    separador(ui);
                    if tema::botao_icone(ui, Icone::Ajustar, 28.0).on_hover_text("Ajustar à tela (Shift+1)").clicked() {
                        l.ajustar_tudo();
                    }
                });
            });
        });
}

fn apresentar(l: &mut Lousa, ctx: &egui::Context, area: Rect, c: &Contexto) -> Vec<Acao> {
    let mut acoes = Vec::new();
    let cartoes = l.cartoes();
    // Centrado na altura da barra de ferramentas, na mesma faixa.
    let meio = topo(c) + ALTURA_FERRAMENTAS / 2.0;
    egui::Area::new(Id::new(("lousa-apresentar", l.dono)))
        .order(Order::Foreground)
        .pivot(Align2::RIGHT_CENTER)
        .fixed_pos(area.right_top() + vec2(-12.0, meio))
        .show(ctx, |ui| {
            let r = tema::botao_secundario_com(ui, "Apresentar", cartoes > 0);
            let r = if cartoes > 0 {
                r.on_hover_text("F5 · Shift+F5 começa pelo item selecionado")
            } else {
                r.on_hover_text("A lousa não tem cartões para apresentar")
            };
            if r.clicked() {
                acoes.push(Acao::Apresentar(None));
            }
        });
    acoes
}

/// A caixa da seleção na tela (itens; uma ligação sozinha usa o meio dela).
fn caixa_da_selecao(l: &Lousa) -> Option<Rect> {
    let itens = l.selecao.iter().filter_map(|id| l.modelo.buscar(*id));
    let mut caixas = Vec::new();
    for e in itens {
        if e.tipo == TipoElemento::Ligacao {
            let (Some(de), Some(para)) = (l.modelo.buscar(e.de), l.modelo.buscar(e.para)) else { continue };
            let pontos = desenho::pontos_na_tela(l.area, &l.camera, desenho::curva(caixa_quadro(de), caixa_quadro(para)));
            caixas.push(Rect::from_center_size(desenho::meio(&pontos), vec2(2.0, 2.0)));
        } else {
            caixas.push(l.camera.retangulo_na_tela(l.area, desenho::caixa_de_toque(e)));
        }
    }
    camera::envolver(caixas)
}

/// Margem das peças flutuantes até a borda da lousa.
const MARGEM: f32 = 12.0;
/// Espaço da barra da seleção até a seleção: a alça de ligação de cima
/// (a 20 da borda) fica livre.
const VAO_DA_BARRA: f32 = 32.0;

/// Põe a barra acima da caixa (abaixo, se não couber), presa à esquerda da
/// caixa: trocar o tipo muda a largura sem mexer nos botões da esquerda.
/// `largura`: a maior que a barra já teve para esta seleção (perto da borda
/// direita, ela já começa onde a mais larga cabe).
fn lugar_da_barra(area: Rect, caixa: Rect, altura: f32, largura: f32) -> (Pos2, Align2) {
    let x = caixa.left().min(area.right() - MARGEM - largura).clamp(area.left() + MARGEM, (area.right() - MARGEM).max(area.left() + MARGEM));
    if caixa.top() - VAO_DA_BARRA - altura > area.top() + 64.0 {
        (pos2(x, caixa.top() - VAO_DA_BARRA), Align2::LEFT_BOTTOM)
    } else {
        (pos2(x, (caixa.bottom() + VAO_DA_BARRA).min(area.bottom() - altura - 60.0)), Align2::LEFT_TOP)
    }
}

/// Círculos das seis cores; devolve a cor clicada.
fn cores_da_nota(ui: &mut egui::Ui, atual: Option<&str>) -> Option<String> {
    let p = cores();
    let mut escolhida = None;
    for (chave, nome, _) in tema::NOTA_BASE {
        let (r, resposta) = ui.allocate_exact_size(vec2(24.0, 28.0), Sense::click());
        let c = tema::cor_nota(chave);
        let centro = r.center();
        ui.painter().circle(centro, 9.0, c.fundo, Stroke::new(1.0, tema::borda_da_bolinha(&c)));
        if atual == Some(chave) {
            ui.painter().circle_stroke(centro, 13.0, Stroke::new(2.0, p.texto));
        } else if resposta.hovered() {
            ui.painter().circle_stroke(centro, 12.0, Stroke::new(1.0, p.borda));
        }
        if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(nome).clicked() {
            escolhida = Some(chave.to_string());
        }
    }
    escolhida
}

fn barra_da_selecao(l: &mut Lousa, ctx: &egui::Context, area: Rect, c: &Contexto) -> Vec<Acao> {
    let mut acoes = Vec::new();
    let Some(caixa) = caixa_da_selecao(l) else { return acoes };
    if !c.pode_mudar {
        return acoes;
    }
    let ids = l.selecao.clone();
    let itens: Vec<Elemento> = ids.iter().filter_map(|id| l.modelo.buscar(*id).cloned()).collect();
    let um = if let [e] = &itens[..] { Some(e.clone()) } else { None };
    let largura_antes = l.largura_barra.as_ref().filter(|(s, _)| *s == l.selecao).map_or(0.0, |(_, w)| *w);
    let (pos, pivo) = lugar_da_barra(area, caixa, 40.0, largura_antes);
    enum Pedido {
        Tipo(TipoElemento),
        Cor(String),
        Duplicar,
        Apagar,
        Abrir(i64),
        Video(i64, i64),
        Rotulo(i64),
        Inverter(i64),
        Frente,
        Tras,
        Copiar,
        Ligar(i64, i64),
    }
    let mut pedido = None;
    let mut titulo_mudou = false;
    let mut titulo_soltou = false;
    // O título (nota) ou a legenda (imagem) em edição na barra.
    match &um {
        Some(e) if matches!(e.tipo, TipoElemento::Nota | TipoElemento::Imagem | TipoElemento::Codigo) => {
            if l.titulo.as_ref().is_none_or(|(id, _, _)| *id != e.id) {
                l.titulo = Some((e.id, e.titulo.clone(), e.clone()));
            }
        }
        _ => l.titulo = None,
    }
    let mut titulo = l.titulo.take();
    let lugar = egui::Area::new(Id::new(("lousa-selecao", l.dono))).order(Order::Foreground).pivot(pivo).fixed_pos(pos).constrain_to(area.shrink(MARGEM));
    let barra = lugar.show(ctx, |ui| {
        moldura().show(ui, |ui| {
            ui.horizontal(|ui| {
                // 28 de conteúdo e 6 de moldura: a barra tem 40.
                ui.set_height(28.0);
                ui.spacing_mut().item_spacing.x = 4.0;
                // O separador só vai entre dois grupos com conteúdo.
                let inicio = ui.cursor().min.x;
                let tem_antes = |ui: &egui::Ui| ui.cursor().min.x > inicio + 0.5;
                match &um {
                    Some(e) if e.tipo.de_texto() => {
                        let atual = match e.tipo {
                            TipoElemento::Nota => 0,
                            TipoElemento::Texto => 1,
                            _ => 2,
                        };
                        if let Some(i) = tema::segmentado(ui, &["Nota", "Texto", "Código"], atual) {
                            pedido = Some(Pedido::Tipo([TipoElemento::Nota, TipoElemento::Texto, TipoElemento::Codigo][i]));
                        }
                        if e.tipo == TipoElemento::Nota {
                            separador(ui);
                            if let Some(cor) = cores_da_nota(ui, Some(&e.cor)) {
                                pedido = Some(Pedido::Cor(cor));
                            }
                        }
                    }
                    Some(e) if e.tipo == TipoElemento::Video => {
                        if tema::botao_secundario(ui, "Abrir no reprodutor").clicked() {
                            pedido = Some(Pedido::Video(e.id, e.anexo_id));
                        }
                    }
                    Some(e) if e.tipo == TipoElemento::Tarefa && e.tarefa_ref != 0 => {
                        if tema::botao_secundario(ui, "Abrir tarefa").clicked() {
                            pedido = Some(Pedido::Abrir(e.tarefa_ref));
                        }
                    }
                    Some(e) if e.tipo == TipoElemento::Ligacao => {
                        if tema::botao_secundario(ui, "Rótulo").clicked() {
                            pedido = Some(Pedido::Rotulo(e.id));
                        }
                        if tema::botao_secundario(ui, "Inverter").clicked() {
                            pedido = Some(Pedido::Inverter(e.id));
                        }
                    }
                    None => {
                        if itens.iter().any(|e| e.tipo == TipoElemento::Nota)
                            && let Some(cor) = cores_da_nota(ui, None)
                        {
                            pedido = Some(Pedido::Cor(cor));
                        }
                        let normais: Vec<i64> = itens.iter().filter(|e| e.tipo != TipoElemento::Ligacao).map(|e| e.id).collect();
                        if let [a, b] = normais[..] {
                            separador(ui);
                            if tema::botao_secundario(ui, "Ligar").on_hover_text("Liga o primeiro selecionado ao segundo").clicked() {
                                pedido = Some(Pedido::Ligar(a, b));
                            }
                        }
                    }
                    _ => {}
                }
                if let Some((_, texto, _)) = &mut titulo {
                    if tem_antes(ui) {
                        separador(ui);
                    }
                    let dica = if um.as_ref().is_some_and(|e| e.tipo == TipoElemento::Imagem) { "Legenda" } else { "Adicionar título" };
                    let r = campo_da_barra(ui, texto, dica, 160.0, None, MAX_TITULO, 13.0);
                    titulo_mudou = r.changed();
                    titulo_soltou = r.lost_focus();
                }
                if tem_antes(ui) {
                    separador(ui);
                }
                if um.as_ref().is_none_or(|e| e.tipo != TipoElemento::Ligacao && e.tipo != TipoElemento::Video && e.tipo != TipoElemento::Tarefa)
                    && tema::botao_icone(ui, Icone::Duplicar, 28.0).on_hover_text("Duplicar (Ctrl+D)").clicked()
                {
                    pedido = Some(Pedido::Duplicar);
                }
                if tema::botao_icone(ui, Icone::Apagar, 28.0).on_hover_text("Apagar (Delete)").clicked() {
                    pedido = Some(Pedido::Apagar);
                }
                let mais = tema::botao_icone(ui, Icone::Mais, 28.0).on_hover_text("Mais");
                egui::Popup::menu(&mais).show(|ui| {
                    ui.set_min_width(220.0);
                    if tema::opcao_menu_com(ui, "Trazer para frente", None, true) {
                        pedido = Some(Pedido::Frente);
                        ui.close();
                    }
                    if tema::opcao_menu_com(ui, "Enviar para trás", None, true) {
                        pedido = Some(Pedido::Tras);
                        ui.close();
                    }
                    if tema::opcao_menu_com(ui, "Copiar", Some("Ctrl+C"), true) {
                        pedido = Some(Pedido::Copiar);
                        ui.close();
                    }
                });
            });
        });
    });
    let largura = barra.response.rect.width();
    if largura > largura_antes + 0.5 {
        // Mais larga que antes: no próximo quadro ela já nasce onde cabe.
        l.largura_barra = Some((l.selecao.clone(), largura));
        ctx.request_repaint();
    }
    // O título muda na hora; o desfazer guarda a edição inteira ao sair do campo.
    if let Some((id, texto, antes)) = &titulo {
        if titulo_mudou {
            let t = texto.clone();
            l.modelo.mudar(*id, Mudou::TITULO, |e| e.titulo = t);
            let agora = ctx.input(|i| i.time);
            l.agendar(ctx, agora + super::ESPERA_TEXTO);
        }
        if titulo_soltou {
            let depois = l.modelo.buscar(*id).cloned();
            l.modelo.registrar(Comando { numero: 0, pares: vec![(Some(antes.clone()), depois.clone())] });
            if let Some(d) = depois {
                titulo = Some((*id, d.titulo.clone(), d));
            }
            let agora = ctx.input(|i| i.time);
            l.agendar(ctx, agora);
        }
    }
    l.titulo = titulo;
    match pedido {
        Some(Pedido::Tipo(tipo)) => l.mudar_varios(ctx, &ids, Mudou::TIPO, |e| e.tipo = tipo),
        Some(Pedido::Cor(cor)) => {
            let notas: Vec<i64> = itens.iter().filter(|e| e.tipo == TipoElemento::Nota).map(|e| e.id).collect();
            l.mudar_varios(ctx, &notas, Mudou::COR, |e| e.cor = cor.clone());
        }
        Some(Pedido::Duplicar) => {
            let copia = l.copiar_selecao();
            l.colar_itens(ctx, &copia, Vec2::splat(super::CASCATA));
        }
        Some(Pedido::Apagar) => acoes.extend(l.apagar_selecao(ctx)),
        Some(Pedido::Abrir(tarefa)) => acoes.push(Acao::AbrirTarefa(tarefa)),
        Some(Pedido::Video(id, anexo)) => {
            let agora = ctx.input(|i| i.time);
            l.abrir_video(ctx, id, anexo, agora);
        }
        Some(Pedido::Rotulo(id)) => l.editar(id),
        Some(Pedido::Inverter(id)) => {
            if let Some(e) = l.modelo.buscar(id).cloned() {
                l.mudar_varios(ctx, &[id], Mudou::PONTAS, |x| (x.de, x.para) = (e.para, e.de));
            }
        }
        Some(Pedido::Frente) => {
            let z = l.modelo.z_maximo();
            let ids: Vec<i64> = ids.clone();
            for (n, id) in ids.iter().enumerate() {
                l.mudar_varios(ctx, &[*id], Mudou::Z, |e| e.z = z + 1 + n as i64);
            }
        }
        Some(Pedido::Tras) => {
            let z = l.modelo.z_minimo();
            for (n, id) in ids.iter().enumerate() {
                l.mudar_varios(ctx, &[*id], Mudou::Z, |e| e.z = z - 1 - n as i64);
            }
        }
        Some(Pedido::Copiar) => {
            let itens = l.copiar_selecao();
            let texto = texto_da_area(&itens);
            ctx.copy_text(texto.clone());
            ctx.data_mut(|d| d.insert_temp(Id::new("lousa-area-interna"), (texto, itens)));
        }
        Some(Pedido::Ligar(a, b)) => l.ligar(ctx, a, b),
        None => {}
    }
    acoes
}

/// O editor do texto aberto: no lugar do conteúdo do item, sem moldura (o
/// papel e a faixa do título são os do cartão) ou, com a letra pequena
/// demais, flutuando com o canto preso ao do item (no mínimo 13 px e 280 de
/// largura), dentro da lousa.
fn editor(l: &mut Lousa, ui: &mut egui::Ui, area: Rect, c: &Contexto) {
    let ctx = ui.ctx().clone();
    let p = cores();
    let Some(ed) = &l.edicao else { return };
    let Some(e) = l.modelo.buscar(ed.id).cloned() else {
        l.edicao = None;
        return;
    };
    if !c.pode_mudar {
        let agora = ctx.input(|i| i.time);
        l.encerrar_edicao(&ctx, agora);
        return;
    }
    let z = l.camera.zoom;
    let r = l.camera.retangulo_na_tela(area, caixa_quadro(&e));
    let id = Id::new(("lousa-editor", l.dono, ed.sessao));
    let com_foco = ctx.memory(|m| m.has_focus(id));
    // Ctrl+Enter sai da edição (antes de o campo ver o Enter).
    let sair = com_foco && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
    let codigo = e.tipo == TipoElemento::Codigo;
    let t = desenho::tinta(&e);
    let (fonte, cor, papel) = if codigo {
        (12.5, CODIGO.texto, CODIGO.fundo)
    } else if e.tipo == TipoElemento::Nota {
        (14.0, t.texto, t.papel)
    } else {
        (14.0, p.texto, p.superficie_alta)
    };
    let faixa = if e.titulo.is_empty() || e.tipo == TipoElemento::Texto { 0.0 } else { FAIXA * z };
    let recuo = if codigo {
        12.0
    } else if e.tipo == TipoElemento::Nota {
        RECUO
    } else {
        0.0
    } * z;
    let conteudo = Rect::from_min_max(pos2(r.left() + recuo, r.top() + faixa + recuo), pos2(r.right() - recuo, r.bottom() - recuo));
    let ed = l.edicao.as_mut().expect("editando");
    let flutuante = ed.rotulo || fonte * z < 13.0 || conteudo.width() < 48.0;
    let segredo_antes = ed.segredo;
    let resposta;
    let caixa_editor;
    let tamanho = if flutuante { fonte } else { fonte * z };
    let fonte_id = if codigo { FontId::monospace(tamanho) } else { FontId::proportional(tamanho) };
    let rotulo = ed.rotulo;
    let te = |ui: &mut egui::Ui, largura: f32, altura: f32, texto: &mut String| {
        let campo = if rotulo { egui::TextEdit::singleline(texto) } else { egui::TextEdit::multiline(texto).desired_rows(1).lock_focus(codigo) };
        ui.add(
            campo
                .id(id)
                .frame(egui::Frame::NONE)
                .font(fonte_id.clone())
                .text_color(cor)
                .desired_width(largura)
                .min_size(vec2(largura, altura))
                .char_limit(if rotulo { MAX_TITULO } else { MAX_TEXTO })
                .hint_text(RichText::new(if rotulo { "Rótulo da ligação" } else { "Escreva aqui (markdown)" }).color(t.secundaria)),
        )
    };
    let borda = if segredo_antes { Stroke::new(1.5, p.erro) } else { Stroke::new(2.0, p.destaque) };
    if flutuante {
        let largura = (r.width()).max(280.0).min(area.width() - 24.0);
        let canto = if rotulo {
            let (de, para) = (l.modelo.buscar(e.de).cloned(), l.modelo.buscar(e.para).cloned());
            match (de, para) {
                (Some(de), Some(para)) => {
                    desenho::meio(&desenho::pontos_na_tela(area, &l.camera, desenho::curva(caixa_quadro(&de), caixa_quadro(&para)))) - vec2(largura / 2.0, 20.0)
                }
                _ => area.center(),
            }
        } else {
            r.left_top()
        };
        // O canto do editor fica no canto do item; só anda o necessário para
        // caber na lousa (a barra de formatação vai em cima dele).
        let canto = pos2(
            canto.x.clamp(area.left() + MARGEM, (area.right() - largura - MARGEM).max(area.left() + MARGEM)),
            canto.y.clamp(area.top() + 56.0, (area.bottom() - 96.0).max(area.top() + 56.0)),
        );
        let ed = l.edicao.as_mut().expect("editando");
        let resultado = egui::Area::new(Id::new(("lousa-editor-flutuante", l.dono)))
            .order(Order::Foreground)
            .fixed_pos(canto)
            .constrain_to(area.shrink(MARGEM))
            .show(&ctx, |ui| {
                tema::moldura_flutuante().fill(if rotulo { p.superficie_alta } else { papel }).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_width(largura - 24.0);
                    te(ui, largura - 24.0, 0.0, &mut ed.texto)
                })
            });
        caixa_editor = resultado.response.rect;
        resposta = Some(resultado.inner.inner);
        if segredo_antes {
            let pintor = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new(("lousa-segredo", l.dono))));
            pintor.rect_stroke(caixa_editor, CornerRadius::same(tema::RAIO_CARTAO), borda, StrokeKind::Inside);
        }
    } else {
        // No lugar: o campo ocupa o corpo do cartão (abaixo da faixa do
        // título), sem moldura nem sombra; a única moldura é o contorno.
        let ed = l.edicao.as_mut().expect("editando");
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(conteudo).layout(egui::Layout::top_down(egui::Align::Min)));
        filho.set_clip_rect(r.intersect(area));
        resposta = Some(te(&mut filho, conteudo.width(), conteudo.height(), &mut ed.texto));
        caixa_editor = r;
        ui.painter().rect_stroke(r.expand(3.0), CornerRadius::same(tema::RAIO_CARTAO + 3), borda, StrokeKind::Outside);
    }
    let resposta = resposta.expect("campo");
    let ed = l.edicao.as_mut().expect("editando");
    if std::mem::take(&mut ed.focar) {
        resposta.request_focus();
    }
    let segredo = ed.segredo;
    let n = ed.texto.chars().count();
    // Faixa do segredo recusado e o contador.
    if segredo {
        // A faixa da recusa, colada embaixo do editor e da mesma largura.
        let pintor = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new(("lousa-segredo", l.dono))));
        let base = if flutuante { caixa_editor } else { caixa_editor.expand(3.0) };
        let faixa = Rect::from_min_size(base.left_bottom(), vec2(base.width(), 28.0));
        let c = tema::RAIO_CONTROLE;
        pintor.rect(
            faixa,
            CornerRadius { nw: 0, ne: 0, sw: c, se: c },
            tema::fundo_tingido(p, p.erro, tema::claro()),
            Stroke::new(1.5, p.erro),
            StrokeKind::Inside,
        );
        // Estreita, a frase curta (o "não salvei" não pode sumir no corte).
        let longa = "Parece ter senha ou chave; não salvei este texto";
        let cabe = pintor.layout_no_wrap(longa.into(), FontId::proportional(12.5), p.texto).size().x <= faixa.width() - 20.0;
        let frase = if cabe { longa } else { "Não salvei: parece ter senha" };
        let g = tema::cortar(&pintor, frase, egui::TextFormat::simple(FontId::proportional(12.5), p.texto), faixa.width() - 20.0, 1, false);
        pintor.galley(pos2(faixa.left() + 10.0, faixa.center().y - g.size().y / 2.0), g, p.texto);
    } else if n >= AVISO_TEXTO {
        let cor = if n > MAX_TEXTO { p.erro } else { p.alerta };
        let pintor = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new(("lousa-contador", l.dono))));
        pintor.text(caixa_editor.right_bottom() + vec2(0.0, 4.0), Align2::RIGHT_TOP, format!("{} / 8 000", milhar(n)), FontId::proportional(11.5), cor);
    }
    let mudou = resposta.changed();
    let perdeu = resposta.lost_focus();
    if !ed.rotulo {
        formatacao(l, &ctx, caixa_editor, id, area);
    }
    if mudou {
        let pintor = ui.painter().clone();
        l.texto_mudou(&ctx, &pintor);
    }
    let agora = ctx.input(|i| i.time);
    if sair {
        l.encerrar_edicao(&ctx, agora);
        return;
    }
    if perdeu {
        // Um clique na barra de formatação tira o foco do campo: volta para ele.
        let na_barra = l.barra_formato.is_some_and(|b| ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| b.contains(p)));
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        let ed = l.edicao.as_mut().expect("editando");
        if na_barra {
            ed.focar = true;
            return;
        }
        if ed.segredo && esc {
            if ed.esc {
                // O segundo Esc descarta o texto recusado: volta o de antes (e
                // a nota criada agora, vazia de novo, some como qualquer outra).
                let antes = ed.antes.texto.clone();
                let id_item = ed.id;
                l.modelo.mudar(id_item, Mudou::TEXTO, |e| e.texto = antes);
                if let Some(ed) = l.edicao.take() {
                    l.terminar_texto(ed);
                }
                l.agendar(&ctx, agora);
                return;
            }
            ed.esc = true;
            ed.focar = true;
            return;
        }
        l.encerrar_edicao(&ctx, agora);
    }
}

fn milhar(n: usize) -> String {
    let s = n.to_string();
    if s.len() > 3 { format!("{} {}", &s[..s.len() - 3], &s[s.len() - 3..]) } else { s }
}

/// A barra de formatação, 8 acima do editor: só insere os marcadores.
fn formatacao(l: &mut Lousa, ctx: &egui::Context, editor: Rect, id: Id, area: Rect) {
    let p = cores();
    #[derive(Clone, Copy)]
    enum Marca {
        Negrito,
        Italico,
        Codigo,
        Titulo,
        Lista,
        Caixa,
        Tabela,
    }
    let mut clicada = None;
    const DICA: &str = "Markdown: **negrito**, # título, - lista, | tabela |";
    // Os 7 botões e a moldura; a dica só entra se couber inteira.
    let botoes_largura = 7.0 * 28.0 + 6.0 * 2.0 + 12.0;
    let dica_largura = ctx.fonts_mut(|f| f.layout_no_wrap(DICA.into(), FontId::proportional(11.5), p.suave).size().x) + 6.0 + 2.0;
    let x = editor.left().clamp(area.left() + MARGEM, (area.right() - MARGEM - botoes_largura).max(area.left() + MARGEM));
    let com_dica = area.right() - MARGEM - x >= botoes_largura + dica_largura;
    let pos = pos2(x, (editor.top() - 8.0).max(area.top() + 44.0));
    let resposta = egui::Area::new(Id::new(("lousa-formatacao", l.dono)))
        .order(Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(pos)
        .constrain_to(area.shrink(MARGEM))
        .show(ctx, |ui| {
            moldura().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let botoes: [(Marca, &str, FontId, &str); 7] = [
                        (Marca::Negrito, "B", forte(13.0), "Negrito (**texto**)"),
                        (Marca::Italico, "I", FontId::proportional(13.0), "Itálico (*texto*)"),
                        (Marca::Codigo, "‹›", FontId::monospace(12.0), "Código (`texto`)"),
                        (Marca::Titulo, "T", forte(14.0), "Título (# )"),
                        (Marca::Lista, "•", FontId::proportional(15.0), "Lista (- )"),
                        (Marca::Caixa, "", FontId::proportional(13.0), "Caixa de seleção (- [ ] )"),
                        (Marca::Tabela, "", FontId::proportional(13.0), "Tabela"),
                    ];
                    for (marca, texto, fonte, dica) in botoes {
                        let (r, resposta) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                        if resposta.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
                        }
                        let c = r.center();
                        match marca {
                            Marca::Caixa => {
                                ui.painter().rect_stroke(
                                    Rect::from_center_size(c, vec2(12.0, 12.0)),
                                    CornerRadius::same(3),
                                    Stroke::new(1.2, p.texto),
                                    StrokeKind::Inside,
                                );
                            }
                            Marca::Tabela => {
                                let q = Rect::from_center_size(c, vec2(13.0, 11.0));
                                ui.painter().rect_stroke(q, CornerRadius::same(2), Stroke::new(1.2, p.texto), StrokeKind::Inside);
                                ui.painter().line_segment([q.center_top(), q.center_bottom()], Stroke::new(1.2, p.texto));
                                ui.painter().line_segment([q.left_center(), q.right_center()], Stroke::new(1.2, p.texto));
                            }
                            Marca::Italico => {
                                let mut f = egui::TextFormat::simple(fonte, p.texto);
                                f.italics = true;
                                let g = ui.painter().layout_job(egui::text::LayoutJob::single_section(texto.into(), f));
                                ui.painter().galley(c - g.size() / 2.0, g, p.texto);
                            }
                            _ => {
                                ui.painter().text(c, Align2::CENTER_CENTER, texto, fonte, p.texto);
                            }
                        }
                        if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(dica).clicked() {
                            clicada = Some(marca);
                        }
                    }
                    if com_dica {
                        ui.add_space(6.0);
                        ui.label(RichText::new(DICA).color(p.suave).size(11.5));
                    }
                });
            });
        });
    l.barra_formato = Some(resposta.response.rect);
    let Some(marca) = clicada else { return };
    let Some(ed) = &mut l.edicao else { return };
    let mut estado = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let total = ed.texto.chars().count();
    let (inicio, fim) = estado.cursor.char_range().map_or((total, total), |r| {
        let (a, b) = (r.primary.index.0.min(total), r.secondary.index.0.min(total));
        (a.min(b), a.max(b))
    });
    let indice = |n: usize| ed.texto.char_indices().nth(n).map_or(ed.texto.len(), |(i, _)| i);
    let (bi, bf) = (indice(inicio), indice(fim));
    let selecionado = ed.texto[bi..bf].to_string();
    let inicio_da_linha = ed.texto[..bi].rfind('\n').map_or(0, |i| i + 1);
    let cursor_final = match marca {
        Marca::Negrito | Marca::Italico | Marca::Codigo => {
            let m = match marca {
                Marca::Negrito => "**",
                Marca::Italico => "*",
                _ => "`",
            };
            ed.texto.replace_range(bi..bf, &format!("{m}{selecionado}{m}"));
            inicio + m.chars().count() + selecionado.chars().count() + if selecionado.is_empty() { 0 } else { m.chars().count() }
        }
        Marca::Titulo | Marca::Lista | Marca::Caixa => {
            let m = match marca {
                Marca::Titulo => "# ",
                Marca::Lista => "- ",
                _ => "- [ ] ",
            };
            ed.texto.insert_str(inicio_da_linha, m);
            fim + m.chars().count()
        }
        Marca::Tabela => {
            let modelo = "| Coluna | Coluna |\n|---|---|\n|  |  |\n";
            let prefixo = if bi > 0 && !ed.texto[..bi].ends_with('\n') { "\n" } else { "" };
            ed.texto.insert_str(bi, &format!("{prefixo}{modelo}"));
            inicio + prefixo.len() + modelo.chars().count()
        }
    };
    estado.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(cursor_final))));
    estado.store(ctx, id);
    ed.focar = true;
    let pintor = ctx.layer_painter(egui::LayerId::background());
    l.texto_mudou(ctx, &pintor);
}

/// A busca do cartão de tarefa: por título ou #número; primeiro as tarefas
/// dos projetos preferidos (do workspace, ou o projeto da tarefa).
fn busca(l: &mut Lousa, ctx: &egui::Context, area: Rect, c: &Contexto) {
    let p = cores();
    let Some(b) = &mut l.busca else { return };
    let id_campo = Id::new(("lousa-busca", l.dono));
    let com_foco = ctx.memory(|m| m.has_focus(id_campo));
    let (cima, baixo, enter, esc) = if com_foco {
        ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        })
    } else {
        (false, false, false, false)
    };
    let filtro = b.texto.trim().to_lowercase();
    let numero = filtro.strip_prefix('#').and_then(|n| n.parse::<i64>().ok());
    let mut lista: Vec<&crate::dados::Tarefa> = c
        .modelo
        .tarefas
        .iter()
        .filter(|t| filtro.is_empty() || numero.is_some_and(|n| t.id == n) || t.titulo.to_lowercase().contains(&filtro) || t.id.to_string() == filtro)
        .collect();
    lista.sort_by_key(|t| (!c.preferidos.contains(&t.projeto_id), t.titulo.to_lowercase()));
    lista.truncate(8);
    if baixo {
        b.escolhida = (b.escolhida + 1).min(lista.len().saturating_sub(1));
    }
    if cima {
        b.escolhida = b.escolhida.saturating_sub(1);
    }
    b.escolhida = b.escolhida.min(lista.len().saturating_sub(1));
    let mut escolhida = if enter { lista.get(b.escolhida).map(|t| t.id) } else { None };
    let pos = area.center_top() + vec2(0.0, 64.0);
    let resposta = egui::Area::new(Id::new(("lousa-busca-caixa", l.dono))).order(Order::Foreground).pivot(Align2::CENTER_TOP).fixed_pos(pos).show(ctx, |ui| {
        tema::moldura_flutuante().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_width(380.0);
            // O campo tem a largura das linhas da lista (360) e o mesmo recuo do texto delas.
            let campo = campo_da_barra(ui, &mut b.texto, "Buscar tarefa por título ou #número", 360.0 - 22.0, Some(id_campo), 200, 13.5);
            if std::mem::take(&mut b.focar) {
                campo.request_focus();
            }
            ui.add_space(8.0);
            if lista.is_empty() {
                ui.label(RichText::new("Nenhuma tarefa encontrada").color(p.suave).size(13.0));
            }
            for (i, t) in lista.iter().enumerate() {
                let (r, resposta) = ui.allocate_exact_size(vec2(360.0, 40.0), Sense::click());
                if i == b.escolhida || resposta.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_ETIQUETA), if i == b.escolhida { tema::fundo_escolhido() } else { p.realce });
                }
                let g = tema::cortar(ui.painter(), &t.titulo, egui::TextFormat::simple(FontId::proportional(13.5), p.texto), 340.0, 1, false);
                ui.painter().galley(r.left_top() + vec2(10.0, 4.0), g, p.texto);
                ui.painter().text(
                    r.left_bottom() + vec2(10.0, -4.0),
                    Align2::LEFT_BOTTOM,
                    format!("#{} · {}", t.id, t.projeto),
                    FontId::proportional(11.5),
                    p.suave,
                );
                if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    escolhida = Some(t.id);
                }
            }
        });
    });
    let fora = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !resposta.response.rect.contains(p)));
    if let Some(tarefa) = escolhida {
        let posicao = b.posicao;
        l.busca = None;
        l.criar(ctx, TipoElemento::Tarefa, posicao, |e| e.tarefa_ref = tarefa);
    } else if esc || fora {
        l.busca = None;
    }
}

/// O menu do botão direito: no item ou no fundo.
pub fn menu_contexto(l: &mut Lousa, ui: &mut egui::Ui, c: &Contexto) -> Vec<Acao> {
    let ctx = ui.ctx().clone();
    let mut acoes = Vec::new();
    ui.set_min_width(220.0);
    let pode = c.pode_mudar;
    let Some((alvo, onde)) = l.menu else {
        ui.close();
        return acoes;
    };
    match alvo.and_then(|id| l.modelo.buscar(id).cloned()) {
        Some(e) => {
            let texto = e.tipo.de_texto() || e.tipo == TipoElemento::Ligacao;
            if tema::opcao_menu_com(ui, "Editar", Some("Enter"), pode && texto) {
                l.editar(e.id);
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Duplicar", Some("Ctrl+D"), pode && e.tipo != TipoElemento::Ligacao) {
                let copia = l.copiar_selecao();
                l.colar_itens(&ctx, &copia, Vec2::splat(super::CASCATA));
                ui.close();
            }
            if e.tipo == TipoElemento::Nota && pode {
                // "Cor" e as bolinhas na própria linha (a atual com o anel),
                // com o recuo das outras opções.
                let p = cores();
                let mut escolhida = None;
                ui.horizontal(|ui| {
                    ui.set_height(32.0);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(14.0);
                    let g = ui.painter().layout_no_wrap("Cor".into(), FontId::proportional(13.5), p.texto);
                    let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 12.0, 32.0), Sense::hover());
                    ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, p.texto);
                    escolhida = cores_da_nota(ui, Some(&e.cor));
                });
                if let Some(chave) = escolhida {
                    let ids: Vec<i64> = l.selecao.clone();
                    l.mudar_varios(&ctx, &ids, Mudou::COR, |x| {
                        if x.tipo == TipoElemento::Nota {
                            x.cor = chave.clone()
                        }
                    });
                    ui.close();
                }
            }
            if tema::opcao_menu_com(ui, "Trazer para frente", None, pode) {
                let z = l.modelo.z_maximo();
                l.mudar_varios(&ctx, &[e.id], Mudou::Z, |x| x.z = z + 1);
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Enviar para trás", None, pode) {
                let z = l.modelo.z_minimo();
                l.mudar_varios(&ctx, &[e.id], Mudou::Z, |x| x.z = z - 1);
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Copiar", Some("Ctrl+C"), true) {
                let itens = l.copiar_selecao();
                let texto = texto_da_area(&itens);
                ctx.copy_text(texto.clone());
                ctx.data_mut(|d| d.insert_temp(Id::new("lousa-area-interna"), (texto, itens)));
                ui.close();
            }
            if e.tipo == TipoElemento::Tarefa && e.tarefa_ref != 0 && tema::opcao_menu_com(ui, "Abrir tarefa", None, true) {
                acoes.push(Acao::AbrirTarefa(e.tarefa_ref));
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Apagar", Some("Delete"), pode) {
                acoes.extend(l.apagar_selecao(&ctx));
                ui.close();
            }
        }
        None => {
            for (rotulo, atalho, tipo) in
                [("Nova nota aqui", "N", TipoElemento::Nota), ("Novo texto", "T", TipoElemento::Texto), ("Novo bloco de código", "C", TipoElemento::Codigo)]
            {
                if tema::opcao_menu_com(ui, rotulo, Some(atalho), pode) {
                    l.criar(&ctx, tipo, onde, |_| {});
                    ui.close();
                }
            }
            if tema::opcao_menu_com(ui, "Colar", Some("Ctrl+V"), pode) {
                acoes.extend(colar_do_menu(l, &ctx, c.perfil, onde));
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Inserir imagem…", Some("I"), pode) {
                escolher_arquivos(l, &ctx, false);
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Inserir vídeo…", None, pode) {
                escolher_arquivos(l, &ctx, true);
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Cartão de tarefa…", Some("K"), pode) {
                l.busca = Some(Busca::nova(onde));
                ui.close();
            }
            if tema::opcao_menu_com(ui, "Ajustar à tela", Some("Shift+1"), true) {
                l.ajustar_tudo();
                ui.close();
            }
        }
    }
    acoes
}

/// "Colar" do menu: os itens copiados aqui, um texto ou uma imagem.
fn colar_do_menu(l: &mut Lousa, ctx: &egui::Context, perfil: i64, onde: Pos2) -> Option<Acao> {
    let texto = arboard::Clipboard::new().ok().and_then(|mut a| a.get_text().ok());
    let interna = ctx.data(|d| d.get_temp::<(String, Vec<Elemento>)>(Id::new("lousa-area-interna")));
    match (texto, interna) {
        (Some(t), Some((marca, itens))) if t == marca && !itens.is_empty() => {
            let caixa = camera::envolver(itens.iter().filter(|e| e.tipo != TipoElemento::Ligacao).map(caixa_quadro));
            let desloca = caixa.map_or(Vec2::splat(super::CASCATA), |c| onde - c.center());
            l.colar_itens(ctx, &itens, desloca);
            None
        }
        (Some(t), _) if !t.trim().is_empty() => {
            let t: String = t.chars().take(MAX_TEXTO).collect();
            l.criar(ctx, TipoElemento::Nota, onde, |e| e.texto = t);
            l.edicao = None;
            None
        }
        _ => l.colar_imagem(ctx, perfil, onde),
    }
}

/// O cartão da lousa vazia, no centro.
fn vazio(l: &mut Lousa, ctx: &egui::Context, area: Rect, c: &Contexto) -> Vec<Acao> {
    let p = cores();
    let mut acoes = Vec::new();
    let (mut nota, mut imagem, mut tarefa) = (false, false, false);
    let tarefa_com_agente = l.dono.tarefa_id != 0 && c.tem_agente;
    egui::Area::new(Id::new(("lousa-vazia", l.dono))).order(Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(area.center()).show(ctx, |ui| {
        tema::moldura_flutuante().show(ui, |ui| {
            ui.set_max_width(420.0 - 32.0);
            ui.label(
                RichText::new("Lousa vazia. Clique duas vezes em qualquer lugar para criar uma nota, ou cole (Ctrl+V) um texto ou uma imagem.")
                    .color(p.suave)
                    .size(13.5),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                nota = tema::botao_secundario_com(ui, "Nova nota", c.pode_mudar).clicked();
                imagem = tema::botao_secundario_com(ui, "Inserir imagem", c.pode_mudar).clicked();
                tarefa = tema::botao_secundario_com(ui, "Cartão de tarefa", c.pode_mudar).clicked();
            });
            if tarefa_com_agente {
                ui.add_space(12.0);
                ui.label(RichText::new("Ou peça ao agente:").color(p.suave).size(12.5));
                ui.add_space(4.0);
                if tema::botao_secundario_com(ui, "Montar o fluxo desta tarefa", c.pode_mudar)
                    .on_hover_text("Põe o pedido na caixa de mensagem; você edita e envia")
                    .clicked()
                {
                    acoes.push(Acao::PedirAoAgente(PEDIDO_FLUXO.into()));
                }
            }
        });
    });
    let centro = l.centro_da_vista();
    if nota {
        l.criar(ctx, TipoElemento::Nota, centro, |_| {});
        l.ativa = true;
    }
    if imagem {
        escolher_arquivos(l, ctx, false);
    }
    if tarefa {
        l.busca = Some(Busca::nova(centro));
    }
    acoes
}

/// Os cartões de envio que falharam: "Não consegui enviar a imagem · Tentar de novo · ×".
fn envios_com_erro(l: &mut Lousa, ui: &mut egui::Ui, c: &Contexto) {
    let p = cores();
    let ctx = ui.ctx().clone();
    let mut tentar = None;
    let mut fechar = None;
    for (numero, r) in l.envios_com_erro.clone() {
        let caixa = Rect::from_center_size(r.center(), vec2(r.width().max(240.0), 80.0));
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa).layout(egui::Layout::top_down(egui::Align::Center)));
        filho.label(RichText::new("Não consegui enviar a imagem").color(p.texto).size(12.5));
        filho.horizontal(|ui| {
            if tema::botao_secundario(ui, "Tentar de novo").clicked() {
                tentar = Some(numero);
            }
            if tema::botao_icone(ui, Icone::Fechar, 24.0).clicked() {
                fechar = Some(numero);
            }
        });
    }
    if let Some(n) = tentar {
        l.reenviar(&ctx, c.perfil, n);
    }
    if let Some(n) = fechar {
        l.envios.retain(|e| e.numero != n);
    }
    let _ = Color32::TRANSPARENT;
}

/// Sem o núcleo: a lousa fica só para leitura (mover a vista e o zoom funcionam).
pub fn faixa_sem_nucleo(ui: &mut egui::Ui, area: Rect) {
    let p = cores();
    let faixa = Rect::from_min_size(area.min, vec2(area.width(), 32.0));
    let pintor = ui.ctx().layer_painter(egui::LayerId::new(Order::Foreground, Id::new("lousa-sem-nucleo")));
    pintor.rect_filled(faixa, 0, tema::fundo_tingido(p, p.alerta, tema::claro()));
    pintor.circle_filled(faixa.left_center() + vec2(16.0, 0.0), 3.5, p.alerta);
    pintor.text(
        faixa.left_center() + vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        "Sem ligação com o núcleo: a lousa está só para leitura",
        FontId::proportional(13.0),
        p.texto,
    );
}
