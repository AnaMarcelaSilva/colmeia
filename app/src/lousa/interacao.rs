//! A interação da lousa: o que está sob o mouse, clicar, arrastar (mover,
//! redimensionar, puxar ligação, caixa de seleção, mover a vista), a
//! rodinha, a pinça e os atalhos. A moldura da interação (seleção, alças,
//! caixa) é desenhada aqui, em pixels de tela.

use eframe::egui::{self, CornerRadius, CursorIcon, Event, Id, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::desenho::{self, caixa_quadro};
use super::modelo::{Comando, Mudou};
use super::{Acao, Arrasto, CASCATA, Contexto, Lousa, barra};
use crate::api::{ElementoLousa as Elemento, TipoElemento};
use crate::tema::{self, cores};

/// Uma das oito alças de redimensionar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alca {
    Cima,
    Baixo,
    Esquerda,
    Direita,
    CimaEsquerda,
    CimaDireita,
    BaixoEsquerda,
    BaixoDireita,
}

impl Alca {
    const TODAS: [Alca; 8] =
        [Alca::CimaEsquerda, Alca::Cima, Alca::CimaDireita, Alca::Direita, Alca::BaixoDireita, Alca::Baixo, Alca::BaixoEsquerda, Alca::Esquerda];

    fn ponto(self, r: Rect) -> Pos2 {
        match self {
            Alca::Cima => r.center_top(),
            Alca::Baixo => r.center_bottom(),
            Alca::Esquerda => r.left_center(),
            Alca::Direita => r.right_center(),
            Alca::CimaEsquerda => r.left_top(),
            Alca::CimaDireita => r.right_top(),
            Alca::BaixoEsquerda => r.left_bottom(),
            Alca::BaixoDireita => r.right_bottom(),
        }
    }

    fn cursor(self) -> CursorIcon {
        match self {
            Alca::Cima | Alca::Baixo => CursorIcon::ResizeVertical,
            Alca::Esquerda | Alca::Direita => CursorIcon::ResizeHorizontal,
            Alca::CimaEsquerda | Alca::BaixoDireita => CursorIcon::ResizeNwSe,
            Alca::CimaDireita | Alca::BaixoEsquerda => CursorIcon::ResizeNeSw,
        }
    }

    /// O retângulo novo ao puxar a alça até `p` (no quadro).
    pub fn puxar(self, r: Rect, p: Pos2, proporcao: Option<f32>) -> Rect {
        let min = super::modelo_min_lado();
        let (mut x0, mut y0, mut x1, mut y1) = (r.left(), r.top(), r.right(), r.bottom());
        match self {
            Alca::Cima | Alca::CimaEsquerda | Alca::CimaDireita => y0 = p.y.min(y1 - min),
            Alca::Baixo | Alca::BaixoEsquerda | Alca::BaixoDireita => y1 = p.y.max(y0 + min),
            _ => {}
        }
        match self {
            Alca::Esquerda | Alca::CimaEsquerda | Alca::BaixoEsquerda => x0 = p.x.min(x1 - min),
            Alca::Direita | Alca::CimaDireita | Alca::BaixoDireita => x1 = p.x.max(x0 + min),
            _ => {}
        }
        let mut novo = Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
        if let Some(prop) = proporcao.filter(|p| *p > 0.0) {
            // A imagem mantém a proporção: manda o lado que mais andou.
            let por_largura =
                matches!(self, Alca::Esquerda | Alca::Direita) || (novo.width() / prop >= novo.height() && !matches!(self, Alca::Cima | Alca::Baixo));
            let (l, a) = if por_largura { (novo.width(), novo.width() / prop) } else { (novo.height() * prop, novo.height()) };
            let (l, a) = (l.max(min), a.max(min));
            let ancora_x = if matches!(self, Alca::Esquerda | Alca::CimaEsquerda | Alca::BaixoEsquerda) { r.right() - l } else { r.left() };
            let ancora_y = if matches!(self, Alca::Cima | Alca::CimaEsquerda | Alca::CimaDireita) { r.bottom() - a } else { r.top() };
            novo = Rect::from_min_size(pos2(ancora_x, ancora_y), vec2(l, a));
        }
        Rect::from_min_max(novo.min.round(), novo.max.round())
    }
}

/// O que está sob o mouse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alvo {
    Item(i64),
    Ligacao(i64),
    Redimensionar(i64, Alca),
    /// A alça de puxar ligação de um item (o lado: 0 cima, 1 direita, 2 baixo, 3 esquerda).
    Ligar(i64, u8),
    /// Fora do item, mas no caminho até as alças de ligação dele: só mantém
    /// as alças à vista (o clique ali é como no vazio).
    Perto(i64),
}

impl Alvo {
    pub fn id(self) -> i64 {
        match self {
            Alvo::Item(id) | Alvo::Ligacao(id) | Alvo::Redimensionar(id, _) | Alvo::Ligar(id, _) | Alvo::Perto(id) => id,
        }
    }
}

/// Raio do círculo da alça de ligação e a distância dele à borda (longe
/// das alças quadradas de redimensionar, que ficam a 3).
const ALCA_LIGAR: f32 = 5.0;
const FORA_LIGAR: f32 = 20.0;

fn pontos_de_ligar(r: Rect) -> [Pos2; 4] {
    [
        r.center_top() - vec2(0.0, FORA_LIGAR),
        r.right_center() + vec2(FORA_LIGAR, 0.0),
        r.center_bottom() + vec2(0.0, FORA_LIGAR),
        r.left_center() - vec2(FORA_LIGAR, 0.0),
    ]
}

fn na_tela(l: &Lousa, e: &Elemento) -> Rect {
    l.camera.retangulo_na_tela(l.area, desenho::caixa_de_toque(e))
}

/// O que está em `pos` (tela): alças da seleção, alças de ligação do item
/// com o mouse em cima, itens de cima para baixo e, por fim, as ligações.
pub fn alvo_em(l: &Lousa, pos: Pos2, com_alcas: bool, item_antes: Option<i64>) -> Option<Alvo> {
    let visivel = l.camera.visivel(l.area);
    if com_alcas {
        if let [id] = l.selecao[..]
            && let Some(e) = l.modelo.buscar(id).filter(|e| e.tipo != TipoElemento::Ligacao)
        {
            let r = na_tela(l, e).expand(3.0);
            if r.width() >= 24.0 && r.height() >= 24.0 {
                for alca in Alca::TODAS {
                    if (alca.ponto(r) - pos).length() <= 7.0 {
                        return Some(Alvo::Redimensionar(id, alca));
                    }
                }
            }
        }
        if let Some(id) = item_antes
            && let Some(e) = l.modelo.buscar(id).filter(|e| e.tipo != TipoElemento::Ligacao)
        {
            for (i, p) in pontos_de_ligar(na_tela(l, e)).iter().enumerate() {
                if (*p - pos).length() <= ALCA_LIGAR + 3.0 {
                    return Some(Alvo::Ligar(id, i as u8));
                }
            }
        }
    }
    let perto = item_antes
        .filter(|_| com_alcas)
        .and_then(|id| l.modelo.buscar(id))
        .filter(|e| e.tipo != TipoElemento::Ligacao && na_tela(l, e).expand(FORA_LIGAR + ALCA_LIGAR + 3.0).contains(pos))
        .map(|e| Alvo::Perto(e.id));
    for e in l.modelo.elementos.iter().rev() {
        if e.tipo == TipoElemento::Ligacao || !caixa_quadro(e).intersects(visivel) {
            continue;
        }
        if na_tela(l, e).contains(pos) {
            return Some(Alvo::Item(e.id));
        }
    }
    for e in l.modelo.elementos.iter().rev().filter(|e| e.tipo == TipoElemento::Ligacao) {
        let (Some(de), Some(para)) = (l.modelo.buscar(e.de), l.modelo.buscar(e.para)) else { continue };
        let pontos = desenho::pontos_na_tela(l.area, &l.camera, desenho::curva(caixa_quadro(de), caixa_quadro(para)));
        if desenho::distancia(&pontos, pos) <= 6.0 {
            return Some(Alvo::Ligacao(e.id));
        }
    }
    perto
}

/// O item (não ligação) com o mouse em cima, guardado no último quadro.
pub fn item_em_cima(l: &Lousa, ui: &egui::Ui) -> Option<i64> {
    ui.ctx().data(|d| d.get_temp::<Option<Alvo>>(id_em_cima(l))).flatten().and_then(|a| match a {
        Alvo::Item(id) | Alvo::Ligar(id, _) | Alvo::Perto(id) => Some(id),
        _ => None,
    })
}

/// A ligação com o mouse em cima (destacada).
pub fn ligacao_em_cima(l: &Lousa, ui: &egui::Ui) -> Option<i64> {
    ui.ctx().data(|d| d.get_temp::<Option<Alvo>>(id_em_cima(l))).flatten().and_then(|a| if let Alvo::Ligacao(id) = a { Some(id) } else { None })
}

fn id_em_cima(l: &Lousa) -> Id {
    Id::new(("lousa-em-cima", l.dono))
}

/// Trata o mouse, a rodinha, o teclado e o que foi solto na lousa.
pub fn tratar(l: &mut Lousa, ui: &mut egui::Ui, area: Rect, c: &Contexto) -> Vec<Acao> {
    let ctx = ui.ctx().clone();
    let mut acoes = Vec::new();
    let resposta = ui.interact(area, Id::new(("lousa", l.dono)), Sense::click_and_drag());
    let ponteiro = ctx.input(|i| i.pointer.hover_pos());
    let quadro_do_mouse = ponteiro.filter(|p| area.contains(*p)).map(|p| l.camera.para_quadro(area, p));
    let em_cima_antes = item_em_cima(l, ui);
    let pode = c.pode_mudar;

    // Onde está o mouse (só sem arrasto: durante ele, o alvo é o do começo).
    let alvo = if resposta.hovered() && l.arrasto.is_none() { ponteiro.and_then(|p| alvo_em(l, p, pode, em_cima_antes)) } else { None };
    ctx.data_mut(|d| d.insert_temp(id_em_cima(l), alvo));
    let shift = ctx.input(|i| i.modifiers.shift);
    l.espaco = resposta.hovered() && ctx.input(|i| i.key_down(Key::Space)) && !ctx.text_edit_focused();

    // A lousa ganha o teclado no clique nela (o egui diz se o clique foi no
    // fundo dela ou numa peça por cima, como o aviso) e perde no clique fora.
    if ctx.input(|i| i.pointer.any_pressed()) {
        let nela = resposta.contains_pointer() && resposta.hovered();
        let na_area = ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| area.contains(p));
        if nela {
            l.ativa = true;
            // A marca de "novo" some no próximo clique da dona na lousa.
            l.modelo.novos.clear();
        } else if !na_area {
            l.ativa = false;
        }
    }

    // Cursores.
    if resposta.hovered() {
        let cursor = match (&l.arrasto, alvo) {
            (Some(Arrasto::Vista), _) => CursorIcon::Grabbing,
            (Some(Arrasto::Mover { .. }), _) => CursorIcon::Move,
            (Some(Arrasto::Redimensionar { alca, .. }), _) => alca.cursor(),
            (Some(Arrasto::Caixa { .. }), _) => CursorIcon::Crosshair,
            (Some(Arrasto::Ligar { .. }), _) => CursorIcon::PointingHand,
            (None, Some(Alvo::Redimensionar(_, alca))) => alca.cursor(),
            (None, Some(Alvo::Ligar(..))) => CursorIcon::PointingHand,
            (None, Some(Alvo::Item(id))) if l.selecao.contains(&id) && pode => CursorIcon::Move,
            (None, Some(Alvo::Item(_) | Alvo::Ligacao(_))) => CursorIcon::Default,
            (None, None | Some(Alvo::Perto(_))) if l.espaco => CursorIcon::Grab,
            (None, None | Some(Alvo::Perto(_))) if shift => CursorIcon::Crosshair,
            (None, None | Some(Alvo::Perto(_))) => CursorIcon::Grab,
        };
        ctx.set_cursor_icon(cursor);
    }

    // Rodinha e pinça (só com o mouse sobre a lousa).
    if resposta.hovered()
        && let Some(p) = ponteiro
    {
        let zoom = ctx.input(|i| i.zoom_delta());
        let passo = l.pinca.passo(zoom, c.agora);
        if passo != 0 {
            l.camera.passo_de_zoom(area, p, passo);
        }
        let mut rolagem = ctx.input(|i| i.smooth_scroll_delta);
        if shift && rolagem.x == 0.0 {
            rolagem = vec2(rolagem.y, 0.0);
        }
        if rolagem != Vec2::ZERO && zoom == 1.0 {
            l.camera.mover(rolagem);
        }
    }

    // Arrastar.
    let origem = ctx.input(|i| i.pointer.press_origin());
    if resposta.drag_started_by(PointerButton::Middle) {
        l.arrasto = Some(Arrasto::Vista);
    } else if resposta.drag_started_by(PointerButton::Primary)
        && let Some(inicio) = origem
    {
        l.arrasto = Some(comecar_arrasto(l, inicio, shift, pode));
        if let Some(e) = l.edicao.take() {
            l.terminar_texto(e);
        }
    }
    if l.arrasto.is_some() && (resposta.dragged() || resposta.drag_stopped()) {
        let delta = ctx.input(|i| i.pointer.delta());
        if let Some(p) = ponteiro.or(ctx.input(|i| i.pointer.interact_pos())) {
            andar_arrasto(l, &ctx, delta, l.camera.para_quadro(area, p), shift);
        }
    }
    if resposta.drag_stopped()
        && let Some(a) = l.arrasto.take()
    {
        let fim = ctx.input(|i| i.pointer.interact_pos()).map(|p| l.camera.para_quadro(area, p));
        terminar_arrasto(l, &ctx, a, fim, c.agora);
    }

    // Cliques.
    if resposta.double_clicked() {
        if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
            acoes.extend(clique_duplo(l, &ctx, p, pode, c.agora));
        }
    } else if resposta.clicked()
        && let Some(p) = ctx.input(|i| i.pointer.interact_pos())
    {
        acoes.extend(clique(l, &ctx, p, shift, c.agora));
    }
    if resposta.secondary_clicked()
        && let Some(p) = ctx.input(|i| i.pointer.interact_pos())
    {
        let alvo = alvo_em(l, p, false, None).map(|a| a.id());
        if let Some(id) = alvo
            && !l.selecao.contains(&id)
        {
            l.selecao = vec![id];
        }
        l.menu = Some((alvo, l.camera.para_quadro(area, p)));
    }
    // O selo do agente explica quem acrescentou o item.
    if let (Some(Alvo::Item(id)), Some(p)) = (alvo, ponteiro)
        && let Some(e) = l.modelo.buscar(id).filter(|e| e.do_agente())
        && (na_tela(l, e).right_top() + vec2(-14.0, 14.0) - p).length() <= 9.0
    {
        let quem = c.modelo.tarefas.iter().flat_map(|t| &t.agentes).find(|a| a.id == e.agente_id).map_or("o agente".to_string(), |a| a.nome_com_papel());
        resposta.clone().on_hover_text(format!("Acrescentado por {quem}"));
    }
    let mut menu = Vec::new();
    resposta.context_menu(|ui| menu = barra::menu_contexto(l, ui, c));
    acoes.extend(menu);

    // Arquivos soltos na lousa (ou escolhidos no "Inserir imagem…").
    let soltos: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|c| c.is_file()).collect());
    let escolhidos = ctx.data_mut(|d| d.remove_temp::<Vec<std::path::PathBuf>>(Id::new(("lousa-arquivos", l.dono)))).unwrap_or_default();
    // Os soltos nascem onde o mouse está; os escolhidos no diálogo, no centro
    // da vista (o mouse estava no botão do diálogo, não na lousa).
    if !soltos.is_empty() && pode {
        let onde = quadro_do_mouse.unwrap_or_else(|| l.centro_da_vista());
        acoes.extend(l.anexar_arquivos(&ctx, c.perfil, soltos, onde));
    }
    if !escolhidos.is_empty() && pode {
        let onde = l.centro_da_vista();
        acoes.extend(l.anexar_arquivos(&ctx, c.perfil, escolhidos, onde));
    }

    // Colar com o mouse sobre a lousa vale mesmo sem um clique antes nela
    // (abrir, copiar um print e colar).
    let colando = ctx.input(|i| {
        i.events.iter().any(|e| matches!(e, Event::Paste(_)) || matches!(e, Event::Key { key: Key::V, pressed: false, modifiers, .. } if modifiers.command))
    });
    if colando && !l.ativa && resposta.hovered() {
        l.ativa = true;
    }
    // Teclado: com a lousa ativa e nenhum campo de texto em foco.
    let teclado = l.ativa && l.edicao.is_none() && l.busca.is_none() && !ctx.text_edit_focused();
    if teclado {
        acoes.extend(atalhos(l, &ctx, c, quadro_do_mouse));
    }
    acoes
}

fn comecar_arrasto(l: &mut Lousa, inicio: Pos2, shift: bool, pode: bool) -> Arrasto {
    let quadro = l.camera.para_quadro(l.area, inicio);
    if l.espaco {
        return Arrasto::Vista;
    }
    let em_cima = l
        .modelo
        .elementos
        .iter()
        .rev()
        .find(|e| e.tipo != TipoElemento::Ligacao && na_tela(l, e).expand(FORA_LIGAR + ALCA_LIGAR).contains(inicio))
        .map(|e| e.id);
    match alvo_em(l, inicio, pode, em_cima) {
        Some(Alvo::Redimensionar(id, alca)) if pode => {
            let antes = Box::new(l.modelo.buscar(id).cloned().unwrap_or_default());
            Arrasto::Redimensionar { id, alca, antes }
        }
        Some(Alvo::Ligar(id, _)) if pode => Arrasto::Ligar { de: id, ponta: quadro },
        Some(Alvo::Item(id)) if pode => {
            if !l.selecao.contains(&id) {
                if shift {
                    l.selecao.push(id);
                } else {
                    l.selecao = vec![id];
                }
            }
            let antes: Vec<Elemento> = l.modelo.elementos.iter().filter(|e| l.selecao.contains(&e.id) && e.tipo != TipoElemento::Ligacao).cloned().collect();
            Arrasto::Mover { ultimo: quadro, antes }
        }
        Some(Alvo::Item(_) | Alvo::Ligacao(_)) => Arrasto::Vista,
        _ if shift => Arrasto::Caixa { inicio: quadro, atual: quadro, somar: l.selecao.clone() },
        _ => Arrasto::Vista,
    }
}

fn andar_arrasto(l: &mut Lousa, ctx: &egui::Context, delta: Vec2, p: Pos2, shift: bool) {
    match &mut l.arrasto {
        Some(Arrasto::Vista) => l.camera.mover(delta),
        Some(Arrasto::Mover { ultimo, antes }) => {
            // A posição sai do começo do arrasto (sem acumular erro), em unidades inteiras.
            let inicio = *ultimo;
            let d = p - inicio;
            let posicoes: Vec<(i64, Pos2)> = antes.iter().map(|e| (e.id, pos2((e.x + d.x).round(), (e.y + d.y).round()))).collect();
            for (id, pos) in posicoes {
                l.modelo.mudar(id, Mudou::POSICAO, |e| (e.x, e.y) = (pos.x, pos.y));
            }
        }
        Some(Arrasto::Redimensionar { id, alca, antes, .. }) => {
            let proporcao = (antes.tipo == TipoElemento::Imagem && !shift).then(|| antes.largura / antes.altura.max(1.0));
            // A alça do texto solto está na folga: puxa a caixa com folga e devolve sem ela.
            let folga = desenho::caixa_de_toque(antes).width() - caixa_quadro(antes).width();
            let novo = alca.puxar(desenho::caixa_de_toque(antes), p, proporcao).shrink(folga / 2.0);
            let id = *id;
            l.modelo.mudar(id, Mudou::POSICAO.com(Mudou::TAMANHO), |e| {
                (e.x, e.y, e.largura, e.altura) = (novo.left(), novo.top(), novo.width().clamp(16.0, 6000.0), novo.height().clamp(16.0, 6000.0));
            });
        }
        Some(Arrasto::Caixa { atual, .. }) => *atual = p,
        Some(Arrasto::Ligar { ponta, .. }) => *ponta = p,
        None => {}
    }
    ctx.request_repaint();
}

fn terminar_arrasto(l: &mut Lousa, ctx: &egui::Context, arrasto: Arrasto, fim: Option<Pos2>, agora: f64) {
    match arrasto {
        Arrasto::Mover { antes, .. } => {
            let pares: Vec<_> = antes.into_iter().map(|a| (l.modelo.buscar(a.id).cloned(), Some(a))).map(|(d, a)| (a, d)).collect();
            l.modelo.registrar(Comando { numero: 0, pares });
            l.agendar(ctx, agora);
        }
        Arrasto::Redimensionar { id, antes, .. } => {
            let depois = l.modelo.buscar(id).cloned();
            l.modelo.registrar(Comando { numero: 0, pares: vec![(Some(*antes), depois)] });
            l.agendar(ctx, agora);
        }
        Arrasto::Caixa { inicio, atual, somar } => {
            let caixa = Rect::from_two_pos(inicio, atual);
            let mut selecao = somar;
            for e in &l.modelo.elementos {
                if e.tipo != TipoElemento::Ligacao && caixa.intersects(caixa_quadro(e)) && !selecao.contains(&e.id) {
                    selecao.push(e.id);
                }
            }
            l.selecao = selecao;
        }
        Arrasto::Ligar { de, ponta } => {
            let ponta = fim.unwrap_or(ponta);
            let alvo = l.modelo.elementos.iter().rev().find(|e| e.tipo != TipoElemento::Ligacao && e.id != de && caixa_quadro(e).contains(ponta)).map(|e| e.id);
            match alvo {
                Some(para) => l.ligar(ctx, de, para),
                None => {
                    // Soltar no vazio cria uma nota ligada ali, já em edição.
                    let nova = l.criar(ctx, TipoElemento::Nota, ponta, |_| {});
                    l.ligar(ctx, de, nova);
                    l.selecao = vec![nova];
                    l.editar(nova);
                }
            }
        }
        Arrasto::Vista => {}
    }
}

fn clique(l: &mut Lousa, ctx: &egui::Context, p: Pos2, shift: bool, agora: f64) -> Vec<Acao> {
    let mut acoes = Vec::new();
    l.menu = None;
    match alvo_em(l, p, false, None) {
        Some(Alvo::Item(id)) => {
            let Some(e) = l.modelo.buscar(id).cloned() else { return acoes };
            let r = na_tela(l, &e);
            // O ↗ do cartão de tarefa e o play do vídeo respondem ao clique.
            if e.tipo == TipoElemento::Tarefa && e.tarefa_ref != 0 && (r.right_top() + vec2(-15.0, 15.0) - p).length() <= 10.0 {
                acoes.push(Acao::AbrirTarefa(e.tarefa_ref));
            } else if e.tipo == TipoElemento::Video && desenho::no_play(r, l.camera.zoom, p) {
                l.abrir_video(ctx, e.id, e.anexo_id, agora);
            }
            if shift {
                if let Some(i) = l.selecao.iter().position(|s| *s == id) {
                    l.selecao.remove(i);
                } else {
                    l.selecao.push(id);
                }
            } else {
                l.selecao = vec![id];
            }
        }
        Some(Alvo::Ligacao(id)) => {
            if shift {
                l.selecao.push(id);
            } else {
                l.selecao = vec![id];
            }
        }
        _ => {
            if !shift {
                l.selecao.clear();
            }
        }
    }
    acoes
}

fn clique_duplo(l: &mut Lousa, ctx: &egui::Context, p: Pos2, pode: bool, agora: f64) -> Vec<Acao> {
    let mut acoes = Vec::new();
    match alvo_em(l, p, false, None) {
        None if pode => {
            let quadro = l.camera.para_quadro(l.area, p);
            l.criar(ctx, TipoElemento::Nota, quadro, |_| {});
        }
        Some(Alvo::Item(id) | Alvo::Ligacao(id)) => {
            let Some(e) = l.modelo.buscar(id).cloned() else { return acoes };
            match e.tipo {
                TipoElemento::Tarefa if e.tarefa_ref != 0 => acoes.push(Acao::AbrirTarefa(e.tarefa_ref)),
                TipoElemento::Video => l.abrir_video(ctx, e.id, e.anexo_id, agora),
                t if pode && (t.de_texto() || t == TipoElemento::Ligacao) => l.editar(id),
                _ => {}
            }
        }
        _ => {}
    }
    acoes
}

/// Os atalhos da lousa (com ela ativa e nenhum campo em foco).
fn atalhos(l: &mut Lousa, ctx: &egui::Context, c: &Contexto, mouse: Option<Pos2>) -> Vec<Acao> {
    let mut acoes = Vec::new();
    let pode = c.pode_mudar;
    let tecla = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
    if tecla(Modifiers::NONE, Key::Escape) {
        l.selecao.clear();
    }
    if tecla(Modifiers::COMMAND, Key::A) {
        l.selecao = l.modelo.elementos.iter().map(|e| e.id).collect();
    }
    // Shift+0, Shift+1 e Shift+2 pela tecla física (o símbolo muda com o teclado).
    let fisica = |numero: Key| {
        ctx.input_mut(|i| {
            let achou = i.events.iter().position(
                |e| matches!(e, Event::Key { physical_key: Some(k), pressed: true, modifiers, .. } if *k == numero && modifiers.shift && !modifiers.command),
            );
            if let Some(n) = achou {
                i.events.remove(n);
            }
            achou.is_some()
        })
    };
    if fisica(Key::Num0) {
        l.zoom_100();
    }
    if fisica(Key::Num1) {
        l.ajustar_tudo();
    }
    if fisica(Key::Num2) {
        l.ajustar_selecao();
    }
    if tecla(Modifiers::SHIFT, Key::F5) {
        acoes.push(Acao::Apresentar(l.selecao.first().copied()));
    } else if tecla(Modifiers::NONE, Key::F5) {
        acoes.push(Acao::Apresentar(None));
    }
    if !pode {
        return acoes;
    }
    if tecla(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || tecla(Modifiers::COMMAND, Key::Y) {
        l.refazer(ctx);
    } else if tecla(Modifiers::COMMAND, Key::Z) {
        l.desfazer(ctx);
    }
    if tecla(Modifiers::NONE, Key::Delete) || tecla(Modifiers::NONE, Key::Backspace) {
        acoes.extend(l.apagar_selecao(ctx));
    }
    if tecla(Modifiers::NONE, Key::Enter)
        && let [id] = l.selecao[..]
    {
        l.editar(id);
    }
    if tecla(Modifiers::COMMAND, Key::D) {
        let itens = l.copiar_selecao();
        if !itens.is_empty() {
            l.colar_itens(ctx, &itens, Vec2::splat(CASCATA));
        }
    }
    // Copiar, recortar e colar: o egui manda Ctrl+C e Ctrl+X como Copy e Cut;
    // Ctrl+V com texto como Paste, e sem texto só a tecla V solta.
    let (copiou, recortou, colado, soltou_v) = ctx.input(|i| {
        (
            i.events.iter().any(|e| matches!(e, Event::Copy)),
            i.events.iter().any(|e| matches!(e, Event::Cut)),
            i.events.iter().find_map(|e| if let Event::Paste(t) = e { Some(t.clone()) } else { None }),
            i.events.iter().any(|e| matches!(e, Event::Key { key: Key::V, pressed: false, modifiers, .. } if modifiers.command)),
        )
    });
    if (copiou || recortou) && !l.selecao.is_empty() {
        let itens = l.copiar_selecao();
        let texto = barra::texto_da_area(&itens);
        ctx.copy_text(texto.clone());
        ctx.data_mut(|d| d.insert_temp(Id::new("lousa-area-interna"), (texto, itens)));
        if recortou {
            acoes.extend(l.apagar_selecao(ctx));
        }
    }
    let onde = mouse.unwrap_or_else(|| l.centro_da_vista());
    if let Some(texto) = colado {
        let interna = ctx.data(|d| d.get_temp::<(String, Vec<Elemento>)>(Id::new("lousa-area-interna")));
        match interna {
            // A área de transferência ainda tem o que foi copiado aqui: cola os itens.
            Some((marca, itens)) if marca == texto && !itens.is_empty() => {
                let caixa = super::camera::envolver(itens.iter().filter(|e| e.tipo != TipoElemento::Ligacao).map(caixa_quadro));
                let desloca = match (mouse, caixa) {
                    (Some(m), Some(c)) => m - c.center(),
                    _ => Vec2::splat(CASCATA),
                };
                l.colar_itens(ctx, &itens, desloca);
            }
            _ => {
                let texto: String = texto.chars().take(barra::MAX_TEXTO).collect();
                let id = l.criar(ctx, TipoElemento::Nota, onde, |e| e.texto = texto);
                // A nota colada não entra em edição: fica pronta, do tamanho do texto.
                l.edicao = None;
                let pintor = ctx.layer_painter(egui::LayerId::background());
                l.crescer(&pintor, id);
                let agora = ctx.input(|i| i.time);
                l.agendar(ctx, agora);
            }
        }
    } else if soltou_v {
        acoes.extend(l.colar_imagem(ctx, c.perfil, onde));
    }
    // Criar pela tecla, na posição do mouse.
    if let Some(m) = mouse {
        for (k, tipo) in [(Key::N, TipoElemento::Nota), (Key::T, TipoElemento::Texto), (Key::C, TipoElemento::Codigo)] {
            if tecla(Modifiers::NONE, k) {
                l.criar(ctx, tipo, m, |_| {});
            }
        }
        if tecla(Modifiers::NONE, Key::I) {
            barra::escolher_arquivos(l, ctx, false);
        }
        if tecla(Modifiers::NONE, Key::K) {
            l.busca = Some(barra::Busca::nova(m));
        }
    }
    // As letras não servem a mais ninguém aqui.
    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Text(_))));
    acoes
}

/// A moldura da interação, em px de tela: o contorno de quem está sob o
/// mouse, a seleção, as alças, a caixa, a ligação sendo puxada e os envios.
pub fn pintar_moldura(l: &mut Lousa, ui: &egui::Ui, area: Rect, c: &Contexto) {
    let p = cores();
    let pintor = ui.painter_at(area);
    let em_cima = ui.ctx().data(|d| d.get_temp::<Option<Alvo>>(id_em_cima(l))).flatten();
    let arrastando = l.arrasto.is_some();
    // Mouse em cima (sem seleção).
    if let Some(Alvo::Item(id) | Alvo::Ligar(id, _)) = em_cima
        && !l.selecao.contains(&id)
        && !arrastando
        && let Some(e) = l.modelo.buscar(id)
    {
        let r = na_tela(l, e);
        pintor.rect_stroke(r.expand(2.0), CornerRadius::same(tema::RAIO_CARTAO + 2), Stroke::new(1.0, p.destaque.gamma_multiply(0.55)), StrokeKind::Outside);
    }
    // Seleção.
    let mut uma = None;
    for id in &l.selecao {
        let Some(e) = l.modelo.buscar(*id) else { continue };
        if e.tipo == TipoElemento::Ligacao {
            continue;
        }
        let r = na_tela(l, e);
        pintor.rect_stroke(r.expand(3.0), CornerRadius::same(tema::RAIO_CARTAO + 3), Stroke::new(2.0, p.destaque), StrokeKind::Outside);
        uma = Some(r);
    }
    let editando = l.edicao.is_some();
    if let ([_], Some(r)) = (&l.selecao[..], uma)
        && c.pode_mudar
        && !editando
        && r.width() >= 24.0
        && r.height() >= 24.0
        && !matches!(l.arrasto, Some(Arrasto::Mover { .. }))
    {
        let r = r.expand(3.0);
        for alca in Alca::TODAS {
            let q = Rect::from_center_size(alca.ponto(r), vec2(8.0, 8.0));
            pintor.rect(q, CornerRadius::same(2), p.superficie_alta, Stroke::new(1.5, p.destaque), StrokeKind::Inside);
        }
    }
    // Alças de ligação do item com o mouse em cima.
    if c.pode_mudar
        && !arrastando
        && !editando
        && let Some(Alvo::Item(id) | Alvo::Ligar(id, _) | Alvo::Perto(id)) = em_cima
        && let Some(e) = l.modelo.buscar(id)
    {
        for (i, ponto) in pontos_de_ligar(na_tela(l, e)).iter().enumerate() {
            let raio = if em_cima == Some(Alvo::Ligar(id, i as u8)) { 6.0 } else { ALCA_LIGAR };
            pintor.circle(*ponto, raio, p.destaque, Stroke::new(1.5, p.fundo));
        }
    }
    match &l.arrasto {
        Some(Arrasto::Caixa { inicio, atual, .. }) => {
            let r = Rect::from_two_pos(l.camera.para_tela(area, *inicio), l.camera.para_tela(area, *atual));
            pintor.rect(r, CornerRadius::ZERO, p.destaque.gamma_multiply(0.08), Stroke::new(1.0, p.destaque.gamma_multiply(0.7)), StrokeKind::Inside);
        }
        Some(Arrasto::Ligar { de, ponta }) => {
            if let Some(e) = l.modelo.buscar(*de) {
                let alvo = l.modelo.elementos.iter().rev().find(|x| x.tipo != TipoElemento::Ligacao && x.id != *de && caixa_quadro(x).contains(*ponta));
                let destino = match alvo {
                    Some(a) => {
                        let r = na_tela(l, a);
                        pintor.rect(
                            r.expand(3.0),
                            CornerRadius::same(tema::RAIO_CARTAO + 3),
                            p.destaque.gamma_multiply(0.08),
                            Stroke::new(2.0, p.destaque.gamma_multiply(0.6)),
                            StrokeKind::Outside,
                        );
                        caixa_quadro(a)
                    }
                    None => Rect::from_center_size(*ponta, Vec2::splat(2.0)),
                };
                let pontos = desenho::pontos_na_tela(area, &l.camera, desenho::curva(caixa_quadro(e), destino));
                desenho::tracejado_com_seta(&pintor, &pontos, p.destaque, 2.0);
            }
        }
        _ => {}
    }
    // Imagens e vídeos indo para o núcleo.
    let mut acoes_envio = Vec::new();
    for envio in &l.envios {
        let r = l.camera.retangulo_na_tela(area, Rect::from_min_size(envio.posicao, envio.tamanho));
        let canto = CornerRadius::same(desenho::canto(tema::RAIO_CARTAO as f32, l.camera.zoom) as u8);
        match &envio.erro {
            None => {
                tema::esqueleto(&pintor, r, canto.nw);
                let texto = if envio.video { "Enviando vídeo…" } else { "Enviando imagem…" };
                pintor.text(r.center(), egui::Align2::CENTER_CENTER, texto, egui::FontId::proportional(12.5), p.suave);
            }
            Some(_) => {
                pintor.rect(r, canto, p.superficie_alta, Stroke::new(1.5, p.erro), StrokeKind::Inside);
                acoes_envio.push((envio.numero, r));
            }
        }
    }
    l.envios_com_erro = acoes_envio;
}
