//! Quadro de tarefas do projeto: cinco colunas, cartões arrastáveis e lista
//! virtualizada (só os cartões visíveis são desenhados). O resumo do topo
//! ("2 esperam você · 1 com erro") destaca no quadro só as tarefas dele, e o
//! Concluído mostra só o que terminou hoje; o resto fica no Histórico.

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{self, Color32, CornerRadius, DragAndDrop, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};

use std::collections::{HashMap, HashSet};

use crate::dados::{AgenteTela, Coluna, Tarefa};
use crate::tema::{self, EstadoVisual, Icone, RAIO_CARTAO, RAIO_SUPERFICIE, cores, forte};
use crate::terminal::TerminalAgente;

const ESPACO: f32 = 10.0;
const CABECALHO: f32 = 46.0;
const RODAPE: f32 = 24.0;
const VAZIA: f32 = 80.0;
const LINK_HISTORICO: f32 = 32.0;
/// Quanto um cartão fora do destaque do resumo fica visível.
const APAGADO: f32 = 0.32;

/// O que o quadro pede para a tela principal fazer.
pub enum Acao {
    AbrirTarefa(i64),
    /// Um cartão foi arrastado para outra coluna.
    Moveu(i64, Coluna),
    /// Pediram para remover a tarefa (menu do botão direito ou do "⋯").
    Remover(i64),
    /// O "+" do cabeçalho: uma tarefa nova que já entra nesta coluna.
    NovaNaColuna(Coluna),
    /// "Mais N no Histórico", no fim do Concluído.
    VerHistorico,
}

/// Os grupos do resumo do topo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grupo {
    Espera,
    Trabalhando,
    Erro,
}

/// Quantas tarefas à mostra estão em cada grupo do resumo.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Resumo {
    pub espera: usize,
    pub trabalhando: usize,
    pub erro: usize,
}

impl Resumo {
    fn de(&self, g: Grupo) -> usize {
        match g {
            Grupo::Espera => self.espera,
            Grupo::Trabalhando => self.trabalhando,
            Grupo::Erro => self.erro,
        }
    }
}

/// O que o quadro guarda entre os quadros.
#[derive(Default)]
pub struct Quadro {
    /// O grupo do resumo em destaque (os outros cartões ficam apagados).
    pub foco: Option<Grupo>,
    /// Tarefas que você soltou no Concluído nesta sessão: ficam no Concluído
    /// de hoje mesmo sem um agente que tenha terminado hoje.
    concluidas_agora: HashSet<i64>,
}

/// O que a tela principal passa para desenhar o quadro.
pub struct Vista<'a> {
    /// Os projetos à mostra (um, os de um workspace ou todos do perfil).
    pub no_escopo: &'a HashSet<i64>,
    /// O nome do projeto em cada cartão (por id, com o workspace na frente
    /// quando o nome se repete no recorte); None na visão de um projeto só.
    pub rotulos: Option<&'a HashMap<i64, String>>,
    pub filtro: Option<&'a str>,
    pub terminais: &'a HashMap<i64, TerminalAgente>,
    /// Tarefas cuja frase da reunião o agente está escrevendo.
    pub frases: &'a HashSet<i64>,
    /// Sem o núcleo, nada se arrasta nem se remove.
    pub pode_mudar: bool,
    /// O "+" das colunas cria tarefas (fora da demonstração).
    pub pode_criar: bool,
    /// O Histórico existe (fora da demonstração): o Concluído leva até ele.
    pub com_historico: bool,
    /// "2026-10-16": o dia de hoje, no formato do fim do agente.
    pub hoje: &'a str,
}

impl Vista<'_> {
    fn mostra(&self, t: &Tarefa) -> bool {
        self.no_escopo.contains(&t.projeto_id) && self.filtro.is_none_or(|b| t.branch == b)
    }
}

/// O estado do agente no cartão. Na demonstração os agentes são os
/// terminais de teste: o estado vem deles.
pub fn estado_do_agente(agente: &AgenteTela, terminais: &HashMap<i64, TerminalAgente>) -> EstadoVisual {
    if agente.fim.is_none() && agente.ativo && agente.desde.is_empty() && agente.motivo.is_empty() {
        let rodando = terminais.get(&agente.id).is_some_and(|t| !t.encerrado());
        if rodando { EstadoVisual::Trabalhando } else { EstadoVisual::Terminou }
    } else {
        agente.visual()
    }
}

/// Em que grupo do resumo a tarefa entra: o erro antes de tudo, depois o
/// que espera você e por fim o que tem agente trabalhando.
pub fn grupo(t: &Tarefa, terminais: &HashMap<i64, TerminalAgente>) -> Option<Grupo> {
    if t.erro.is_some() {
        return Some(Grupo::Erro);
    }
    if t.coluna == Coluna::AguardandoVoce || t.agentes.iter().any(|a| a.visual().espera_voce()) {
        return Some(Grupo::Espera);
    }
    t.agentes.iter().any(|a| estado_do_agente(a, terminais) == EstadoVisual::Trabalhando).then_some(Grupo::Trabalhando)
}

pub fn resumir<'a>(tarefas: impl Iterator<Item = &'a Tarefa>, terminais: &HashMap<i64, TerminalAgente>) -> Resumo {
    let mut r = Resumo::default();
    for t in tarefas {
        match grupo(t, terminais) {
            Some(Grupo::Espera) => r.espera += 1,
            Some(Grupo::Trabalhando) => r.trabalhando += 1,
            Some(Grupo::Erro) => r.erro += 1,
            None => {}
        }
    }
    r
}

/// A tarefa terminou hoje: um agente dela terminou hoje (a data vem do
/// núcleo) ou você a soltou no Concluído agora há pouco.
fn de_hoje(t: &Tarefa, hoje: &str, agora: &HashSet<i64>) -> bool {
    agora.contains(&t.id) || t.agentes.iter().any(|a| a.fim.as_ref().is_some_and(|f| f.data == hoje))
}

/// A etapa da tarefa no caminho que ela anda sozinha (0: Trabalhando ⇄
/// Aguardando, 1: Revisão, 2: Concluído), pelas regras do núcleo: precisa de
/// um agente (o terminal comum não conta); de Revisão só sai com um revisor;
/// um Aguardando ou Concluído posto por você fica onde está.
pub fn anda_sozinha(t: &Tarefa) -> Option<u8> {
    let mut agentes = t.agentes.iter().filter(|a| a.ferramenta != "shell").peekable();
    agentes.peek()?;
    match t.coluna {
        Coluna::Trabalhando => Some(0),
        Coluna::AguardandoVoce if t.coluna_auto => Some(0),
        Coluna::Revisao if agentes.any(|a| a.papel == "revisor") => Some(1),
        Coluna::Concluido if t.coluna_auto => Some(2),
        _ => None,
    }
}

/// A linha de espera da tarefa só aparece quando nenhum agente dela já diz
/// que espera você (na demonstração, onde o motivo é fixo). Com um agente
/// esperando, a coluna e a linha do agente já contam tudo.
fn mostra_motivo(t: &Tarefa) -> bool {
    t.motivo.is_some() && !t.agentes.iter().any(|a| a.visual().espera_voce())
}

/// O rodapé do cartão: a marca "sozinha" e o "Responder" ou "Ver o erro".
/// O rodapé: a marca "sozinha" e o que fazer (responder, ver o erro ou, em
/// Revisão, concluir sem precisar de revisor).
fn tem_rodape(t: &Tarefa, grupo: Option<Grupo>) -> bool {
    anda_sozinha(t).is_some() || matches!(grupo, Some(Grupo::Espera | Grupo::Erro)) || t.coluna == Coluna::Revisao
}

fn altura_cartao(t: &Tarefa, com_projeto: bool, frase: bool, rodape: bool) -> f32 {
    let avisos = [mostra_motivo(t), t.erro.is_some(), frase].iter().filter(|a| **a).count() as f32;
    let projeto = if com_projeto { 16.0 } else { 0.0 };
    let rodape = if rodape { RODAPE } else { 0.0 };
    66.0 + projeto + 19.0 * avisos + 40.0 * t.agentes.len() as f32 + rodape
}

/// O ícone, a cor e o texto da coluna vazia.
fn jeito(coluna: Coluna) -> (Icone, Color32, &'static str) {
    let p = cores();
    match coluna {
        Coluna::Backlog => (Icone::Coluna, p.suave, "Sem tarefas. Crie uma em \"Nova tarefa\"."),
        Coluna::Trabalhando => (Icone::Agente, p.ok, "Nenhum agente rodando agora."),
        Coluna::AguardandoVoce => (Icone::Relogio, p.alerta, "Nada esperando você."),
        Coluna::Revisao => (Icone::Olho, p.destaque, "Arraste para cá o que precisa de revisão."),
        Coluna::Concluido => (Icone::Check, p.destaque, "Nada concluído hoje."),
    }
}

/// O resumo do topo: um item por grupo com tarefas. O clique destaca o
/// grupo no quadro; de novo, mostra tudo.
pub fn mostrar_resumo(ui: &mut egui::Ui, r: Resumo, foco: &mut Option<Grupo>) {
    let p = cores();
    if foco.is_some_and(|g| r.de(g) == 0) {
        *foco = None;
    }
    let itens = [
        (Grupo::Espera, if r.espera == 1 { "1 espera você".to_string() } else { format!("{} esperam você", r.espera) }, EstadoVisual::SuaVez),
        (Grupo::Trabalhando, format!("{} trabalhando", r.trabalhando), EstadoVisual::Trabalhando),
        (Grupo::Erro, format!("{} com erro", r.erro), EstadoVisual::Erro),
    ];
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (g, texto, estado) in itens {
            if r.de(g) == 0 {
                continue;
            }
            let ativo = *foco == Some(g);
            let cor = estado.cor();
            let galeria = ui.painter().layout_no_wrap(texto, if g == Grupo::Espera { forte(12.5) } else { FontId::proportional(12.5) }, p.texto);
            let (rect, resposta) = ui.allocate_exact_size(vec2(galeria.size().x + 34.0, 26.0), Sense::click());
            let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if ativo { "Mostrar tudo" } else { "Destacar no quadro" });
            let base = if g == Grupo::Espera { cor.gamma_multiply(0.1) } else { Color32::TRANSPARENT };
            let fundo = tema::cor_animada(ui, resposta.id.with("ativo"), ativo, base, cor.gamma_multiply(0.16));
            let fundo = if resposta.hovered() && !ativo { tema::misturar(fundo, p.realce, 0.6) } else { fundo };
            let borda = tema::cor_animada(ui, resposta.id.with("borda"), ativo, Color32::TRANSPARENT, cor.gamma_multiply(0.5));
            ui.painter().rect(rect, CornerRadius::same(13), fundo, Stroke::new(1.0, borda), StrokeKind::Inside);
            tema::ponto(ui.painter(), pos2(rect.left() + 14.0, rect.center().y), 3.5, estado);
            let cor_texto = if g == Grupo::Espera {
                p.alerta
            } else if ativo {
                p.texto
            } else {
                p.suave
            };
            ui.painter().galley(pos2(rect.left() + 24.0, rect.center().y - galeria.size().y / 2.0), galeria, cor_texto);
            if resposta.clicked() {
                *foco = if ativo { None } else { Some(g) };
            }
        }
    });
}

pub fn mostrar(ui: &mut egui::Ui, estado: &mut Quadro, tarefas: &mut Vec<Tarefa>, v: &Vista) -> Vec<Acao> {
    let mut acoes = Vec::new();
    let mut mover: Option<(i64, Coluna)> = None;
    let com_projeto = v.rotulos.is_some();

    let area = ui.available_rect_before_wrap();
    // As colunas do meio (o que roda e o que espera você) são um pouco mais largas.
    let pesos = Coluna::TODAS.map(|c| if matches!(c, Coluna::Trabalhando | Coluna::AguardandoVoce) { 1.1 } else { 0.95 });
    let unidade = (area.width() - 4.0 * 10.0) / pesos.iter().sum::<f32>();
    let p = cores();
    let mut esquerda = area.left();

    for (i, coluna) in Coluna::TODAS.into_iter().enumerate() {
        let largura = unidade * pesos[i];
        let caixa = Rect::from_min_size(pos2(esquerda, area.top()), vec2(largura, area.height()));
        esquerda += largura + 10.0;
        // A coluna inteira é área de soltar; registrada antes dos cartões,
        // que ficam por cima e continuam recebendo clique e arrasto.
        let zona = ui.interact(caixa, Id::new(("coluna", i)), Sense::hover());
        let recebendo = zona.dnd_hover_payload::<Arrastando>().is_some();
        if let Some(arrastado) = zona.dnd_release_payload::<Arrastando>() {
            mover = Some((arrastado.0, coluna));
        }

        let mut visiveis: Vec<usize> = tarefas.iter().enumerate().filter(|(_, t)| t.coluna == coluna && v.mostra(t)).map(|(i, _)| i).collect();
        // O Concluído é só o de hoje; o resto fica no Histórico.
        let mut mais_antigas = 0;
        if coluna == Coluna::Concluido {
            let antes = visiveis.len();
            visiveis.retain(|&i| de_hoje(&tarefas[i], v.hoje, &estado.concluidas_agora));
            mais_antigas = antes - visiveis.len();
        }

        let (icone, cor, vazia) = jeito(coluna);
        // "Aguardando você" com cartões vem em âmbar: é o que pede você.
        let espera = coluna == Coluna::AguardandoVoce && !visiveis.is_empty();
        // Colunas translúcidas: o favo do fundo aparece por trás, bem de leve.
        let translucida = Color32::from_rgba_unmultiplied(p.superficie.r(), p.superficie.g(), p.superficie.b(), 190);
        let (tinta, borda) = if recebendo {
            (p.destaque.gamma_multiply(0.12), Stroke::new(1.5, p.destaque))
        } else if espera {
            (p.alerta.gamma_multiply(0.08), Stroke::new(1.0, p.alerta.gamma_multiply(0.4)))
        } else {
            (Color32::TRANSPARENT, Stroke::NONE)
        };
        let pintor = ui.painter();
        if !recebendo {
            pintor.rect_filled(caixa, RAIO_SUPERFICIE, translucida);
        }
        pintor.rect(caixa, RAIO_SUPERFICIE, tinta, borda, StrokeKind::Inside);

        // Cabeçalho: ícone, nome, "hoje" no Concluído e a contagem.
        let meio = caixa.top() + 23.0;
        let x = caixa.left() + 10.0;
        let selo = Rect::from_min_size(pos2(x, meio - 11.0), vec2(22.0, 22.0));
        pintor.rect_filled(selo, CornerRadius::same(7), cor.gamma_multiply(0.15));
        tema::desenhar_icone(pintor, selo.center(), icone, cor);
        let cor_nome = if espera { p.alerta } else { p.texto };
        let largura_nome = linha_cortada(pintor, pos2(x + 29.0, meio - 9.0), coluna.nome(), forte(13.5), cor_nome, largura - 29.0 - 10.0 - 34.0);
        let mut fim_nome = x + 29.0 + largura_nome;
        if coluna == Coluna::Concluido {
            let hoje = pintor.layout_no_wrap("hoje".into(), FontId::proportional(11.5), p.suave);
            fim_nome += 6.0;
            pintor.galley(pos2(fim_nome, meio - hoje.size().y / 2.0), hoje.clone(), p.suave);
            fim_nome += hoje.size().x;
        }
        let (fundo_n, cor_n) = if espera { (p.alerta.gamma_multiply(0.22), p.alerta) } else { (p.realce, p.suave) };
        let contagem = pintor.layout_no_wrap(visiveis.len().to_string(), forte(11.0), cor_n);
        let pilula = Rect::from_min_size(pos2(fim_nome + 7.0, meio - 9.0), vec2(contagem.size().x.max(6.0) + 12.0, 18.0));
        pintor.rect_filled(pilula, 9.0, fundo_n);
        pintor.galley(pilula.center() - contagem.size() / 2.0, contagem, cor_n);
        // O "+" aparece com o mouse na coluna: a tarefa nova já entra nela.
        // Com fundo próprio: numa coluna estreita, ele fica por cima do fim do nome.
        if v.pode_criar && ui.rect_contains_pointer(caixa) && DragAndDrop::payload::<Arrastando>(ui.ctx()).is_none() {
            let botao = Rect::from_min_size(pos2(caixa.right() - 8.0 - 24.0, meio - 12.0), vec2(24.0, 24.0));
            pintor.rect_filled(botao.expand(2.0), CornerRadius::same(8), p.superficie_alta);
            let mais = tema::botao_icone_em(ui, botao, Id::new(("nova-na-coluna", i)), Icone::MaisZoom).on_hover_text("Nova tarefa nesta coluna");
            if mais.clicked() {
                acoes.push(Acao::NovaNaColuna(coluna));
            }
        }

        let mut filho =
            ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(pos2(caixa.left() + 7.0, caixa.top() + CABECALHO), caixa.max - vec2(7.0, 8.0))));
        let grupos: Vec<Option<Grupo>> = visiveis.iter().map(|&i| grupo(&tarefas[i], v.terminais)).collect();
        let alturas: Vec<f32> = visiveis
            .iter()
            .zip(&grupos)
            .map(|(&i, g)| altura_cartao(&tarefas[i], com_projeto, v.frases.contains(&tarefas[i].id), tem_rodape(&tarefas[i], *g)))
            .collect();
        let link = v.com_historico && mais_antigas > 0;
        let total: f32 =
            alturas.iter().map(|h| h + ESPACO).sum::<f32>() + if visiveis.is_empty() { VAZIA + ESPACO } else { 0.0 } + if link { LINK_HISTORICO } else { 0.0 };
        egui::ScrollArea::vertical().id_salt(("rolagem", i)).auto_shrink(false).show_viewport(&mut filho, |ui, janela| {
            ui.set_height(total);
            let origem = ui.max_rect().min;
            let largura = ui.available_width();
            let mut y = 0.0;
            for ((&indice, &h), g) in visiveis.iter().zip(&alturas).zip(&grupos) {
                // Virtualização: pula o que está fora da parte visível.
                if y + h >= janela.min.y && y <= janela.max.y {
                    let rect = Rect::from_min_size(origem + vec2(0.0, y), vec2(largura, h));
                    let t = &tarefas[indice];
                    // Fora do destaque do resumo, o cartão apaga (com transição).
                    let aceso = estado.foco.is_none_or(|f| *g == Some(f));
                    let luz = ui.ctx().animate_bool_with_time(Id::new(("cartao-aceso", t.id)), aceso, tema::TRANSICAO);
                    let mut cartao_ui = ui.new_child(UiBuilder::new().max_rect(rect));
                    cartao_ui.multiply_opacity(APAGADO + (1.0 - APAGADO) * luz);
                    if let Some(a) = cartao(&mut cartao_ui, rect, t, *g, v) {
                        acoes.push(a);
                    }
                }
                y += h + ESPACO;
            }
            if visiveis.is_empty() {
                coluna_vazia(ui.painter(), Rect::from_min_size(origem, vec2(largura, VAZIA)), vazia);
                y += VAZIA + ESPACO;
            }
            if link {
                let rect = Rect::from_min_size(origem + vec2(0.0, y), vec2(largura, LINK_HISTORICO - 4.0));
                let texto = if mais_antigas == 1 { "Mais 1 no Histórico".to_string() } else { format!("Mais {mais_antigas} no Histórico") };
                if link_historico(ui, rect, &texto).clicked() {
                    acoes.push(Acao::VerHistorico);
                }
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
            if coluna == Coluna::Concluido {
                estado.concluidas_agora.insert(id);
            }
        }
        t.coluna = coluna;
        tarefas.push(t);
    }
    acoes
}

/// Identifica o cartão sendo arrastado.
#[derive(Clone, Copy)]
struct Arrastando(i64);

/// A coluna sem cartões: uma caixa tracejada com o que costuma entrar nela.
fn coluna_vazia(pintor: &egui::Painter, rect: Rect, texto: &str) {
    let p = cores();
    let r = rect.shrink(0.5);
    let caminho = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for forma in egui::Shape::dashed_line(&caminho, Stroke::new(1.0, p.borda), 5.0, 4.0) {
        pintor.add(forma);
    }
    let galeria = tema::cortar(pintor, texto, egui::TextFormat::simple(FontId::proportional(12.5), p.suave), rect.width() - 28.0, 3, false);
    pintor.galley(rect.center() - galeria.size() / 2.0, galeria, p.suave);
}

/// "Mais N no Histórico": relógio, texto e seta, com fundo ao passar o mouse.
fn link_historico(ui: &mut egui::Ui, rect: Rect, texto: &str) -> egui::Response {
    let p = cores();
    let resposta = ui.interact(rect, Id::new("mais-no-historico"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    tema::fundo_ao_passar(ui, &resposta, rect, tema::RAIO_CONTROLE, true);
    let cor = tema::cor_animada(ui, resposta.id.with("cor"), resposta.hovered(), p.suave, p.texto);
    let meio = rect.center().y;
    tema::desenhar_icone(ui.painter(), pos2(rect.left() + 14.0, meio), Icone::Relogio, cor);
    ui.painter().text(pos2(rect.left() + 28.0, meio), egui::Align2::LEFT_CENTER, texto, FontId::proportional(12.5), cor);
    tema::desenhar_icone(ui.painter(), pos2(rect.right() - 12.0, meio), Icone::Proximo, cor);
    resposta
}

/// Uma linha de texto cortada com reticências se não couber na largura.
/// Devolve a largura que ocupou.
fn linha_cortada(pintor: &egui::Painter, pos: Pos2, texto: &str, fonte: FontId, cor: Color32, largura: f32) -> f32 {
    let mut trabalho = LayoutJob::simple_singleline(texto.to_owned(), fonte, cor);
    trabalho.wrap = TextWrapping { max_width: largura.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let galeria = pintor.layout_job(trabalho);
    let ocupou = galeria.size().x;
    pintor.galley(pos, galeria, cor);
    ocupou
}

/// A etiqueta do tipo pela branch e o nome da branch ao lado (cortado).
/// Uma branch de outro tipo vai inteira numa etiqueta âmbar; uma pasta sem
/// git não tem branch.
fn etiqueta_da_branch(pintor: &egui::Painter, pos: Pos2, branch: &str, largura: f32) {
    let p = cores();
    if branch.is_empty() {
        tema::etiqueta(pintor, pos, "pasta", FontId::proportional(11.0), p.suave);
        return;
    }
    match tema::tipo_da_branch(branch) {
        Some((tipo, cor)) => {
            let rect = tema::etiqueta(pintor, pos, tipo, forte(11.0), cor);
            if branch != tipo {
                let resto = largura - rect.width() - 6.0;
                if resto > 48.0 {
                    linha_cortada(pintor, pos2(rect.right() + 6.0, pos.y + 3.0), branch, FontId::monospace(11.0), p.suave, resto);
                }
            }
        }
        None => {
            tema::etiqueta(pintor, pos, branch, FontId::monospace(11.0), p.etiquetas[3]);
        }
    }
}

/// O trilho mínimo de "anda sozinha": Trabalho · Revisão · Concluído.
fn trilho_sozinha(pintor: &egui::Painter, pos: Pos2, etapa: u8) -> f32 {
    let p = cores();
    let mut x = pos.x;
    for i in 0..3u8 {
        let largura = if i == etapa { 16.0 } else { 11.0 };
        let cor = match i.cmp(&etapa) {
            std::cmp::Ordering::Less => p.destaque.gamma_multiply(0.55),
            std::cmp::Ordering::Equal => p.destaque,
            std::cmp::Ordering::Greater => p.realce,
        };
        pintor.rect_filled(Rect::from_min_size(pos2(x, pos.y - 2.0), vec2(largura, 4.0)), CornerRadius::same(2), cor);
        x += largura + 2.0;
    }
    x - pos.x
}

/// A dica da marca "sozinha": o caminho e onde a tarefa está nele.
fn dica_sozinha(ui: &mut egui::Ui, etapa: u8) {
    let p = cores();
    ui.set_max_width(270.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        tema::desenhar_icone(ui.painter(), rect.center(), Icone::Ciclo, p.destaque);
        ui.label(tema::texto_forte("Esta tarefa anda sozinha", 13.0).color(p.texto));
    });
    ui.add_space(4.0);
    let etapas = [
        ("Trabalhando ou Aguardando", "o agente trabalha ou pede você"),
        ("Revisão", "quando os agentes terminam bem"),
        ("Concluído", "quando o revisor termina"),
    ];
    for (i, (nome, quando)) in etapas.into_iter().enumerate() {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(10.0, 30.0), Sense::hover());
            let feita = i as u8 <= etapa;
            if feita {
                ui.painter().circle_filled(rect.center(), 4.0, p.destaque);
            } else {
                ui.painter().circle_stroke(rect.center(), 3.25, Stroke::new(1.5, p.suave));
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let nome = egui::RichText::new(nome).size(12.5).color(p.texto);
                ui.label(if i as u8 == etapa { nome.strong() } else { nome });
                ui.label(egui::RichText::new(quando).size(11.5).color(p.suave));
            });
        });
    }
    ui.add_space(2.0);
    ui.label(egui::RichText::new("Mover o cartão à mão continua mandando.").size(11.5).color(p.suave));
}

fn cartao(ui: &mut egui::Ui, rect: Rect, t: &Tarefa, grupo: Option<Grupo>, v: &Vista) -> Option<Acao> {
    let p = cores();
    let arrastado = DragAndDrop::payload::<Arrastando>(ui.ctx()).is_some_and(|p| p.0 == t.id);
    let em_cima = ui.rect_contains_pointer(rect) && !arrastado;
    let pintor = ui.painter();
    let fundo = if arrastado { p.superficie_alta.gamma_multiply(0.45) } else { p.superficie_alta };
    let espera = grupo == Some(Grupo::Espera);
    let contorno = if em_cima {
        p.destaque.gamma_multiply(0.55)
    } else if espera {
        p.alerta.gamma_multiply(0.45)
    } else {
        p.borda
    };
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
    if let Some(rotulos) = v.rotulos {
        let rotulo = rotulos.get(&t.projeto_id).map_or(t.projeto.as_str(), String::as_str);
        linha_cortada(pintor, pos2(x, y), rotulo, FontId::proportional(11.5), p.suave, largura);
        y += 16.0;
    }
    linha_cortada(pintor, pos2(x, y), &t.titulo, forte(13.5), p.texto, largura);
    y += 22.0;

    // O número da tarefa e o tipo pela branch.
    let numero = pintor.layout_no_wrap(format!("#{}", t.id), FontId::proportional(11.5), p.suave);
    let largura_numero = numero.size().x;
    pintor.galley(pos2(x, y + 2.0), numero, p.suave);
    // O "⋯" fica na ponta desta linha: a branch não passa por baixo dele.
    etiqueta_da_branch(pintor, pos2(x + largura_numero + 8.0, y), &t.branch, largura - largura_numero - 8.0 - 26.0);
    let y_etiqueta = y;
    y += 28.0;

    if let Some(erro) = &t.erro {
        linha_cortada(pintor, pos2(x, y), &format!("Erro: {erro}"), FontId::proportional(12.0), p.erro, largura);
        y += 19.0;
    }
    if v.frases.contains(&t.id) {
        linha_cortada(pintor, pos2(x, y), "Escrevendo a frase…", FontId::proportional(12.0), p.suave, largura);
        y += 19.0;
    }
    if let Some(motivo) = t.motivo.as_ref().filter(|_| mostra_motivo(t)) {
        linha_cortada(pintor, pos2(x, y), &format!("Aguardando: {motivo}"), FontId::proportional(12.0), p.alerta, largura);
        y += 19.0;
    }
    for agente in &t.agentes {
        let terminal = v.terminais.get(&agente.id).filter(|t| !t.encerrado());
        let estado = estado_do_agente(agente, v.terminais);
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
    let resposta = if v.pode_mudar { resposta.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resposta };

    // O rodapé: a marca "sozinha" (a dica explica o caminho) e o que fazer.
    let mut concluir = false;
    if tem_rodape(t, grupo) {
        let meio = y + 2.0 + RODAPE / 2.0 - 4.0;
        if let Some(etapa) = anda_sozinha(t) {
            let curto = espera;
            let mut fim = x + 15.0;
            tema::desenhar_icone(pintor, pos2(x + 6.0, meio), Icone::Ciclo, p.destaque);
            if !curto {
                let texto = pintor.layout_no_wrap("sozinha".into(), FontId::proportional(11.5), p.destaque);
                fim += texto.size().x;
                pintor.galley(pos2(x + 17.0, meio - texto.size().y / 2.0), texto, p.destaque);
            }
            fim += 6.0;
            fim += trilho_sozinha(pintor, pos2(fim, meio), etapa);
            let marca = Rect::from_min_max(pos2(x - 4.0, meio - 10.0), pos2(fim + 4.0, meio + 10.0));
            let dica = ui.interact(marca, Id::new(("sozinha", t.id)), Sense::hover());
            if dica.hovered() {
                ui.painter().rect_filled(marca, CornerRadius::same(6), p.destaque.gamma_multiply(0.1));
            }
            dica.on_hover_ui(|ui| dica_sozinha(ui, etapa));
        }
        let acao = match grupo {
            Some(Grupo::Espera) => Some(("Responder", p.alerta)),
            Some(Grupo::Erro) => Some(("Ver o erro", p.erro)),
            _ if t.coluna == Coluna::Revisao && v.pode_mudar => Some(("Concluir", p.ok)),
            _ => None,
        };
        if let Some((texto, cor)) = acao {
            let galeria = ui.painter().layout_no_wrap(texto.into(), forte(12.0), cor);
            let direita = rect.right() - 14.0 - 10.0;
            let largura = galeria.size().x;
            if texto == "Concluir" {
                // Clicável à parte do cartão: leva para Concluído sem revisor.
                let zona = Rect::from_min_max(pos2(direita - largura - 10.0, meio - 11.0), pos2(direita + 10.0, meio + 11.0));
                let botao = ui.interact(zona, Id::new(("concluir-cartao", t.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                if botao.hovered() {
                    ui.painter().rect_filled(zona, CornerRadius::same(6), cor.gamma_multiply(0.14));
                }
                if botao.on_hover_text("Levar para Concluído (sem precisar de revisor)").clicked() {
                    concluir = true;
                }
            }
            let vao = if texto == "Concluir" { 7.0 } else { 4.0 };
            ui.painter().galley(pos2(direita - largura - vao, meio - galeria.size().y / 2.0), galeria, cor);
            let icone = if texto == "Concluir" { Icone::Check } else { Icone::Proximo };
            tema::desenhar_icone(ui.painter(), pos2(direita + 2.0, meio), icone, cor);
        }
    }

    // Só o botão esquerdo arrasta; o direito abre o menu do cartão. Sem o núcleo, nada se move.
    if v.pode_mudar && resposta.drag_started_by(egui::PointerButton::Primary) {
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

    let mut acao = if concluir { Some(Acao::Moveu(t.id, Coluna::Concluido)) } else { resposta.clicked().then_some(Acao::AbrirTarefa(t.id)) };
    let pode_mudar = v.pode_mudar;
    let passo = crate::tarefa::proximo_passo(t.coluna);
    let menu = |ui: &mut egui::Ui, acao: &mut Option<Acao>| {
        ui.set_min_width(200.0);
        if tema::opcao_menu(ui, "Abrir tarefa", false) {
            *acao = Some(Acao::AbrirTarefa(t.id));
            ui.close();
        }
        if let Some((rotulo, _, coluna)) = passo
            && tema::opcao_menu_com(ui, rotulo, None, pode_mudar)
        {
            *acao = Some(Acao::Moveu(t.id, coluna));
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
        let mais = tema::botao_icone_em(ui, botao, Id::new(("mais-cartao", t.id)), Icone::Mais);
        if mais.clicked() {
            acao = None;
        }
        egui::Popup::menu(&mais).id(Id::new(("menu-cartao", t.id))).show(|ui| menu(ui, &mut acao));
    }
    acao
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::dados::{AgenteTela, EstadoAgente};

    fn agente(id: i64, ferramenta: &str, papel: &str) -> AgenteTela {
        AgenteTela {
            id,
            ferramenta: ferramenta.into(),
            papel: papel.into(),
            ativo: false,
            estado: EstadoAgente::Trabalhando,
            motivo: String::new(),
            desde: String::new(),
            desde_em: String::new(),
            fim: None,
            erro_visto: false,
        }
    }

    fn tarefa(coluna: Coluna, agentes: Vec<AgenteTela>) -> Tarefa {
        let mut t = crate::dados::gerar_demo(1).remove(0);
        t.coluna = coluna;
        t.agentes = agentes;
        t.motivo = None;
        t
    }

    #[test]
    fn concluido_de_hoje_pela_data_do_fim_ou_pelo_que_voce_soltou() {
        let mut t = tarefa(Coluna::Concluido, vec![agente(1, "claude", "dev")]);
        let agora = HashSet::new();
        assert!(!de_hoje(&t, "2026-10-16", &agora));
        t.agentes[0].fim = Some(crate::api::Fim { motivo: "terminou".into(), data: "2026-10-15".into(), ..Default::default() });
        assert!(!de_hoje(&t, "2026-10-16", &agora));
        t.agentes[0].fim.as_mut().unwrap().data = "2026-10-16".into();
        assert!(de_hoje(&t, "2026-10-16", &agora));
        let sem_agente = tarefa(Coluna::Concluido, Vec::new());
        assert!(de_hoje(&sem_agente, "2026-10-16", &HashSet::from([sem_agente.id])));
    }

    #[test]
    fn anda_sozinha_segue_as_regras_do_nucleo() {
        assert_eq!(anda_sozinha(&tarefa(Coluna::Trabalhando, vec![agente(1, "claude", "dev")])), Some(0));
        // O terminal comum não anda a tarefa.
        assert_eq!(anda_sozinha(&tarefa(Coluna::Trabalhando, vec![agente(1, "shell", "dev")])), None);
        // Um Aguardando seu fica; o do núcleo volta sozinho.
        let mut t = tarefa(Coluna::AguardandoVoce, vec![agente(1, "claude", "dev")]);
        assert_eq!(anda_sozinha(&t), None);
        t.coluna_auto = true;
        assert_eq!(anda_sozinha(&t), Some(0));
        // De Revisão só sai com um revisor.
        assert_eq!(anda_sozinha(&tarefa(Coluna::Revisao, vec![agente(1, "claude", "dev")])), None);
        assert_eq!(anda_sozinha(&tarefa(Coluna::Revisao, vec![agente(1, "claude", "dev"), agente(2, "gemini", "revisor")])), Some(1));
        assert_eq!(anda_sozinha(&tarefa(Coluna::Backlog, vec![agente(1, "claude", "dev")])), None);
    }

    #[test]
    fn resumo_conta_erro_antes_da_espera() {
        let terminais = HashMap::new();
        let mut erro = tarefa(Coluna::AguardandoVoce, Vec::new());
        erro.erro = Some("saiu com código 1".into());
        let espera = tarefa(Coluna::AguardandoVoce, Vec::new());
        let parada = tarefa(Coluna::Backlog, Vec::new());
        let r = resumir([&erro, &espera, &parada].into_iter(), &terminais);
        assert_eq!(r, Resumo { espera: 1, trabalhando: 0, erro: 1 });
    }
}
