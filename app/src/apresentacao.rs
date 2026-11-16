//! Modo apresentação da daily e da sprint: uma capa com o resumo do período,
//! um slide por bloco (as tarefas de um projeto, ou de um assunto da sprint,
//! com as notas) e, para as tarefas com fotos, vídeos ou lousa, o slide
//! delas logo depois do bloco. Qualquer tarefa abre em detalhe de dentro do
//! bloco (clique ou Enter; Esc volta), onde a nota se edita, os anexos
//! entram e o agente recebe pedidos. Abre em tela cheia (F5) ou em janela
//! (clique num cartão), e o F11 alterna.
//!
//! Nada se redesenha por tempo: a troca de slide é seca, as imagens são
//! decodificadas fora da thread da tela no tamanho da região, e o envio de
//! um arquivo só repinta quando o progresso muda. Eventos que chegam durante
//! a apresentação não mexem nos slides: aparece "Novidades · R atualiza".

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, CornerRadius, Event, FontId, Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::api;
use crate::dados;
use crate::lousa;
use crate::pedido;
use crate::registro::{self, CacheImagens, Miniatura};
use crate::sistema;
use crate::tema::{self, Pilula, cores, forte};

/// Onde a apresentação começa.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Inicio {
    Capa,
    Tarefa(i64),
    /// O último slide visto nesta sessão (Shift+F5).
    Retomar,
}

/// O que a apresentação pede à tela principal.
pub enum Pedido {
    Sair,
    AbrirTarefa(i64),
}

/// Mudanças que a própria apresentação faz: o evento delas, quando chega,
/// não acende "Novidades" (as mesmas mudanças vindas de fora acendem).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Proprio {
    Nota,
    AnexoNovo,
    AnexoRemovido,
    ForaDaDaily,
}

/// As regiões do slide, calculadas só pelo tamanho da janela (dá para testar).
/// Os valores de referência são de 1600×900; o resto escala por `escala`.
#[derive(Clone, Copy, Debug)]
pub struct Regioes {
    pub escala: f32,
    pub cabecalho: Rect,
    pub titulo: Rect,
    pub conteudo: Rect,
    pub esquerda: Rect,
    pub direita: Rect,
    /// Seletor da coluna da direita ("Anexos | Lousa"): altura 0 quando há
    /// uma aba só (só anexos, ou só a lousa).
    pub seletor_direita: Rect,
    pub acoes: Rect,
    /// Faixa central do rodapé, entre a navegação e os botões: avisos, o
    /// envio em andamento e o "Desfazer". Nada dela cai sobre as colunas.
    pub avisos: Rect,
    /// O botão "Pedir ao agente (P)", só nos slides de tarefa.
    pub reserva_agente: Rect,
    pub fechar: Rect,
}

/// Largura da navegação do rodapé (anterior, próximo, até 24 pontos e
/// "? atalhos") e dos botões ("Abrir tarefa" e "Adicionar foto ou vídeo"):
/// o rodapé não escala com a tela, então as larguras são fixas.
const LARGURA_NAVEGACAO: f32 = 400.0;
const LARGURA_BOTOES: f32 = 330.0;
/// O "Pedir ao agente (P)" no rodapé.
const LARGURA_RESERVA_AGENTE: f32 = 176.0;

/// Fator de escala da apresentação: 1 em 1600×900.
pub fn escala(tamanho: Vec2) -> f32 {
    (tamanho.x / 1600.0).min(tamanho.y / 900.0).clamp(0.75, 1.4)
}

/// Calcula as regiões. `com_anexos` falso: a esquerda ocupa a largura toda.
/// `com_abas`: a coluna da direita tem anexos e lousa (o seletor aparece).
pub fn regioes(tamanho: Vec2, com_anexos: bool, com_abas: bool) -> Regioes {
    let s = escala(tamanho);
    let px = |v: f32| (v * s).round();
    let margem = px(64.0);
    let topo = px(40.0);
    let largura = tamanho.x - 2.0 * margem;
    let cabecalho = Rect::from_min_size(pos2(margem, topo), vec2(largura, px(32.0)));
    // Duas linhas de título: 48 de altura cada, nunca menos que a fonte mínima de 34 (42 por linha).
    let altura_titulo = px(96.0).max(84.0);
    let titulo = Rect::from_min_size(pos2(margem, cabecalho.bottom() + px(16.0)), vec2(largura, altura_titulo));
    let altura_acoes = px(64.0).max(48.0);
    let acoes = Rect::from_min_size(pos2(0.0, tamanho.y - altura_acoes), vec2(tamanho.x, altura_acoes));
    let conteudo = Rect::from_min_max(pos2(margem, titulo.bottom() + px(28.0)), pos2(margem + largura, acoes.top() - px(24.0)));
    let vao = px(48.0);
    let (esquerda, direita) = if com_anexos {
        let esquerda_largura = ((largura - vao) * 0.584).round();
        let esquerda = Rect::from_min_size(conteudo.min, vec2(esquerda_largura, conteudo.height()));
        let direita = Rect::from_min_max(pos2(esquerda.right() + vao, conteudo.top()), conteudo.max);
        (esquerda, direita)
    } else {
        (conteudo, Rect::from_min_size(conteudo.right_top(), vec2(0.0, conteudo.height())))
    };
    // O seletor é de 34 px fixos, como o rodapé (não escala); a coluna começa 12 abaixo.
    let seletor_direita = Rect::from_min_size(direita.min, vec2(direita.width(), if com_abas && com_anexos { 34.0 } else { 0.0 }));
    let lado = px(32.0).max(28.0);
    let fechar = Rect::from_min_size(pos2(tamanho.x - margem - lado, acoes.center().y - lado / 2.0), vec2(lado, lado));
    // Largura fixa, como o resto do rodapé: o botão "Pedir ao agente (P)" não escala.
    let reserva_agente = Rect::from_min_size(pos2(fechar.left() - 12.0 - LARGURA_RESERVA_AGENTE, fechar.top()), vec2(LARGURA_RESERVA_AGENTE, lado));
    let avisos = Rect::from_min_max(
        pos2(margem + LARGURA_NAVEGACAO + 16.0, acoes.top() + 4.0),
        pos2((reserva_agente.left() - 12.0 - LARGURA_BOTOES - 16.0).max(margem + LARGURA_NAVEGACAO + 96.0), acoes.bottom() - 4.0),
    );
    Regioes { escala: s, cabecalho, titulo, conteudo, esquerda, direita, seletor_direita, acoes, avisos, reserva_agente, fechar }
}

/// Uma página da apresentação.
#[derive(Clone, Debug, PartialEq)]
enum Pagina {
    Capa,
    /// O slide de uma tarefa: só quem tem anexos ou lousa ganha o seu, logo
    /// depois do bloco; as outras abrem pelo detalhe (clique ou Enter).
    Tarefa(usize),
    /// Um bloco: as tarefas de um projeto (ou de um assunto da sprint), com
    /// as notas, numa tela só. Um bloco que não cabe segue em partes.
    Bloco {
        secao_id: i64,
        projeto: String,
        workspace: String,
        /// Os índices dos slides do deck nesta parte.
        tarefas: Vec<usize>,
        parte: usize,
        partes: usize,
    },
    Mais,
    Fim,
}

/// Monta a sequência de páginas do deck (sem os slides escondidos com H):
/// a capa, um bloco por seção (dividido em partes pela altura que `medir`
/// dá a cada tarefa, sem cortar tarefa ao meio), o slide das tarefas com
/// anexos ou lousa logo depois do bloco delas, "mais" e o fim. Com
/// `altura` infinita, cada seção é um bloco só (os testes e a primeira
/// montagem, antes de a tela medir).
fn montar_paginas(deck: &api::Deck, escondidos: &HashSet<i64>, medir: &dyn Fn(&api::Slide) -> f32, altura: f32) -> Vec<Pagina> {
    let mut paginas = vec![Pagina::Capa];
    let secao = |s: &api::Slide| (s.secao_id, s.secao.clone());
    let mut grupos: Vec<Vec<usize>> = Vec::new();
    for (i, s) in deck.slides.iter().enumerate() {
        if escondidos.contains(&s.tarefa_id) {
            continue;
        }
        match grupos.last_mut() {
            Some(lista) if secao(&deck.slides[lista[0]]) == secao(s) => lista.push(i),
            _ => grupos.push(vec![i]),
        }
    }
    for lista in grupos {
        let s0 = &deck.slides[lista[0]];
        let projeto = if s0.projeto.is_empty() || s0.secao_id < 0 { s0.secao.clone() } else { s0.projeto.clone() };
        let mut partes: Vec<Vec<usize>> = Vec::new();
        let mut atual: Vec<usize> = Vec::new();
        let mut soma = 0.0;
        for i in lista {
            let h = medir(&deck.slides[i]);
            if !atual.is_empty() && soma + h > altura {
                partes.push(std::mem::take(&mut atual));
                soma = 0.0;
            }
            soma += h;
            atual.push(i);
        }
        if !atual.is_empty() {
            partes.push(atual);
        }
        let n = partes.len();
        for (k, tarefas) in partes.into_iter().enumerate() {
            let com_slide: Vec<usize> = tarefas.iter().copied().filter(|i| !deck.slides[*i].anexos.is_empty() || deck.slides[*i].lousa.is_some()).collect();
            paginas.push(Pagina::Bloco { secao_id: s0.secao_id, projeto: projeto.clone(), workspace: s0.workspace.clone(), tarefas, parte: k + 1, partes: n });
            paginas.extend(com_slide.into_iter().map(Pagina::Tarefa));
        }
    }
    if deck.mais > 0 {
        paginas.push(Pagina::Mais);
    }
    paginas.push(Pagina::Fim);
    paginas
}

/// A página em que a tarefa aparece: o slide dela, senão o bloco que a contém.
fn pagina_da_tarefa(paginas: &[Pagina], deck: &api::Deck, tarefa: i64) -> Option<usize> {
    let indice = deck.slides.iter().position(|s| s.tarefa_id == tarefa)?;
    paginas
        .iter()
        .position(|p| matches!(p, Pagina::Tarefa(i) if *i == indice))
        .or_else(|| paginas.iter().position(|p| matches!(p, Pagina::Bloco { tarefas, .. } if tarefas.contains(&indice))))
}

/// Para onde ir ao avançar ou voltar, dentro dos limites.
fn navegar(atual: usize, total: usize, passo: isize) -> usize {
    (atual as isize + passo).clamp(0, total.saturating_sub(1) as isize) as usize
}

#[derive(Clone, PartialEq)]
enum EstadoNota {
    Salvando,
    Salvo,
    Erro(String),
}

/// Arquivo na fila de envio.
enum Envio {
    Arquivo(PathBuf),
    Png(Vec<u8>),
}

struct Progresso {
    nome: String,
    arquivo: usize,
    arquivos: usize,
    enviados: u64,
    total: u64,
}

/// Anexo tirado do slide, à espera do "Desfazer" antes de sair de verdade.
struct Remocao {
    tarefa: i64,
    anexo: api::AnexoSlide,
    indice: usize,
    ate: f64,
}

/// Imagem aberta em tamanho grande: as setas andam pelas imagens do slide.
struct Visor {
    tarefa: i64,
    anexo: i64,
    imagem: Option<Result<egui::TextureHandle, String>>,
}

enum Mensagem {
    Deck {
        resultado: Result<Box<api::Deck>, String>,
        manter: Option<i64>,
    },
    NotaSalva {
        tarefa: i64,
        texto: String,
        resultado: Result<api::NotaGravada, String>,
    },
    Progresso(Progresso),
    Enviado {
        tarefa: i64,
        nome: String,
        resultado: Result<api::AnexoSlide, String>,
    },
    FimDosEnvios,
    VideoFalhou(String),
    Grande {
        anexo: i64,
        resultado: Result<egui::ColorImage, String>,
    },
    RemocaoFalhou(i64),
    ForaDaDailyFalhou {
        tarefa: i64,
        erro: String,
    },
    Lousa {
        tarefa: i64,
        resultado: Result<api::LousaAberta, String>,
    },
    /// A lousa da reunião (da daily ou da sprint), para o palco.
    LousaReuniao(Result<api::LousaAberta, String>),
}

/// A lousa de uma tarefa no slide: buscada sob demanda, em cache por tarefa.
enum LousaDoSlide {
    Carregando,
    Pronta(Vec<api::ElementoLousa>),
    Erro(String),
}

const ALTURA_IMAGEM: u32 = 1080;
const MAX_IMAGENS: usize = 24;
const MAX_TOPICOS: usize = 6;
const MAX_NOTA: usize = 4000;
/// Abaixo da nota: 6 de vão e a linha do estado da gravação ("salvo").
const RODAPE_NOTA: f32 = 6.0 + 16.0;
/// O tom de um aviso da faixa: a cor do ponto e do fundo. Laranja só para o
/// que deu errado; na tela compartilhada, boa notícia não parece problema.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Tom {
    Neutro,
    Concluiu,
    Alerta,
}

/// A nota é o roteiro da fala: o maior texto do slide. No bloco, 22 por
/// escala; no slide da tarefa, que tem a coluna só para ela, 26.
const FONTE_NOTA: f32 = 22.0;
const FONTE_NOTA_TAREFA: f32 = 26.0;
/// O vão entre duas tarefas no slide do bloco.
const VAO_TAREFA_BLOCO: f32 = 26.0;
/// O respiro de cada linha da tabela do bloco (metade acima, metade abaixo do fio).
const VAO_LINHA_BLOCO: f32 = 44.0;

/// "O que foi feito" recolhido: só a linha que abre a lista.
const ALTURA_FEITO_RECOLHIDO: f32 = 30.0;

/// O estilo da nota em itens no slide: a fonte dada (já na escala), linhas
/// a 1,35 da fonte, a coluna do marcador e os vãos na mesma proporção.
fn estilo_nota(fonte: f32, cor: Color32, largura: f32) -> registro::EstiloNota {
    let p = cores();
    registro::EstiloNota {
        fonte,
        cor,
        marcador: p.suave,
        largura: largura.max(40.0),
        altura_linha: (fonte * 1.35).round(),
        recuo: (fonte * 1.36).round(),
        vao_item: (fonte * 0.27).round(),
        vao_bloco: (fonte * 0.55).round(),
    }
}

/// A nota que o slide lê: a de hoje, ou a de ontem (na sprint, a da sprint
/// anterior) para quem parou. `None` sem nota nenhuma.
fn nota_do_slide(slide: &api::Slide) -> Option<(&str, bool)> {
    if !slide.nota.trim().is_empty() {
        return Some((slide.nota.trim(), false));
    }
    slide.nota_anterior.as_ref().filter(|a| !a.texto.trim().is_empty()).map(|a| (a.texto.trim(), true))
}

/// Uma tarefa medida para o slide do bloco, do jeito que a pessoa monta o
/// slide dela: o tópico à esquerda, em negrito, e a frase à direita, com um
/// fio entre as linhas. Sem montagem, o título e a nota em itens, como antes.
struct TarefaNoBloco {
    titulo: std::sync::Arc<egui::Galley>,
    selos: Vec<(String, Color32)>,
    /// A frase da montagem (a coluna da direita).
    frase: Option<std::sync::Arc<egui::Galley>>,
    nota: Option<registro::NotaMedida>,
    estado: Option<std::sync::Arc<egui::Galley>>,
    altura: f32,
}

/// A linha cinza de quem não tem nota (a mesma da página).
fn estado_sem_nota(s: &api::Slide) -> &'static str {
    if s.removida {
        return "A tarefa foi removida.";
    }
    match s.grupo.as_str() {
        "trabalhando" => "Segue em andamento.",
        "aguardando" => "Espera minha resposta.",
        "erros" => "Parou com erro.",
        "revisao" => "Em revisão, ainda sem nota.",
        "concluidas" => "Concluída, ainda sem nota.",
        _ => "Ainda sem nota.",
    }
}

/// A largura da coluna do tópico no bloco (a frase fica com o resto).
fn coluna_do_topico(largura: f32, s: f32) -> f32 {
    (largura * 0.37).min((430.0 * s).round()).max(160.0)
}

/// Os selos de uma tarefa: o dia (quando não é hoje) e o estado que pede atenção.
fn selos_do_slide(slide: &api::Slide) -> Vec<(String, Color32)> {
    let p = cores();
    let mut selos = Vec::new();
    if !slide.partes.iter().any(|p| p == "Hoje")
        && let Some(parte) = slide.partes.iter().find(|p| *p != "No período")
    {
        selos.push((parte.clone(), p.suave));
    }
    match slide.grupo.as_str() {
        "aguardando" if !slide.removida => selos.push(("esperando você".into(), p.alerta)),
        "erros" if !slide.removida => selos.push(("parou com erro".into(), p.erro)),
        _ => {}
    }
    selos
}

/// Mede uma tarefa como ela aparece no bloco (`s` é a escala, `largura` a
/// da coluna). O slide pinta exatamente o que foi medido.
fn medir_tarefa_no_bloco(pintor: &egui::Painter, slide: &api::Slide, s: f32, largura: f32) -> TarefaNoBloco {
    let p = cores();
    let px = |v: f32| (v * s).round();
    let selos = selos_do_slide(slide);
    let fonte_selo = forte(px(14.0));
    if let Some(m) = slide.montagem.as_ref().filter(|m| !m.frase.trim().is_empty()) {
        let coluna = coluna_do_topico(largura, s);
        let topico = m.topico.trim().trim_end_matches('.');
        let topico = if topico.is_empty() { slide.titulo.trim_end_matches('.') } else { topico };
        let titulo = pintor.layout(topico.to_string(), forte(px(27.0)), p.texto, coluna - px(24.0));
        let frase = pintor.layout(m.frase.trim().to_string(), FontId::proportional(px(25.0)), p.texto, largura - coluna - px(8.0));
        let altura_selos = if selos.is_empty() { 0.0 } else { px(24.0) };
        let altura = (titulo.size().y + altura_selos).max(frase.size().y) + px(VAO_LINHA_BLOCO);
        return TarefaNoBloco { titulo, selos, frase: Some(frase), nota: None, estado: None, altura };
    }
    let largura_selos: f32 = selos.iter().map(|(t, _)| pintor.layout_no_wrap(t.clone(), fonte_selo.clone(), p.suave).size().x + 12.0 + px(8.0)).sum();
    let recuo = px(24.0);
    let largura_texto = (largura - recuo - largura_selos - if selos.is_empty() { 0.0 } else { px(8.0) }).max(120.0);
    let titulo = pintor.layout(slide.titulo.trim_end_matches('.').to_string(), forte(px(26.0)), p.texto, largura_texto);
    let mut altura = titulo.size().y + px(10.0);
    let anterior = !matches!(slide.grupo.as_str(), "trabalhando" | "aguardando" | "erros");
    let (nota, estado) = match nota_do_slide(slide) {
        Some((texto, de_ontem)) if !de_ontem || anterior => {
            let n = registro::NotaMedida::nova(pintor, texto, estilo_nota(px(FONTE_NOTA), p.texto, largura - recuo));
            altura += n.altura;
            (Some(n), None)
        }
        _ => {
            let g = pintor.layout_no_wrap(estado_sem_nota(slide).to_string(), FontId::proportional(px(20.0)), p.suave);
            altura += g.size().y;
            (None, Some(g))
        }
    };
    altura += px(VAO_TAREFA_BLOCO);
    TarefaNoBloco { titulo, selos, frase: None, nota, estado, altura }
}

pub struct Apresentacao {
    perfil: i64,
    /// O escopo em que ela abriu: o R atualiza nele, mesmo que a barra
    /// lateral mude depois.
    recorte: api::Recorte,
    /// A opção "Mostrar tempo dos agentes" do perfil (a tela principal põe a
    /// cada quadro, por `definir_tempo`).
    mostrar_tempo: bool,
    /// A opção foi desligada com um deck que tinha o tempo: o deck saiu da
    /// memória e um novo, sem o tempo, está a caminho (a tela diz "Atualizando…").
    tirando_tempo: bool,
    /// None é a daily.
    periodo: Option<api::PeriodoSprint>,
    deck: Option<api::Deck>,
    erro: Option<String>,
    paginas: Vec<Pagina>,
    atual: usize,
    /// As páginas foram montadas sem medir (deck novo): a próxima pintura
    /// divide os blocos pela altura da tela. Guarda o tamanho usado.
    repaginar: bool,
    tamanho_paginado: Vec2,
    /// O detalhe de uma tarefa aberto de dentro do bloco (clique ou Enter):
    /// o índice do slide no deck. Esc volta ao bloco.
    detalhe: Option<usize>,
    /// A tarefa sob o mouse no slide do bloco (o H esconde essa).
    sob_mouse: Option<i64>,
    inicio: Inicio,
    escondidos: HashSet<i64>,
    pub tela_cheia: bool,
    /// Houve evento no perfil desde que o deck foi montado.
    pub novidades: bool,
    aviso: Option<(String, f64, Tom)>,
    // Notas
    nota: Option<(i64, String)>,
    /// O último texto que o núcleo não salvou (para o Esc largar a edição).
    nota_recusada: Option<(i64, String)>,
    /// A nota como estava quando a edição começou (texto e versão): ao
    /// salvar, o núcleo confere que ninguém mexeu nela desde então.
    base_nota: Option<(i64, String, String)>,
    /// O agente mexeu na nota durante a edição (o rodapé avisa).
    nota_agente: HashSet<i64>,
    /// A nota mudou de um jeito que não dá para juntar sozinho.
    conflito: Option<Conflito>,
    focar_nota: bool,
    estado_nota: HashMap<i64, EstadoNota>,
    /// "O que foi feito" aberto (começa recolhido; vale para todos os slides).
    feito_aberto: bool,
    // Anexos
    principal: HashMap<i64, usize>,
    imagens: CacheImagens,
    miniaturas: CacheImagens,
    fila: Vec<(i64, Envio, String)>,
    progresso: Option<Progresso>,
    enviando: bool,
    remocao: Option<Remocao>,
    abrindo_video: Option<f64>,
    erro_video: Option<String>,
    visor: Option<Visor>,
    atalhos: bool,
    /// Quantos eventos de cada mudança própria ainda vão chegar, por tarefa.
    esperados: HashMap<(Proprio, i64), u32>,
    /// Tema só desta apresentação (T), sem gravar no perfil.
    tema_base: tema::Escolha,
    tema_sessao: Option<tema::Escolha>,
    canal: (Sender<Mensagem>, Receiver<Mensagem>),
    // Pedir ao agente
    /// O pedido de cada tarefa (a tela principal põe a cada quadro).
    pub pedidos: pedido::Pedidos,
    caixa: Option<pedido::Caixa>,
    rascunhos: HashMap<i64, String>,
    /// O agente respondeu num slide que não é o atual: a faixa oferece "Ver".
    respondido: Option<(i64, String)>,
    /// Um pedido do deck em andamento e outro pedido depois dele (mudanças
    /// do agente chegam em rajada: uma busca por vez).
    buscando: bool,
    buscar_de_novo: bool,
    // Lousa da tarefa no slide
    /// A aba escolhida fica para os próximos slides que têm lousa (L troca).
    aba_lousa: bool,
    lousas: HashMap<i64, LousaDoSlide>,
    desenho_lousa: lousa::desenho::Desenho,
    /// O palco da lousa do slide, por cima da apresentação (Esc volta ao slide).
    palco: Option<Box<lousa::palco::Palco>>,
    /// A lousa da reunião está sendo buscada para o palco (Q).
    pedindo_lousa_reuniao: bool,
}

/// A nota mudou enquanto você editava e não dá para juntar sozinho (o agente
/// reescreveu): a janela mostra as duas versões.
struct Conflito {
    tarefa: i64,
    minha: String,
    do_agente: String,
    versao: String,
}

/// O botão da faixa do rodapé.
#[derive(Clone, Copy, PartialEq, Debug)]
enum AcaoFaixa {
    Desfazer,
    Ver(i64),
    AbrirTarefa(i64),
    Cancelar(i64),
}

impl AcaoFaixa {
    fn rotulo(self) -> &'static str {
        match self {
            AcaoFaixa::Desfazer => "Desfazer",
            AcaoFaixa::Ver(_) => "Ver",
            // "Abrir tarefa" já está no rodapé: a faixa diz para quê.
            AcaoFaixa::AbrirTarefa(_) => "Ver no terminal",
            AcaoFaixa::Cancelar(_) => "Cancelar",
        }
    }
}

/// O texto da nota depois de juntar a sua edição com o que o agente
/// acrescentou no fim da versão que você leu. None: o agente mudou o que já
/// estava (a janela de conflito decide).
pub fn juntar_nota(base: &str, minha: &str, atual: &str) -> Option<String> {
    if base.is_empty() {
        return Some(if minha.trim().is_empty() { atual.to_string() } else { format!("{}\n\n{}", minha.trim_end(), atual.trim_start()) });
    }
    let acrescentado = atual.strip_prefix(base)?;
    Some(format!("{}{}", minha.trim_end(), acrescentado))
}

/// Quantas células a capa usa para N partes: até 6 (3 por fileira); com
/// mais, a sexta diz quais projetos ficaram de fora.
fn celulas_capa(partes: usize) -> usize {
    partes.min(6)
}

/// O rótulo da coluna da capa: "estudos › cliente-x · 1" (o núcleo separa o
/// workspace com " · "; na tela o caminho usa "›" e o "·" fica só antes da contagem).
fn rotulo_parte(titulo: &str, n: usize) -> String {
    format!("{} · {n}", titulo.replacen(" · ", " › ", 1))
}

impl Apresentacao {
    #[allow(clippy::too_many_arguments)]
    pub fn nova(
        ctx: &egui::Context,
        perfil: i64,
        recorte: api::Recorte,
        periodo: Option<api::PeriodoSprint>,
        deck: Option<api::Deck>,
        inicio: Inicio,
        tela_cheia: bool,
        tema_base: tema::Escolha,
        mostrar_tempo: bool,
    ) -> Self {
        let mut a = Apresentacao {
            perfil,
            recorte,
            mostrar_tempo,
            tirando_tempo: false,
            periodo,
            deck: None,
            erro: None,
            paginas: vec![Pagina::Capa],
            atual: 0,
            repaginar: false,
            tamanho_paginado: Vec2::ZERO,
            detalhe: None,
            sob_mouse: None,
            inicio,
            escondidos: HashSet::new(),
            tela_cheia,
            novidades: false,
            aviso: None,
            nota: None,
            nota_recusada: None,
            base_nota: None,
            nota_agente: HashSet::new(),
            conflito: None,
            focar_nota: false,
            estado_nota: HashMap::new(),
            feito_aberto: false,
            principal: HashMap::new(),
            imagens: CacheImagens::new(Some(ALTURA_IMAGEM), MAX_IMAGENS, "slide"),
            miniaturas: CacheImagens::new(Some(128), 64, "slide-miniatura"),
            fila: Vec::new(),
            progresso: None,
            enviando: false,
            remocao: None,
            abrindo_video: None,
            erro_video: None,
            visor: None,
            atalhos: false,
            esperados: HashMap::new(),
            tema_base,
            tema_sessao: None,
            canal: mpsc::channel(),
            pedidos: pedido::Pedidos::new(),
            caixa: None,
            rascunhos: HashMap::new(),
            respondido: None,
            buscando: false,
            buscar_de_novo: false,
            aba_lousa: false,
            lousas: HashMap::new(),
            desenho_lousa: lousa::desenho::Desenho::novo("slide-lousa"),
            palco: None,
            pedindo_lousa_reuniao: false,
        };
        match deck {
            // Um deck com o tempo e a opção desligada nunca entra na tela.
            Some(d) if !(d.tempo_agentes && !mostrar_tempo) => a.trocar_deck(d, None),
            _ => a.pedir_deck(ctx, None),
        }
        // A apresentação é em papel (o tema Leitura): fundo claro projeta
        // melhor e é o que a pessoa leva para a reunião. T troca para o escuro.
        if tema_base != tema::Escolha::Leitura {
            tema::Escolha::Leitura.aplicar(ctx);
            a.tema_sessao = Some(tema::Escolha::Leitura);
        }
        a
    }

    pub fn daily(&self) -> bool {
        self.periodo.is_none()
    }

    /// A opção do perfil. Desligada com um deck que tinha o tempo, as frases
    /// dele (que vêm prontas do núcleo) saem da tela na hora: o deck sai da
    /// memória e um novo é pedido no mesmo escopo, mantendo o slide. Ligada,
    /// busca de novo para trazer o tempo.
    pub fn definir_tempo(&mut self, ctx: &egui::Context, mostrar: bool) {
        if mostrar == self.mostrar_tempo {
            return;
        }
        self.mostrar_tempo = mostrar;
        if self.deck.is_none() && !self.buscando {
            return;
        }
        let atual = self.tarefa_atual();
        if !mostrar && self.deck.as_ref().is_some_and(|d| d.tempo_agentes) {
            self.fechar_caixa();
            self.salvar_nota(ctx);
            self.deck = None;
            self.tirando_tempo = true;
            self.inicio = match atual {
                Some(t) => Inicio::Tarefa(t),
                None => Inicio::Capa,
            };
        }
        if self.buscando {
            // A resposta que está a caminho pode ter o tempo: ela é descartada
            // ou trocada por uma busca nova ao chegar.
            self.buscar_de_novo = true;
        } else {
            self.pedir_deck(ctx, atual);
        }
        ctx.request_repaint();
    }

    /// Um evento de nota ou anexo chegou: diz se foi a própria apresentação
    /// que causou (e dá baixa nele).
    pub fn evento_proprio(&mut self, tipo: Proprio, tarefa: i64) -> bool {
        match self.esperados.get_mut(&(tipo, tarefa)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
    }

    fn esperar(&mut self, tipo: Proprio, tarefa: i64) {
        *self.esperados.entry((tipo, tarefa)).or_default() += 1;
    }

    /// A mudança falhou: o evento dela não vem.
    fn nao_esperar(&mut self, tipo: Proprio, tarefa: i64) {
        self.evento_proprio(tipo, tarefa);
    }

    /// A tarefa do slide atual (para voltar à página rolada até o cartão dela).
    pub fn tarefa_atual(&self) -> Option<i64> {
        self.slide().map(|s| s.tarefa_id)
    }

    /// O agente respondeu um pedido: o slide traz a nota e as capturas novas
    /// sem você sair dali. Em outro slide, a faixa oferece "Ver".
    pub fn pedido_respondido(&mut self, ctx: &egui::Context, tarefa: i64, titulo: &str) {
        let atual = self.tarefa_atual();
        if atual == Some(tarefa) {
            self.aviso = Some(("O agente respondeu o pedido".into(), 0.0, Tom::Concluiu));
        } else {
            self.respondido = Some((tarefa, titulo.to_string()));
        }
        self.atualizar_pelo_agente(ctx);
    }

    /// Busca o deck de novo mantendo o slide atual: uma busca por vez, e
    /// mais uma no fim se algo mudou enquanto isso.
    pub fn atualizar_pelo_agente(&mut self, ctx: &egui::Context) {
        if self.buscando {
            self.buscar_de_novo = true;
            return;
        }
        let atual = self.tarefa_atual();
        self.pedir_deck(ctx, atual);
    }

    /// A lousa da tarefa mudou: o slide busca de novo quando aparecer.
    /// Q: o palco da lousa da reunião (a da daily de hoje ou a da sprint
    /// fixa), cartão por cartão, na ordem das setas.
    fn abrir_lousa_da_reuniao(&mut self, ctx: &egui::Context) {
        let Some(deck) = &self.deck else { return };
        if self.pedindo_lousa_reuniao {
            return;
        }
        let dono = if self.daily() {
            api::DonoLousa::daily(self.perfil, &deck.chave_nota)
        } else if deck.sprint_id != 0 {
            api::DonoLousa::sprint(deck.sprint_id)
        } else {
            self.aviso = Some(("A lousa da reunião é da sprint fixa; escolha a sprint na página".into(), 0.0, Tom::Neutro));
            return;
        };
        self.pedindo_lousa_reuniao = true;
        let envio = self.canal.0.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = envio.send(Mensagem::LousaReuniao(api::abrir_lousa(dono)));
            ctx.request_repaint();
        });
    }

    pub fn lousa_mudou(&mut self, tarefa: i64) {
        self.lousas.remove(&tarefa);
    }

    /// Pede a lousa da tarefa (uma vez; o cache vale até ela mudar).
    fn pedir_lousa(&mut self, ctx: &egui::Context, tarefa: i64, id: i64) {
        if self.lousas.contains_key(&tarefa) {
            return;
        }
        self.lousas.insert(tarefa, LousaDoSlide::Carregando);
        registro::em_segundo_plano(&self.canal.0, ctx, move || Mensagem::Lousa { tarefa, resultado: api::ler_lousa(id) });
    }

    /// O slide mostra a lousa na coluna da direita (a aba dela, ou só ela).
    fn mostra_lousa(&self, slide: &api::Slide) -> bool {
        slide.lousa.is_some() && (slide.anexos.is_empty() || self.aba_lousa)
    }

    /// O agente mexeu na nota da tarefa: se você está editando, o rodapé avisa
    /// que vai junto quando salvar.
    pub fn nota_do_agente(&mut self, tarefa: i64) {
        if self.nota.as_ref().is_some_and(|(t, _)| *t == tarefa) {
            self.nota_agente.insert(tarefa);
        }
    }

    /// Começa a editar a nota do slide, guardando o texto e a versão lidos.
    fn comecar_nota(&mut self, slide: &api::Slide) {
        self.nota = Some((slide.tarefa_id, slide.nota.clone()));
        self.base_nota = Some((slide.tarefa_id, slide.nota.clone(), slide.nota_versao.clone()));
        self.nota_agente.remove(&slide.tarefa_id);
    }

    fn pedir_deck(&mut self, ctx: &egui::Context, manter: Option<i64>) {
        let (perfil, projeto, periodo) = (self.perfil, self.recorte, self.periodo.clone());
        self.buscando = true;
        // Nos testes nada fala com um núcleo: a resposta é posta no canal à mão.
        if cfg!(test) {
            return;
        }
        registro::em_segundo_plano(&self.canal.0, ctx, move || Mensagem::Deck {
            resultado: api::apresentacao(perfil, projeto, periodo.as_ref()).map(Box::new),
            manter,
        });
    }

    /// Troca o deck mantendo o slide pela tarefa (R), ou indo ao início pedido.
    fn trocar_deck(&mut self, deck: api::Deck, manter: Option<i64>) {
        let antes = self.atual;
        self.paginas = montar_paginas(&deck, &self.escondidos, &|_| 0.0, f32::INFINITY);
        self.repaginar = true;
        let alvo = match (manter, self.inicio) {
            (Some(t), _) | (None, Inicio::Tarefa(t)) => Some(t),
            _ => None,
        };
        // O detalhe segue a mesma tarefa no deck novo (ou fecha, se ela saiu).
        self.detalhe = self
            .detalhe
            .and_then(|i| self.deck.as_ref().and_then(|d| d.slides.get(i)).map(|s| s.tarefa_id))
            .and_then(|t| deck.slides.iter().position(|s| s.tarefa_id == t));
        self.atual = 0;
        if let Some(t) = alvo {
            match pagina_da_tarefa(&self.paginas, &deck, t) {
                Some(i) => {
                    self.atual = i;
                    // A tarefa que você via continua na tela: no bloco, em detalhe.
                    if matches!(self.paginas[i], Pagina::Bloco { .. }) {
                        self.detalhe = deck.slides.iter().position(|s| s.tarefa_id == t);
                    }
                }
                None if manter.is_some() => {
                    // A tarefa saiu do deck: fica no vizinho e avisa.
                    let titulo = self.deck.as_ref().and_then(|d| d.slides.iter().find(|s| s.tarefa_id == t)).map(|s| s.titulo.clone()).unwrap_or_default();
                    self.atual = antes.min(self.paginas.len() - 1);
                    let quem = if self.daily() { "daily" } else { "sprint" };
                    self.aviso = Some((format!("“{titulo}” saiu da {quem}"), 0.0, Tom::Neutro));
                }
                None => {}
            }
        }
        self.inicio = Inicio::Capa;
        self.deck = Some(deck);
        self.novidades = false;
    }

    /// O slide de tarefa na tela: o detalhe aberto, ou a página de tarefa.
    fn slide(&self) -> Option<&api::Slide> {
        let deck = self.deck.as_ref()?;
        if let Some(i) = self.detalhe {
            return deck.slides.get(i);
        }
        match self.paginas.get(self.atual)? {
            Pagina::Tarefa(i) => deck.slides.get(*i),
            _ => None,
        }
    }

    /// Divide os blocos pela altura da tela, mantendo a página pelo conteúdo.
    fn repaginar(&mut self, pintor: &egui::Painter, tamanho: Vec2) {
        let Some(deck) = &self.deck else { return };
        let r = regioes(tamanho, false, false);
        let s = r.escala;
        let largura = r.conteudo.width().min((1100.0 * s).round());
        let alvo = match self.paginas.get(self.atual) {
            Some(Pagina::Tarefa(i)) => Some(deck.slides[*i].tarefa_id),
            Some(Pagina::Bloco { tarefas, .. }) => tarefas.first().map(|i| deck.slides[*i].tarefa_id),
            _ => None,
        };
        let no_fim = matches!(self.paginas.get(self.atual), Some(Pagina::Fim));
        let no_mais = matches!(self.paginas.get(self.atual), Some(Pagina::Mais));
        let medir = |slide: &api::Slide| medir_tarefa_no_bloco(pintor, slide, s, largura).altura;
        self.paginas = montar_paginas(deck, &self.escondidos, &medir, r.conteudo.height());
        self.atual = if no_fim {
            self.paginas.len() - 1
        } else if no_mais {
            self.paginas.iter().position(|p| *p == Pagina::Mais).unwrap_or(0)
        } else {
            alvo.and_then(|t| pagina_da_tarefa(&self.paginas, deck, t)).unwrap_or(0)
        };
        self.repaginar = false;
        self.tamanho_paginado = tamanho;
    }

    fn ir(&mut self, pagina: usize, ctx: &egui::Context) {
        if self.detalhe.is_some() {
            self.fechar_caixa();
            self.descartar_recusada();
            self.salvar_nota(ctx);
            self.detalhe = None;
        }
        if pagina != self.atual {
            self.fechar_caixa();
            self.descartar_recusada();
            self.salvar_nota(ctx);
            self.atual = pagina.min(self.paginas.len().saturating_sub(1));
            self.erro_video = None;
            self.abrindo_video = None;
        }
    }

    /// Vai ao slide da tarefa, ou ao bloco dela com o detalhe aberto.
    fn ir_para_tarefa(&mut self, tarefa: i64, ctx: &egui::Context) {
        let Some(deck) = &self.deck else { return };
        let Some(i) = pagina_da_tarefa(&self.paginas, deck, tarefa) else { return };
        let indice = deck.slides.iter().position(|s| s.tarefa_id == tarefa);
        self.ir(i, ctx);
        if matches!(self.paginas.get(i), Some(Pagina::Bloco { .. })) {
            self.detalhe = indice;
        }
    }

    /// Abre o detalhe da tarefa de dentro do bloco (clique ou Enter).
    fn abrir_detalhe(&mut self, indice: usize) {
        self.detalhe = Some(indice);
        self.sob_mouse = None;
    }

    /// Fecha o detalhe e volta ao bloco (Esc).
    fn fechar_detalhe(&mut self, ctx: &egui::Context) {
        self.fechar_caixa();
        self.descartar_recusada();
        self.salvar_nota(ctx);
        self.detalhe = None;
    }

    /// Grava a nota em edição, se mudou. Chamado ao perder o foco, ao trocar
    /// de slide e ao sair: nunca por tempo.
    fn salvar_nota(&mut self, ctx: &egui::Context) {
        let Some((tarefa, texto)) = self.nota.take() else { return };
        let Some(deck) = &self.deck else { return };
        let Some(slide) = deck.slides.iter().find(|s| s.tarefa_id == tarefa) else { return };
        if slide.nota == texto {
            return;
        }
        let (tipo, periodo) = (deck.tipo.clone(), deck.chave_nota.clone());
        // A versão lida quando a edição começou (o deck pode ter sido atualizado depois).
        let versao = match &self.base_nota {
            Some((t, _, v)) if *t == tarefa => v.clone(),
            _ => slide.nota_versao.clone(),
        };
        self.gravar_nota(ctx, tarefa, tipo, periodo, texto, versao);
    }

    fn gravar_nota(&mut self, ctx: &egui::Context, tarefa: i64, tipo: String, periodo: String, texto: String, versao: String) {
        self.estado_nota.insert(tarefa, EstadoNota::Salvando);
        self.esperar(Proprio::Nota, tarefa);
        registro::em_segundo_plano(&self.canal.0, ctx, move || {
            let resultado = api::gravar_nota(tarefa, &tipo, &periodo, &texto, &versao);
            Mensagem::NotaSalva { tarefa, texto, resultado }
        });
    }

    /// A nota mudou desde que a edição começou (o agente complementou): junta
    /// sozinha quando o agente só acrescentou; senão abre a janela de conflito.
    fn nota_mudou(&mut self, ctx: &egui::Context, tarefa: i64, minha: String, atual: String, versao: String) {
        let base = match &self.base_nota {
            Some((t, b, _)) if *t == tarefa => b.clone(),
            _ => String::new(),
        };
        self.nota_agente.remove(&tarefa);
        let Some(deck) = &self.deck else { return };
        let (tipo, periodo) = (deck.tipo.clone(), deck.chave_nota.clone());
        match juntar_nota(&base, &minha, &atual) {
            Some(junta) => {
                self.base_nota = Some((tarefa, atual, versao.clone()));
                self.aviso = Some(("Juntei com o que o agente acrescentou".into(), 0.0, Tom::Neutro));
                self.gravar_nota(ctx, tarefa, tipo, periodo, junta, versao);
            }
            None => {
                self.estado_nota.remove(&tarefa);
                self.conflito = Some(Conflito { tarefa, minha, do_agente: atual, versao });
            }
        }
    }

    /// Larga a edição da nota se ela é o mesmo texto que o núcleo acabou de
    /// recusar (mandar de novo daria o mesmo erro). Diz se largou.
    fn descartar_recusada(&mut self) -> bool {
        if self.nota.is_none() || self.nota != self.nota_recusada {
            return false;
        }
        if let Some((tarefa, _)) = self.nota.take() {
            self.estado_nota.remove(&tarefa);
            self.aviso = Some(("A nota não foi salva; ficou a anterior".into(), 0.0, Tom::Alerta));
        }
        self.nota_recusada = None;
        true
    }

    /// Sair: grava a nota pendente e confirma a remoção que esperava o "Desfazer".
    pub fn encerrar(&mut self, ctx: &egui::Context) {
        self.descartar_recusada();
        self.salvar_nota(ctx);
        self.confirmar_remocao();
        if self.tema_sessao.take().is_some() {
            self.tema_base.aplicar(ctx);
        }
    }

    /// Guarda que a tarefa saiu da daily de hoje. Se o núcleo recusar, o
    /// slide continua escondido só nesta apresentação, e a faixa avisa.
    fn tirar_da_daily(&mut self, tarefa: i64, dia: String) {
        self.esperar(Proprio::ForaDaDaily, tarefa);
        self.aviso = Some(("Tirada desta daily; a sprint continua com ela".into(), 0.0, Tom::Neutro));
        let envio = self.canal.0.clone();
        std::thread::spawn(move || {
            if let Err(erro) = api::tirar_da_daily(tarefa, &dia, true) {
                let _ = envio.send(Mensagem::ForaDaDailyFalhou { tarefa, erro });
            }
        });
    }

    fn confirmar_remocao(&mut self) {
        if let Some(r) = self.remocao.take() {
            let (id, tarefa) = (r.anexo.id, r.tarefa);
            self.esperar(Proprio::AnexoRemovido, tarefa);
            let envio = self.canal.0.clone();
            std::thread::spawn(move || {
                if let Err(e) = api::remover_anexo(id) {
                    eprintln!("removendo o anexo {id}: {e}");
                    let _ = envio.send(Mensagem::RemocaoFalhou(tarefa));
                }
            });
        }
    }

    fn receber(&mut self, ctx: &egui::Context) {
        self.imagens.receber(ctx);
        self.miniaturas.receber(ctx);
        while let Ok(m) = self.canal.1.try_recv() {
            match m {
                Mensagem::LousaReuniao(Ok(l)) => {
                    self.pedindo_lousa_reuniao = false;
                    if l.elementos.is_empty() {
                        self.aviso = Some(("A lousa da reunião ainda está vazia".into(), 0.0, Tom::Neutro));
                        continue;
                    }
                    let tema_base = self.tema_sessao.unwrap_or(self.tema_base);
                    self.palco = Some(Box::new(lousa::palco::Palco::novo(l.elementos, None, tema_base)));
                }
                Mensagem::LousaReuniao(Err(e)) => {
                    self.pedindo_lousa_reuniao = false;
                    self.aviso = Some((format!("Não consegui abrir a lousa da reunião: {e}"), 0.0, Tom::Alerta));
                }
                Mensagem::Deck { resultado: Ok(deck), manter } => {
                    self.buscando = false;
                    self.erro = None;
                    // Pedido antes de a opção ser desligada: nunca entra na tela.
                    if !self.mostrar_tempo && deck.tempo_agentes {
                        self.buscar_de_novo = false;
                        let manter = manter.or(self.tarefa_atual());
                        self.pedir_deck(ctx, manter);
                        continue;
                    }
                    self.tirando_tempo = false;
                    // Mais mudanças chegaram durante a busca: busca de novo, no slide de agora.
                    let manter = if std::mem::take(&mut self.buscar_de_novo) {
                        let atual = self.tarefa_atual();
                        self.pedir_deck(ctx, atual);
                        atual
                    } else {
                        manter
                    };
                    self.trocar_deck(*deck, manter);
                }
                Mensagem::Deck { resultado: Err(e), .. } => {
                    self.buscando = false;
                    self.erro = Some(e);
                }
                Mensagem::NotaSalva { tarefa, texto, resultado } => match resultado {
                    Ok(api::NotaGravada::Ok(versao)) => {
                        if let Some(s) = self.deck.as_mut().and_then(|d| d.slides.iter_mut().find(|s| s.tarefa_id == tarefa)) {
                            s.nota = texto.clone();
                            s.nota_versao = versao.clone();
                        }
                        if self.base_nota.as_ref().is_some_and(|(t, _, _)| *t == tarefa) {
                            self.base_nota = Some((tarefa, texto, versao));
                        }
                        self.nota_agente.remove(&tarefa);
                        self.estado_nota.insert(tarefa, EstadoNota::Salvo);
                    }
                    Ok(api::NotaGravada::Mudou { texto: atual, versao }) => {
                        self.nao_esperar(Proprio::Nota, tarefa);
                        self.nota_mudou(ctx, tarefa, texto, atual, versao);
                    }
                    Err(e) => {
                        self.nao_esperar(Proprio::Nota, tarefa);
                        // Guarda o texto para tentar de novo (ou corrigir).
                        self.nota_recusada = Some((tarefa, texto.clone()));
                        if self.nota.is_none() {
                            self.nota = Some((tarefa, texto));
                        }
                        self.estado_nota.insert(tarefa, EstadoNota::Erro(e));
                    }
                },
                Mensagem::RemocaoFalhou(tarefa) => self.nao_esperar(Proprio::AnexoRemovido, tarefa),
                Mensagem::ForaDaDailyFalhou { tarefa, erro } => {
                    self.nao_esperar(Proprio::ForaDaDaily, tarefa);
                    self.aviso = Some((format!("Escondido só agora; não consegui tirar da daily: {erro}"), 0.0, Tom::Alerta));
                }
                Mensagem::Progresso(p) => self.progresso = Some(p),
                Mensagem::Enviado { tarefa, nome, resultado } => match resultado {
                    Ok(anexo) => {
                        if let Some(s) = self.deck.as_mut().and_then(|d| d.slides.iter_mut().find(|s| s.tarefa_id == tarefa)) {
                            if !anexo.video() {
                                s.numeros.capturas += 1;
                            }
                            s.anexos.insert(0, anexo);
                        }
                        // O anexo novo vira o principal.
                        self.principal.insert(tarefa, 0);
                    }
                    Err(e) => {
                        self.nao_esperar(Proprio::AnexoNovo, tarefa);
                        self.aviso = Some((format!("Não consegui anexar {nome}: {e}"), 0.0, Tom::Alerta));
                    }
                },
                Mensagem::FimDosEnvios => {
                    self.enviando = false;
                    self.progresso = None;
                    self.enviar_fila(ctx);
                }
                Mensagem::VideoFalhou(e) => {
                    self.abrindo_video = None;
                    self.erro_video = Some(e);
                }
                Mensagem::Lousa { tarefa, resultado } => {
                    let estado = match resultado {
                        Ok(aberta) => LousaDoSlide::Pronta(aberta.elementos),
                        Err(e) => LousaDoSlide::Erro(e),
                    };
                    self.lousas.insert(tarefa, estado);
                }
                Mensagem::Grande { anexo, resultado } => {
                    if let Some(v) = self.visor.as_mut().filter(|v| v.anexo == anexo) {
                        v.imagem = Some(resultado.map(|img| ctx.load_texture(format!("visor-{anexo}"), img, egui::TextureOptions::LINEAR)));
                    }
                }
            }
        }
    }

    // Anexar

    /// Confere os arquivos antes de enviar (formato e tamanho) e põe na fila.
    fn anexar(&mut self, ctx: &egui::Context, arquivos: Vec<PathBuf>) {
        let Some(slide) = self.slide() else {
            self.aviso = Some(("Abra o slide de uma tarefa para anexar".into(), 0.0, Tom::Alerta));
            return;
        };
        let tarefa = slide.tarefa_id;
        for arquivo in arquivos {
            let nome = arquivo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let Some((_, video)) = api::tipo_do_arquivo(&arquivo) else {
                let extensao = arquivo.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_else(|| "sem extensão".into());
                self.aviso = Some((format!("{extensao} não é aceito. Use png, jpg, mp4, webm, mkv ou mov."), 0.0, Tom::Alerta));
                continue;
            };
            let bytes = std::fs::metadata(&arquivo).map(|m| m.len()).unwrap_or(0);
            if video && bytes > api::MAIOR_VIDEO {
                self.aviso = Some((format!("{nome} tem {}; o limite é 512 MB.", registro::tamanho(bytes)), 0.0, Tom::Alerta));
                continue;
            }
            if !video && bytes > 8 << 20 {
                self.aviso = Some((format!("{nome} tem {}; o limite de uma foto é 8 MB.", registro::tamanho(bytes)), 0.0, Tom::Alerta));
                continue;
            }
            self.fila.push((tarefa, Envio::Arquivo(arquivo), nome));
            self.esperar(Proprio::AnexoNovo, tarefa);
        }
        self.enviar_fila(ctx);
    }

    /// Envia a fila numa thread só, um arquivo depois do outro. O progresso
    /// só repinta quando o número muda.
    fn enviar_fila(&mut self, ctx: &egui::Context) {
        if self.enviando || self.fila.is_empty() {
            return;
        }
        self.enviando = true;
        let fila = std::mem::take(&mut self.fila);
        let envio = self.canal.0.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let arquivos = fila.len();
            for (n, (tarefa, item, nome)) in fila.into_iter().enumerate() {
                let mut ultimo = u64::MAX;
                let mut avisar = |enviados: u64, total: u64| {
                    let porcento = enviados * 100 / total.max(1);
                    if porcento != ultimo {
                        ultimo = porcento;
                        let _ = envio.send(Mensagem::Progresso(Progresso { nome: nome.clone(), arquivo: n + 1, arquivos, enviados, total }));
                        ctx.request_repaint();
                    }
                };
                let resultado = match &item {
                    Envio::Arquivo(caminho) => {
                        let video = api::tipo_do_arquivo(caminho).is_some_and(|(_, v)| v);
                        let bytes = std::fs::metadata(caminho).map(|m| m.len()).unwrap_or(0);
                        api::anexar_arquivo(tarefa, caminho, &mut avisar).map(|a| api::AnexoSlide {
                            id: a.id,
                            tipo: if video { "video".into() } else { "imagem".into() },
                            nome: nome.clone(),
                            bytes,
                            ..Default::default()
                        })
                    }
                    Envio::Png(png) => {
                        avisar(0, png.len() as u64);
                        api::anexar_png(tarefa, png).map(|a| api::AnexoSlide { id: a.id, tipo: "imagem".into(), ..Default::default() })
                    }
                };
                let _ = envio.send(Mensagem::Enviado { tarefa, nome, resultado });
                ctx.request_repaint();
            }
            let _ = envio.send(Mensagem::FimDosEnvios);
            ctx.request_repaint();
        });
    }

    fn escolher_arquivos(&mut self, ctx: &egui::Context) {
        if self.slide().is_none() {
            self.aviso = Some(("Abra o slide de uma tarefa para anexar".into(), 0.0, Tom::Alerta));
            return;
        }
        // O diálogo do sistema numa thread; os arquivos voltam como se tivessem sido soltos.
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let escolhidos =
                rfd::FileDialog::new().set_title("Adicionar foto ou vídeo").add_filter("Fotos e vídeos", &api::FORMATOS_ANEXO).pick_files().unwrap_or_default();
            if !escolhidos.is_empty() {
                ctx.data_mut(|d| d.insert_temp(Id::new("arquivos-escolhidos"), escolhidos));
                ctx.request_repaint();
            }
        });
    }

    fn colar_imagem(&mut self) {
        let Some(slide) = self.slide() else {
            self.aviso = Some(("Abra o slide de uma tarefa para anexar".into(), 0.0, Tom::Alerta));
            return;
        };
        let tarefa = slide.tarefa_id;
        let Ok(mut area) = arboard::Clipboard::new() else { return };
        let Ok(imagem) = area.get_image() else { return };
        if imagem.width * imagem.height > 40_000_000 {
            self.aviso = Some(("Imagem grande demais para anexar.".into(), 0.0, Tom::Alerta));
            return;
        }
        let mut png = Vec::new();
        let mut codificador = png::Encoder::new(&mut png, imagem.width as u32, imagem.height as u32);
        codificador.set_color(png::ColorType::Rgba);
        codificador.set_depth(png::BitDepth::Eight);
        let ok = codificador.write_header().and_then(|mut e| e.write_image_data(&imagem.bytes).and_then(|_| e.finish())).is_ok();
        if ok {
            self.fila.push((tarefa, Envio::Png(png), "imagem colada".into()));
            self.esperar(Proprio::AnexoNovo, tarefa);
        }
    }

    fn abrir_video(&mut self, ctx: &egui::Context, anexo: i64, agora: f64) {
        self.erro_video = None;
        self.abrindo_video = Some(agora);
        ctx.request_repaint_after(std::time::Duration::from_secs(3));
        let envio = self.canal.0.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let resultado = api::info_anexo(anexo).and_then(|info| {
                let falhou = envio.clone();
                let acordar = ctx.clone();
                sistema::abrir_arquivo(&info.caminho, move |e| {
                    let _ = falhou.send(Mensagem::VideoFalhou(e));
                    acordar.request_repaint();
                })
            });
            if let Err(e) = resultado {
                let _ = envio.send(Mensagem::VideoFalhou(e));
                ctx.request_repaint();
            }
        });
    }

    fn remover_anexo(&mut self, tarefa: i64, indice: usize, agora: f64) {
        self.confirmar_remocao();
        let Some(s) = self.deck.as_mut().and_then(|d| d.slides.iter_mut().find(|s| s.tarefa_id == tarefa)) else { return };
        if indice >= s.anexos.len() {
            return;
        }
        let anexo = s.anexos.remove(indice);
        if !anexo.video() {
            s.numeros.capturas = s.numeros.capturas.saturating_sub(1);
        }
        let n = s.anexos.len();
        if let Some(p) = self.principal.get_mut(&tarefa) {
            *p = (*p).min(n.saturating_sub(1));
        }
        self.remocao = Some(Remocao { tarefa, anexo, indice, ate: agora + 8.0 });
    }

    fn desfazer_remocao(&mut self) {
        let Some(r) = self.remocao.take() else { return };
        if let Some(s) = self.deck.as_mut().and_then(|d| d.slides.iter_mut().find(|s| s.tarefa_id == r.tarefa)) {
            if !r.anexo.video() {
                s.numeros.capturas += 1;
            }
            let i = r.indice.min(s.anexos.len());
            s.anexos.insert(i, r.anexo);
            self.principal.insert(r.tarefa, i);
        }
    }

    fn abrir_visor(&mut self, ctx: &egui::Context, tarefa: i64, anexo: i64) {
        self.visor = Some(Visor { tarefa, anexo, imagem: None });
        registro::em_segundo_plano(&self.canal.0, ctx, move || Mensagem::Grande {
            anexo,
            resultado: api::ler_anexo(anexo).and_then(|png| registro::decodificar(&png, None)),
        });
    }

    // Teclado

    /// Atalhos da apresentação, tirados da fila antes de qualquer campo ver.
    /// Só valem sem campo de texto em foco (Backspace na nota apaga texto).
    fn teclado(&mut self, ctx: &egui::Context, agora: f64) -> Option<Pedido> {
        if self.visor.is_some() {
            return self.teclado_visor(ctx);
        }
        // Com a caixa "Pedir ao agente" ou a janela de conflito abertas, as
        // teclas são delas (setas, F11, letras e Esc).
        if self.caixa.is_some() || self.conflito.is_some() {
            return None;
        }
        let tecla = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if self.atalhos {
            if tecla(Key::Escape) || tecla(Key::Questionmark) {
                self.atalhos = false;
            }
            return None;
        }
        if tecla(Key::F11) {
            self.tela_cheia = !self.tela_cheia;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.tela_cheia));
        }
        // Esc com a nota em edição: o egui já tirou o foco do campo no começo
        // do quadro; aqui o Esc só salva e a apresentação continua.
        if self.nota.is_some() && !ctx.text_edit_focused() && tecla(Key::Escape) {
            // A nota que o núcleo recusou (parece ter senha) sai sem salvar no
            // segundo Esc; a gravada antes continua. Senão o Esc nunca sairia.
            if !self.descartar_recusada() {
                self.salvar_nota(ctx);
            }
            return None;
        }
        // Com a nota em foco, o clique fora é do campo (que salva ao perder o foco).
        if ctx.text_edit_focused() {
            if self.nota.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter)) {
                ctx.memory_mut(|m| m.surrender_focus(self.id_nota()));
            }
            return None;
        }
        let total = self.paginas.len();
        let de_novo = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Key { key: Key::H, pressed: true, repeat: true, .. })));
        let interrogacao = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Text(t) if t == "?")));
        let colou = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Key { key: Key::V, pressed: false, modifiers, .. } if modifiers.command)));
        if tecla(Key::Escape) {
            if self.detalhe.is_some() {
                self.fechar_detalhe(ctx);
                return None;
            }
            return Some(Pedido::Sair);
        }
        // Enter num bloco abre o detalhe da primeira tarefa dele.
        let bloco = match self.paginas.get(self.atual) {
            Some(Pagina::Bloco { tarefas, .. }) if self.detalhe.is_none() => tarefas.first().copied(),
            _ => None,
        };
        if let Some(i) = bloco
            && tecla(Key::Enter)
        {
            self.abrir_detalhe(i);
        } else if tecla(Key::ArrowRight) || tecla(Key::PageDown) || tecla(Key::Space) || tecla(Key::Enter) {
            self.ir(navegar(self.atual, total, 1), ctx);
        } else if tecla(Key::ArrowLeft) || tecla(Key::PageUp) || tecla(Key::Backspace) {
            self.ir(navegar(self.atual, total, -1), ctx);
        } else if tecla(Key::Home) || tecla(Key::C) {
            self.ir(0, ctx);
        } else if tecla(Key::End) {
            self.ir(total.saturating_sub(1), ctx);
        } else if tecla(Key::N) {
            self.focar_nota = self.slide().is_some();
        } else if tecla(Key::P) {
            self.abrir_caixa(ctx);
        } else if tecla(Key::L) {
            // Anexos ou lousa: a escolha vale para os próximos slides.
            if self.slide().is_some_and(|s| s.lousa.is_some() && !s.anexos.is_empty()) {
                self.aba_lousa = !self.aba_lousa;
            }
        } else if tecla(Key::A) {
            self.escolher_arquivos(ctx);
        } else if tecla(Key::R) {
            let manter = self.tarefa_atual();
            self.pedir_deck(ctx, manter);
        } else if tecla(Key::H) {
            // No bloco, some a tarefa sob o mouse; no slide ou no detalhe, a dele.
            // Só o primeiro toque vale: com a tecla segurada, a repetição
            // esconderia a tarefa que sobe para o lugar da que saiu.
            if let Some(t) = self.tarefa_atual().or(self.sob_mouse).filter(|_| !de_novo) {
                self.sob_mouse = None;
                self.escondidos.insert(t);
                self.detalhe = None;
                if let Some(d) = &self.deck {
                    self.paginas = montar_paginas(d, &self.escondidos, &|_| 0.0, f32::INFINITY);
                    self.repaginar = true;
                    self.atual = self.atual.min(self.paginas.len() - 1);
                    // Na daily, a tarefa sai também da daily de hoje (a sprint continua com ela).
                    if d.tipo == "daily" {
                        let dia = d.chave_nota.clone();
                        self.tirar_da_daily(t, dia);
                    }
                }
            }
        } else if tecla(Key::T) {
            let atual = self.tema_sessao.unwrap_or(self.tema_base);
            let outro = if atual.escuro() { tema::Escolha::Leitura } else { tema::Escolha::Escuro };
            outro.aplicar(ctx);
            self.tema_sessao = Some(outro);
        } else if tecla(Key::Q) {
            self.abrir_lousa_da_reuniao(ctx);
        } else if interrogacao || tecla(Key::Questionmark) {
            self.atalhos = true;
        } else if colou {
            self.colar_imagem();
            self.enviar_fila(ctx);
        }
        // Sem campo em foco, o texto das teclas não serve a ninguém; tirar da
        // fila evita que o "n" do atalho caia na nota que acabou de abrir.
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Text(_))));
        let _ = agora;
        None
    }

    fn teclado_visor(&mut self, ctx: &egui::Context) -> Option<Pedido> {
        let tecla = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        let visor = self.visor.as_ref()?;
        let (tarefa, anexo) = (visor.tarefa, visor.anexo);
        if tecla(Key::Escape) {
            self.visor = None;
            return None;
        }
        let passo = if tecla(Key::ArrowRight) {
            1
        } else if tecla(Key::ArrowLeft) {
            -1
        } else {
            return None;
        };
        let imagens: Vec<i64> = self.slide().map(|s| s.anexos.iter().filter(|a| !a.video()).map(|a| a.id).collect()).unwrap_or_default();
        if let Some(i) = imagens.iter().position(|a| *a == anexo) {
            let proxima = imagens[navegar(i, imagens.len(), passo)];
            if proxima != anexo {
                self.abrir_visor(ctx, tarefa, proxima);
            }
        }
        None
    }

    fn id_nota(&self) -> Id {
        Id::new(("nota-slide", self.nota.as_ref().map_or(0, |n| n.0)))
    }

    // Desenho

    /// Desenha a apresentação na janela inteira e devolve o que ela pede.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, favo: &mut tema::Favo, modelo: &dados::Modelo, agora: f64) -> Option<Pedido> {
        let ctx = ui.ctx().clone();
        self.receber(&ctx);
        self.desenho_lousa.receber(&ctx);
        // O palco da lousa do slide, por cima: Esc volta ao mesmo slide.
        if let Some(palco) = &mut self.palco {
            match palco.mostrar(ui, modelo, agora) {
                Some(lousa::palco::PedidoPalco::Sair) => {
                    self.palco = None;
                    ctx.request_repaint();
                }
                Some(lousa::palco::PedidoPalco::AbrirTarefa(t)) => return Some(Pedido::AbrirTarefa(t)),
                None => {}
            }
            return None;
        }
        if let Some(arquivos) = ctx.data_mut(|d| d.remove_temp::<Vec<PathBuf>>(Id::new("arquivos-escolhidos"))) {
            self.anexar(&ctx, arquivos);
        }
        let soltos: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|c| c.is_file()).collect());
        if !soltos.is_empty() {
            self.anexar(&ctx, soltos);
        }
        let mut pedido = self.teclado(&ctx, agora);
        if let Some(r) = &self.remocao
            && agora >= r.ate
        {
            self.confirmar_remocao();
        }
        let p = cores();
        let tela = ui.max_rect();
        ui.painter().rect_filled(tela, 0, p.fundo);
        if self.deck.is_some() && (self.repaginar || self.tamanho_paginado != tela.size()) {
            self.repaginar(ui.painter(), tela.size());
        }
        self.sob_mouse = None;
        let (direita, abas) = self.slide().map_or((false, false), |s| (!s.anexos.is_empty() || s.lousa.is_some(), !s.anexos.is_empty() && s.lousa.is_some()));
        let r = regioes(tela.size(), direita, abas);
        let deslocar = |rect: Rect| rect.translate(tela.min.to_vec2());
        let r = Regioes {
            cabecalho: deslocar(r.cabecalho),
            titulo: deslocar(r.titulo),
            conteudo: deslocar(r.conteudo),
            esquerda: deslocar(r.esquerda),
            direita: deslocar(r.direita),
            seletor_direita: deslocar(r.seletor_direita),
            acoes: deslocar(r.acoes),
            avisos: deslocar(r.avisos),
            reserva_agente: deslocar(r.reserva_agente),
            fechar: deslocar(r.fechar),
            ..r
        };

        match (&self.deck, &self.erro) {
            (None, Some(e)) => {
                let texto = format!("Não consegui montar a apresentação: {e}");
                ui.painter().text(tela.center(), egui::Align2::CENTER_CENTER, texto, FontId::proportional(18.0), p.erro);
            }
            (None, None) => {
                let texto = if self.tirando_tempo { "Atualizando sem o tempo dos agentes…" } else { "Montando a apresentação…" };
                ui.painter().text(tela.center(), egui::Align2::CENTER_CENTER, texto, FontId::proportional(18.0), p.suave);
            }
            (Some(_), _) => {
                let pagina = self.paginas.get(self.atual).cloned().unwrap_or(Pagina::Capa);
                match (self.detalhe, pagina) {
                    (Some(i), _) => self.slide_tarefa(ui, &r, i, modelo, agora),
                    (None, Pagina::Capa) => {
                        favo.desenhar(ui.painter(), tela);
                        self.capa(ui, &r, tela);
                    }
                    (None, Pagina::Tarefa(i)) => self.slide_tarefa(ui, &r, i, modelo, agora),
                    (None, Pagina::Bloco { projeto, workspace, tarefas, parte, partes, .. }) => {
                        self.slide_bloco(ui, &r, &projeto, &workspace, &tarefas, parte, partes);
                    }
                    (None, Pagina::Mais) => {
                        favo.desenhar(ui.painter(), tela);
                        self.pagina_mais(ui, tela, r.escala);
                    }
                    (None, Pagina::Fim) => {
                        favo.desenhar(ui.painter(), tela);
                        self.fim(ui, tela, r.escala);
                    }
                }
            }
        }
        if let Some(p) = self.barra(ui, &r, agora) {
            pedido = Some(p);
        }
        self.camadas(&ctx, tela);
        if let Some(Pedido::Sair) = pedido {
            self.encerrar(&ctx);
        }
        // O próximo e o anterior já ficam prontos (só a imagem principal de cada).
        self.preparar_vizinhos(&ctx);
        pedido
    }

    fn preparar_vizinhos(&mut self, ctx: &egui::Context) {
        let Some(deck) = &self.deck else { return };
        let mut ids = Vec::new();
        let mut lousas = Vec::new();
        for passo in [-1isize, 1] {
            let i = navegar(self.atual, self.paginas.len(), passo);
            if let Some(Pagina::Tarefa(s)) = self.paginas.get(i) {
                let slide = &deck.slides[*s];
                if let Some(l) = slide.lousa {
                    lousas.push((slide.tarefa_id, l.id));
                }
                if let Some(a) = slide.anexos.get(self.principal.get(&slide.tarefa_id).copied().unwrap_or(0))
                    && !a.video()
                {
                    ids.push(a.id);
                }
            }
        }
        for id in ids {
            self.imagens.pedir(ctx, id);
        }
        for (tarefa, id) in lousas {
            self.pedir_lousa(ctx, tarefa, id);
        }
    }

    /// Capa: título, período, números grandes, destaques e as partes com as tarefas.
    fn capa(&mut self, ui: &mut egui::Ui, r: &Regioes, tela: Rect) {
        let p = cores();
        let Some(deck) = self.deck.clone() else { return };
        let s = r.escala;
        let px = |v: f32| (v * s).round();
        let margem = px(96.0);
        let esquerda = tela.left() + margem;
        let largura = tela.width() - 2.0 * margem;
        let pintor = ui.painter().clone();
        let titulo = registro::texto_em_linhas(&pintor, &deck.titulo, forte(px(56.0)), p.texto, largura, 1);
        pintor.galley(pos2(esquerda, tela.top() + px(96.0)), titulo.clone(), p.texto);
        pintor.text(
            pos2(esquerda, tela.top() + px(96.0) + titulo.size().y + px(12.0)),
            egui::Align2::LEFT_TOP,
            &deck.periodo,
            FontId::proportional(px(20.0)),
            p.suave,
        );

        let numeros = Rect::from_min_size(pos2(esquerda, tela.top() + px(240.0)), vec2(largura, px(110.0)));
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(numeros));
        registro::numeros(&mut filho, &deck.capa.numeros, px(64.0).max(48.0), true, self.com_tempo());

        // Destaques: até 2 linhas cada (o núcleo já põe o verbo antes dos títulos).
        let mut y = tela.top() + px(384.0);
        for d in deck.capa.destaques.iter().filter(|d| !d.is_empty()).take(3) {
            let g = registro::texto_em_linhas(&pintor, d, FontId::proportional(px(22.0)), p.texto, largura - 24.0, 2);
            let primeira = g.rows.first().map_or(px(28.0), |l| l.rect().height());
            pintor.circle_filled(pos2(esquerda + 4.0, y + primeira / 2.0), 4.0, p.destaque);
            let altura = g.size().y;
            pintor.galley(pos2(esquerda + 20.0, y), g, p.texto);
            y += altura + px(12.0);
        }

        // Partes em colunas: os dias na daily, os projetos na sprint. Até 3
        // por fileira e até 2 fileiras; passando de 6, a última célula diz
        // quais projetos ficaram de fora (nunca as tarefas de um projeto sob
        // o rótulo de outro). Cada tarefa: a marca do estado, o título em até
        // 2 linhas e a palavra do estado, para não depender só da cor do ponto.
        let topo = (y + px(28.0)).max(tela.top() + px(500.0));
        let base = r.acoes.top() - px(24.0);
        let partes: Vec<&api::ParteCapa> = deck.capa.partes.iter().filter(|p| !p.tarefas.is_empty()).collect();
        let celulas = celulas_capa(partes.len());
        let colunas = celulas.clamp(1, 3);
        let fileiras = celulas.div_ceil(3).max(1);
        let vao = px(48.0);
        let vao_fileira = px(24.0);
        let largura_coluna = (largura - vao * (colunas as f32 - 1.0)) / colunas as f32;
        let altura_fileira = ((base - topo) - vao_fileira * (fileiras as f32 - 1.0)) / fileiras as f32;
        let fonte = FontId::proportional(px(20.0));
        let raio = px(5.0);
        let mut ir_para = None;
        for c in 0..celulas {
            let x = esquerda + (c % 3) as f32 * (largura_coluna + vao);
            let topo_celula = topo + (c / 3) as f32 * (altura_fileira + vao_fileira);
            let base_celula = topo_celula + altura_fileira;
            if c + 1 == celulas && partes.len() > celulas {
                // A última célula: os projetos que não couberam, pelo nome.
                let fora = &partes[c..];
                let titulo = if fora.len() == 1 { "E mais 1 projeto".to_string() } else { format!("E mais {} projetos", fora.len()) };
                pintor.text(pos2(x, topo_celula), egui::Align2::LEFT_TOP, titulo, forte(px(17.0)), p.suave);
                let nomes: Vec<String> = fora.iter().map(|p| rotulo_parte(&p.titulo, p.tarefas.len())).collect();
                let linhas = (((base_celula - topo_celula - px(32.0)) / px(26.0)).floor().max(1.0)) as usize;
                let g = tema::cortar(&pintor, &nomes.join(", "), egui::TextFormat::simple(fonte.clone(), p.texto), largura_coluna, linhas, false);
                pintor.galley(pos2(x, topo_celula + px(32.0)), g, p.texto);
                continue;
            }
            let parte = partes[c];
            let rotulo = rotulo_parte(&parte.titulo, parte.tarefas.len());
            let g_rotulo = tema::cortar(&pintor, &rotulo, egui::TextFormat::simple(forte(px(17.0)), p.suave), largura_coluna, 1, false);
            pintor.galley(pos2(x, topo_celula), g_rotulo, p.suave);
            let mut y = topo_celula + px(32.0);
            let linha_mais = px(30.0);
            for (n, tarefa) in parte.tarefas.iter().enumerate() {
                let Some(slide) = deck.slides.iter().find(|s| s.tarefa_id == *tarefa) else { continue };
                let (cor, rotulo, marca) = registro::estado_do_slide(slide);
                let palavra = if rotulo == "Aguardando você" { "aguardando".to_string() } else { rotulo.to_lowercase() };
                // O título se corta; a palavra do estado fica inteira, no fim da última linha.
                let g_palavra = pintor.layout_no_wrap(format!(" · {palavra}"), fonte.clone(), p.suave);
                let largura_titulo = largura_coluna - px(36.0) - g_palavra.size().x;
                let g = tema::cortar(&pintor, &slide.titulo, egui::TextFormat::simple(fonte.clone(), p.texto), largura_titulo, 2, false);
                let restam = parte.tarefas.len() - n;
                // Sem lugar para esta e as que faltam: "+N" no lugar dela.
                if y + g.size().y + px(8.0) > base_celula || (restam > 1 && y + g.size().y + px(8.0) + linha_mais > base_celula) {
                    pintor.text(
                        pos2(x + px(22.0), y + linha_mais / 2.0),
                        egui::Align2::LEFT_CENTER,
                        format!("+{restam}"),
                        FontId::proportional(px(18.0)),
                        p.suave,
                    );
                    break;
                }
                let area = Rect::from_min_size(pos2(x - 8.0, y - px(4.0)), vec2(largura_coluna, g.size().y + px(8.0)));
                let resposta = ui.interact(area, Id::new(("capa-tarefa", c, tarefa)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                let resposta = if g.elided { resposta.on_hover_text(&slide.titulo) } else { resposta };
                if resposta.hovered() {
                    pintor.rect_filled(area, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
                }
                let primeira = g.rows.first().map_or(px(24.0), |l| l.rect().height());
                tema::marca(&pintor, pos2(x + raio, y + primeira / 2.0), raio, cor, marca);
                let altura = g.size().y;
                let fim = g.rows.last().map_or(Rect::NOTHING, |l| l.rect());
                pintor.galley(pos2(x + px(22.0), y), g, p.texto);
                pintor.galley(pos2(x + px(22.0) + fim.right(), y + fim.top()), g_palavra, p.suave);
                if resposta.clicked() {
                    ir_para = Some(*tarefa);
                }
                y += altura + px(8.0);
            }
        }
        if partes.is_empty() && !deck.capa.novas.is_empty() {
            let mut y = topo;
            pintor.text(pos2(esquerda, y), egui::Align2::LEFT_TOP, "Tarefas novas", forte(px(17.0)), p.suave);
            for nova in deck.capa.novas.iter().take(6) {
                y += px(34.0);
                let g = registro::texto_em_linhas(&pintor, nova, FontId::proportional(px(20.0)), p.texto, largura - px(18.0), 1);
                pintor.galley(pos2(esquerda + px(18.0), y), g, p.texto);
            }
        }
        if let Some(t) = ir_para {
            self.ir_para_tarefa(t, ui.ctx());
        }
    }

    /// O tempo dos agentes aparece (a opção do perfil e o deck que veio com ele).
    fn com_tempo(&self) -> bool {
        self.mostrar_tempo && self.deck.as_ref().is_some_and(|d| d.tempo_agentes)
    }

    /// O slide de um bloco, como a pessoa monta o slide dela: a etiqueta do
    /// grupo em cima, o nome como título e, embaixo, uma tabela com o tópico
    /// e a frase de cada tarefa, separadas por um fio. O clique numa linha
    /// abre o detalhe.
    #[allow(clippy::too_many_arguments)]
    fn slide_bloco(&mut self, ui: &mut egui::Ui, r: &Regioes, projeto: &str, workspace: &str, tarefas: &[usize], parte: usize, partes: usize) {
        let p = cores();
        let Some(deck) = self.deck.clone() else { return };
        let s = r.escala;
        let px = |v: f32| (v * s).round();
        let pintor = ui.painter().clone();

        // Cabeçalho: a etiqueta (a daily ou a sprint e o período, com o
        // workspace quando há mais de um) à esquerda, a posição à direita.
        let etiqueta = {
            let mut partes_etiqueta = vec![if self.daily() { "Daily".to_string() } else { "Sprint".to_string() }];
            if !deck.periodo.is_empty() {
                partes_etiqueta.push(deck.periodo.clone());
            }
            if !workspace.is_empty() {
                partes_etiqueta.push(workspace.to_string());
            }
            partes_etiqueta.join("  ·  ").to_uppercase()
        };
        let g = tema::cortar(&pintor, &etiqueta, egui::TextFormat::simple(forte(px(14.0)), p.destaque), r.cabecalho.width() - px(120.0), 1, false);
        pintor.galley(pos2(r.cabecalho.left(), r.cabecalho.center().y - g.size().y / 2.0), g, p.destaque);
        let blocos: Vec<usize> = self.paginas.iter().enumerate().filter(|(_, p)| matches!(p, Pagina::Bloco { .. })).map(|(n, _)| n).collect();
        let posicao = blocos.iter().position(|n| *n == self.atual).map_or(0, |n| n + 1);
        pintor.text(r.cabecalho.right_center(), egui::Align2::RIGHT_CENTER, format!("{posicao} / {}", blocos.len()), forte(15.0), p.suave);

        // Título: só o assunto da sprint; o nome do projeto não aparece na
        // apresentação. "· 2 de 3" quando o bloco continua.
        let assunto = tarefas.first().and_then(|i| deck.slides.get(*i)).is_some_and(|s0| s0.projeto.is_empty() || s0.secao_id < 0);
        let titulo = if assunto { projeto } else { "" };
        let tamanho_titulo = px(44.0).max(34.0);
        let galeria = tema::cortar(&pintor, titulo, egui::TextFormat::simple(forte(tamanho_titulo), p.texto), r.titulo.width() - px(120.0), 1, false);
        let largura_titulo = galeria.size().x;
        let altura_titulo = galeria.size().y;
        pintor.galley(r.titulo.min, galeria, p.texto);
        if partes > 1 {
            let texto = if titulo.is_empty() { format!("Parte {parte} de {partes}") } else { format!(" · {parte} de {partes}") };
            let g = pintor.layout_no_wrap(texto, FontId::proportional(px(20.0)), p.suave);
            pintor.galley(pos2(r.titulo.left() + largura_titulo, r.titulo.top() + altura_titulo - g.size().y - px(6.0)), g, p.suave);
        }
        let sobra = (r.titulo.height() - altura_titulo).max(0.0);
        let largura = r.conteudo.width().min(px(1100.0));
        let area = Rect::from_min_max(pos2(r.conteudo.left(), r.conteudo.top() - sobra), pos2(r.conteudo.left() + largura, r.conteudo.bottom()));
        let recortado = pintor.with_clip_rect(Rect::from_min_max(area.min - vec2(px(12.0), px(8.0)), pos2(r.conteudo.right(), area.bottom())));
        let coluna = coluna_do_topico(largura, s);
        let fio = Stroke::new(1.0, p.borda);
        // O fio de cima da tabela.
        recortado.hline(area.left()..=area.right(), area.top(), fio);
        let mut y = area.top();
        let mut abrir = None;
        for &i in tarefas {
            let Some(slide) = deck.slides.get(i) else { continue };
            let medida = medir_tarefa_no_bloco(&pintor, slide, s, largura);
            let faixa = Rect::from_min_size(pos2(area.left() - px(12.0), y + 1.0), vec2(largura + px(24.0), medida.altura - 1.0));
            let resposta = ui.interact(faixa.intersect(area.expand2(vec2(px(12.0), px(8.0)))), Id::new(("tarefa-no-bloco", slide.tarefa_id)), Sense::click());
            if resposta.hovered() {
                recortado.rect_filled(faixa, CornerRadius::same(tema::RAIO_SUPERFICIE), p.realce);
                self.sob_mouse = Some(slide.tarefa_id);
            }
            if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                abrir = Some(i);
            }
            let (cor, _, marca) = registro::estado_do_slide(slide);
            let topo = y + px(VAO_LINHA_BLOCO) / 2.0;
            match &medida.frase {
                Some(frase) => {
                    // A tabela: a marca do estado, o tópico e, à direita, a frase.
                    let primeira = medida.titulo.rows.first().map_or(px(27.0), |l| l.rect().height());
                    tema::marca(&recortado, pos2(area.left() - px(2.0), topo + primeira / 2.0), px(5.0), cor, marca);
                    recortado.galley(pos2(area.left() + px(16.0), topo), medida.titulo.clone(), p.texto);
                    let mut x_selo = area.left() + px(16.0);
                    let y_selo = topo + medida.titulo.size().y + px(4.0);
                    for (texto, cor) in &medida.selos {
                        x_selo = registro::selo(&recortado, pos2(x_selo, y_selo), texto, forte(px(14.0)), *cor).right() + px(8.0);
                    }
                    recortado.galley(pos2(area.left() + coluna, topo), frase.clone(), p.texto);
                }
                None => {
                    // Sem montagem: o título e a nota em itens, como na página.
                    let primeira = medida.titulo.rows.first().map_or(px(26.0), |l| l.rect().height());
                    tema::marca(&recortado, pos2(area.left() + px(7.0), topo + primeira / 2.0), px(6.0), cor, marca);
                    let recuo = px(24.0);
                    recortado.galley(pos2(area.left() + recuo, topo), medida.titulo.clone(), p.texto);
                    let mut x_selo = area.right();
                    for (texto, cor) in medida.selos.iter().rev() {
                        let w = pintor.layout_no_wrap(texto.clone(), forte(px(14.0)), *cor).size().x + 12.0;
                        x_selo -= w;
                        registro::selo(&recortado, pos2(x_selo, topo + (primeira - px(14.0) - 6.0) / 2.0), texto, forte(px(14.0)), *cor);
                        x_selo -= px(8.0);
                    }
                    let y_corpo = topo + medida.titulo.size().y + px(10.0);
                    if let Some(n) = &medida.nota {
                        n.pintar(&recortado, pos2(area.left() + recuo, y_corpo));
                    } else if let Some(g) = &medida.estado {
                        recortado.galley(pos2(area.left() + recuo, y_corpo), g.clone(), p.suave);
                    }
                }
            }
            y += medida.altura;
            recortado.hline(area.left()..=area.right(), y, fio);
            if y > area.bottom() {
                break;
            }
        }
        if let Some(i) = abrir {
            self.abrir_detalhe(i);
        }
    }

    fn pagina_mais(&self, ui: &mut egui::Ui, tela: Rect, s: f32) {
        let p = cores();
        let Some(deck) = &self.deck else { return };
        let pintor = ui.painter();
        let onde = if self.daily() { "na página Daily" } else { "na página Sprint" };
        pintor.text(tela.center() - vec2(0.0, 24.0 * s), egui::Align2::CENTER_CENTER, format!("E mais {} tarefas", deck.mais), forte(32.0 * s), p.texto);
        pintor.text(tela.center() + vec2(0.0, 20.0 * s), egui::Align2::CENTER_CENTER, format!("veja a lista {onde}"), FontId::proportional(18.0 * s), p.suave);
    }

    fn fim(&self, ui: &mut egui::Ui, tela: Rect, s: f32) {
        let p = cores();
        let Some(deck) = &self.deck else { return };
        let titulo = if self.daily() { "Fim da daily" } else { "Fim da sprint" };
        let c = tela.center() - vec2(0.0, 80.0 * s);
        ui.painter().text(c, egui::Align2::CENTER_CENTER, titulo, forte(48.0 * s), p.texto);
        ui.painter().text(c + vec2(0.0, 44.0 * s), egui::Align2::CENTER_CENTER, "Esc volta", FontId::proportional(18.0 * s), p.suave);
        // Centralizados: a largura da fileira é medida antes.
        let com_tempo = self.com_tempo();
        let largura = registro::largura_numeros(ui.painter(), &deck.capa.numeros, 32.0, false, com_tempo);
        let area = Rect::from_min_size(pos2(tela.center().x - largura / 2.0, c.y + 44.0 * s + 40.0 * s), vec2(largura + 8.0, 90.0));
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area));
        registro::numeros(&mut filho, &deck.capa.numeros, 32.0, false, com_tempo);
    }

    /// O slide de uma tarefa: cabeçalho, título, o que foi feito, números,
    /// nota (à esquerda) e anexos (à direita).
    fn slide_tarefa(&mut self, ui: &mut egui::Ui, r: &Regioes, i: usize, modelo: &dados::Modelo, agora: f64) {
        let p = cores();
        let Some(slide) = self.deck.as_ref().map(|d| d.slides[i].clone()) else { return };
        let s = r.escala;
        let px = |v: f32| (v * s).round();
        let pintor = ui.painter().clone();

        // Cabeçalho: estado, Ontem · Hoje e a posição (sem o nome do projeto).
        let mut x = r.cabecalho.left();
        let y = r.cabecalho.center().y - 16.0;
        x = registro::pilula_do_slide(&slide, true).pintar(&pintor, pos2(x, y)).right() + 12.0;
        let mut marcas = slide.partes.iter().filter(|p| *p != "No período").peekable();
        while let Some(parte) = marcas.next() {
            let (fonte, cor) = if parte == "Hoje" { (forte(15.0), p.destaque) } else { (FontId::proportional(15.0), p.suave) };
            let g = pintor.layout_no_wrap(parte.clone(), fonte, cor);
            let w = g.size().x;
            pintor.galley(pos2(x, r.cabecalho.center().y - g.size().y / 2.0), g, cor);
            x += w;
            if marcas.peek().is_some() {
                let g = pintor.layout_no_wrap(" · ".into(), FontId::proportional(15.0), p.suave);
                let w = g.size().x;
                pintor.galley(pos2(x, r.cabecalho.center().y - g.size().y / 2.0), g, p.suave);
                x += w;
            }
        }
        if self.detalhe.is_some() {
            // Aberta de dentro do bloco: a pílula volta a ele.
            let pilula = Pilula::neutra("Voltar ao bloco (Esc)").grande();
            let largura = pilula.largura(&pintor);
            let caixa = pilula.pintar(&pintor, pos2(r.cabecalho.right() - largura, r.cabecalho.center().y - 16.0));
            if ui.interact(caixa, Id::new("voltar-ao-bloco"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.fechar_detalhe(ui.ctx());
            }
        } else {
            let tarefas: Vec<usize> = self.paginas.iter().enumerate().filter(|(_, p)| matches!(p, Pagina::Tarefa(_))).map(|(n, _)| n).collect();
            let posicao = tarefas.iter().position(|n| *n == self.atual).map_or(0, |n| n + 1);
            pintor.text(r.cabecalho.right_center(), egui::Align2::RIGHT_CENTER, format!("{posicao} / {}", tarefas.len()), forte(15.0), p.suave);
        }

        // Título em até 2 linhas, com a dica do título inteiro se cortou.
        let tamanho_titulo = px(40.0).max(34.0);
        let galeria = tema::cortar(&pintor, &slide.titulo, egui::TextFormat::simple(forte(tamanho_titulo), p.texto), r.titulo.width(), 2, false);
        let cortou = galeria.elided;
        let area_titulo = Rect::from_min_size(r.titulo.min, galeria.size());
        // Título de uma linha: o conteúdo sobe o que sobrou (sem vão grande
        // entre o título e "O que foi feito").
        let sobra = (r.titulo.height() - galeria.size().y).max(0.0);
        pintor.galley(r.titulo.min, galeria, p.texto);
        if cortou {
            ui.interact(area_titulo, Id::new("titulo-slide"), Sense::hover()).on_hover_text(&slide.titulo);
        }
        let subir = |rect: Rect| Rect::from_min_max(rect.min - vec2(0.0, sobra), rect.max);
        let r = Regioes { esquerda: subir(r.esquerda), direita: subir(r.direita), seletor_direita: r.seletor_direita.translate(vec2(0.0, -sobra)), ..*r };

        // A imagem que a montagem escolheu é a que aparece primeiro.
        if !self.principal.contains_key(&slide.tarefa_id)
            && let Some(m) = &slide.montagem
            && let Some(c) = m.cartoes.iter().find(|c| c.imagem != 0)
            && let Some(i) = slide.anexos.iter().position(|a| a.id == c.imagem)
        {
            self.principal.insert(slide.tarefa_id, i);
        }
        self.coluna_esquerda(ui, &r, &slide);
        // "Anexos | Lousa" quando a tarefa tem os dois.
        if !slide.anexos.is_empty() && slide.lousa.is_some() {
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(r.seletor_direita).layout(egui::Layout::left_to_right(egui::Align::Center)));
            if let Some(i) = tema::segmentado(&mut filho, &["Anexos", "Lousa"], self.aba_lousa as usize) {
                self.aba_lousa = i == 1;
            }
        }
        if self.mostra_lousa(&slide) {
            self.lousa_do_slide(ui, &r, &slide, modelo);
        } else if !slide.anexos.is_empty() {
            self.coluna_direita(ui, &r, &slide, agora);
        }
    }

    /// A lousa da tarefa na coluna da direita: só leitura, ajustada para
    /// caber (no máximo 100%). O clique abre o palco dela.
    fn lousa_do_slide(&mut self, ui: &mut egui::Ui, r: &Regioes, slide: &api::Slide, modelo: &dados::Modelo) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let s = r.escala;
        let px = |v: f32| (v * s).round();
        let topo = if r.seletor_direita.height() > 0.0 { r.seletor_direita.bottom() + px(12.0) } else { r.direita.top() };
        let area = Rect::from_min_max(pos2(r.direita.left(), topo), r.direita.max);
        let Some(resumo) = slide.lousa else { return };
        self.pedir_lousa(&ctx, slide.tarefa_id, resumo.id);
        let resposta = ui.interact(area, Id::new(("lousa-slide", slide.tarefa_id)), Sense::click());
        ui.painter().rect_filled(area, CornerRadius::same(tema::RAIO_SUPERFICIE), p.superficie);
        let dentro = area.shrink(px(16.0));
        match self.lousas.get(&slide.tarefa_id) {
            Some(LousaDoSlide::Pronta(elementos)) => {
                let caixas = elementos.iter().filter(|e| e.tipo != api::TipoElemento::Ligacao).map(lousa::desenho::caixa_quadro);
                if let Some(caixa) = lousa::camera::envolver(caixas) {
                    let camera = lousa::camera::Camera::enquadrar(dentro, caixa, 0.0, 1.0, false);
                    let tarefas = |id: i64| lousa::info_tarefa(modelo, id);
                    let marcas = lousa::desenho::Marcas { limpo: true, ..Default::default() };
                    self.desenho_lousa.desenhar(ui, dentro, &camera, elementos, &tarefas, &marcas);
                }
                if resposta.hovered() {
                    ui.painter().rect_stroke(
                        area,
                        CornerRadius::same(tema::RAIO_SUPERFICIE),
                        Stroke::new(1.0, p.destaque.gamma_multiply(0.55)),
                        StrokeKind::Inside,
                    );
                    let pilula = Pilula::neutra("Ampliar");
                    let largura = pilula.largura(ui.painter());
                    pilula.pintar(ui.painter(), area.right_bottom() - vec2(12.0 + largura, 12.0 + 22.0));
                }
                if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    let tema_base = self.tema_sessao.unwrap_or(self.tema_base);
                    self.palco = Some(Box::new(lousa::palco::Palco::novo(elementos.clone(), None, tema_base)));
                }
            }
            Some(LousaDoSlide::Erro(e)) => {
                ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, format!("Não consegui ler a lousa: {e}"), FontId::proportional(14.0), p.suave);
            }
            _ => {}
        }
    }

    /// A coluna da esquerda do slide da tarefa: os cartões da montagem (o
    /// resumo, uma mudança por cartão, o fluxo numerado, o que falta), como
    /// na lousa da reunião. Editando a nota (N), ou sem montagem, a coluna
    /// clássica (a nota, os números e o que foi feito).
    fn coluna_esquerda(&mut self, ui: &mut egui::Ui, r: &Regioes, slide: &api::Slide) {
        let editando = self.nota.as_ref().is_some_and(|(t, _)| *t == slide.tarefa_id) || self.focar_nota;
        let cartoes: Vec<&api::Cartao> = slide.montagem.as_ref().map(|m| m.cartoes.iter().filter(|c| c.imagem == 0).collect()).unwrap_or_default();
        if editando || cartoes.is_empty() {
            return self.coluna_esquerda_classica(ui, r, slide);
        }
        let p = cores();
        let s = r.escala;
        let px = |v: f32| (v * s).round();
        let pintor = ui.painter().clone();
        let area = r.esquerda;
        let largura = area.width().min(px(1100.0));
        let mut y = area.top();
        // A linha do pedido ao agente, acima dos cartões.
        if let Some(resumo) = self.pedidos.get(&slide.tarefa_id).cloned() {
            let meio = y + px(11.0);
            tema::marca(
                &pintor,
                pos2(area.left() + 4.5, meio),
                4.5,
                resumo.estado.cor,
                if resumo.estado.cheio { tema::Marca::Cheia } else { tema::Marca::Anel },
            );
            let texto = format!("Pedido: «{}» · {}", resumo.pedido.texto.replace('\n', " "), resumo.estado.longo.to_lowercase());
            let g = registro::texto_em_linhas(&pintor, &texto, FontId::proportional(px(14.0)), p.suave, largura - 16.0, 1);
            pintor.galley(pos2(area.left() + 16.0, meio - g.size().y / 2.0), g, p.suave);
            y += px(22.0) + px(12.0);
        }
        let limite = area.bottom() - px(30.0);
        let estilo = estilo_nota(px(21.0), p.texto, largura - px(28.0));
        let total = cartoes.len();
        for (n, c) in cartoes.iter().enumerate() {
            let titulo = pintor.layout(c.titulo.trim_end_matches('.').to_string(), forte(px(23.0)), p.texto, largura - px(28.0));
            let texto = registro::NotaMedida::nova(&pintor, c.texto.trim(), estilo);
            let altura_titulo = if c.titulo.trim().is_empty() { 0.0 } else { titulo.size().y + px(8.0) };
            let altura = altura_titulo + texto.altura;
            if y + altura > limite {
                let restam = total - n;
                let aviso =
                    if restam == 1 { "+1 cartão na lousa da reunião (Q)".to_string() } else { format!("+{restam} cartões na lousa da reunião (Q)") };
                pintor.text(pos2(area.left(), y + px(8.0)), egui::Align2::LEFT_TOP, aviso, FontId::proportional(px(15.0)), p.suave);
                break;
            }
            // O fio à esquerda marca o cartão; o do "Falta" leva a cor de alerta.
            let cor = if c.titulo.trim().eq_ignore_ascii_case("falta") {
                p.alerta
            } else if n == 0 {
                p.destaque
            } else {
                p.borda
            };
            pintor.vline(area.left() + px(4.0), y..=y + altura, Stroke::new(3.0, cor));
            if altura_titulo > 0.0 {
                pintor.galley(pos2(area.left() + px(24.0), y), titulo, p.texto);
            }
            texto.pintar(&pintor, pos2(area.left() + px(24.0), y + altura_titulo));
            y += altura + px(22.0);
        }
        // N edita a nota, que é de onde a montagem sai.
        pintor.text(
            pos2(area.left(), area.bottom() - px(18.0)),
            egui::Align2::LEFT_CENTER,
            "N edita a nota · a montagem acompanha",
            FontId::proportional(px(13.0)),
            p.suave,
        );
    }

    fn coluna_esquerda_classica(&mut self, ui: &mut egui::Ui, r: &Regioes, slide: &api::Slide) {
        let p = cores();
        let s = r.escala;
        let px = |v: f32| (v * s).round();
        let pintor = ui.painter().clone();
        let area = r.esquerda;
        // Sem anexos, o texto não passa de 1100 de largura.
        let largura = area.width().min(px(1100.0));

        // De cima para baixo: a nota (o roteiro da fala) logo abaixo do
        // título, os números e, por último, "O que foi feito", que fica com o
        // que sobra (recolhido, só o rótulo).
        let n = &slide.numeros;
        let com_tempo = self.com_tempo();
        let tem_numeros = (com_tempo && n.tempo_s > 0) || n.sessoes > 0 || n.erros > 0 || n.capturas > 0;
        let altura_numeros = if tem_numeros { px(64.0) } else { 0.0 };
        let vao_numeros = if tem_numeros { px(20.0) } else { 0.0 };
        // A linha do pedido ao agente, acima da nota.
        let resumo = self.pedidos.get(&slide.tarefa_id).cloned();
        let altura_pedido = if resumo.is_some() { px(22.0) + px(8.0) } else { 0.0 };
        let feito_minimo = if slide.feito.is_empty() { 0.0 } else { px(ALTURA_FEITO_RECOLHIDO) + px(12.0) };
        let maximo_nota = (area.height() - altura_pedido - altura_numeros - vao_numeros - feito_minimo - px(12.0)).max(px(80.0));
        // Respondido um pedido, se ainda não couber, o começo sai (rola até o fim).
        let pelo_fim = resumo.as_ref().is_some_and(|r| r.pedido.estado == "respondido");
        let altura_nota = self.altura_nota(&pintor, slide, s, largura, maximo_nota);

        let mut y = area.top();
        if let Some(r) = resumo {
            let meio = y + px(11.0);
            tema::marca(&pintor, pos2(area.left() + 4.5, meio), 4.5, r.estado.cor, if r.estado.cheio { tema::Marca::Cheia } else { tema::Marca::Anel });
            let texto = format!("Pedido: «{}» · {}", r.pedido.texto.replace('\n', " "), r.estado.longo.to_lowercase());
            let g = registro::texto_em_linhas(&pintor, &texto, FontId::proportional(px(14.0)), p.suave, largura - 16.0, 1);
            if g.elided {
                let caixa = Rect::from_min_size(pos2(area.left() + 16.0, y), vec2(g.size().x, px(22.0)));
                ui.interact(caixa, Id::new(("linha-pedido", slide.tarefa_id)), Sense::hover()).on_hover_text(&texto);
            }
            pintor.galley(pos2(area.left() + 16.0, meio - g.size().y / 2.0), g, p.suave);
            y += altura_pedido;
        }
        let area_nota = Rect::from_min_size(pos2(area.left(), y), vec2(largura, altura_nota));
        self.notas(ui, area_nota, slide, s, pelo_fim);
        y += altura_nota;
        if altura_nota > 0.0 {
            y += px(24.0);
        }

        // Números da tarefa (zeros escondidos).
        if tem_numeros {
            let numeros = Rect::from_min_size(pos2(area.left(), y), vec2(largura, altura_numeros));
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(numeros).layout(egui::Layout::left_to_right(egui::Align::Min)));
            filho.spacing_mut().item_spacing.x = px(40.0);
            if com_tempo && n.tempo_s > 0 {
                tema::metrica(&mut filho, &registro::texto_tempo(n.tempo_s), "Tempo de agente", None, px(28.0));
            }
            if n.sessoes > 0 {
                tema::metrica(&mut filho, &n.sessoes.to_string(), if n.sessoes == 1 { "Sessão" } else { "Sessões" }, None, px(28.0));
            }
            if n.erros > 0 {
                tema::metrica(&mut filho, &n.erros.to_string(), if n.erros == 1 { "Erro" } else { "Erros" }, Some((p.erro, tema::Marca::AnelGrosso)), px(28.0));
            }
            if n.capturas > 0 {
                tema::metrica(&mut filho, &n.capturas.to_string(), if n.capturas == 1 { "Captura" } else { "Capturas" }, None, px(28.0));
            }
            y += altura_numeros + vao_numeros;
        }

        // "O que foi feito", recolhido por padrão: o que o agente fez é apoio; a fala é a nota.
        if slide.feito.is_empty() || y + px(ALTURA_FEITO_RECOLHIDO) > area.bottom() {
            return;
        }
        let limite = area.bottom();
        let itens: usize = slide.feito.iter().map(|f| f.itens.len() + f.mais).sum();
        let rotulo = format!("O que foi feito · {itens} {}", if itens == 1 { "item" } else { "itens" });
        let g = pintor.layout_no_wrap(rotulo, FontId::proportional(13.5), p.suave);
        let caixa = Rect::from_min_size(pos2(area.left(), y), g.size() + vec2(16.0, 4.0));
        let resposta = ui.interact(caixa, Id::new(("feito", slide.tarefa_id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let cor = if resposta.hovered() { p.texto } else { p.suave };
        // A seta desenhada: a fonte não tem ▸/▾.
        let c = pos2(caixa.left() + 5.0, caixa.top() + g.size().y / 2.0);
        let pontas = if self.feito_aberto {
            vec![c + vec2(-4.0, -2.5), c + vec2(4.0, -2.5), c + vec2(0.0, 3.0)]
        } else {
            vec![c + vec2(-2.5, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.5, 4.0)]
        };
        pintor.add(egui::Shape::convex_polygon(pontas, cor, Stroke::NONE));
        pintor.galley(caixa.min + vec2(16.0, 0.0), g, cor);
        if resposta.clicked() {
            self.feito_aberto = !self.feito_aberto;
        }
        y += if self.feito_aberto { px(22.0) + px(12.0) } else { px(ALTURA_FEITO_RECOLHIDO) };
        let feito: &[api::Feito] = if self.feito_aberto { &slide.feito } else { &[] };
        let so_periodo = slide.feito.len() == 1 && slide.feito[0].parte == "No período";
        let linha = px(28.0);
        for f in feito {
            if y + linha > limite {
                break;
            }
            if !so_periodo {
                pintor.text(pos2(area.left(), y), egui::Align2::LEFT_TOP, &f.parte, forte(px(17.0)), p.texto);
                y += px(26.0);
            }
            let cabem = (((limite - y) / (linha + px(8.0))).floor().max(0.0) as usize).min(MAX_TOPICOS);
            let total = f.itens.len() + f.mais;
            let mostrar = if total > cabem { cabem.saturating_sub(1) } else { total.min(f.itens.len()) };
            for item in f.itens.iter().take(mostrar) {
                let meio = y + linha / 2.0;
                pintor.circle_filled(pos2(area.left() + px(14.0), meio), 3.0, p.suave);
                let g = registro::texto_em_linhas(&pintor, item, FontId::proportional(px(20.0)), p.texto, largura - px(28.0), 1);
                if g.elided {
                    let caixa = Rect::from_min_size(pos2(area.left() + px(28.0), y), vec2(g.size().x, linha));
                    ui.interact(caixa, Id::new(("topico", slide.tarefa_id, y as i32)), Sense::hover()).on_hover_text(item);
                }
                pintor.galley(pos2(area.left() + px(28.0), meio - g.size().y / 2.0), g, p.texto);
                y += linha + px(8.0);
            }
            if total > mostrar {
                let n = total - mostrar;
                let texto = if n == 1 { "+1 item".to_string() } else { format!("+{n} itens") };
                pintor.text(pos2(area.left() + px(28.0), y + linha / 2.0), egui::Align2::LEFT_CENTER, texto, FontId::proportional(15.0), p.suave);
                y += linha;
            }
            y += px(20.0) - px(8.0);
        }
    }

    /// Altura do bloco de notas: o campo em edição, a nota inteira (até
    /// `maximo`; o que passa rola) com o estado da gravação logo embaixo, ou
    /// o convite para escrever.
    fn altura_nota(&self, pintor: &egui::Painter, slide: &api::Slide, s: f32, largura: f32, maximo: f32) -> f32 {
        let editando = self.nota.as_ref().is_some_and(|(t, _)| *t == slide.tarefa_id) || self.focar_nota;
        let tem = nota_do_slide(slide).is_some();
        if editando {
            return maximo.min((260.0 * s).round()).max((132.0 * s).round().min(maximo)).max(110.0);
        }
        if !tem {
            return if self.tela_cheia { 0.0 } else { (44.0 * s).round() };
        }
        let (texto, prefixo) = match nota_do_slide(slide) {
            Some((t, true)) => (t, 20.0),
            Some((t, false)) => (t, 0.0),
            None => ("", 0.0),
        };
        let medida = registro::NotaMedida::nova(pintor, texto, estilo_nota((FONTE_NOTA_TAREFA * s).round(), cores().texto, largura - 15.0 - 12.0));
        (prefixo + medida.altura + RODAPE_NOTA).min(maximo)
    }

    fn notas(&mut self, ui: &mut egui::Ui, area: Rect, slide: &api::Slide, s: f32, pelo_fim: bool) {
        if area.height() <= 0.0 {
            // Em tela cheia sem nota, nada aparece; o N ainda abre o campo.
            if std::mem::take(&mut self.focar_nota) {
                self.comecar_nota(slide);
                self.focar_nota = true;
            }
            return;
        }
        let p = cores();
        let ctx = ui.ctx().clone();
        let tarefa = slide.tarefa_id;
        if std::mem::take(&mut self.focar_nota) && self.nota.as_ref().is_none_or(|(t, _)| *t != tarefa) {
            self.comecar_nota(slide);
            let id = Id::new(("nota-slide", tarefa));
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let editando = self.nota.as_ref().is_some_and(|(t, _)| *t == tarefa);
        let estado = self.estado_nota.get(&tarefa).cloned();
        let fonte_estado = FontId::proportional(12.5);
        if editando {
            let rodape = 20.0;
            let id = Id::new(("nota-slide", tarefa));
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(area.min, vec2(area.width(), area.height() - rodape))));
            let mut texto = self.nota.as_ref().map(|n| n.1.clone()).unwrap_or_default();
            let resposta = tema::campo_multilinha_com(
                &mut filho,
                &mut texto,
                3,
                area.height() - rodape - 20.0,
                id,
                false,
                FontId::proportional((FONTE_NOTA_TAREFA * s).round()),
            );
            if texto.chars().count() > MAX_NOTA {
                texto = texto.chars().take(MAX_NOTA).collect();
            }
            if let Some(n) = self.nota.as_mut() {
                n.1 = texto.clone();
            }
            if resposta.lost_focus() {
                self.salvar_nota(&ctx);
            }
            let n = texto.chars().count();
            let mut x = area.left();
            let y = area.bottom() - rodape + 4.0;
            if n >= 3600 {
                let g = ui.painter().layout_no_wrap(format!("{n} / {MAX_NOTA}"), fonte_estado.clone(), p.alerta);
                let w = g.size().x;
                ui.painter().galley(pos2(x, y), g, p.alerta);
                x += w + 12.0;
            }
            // A nota que não foi salva diz por quê, no lugar da dica.
            if let Some(EstadoNota::Erro(e)) = &estado {
                ui.painter().text(pos2(x, y), egui::Align2::LEFT_TOP, format!("Não salvei: {e}"), fonte_estado, p.erro);
            } else if self.nota_agente.contains(&tarefa) {
                ui.painter().text(pos2(x, y), egui::Align2::LEFT_TOP, "O agente acrescentou à nota; vai junto quando você salvar", fonte_estado, p.destaque);
            } else if !self.tela_cheia {
                // Sem lugar para tudo, sai o "Ctrl+Enter, " (o Esc já explica).
                let longa = "A nota aparece no slide para quem está vendo · Ctrl+Enter, Esc ou clique fora salva";
                let cabe = ui.painter().layout_no_wrap(longa.into(), fonte_estado.clone(), p.suave).size().x <= area.right() - x;
                let dica = if cabe { longa } else { "A nota aparece no slide para quem está vendo · Esc ou clique fora salva" };
                ui.painter().text(pos2(x, y), egui::Align2::LEFT_TOP, dica, fonte_estado, p.suave);
            }
            return;
        }
        let corpo = area;
        // Com o "Desfazer" de um anexo na tela, o clique não abre a nota.
        let sentido = if self.remocao.is_some() { Sense::hover() } else { Sense::click() };
        let resposta = ui.interact(corpo, Id::new(("nota-ler", tarefa)), sentido).on_hover_cursor(egui::CursorIcon::Text);
        let pintor = ui.painter().clone();
        // O estado da gravação fica logo abaixo da última linha, alinhado ao texto.
        let mut fim_texto = pos2(corpo.left(), corpo.top() + 44.0 * s);
        if nota_do_slide(slide).is_some() {
            let (texto, cor, prefixo) = match nota_do_slide(slide) {
                Some((t, true)) => (t, p.suave, slide.nota_anterior.as_ref().map(|a| format!("Nota de {}", a.periodo))),
                Some((t, false)) => (t, p.texto, None),
                None => ("", p.texto, None),
            };
            let mut y = corpo.top();
            if let Some(prefixo) = prefixo {
                pintor.text(pos2(corpo.left() + 15.0, y), egui::Align2::LEFT_TOP, prefixo, fonte_estado.clone(), p.suave);
                y += 20.0;
            }
            // A nota sempre inteira, em itens: o que não cabe no espaço livre
            // rola dentro dela. Respondido um pedido, começa no fim (a
            // resposta do agente vem embaixo); a nota de outro período, não.
            let largura_nota = corpo.width() - 15.0 - 12.0;
            let medida = registro::NotaMedida::nova(&pintor, texto, estilo_nota((FONTE_NOTA_TAREFA * s).round(), cor, largura_nota));
            let cabe = (corpo.bottom() - y - RODAPE_NOTA).max(FONTE_NOTA * 2.0);
            let altura = medida.altura.min(cabe);
            pintor.rect_filled(Rect::from_min_size(pos2(corpo.left(), y), vec2(3.0, altura)), CornerRadius::same(2), p.destaque);
            if medida.altura <= cabe {
                medida.pintar(&pintor, pos2(corpo.left() + 15.0, y));
            } else {
                let caixa = Rect::from_min_size(pos2(corpo.left() + 15.0, y), vec2(corpo.width() - 15.0, altura));
                let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa).layout(egui::Layout::top_down(egui::Align::Min)));
                egui::ScrollArea::vertical()
                    .id_salt(("nota-rolagem", tarefa))
                    .max_height(altura)
                    .auto_shrink([false, false])
                    .stick_to_bottom(pelo_fim && !slide.nota.is_empty())
                    .show(&mut filho, |ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(largura_nota, medida.altura), Sense::hover());
                        medida.pintar(ui.painter(), rect.min);
                    });
            }
            fim_texto = pos2(corpo.left() + 15.0, y + altura);
        } else if !self.tela_cheia {
            pintor.text(corpo.left_top(), egui::Align2::LEFT_TOP, "Escreva uma nota para esta tarefa (N)", FontId::proportional(15.0), p.suave);
            pintor.text(
                corpo.left_top() + vec2(0.0, 22.0),
                egui::Align2::LEFT_TOP,
                "A nota aparece no slide para quem está vendo",
                fonte_estado.clone(),
                p.suave,
            );
        }
        let pos_estado = fim_texto + vec2(0.0, 6.0);
        match estado {
            // "salvo" é o estado normal: na tela cheia, para quem assiste, não aparece.
            Some(EstadoNota::Salvo) if !self.tela_cheia => {
                pintor.text(pos_estado, egui::Align2::LEFT_TOP, "salvo", fonte_estado, p.suave);
            }
            Some(EstadoNota::Salvando) => {
                pintor.text(pos_estado, egui::Align2::LEFT_TOP, "salvando…", fonte_estado, p.suave);
            }
            Some(EstadoNota::Erro(e)) => {
                let g = pintor.layout_no_wrap(format!("Não salvei: {e} · "), fonte_estado.clone(), p.erro);
                let w = g.size().x;
                pintor.galley(pos_estado, g, p.erro);
                let g = pintor.layout_no_wrap("Tentar de novo".into(), fonte_estado, p.destaque);
                let link = Rect::from_min_size(pos_estado + vec2(w, 0.0), g.size());
                pintor.galley(link.min, g, p.destaque);
                if ui.interact(link, Id::new(("tentar-nota", tarefa)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.salvar_nota(&ctx);
                }
            }
            _ => {}
        }
        if resposta.clicked() {
            self.comecar_nota(slide);
            ctx.memory_mut(|m| m.request_focus(Id::new(("nota-slide", tarefa))));
        }
    }

    /// Coluna da direita: imagem (ou vídeo) principal, miniaturas embaixo e o × ao passar o mouse.
    fn coluna_direita(&mut self, ui: &mut egui::Ui, r: &Regioes, slide: &api::Slide, agora: f64) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let s = r.escala;
        let px = |v: f32| (v * s).round();
        let topo = if r.seletor_direita.height() > 0.0 { r.seletor_direita.bottom() + px(12.0) } else { r.direita.top() };
        let area = Rect::from_min_max(pos2(r.direita.left(), topo), r.direita.max);
        let tarefa = slide.tarefa_id;
        let n = slide.anexos.len();
        let indice = self.principal.get(&tarefa).copied().unwrap_or(0).min(n - 1);
        let lado = px(64.0);
        let principal = if n > 1 { Rect::from_min_max(area.min, pos2(area.right(), area.bottom() - lado - px(16.0))) } else { area };
        let anexo = slide.anexos[indice].clone();
        let resposta = if anexo.video() {
            // Cartão 16:9 no fundo da moldura das imagens, centrado: o play,
            // o nome, o tamanho e a dica dentro dele (o app não toca vídeo).
            let largura = principal.width().min(principal.height() * 16.0 / 9.0);
            let cartao = Rect::from_center_size(principal.center(), vec2(largura, largura * 9.0 / 16.0));
            let resposta = ui.interact(cartao, Id::new(("video-slide", anexo.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let pintor = ui.painter();
            let borda = if resposta.hovered() { p.destaque.gamma_multiply(0.55) } else { p.borda };
            pintor.rect(cartao, CornerRadius::same(tema::RAIO_SUPERFICIE), p.superficie, Stroke::new(1.0, borda), StrokeKind::Inside);
            let c = cartao.center() - vec2(0.0, px(40.0));
            pintor.circle_filled(c, px(28.0), p.destaque);
            tema::play(pintor, c, px(11.0), tema::sobre_destaque());
            let nome = if anexo.nome.is_empty() { "vídeo".to_string() } else { anexo.nome.clone() };
            let g = registro::texto_em_linhas(pintor, &nome, forte(17.0), p.texto, cartao.width() - 40.0, 1);
            let y = c.y + px(28.0) + 16.0;
            pintor.galley(pos2(cartao.center().x - g.size().x / 2.0, y), g, p.texto);
            let mut linha = registro::tamanho(anexo.bytes);
            let mut cor = p.suave;
            if let Some(e) = &self.erro_video {
                linha = e.clone();
                cor = p.erro;
            } else if self.abrindo_video.is_some_and(|t| agora - t < 3.0) {
                linha = "Abrindo no reprodutor…".into();
            }
            pintor.text(pos2(cartao.center().x, y + 26.0), egui::Align2::CENTER_TOP, linha, FontId::proportional(14.0), cor);
            pintor.text(pos2(cartao.center().x, y + 48.0), egui::Align2::CENTER_TOP, "Abre no reprodutor do sistema", FontId::proportional(14.0), p.suave);
            if resposta.clicked() {
                self.abrir_video(&ctx, anexo.id, agora);
            }
            resposta
        } else {
            let resposta = ui.interact(principal, Id::new(("imagem-slide", anexo.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().rect_filled(principal, CornerRadius::same(tema::RAIO_SUPERFICIE), p.superficie);
            match self.imagens.pedir(&ctx, anexo.id) {
                Miniatura::Pronta(t) => {
                    let tamanho = t.size_vec2();
                    let escala = (principal.width() / tamanho.x).min(principal.height() / tamanho.y);
                    let caixa = Rect::from_center_size(principal.center(), tamanho * escala);
                    egui::Image::new(t).corner_radius(CornerRadius::same(tema::RAIO_CARTAO)).paint_at(ui, caixa);
                }
                Miniatura::Falhou => {
                    ui.painter().text(principal.center(), egui::Align2::CENTER_CENTER, "imagem indisponível", FontId::proportional(14.0), p.suave);
                }
                Miniatura::Carregando => {}
            }
            if resposta.clicked() {
                self.abrir_visor(&ctx, tarefa, anexo.id);
            }
            resposta
        };
        // × para remover, só ao passar o mouse sobre a mídia.
        let alvo = resposta.rect;
        if ui.rect_contains_pointer(alvo) {
            let x = Rect::from_min_size(alvo.right_top() + vec2(-8.0 - 24.0, 8.0), vec2(24.0, 24.0));
            ui.painter().rect_filled(x, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie_alta);
            if tema::botao_icone_em(ui, x, Id::new(("remover-anexo-slide", anexo.id)), tema::Icone::Fechar).on_hover_text("Remover da tarefa").clicked() {
                self.remover_anexo(tarefa, indice, agora);
                return;
            }
        }
        if n > 1 {
            let mut x = area.left();
            let y = area.bottom() - lado;
            for (i, a) in slide.anexos.iter().enumerate() {
                if x + lado > area.right() + 0.5 {
                    break;
                }
                let caixa = Rect::from_min_size(pos2(x, y), vec2(lado, lado));
                let resposta = if a.video() {
                    registro_caixa_video(ui, caixa, Id::new(("mini-video", a.id)))
                } else {
                    let resposta = ui.interact(caixa, Id::new(("mini-slide", a.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    let raio = CornerRadius::same(tema::RAIO_CONTROLE);
                    match self.miniaturas.pedir(&ctx, a.id) {
                        Miniatura::Pronta(t) => {
                            let tamanho = t.size_vec2();
                            let escala = (caixa.width() / tamanho.x).max(caixa.height() / tamanho.y);
                            let visivel = vec2(caixa.width() / (tamanho.x * escala), caixa.height() / (tamanho.y * escala));
                            egui::Image::new(t).uv(Rect::from_center_size(pos2(0.5, 0.5), visivel)).corner_radius(raio).paint_at(ui, caixa);
                        }
                        _ => {
                            ui.painter().rect_filled(caixa, raio, p.realce);
                        }
                    }
                    resposta
                };
                let contorno = if i == indice { Stroke::new(2.0, p.destaque) } else { Stroke::new(1.0, p.borda) };
                let tipo = if i == indice { StrokeKind::Outside } else { StrokeKind::Inside };
                ui.painter().rect_stroke(caixa, CornerRadius::same(tema::RAIO_CONTROLE), contorno, tipo);
                if resposta.clicked() {
                    self.principal.insert(tarefa, i);
                    self.erro_video = None;
                }
                x += lado + 8.0;
            }
        }
    }

    /// Barra de baixo: navegação e pontos à esquerda; avisos, envio e o
    /// "Desfazer" na faixa do centro; anexar, abrir a tarefa, o lugar do
    /// "Pedir ao agente" e o × à direita.
    fn barra(&mut self, ui: &mut egui::Ui, r: &Regioes, agora: f64) -> Option<Pedido> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let s = r.escala;
        let margem = (64.0 * s).round();
        let mut pedido = None;
        let meio = r.acoes.center().y;
        let mut filho = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(r.acoes.left() + margem, meio - 16.0), pos2(r.avisos.left() - 16.0, meio + 16.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let total = self.paginas.len();
        if tema::botao_icone(&mut filho, tema::Icone::Anterior, 32.0).on_hover_text("Anterior (←)").clicked() {
            self.ir(navegar(self.atual, total, -1), &ctx);
        }
        if tema::botao_icone(&mut filho, tema::Icone::Proximo, 32.0).on_hover_text("Próximo (→)").clicked() {
            self.ir(navegar(self.atual, total, 1), &ctx);
        }
        filho.add_space(12.0);
        // Pontos: até 24, numa janela centrada no atual. As tarefas são
        // pontos (o atual, uma pílula) e batem com o "N / total" do
        // cabeçalho; a capa, os divisores e o fim são anéis, um pouco afastados.
        let inicio = self.atual.saturating_sub(12).min(total.saturating_sub(24));
        let mut ir = None;
        let mut anterior_tarefa = None;
        for i in inicio..(inicio + 24).min(total) {
            let tarefa = matches!(self.paginas[i], Pagina::Tarefa(_));
            if anterior_tarefa.is_some_and(|a| a != tarefa) {
                filho.add_space(6.0);
            }
            anterior_tarefa = Some(tarefa);
            let atual = i == self.atual;
            let largura = if atual && tarefa { 20.0 } else { 8.0 };
            let (rect, resposta) = filho.allocate_exact_size(vec2(largura, 12.0), Sense::click());
            let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
            let cor = if atual {
                p.destaque
            } else if resposta.hovered() {
                p.texto
            } else if tarefa {
                p.borda
            } else {
                p.suave
            };
            if tarefa {
                let ponto = Rect::from_center_size(rect.center(), vec2(largura, 8.0));
                filho.painter().rect_filled(ponto, CornerRadius::same(4), cor);
                // Slide com pedido aberto ao agente: contorno âmbar, parado.
                let tarefa_id = match (&self.paginas[i], &self.deck) {
                    (Pagina::Tarefa(s), Some(d)) => d.slides.get(*s).map_or(0, |s| s.tarefa_id),
                    _ => 0,
                };
                if self.pedidos.get(&tarefa_id).is_some_and(|r| r.estado.aberto) {
                    filho.painter().rect_stroke(ponto.expand(2.0), CornerRadius::same(6), Stroke::new(1.5, p.alerta), StrokeKind::Outside);
                }
            } else {
                filho.painter().circle_stroke(rect.center(), 3.5, Stroke::new(if atual { 2.0 } else { 1.5 }, cor));
            }
            if resposta.clicked() {
                ir = Some(i);
            }
            filho.add_space(2.0);
        }
        if let Some(i) = ir {
            self.ir(i, &ctx);
        }
        filho.add_space(16.0);
        let atalhos = filho.add(egui::Label::new(egui::RichText::new("? atalhos").color(p.suave).size(13.0)).sense(Sense::click()));
        if atalhos.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.atalhos = true;
        }
        let fim_navegacao = filho.min_rect().right();

        // Direita.
        let mut direita = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(r.avisos.left(), meio - 16.0), pos2(r.reserva_agente.left() - 12.0, meio + 16.0)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        if let Some(slide) = self.slide() {
            let tarefa = slide.tarefa_id;
            let removida = slide.removida;
            if !removida && tema::botao_secundario(&mut direita, "Abrir tarefa").clicked() {
                pedido = Some(Pedido::AbrirTarefa(tarefa));
            }
            direita.add_space(8.0);
            if tema::botao_secundario(&mut direita, "Adicionar foto ou vídeo (A)").clicked() {
                self.escolher_arquivos(&ctx);
            }
        }
        let inicio_botoes = if self.slide().is_some() { direita.min_rect().left() } else { r.reserva_agente.left() - 12.0 };
        if tema::botao_icone_em(ui, r.fechar, Id::new("fechar-apresentacao"), tema::Icone::Fechar).on_hover_text("Sair (Esc)").clicked() {
            pedido = Some(Pedido::Sair);
        }
        // "Pedir ao agente (P)" na reserva, só nos slides de tarefa: à esquerda,
        // com o mesmo vão de 16 dos outros botões do rodapé (a reserva começa
        // 12 depois deles).
        let mut chip = None;
        if self.slide().is_some_and(|s| !s.removida) {
            let mut reserva = ui.new_child(egui::UiBuilder::new().max_rect(r.reserva_agente).layout(egui::Layout::left_to_right(egui::Align::Center)));
            reserva.add_space(4.0);
            let aberta = self.caixa.is_some();
            let resposta = tema::chip_alternar(&mut reserva, "Pedir ao agente (P)", aberta);
            chip = Some(resposta.rect);
            if resposta.on_hover_text("Pedir algo a mais ao agente desta tarefa").clicked() {
                if aberta {
                    self.fechar_caixa();
                } else {
                    self.abrir_caixa(&ctx);
                }
            }
        }

        // Centro: a faixa entre a navegação e os botões, medida agora (a de
        // `regioes` é a reserva mínima com os tamanhos de referência).
        let faixa =
            Rect::from_min_max(pos2(fim_navegacao + 16.0, r.acoes.top() + 4.0), pos2((inicio_botoes - 16.0).max(fim_navegacao + 96.0), r.acoes.bottom() - 4.0));
        match self.faixa_de_avisos(ui, faixa, agora) {
            Some(AcaoFaixa::Desfazer) => self.desfazer_remocao(),
            Some(AcaoFaixa::Ver(tarefa)) => {
                self.respondido = None;
                self.ir_para_tarefa(tarefa, &ctx);
            }
            Some(AcaoFaixa::AbrirTarefa(tarefa)) => pedido = Some(Pedido::AbrirTarefa(tarefa)),
            Some(AcaoFaixa::Cancelar(id)) => {
                std::thread::spawn(move || {
                    if let Err(e) = api::cancelar_pedido(id) {
                        eprintln!("cancelando o pedido {id}: {e}");
                    }
                });
            }
            None => {}
        }
        // A caixa "Pedir ao agente", presa ao botão, para cima, sobre o slide (sem véu).
        if let Some(caixa) = &mut self.caixa {
            let tarefa = caixa.tarefa;
            let ancora = chip.map_or(r.reserva_agente.right_top(), |c| c.right_top()) - vec2(0.0, 8.0);
            match caixa.mostrar(&ctx, ancora, egui::Align2::RIGHT_BOTTOM, self.pedidos.get(&tarefa)) {
                None => {}
                Some(pedido::Saida::Fechar) => self.fechar_caixa(),
                Some(pedido::Saida::Enviado) => {
                    self.caixa = None;
                    self.rascunhos.remove(&tarefa);
                    self.aviso = Some(("Pedido enviado ao agente".into(), 0.0, Tom::Neutro));
                }
                Some(pedido::Saida::AbrirTarefa) => {
                    self.fechar_caixa();
                    pedido = Some(Pedido::AbrirTarefa(tarefa));
                }
            }
        }
        if self.aviso.as_ref().is_some_and(|(_, t, _)| *t == 0.0) {
            // O aviso some na próxima troca de slide ou em 6 s (um redesenho só).
            self.aviso.as_mut().expect("aviso").1 = agora + 6.0;
            ctx.request_repaint_after(std::time::Duration::from_secs(6));
        }
        if self.aviso.as_ref().is_some_and(|(_, t, _)| agora >= *t) {
            self.aviso = None;
        }
        pedido
    }

    /// A faixa do centro do rodapé: o envio em andamento, o "Desfazer" da
    /// remoção, o pedido ao agente (o deste slide, ou a resposta num outro),
    /// um aviso ou as novidades, nessa ordem. O texto quebra em até 2 linhas
    /// dentro da faixa. Devolve a ação clicada.
    fn faixa_de_avisos(&mut self, ui: &mut egui::Ui, faixa: Rect, agora: f64) -> Option<AcaoFaixa> {
        let p = cores();
        let tarefa_atual = self.tarefa_atual();
        let pedido_atual = tarefa_atual.and_then(|t| self.pedidos.get(&t)).filter(|r| r.estado.aberto || r.pedido.estado == "falhou");
        let (texto, cor, acao) = if let Some(pr) = &self.progresso {
            let porcento = pr.enviados * 100 / pr.total.max(1);
            let texto = if pr.arquivos > 1 {
                format!("Enviando {} de {}… {porcento}%", pr.arquivo, pr.arquivos)
            } else {
                format!("Enviando {}… {porcento}%", pr.nome)
            };
            (texto, p.ok, None)
        } else if self.enviando {
            ("Enviando…".to_string(), p.ok, None)
        } else if let Some(r) = &self.remocao {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((r.ate - agora).max(0.0)));
            (if r.anexo.video() { "Vídeo removido do slide" } else { "Imagem removida do slide" }.to_string(), p.suave, Some(AcaoFaixa::Desfazer))
        } else if let Some((tarefa, titulo)) = self.respondido.clone().filter(|(t, _)| Some(*t) != tarefa_atual) {
            (format!("O agente respondeu em «{titulo}»"), p.destaque, Some(AcaoFaixa::Ver(tarefa)))
        } else if let Some(r) = pedido_atual {
            let acao = match r.estado.acao {
                Some(pedido::AcaoPedido::Cancelar) => Some(AcaoFaixa::Cancelar(r.pedido.id)),
                Some(pedido::AcaoPedido::AbrirTarefa) => Some(AcaoFaixa::AbrirTarefa(r.pedido.tarefa_id)),
                _ => None,
            };
            (r.estado.longo.clone(), r.estado.cor, acao)
        } else if let Some((texto, _, tom)) = &self.aviso {
            let cor = match tom {
                Tom::Neutro => p.suave,
                Tom::Concluiu => p.destaque,
                Tom::Alerta => p.alerta,
            };
            (texto.clone(), cor, None)
        } else if self.novidades {
            ("Novidades · R atualiza".to_string(), p.destaque, None)
        } else {
            return None;
        };
        let pintor = ui.painter().clone();
        let fonte = FontId::proportional(13.0);
        let largura_botao = acao.map_or(0.0, |a| pintor.layout_no_wrap(a.rotulo().into(), fonte.clone(), p.texto).size().x + 28.0 + 10.0);
        let largura_texto = faixa.width() - 2.0 * 14.0 - 14.0 - largura_botao;
        let g = registro::texto_em_linhas(&pintor, &texto, fonte, p.texto, largura_texto, 2);
        let largura = (14.0 + 14.0 + g.size().x + 14.0 + largura_botao).min(faixa.width());
        let altura = (g.size().y + 12.0).max(if acao.is_some() { 36.0 } else { 28.0 }).min(faixa.height());
        let caixa = Rect::from_center_size(faixa.center(), vec2(largura, altura));
        pintor.rect(caixa, CornerRadius::same((altura / 2.0).min(14.0) as u8), tema::fundo_tingido(p, cor, tema::claro()), Stroke::NONE, StrokeKind::Inside);
        pintor.circle_filled(pos2(caixa.left() + 14.0 + 3.5, caixa.center().y), 3.5, cor);
        if g.elided {
            ui.interact(caixa, Id::new("faixa-aviso"), Sense::hover()).on_hover_text(&texto);
        }
        pintor.galley(pos2(caixa.left() + 28.0, caixa.center().y - g.size().y / 2.0), g, p.texto);
        let acao = acao?;
        let botao = Rect::from_min_size(pos2(caixa.right() - largura_botao + 4.0, caixa.center().y - 14.0), vec2(largura_botao - 10.0, 28.0));
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(botao).layout(egui::Layout::left_to_right(egui::Align::Center)));
        tema::botao_secundario(&mut filho, acao.rotulo()).clicked().then_some(acao)
    }

    /// Abre a caixa "Pedir ao agente" do slide atual (P ou o botão do rodapé).
    fn abrir_caixa(&mut self, ctx: &egui::Context) {
        let Some(slide) = self.slide().filter(|s| !s.removida) else { return };
        let tarefa = slide.tarefa_id;
        let Some(deck) = &self.deck else { return };
        let (tipo, periodo) = (deck.tipo.clone(), deck.chave_nota.clone());
        // A nota em edição é salva antes (o agente vai escrever nela).
        self.salvar_nota(ctx);
        let rascunho = self.rascunhos.remove(&tarefa).unwrap_or_default();
        self.caixa = Some(pedido::Caixa::nova(ctx, tarefa, &tipo, &periodo, rascunho));
    }

    /// Fecha a caixa guardando o rascunho da tarefa.
    fn fechar_caixa(&mut self) {
        if let Some(c) = self.caixa.take()
            && !c.texto.trim().is_empty()
        {
            self.rascunhos.insert(c.tarefa, c.texto);
        }
    }

    /// O que fica por cima: arrastar arquivos, o visor, o painel de atalhos e o "Desfazer".
    fn camadas(&mut self, ctx: &egui::Context, tela: Rect) {
        let p = cores();
        let arrastando = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if arrastando {
            let slide = self.slide().map(|s| s.titulo.clone());
            egui::Area::new(Id::new("soltar-arquivo")).order(egui::Order::Foreground).fixed_pos(tela.min).show(ctx, |ui| {
                let pintor = ui.painter();
                pintor.rect_filled(tela, 0, Color32::from_rgba_unmultiplied(p.fundo.r(), p.fundo.g(), p.fundo.b(), 230));
                let caixa = tela.shrink(24.0);
                let cor = if slide.is_some() { p.destaque } else { p.suave };
                tracejado(pintor, caixa, cor);
                let (titulo, detalhe) = match &slide {
                    Some(t) => (format!("Solte para anexar a «{t}»"), "png, jpg, mp4, webm, mkv ou mov"),
                    None => ("Abra o slide de uma tarefa para anexar".to_string(), ""),
                };
                pintor.text(caixa.center() - vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, titulo, forte(24.0), p.texto);
                pintor.text(caixa.center() + vec2(0.0, 22.0), egui::Align2::CENTER_CENTER, detalhe, FontId::proportional(15.0), p.suave);
            });
        }
        if let Some(v) = &self.visor {
            let mut fechar = false;
            egui::Area::new(Id::new("visor-slide")).order(egui::Order::Foreground).fixed_pos(tela.min).show(ctx, |ui| {
                let fundo = ui.allocate_rect(tela, Sense::click());
                ui.painter().rect_filled(tela, 0, Color32::from_black_alpha(if tema::claro() && !self.tela_cheia { 60 } else { 140 }));
                let area = tela.shrink(48.0);
                match &v.imagem {
                    Some(Ok(t)) => {
                        let tamanho = t.size_vec2();
                        let escala = (area.width() / tamanho.x).min(area.height() / tamanho.y).min(1.0);
                        let caixa = Rect::from_center_size(area.center(), tamanho * escala);
                        egui::Image::new(t).corner_radius(CornerRadius::same(tema::RAIO_CARTAO)).paint_at(ui, caixa);
                    }
                    Some(Err(_)) => {
                        ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, "imagem indisponível", FontId::proportional(16.0), Color32::WHITE);
                    }
                    None => {}
                }
                ui.painter().text(
                    tela.center_bottom() - vec2(0.0, 20.0),
                    egui::Align2::CENTER_BOTTOM,
                    "← → outras imagens · Esc fecha",
                    FontId::proportional(13.0),
                    Color32::from_gray(220),
                );
                fechar = fundo.clicked();
            });
            if fechar {
                self.visor = None;
            }
        }
        if let Some(c) = &self.conflito {
            let mut escolha = None;
            egui::Area::new(Id::new("veu-conflito")).order(egui::Order::Middle).fixed_pos(tela.min).show(ctx, |ui| {
                ui.allocate_rect(tela, Sense::click());
                ui.painter().rect_filled(tela, 0, Color32::from_black_alpha(if tema::claro() && !self.tela_cheia { 60 } else { 140 }));
            });
            egui::Area::new(Id::new("conflito-nota")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                tema::moldura_janela().show(ui, |ui| {
                    ui.set_width(760.0 - 48.0);
                    tema::cabecalho(ui, "A nota mudou enquanto você editava", "O agente reescreveu a nota. Escolha o que fica.");
                    ui.add_space(16.0);
                    // As duas colunas com a mesma altura (260): a curta não encolhe,
                    // a longa rola. As linhas pedidas enchem os 260.
                    let linha = ui.painter().layout_no_wrap("Ág".into(), FontId::proportional(13.5), p.texto).size().y.max(1.0);
                    let linhas = (260.0 / linha).floor() as usize;
                    ui.columns(2, |colunas| {
                        for (n, (rotulo, texto)) in [("Sua versão", &c.minha), ("Versão do agente", &c.do_agente)].into_iter().enumerate() {
                            let col = &mut colunas[n];
                            col.label(egui::RichText::new(rotulo).color(p.suave).size(12.5));
                            let mut copia = texto.clone();
                            tema::campo_multilinha(col, &mut copia, linhas, 260.0, Id::new(("conflito", n)), true);
                        }
                    });
                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tema::botao_principal(ui, "Juntar as duas", true).clicked() {
                            escolha = Some(format!("{}\n\n{}", c.minha.trim_end(), c.do_agente.trim_start()));
                        }
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Ficar com a do agente").clicked() {
                            escolha = Some(c.do_agente.clone());
                        }
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Ficar com a minha").clicked() {
                            escolha = Some(c.minha.clone());
                        }
                    });
                });
            });
            if let Some(texto) = escolha
                && let Some(c) = self.conflito.take()
            {
                self.nota = None;
                if let Some(s) = self.deck.as_mut().and_then(|d| d.slides.iter_mut().find(|s| s.tarefa_id == c.tarefa)) {
                    s.nota = c.do_agente.clone();
                    s.nota_versao = c.versao.clone();
                }
                self.base_nota = Some((c.tarefa, c.do_agente.clone(), c.versao.clone()));
                if texto != c.do_agente
                    && let Some(d) = &self.deck
                {
                    let (tipo, periodo) = (d.tipo.clone(), d.chave_nota.clone());
                    self.gravar_nota(ctx, c.tarefa, tipo, periodo, texto, c.versao);
                }
            }
        }
        if self.atalhos {
            let mut fechar = false;
            // Véu atrás do painel, como o do visor: o slide de baixo não aparece pelos lados.
            egui::Area::new(Id::new("veu-atalhos")).order(egui::Order::Middle).fixed_pos(tela.min).show(ctx, |ui| {
                let fundo = ui.allocate_rect(tela, Sense::click());
                ui.painter().rect_filled(tela, 0, Color32::from_black_alpha(if tema::claro() && !self.tela_cheia { 60 } else { 140 }));
                fechar |= fundo.clicked();
            });
            egui::Area::new(Id::new("atalhos-apresentacao")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
                ctx,
                |ui| {
                    tema::moldura_janela().show(ui, |ui| {
                        ui.set_width(472.0);
                        tema::cabecalho(ui, "Atalhos", "");
                        ui.add_space(10.0);
                        for (acao, teclas) in ATALHOS {
                            ui.horizontal(|ui| {
                                ui.set_height(30.0);
                                ui.label(egui::RichText::new(*acao).color(p.texto).size(14.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for t in teclas.iter().rev() {
                                        tema::tecla(ui, t);
                                    }
                                });
                            });
                        }
                        ui.add_space(10.0);
                        fechar |= tema::botao_secundario(ui, "Fechar").clicked();
                    });
                },
            );
            if fechar {
                self.atalhos = false;
            }
        }
    }
}

/// Os atalhos do painel "?".
const ATALHOS: &[(&str, &[&str])] = &[
    ("Avançar", &["→", "PgDn", "Espaço"]),
    ("Voltar", &["←", "PgUp", "Backspace"]),
    ("Abrir a tarefa do bloco / voltar ao bloco", &["Enter", "Esc"]),
    ("Primeiro / último", &["Home", "End"]),
    ("Ir à capa", &["C"]),
    ("Pedir ao agente", &["P"]),
    ("Anexos ou lousa", &["L"]),
    ("Lousa da reunião, cartão por cartão", &["Q"]),
    ("Editar a nota (no slide da tarefa)", &["N"]),
    ("Adicionar foto ou vídeo", &["A"]),
    ("Esconder a tarefa (na daily, tira desta daily)", &["H"]),
    ("Tema claro ou escuro (só agora)", &["T"]),
    ("Atualizar com as novidades", &["R"]),
    ("Tela cheia", &["F11"]),
    ("Atalhos", &["?"]),
    ("Sair", &["Esc"]),
];

/// Borda tracejada (traço 8, vão 6) num retângulo.
fn tracejado(pintor: &egui::Painter, r: Rect, cor: Color32) {
    let cantos = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    let caminho: Vec<Pos2> = cantos.to_vec();
    for forma in egui::Shape::dashed_line(&caminho, Stroke::new(2.0, cor), 8.0, 6.0) {
        pintor.add(forma);
    }
}

fn registro_caixa_video(ui: &mut egui::Ui, caixa: Rect, id: Id) -> egui::Response {
    let p = cores();
    let resposta = ui.interact(caixa, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().rect_filled(caixa, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie);
    ui.painter().circle_filled(caixa.center(), 12.0, p.destaque);
    tema::play(ui.painter(), caixa.center(), 5.0, tema::sobre_destaque());
    resposta
}

#[cfg(test)]
mod testes {
    use super::*;

    fn sem_sobreposicao(r: &Regioes, tela: Vec2, com_anexos: bool) {
        let mut regioes = vec![("cabecalho", r.cabecalho), ("titulo", r.titulo), ("esquerda", r.esquerda), ("acoes", r.acoes)];
        if com_anexos {
            regioes.push(("direita", r.direita));
        }
        // O seletor "Anexos | Lousa" fica dentro da coluna da direita, no topo dela.
        if r.seletor_direita.height() > 0.0 {
            assert!(r.direita.contains_rect(r.seletor_direita), "seletor fora da coluna em {tela:?}");
            assert!(!r.seletor_direita.intersects(r.esquerda) && r.seletor_direita.top() == r.direita.top());
            assert!(r.seletor_direita.height() == 34.0 && r.direita.height() - 34.0 - 12.0 >= 200.0, "sobra pouco abaixo do seletor em {tela:?}");
        }
        let janela = Rect::from_min_size(Pos2::ZERO, tela);
        for (i, (a, ra)) in regioes.iter().enumerate() {
            assert!(janela.contains_rect(*ra), "{a} fora da janela {tela:?}: {ra:?}");
            for (b, rb) in &regioes[i + 1..] {
                assert!(!ra.intersects(*rb), "{a} e {b} se sobrepõem em {tela:?}: {ra:?} {rb:?}");
            }
        }
        assert!(janela.contains_rect(r.fechar) && r.acoes.contains_rect(r.fechar));
        assert!(r.acoes.contains_rect(r.reserva_agente) && !r.reserva_agente.intersects(r.fechar));
        // A faixa dos avisos fica no rodapé, entre a navegação e os botões, com lugar para texto.
        assert!(
            r.acoes.contains_rect(r.avisos) && !r.avisos.intersects(r.reserva_agente) && !r.avisos.intersects(r.fechar),
            "avisos em {tela:?}: {:?}",
            r.avisos
        );
        assert!(r.avisos.width() >= 80.0 && r.avisos.left() >= r.esquerda.left() + LARGURA_NAVEGACAO);
        // O botão "Pedir ao agente (P)", medido de verdade, cabe na reserva.
        let ctx = egui::Context::default();
        tema::instalar(&ctx);
        let mut largura = 0.0;
        let mut saida = ctx.run_ui(egui::RawInput::default(), |ui| {
            largura = ui.painter().layout_no_wrap("Pedir ao agente (P)".into(), forte(13.0), Color32::WHITE).size().x + 28.0;
        });
        saida.textures_delta.clear();
        assert!(largura <= r.reserva_agente.width(), "botão de {largura} numa reserva de {}", r.reserva_agente.width());
    }

    #[test]
    fn nota_junta_o_que_o_agente_acrescentou() {
        // O agente só acrescentou no fim: a sua edição fica e o acréscimo vai junto.
        let base = "Tela de pedidos quase pronta.";
        let atual = "Tela de pedidos quase pronta.\n\nTotal de testes: 42";
        assert_eq!(
            juntar_nota(base, "Tela de pedidos pronta, falta o filtro.", atual).as_deref(),
            Some("Tela de pedidos pronta, falta o filtro.\n\nTotal de testes: 42")
        );
        // Não havia nota: a sua e a do agente, nessa ordem.
        assert_eq!(juntar_nota("", "Minha", "Do agente").as_deref(), Some("Minha\n\nDo agente"));
        // O agente reescreveu: não dá para juntar sozinho.
        assert_eq!(juntar_nota(base, "Minha", "Outra coisa"), None);
    }

    #[test]
    fn regioes_em_1600x900_e_1280x720() {
        for tela in [vec2(1600.0, 900.0), vec2(1280.0, 720.0), vec2(1920.0, 1080.0), vec2(1024.0, 640.0)] {
            for (com_anexos, com_abas) in [(true, false), (false, false), (true, true)] {
                let r = regioes(tela, com_anexos, com_abas);
                sem_sobreposicao(&r, tela, com_anexos);
                assert_eq!(r.seletor_direita.height() > 0.0, com_abas, "seletor em {tela:?}");
                if com_anexos {
                    let proporcao = r.esquerda.width() / (r.esquerda.width() + r.direita.width());
                    assert!((0.55..=0.62).contains(&proporcao), "proporção {proporcao} em {tela:?}");
                } else {
                    assert_eq!(r.esquerda, r.conteudo);
                }
            }
        }
        let r = regioes(vec2(1600.0, 900.0), true, false);
        assert_eq!((r.escala, r.cabecalho.left(), r.cabecalho.top()), (1.0, 64.0, 40.0));
        assert_eq!((r.esquerda.width(), r.direita.left(), r.direita.width()), (832.0, 944.0, 592.0));
        assert_eq!(r.acoes, Rect::from_min_size(pos2(0.0, 836.0), vec2(1600.0, 64.0)));
        assert_eq!(r.fechar, Rect::from_min_size(pos2(1504.0, 852.0), vec2(32.0, 32.0)));
        // Em 1280×720 o título ainda cabe em duas linhas de 34 px.
        let pequeno = regioes(vec2(1280.0, 720.0), true, false);
        assert!((pequeno.escala - 0.8).abs() < 1e-6 && pequeno.titulo.height() >= 84.0);
    }

    fn deck() -> api::Deck {
        serde_json::from_value(serde_json::json!({
            "tipo": "sprint", "titulo": "Sprint", "mais": 2,
            "slides": [
                {"tarefa_id": 1, "titulo": "A", "secao": "loja-web", "secao_id": 1, "projeto": "loja-web"},
                {"tarefa_id": 2, "titulo": "B", "secao": "loja-web", "secao_id": 1, "projeto": "loja-web"},
                {"tarefa_id": 3, "titulo": "C", "secao": "cliente-x", "secao_id": 2, "projeto": "cliente-x"}
            ]
        }))
        .unwrap()
    }

    fn bloco(projeto: &str, workspace: &str, tarefas: &[usize], parte: usize, partes: usize) -> Pagina {
        Pagina::Bloco { secao_id: 0, projeto: projeto.into(), workspace: workspace.into(), tarefas: tarefas.to_vec(), parte, partes }
    }

    /// Compara as páginas ignorando o id da seção.
    fn iguais(a: &[Pagina], b: &[Pagina]) -> bool {
        let sem_id = |p: &Pagina| match p {
            Pagina::Bloco { projeto, workspace, tarefas, parte, partes, .. } => bloco(projeto, workspace, tarefas, *parte, *partes),
            outra => outra.clone(),
        };
        a.iter().map(sem_id).eq(b.iter().map(sem_id))
    }

    #[test]
    fn paginas_do_deck_em_blocos() {
        let d = deck();
        let tudo = |_: &api::Slide| 0.0;
        let paginas = montar_paginas(&d, &HashSet::new(), &tudo, f32::INFINITY);
        assert!(
            iguais(&paginas, &[Pagina::Capa, bloco("loja-web", "", &[0, 1], 1, 1), bloco("cliente-x", "", &[2], 1, 1), Pagina::Mais, Pagina::Fim]),
            "{paginas:?}"
        );
        // Esconder (H) tira a tarefa do bloco; um bloco sem tarefas some.
        let escondidos: HashSet<i64> = [3].into();
        let paginas = montar_paginas(&d, &escondidos, &tudo, f32::INFINITY);
        assert!(iguais(&paginas, &[Pagina::Capa, bloco("loja-web", "", &[0, 1], 1, 1), Pagina::Mais, Pagina::Fim]), "{paginas:?}");
        // Um projeto só ainda é um bloco: é a daily do projeto.
        let mut daily = d.clone();
        daily.tipo = "daily".into();
        daily.mais = 0;
        daily.slides.pop();
        let paginas = montar_paginas(&daily, &HashSet::new(), &tudo, f32::INFINITY);
        assert!(iguais(&paginas, &[Pagina::Capa, bloco("loja-web", "", &[0, 1], 1, 1), Pagina::Fim]), "{paginas:?}");
        // Dois loja-web de workspaces diferentes são blocos diferentes.
        let mut dois: api::Deck = serde_json::from_value(serde_json::json!({
            "tipo": "daily", "titulo": "Daily",
            "slides": [
                {"tarefa_id": 1, "titulo": "A", "secao": "estudos · loja-web", "secao_id": 1, "projeto": "loja-web", "workspace": "estudos"},
                {"tarefa_id": 2, "titulo": "B", "secao": "trabalho-x · loja-web", "secao_id": 3, "projeto": "loja-web", "workspace": "trabalho-x"}
            ]
        }))
        .unwrap();
        let paginas = montar_paginas(&dois, &HashSet::new(), &tudo, f32::INFINITY);
        assert!(
            iguais(&paginas, &[Pagina::Capa, bloco("loja-web", "estudos", &[0], 1, 1), bloco("loja-web", "trabalho-x", &[1], 1, 1), Pagina::Fim]),
            "{paginas:?}"
        );
        dois.slides[1].secao_id = 1;
        dois.slides[1].secao = "estudos · loja-web".into();
        assert_eq!(montar_paginas(&dois, &HashSet::new(), &tudo, f32::INFINITY).len(), 3);
    }

    #[test]
    fn tarefa_com_anexo_ganha_slide_depois_do_bloco() {
        let mut d = deck();
        d.slides[1].anexos = vec![serde_json::from_value(serde_json::json!({"id": 9, "tipo": "imagem", "bytes": 1, "largura": 1, "altura": 1})).unwrap()];
        d.slides[2].lousa = Some(api::ResumoLousa { id: 1, elementos: 2 });
        let paginas = montar_paginas(&d, &HashSet::new(), &|_| 0.0, f32::INFINITY);
        assert!(
            iguais(
                &paginas,
                &[
                    Pagina::Capa,
                    bloco("loja-web", "", &[0, 1], 1, 1),
                    Pagina::Tarefa(1),
                    bloco("cliente-x", "", &[2], 1, 1),
                    Pagina::Tarefa(2),
                    Pagina::Mais,
                    Pagina::Fim
                ]
            ),
            "{paginas:?}"
        );
        // A tarefa com slide próprio abre nele; a outra, no bloco dela.
        assert_eq!(pagina_da_tarefa(&paginas, &d, 2), Some(2));
        assert_eq!(pagina_da_tarefa(&paginas, &d, 1), Some(1));
        assert_eq!(pagina_da_tarefa(&paginas, &d, 3), Some(4));
        assert_eq!(pagina_da_tarefa(&paginas, &d, 99), None);
    }

    #[test]
    fn bloco_que_nao_cabe_segue_em_partes_sem_cortar_tarefa() {
        let d = deck();
        // Cada tarefa mede 60; cabem 100 por tela: a segunda do loja-web vai para a parte 2.
        let paginas = montar_paginas(&d, &HashSet::new(), &|_| 60.0, 100.0);
        assert!(
            iguais(
                &paginas,
                &[
                    Pagina::Capa,
                    bloco("loja-web", "", &[0], 1, 2),
                    bloco("loja-web", "", &[1], 2, 2),
                    bloco("cliente-x", "", &[2], 1, 1),
                    Pagina::Mais,
                    Pagina::Fim
                ]
            ),
            "{paginas:?}"
        );
        // Uma tarefa maior que a tela fica sozinha na parte dela, nunca cortada.
        let paginas = montar_paginas(&d, &HashSet::new(), &|s| if s.tarefa_id == 1 { 500.0 } else { 10.0 }, 100.0);
        assert!(
            iguais(
                &paginas,
                &[
                    Pagina::Capa,
                    bloco("loja-web", "", &[0], 1, 2),
                    bloco("loja-web", "", &[1], 2, 2),
                    bloco("cliente-x", "", &[2], 1, 1),
                    Pagina::Mais,
                    Pagina::Fim
                ]
            ),
            "{paginas:?}"
        );
    }

    #[test]
    fn navegacao_respeita_os_limites() {
        assert_eq!(navegar(0, 5, -1), 0);
        assert_eq!(navegar(0, 5, 1), 1);
        assert_eq!(navegar(4, 5, 1), 4);
        assert_eq!(navegar(2, 5, 10), 4);
        assert_eq!(navegar(0, 0, 1), 0);
    }

    #[test]
    fn comeca_no_slide_pedido_e_mantem_a_tarefa_ao_atualizar() {
        let ctx = egui::Context::default();
        let mut a = Apresentacao::nova(
            &ctx,
            1,
            api::Recorte::Perfil,
            Some(api::PeriodoSprint::Ultimos(7)),
            Some(deck()),
            Inicio::Tarefa(3),
            false,
            tema::Escolha::Escuro,
            false,
        );
        assert_eq!(a.tarefa_atual(), Some(3));
        // R com a tarefa 3 fora do deck: fica no vizinho e avisa.
        let mut sem_c = deck();
        sem_c.slides.pop();
        a.trocar_deck(sem_c, Some(3));
        assert!(a.atual < a.paginas.len());
        assert!(a.aviso.as_ref().is_some_and(|(t, _, _)| t.contains("“C” saiu da sprint")));
        a.trocar_deck(deck(), Some(2));
        assert_eq!(a.tarefa_atual(), Some(2));
        let capa = Apresentacao::nova(&ctx, 1, api::Recorte::Perfil, None, Some(deck()), Inicio::Capa, true, tema::Escolha::Escuro, false);
        assert_eq!(capa.atual, 0);
    }

    #[test]
    fn desligar_o_tempo_tira_as_frases_do_deck_na_hora() {
        let ctx = egui::Context::default();
        let mut com_tempo = deck();
        com_tempo.tempo_agentes = true;
        com_tempo.capa.destaques = vec!["Agentes trabalharam 1 min".into()];
        let mut a = Apresentacao::nova(
            &ctx,
            1,
            api::Recorte::Perfil,
            Some(api::PeriodoSprint::Ultimos(7)),
            Some(com_tempo.clone()),
            Inicio::Tarefa(2),
            false,
            tema::Escolha::Escuro,
            true,
        );
        assert!(a.com_tempo() && a.tarefa_atual() == Some(2));
        // Desligada com o deck aberto: o deck antigo sai da memória na hora.
        a.definir_tempo(&ctx, false);
        assert!(a.deck.is_none() && a.tirando_tempo && !a.com_tempo());
        assert!(matches!(a.inicio, Inicio::Tarefa(2)));
        // Uma resposta com o tempo (pedida antes) é descartada ao chegar.
        assert!(a.buscando);
        a.canal.0.send(Mensagem::Deck { resultado: Ok(Box::new(com_tempo.clone())), manter: Some(2) }).unwrap();
        a.receber(&ctx);
        assert!(a.deck.is_none(), "o deck com o tempo entrou na tela");
        // A resposta sem o tempo entra e volta ao mesmo slide.
        let mut sem = deck();
        sem.tempo_agentes = false;
        a.canal.0.send(Mensagem::Deck { resultado: Ok(Box::new(sem)), manter: Some(2) }).unwrap();
        a.receber(&ctx);
        assert!(a.deck.as_ref().is_some_and(|d| !d.tempo_agentes) && !a.tirando_tempo);
        assert_eq!(a.tarefa_atual(), Some(2));
        // Aberta já com a opção desligada, um deck com o tempo não entra.
        let b = Apresentacao::nova(&ctx, 1, api::Recorte::Perfil, None, Some(com_tempo), Inicio::Capa, false, tema::Escolha::Escuro, false);
        assert!(b.deck.is_none());
    }

    #[test]
    fn capa_nunca_junta_um_projeto_sob_o_rotulo_de_outro() {
        assert_eq!(celulas_capa(2), 2);
        assert_eq!(celulas_capa(4), 4);
        assert_eq!(celulas_capa(6), 6);
        assert_eq!(celulas_capa(9), 6);
        assert_eq!(rotulo_parte("estudos · cliente-x", 1), "estudos › cliente-x · 1");
        assert_eq!(rotulo_parte("loja-web", 3), "loja-web · 3");
    }

    #[test]
    fn h_esconde_a_tarefa_sob_o_mouse() {
        let ctx = egui::Context::default();
        tema::instalar(&ctx);
        let mut d = deck();
        d.slides[0].nota = "Primeira nota\n- item um\n- item dois".into();
        d.slides[1].nota = "Segunda nota".into();
        let mut a = Apresentacao::nova(&ctx, 1, api::Recorte::Perfil, None, Some(d), Inicio::Capa, true, tema::Escolha::Escuro, false);
        a.atual = 1;
        let modelo = dados::Modelo::default();
        let mut favo = tema::Favo::default();
        let tela = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));
        let mut quadro = |a: &mut Apresentacao, eventos: Vec<Event>, t: f64| {
            let entrada = egui::RawInput { screen_rect: Some(tela), events: eventos, time: Some(t), ..Default::default() };
            let mut s = ctx.run_ui(entrada, |ui| {
                a.mostrar(ui, &mut favo, &modelo, t);
            });
            s.textures_delta.clear();
        };
        // Acha a linha da tarefa 1 (A) movendo o mouse.
        let mut t = 0.0;
        let mut achou = None;
        for y in (100..800).step_by(10) {
            t += 0.1;
            quadro(&mut a, vec![Event::PointerMoved(pos2(400.0, y as f32))], t);
            t += 0.1;
            quadro(&mut a, vec![], t);
            if a.sob_mouse == Some(1) {
                achou = Some(y);
                break;
            }
        }
        assert!(achou.is_some());
        let tecla = |pressed| Event::Key { key: Key::H, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        t += 0.1;
        quadro(&mut a, vec![tecla(true), Event::Text("h".into())], t);
        // Segurando o H, a repetição não leva a tarefa que subiu (B).
        for _ in 0..4 {
            t += 0.05;
            quadro(&mut a, vec![tecla(true)], t);
        }
        t += 0.05;
        quadro(&mut a, vec![tecla(false)], t);
        for _ in 0..4 {
            t += 0.1;
            quadro(&mut a, vec![], t);
        }
        assert_eq!(a.escondidos, [1].into());
        assert!(a.paginas.iter().any(|p| matches!(p, Pagina::Bloco { tarefas, .. } if tarefas == &[1])));
    }
}
