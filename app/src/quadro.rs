//! Quadro de tarefas do projeto: cinco colunas, cartões arrastáveis e lista
//! virtualizada (só os cartões visíveis são desenhados).

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{self, Color32, DragAndDrop, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};

use std::collections::HashMap;

use crate::dados::{Coluna, Tarefa};
use crate::tema::{self, EstadoVisual, RAIO_CARTAO, RAIO_SUPERFICIE, cores, forte};
use crate::terminal::TerminalAgente;

const ESPACO: f32 = 10.0;

/// O que o quadro pede para a tela principal fazer.
pub enum Acao {
    AbrirTarefa(i64),
    /// Um cartão foi arrastado para outra coluna.
    Moveu(i64, Coluna),
    /// Pediram para remover a tarefa (menu do botão direito ou do "⋯").
    Remover(i64),
}

/// Identifica o cartão sendo arrastado.
#[derive(Clone, Copy)]
struct Arrastando(i64);

/// A linha de espera da tarefa só aparece quando nenhum agente dela já diz
/// que espera você (na demonstração, onde o motivo é fixo). Com um agente
/// esperando, a coluna e a linha do agente já contam tudo.
fn mostra_motivo(t: &Tarefa) -> bool {
    t.motivo.is_some() && !t.agentes.iter().any(|a| a.visual().espera_voce())
}

fn altura_cartao(t: &Tarefa, com_projeto: bool) -> f32 {
    let avisos = [mostra_motivo(t), t.erro.is_some()].iter().filter(|a| **a).count() as f32;
    let projeto = if com_projeto { 16.0 } else { 0.0 };
    66.0 + projeto + 19.0 * avisos + 40.0 * t.agentes.len() as f32
}

/// `projeto` = None mostra o perfil inteiro, com o nome do projeto em cada cartão.
/// Sem `pode_mudar` (núcleo fora), nada se arrasta nem se remove.
pub fn mostrar(
    ui: &mut egui::Ui,
    tarefas: &mut Vec<Tarefa>,
    projeto: Option<i64>,
    filtro: Option<&str>,
    terminais: &HashMap<i64, TerminalAgente>,
    pode_mudar: bool,
) -> Vec<Acao> {
    let mut acoes = Vec::new();
    let com_projeto = projeto.is_none();
    let mut mover: Option<(i64, Coluna)> = None;

    let area = ui.available_rect_before_wrap();
    let largura = (area.width() - 4.0 * 10.0) / 5.0;

    for (i, coluna) in Coluna::TODAS.into_iter().enumerate() {
        let caixa = Rect::from_min_size(area.min + vec2(i as f32 * (largura + 10.0), 0.0), vec2(largura, area.height()));
        // A coluna inteira é área de soltar; registrada antes dos cartões,
        // que ficam por cima e continuam recebendo clique e arrasto.
        let zona = ui.interact(caixa, Id::new(("coluna", i)), Sense::hover());
        let recebendo = zona.dnd_hover_payload::<Arrastando>().is_some();
        if let Some(p) = zona.dnd_release_payload::<Arrastando>() {
            mover = Some((p.0, coluna));
        }

        let visiveis: Vec<usize> = tarefas
            .iter()
            .enumerate()
            .filter(|(_, t)| t.coluna == coluna && projeto.is_none_or(|p| t.projeto_id == p) && filtro.is_none_or(|b| t.branch == b))
            .map(|(i, _)| i)
            .collect();

        // Colunas translúcidas: o favo do fundo aparece por trás, bem de leve.
        let p = cores();
        let fundo_coluna = Color32::from_rgba_unmultiplied(p.superficie.r(), p.superficie.g(), p.superficie.b(), 190);
        let pintor = ui.painter();
        pintor.rect_filled(caixa, RAIO_SUPERFICIE, if recebendo { p.destaque.gamma_multiply(0.12) } else { fundo_coluna });
        if recebendo {
            pintor.rect_stroke(caixa, RAIO_SUPERFICIE, Stroke::new(1.5, p.destaque), StrokeKind::Inside);
        }
        let cor_nome = if coluna == Coluna::AguardandoVoce && !visiveis.is_empty() { p.alerta } else { p.texto };
        let nome = pintor.layout_no_wrap(coluna.nome().to_owned(), forte(13.5), cor_nome);
        let topo = caixa.top() + 14.0;
        let x = caixa.left() + 14.0;
        let largura_nome = nome.size().x;
        pintor.galley(pos2(x, topo), nome, cor_nome);
        // Contador numa pílula ao lado do nome.
        let contagem = pintor.layout_no_wrap(visiveis.len().to_string(), FontId::proportional(11.5), p.suave);
        let pilula = Rect::from_min_size(pos2(x + largura_nome + 8.0, topo - 1.0), vec2(contagem.size().x + 12.0, 18.0));
        pintor.rect_filled(pilula, 9.0, p.realce);
        pintor.galley(pilula.center() - contagem.size() / 2.0, contagem, p.suave);

        let mut filho = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(pos2(caixa.left() + 8.0, caixa.top() + 44.0), caixa.max - vec2(8.0, 8.0))));
        let alturas: Vec<f32> = visiveis.iter().map(|&i| altura_cartao(&tarefas[i], com_projeto)).collect();
        let total: f32 = alturas.iter().map(|h| h + ESPACO).sum();
        egui::ScrollArea::vertical().id_salt(("rolagem", i)).auto_shrink(false).show_viewport(&mut filho, |ui, janela| {
            ui.set_height(total);
            let origem = ui.max_rect().min;
            let largura = ui.available_width();
            let mut y = 0.0;
            for (&indice, &h) in visiveis.iter().zip(&alturas) {
                // Virtualização: pula o que está fora da parte visível.
                if y + h >= janela.min.y && y <= janela.max.y {
                    let rect = Rect::from_min_size(origem + vec2(0.0, y), vec2(largura, h));
                    if let Some(a) = cartao(ui, rect, &tarefas[indice], terminais, com_projeto, pode_mudar) {
                        acoes.push(a);
                    }
                }
                y += h + ESPACO;
            }
        });
    }

    // O cartão solto vai para o fim da coluna de destino.
    if let Some((id, coluna)) = mover
        && let Some(i) = tarefas.iter().position(|t| t.id == id)
    {
        let mut t = tarefas.remove(i);
        if t.coluna != coluna {
            acoes.push(Acao::Moveu(id, coluna));
        }
        t.coluna = coluna;
        tarefas.push(t);
    }
    acoes
}

/// Uma linha de texto cortada com reticências se não couber na largura.
fn linha_cortada(pintor: &egui::Painter, pos: Pos2, texto: &str, fonte: FontId, cor: Color32, largura: f32) -> f32 {
    let mut trabalho = LayoutJob::simple_singleline(texto.to_owned(), fonte, cor);
    trabalho.wrap = TextWrapping { max_width: largura, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let galeria = pintor.layout_job(trabalho);
    let altura = galeria.size().y;
    pintor.galley(pos, galeria, cor);
    altura
}

fn cartao(ui: &mut egui::Ui, rect: Rect, t: &Tarefa, terminais: &HashMap<i64, TerminalAgente>, com_projeto: bool, pode_mudar: bool) -> Option<Acao> {
    let p = cores();
    let arrastado = DragAndDrop::payload::<Arrastando>(ui.ctx()).is_some_and(|p| p.0 == t.id);
    let em_cima = ui.rect_contains_pointer(rect) && !arrastado;
    let pintor = ui.painter();
    let fundo = if arrastado { p.superficie_alta.gamma_multiply(0.45) } else { p.superficie_alta };
    let contorno = if em_cima { p.destaque.gamma_multiply(0.55) } else { p.borda };
    pintor.rect(rect, RAIO_CARTAO, fundo, Stroke::new(1.0, contorno), StrokeKind::Inside);
    if t.erro.is_some() {
        // Erro como faixa na lateral: chama atenção sem pintar o cartão inteiro.
        // Só o erro tem faixa: "aguardando" já é dito pela coluna.
        let faixa = Rect::from_min_size(rect.min + vec2(0.0, 10.0), vec2(3.0, rect.height() - 20.0));
        pintor.rect_filled(faixa, 2.0, p.erro);
    }

    let x = rect.left() + 14.0;
    let largura = rect.width() - 28.0;
    let mut y = rect.top() + 12.0;
    if com_projeto {
        linha_cortada(pintor, pos2(x, y), &t.projeto, FontId::proportional(11.5), p.suave, largura);
        y += 16.0;
    }
    linha_cortada(pintor, pos2(x, y), &t.titulo, forte(13.5), p.texto, largura);
    y += 22.0;

    // Número da tarefa e a branch numa etiqueta (uma pasta sem git não tem branch).
    let numero = pintor.layout_no_wrap(format!("#{}", t.id), FontId::proportional(11.5), p.suave);
    let largura_numero = numero.size().x;
    pintor.galley(pos2(x, y + 2.0), numero, p.suave);
    let (texto, cor) = if t.branch.is_empty() { ("pasta", p.suave) } else { (t.branch.as_str(), p.destaque) };
    tema::etiqueta(pintor, pos2(x + largura_numero + 8.0, y), texto, FontId::monospace(11.0), cor);
    let y_etiqueta = y;
    y += 28.0;

    if let Some(erro) = &t.erro {
        linha_cortada(pintor, pos2(x, y), &format!("Erro: {erro}"), FontId::proportional(12.0), p.erro, largura);
        y += 19.0;
    }
    if let Some(motivo) = t.motivo.as_ref().filter(|_| mostra_motivo(t)) {
        linha_cortada(pintor, pos2(x, y), &format!("Aguardando: {motivo}"), FontId::proportional(12.0), p.alerta, largura);
        y += 19.0;
    }
    for agente in &t.agentes {
        let terminal = terminais.get(&agente.id).filter(|t| !t.encerrado());
        // Na demonstração os agentes são os terminais de teste: o estado vem deles.
        let estado = if agente.fim.is_none() && agente.ativo && agente.desde.is_empty() && agente.motivo.is_empty() {
            if terminal.is_some() { EstadoVisual::Trabalhando } else { EstadoVisual::Terminou }
        } else {
            agente.visual()
        };
        y += 2.0;
        tema::ponto(pintor, pos2(x + 3.5, y + 8.0), 3.5, estado);
        let nome = pintor.layout_no_wrap(agente.nome(), FontId::proportional(12.5), p.texto);
        let largura_nome = nome.size().x;
        pintor.galley(pos2(x + 13.0, y), nome, p.texto);
        pintor.text(pos2(x + 19.0 + largura_nome, y), egui::Align2::LEFT_TOP, &agente.papel, FontId::proportional(12.0), p.suave);
        y += 18.0;
        // Trabalhando: a última linha do terminal. Fora disso, o estado (que não muda a cada quadro).
        if estado == EstadoVisual::Trabalhando {
            // O fim da linha (o prompt, o caminho) é o que importa: corta pelo começo.
            let ultima = terminal.map_or_else(String::new, |t| t.ultima_linha());
            tema::texto_sem_inicio(pintor, pos2(x + 13.0, y), &ultima, FontId::monospace(11.0), p.suave, largura - 13.0);
        } else {
            let (texto, hora) = if agente.ativo || agente.fim.is_some() { agente.estado_curto() } else { ("Parado".to_string(), String::new()) };
            tema::texto_com_fim(pintor, pos2(x + 13.0, y), &texto, &hora, FontId::proportional(12.0), estado.cor(), p.suave, largura - 13.0);
        }
        y += 20.0;
    }

    // Registrado depois do conteúdo para ficar por cima e receber clique e arrasto.
    let resposta = ui.interact(rect, Id::new(("cartao", t.id)), Sense::click_and_drag());
    let resposta = if pode_mudar { resposta.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resposta };
    // Só o botão esquerdo arrasta; o direito abre o menu do cartão. Sem o núcleo, nada se move.
    if pode_mudar && resposta.drag_started_by(egui::PointerButton::Primary) {
        DragAndDrop::set_payload(ui.ctx(), Arrastando(t.id));
    }

    if arrastado && let Some(pos) = ui.ctx().pointer_interact_pos() {
        // Cópia do cartão seguindo o ponteiro, com sombra para parecer "no ar".
        let camada = egui::LayerId::new(egui::Order::Tooltip, Id::new("cartao-arrastado"));
        let fantasma = Rect::from_min_size(pos - vec2(20.0, 16.0), vec2(rect.width(), 40.0));
        let pintor = ui.ctx().layer_painter(camada);
        let sombra = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(70) };
        pintor.add(sombra.as_shape(fantasma, RAIO_CARTAO));
        pintor.rect(fantasma, RAIO_CARTAO, p.superficie_alta, Stroke::new(1.0, p.destaque), StrokeKind::Inside);
        linha_cortada(&pintor, fantasma.left_center() + vec2(14.0, -9.0), &t.titulo, forte(13.5), p.texto, fantasma.width() - 28.0);
    }

    let mut acao = resposta.clicked().then_some(Acao::AbrirTarefa(t.id));
    let menu = |ui: &mut egui::Ui, acao: &mut Option<Acao>| {
        ui.set_min_width(200.0);
        if tema::opcao_menu(ui, "Abrir tarefa", false) {
            *acao = Some(Acao::AbrirTarefa(t.id));
            ui.close();
        }
        if tema::opcao_menu_com(ui, "Remover tarefa…", None, pode_mudar) {
            *acao = Some(Acao::Remover(t.id));
            ui.close();
        }
    };
    resposta.context_menu(|ui| menu(ui, &mut acao));
    // O "⋯" só aparece com o mouse em cima, na linha do número (não corta o
    // título nem faz o texto pular). Registrado depois do cartão, fica por cima.
    let menu_aberto = egui::Popup::is_id_open(ui.ctx(), Id::new(("menu-cartao", t.id)));
    if em_cima || menu_aberto {
        let botao = Rect::from_min_size(pos2(rect.right() - 10.0 - 24.0, y_etiqueta + 9.5 - 12.0), vec2(24.0, 24.0));
        let mais = tema::botao_icone_em(ui, botao, Id::new(("mais-cartao", t.id)), tema::Icone::Mais);
        if mais.clicked() {
            acao = None;
        }
        egui::Popup::menu(&mais).id(Id::new(("menu-cartao", t.id))).show(|ui| menu(ui, &mut acao));
    }
    acao
}
