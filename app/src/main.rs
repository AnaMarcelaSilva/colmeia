// No Windows a tela é um programa de janela: sem console aberto atrás dela.
#![cfg_attr(windows, windows_subsystem = "windows")]
//! Tela da Colmeia: entrada por perfil, quadro de tarefas por projeto, painel
//! da tarefa (só o terminal em foco é tempo real, com caixa de mensagem para os
//! agentes), linha do tempo com daily e sprint, e a abelha da barra lateral,
//! que resume o que mais precisa de você.
//! Fala com o núcleo em Go pelo canal local (socket Unix + token); o que muda
//! chega pelo WebSocket de eventos, sem consulta periódica.

mod abelha;
mod api;
mod apresentacao;
mod banco;
mod canal;
mod compositor;
mod dados;
mod dialogos;
mod entrada;
mod eventos;
mod gaveta;
mod lousa;
mod pedido;
mod quadro;
mod registro;
mod sistema;
mod teclas;
mod tema;
mod terminal;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use abelha::Abelha;
use apresentacao::{Apresentacao, Inicio};
use compositor::Compositor;
use dados::{AgenteTela, Coluna, Efeito, Projeto};
use dialogos::{Dialogo, Resultado};
use eframe::egui::{self, Color32, CornerRadius, Key, Modifiers, RichText, Stroke};
use eventos::{Mensagem, Ouvinte};
use tema::{EstadoVisual, TipoAviso, cores, texto_forte};
use terminal::{MINIATURA, SO_CARTAO, TEMPO_REAL, TerminalAgente, pedir_carga};

/// O que está na tela: todos os projetos do perfil, os de um workspace ou
/// um projeto. Vale para o quadro e para o registro (linha do tempo, daily e
/// sprint), e a barra lateral destaca o mesmo. A abelha resume o escopo.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Escopo {
    Perfil,
    Workspace(i64),
    Projeto(i64),
}

impl Escopo {
    /// O projeto está no escopo (o workspace de cada projeto vem de `projetos`, sem consulta).
    fn contem(self, projeto: i64, projetos: &[Projeto]) -> bool {
        match self {
            Escopo::Perfil => true,
            Escopo::Workspace(w) => projetos.iter().any(|p| p.id == projeto && p.workspace_id == w),
            Escopo::Projeto(p) => p == projeto,
        }
    }

    fn projeto(self) -> Option<i64> {
        match self {
            Escopo::Projeto(p) => Some(p),
            _ => None,
        }
    }

    fn recorte(self) -> api::Recorte {
        match self {
            Escopo::Perfil => api::Recorte::Perfil,
            Escopo::Workspace(w) => api::Recorte::Workspace(w),
            Escopo::Projeto(p) => api::Recorte::Projeto(p),
        }
    }

    fn do_recorte(r: api::Recorte) -> Escopo {
        match r {
            api::Recorte::Perfil => Escopo::Perfil,
            api::Recorte::Workspace(w) => Escopo::Workspace(w),
            api::Recorte::Projeto(p) => Escopo::Projeto(p),
        }
    }
}

enum Tela {
    Entrada(Box<entrada::Entrada>),
    Quadro,
    /// Linha do tempo, daily ou sprint.
    Registro(registro::Aba),
    /// `foco` é o agente em tempo real (0 quando a tarefa não tem agentes).
    Tarefa {
        id: i64,
        foco: i64,
    },
    /// A lousa (quadro livre) do workspace.
    Lousa(i64),
    /// Os bancos de dados do perfil.
    Banco,
}

/// Como está a ligação com o núcleo.
#[derive(Clone, Copy, PartialEq)]
enum Conexao {
    Conectando,
    Ligado,
    Fora,
    /// O núcleo em execução é de uma versão sem eventos.
    Antigo,
}

/// O que o botão de um aviso do rodapé faz.
#[derive(Clone, Copy)]
enum AcaoAviso {
    DesfazerCaptura(i64),
    Abrir {
        tarefa: i64,
        agente: i64,
    },
    VerAbelha,
    /// O agente respondeu: ver o cartão da tarefa na Daily.
    VerPedido(i64),
    /// Os avisos da lousa: desfazer o que foi apagado, ver o que o agente
    /// acrescentou e tentar gravar de novo.
    Lousa(api::DonoLousa, AcaoLousa),
}

#[derive(Clone, Copy)]
enum AcaoLousa {
    /// Desfazer o comando de número dado (a remoção que o aviso anunciou).
    Desfazer(u64),
    Ver,
    Tentar,
}

/// Aviso passageiro no rodapé. Um por vez: o mais novo substitui o anterior.
/// A resposta do núcleo a um pedido de consulta: (id do pedido, resultado).
type RespostaPedido = (String, Result<(), String>);

struct Aviso {
    tipo: TipoAviso,
    texto: String,
    acao: Option<(&'static str, AcaoAviso)>,
    ate: f64,
}

/// Captura de um terminal: agendada para um quadro depois (para o menu ou o
/// diálogo que pediu não sair na imagem), depois pedida à janela.
enum Captura {
    Agendada { agente: i64, tarefa: i64, quadros: u8 },
    Pedida { agente: i64, tarefa: i64, area: egui::Rect, ppp: f32 },
}

/// Resultado do envio de uma captura ao núcleo (feito numa thread).
struct CapturaEnviada {
    tarefa: i64,
    resultado: Result<api::Anexo, String>,
}

/// A peça presa ao botão "Navegador": o endereço a abrir ou o aviso de que
/// não há Chrome instalado.
enum PopoverNavegador {
    Endereco { tarefa: i64, botao: egui::Rect, url: String, erro: Option<String>, focar: bool, quadros: u32 },
    SemChrome { botao: egui::Rect, quadros: u32 },
}

/// Respostas do núcleo para o navegador (pedidas numa thread).
enum RespostaNavegador {
    Aberto { tarefa: i64, url: String, resultado: Result<(), String> },
    Capturado { tarefa: i64, resultado: Result<i64, String> },
    Fechado(Result<(), String>),
}

/// Largura mínima do título no cabeçalho do painel da tarefa.
const TITULO_MINIMO: f32 = 160.0;

/// Altura que a abelha e o seletor de tema ocupam no pé da barra lateral.
const RESERVA_ABELHA: f32 = 250.0;

/// Avisos de "precisa de você" que chegam juntos viram um só.
const JUNTAR_AVISOS: f64 = 10.0;
/// Duração do clarão na borda do terminal capturado.
const CLARAO: f64 = 0.3;

struct Colmeia {
    /// O núcleo está em modo demonstração (dados de exemplo, cargas de teste e cenários simulados).
    demo: bool,
    /// Problema com o núcleo ao abrir (não iniciou, por exemplo), mostrado na faixa do topo.
    problema_nucleo: Option<String>,
    aviso: Option<Aviso>,
    perfil: Option<api::Perfil>,
    perfis: Vec<api::Perfil>,
    /// O nome do workspace em foco, para o aviso se ele for removido.
    nome_workspace: String,
    /// A opção do tempo dos agentes mandada ao núcleo e ainda sem resposta
    /// (o valor anterior, para desfazer se der erro).
    tempo_pendente: Option<(bool, Receiver<Result<(), String>>)>,
    modelo: dados::Modelo,
    branches: Vec<String>,
    /// Terminais dos agentes, pelo id do agente (na demonstração, o número do terminal de teste).
    terminais: HashMap<i64, TerminalAgente>,
    /// Ferramentas instaladas e contas do perfil, lidas quando precisa.
    ferramentas: Option<Vec<api::Ferramenta>>,
    contas: Option<Vec<api::Conta>>,
    /// Editor para abrir a pasta da tarefa (IntelliJ ou VS Code), se houver.
    editor: Option<(&'static str, &'static str)>,
    tela: Tela,
    escopo: Escopo,
    dialogo: Option<Dialogo>,
    abelha: Abelha,
    sem_abelha: bool,
    tema: tema::Escolha,
    favo: tema::Favo,
    compositor: Compositor,
    filtro: Option<String>,
    /// Eventos do perfil aberto e o estado da ligação.
    ouvinte: Option<Ouvinte>,
    conexao: Conexao,
    /// O primeiro retrato do perfil escolhe o projeto em foco.
    primeira_carga: bool,
    registro: registro::Registro,
    /// A apresentação da daily ou da sprint, por cima de tudo (a tela de
    /// baixo fica guardada para a volta).
    apresentacao: Option<Box<Apresentacao>>,
    /// A janela já estava em tela cheia antes da apresentação.
    tela_cheia_antes: bool,
    /// Último slide visto (daily ou sprint e a tarefa), para o Shift+F5.
    ultimo_slide: Option<(bool, i64)>,
    captura: Option<Captura>,
    clarao: Option<(i64, f64)>,
    capturas: (Sender<CapturaEnviada>, Receiver<CapturaEnviada>),
    /// Quando chegaram os últimos avisos de "precisa de você".
    atencoes: Vec<f64>,
    /// Título atual da janela, para só mandar quando muda.
    titulo: String,
    /// A janela pediu atenção ao sistema e ainda não ganhou foco.
    pediu_atencao: bool,
    /// Já confirmou fechar com agentes rodando.
    pode_fechar: bool,
    /// Onde o aviso do rodapé aparece neste quadro.
    ancora_aviso: Option<egui::Pos2>,
    /// A caixa "Pedir ao agente" aberta na Daily ou na Sprint, presa ao botão
    /// do cartão, e os rascunhos por tarefa (ficam quando a caixa fecha).
    caixa_pedido: Option<(pedido::Caixa, egui::Rect)>,
    rascunhos: HashMap<i64, String>,
    /// Quando cada agente respondeu um pedido pela última vez.
    respondeu_em: HashMap<i64, f64>,
    /// Navegador da tarefa: se há Chrome (lido uma vez), a caixa de endereço
    /// ou o aviso de "sem Chrome" presos ao botão, o último endereço de cada
    /// tarefa e as respostas do núcleo (abrir pode levar uns segundos).
    navegador: Option<api::InfoNavegador>,
    popover_navegador: Option<PopoverNavegador>,
    enderecos: HashMap<i64, String>,
    respostas_navegador: (Sender<RespostaNavegador>, Receiver<RespostaNavegador>),
    /// As respostas aos pedidos de consulta voltando do núcleo (aprovar com
    /// senha testa a conexão antes: pode levar uns segundos).
    respostas_pedido: (Sender<RespostaPedido>, Receiver<RespostaPedido>),
    abrindo_navegador: Option<i64>,
    /// O topo do terminal da tarefa, em pontos da janela (o navegador sem
    /// espaço ao lado fica abaixo do cabeçalho).
    topo_terminal: Option<f32>,
    /// O que o núcleo já sabe da tela: o perfil, se ela mostra algo que
    /// pode estar compartilhado (apresentação, Daily ou Sprint) e onde o
    /// navegador abre ao lado dela. Mandado só quando muda (e de novo ao
    /// reconectar); a geometria só chega depois dos primeiros quadros.
    tela_avisada: Option<(i64, bool, Option<[i32; 4]>)>,
    /// Gaveta de arquivos: a árvore de cada tarefa (em cache) e a tarefa com a gaveta aberta.
    arvores: HashMap<i64, gaveta::Arvore>,
    gaveta: Option<i64>,
    /// Lousas abertas nesta sessão (a câmera de cada uma fica lembrada), as
    /// tarefas com a lousa cobrindo o terminal, as que o agente mexeu sem
    /// você ver (o ponto no chip) e o workspace para onde o "‹ Lousa" volta.
    lousas: HashMap<api::DonoLousa, lousa::Lousa>,
    lousa_na_tarefa: HashSet<i64>,
    /// A tarefa que está na tela (ao abrir outra, a lousa dela começa fechada).
    tarefa_na_tela: Option<i64>,
    novidade_na_lousa: HashSet<i64>,
    voltar_para_lousa: Option<i64>,
    /// A lousa que acabou de abrir pela barra lateral: ganha o teclado no primeiro quadro.
    lousa_ativa_ao_abrir: Option<api::DonoLousa>,
    /// Onde a lousa da tarefa vai neste quadro (o corpo do terminal em foco).
    area_lousa: Option<egui::Rect>,
    /// A lousa apresentada em tela cheia (o palco).
    palco: Option<(api::DonoLousa, Box<lousa::palco::Palco>)>,
    /// Os bancos de dados do perfil (conexões, árvore e consoles), a tela de
    /// onde Ctrl+Shift+K veio e o que você está fazendo em cada pedido de
    /// consulta de um agente.
    banco: banco::Bancos,
    tela_antes_do_banco: Option<Tela>,
    pedidos_consulta: HashMap<String, banco::aprovacao::Estado>,
    carga: &'static str,
    bytes: Arc<AtomicU64>,
    quadros: u64,
    // Medidor: valores do último segundo.
    ultimo_segundo: f64,
    bytes_antes: u64,
    quadros_antes: u64,
    fps: u64,
    vazao: u64,
    /// COLMEIA_FPS=1 mostra o contador de quadros também fora da demonstração (medição).
    mostrar_fps: bool,
}

impl Colmeia {
    fn new(cc: &eframe::CreationContext, problema: Option<String>) -> Self {
        tema::instalar(&cc.egui_ctx);
        // COLMEIA_TEMA=claro | escuro | leitura escolhe o tema antes de entrar num perfil.
        let escolha = tema::Escolha::da_chave(&std::env::var("COLMEIA_TEMA").unwrap_or_default());
        escolha.aplicar(&cc.egui_ctx);
        let demo = canal::pedir("GET", "/v1/versao").is_ok_and(|v| v.contains("\"demo\":true"));
        let bytes = Arc::new(AtomicU64::new(0));

        let mut app = Colmeia {
            demo,
            problema_nucleo: problema,
            aviso: None,
            perfil: None,
            perfis: Vec::new(),
            nome_workspace: String::new(),
            tempo_pendente: None,
            modelo: dados::Modelo::default(),
            branches: Vec::new(),
            terminais: HashMap::new(),
            ferramentas: None,
            contas: None,
            editor: sistema::editor(),
            tela: Tela::Quadro,
            escopo: Escopo::Perfil,
            dialogo: None,
            abelha: Abelha::new(),
            sem_abelha: std::env::var("COLMEIA_SEM_ABELHA").is_ok_and(|v| v == "1"),
            tema: escolha,
            favo: tema::Favo::default(),
            compositor: Compositor::default(),
            filtro: None,
            ouvinte: None,
            conexao: Conexao::Ligado,
            primeira_carga: false,
            registro: registro::Registro::default(),
            apresentacao: None,
            tela_cheia_antes: false,
            ultimo_slide: None,
            captura: None,
            clarao: None,
            capturas: mpsc::channel(),
            atencoes: Vec::new(),
            titulo: "Colmeia".into(),
            pediu_atencao: false,
            pode_fechar: false,
            ancora_aviso: None,
            caixa_pedido: None,
            rascunhos: HashMap::new(),
            respondeu_em: HashMap::new(),
            navegador: None,
            popover_navegador: None,
            enderecos: HashMap::new(),
            respostas_navegador: mpsc::channel(),
            respostas_pedido: mpsc::channel(),
            abrindo_navegador: None,
            topo_terminal: None,
            tela_avisada: None,
            arvores: HashMap::new(),
            gaveta: None,
            lousas: HashMap::new(),
            lousa_na_tarefa: HashSet::new(),
            tarefa_na_tela: None,
            novidade_na_lousa: HashSet::new(),
            voltar_para_lousa: None,
            lousa_ativa_ao_abrir: None,
            area_lousa: None,
            palco: None,
            banco: banco::Bancos::novo(0),
            tela_antes_do_banco: None,
            pedidos_consulta: HashMap::new(),
            carga: "parada",
            bytes: bytes.clone(),
            quadros: 0,
            ultimo_segundo: 0.0,
            bytes_antes: 0,
            quadros_antes: 0,
            fps: 0,
            vazao: 0,
            mostrar_fps: std::env::var("COLMEIA_FPS").is_ok_and(|v| v == "1"),
        };

        app.compositor.focar = true;
        if !demo {
            app.tela = Tela::Entrada(Box::new(entrada::Entrada::new(false)));
            return app;
        }

        // Modo demonstração: dados de exemplo e os terminais de teste do núcleo.
        // COLMEIA_CARTOES=500, COLMEIA_TAREFA=101 e COLMEIA_CENARIO=erro ajudam a medir sem clicar.
        app.perfil = Some(api::Perfil { id: 0, nome: "Demonstração".into(), tema: escolha.chave().into(), aviso_captura: false, tempo_agentes: false });
        app.modelo.projetos = dados::projetos_demo();
        let cartoes = std::env::var("COLMEIA_CARTOES").ok().and_then(|v| v.parse().ok()).unwrap_or(50);
        app.modelo.tarefas = dados::gerar_demo(cartoes);
        if std::env::var("COLMEIA_CENARIO").is_ok_and(|v| v == "erro")
            && let Some(t) = app.modelo.tarefas.iter_mut().find(|t| t.id == 103)
        {
            t.erro = Some("agente-6 parou: 3 testes falhando".into());
        }
        app.terminais = (0..dados::PAPEIS_DEMO.len() as i64)
            .map(|id| (id, TerminalAgente::conectar(format!("/v1/terminais/{id}"), cc.egui_ctx.clone(), bytes.clone(), SO_CARTAO)))
            .collect();
        app.escopo = Escopo::Projeto(1);
        app.carregar_branches();
        if let Some(t) = std::env::var("COLMEIA_TAREFA").ok().and_then(|v| v.parse::<i64>().ok()).and_then(|id| app.modelo.tarefas.iter().find(|t| t.id == id))
        {
            app.tela = Tela::Tarefa { id: t.id, foco: t.agentes.first().map_or(0, |a| a.id) };
        }
        app
    }

    fn medir(&mut self, agora: f64) {
        self.quadros += 1;
        if self.mostrar_fps {
            // Cada quadro no log (só com COLMEIA_FPS=1): parada, a tela não escreve nada.
            eprintln!("quadro {agora:.3}");
        }
        if agora - self.ultimo_segundo >= 1.0 {
            let bytes = self.bytes.load(Ordering::Relaxed);
            self.fps = self.quadros - self.quadros_antes;
            self.vazao = bytes - self.bytes_antes;
            self.quadros_antes = self.quadros;
            self.bytes_antes = bytes;
            self.ultimo_segundo = agora;
        }
    }

    fn avisar(&mut self, tipo: TipoAviso, texto: impl Into<String>, agora: f64) {
        let ate = agora + if tipo == TipoAviso::Neutro { 4.0 } else { 8.0 };
        self.aviso = Some(Aviso { tipo, texto: texto.into(), acao: None, ate });
    }

    fn avisar_com(&mut self, tipo: TipoAviso, texto: impl Into<String>, acao: &'static str, o_que: AcaoAviso, agora: f64) {
        self.aviso = Some(Aviso { tipo, texto: texto.into(), acao: Some((acao, o_que)), ate: agora + 8.0 });
    }

    fn erro(&mut self, texto: impl Into<String>, agora: f64) {
        self.avisar(TipoAviso::Erro, texto, agora);
    }

    /// Sem o núcleo, nada de criar, mover ou remover: a tela mostra, mas não pede.
    fn pode_mudar(&self) -> bool {
        self.demo || self.conexao == Conexao::Ligado
    }

    // Perfil e eventos

    fn entrar(&mut self, perfil: api::Perfil, ctx: &egui::Context) {
        // O perfil anterior deixa de estar na tela.
        if let Some((anterior, true, _)) = self.tela_avisada.take() {
            avisar_apresentando(anterior, false, None);
        }
        self.tema = tema::Escolha::da_chave(&perfil.tema);
        self.tema.aplicar(ctx);
        self.perfis = api::perfis().unwrap_or_default();
        self.filtro = None;
        self.tela = Tela::Quadro;
        // Os terminais do perfil anterior saem da tela; no núcleo eles continuam rodando.
        self.terminais.clear();
        self.contas = None;
        self.modelo = dados::Modelo::default();
        self.registro = registro::Registro::default();
        self.apresentacao = None;
        self.ultimo_slide = None;
        self.escopo = Escopo::Perfil;
        // As lousas do perfil anterior gravam o que falta antes de sair.
        for l in self.lousas.values_mut() {
            l.gravar_ja();
        }
        self.lousas.clear();
        self.lousa_na_tarefa.clear();
        self.novidade_na_lousa.clear();
        self.voltar_para_lousa = None;
        self.palco = None;
        // As senhas e os consoles do perfil anterior ficam para trás.
        self.banco = banco::Bancos::novo(perfil.id);
        self.banco.nome_perfil = perfil.nome.clone();
        self.tela_antes_do_banco = None;
        self.pedidos_consulta.clear();
        // O retrato do quadro chega pela thread de eventos, fora da thread da tela.
        self.ouvinte = Some(Ouvinte::iniciar(perfil.id, ctx.clone()));
        self.conexao = Conexao::Conectando;
        self.primeira_carga = true;
        self.perfil = Some(perfil);
    }

    /// Aplica o que a thread de eventos mandou desde o último quadro. Uma
    /// rajada (dez agentes mudando juntos) é aplicada inteira num quadro só.
    fn receber_eventos(&mut self, ctx: &egui::Context, agora: f64) {
        let Some(ouvinte) = &self.ouvinte else { return };
        let mensagens: Vec<Mensagem> = ouvinte.recebe.try_iter().collect();
        if mensagens.is_empty() {
            return;
        }
        let mut atencoes = Vec::new();
        let (mut linha_mudou, mut estado_mudou) = (false, false);
        // O nome do workspace em foco, para o aviso se ele sumir.
        if let Escopo::Workspace(w) = self.escopo
            && let Some(p) = self.modelo.projetos.iter().find(|p| p.workspace_id == w)
        {
            self.nome_workspace = p.workspace.clone();
        }
        // Novidade para a apresentação: o que não veio dela mesma (nota e anexos).
        let mut de_fora = false;
        for m in mensagens {
            match m {
                Mensagem::Quadro(q) => {
                    self.modelo.carregar(q);
                    self.conectar_agentes(ctx);
                    if self.primeira_carga {
                        self.primeira_carga = false;
                        self.escopo = self.modelo.projetos.first().map_or(Escopo::Perfil, |p| Escopo::Projeto(p.id));
                        self.carregar_branches();
                    }
                    if let Escopo::Projeto(id) = self.escopo
                        && !self.modelo.projetos.iter().any(|p| p.id == id)
                    {
                        self.escopo = Escopo::Perfil;
                    }
                    self.conferir_workspace(agora);
                    if matches!(self.conexao, Conexao::Fora | Conexao::Antigo) {
                        self.avisar(TipoAviso::Neutro, "Conectado de novo", agora);
                        // O núcleo pode ter reiniciado: esquece o que ele sabia da
                        // tela e se havia Chrome (lido de novo no próximo clique).
                        self.tela_avisada = None;
                        self.navegador = None;
                        // E se há chaveiro (o diálogo de senha diz onde ela vai ficar).
                        self.banco.recarregar(ctx);
                    }
                    self.conexao = Conexao::Ligado;
                    linha_mudou = true;
                    for l in self.lousas.values_mut() {
                        l.recarregar(ctx);
                    }
                }
                Mensagem::Evento(e) => {
                    linha_mudou |= e.entra_na_linha();
                    // A opção do tempo mudou (aqui ou em outra tela): o perfil
                    // guarda, e o registro esconde na hora ou busca de novo.
                    if let dados::Evento::PerfilTempoAgentes { perfil_id, mostrar, .. } = &e
                        && let Some(p) = self.perfil.as_mut().filter(|p| p.id == *perfil_id)
                    {
                        p.tempo_agentes = *mostrar;
                    }
                    if let dados::Evento::LousaMudou { lousa_id, dono, elementos, removidos, agente_id, .. } = &e {
                        self.lousa_mudou(*lousa_id, *dono, elementos.clone(), removidos, *agente_id, agora);
                    }
                    // Nota e anexos que a própria apresentação mudou não são novidade;
                    // os que vieram de fora (outra tela, a API) são.
                    if let (dados::Evento::NotaAtualizada { tarefa_id, agente_id, .. }, Some(a)) = (&e, self.apresentacao.as_mut())
                        && *agente_id != 0
                    {
                        a.nota_do_agente(*tarefa_id);
                    }
                    let tarefa_mudada = match &e {
                        dados::Evento::NotaAtualizada { tarefa_id, .. } | dados::Evento::AnexoAdicionado { tarefa_id, .. } => Some(*tarefa_id),
                        _ => None,
                    };
                    let proprio = match (&e, self.apresentacao.as_mut()) {
                        (dados::Evento::NotaAtualizada { tarefa_id, .. }, Some(a)) => a.evento_proprio(apresentacao::Proprio::Nota, *tarefa_id),
                        (dados::Evento::AnexoAdicionado { tarefa_id, .. }, Some(a)) => a.evento_proprio(apresentacao::Proprio::AnexoNovo, *tarefa_id),
                        (dados::Evento::AnexoRemovido { tarefa_id, .. }, Some(a)) => a.evento_proprio(apresentacao::Proprio::AnexoRemovido, *tarefa_id),
                        (dados::Evento::DailyFora { tarefa_id, .. }, Some(a)) => a.evento_proprio(apresentacao::Proprio::ForaDaDaily, *tarefa_id),
                        _ => false,
                    };
                    de_fora |= e.entra_na_linha() && !proprio;
                    // O agente respondendo um pedido (a nota e as capturas chegam
                    // uma a uma): o slide se atualiza sozinho, sem sair dali.
                    if let (Some(t), false, Some(a)) = (tarefa_mudada, proprio, self.apresentacao.as_mut())
                        && self.modelo.pedidos.iter().any(|p| {
                            p.tarefa_id == t
                                && (p.aberto() || (p.estado == "respondido" && self.respondeu_em.get(&p.agente_id).is_some_and(|r| agora - r < 120.0)))
                        })
                    {
                        a.atualizar_pelo_agente(ctx);
                    }
                    estado_mudou |= matches!(e, dados::Evento::AgenteEstado { .. });
                    for efeito in self.modelo.aplicar(e) {
                        match efeito {
                            Efeito::Conectar(id) => {
                                if self.terminais.get(&id).is_none_or(|t| t.encerrado()) {
                                    let t = TerminalAgente::conectar(format!("/v1/agentes/{id}/terminal"), ctx.clone(), self.bytes.clone(), SO_CARTAO);
                                    self.terminais.insert(id, t);
                                }
                            }
                            Efeito::Soltar(id) => {
                                self.terminais.remove(&id);
                                self.compositor.esquecer(id);
                            }
                            Efeito::Concluiu { tarefa, projeto } => self.abelha.concluiu(tarefa, projeto, agora),
                            // O agente que acabou de responder um pedido espera você: é o
                            // normal, e o aviso "respondeu · Ver" fica no lugar.
                            // Também não chama quem espera só para receber um pedido da fila.
                            Efeito::Atencao { agente, erro: false, .. }
                                if self.respondeu_em.get(&agente).is_some_and(|t| agora - t < 30.0)
                                    || self.modelo.pedidos.iter().any(|p| p.agente_id == agente && p.estado == "fila") => {}
                            Efeito::Atencao { tarefa, agente, texto, erro } => atencoes.push((tarefa, agente, texto, erro)),
                            Efeito::Recarregar => {
                                for l in self.lousas.values_mut() {
                                    l.recarregar(ctx);
                                }
                            }
                            // Um agente quer consultar o banco: um aviso só, fora da apresentação
                            // e fora da tarefa dele (lá o cartão do pedido já aparece).
                            Efeito::PedidoDeConsulta { tarefa, agente, texto } => {
                                let na_tela = matches!(self.tela, Tela::Tarefa { id, .. } if id == tarefa);
                                if self.apresentacao.is_none() && !na_tela {
                                    self.avisar_com(TipoAviso::Alerta, texto, "Ver", AcaoAviso::Abrir { tarefa, agente }, agora);
                                }
                                let focada = ctx.input(|i| i.viewport().focused.unwrap_or(true));
                                if !focada && !self.pediu_atencao {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                                    self.pediu_atencao = true;
                                }
                            }
                            Efeito::Conexoes { conexao, consulta } => {
                                if consulta {
                                    self.banco.houve_consulta(ctx, conexao);
                                } else {
                                    self.banco.recarregar(ctx);
                                }
                            }
                            Efeito::PedidoRespondido { tarefa, agente, titulo } => {
                                self.respondeu_em.insert(agente, agora);
                                match self.apresentacao.as_mut() {
                                    // Na apresentação: o slide se atualiza e a faixa avisa.
                                    Some(a) => a.pedido_respondido(ctx, tarefa, &titulo),
                                    // Fora dela, o aviso fica até ser visto ou fechado.
                                    None => {
                                        self.avisar_com(
                                            TipoAviso::Neutro,
                                            format!("O agente respondeu em «{titulo}»"),
                                            "Ver",
                                            AcaoAviso::VerPedido(tarefa),
                                            agora,
                                        );
                                        if let Some(a) = &mut self.aviso {
                                            a.ate = f64::INFINITY;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Mensagem::Desconectado => self.conexao = Conexao::Fora,
                Mensagem::Antigo => self.conexao = Conexao::Antigo,
                Mensagem::Aviso(texto) => self.erro(texto, agora),
            }
        }
        if let Tela::Tarefa { id, .. } = self.tela
            && !self.modelo.tarefas.iter().any(|t| t.id == id)
        {
            self.tela = Tela::Quadro;
        }
        self.conferir_workspace(agora);
        if let Some(a) = &mut self.apresentacao {
            // Na apresentação, os slides não mudam sozinhos: aparece "Novidades · R atualiza".
            a.novidades |= de_fora;
            if linha_mudou || estado_mudou {
                self.registro.marcar_suja();
            }
        } else if let Tela::Registro(aba) = self.tela {
            let perfil = self.perfil.as_ref().map_or(0, |p| p.id);
            if linha_mudou {
                self.registro.novidade(aba, perfil, ctx);
            } else if estado_mudou {
                self.registro.estado_mudou(aba, perfil, ctx);
            }
        } else if linha_mudou || estado_mudou {
            // Fora da tela, o registro só fica marcado; busca ao voltar.
            self.registro.marcar_suja();
        }
        self.chamar_atencao(atencoes, ctx, agora);
    }

    /// Um agente passou a precisar de você. Se ele não está na tela, aparece um
    /// aviso (vários em pouco tempo viram um só); com a janela sem foco, ela
    /// pede atenção ao sistema uma vez por lote.
    fn chamar_atencao(&mut self, atencoes: Vec<(i64, i64, String, bool)>, ctx: &egui::Context, agora: f64) {
        if atencoes.is_empty() {
            return;
        }
        // Com a janela sem foco, nada está "na tela": quem abriu a tarefa e
        // foi para o navegador também precisa saber. O filtro vale só para o
        // aviso dentro da janela.
        let focada = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if !focada && !self.pediu_atencao {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
            self.pediu_atencao = true;
        }
        let na_tela = |tarefa: i64| matches!(self.tela, Tela::Tarefa { id, .. } if id == tarefa);
        let fora_da_tela: Vec<_> = atencoes.into_iter().filter(|(t, ..)| !na_tela(*t)).collect();
        if fora_da_tela.is_empty() {
            return;
        }
        self.atencoes.retain(|t| agora - t < JUNTAR_AVISOS);
        self.atencoes.extend(fora_da_tela.iter().map(|_| agora));
        let erro = fora_da_tela.iter().any(|a| a.3);
        let tipo = if erro { TipoAviso::Erro } else { TipoAviso::Alerta };
        if self.atencoes.len() > 1 {
            let n = self.atencoes.len();
            self.avisar_com(tipo, format!("{n} agentes precisam de você"), "Ver", AcaoAviso::VerAbelha, agora);
        } else {
            let (tarefa, agente, texto, _) = fora_da_tela.into_iter().next().expect("um aviso");
            self.avisar_com(tipo, texto, "Abrir", AcaoAviso::Abrir { tarefa, agente }, agora);
        }
    }

    /// Quantos agentes do perfil precisam de você agora.
    fn esperando_voce(&self) -> usize {
        self.modelo.tarefas.iter().flat_map(|t| &t.agentes).filter(|a| a.visual().pede_voce() && (a.visual() != EstadoVisual::Erro || !a.erro_visto)).count()
    }

    /// Título da janela: "Colmeia · 2 esperando você" só com a janela sem foco.
    fn atualizar_titulo(&mut self, ctx: &egui::Context) {
        let focada = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        // Ao ganhar foco, o pedido de atenção é desligado (uma vez só).
        if focada && self.pediu_atencao {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Reset));
            self.pediu_atencao = false;
        }
        let n = if focada { 0 } else { self.esperando_voce() };
        let titulo = if n == 0 { "Colmeia".to_string() } else { format!("Colmeia · {n} esperando você") };
        if titulo != self.titulo {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(titulo.clone()));
            self.titulo = titulo;
        }
    }

    /// Liga a tela aos terminais dos agentes que estão rodando no núcleo e
    /// solta os que não existem mais. Os parados guardam o que ficou na tela.
    fn conectar_agentes(&mut self, ctx: &egui::Context) {
        if self.demo {
            return;
        }
        let todos: Vec<(i64, bool)> = self.modelo.tarefas.iter().flat_map(|t| t.agentes.iter().map(|a| (a.id, a.ativo))).collect();
        self.terminais.retain(|id, _| todos.iter().any(|(a, _)| a == id));
        for (id, ativo) in todos {
            if ativo && self.terminais.get(&id).is_none_or(|t| t.encerrado()) {
                self.terminais.insert(id, TerminalAgente::conectar(format!("/v1/agentes/{id}/terminal"), ctx.clone(), self.bytes.clone(), SO_CARTAO));
            }
        }
    }

    /// Retrato novo do quadro, pedido direto (quando uma mudança foi recusada
    /// e a tela precisa voltar ao que o núcleo tem).
    fn refazer_quadro(&mut self, ctx: &egui::Context) {
        if let Some(perfil) = &self.perfil
            && let Ok(q) = api::quadro(perfil.id)
        {
            self.modelo.carregar(q);
            self.conectar_agentes(ctx);
        }
    }

    /// Ferramentas que o perfil pode abrir como agente: as instaladas que o perfil
    /// escolheu (ou todas as instaladas, se não escolheu nenhuma) e um terminal comum.
    fn opcoes_de_agente(&mut self) -> Vec<dialogos::Opcao> {
        let perfil = self.perfil.as_ref().map(|p| p.id);
        let contas = self.contas.get_or_insert_with(|| perfil.and_then(|p| api::contas(p).ok()).unwrap_or_default());
        let instaladas = self.ferramentas.get_or_insert_with(|| api::ferramentas().unwrap_or_default());
        let mut opcoes: Vec<dialogos::Opcao> = instaladas
            .iter()
            .filter(|f| f.instalada && (contas.is_empty() || contas.iter().any(|c| c.ferramenta == f.id)))
            .map(|f| dialogos::Opcao { id: f.id.clone(), nome: f.nome.clone() })
            .collect();
        opcoes.push(dialogos::Opcao { id: "shell".into(), nome: "Terminal".into() });
        opcoes
    }

    fn adicionar_agente(&mut self, tarefa: i64, retomar: bool) {
        let Some(t) = self.modelo.tarefas.iter().find(|t| t.id == tarefa) else {
            return;
        };
        let pasta = t.pasta.clone();
        let opcoes = self.opcoes_de_agente();
        self.dialogo = Some(Dialogo::NovoAgente(dialogos::NovoAgente::new(tarefa, pasta, opcoes, retomar)));
    }

    /// O agente acabou de ser criado: liga o terminal e põe em foco. A tarefa
    /// sai do Backlog sozinha (regra do núcleo, chega pelos eventos).
    fn agente_criado(&mut self, a: api::Agente, ctx: &egui::Context) {
        let terminal = TerminalAgente::conectar(format!("/v1/agentes/{}/terminal", a.id), ctx.clone(), self.bytes.clone(), TEMPO_REAL);
        self.terminais.insert(a.id, terminal);
        if let Some(t) = self.modelo.tarefas.iter_mut().find(|t| t.id == a.tarefa_id) {
            match t.agentes.iter_mut().find(|x| x.id == a.id) {
                Some(x) => *x = AgenteTela::da_api(&a),
                None => t.agentes.push(AgenteTela::da_api(&a)),
            }
            t.derivar();
        }
        self.tela = Tela::Tarefa { id: a.tarefa_id, foco: a.id };
    }

    fn iniciar_agente(&mut self, id: i64, ctx: &egui::Context, agora: f64) {
        match api::iniciar_agente(id) {
            Ok(_) => {
                let terminal = TerminalAgente::conectar(format!("/v1/agentes/{id}/terminal"), ctx.clone(), self.bytes.clone(), TEMPO_REAL);
                self.terminais.insert(id, terminal);
            }
            Err(e) => self.erro(format!("Não consegui iniciar o agente: {e}"), agora),
        }
    }

    fn remover_agente(&mut self, id: i64, agora: f64) {
        if let Err(e) = api::remover_agente(id) {
            return self.erro(format!("Não consegui remover o agente: {e}"), agora);
        }
        self.terminais.remove(&id);
        self.compositor.esquecer(id);
        for t in &mut self.modelo.tarefas {
            t.agentes.retain(|a| a.id != id);
            t.derivar();
        }
        if let Tela::Tarefa { id: tarefa, foco } = self.tela
            && foco == id
        {
            let proximo = self.modelo.tarefas.iter().find(|t| t.id == tarefa).and_then(|t| t.agentes.first()).map_or(0, |a| a.id);
            self.tela = Tela::Tarefa { id: tarefa, foco: proximo };
        }
    }

    fn carregar_branches(&mut self) {
        self.branches = if self.demo {
            dados::BRANCHES_DEMO.iter().map(|b| b.to_string()).collect()
        } else if let Escopo::Projeto(id) = self.escopo {
            api::branches(id).unwrap_or_default()
        } else {
            let escopo = self.escopo;
            let projetos = &self.modelo.projetos;
            let mut todas: Vec<String> = self.modelo.tarefas.iter().filter(|t| escopo.contem(t.projeto_id, projetos)).map(|t| t.branch.clone()).collect();
            todas.sort();
            todas.dedup();
            todas
        };
    }

    /// O workspace em foco ficou sem projetos (todos removidos): volta para
    /// todos os projetos, com um aviso neutro.
    fn conferir_workspace(&mut self, agora: f64) {
        if let Escopo::Workspace(w) = self.escopo
            && !self.modelo.projetos.iter().any(|p| p.workspace_id == w)
        {
            self.escopo = Escopo::Perfil;
            self.carregar_branches();
            let nome = if self.nome_workspace.is_empty() { String::new() } else { format!(" {}", self.nome_workspace) };
            self.avisar(TipoAviso::Neutro, format!("O workspace{nome} foi removido; mostrando todos os projetos."), agora);
        }
    }

    /// Os projetos do escopo atual, na ordem da barra lateral.
    fn projetos_no_escopo(&self) -> Vec<&Projeto> {
        let escopo = self.escopo;
        self.modelo.projetos.iter().filter(|p| escopo.contem(p.id, &self.modelo.projetos)).collect()
    }

    fn mudar_escopo(&mut self, escopo: Escopo) {
        self.escopo = escopo;
        self.voltar_para_lousa = None;
        // O registro acompanha o escopo; o painel da tarefa volta ao quadro.
        if !matches!(self.tela, Tela::Registro(_)) {
            self.tela = Tela::Quadro;
        }
        self.filtro = None;
        self.carregar_branches();
    }

    fn projeto_em_foco(&self) -> Option<&Projeto> {
        match self.escopo {
            Escopo::Projeto(id) => self.modelo.projetos.iter().find(|p| p.id == id),
            Escopo::Perfil | Escopo::Workspace(_) => None,
        }
    }

    /// Cada terminal recebe no ritmo que a tela precisa: tempo real só para o
    /// que está em foco, miniatura para os outros da tarefa, e o mínimo para
    /// quem só aparece como última linha num cartão.
    fn ajustar_ritmos(&self) {
        let (foco, tarefa) = match self.tela {
            // Com a lousa por cima, o terminal em foco não aparece: ritmo de fundo.
            Tela::Tarefa { id, .. } if self.lousa_na_tarefa.contains(&id) => (None, None),
            Tela::Tarefa { id, foco } => (Some(foco), self.modelo.tarefas.iter().find(|t| t.id == id)),
            _ => (None, None),
        };
        for (&i, t) in &self.terminais {
            let ms = if Some(i) == foco {
                TEMPO_REAL
            } else if tarefa.is_some_and(|t| t.agentes.iter().any(|a| a.id == i)) {
                MINIATURA
            } else {
                SO_CARTAO
            };
            t.definir_intervalo(ms);
        }
    }

    /// Abre a tela de bancos (ou, se ela já está aberta e não foi um clique
    /// na barra lateral, volta para a tela de antes).
    fn alternar_banco(&mut self, pela_lateral: bool) {
        if matches!(self.tela, Tela::Banco) {
            if !pela_lateral {
                self.tela = self.tela_antes_do_banco.take().unwrap_or(Tela::Quadro);
            }
            return;
        }
        let antes = std::mem::replace(&mut self.tela, Tela::Banco);
        self.banco.marcar_aberta();
        self.tela_antes_do_banco = match antes {
            Tela::Entrada(_) => None,
            outra => Some(outra),
        };
    }

    /// Abre a tarefa, com foco no agente pedido (ou no primeiro).
    fn abrir_tarefa(&mut self, id: i64, agente: Option<i64>) {
        let Some(t) = self.modelo.tarefas.iter().find(|t| t.id == id) else {
            return;
        };
        let foco = agente.filter(|a| t.agentes.iter().any(|x| x.id == *a)).or_else(|| t.agentes.first().map(|a| a.id)).unwrap_or(0);
        let projeto = t.projeto_id;
        if !self.escopo.contem(projeto, &self.modelo.projetos) {
            self.escopo = Escopo::Projeto(projeto);
            self.carregar_branches();
        }
        self.tela = Tela::Tarefa { id, foco };
        self.abelha.resumo_aberto = false;
        self.compositor.focar = true;
        self.voltar_para_lousa = None;
    }

    /// Ctrl+Shift+P: primeiro os erros que você não viu, depois quem espera
    /// você, do que espera há mais tempo; abre a tarefa no agente.
    fn proximo_que_precisa(&mut self) {
        let atual = match self.tela {
            Tela::Tarefa { foco, .. } => foco,
            _ => 0,
        };
        let mut candidatos: Vec<(u8, String, i64, i64)> = Vec::new();
        for t in self.modelo.tarefas.iter().filter(|t| self.escopo.contem(t.projeto_id, &self.modelo.projetos)) {
            for a in &t.agentes {
                match a.visual() {
                    // O pedido de consulta vence em minutos: vem antes de tudo.
                    EstadoVisual::PedeConsulta => candidatos.push((0, a.desde.clone(), t.id, a.id)),
                    EstadoVisual::Erro if !a.erro_visto => candidatos.push((1, String::new(), t.id, a.id)),
                    EstadoVisual::PedeAprovacao | EstadoVisual::SuaVez => candidatos.push((2, a.desde.clone(), t.id, a.id)),
                    _ => {}
                }
            }
        }
        candidatos.sort();
        let proximo = candidatos.iter().find(|c| c.3 != atual).or(candidatos.first());
        if let Some(&(_, _, tarefa, agente)) = proximo {
            self.abrir_tarefa(tarefa, Some(agente));
        }
    }

    fn mover(&mut self, id: i64, coluna: Coluna, ctx: &egui::Context, agora: f64) {
        let Some(t) = self.modelo.tarefas.iter().find(|t| t.id == id) else {
            return;
        };
        if self.demo {
            if coluna == Coluna::Concluido {
                self.abelha.concluiu(id, t.projeto_id, agora);
            }
            return;
        }
        // A tela já moveu o cartão; o evento do núcleo confirma (e comemora se concluiu).
        if let Err(e) = api::mover_tarefa(id, coluna.chave()) {
            self.erro(format!("Não consegui mover a tarefa: {e}"), agora);
            self.refazer_quadro(ctx);
        }
    }

    // Captura do terminal

    /// Pede a captura do terminal em foco: na primeira vez do perfil, depois do aviso de segredos.
    fn capturar(&mut self, agente: i64, tarefa: i64) {
        if self.demo || !self.pode_mudar() {
            return;
        }
        if self.perfil.as_ref().is_some_and(|p| p.aviso_captura) {
            self.dialogo = Some(Dialogo::Captura { agente, tarefa, nao_mostrar: false });
        } else {
            self.captura = Some(Captura::Agendada { agente, tarefa, quadros: 1 });
        }
    }

    /// Avança a captura: um quadro depois de pedida, manda a janela tirar a
    /// imagem; quando ela chega, recorta o terminal e envia ao núcleo numa thread.
    fn andar_captura(&mut self, ctx: &egui::Context) {
        match self.captura.take() {
            Some(Captura::Agendada { agente, tarefa, quadros }) if quadros > 0 => {
                self.captura = Some(Captura::Agendada { agente, tarefa, quadros: quadros - 1 });
                ctx.request_repaint();
            }
            Some(Captura::Agendada { agente, tarefa, .. }) => {
                if let Some(area) = self.terminais.get(&agente).and_then(|t| t.area()) {
                    self.captura = Some(Captura::Pedida { agente, tarefa, area, ppp: ctx.pixels_per_point() });
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(agente)));
                }
            }
            Some(Captura::Pedida { agente, tarefa, area, ppp }) => {
                let imagem = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, user_data, .. } if user_data.data.as_ref().and_then(|d| d.downcast_ref::<i64>()) == Some(&agente) => {
                            Some(image.clone())
                        }
                        _ => None,
                    })
                });
                let Some(imagem) = imagem else {
                    self.captura = Some(Captura::Pedida { agente, tarefa, area, ppp });
                    return;
                };
                let envio = self.capturas.0.clone();
                let acordar = ctx.clone();
                std::thread::spawn(move || {
                    let resultado = recortar_png(&imagem, area, ppp).and_then(|png| api::anexar(tarefa, "captura", Some(agente), &png));
                    let _ = envio.send(CapturaEnviada { tarefa, resultado });
                    acordar.request_repaint();
                });
                self.clarao = Some((agente, ctx.input(|i| i.time)));
            }
            None => {}
        }
    }

    fn receber_capturas(&mut self, agora: f64) {
        while let Ok(c) = self.capturas.1.try_recv() {
            let titulo = self.modelo.tarefas.iter().find(|t| t.id == c.tarefa).map(|t| t.titulo.clone()).unwrap_or_default();
            match c.resultado {
                Ok(anexo) => {
                    self.avisar_com(TipoAviso::Neutro, format!("Captura anexada a “{titulo}”"), "Desfazer", AcaoAviso::DesfazerCaptura(anexo.id), agora)
                }
                Err(e) => self.erro(format!("Não consegui capturar: {e}"), agora),
            }
        }
    }

    // Cenários da demonstração para testar a abelha sem esperar um erro de verdade.

    fn concluir_demo(&mut self, id: i64, agora: f64) {
        if let Some(i) = self.modelo.tarefas.iter().position(|t| t.id == id) {
            let mut t = self.modelo.tarefas.remove(i);
            t.coluna = Coluna::Concluido;
            t.motivo = None;
            self.abelha.concluiu(t.id, t.projeto_id, agora);
            self.modelo.tarefas.push(t);
        }
    }

    fn simular_erro(&mut self) {
        if let Some(t) = self.modelo.tarefas.iter_mut().find(|t| t.id == 103) {
            t.erro = Some("agente-6 parou: 3 testes falhando".into());
            t.erro_visto = false;
        }
        self.abelha.interromper_comemoracao();
    }

    fn simular_conclusao(&mut self, agora: f64) {
        let candidata =
            [Coluna::Revisao, Coluna::Backlog].into_iter().find_map(|c| self.modelo.tarefas.iter().find(|t| t.projeto_id == 1 && t.coluna == c).map(|t| t.id));
        if let Some(id) = candidata {
            self.concluir_demo(id, agora);
        }
    }

    fn simular_aprovacao(&mut self) {
        if let Some(t) = self.modelo.tarefas.iter_mut().find(|t| t.projeto_id == 3 && t.coluna == Coluna::Backlog) {
            t.coluna = Coluna::AguardandoVoce;
            t.motivo = Some("pede aprovação: merge na main".into());
        }
    }

    fn resolver_tudo(&mut self) {
        for t in &mut self.modelo.tarefas {
            t.erro = None;
            if t.coluna == Coluna::AguardandoVoce && t.id != 102 {
                t.coluna = Coluna::Backlog;
                t.motivo = None;
            }
        }
    }

    // Partes da tela

    /// Faixa no topo quando o núcleo está fora: o quadro continua visível, mas
    /// nada é pedido até ele voltar.
    fn faixa_nucleo(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        let (texto, cor) = match (self.conexao, &self.problema_nucleo) {
            (Conexao::Antigo, _) => {
                ("O núcleo em execução é de uma versão anterior. Rode este comando e abra a Colmeia de novo (os agentes abertos são encerrados):", p.erro)
            }
            (Conexao::Fora, _) => ("Núcleo desconectado. Tentando reconectar…", p.alerta),
            (_, Some(_)) => ("", p.erro),
            _ => return,
        };
        let mut texto = if texto.is_empty() { self.problema_nucleo.clone().unwrap_or_default() } else { texto.to_string() };
        // Com uma lousa na tela, a faixa dela não aparece: o aviso vem aqui.
        let lousa_na_tela = match self.tela {
            Tela::Lousa(_) => true,
            Tela::Tarefa { id, .. } => self.lousa_na_tarefa.contains(&id),
            _ => false,
        };
        if self.conexao == Conexao::Fora && lousa_na_tela {
            texto.push_str(" · a lousa está só para leitura");
        }
        // Opaco: uma cor translúcida aqui se misturaria com o preto da janela.
        let fundo = tema::fundo_tingido(p, cor, tema::claro());
        let mut tentar = false;
        egui::Panel::top("faixa-nucleo").show_separator_line(false).frame(egui::Frame::new().fill(fundo).inner_margin(egui::Margin::symmetric(20, 6))).show(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(32.0);
                    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, cor);
                    ui.label(RichText::new(texto).color(p.texto).size(13.5));
                    if self.conexao == Conexao::Antigo {
                        tema::etiqueta_ui(ui, "colmeia-nucleo --encerrar", egui::FontId::monospace(12.0), p.texto);
                    }
                    if self.conexao == Conexao::Fora {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            tentar = tema::botao_secundario(ui, "Tentar agora")
                                .on_hover_text("Reconecta e, se o núcleo não estiver rodando, inicia de novo")
                                .clicked();
                        });
                    }
                });
            },
        );
        if tentar && let Some(o) = &self.ouvinte {
            o.tentar_agora();
        }
    }

    fn topo(&mut self, ui: &mut egui::Ui, agora: f64) {
        let p = cores();
        // O caminho é clicável: o perfil e o workspace (e o projeto, dentro
        // de uma tarefa) fecham a tela atual e abrem o quadro deles.
        let mut ir: Option<Escopo> = None;
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            let perfil = self.perfil.as_ref().map(|p| p.nome.clone()).unwrap_or_default();
            let workspace_da_lousa = match self.tela {
                Tela::Lousa(ws) => self.modelo.projetos.iter().find(|p| p.workspace_id == ws).map(|p| p.workspace.clone()),
                _ => None,
            };
            let fps = format!("{} FPS", self.fps);
            let medir = |ui: &mut egui::Ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(&fps).color(p.suave).size(11.5)));
            };
            if matches!(self.tela, Tela::Banco) {
                if migalha(ui, &perfil) {
                    ir = Some(Escopo::Perfil);
                }
                match self.banco.nome_escolhida() {
                    // A conexão do console na trilha: onde o Ctrl+Enter vai rodar.
                    Some(nome) => {
                        ui.label(RichText::new("Bancos de dados  ›").color(p.suave));
                        ui.label(texto_forte(nome, 15.0).color(p.texto));
                    }
                    None => {
                        ui.label(texto_forte("Bancos de dados", 15.0).color(p.texto));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_secundario(ui, "Nova conexão").clicked() {
                        self.banco.nova_conexao("postgres");
                    }
                    if self.mostrar_fps {
                        ui.add_space(12.0);
                        ui.label(RichText::new(&fps).color(p.suave).size(11.5));
                    }
                });
                return;
            }
            if let Some(ws) = workspace_da_lousa {
                if migalha(ui, &perfil) {
                    ir = Some(Escopo::Perfil);
                }
                if migalha(ui, &ws)
                    && let Tela::Lousa(id) = self.tela
                {
                    ir = Some(Escopo::Workspace(id));
                }
                ui.label(texto_forte("Lousa", 15.0).color(p.texto));
                if self.mostrar_fps {
                    medir(ui);
                }
                return;
            }
            let workspace = match self.escopo {
                Escopo::Workspace(w) => {
                    let projetos: Vec<&Projeto> = self.modelo.projetos.iter().filter(|p| p.workspace_id == w).collect();
                    projetos.first().map(|p| (p.workspace.clone(), projetos.len()))
                }
                _ => None,
            };
            // Dentro de uma tarefa, o caminho é sempre o da própria tarefa
            // (perfil › workspace › projeto), seja qual for o escopo de onde se veio.
            let projeto_da_tarefa = match self.tela {
                Tela::Tarefa { id, .. } => {
                    self.modelo.tarefas.iter().find(|t| t.id == id).and_then(|t| self.modelo.projetos.iter().find(|p| p.id == t.projeto_id))
                }
                _ => None,
            };
            let workspace = if projeto_da_tarefa.is_some() { None } else { workspace };
            match (projeto_da_tarefa.or(self.projeto_em_foco()), workspace) {
                (None, Some((nome, n))) => {
                    if migalha(ui, &perfil) {
                        ir = Some(Escopo::Perfil);
                    }
                    ui.label(texto_forte(&nome, 15.0).color(p.texto));
                    ui.label(RichText::new(if n == 1 { "1 projeto".to_string() } else { format!("{n} projetos") }).color(p.suave));
                }
                (None, None) => {
                    ui.label(texto_forte(&perfil, 15.0).color(p.texto));
                    ui.label(RichText::new("todos os projetos").color(p.suave));
                }
                (Some(projeto), _) => {
                    if migalha(ui, &perfil) {
                        ir = Some(Escopo::Perfil);
                    }
                    if migalha(ui, &projeto.workspace) && projeto.workspace_id != 0 {
                        ir = Some(Escopo::Workspace(projeto.workspace_id));
                    }
                    // Dentro de uma tarefa, o nome do projeto volta ao quadro dele.
                    if projeto_da_tarefa.is_some() {
                        let r = ui
                            .add(egui::Label::new(texto_forte(&projeto.nome, 15.0).color(p.texto)).sense(egui::Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text("Voltar ao quadro do projeto");
                        if r.clicked() {
                            ir = Some(Escopo::Projeto(projeto.id));
                        }
                    } else {
                        ui.label(texto_forte(&projeto.nome, 15.0).color(p.texto));
                    }
                }
            }
            if !self.demo {
                if self.mostrar_fps {
                    medir(ui);
                }
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Modo demonstração: cargas de teste e cenários simulados para a abelha.
                let modos = ["parada", "leve", "pesada"];
                let atual = modos.iter().position(|m| *m == self.carga).unwrap_or(0);
                if let Some(i) = tema::segmentado(ui, &["Parado", "Carga leve", "Carga pesada"], atual) {
                    self.carga = modos[i];
                    pedir_carga(modos[i]);
                }
                ui.add_space(8.0);
                let resposta = tema::chip(ui, "Simular", "", false);
                egui::Popup::menu(&resposta).show(|ui| {
                    ui.set_min_width(280.0);
                    if tema::opcao_menu(ui, "Erro no api-pedidos", false) {
                        self.simular_erro();
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Tarefa concluída no loja-web", false) {
                        self.simular_conclusao(agora);
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Os dois ao mesmo tempo", false) {
                        self.simular_erro();
                        self.simular_conclusao(agora);
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Pedido de aprovação no estudos-rust", false) {
                        self.simular_aprovacao();
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Resolver tudo", false) {
                        self.resolver_tudo();
                        ui.close();
                    }
                });
                ui.add_space(12.0);
                ui.label(RichText::new(format!("{} FPS · {} recebidos", self.fps, formatar_vazao(self.vazao))).color(p.suave).size(11.5));
            });
        });
        if let Some(escopo) = ir {
            // Da lousa e dos bancos volta ao quadro; o registro só troca o escopo.
            if matches!(self.tela, Tela::Lousa(_) | Tela::Banco) {
                self.tela = Tela::Quadro;
            }
            self.mudar_escopo(escopo);
        }
    }

    /// Barra acima do quadro e do registro: a troca entre as quatro páginas e
    /// os filtros à esquerda; à direita, "Nova tarefa" no quadro e
    /// "Apresentar" e "Copiar texto" na daily e na sprint.
    fn barra(&mut self, ui: &mut egui::Ui, agora: f64) {
        // Pasta sem git não tem branches para filtrar.
        let sem_branches = self.projeto_em_foco().is_some_and(|p| p.sem_git);
        let aba = match self.tela {
            Tela::Registro(aba) => Some(aba),
            _ => None,
        };
        let mut avisar = None;
        let mut apresentar = false;
        let mut mudar_tempo = None;
        let mut barra_arranjo: Option<ArranjoBarra> = None;
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            if !self.demo {
                let largura_antes = ui.cursor().min.x;
                let atual = match aba {
                    None => 0,
                    Some(registro::Aba::Linha) => 1,
                    Some(registro::Aba::Daily) => 2,
                    Some(registro::Aba::Sprint) => 3,
                };
                if let Some(i) = tema::segmentado(ui, &["Quadro", "Linha do tempo", "Daily", "Sprint"], atual) {
                    self.tela = match i {
                        1 => Tela::Registro(registro::Aba::Linha),
                        2 => Tela::Registro(registro::Aba::Daily),
                        3 => Tela::Registro(registro::Aba::Sprint),
                        _ => Tela::Quadro,
                    };
                }
                // Dica sobre a troca inteira.
                let area = egui::Rect::from_min_max(egui::pos2(largura_antes, ui.min_rect().top()), ui.min_rect().max);
                ui.interact(area, ui.id().with("dica-troca"), egui::Sense::hover())
                    .on_hover_text("Quadro, linha do tempo (Ctrl+Shift+L), daily (Ctrl+Shift+D) e sprint");
                ui.add_space(12.0);
                if let Some(aba) = aba {
                    // O grupo da direita é medido antes: o chip "Ver" fica com o
                    // que sobra (cortado até um mínimo) e nada vai por cima dele.
                    let valor = self.valor_ver();
                    let natural = tema::largura_chip(ui, "Ver", &valor);
                    let livre = ui.available_width() - 8.0;
                    let arranjo = self.arranjo_barra(ui, aba, livre, natural);
                    let maximo = (livre - arranjo.largura).clamp(CHIP_VER_MINIMO, natural);
                    self.chip_ver(ui, maximo);
                    barra_arranjo = Some(arranjo);
                }
            }
            if aba.is_none() {
                let valor = self.filtro.clone().unwrap_or_else(|| "todas".into());
                let resposta = if sem_branches { None } else { Some(tema::chip(ui, "Branch", &valor, self.filtro.is_some())) };
                if let Some(resposta) = resposta {
                    egui::Popup::menu(&resposta).show(|ui| {
                        ui.set_min_width(240.0);
                        if tema::opcao_menu(ui, "todas", self.filtro.is_none()) {
                            self.filtro = None;
                            ui.close();
                        }
                        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            // Pasta sem git (em "Todos os projetos") não tem branch: nada de item vazio.
                            for b in self.branches.iter().filter(|b| !b.trim().is_empty()) {
                                if tema::opcao_menu(ui, b, self.filtro.as_deref() == Some(b)) {
                                    self.filtro = Some(b.clone());
                                    ui.close();
                                }
                            }
                        });
                    });
                }
            }
            if self.demo {
                let quantidade = self.modelo.tarefas.len().to_string();
                let resposta = tema::chip(ui, "Cartões", &quantidade, false);
                egui::Popup::menu(&resposta).show(|ui| {
                    ui.set_min_width(200.0);
                    for n in [50, 500] {
                        if tema::opcao_menu(ui, &format!("{n} cartões"), self.modelo.tarefas.len() == n) {
                            self.modelo.tarefas = dados::gerar_demo(n);
                            ui.close();
                        }
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tempo = self.perfil.as_ref().is_some_and(|p| p.tempo_agentes);
                let pode = self.pode_mudar();
                let dica_tempo = if pode {
                    "Quanto tempo os agentes trabalharam e quanto esperaram você. Fica salvo neste perfil e vale para a daily, a sprint, a linha do tempo, a apresentação e o texto copiado."
                } else {
                    "Sem conexão com o núcleo"
                };
                let arranjo = barra_arranjo.unwrap_or_default();
                match aba {
                    Some(registro::Aba::Linha) => {
                        if let Some(rotulo) = arranjo.caixa {
                            let mut marcada = tempo;
                            if tema::caixa_marcar_com(ui, rotulo, &mut marcada, pode).on_hover_text(dica_tempo).changed() {
                                mudar_tempo = Some(marcada);
                            }
                        } else {
                            let mais = tema::botao_icone(ui, tema::Icone::Mais, 32.0);
                            let aberto = egui::Popup::is_id_open(ui.ctx(), egui::Id::new("menu-registro"));
                            let mais = if aberto { mais } else { mais.on_hover_text("Mais opções da linha do tempo") };
                            egui::Popup::menu(&mais).id(egui::Id::new("menu-registro")).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                                ui.set_min_width(240.0);
                                if opcao_tempo(ui, tempo, pode) {
                                    mudar_tempo = Some(!tempo);
                                    ui.close();
                                }
                            });
                        }
                    }
                    Some(aba) => {
                        // "Apresentar" sempre no mesmo lugar, na ponta: o "⋯" fica
                        // à esquerda dos outros e não o empurra. Sem trabalho no
                        // período, ele não aparece (nada de botão inativo). Logo
                        // depois de desligar o tempo, ficam inativos até a
                        // resposta sem ele chegar.
                        let atualizando = self.registro.atualizando();
                        let texto = self.registro.texto(aba);
                        if atualizando {
                            tema::botao_principal(ui, "Apresentar", false).on_hover_text("Atualizando…");
                            if arranjo.copiar {
                                ui.add_space(8.0);
                                tema::botao_secundario_com(ui, "Copiar texto", false).on_hover_text("Atualizando…");
                            }
                        } else {
                            if self.registro.deck(aba).is_some() {
                                apresentar = tema::botao_principal(ui, "Apresentar", true).on_hover_text("F5").clicked();
                                ui.add_space(8.0);
                            }
                            if arranjo.copiar
                                && let Some(texto) = &texto
                                && tema::botao_secundario_com(ui, "Copiar texto", pode).clicked()
                            {
                                ui.ctx().copy_text(texto.clone());
                                avisar = Some("Copiado");
                            }
                        }
                        // A caixa do tempo fica na barra quando cabe; senão é o
                        // primeiro item do "⋯" (que então aparece nas duas abas).
                        if arranjo.mais {
                            ui.add_space(8.0);
                            let mais = tema::botao_icone(ui, tema::Icone::Mais, 32.0);
                            let aberto = egui::Popup::is_id_open(ui.ctx(), egui::Id::new("menu-registro"));
                            let rotulo = if aba == registro::Aba::Sprint { "Mais opções da sprint" } else { "Mais opções da daily" };
                            let mais = if aberto { mais } else { mais.on_hover_text(rotulo) };
                            // Id fixo e fechar só com clique fora: o clique no item
                            // é lido antes de o menu fechar (o menu fecha pelo ui.close()).
                            egui::Popup::menu(&mais).id(egui::Id::new("menu-registro")).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(
                                |ui| {
                                    ui.set_min_width(240.0);
                                    if arranjo.caixa.is_none() && opcao_tempo(ui, tempo, pode) {
                                        mudar_tempo = Some(!tempo);
                                        ui.close();
                                    }
                                    if !arranjo.copiar {
                                        if arranjo.caixa.is_none() {
                                            linha_do_menu(ui);
                                        }
                                        let tem = texto.is_some() && !atualizando && pode;
                                        if tema::opcao_menu_com(ui, "Copiar texto", None, tem) {
                                            if let Some(t) = &texto {
                                                ui.ctx().copy_text(t.clone());
                                                avisar = Some("Copiado");
                                            }
                                            ui.close();
                                        }
                                    }
                                    if aba == registro::Aba::Sprint {
                                        if arranjo.caixa.is_none() || !arranjo.copiar {
                                            linha_do_menu(ui);
                                        }
                                        let tem = self.registro.markdown().is_some() && pode;
                                        let copiar = tema::opcao_menu_com(ui, "Copiar em Markdown", None, tem);
                                        if copiar {
                                            if let Some(md) = self.registro.markdown() {
                                                ui.ctx().copy_text(md);
                                                avisar = Some("Copiado em Markdown");
                                            }
                                            ui.close();
                                        }
                                        if tema::opcao_menu_com(ui, "Salvar…", Some("Ctrl+S"), tem) {
                                            self.registro.salvar_sprint(ui.ctx());
                                            ui.close();
                                        }
                                    }
                                },
                            );
                        }
                        if let Some(rotulo) = arranjo.caixa {
                            ui.add_space(20.0);
                            let mut marcada = tempo;
                            if tema::caixa_marcar_com(ui, rotulo, &mut marcada, pode).on_hover_text(dica_tempo).changed() {
                                mudar_tempo = Some(marcada);
                            }
                        }
                    }
                    None => {
                        let pode = !self.demo && self.pode_mudar() && !self.modelo.projetos.is_empty();
                        let resposta = tema::botao_principal(ui, "+ Nova tarefa", pode);
                        let resposta = if !self.pode_mudar() { resposta.on_hover_text("Indisponível sem o núcleo") } else { resposta };
                        if resposta.clicked() {
                            // Em "Todos os projetos" e num workspace, o diálogo deixa escolher o projeto.
                            let todos: Vec<Projeto> = self.projetos_no_escopo().into_iter().cloned().collect();
                            let projeto = self.projeto_em_foco().cloned().or_else(|| todos.first().cloned());
                            if let Some(projeto) = projeto {
                                let outros = if self.escopo.projeto().is_none() { todos } else { Vec::new() };
                                self.dialogo = Some(Dialogo::NovaTarefa(dialogos::NovaTarefa::new(projeto, outros)));
                            }
                        }
                    }
                }
            });
        });
        ui.add_space(10.0);
        if let Some(mostrar) = mudar_tempo {
            self.mudar_tempo(ui.ctx(), mostrar);
        }
        if let Some(texto) = avisar {
            self.avisar(TipoAviso::Neutro, texto, agora);
        }
        if apresentar {
            self.apresentar(ui.ctx(), Inicio::Capa, true);
        }
    }

    /// O chip "Ver" da barra do registro: de quais projetos ver (o perfil,
    /// um workspace ou um projeto). Muda o mesmo escopo da barra lateral.
    /// Mede as peças do grupo da direita e escolhe o arranjo da barra.
    fn arranjo_barra(&self, ui: &egui::Ui, aba: registro::Aba, livre: f32, chip: f32) -> ArranjoBarra {
        let medir = |t: &str, fonte: egui::FontId| ui.painter().layout_no_wrap(t.to_owned(), fonte, p_texto()).size().x;
        let espaco = ui.spacing().item_spacing.x;
        let pecas = PecasBarra {
            apresentar: medir("Apresentar", tema::forte(13.5)) + 32.0 + 8.0 + espaco * 2.0,
            copiar: medir("Copiar texto", egui::FontId::proportional(13.0)) + 28.0 + 8.0 + espaco * 2.0,
            mais: 32.0 + 8.0 + espaco * 2.0,
            caixa: medir("Mostrar tempo dos agentes", egui::FontId::proportional(13.5)) + 24.0 + 20.0 + espaco * 2.0,
            caixa_curta: medir("Tempo dos agentes", egui::FontId::proportional(13.5)) + 24.0 + 20.0 + espaco * 2.0,
        };
        let atualizando = self.registro.atualizando();
        let tem_apresentar = atualizando || self.registro.deck(aba).is_some();
        let tem_copiar = atualizando || self.registro.texto(aba).is_some();
        arranjar_barra(aba == registro::Aba::Linha, aba == registro::Aba::Sprint, tem_apresentar, tem_copiar, pecas, livre, chip)
    }

    /// O que o chip "Ver" diz: "todos os projetos", o workspace ou o projeto
    /// (com o workspace na frente quando o nome se repete).
    fn valor_ver(&self) -> String {
        let projetos = &self.modelo.projetos;
        let repetido = |nome: &str| projetos.iter().filter(|p| p.nome == nome).count() > 1;
        match self.escopo {
            Escopo::Perfil => "todos os projetos".to_string(),
            Escopo::Workspace(w) => projetos.iter().find(|p| p.workspace_id == w).map_or_else(|| "workspace".into(), |p| p.workspace.clone()),
            Escopo::Projeto(id) => match projetos.iter().find(|p| p.id == id) {
                Some(p) if repetido(&p.nome) => format!("{} › {}", p.workspace, p.nome),
                Some(p) => p.nome.clone(),
                None => "projeto".into(),
            },
        }
    }

    fn chip_ver(&mut self, ui: &mut egui::Ui, maximo: f32) {
        let projetos = &self.modelo.projetos;
        // Os workspaces na ordem da lateral, com os projetos de cada um.
        let mut workspaces: Vec<(i64, &str, Vec<&Projeto>)> = Vec::new();
        for p in projetos {
            match workspaces.last_mut() {
                Some((id, _, lista)) if *id == p.workspace_id => lista.push(p),
                _ => workspaces.push((p.workspace_id, &p.workspace, vec![p])),
            }
        }
        let valor = self.valor_ver();
        let id_menu = egui::Id::new("menu-ver");
        let resposta = tema::chip_com_largura(ui, "Ver", &valor, self.escopo != Escopo::Perfil, maximo);
        let resposta = if egui::Popup::is_id_open(ui.ctx(), id_menu) {
            resposta
        } else {
            resposta.on_hover_text(format!("Ver: {valor}. De quais projetos ver: o perfil, um workspace ou um projeto. Muda também a barra lateral."))
        };
        let mut escolha = None;
        let varios_workspaces = workspaces.len() > 1;
        // O menu vai até a borda de baixo da janela (menos uma margem); se
        // ainda não couber, a barra de rolagem sólida fica sempre à vista e
        // as linhas param antes dela.
        let altura_menu = (ui.ctx().content_rect().bottom() - resposta.rect.bottom() - 40.0).max(160.0);
        egui::Popup::menu(&resposta).id(id_menu).show(|ui| {
            ui.set_min_width(260.0);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            ui.spacing_mut().scroll.bar_width = 6.0;
            egui::ScrollArea::vertical().max_height(altura_menu).show(ui, |ui| {
                // A largura final já vale na primeira linha (a bolinha do marcado fica no mesmo x).
                ui.set_min_width(ui.available_width().clamp(240.0, 260.0));
                let n = projetos.len();
                if tema::opcao_menu_arvore(ui, "Todos os projetos", Some(&format!("· {n}")), 0, self.escopo == Escopo::Perfil) {
                    escolha = Some(Escopo::Perfil);
                }
                linha_do_menu(ui);
                for (ws, nome, lista) in &workspaces {
                    let nivel = if varios_workspaces && *ws != 0 {
                        let detalhe = if lista.len() == 1 { "· 1 projeto".to_string() } else { format!("· {} projetos", lista.len()) };
                        if tema::opcao_menu_arvore(ui, nome, Some(&detalhe), 0, self.escopo == Escopo::Workspace(*ws)) {
                            escolha = Some(Escopo::Workspace(*ws));
                        }
                        1
                    } else {
                        0
                    };
                    for p in lista {
                        if tema::opcao_menu_arvore(ui, &p.nome, None, nivel, self.escopo == Escopo::Projeto(p.id)) {
                            escolha = Some(Escopo::Projeto(p.id));
                        }
                    }
                }
            });
        });
        if let Some(e) = escolha {
            egui::Popup::close_id(ui.ctx(), id_menu);
            if e != self.escopo {
                self.mudar_escopo(e);
            }
        }
        ui.add_space(8.0);
    }

    /// Liga ou desliga o tempo dos agentes: muda aqui na hora (o registro
    /// esconde na hora ao desligar) e grava no núcleo numa thread. Se o núcleo
    /// recusar, a caixa volta como estava e um aviso diz por quê.
    fn mudar_tempo(&mut self, ctx: &egui::Context, mostrar: bool) {
        let Some(perfil) = self.perfil.as_mut() else { return };
        if perfil.tempo_agentes == mostrar {
            return;
        }
        let antes = perfil.tempo_agentes;
        perfil.tempo_agentes = mostrar;
        let (envio, recebe) = mpsc::channel();
        let (id, ctx) = (perfil.id, ctx.clone());
        std::thread::spawn(move || {
            let _ = envio.send(api::definir_tempo_agentes(id, mostrar));
            ctx.request_repaint();
        });
        // Um pedido anterior ainda sem resposta fica com o valor de antes dele.
        let antes = self.tempo_pendente.take().map_or(antes, |(a, _)| a);
        self.tempo_pendente = Some((antes, recebe));
    }

    /// Recolhe ou abre um workspace na barra lateral: muda aqui na hora e
    /// grava no núcleo numa thread (as outras telas recebem pelo evento). Se
    /// não gravar, só volta aberto na próxima vez que a Colmeia abrir.
    fn recolher_workspace(&mut self, workspace: i64, recolhido: bool) {
        for p in self.modelo.projetos.iter_mut().filter(|p| p.workspace_id == workspace) {
            p.recolhido = recolhido;
        }
        if !self.demo {
            std::thread::spawn(move || {
                if let Err(e) = api::recolher_workspace(workspace, recolhido) {
                    eprintln!("colmeia: não consegui guardar o workspace recolhido: {e}");
                }
            });
        }
    }

    /// A resposta do núcleo à opção do tempo: em erro, desfaz e avisa.
    fn receber_tempo(&mut self, agora: f64) {
        let Some((antes, recebe)) = &self.tempo_pendente else { return };
        let resultado = match recebe.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("sem resposta do núcleo".into()),
        };
        let antes = *antes;
        self.tempo_pendente = None;
        if let Err(e) = resultado {
            if let Some(p) = self.perfil.as_mut() {
                p.tempo_agentes = antes;
            }
            self.avisar(TipoAviso::Erro, format!("Não consegui salvar a opção do tempo: {e}"), agora);
        }
    }

    /// Perfil sem projeto: um convite para adicionar o primeiro.
    fn sem_projetos(&mut self, ui: &mut egui::Ui) {
        let pode = self.pode_mudar();
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.25);
            ui.allocate_ui(egui::vec2(460.0, 0.0), |ui| {
                tema::moldura_janela().show(ui, |ui| {
                    ui.set_width(412.0);
                    tema::cabecalho(ui, "Nenhum projeto ainda", "Adicione um repositório git ou uma pasta de trabalho para começar a organizar as tarefas.");
                    ui.add_space(16.0);
                    if tema::botao_principal(ui, "Adicionar projeto", pode).clicked()
                        && let Some(perfil) = &self.perfil
                    {
                        self.dialogo = Some(Dialogo::NovoProjeto(dialogos::NovoProjeto::new(perfil.id)));
                    }
                });
            });
        });
    }

    fn lateral(&mut self, ui: &mut egui::Ui, agora: f64, linha: &str) -> (Option<egui::Rect>, bool) {
        let p = cores();
        let rodando = !self.demo || self.carga != "parada";
        let (mut caixa_abelha, mut abelha_clicada) = (None, false);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
            tema::logo(ui.painter(), r.center(), 11.0);
            ui.label(texto_forte("Colmeia", 17.0).color(p.texto));
        });
        ui.add_space(18.0);

        // Perfil: trocar, criar ou sair.
        let nome = self.perfil.as_ref().map(|p| p.nome.clone()).unwrap_or_default();
        let resposta = tema::chip(ui, "Perfil", &nome, false);
        // A lista é lida ao abrir o menu: um perfil criado em outro lugar aparece.
        if resposta.clicked() && !self.demo {
            self.perfis = api::perfis().unwrap_or_else(|_| self.perfis.clone());
        }
        if !self.demo {
            let mut ir_para: Option<Tela> = None;
            let mut trocar: Option<api::Perfil> = None;
            egui::Popup::menu(&resposta).show(|ui| {
                ui.set_min_width(220.0);
                for outro in &self.perfis {
                    let atual = self.perfil.as_ref().is_some_and(|p| p.id == outro.id);
                    if tema::opcao_menu(ui, &outro.nome, atual) {
                        if !atual {
                            trocar = Some(outro.clone());
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if tema::opcao_menu(ui, "Criar perfil…", false) {
                    ir_para = Some(Tela::Entrada(Box::new(entrada::Entrada::new(true))));
                    ui.close();
                }
                if tema::opcao_menu(ui, "Sair do perfil", false) {
                    ir_para = Some(Tela::Entrada(Box::new(entrada::Entrada::new(false))));
                    ui.close();
                }
            });
            if let Some(perfil) = trocar {
                self.entrar(perfil, ui.ctx());
            }
            if let Some(tela) = ir_para {
                self.perfil = None;
                self.ouvinte = None;
                self.tela = tela;
                return (None, false);
            }
        }
        ui.add_space(14.0);

        if item_lateral(ui, "Todos os projetos", self.escopo == Escopo::Perfil && !matches!(self.tela, Tela::Lousa(_) | Tela::Banco), None, false, None, 0.0)
            .0
            .clicked()
        {
            self.mudar_escopo(Escopo::Perfil);
        }
        if !self.demo {
            ui.add_space(2.0);
            let (resposta, _) = item_lateral(ui, "Bancos de dados", matches!(self.tela, Tela::Banco), None, false, Some(tema::Icone::Banco), 0.0);
            // O contador dos pedidos de consulta esperando você, no lugar do ponto.
            let pedidos = self.modelo.aprovacoes.len();
            if pedidos > 0 {
                tema::contador(ui.painter(), resposta.rect.right_center() - egui::vec2(14.0, 0.0), pedidos);
            }
            if resposta.on_hover_text("Bancos de dados (Ctrl+Shift+K)").clicked() {
                self.alternar_banco(true);
            }
        }
        ui.add_space(8.0);
        let mut workspace_anterior = None;
        let mut mudar = None;
        let mut mudar_workspace = None;
        let mut remover = None;
        let mut abrir_lousa = None;
        let mut recolher = None;
        let pode = self.pode_mudar();
        let lousa_aberta = if let Tela::Lousa(ws) = self.tela { Some(ws) } else { None };
        let fora_do_escopo = matches!(self.tela, Tela::Lousa(_) | Tela::Banco);
        // A lista rola quando não cabe acima da abelha e do tema (janela
        // baixa, muitos projetos): a abelha nunca cobre um projeto.
        let altura_lista = (ui.available_height() - RESERVA_ABELHA).max(96.0);
        // Barra de rolagem sólida: aparece sempre que há mais embaixo (não só
        // com o mouse em cima) e reserva a largura dela, sem cobrir o realce,
        // a contagem nem o ponto de estado dos itens.
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        ui.spacing_mut().scroll.bar_width = 6.0;
        egui::ScrollArea::vertical().id_salt("lateral-projetos").max_height(altura_lista).auto_shrink([false, true]).show(ui, |ui| {
            for projeto in &self.modelo.projetos {
                // Recolhido, o workspace esconde a lousa e os projetos, menos o que está aberto.
                let recolhido = projeto.recolhido && projeto.workspace_id != 0;
                if workspace_anterior != Some((projeto.workspace_id, &projeto.workspace)) {
                    // Uma linha fina separa um workspace do anterior.
                    if workspace_anterior.is_some() {
                        ui.add_space(4.0);
                        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                        ui.painter().hline(r.x_range().shrink(8.0), r.center().y, Stroke::new(1.0, p.borda));
                    }
                    ui.add_space(6.0);
                    workspace_anterior = Some((projeto.workspace_id, &projeto.workspace));
                    // O nome do workspace abre todos os projetos dele (quadro, daily e sprint juntos).
                    if projeto.workspace_id != 0 {
                        let ativo = self.escopo == Escopo::Workspace(projeto.workspace_id) && !fora_do_escopo;
                        let do_workspace = |id: i64| self.modelo.projetos.iter().any(|x| x.id == id && x.workspace_id == projeto.workspace_id);
                        let n = self.modelo.projetos.iter().filter(|x| x.workspace_id == projeto.workspace_id).count();
                        // O estado mais urgente das tarefas do workspace, para o ponto do recolhido.
                        let tarefas = || self.modelo.tarefas.iter().filter(|t| do_workspace(t.projeto_id));
                        let ponto = recolhido
                            .then(|| {
                                let estado = abelha::estado_base(tarefas(), rodando);
                                let concluiu = self.abelha.conclusoes.iter().any(|c| do_workspace(c.projeto_id) && agora - c.em < abelha::CONCLUSAO_RECENTE);
                                abelha::cor_ponto(estado, tarefas().any(|t| t.erro.is_some()), concluiu)
                            })
                            .flatten();
                        let (resposta, seta) = cabecalho_lateral(ui, &projeto.workspace, ativo, n, recolhido, ponto);
                        if resposta.on_hover_text(format!("Todos os projetos de {} (daily, sprint e quadro juntos)", projeto.workspace)).clicked() {
                            mudar_workspace = Some(projeto.workspace_id);
                        }
                        if seta.clicked() {
                            recolher = Some((projeto.workspace_id, !recolhido));
                        }
                    } else {
                        ui.label(RichText::new(&projeto.workspace).color(p.suave).size(11.5));
                    }
                    // A lousa do workspace, antes dos projetos (fora da demonstração).
                    if !self.demo && projeto.workspace_id != 0 && !recolhido {
                        ui.add_space(2.0);
                        let dica = format!("Lousa do workspace {}: notas, código, imagens e ligações", projeto.workspace);
                        if item_lateral(ui, "Lousa", lousa_aberta == Some(projeto.workspace_id), None, false, Some(tema::Icone::Lousa), RECUO_WORKSPACE)
                            .0
                            .on_hover_text(dica)
                            .clicked()
                        {
                            abrir_lousa = Some(projeto.workspace_id);
                        }
                    }
                }
                let ativo = self.escopo == Escopo::Projeto(projeto.id) && lousa_aberta.is_none() && !matches!(self.tela, Tela::Banco);
                if recolhido && self.escopo != Escopo::Projeto(projeto.id) {
                    continue;
                }
                let estado = abelha::estado_base(self.modelo.tarefas.iter().filter(|t| t.projeto_id == projeto.id), rodando);
                let concluiu = self.abelha.conclusoes.iter().any(|c| c.projeto_id == projeto.id && agora - c.em < abelha::CONCLUSAO_RECENTE);
                let tem_erro = self.modelo.tarefas.iter().any(|t| t.projeto_id == projeto.id && t.erro.is_some());
                let (resposta, mais) = item_lateral(
                    ui,
                    &projeto.nome,
                    ativo,
                    abelha::cor_ponto(estado, tem_erro, concluiu),
                    !self.demo,
                    None,
                    if projeto.workspace_id != 0 { RECUO_WORKSPACE } else { 0.0 },
                );
                let resposta = if projeto.caminho.is_empty() { resposta } else { resposta.on_hover_text(&projeto.caminho) };
                if resposta.clicked() {
                    mudar = Some(projeto.id);
                }
                if self.demo {
                    continue;
                }
                let menu = |ui: &mut egui::Ui, remover: &mut Option<(i64, String)>| {
                    ui.set_min_width(240.0);
                    if tema::opcao_menu_com(ui, "Remover da Colmeia…", None, pode) {
                        *remover = Some((projeto.id, projeto.nome.clone()));
                        ui.close();
                    }
                };
                resposta.context_menu(|ui| menu(ui, &mut remover));
                if let Some(mais) = mais {
                    egui::Popup::menu(&mais).show(|ui| menu(ui, &mut remover));
                }
            }
        });
        if let Some(id) = mudar {
            self.mudar_escopo(Escopo::Projeto(id));
        }
        if let Some((ws, recolhido)) = recolher {
            self.recolher_workspace(ws, recolhido);
            // A lista já foi desenhada neste quadro: sem pedir outro, os
            // projetos só somem quando o mouse mexer.
            ui.ctx().request_repaint();
        }
        if let Some(ws) = mudar_workspace {
            // Da lousa ou dos bancos, o workspace abre no quadro.
            if fora_do_escopo {
                self.tela = Tela::Quadro;
            }
            self.mudar_escopo(Escopo::Workspace(ws));
        }
        if let Some(ws) = abrir_lousa {
            self.abrir_lousa_do_workspace(ws);
        }
        if let Some((id, nome)) = remover {
            self.dialogo = Some(Dialogo::RemoverProjeto { id, nome, erro: None });
        }
        ui.add_space(6.0);
        if !self.demo
            && pode
            && item_lateral(ui, "+ Novo projeto", false, None, false, None, 0.0).0.clicked()
            && let Some(perfil) = &self.perfil
        {
            self.dialogo = Some(Dialogo::NovoProjeto(dialogos::NovoProjeto::new(perfil.id)));
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            let atual = tema::Escolha::TODAS.iter().position(|e| *e == self.tema).unwrap_or(0);
            let largura = ui.available_width();
            if let Some(i) = tema::segmentado_com_largura(ui, &tema::Escolha::TODAS.map(|e| e.nome()), atual, Some(largura)) {
                self.tema = tema::Escolha::TODAS[i];
                self.tema.aplicar(ui.ctx());
                if !self.demo
                    && let Some(perfil) = &mut self.perfil
                {
                    perfil.tema = self.tema.chave().into();
                    let _ = api::definir_tema(perfil.id, self.tema.chave());
                }
            }
            ui.add_space(10.0);
            // COLMEIA_SEM_ABELHA=1 desliga a abelha, como a opção por perfil.
            if !self.sem_abelha {
                let resposta = self.abelha.mostrar(ui, agora, linha);
                if resposta.clicked() {
                    self.abelha.resumo_aberto = !self.abelha.resumo_aberto;
                    abelha_clicada = true;
                }
                caixa_abelha = Some(resposta.rect);
            }
        });
        (caixa_abelha, abelha_clicada)
    }

    fn painel_tarefa(&mut self, ui: &mut egui::Ui, id: i64, foco: i64, agora: f64) {
        let Some(tarefa) = self.modelo.tarefas.iter().find(|t| t.id == id) else {
            self.tela = Tela::Quadro;
            return;
        };
        // Abrir a tarefa mostra o agente (ou o estado vazio): a lousa não
        // fica ligada da última visita. Só a linha do tempo a abre junto.
        if self.tarefa_na_tela != Some(id) {
            self.tarefa_na_tela = Some(id);
            self.lousa_na_tarefa.remove(&id);
        }
        let agentes = tarefa.agentes.clone();
        let pasta = tarefa.pasta.clone();
        let p = cores();
        let pode = self.pode_mudar();
        let (mut voltar, mut novo_agente, mut abrir_editor) = (false, false, false);
        let (mut alternar_gaveta, mut clique_navegador, mut menu_navegador) = (false, None, None);
        let mut alternar_lousa = false;
        let gaveta_aberta = self.gaveta == Some(id);
        let lousa_aberta = self.lousa_na_tarefa.contains(&id);
        let novidade_na_lousa = self.novidade_na_lousa.contains(&id);
        let navegador_aberto = self.modelo.navegadores.contains(&id);
        let volta_para_lousa = self.voltar_para_lousa;
        ui.horizontal(|ui| {
            voltar = if volta_para_lousa.is_some() {
                tema::botao_secundario(ui, "‹ Lousa").on_hover_text("Voltar à lousa (Ctrl+Esc)").clicked()
            } else {
                tema::botao_secundario(ui, "‹ Quadro").on_hover_text("Voltar ao quadro (Ctrl+Esc)").clicked()
            };
            ui.add_space(8.0);
            // O título é o que se corta: os botões da direita, o número, a
            // branch e a etiqueta de estado nunca. O título inteiro fica na dica.
            // O espaço dos botões é medido com as peças de verdade (texto +
            // margens + o vão de 6 + o espaçamento do egui de cada uma; o
            // add_space não leva espaçamento).
            let espaco = ui.spacing().item_spacing.x;
            let corpo = egui::TextStyle::Body.resolve(ui.style());
            let pintor = ui.painter().clone();
            let medir = |t: &str, fonte: egui::FontId| pintor.layout_no_wrap(t.to_owned(), fonte, p.texto).size().x;
            // Uma folga de 2 px contra arredondamento.
            let mut reservado = 2.0;
            let mut reservado_editor = 0.0;
            if !self.demo {
                // "+ Agente" sempre: com ou sem agente, é o caminho para abrir um.
                reservado += medir("+ Agente", tema::forte(13.5)) + 32.0 + 6.0 + espaco;
                reservado += tema::largura_botao_dividido(&pintor, "Navegador") + 6.0 + espaco;
                // Os chips medem sempre em negrito (a largura não muda ao ligar).
                reservado += medir("Arquivos", tema::forte(13.0)) + 28.0 + espaco;
                reservado += medir("Lousa", tema::forte(13.0)) + 28.0 + 6.0 + espaco;
                if let Some((nome, _)) = self.editor {
                    reservado_editor = medir(&format!("Abrir no {nome}"), egui::FontId::proportional(13.0)) + 28.0 + 6.0 + espaco;
                }
            }
            reservado += medir(&format!("#{}", tarefa.id), corpo.clone()) + espaco;
            let branch = if tarefa.branch.is_empty() { "pasta" } else { tarefa.branch.as_str() };
            reservado += medir(branch, egui::FontId::monospace(11.0)) + 12.0 + espaco;
            if tarefa.em_copia && !self.demo {
                reservado += medir("cópia isolada", corpo.clone()) + espaco;
            }
            reservado += medir(tema::rotulo_coluna(tarefa.coluna), tema::fonte_etiqueta()) + 12.0 + espaco;
            if tarefa.erro.is_some() {
                reservado += medir("Erro", tema::fonte_etiqueta()) + 12.0 + espaco;
            }
            // Abaixo de 160 para o título, "Abrir no IntelliJ" vira "IntelliJ".
            let editor_curto = self.editor.is_some() && ui.available_width() - reservado - reservado_editor < TITULO_MINIMO;
            if let Some((nome, _)) = self.editor
                && editor_curto
            {
                reservado_editor = medir(nome, egui::FontId::proportional(13.0)) + 28.0 + 6.0 + espaco;
            }
            reservado += reservado_editor;
            let largura_titulo = (ui.available_width() - reservado).max(80.0);
            ui.allocate_ui(egui::vec2(largura_titulo, 24.0), |ui| {
                ui.add(egui::Label::new(texto_forte(&tarefa.titulo, 16.0).color(p.texto)).truncate());
            });
            ui.label(RichText::new(format!("#{}", tarefa.id)).color(p.suave));
            if tarefa.branch.is_empty() {
                tema::etiqueta_ui(ui, "pasta", egui::FontId::monospace(11.0), p.suave);
            } else {
                tema::etiqueta_ui(ui, &tarefa.branch, egui::FontId::monospace(11.0), p.destaque);
            }
            if tarefa.em_copia && !self.demo {
                ui.label(RichText::new("cópia isolada").color(p.suave)).on_hover_text(&tarefa.pasta);
            }
            tema::etiqueta_ui(ui, tema::rotulo_coluna(tarefa.coluna), tema::fonte_etiqueta(), tema::cor_coluna(tarefa.coluna));
            if let Some(erro) = &tarefa.erro {
                tema::etiqueta_ui(ui, "Erro", tema::fonte_etiqueta(), p.erro).on_hover_text(erro);
            }
            if self.demo {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                novo_agente = tema::botao_principal(ui, "+ Agente", pode).on_hover_text("Abrir um agente (Claude Code, Codex…) nesta tarefa").clicked();
                ui.add_space(6.0);
                let ponto = navegador_aberto.then_some(p.ok);
                let (principal, menu) = tema::botao_dividido(ui, "Navegador", ponto);
                let dica = if navegador_aberto { "Trazer o navegador da tarefa para frente" } else { "Abrir o navegador da tarefa ao lado da Colmeia" };
                // Os balões presos ao botão usam o retângulo dele (não o da
                // fileira, que vai até o "+ Agente").
                let rect_botao = menu.rect.union(principal.rect);
                if principal.on_hover_text(dica).clicked() {
                    clique_navegador = Some(rect_botao);
                }
                egui::Popup::menu(&menu).show(|ui| {
                    ui.set_min_width(250.0);
                    if tema::opcao_menu_com(ui, "Ir para endereço…", None, pode) {
                        menu_navegador = Some(("endereco", rect_botao));
                        ui.close();
                    }
                    if tema::opcao_menu_com(ui, "Capturar navegador", Some("Ctrl+Shift+B"), pode && navegador_aberto) {
                        menu_navegador = Some(("capturar", rect_botao));
                        ui.close();
                    }
                    ui.add_space(6.0);
                    if tema::opcao_menu_com(ui, "Fechar navegador", None, pode && navegador_aberto) {
                        menu_navegador = Some(("fechar", rect_botao));
                        ui.close();
                    }
                });
                ui.add_space(6.0);
                alternar_gaveta = tema::chip_alternar(ui, "Arquivos", gaveta_aberta).on_hover_text("Arquivos da pasta da tarefa (Ctrl+Shift+E)").clicked();
                ui.add_space(6.0);
                let chip = tema::chip_alternar(ui, "Lousa", lousa_aberta);
                if novidade_na_lousa {
                    // O agente acrescentou itens desde a última vez que você viu a lousa.
                    let centro = chip.rect.right_top() + egui::vec2(-7.0, 7.0);
                    ui.painter().circle(centro, 3.5, p.destaque, Stroke::new(1.5, p.superficie_alta));
                }
                let dica = if novidade_na_lousa { "Lousa da tarefa (Ctrl+Shift+Q) · o agente acrescentou itens" } else { "Lousa da tarefa (Ctrl+Shift+Q)" };
                alternar_lousa = chip.on_hover_text(dica).clicked();
                if let Some((nome, _)) = self.editor {
                    ui.add_space(6.0);
                    let rotulo = if editor_curto { nome.to_string() } else { format!("Abrir no {nome}") };
                    abrir_editor = tema::botao_secundario(ui, &rotulo).on_hover_text(format!("Abrir no {nome}: {pasta}")).clicked();
                }
            });
        });
        ui.add_space(8.0);
        self.pedidos_de_consulta(ui, id, &agentes, agora);
        if abrir_editor
            && let Some((nome, comando)) = self.editor
            && let Err(e) = sistema::abrir_com(comando, &pasta)
        {
            self.erro(format!("Não consegui abrir o {nome}: {e}"), agora);
        }
        if alternar_gaveta {
            self.alternar_gaveta(id, ui.ctx());
        }
        if alternar_lousa {
            self.alternar_lousa_da_tarefa(id);
        }
        if let Some(botao) = clique_navegador {
            self.clique_navegador(id, botao, ui.ctx(), agora);
        }
        match menu_navegador {
            Some(("endereco", botao)) => self.pedir_endereco(id, botao),
            Some(("capturar", _)) => self.capturar_navegador(id, ui.ctx()),
            Some(("fechar", _)) => {
                let envio = self.respostas_navegador.0.clone();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let _ = envio.send(RespostaNavegador::Fechado(api::fechar_navegador(id)));
                    ctx.request_repaint();
                });
            }
            _ => {}
        }
        if novo_agente {
            self.adicionar_agente(id, false);
        }
        if voltar || ui.input(|i| i.key_pressed(Key::Escape) && i.modifiers.ctrl) {
            self.tela = match self.voltar_para_lousa.take() {
                Some(ws) => Tela::Lousa(ws),
                None => Tela::Quadro,
            };
            return;
        }
        let dono = api::DonoLousa { workspace_id: 0, tarefa_id: id };
        if agentes.is_empty() && self.lousa_na_tarefa.contains(&id) {
            // Sem agente, a lousa fica abaixo de uma faixa que mantém à vista
            // como abrir um (e como voltar: "Fechar lousa").
            let tudo = ui.available_rect_before_wrap();
            let faixa = egui::Rect::from_min_size(tudo.min, egui::vec2(tudo.width(), 48.0));
            self.faixa_sem_agentes(ui, faixa, id);
            let area = egui::Rect::from_min_max(egui::pos2(tudo.left(), faixa.bottom() + 8.0), tudo.max);
            let acoes = self.mostrar_lousa(ui, area, dono, agora);
            self.tratar_lousa(dono, acoes, ui.ctx(), agora);
            if self.gaveta == Some(id) {
                self.mostrar_gaveta(ui.ctx(), id, area, area.width(), &pasta, None, agora);
            }
            return;
        }
        if agentes.is_empty() {
            // Onde o terminal do primeiro agente vai aparecer: a área toda menos
            // a caixa de mensagem e o cabeçalho do cartão do agente.
            let area = ui.available_rect_before_wrap();
            let terminal = egui::Rect::from_min_max(area.min + egui::vec2(1.5, 40.0), area.max - egui::vec2(1.5, self.compositor.altura() + 10.0 + 8.0));
            terminal::estimar_em_foco(ui, terminal, 13.0);
            self.sem_agentes(ui, id, &pasta);
            if self.gaveta == Some(id) {
                self.mostrar_gaveta(ui.ctx(), id, area, area.width(), &pasta, None, agora);
            }
            return;
        }
        let foco = if agentes.iter().any(|a| a.id == foco) { foco } else { agentes[0].id };

        let area = ui.available_rect_before_wrap();
        self.topo_terminal = Some(area.top());
        let outros: Vec<AgenteTela> = agentes.iter().filter(|a| a.id != foco).cloned().collect();
        let em_foco = agentes.iter().find(|a| a.id == foco).cloned().expect("o foco é um dos agentes");
        let largura_lateral = if outros.is_empty() { 0.0 } else { (area.width() * 0.3).max(280.0) };
        let coluna = egui::Rect::from_min_max(area.min, egui::pos2(area.max.x - largura_lateral - if outros.is_empty() { 0.0 } else { 10.0 }, area.max.y));
        let principal = egui::Rect::from_min_max(coluna.min, egui::pos2(coluna.max.x, coluna.max.y - self.compositor.altura() - 10.0));
        // O aviso do rodapé fica sobre o terminal, sem cobrir a caixa de mensagem.
        self.ancora_aviso = Some(principal.center_bottom() - egui::vec2(0.0, 16.0));
        let mut pedidos = vec![self.caixa_terminal(ui, principal, &em_foco, id, true, 13.0, agora)];
        // A lousa cobre o corpo do terminal em foco (abaixo do cabeçalho do
        // agente), sem mudar o tamanho dele: a caixa de mensagem continua.
        if let Some(area) = self.area_lousa.take() {
            let acoes = self.mostrar_lousa(ui, area, dono, agora);
            self.tratar_lousa(dono, acoes, ui.ctx(), agora);
        }
        // A gaveta de arquivos fica por cima do terminal em foco, sem mudar o tamanho dele.
        if self.gaveta == Some(id) {
            // 2 px para dentro: a borda de foco do terminal continua inteira.
            let area_gaveta = principal.shrink(2.0);
            self.mostrar_gaveta(ui.ctx(), id, area_gaveta, coluna.width(), &pasta, Some(foco), agora);
        }
        let caixa_mensagem = egui::Rect::from_min_max(egui::pos2(coluna.min.x, principal.max.y + 10.0), coluna.max);
        // Com um diálogo ou caixa por cima, o Enter é de quem está por cima.
        let teclado_livre = self.dialogo.is_none()
            && !self.banco.modal_aberto()
            && self.caixa_pedido.is_none()
            && self.apresentacao.is_none()
            && self.palco.is_none()
            && self.popover_navegador.is_none();
        if let Some(envio) = self.compositor.mostrar(ui, caixa_mensagem, &agentes, foco, id, teclado_livre) {
            for agente in envio.destinos {
                if let Some(t) = self.terminais.get(&agente) {
                    t.enviar(&envio.texto);
                }
            }
        }

        // As miniaturas terminam na base da caixa de mensagem (a dica fica abaixo dela).
        let base = area.height() - compositor::ALTURA_DICA;
        let altura = if outros.is_empty() { 0.0 } else { (base - 10.0 * (outros.len() as f32 - 1.0)) / outros.len() as f32 };
        for (n, agente) in outros.iter().enumerate() {
            let min = egui::pos2(area.max.x - largura_lateral, area.min.y + n as f32 * (altura + 10.0));
            let caixa = egui::Rect::from_min_size(min, egui::vec2(largura_lateral, altura));
            pedidos.push(self.caixa_terminal(ui, caixa, agente, id, false, 10.0, agora));
        }
        for pedido in pedidos.into_iter().flatten() {
            match pedido {
                Pedido::Focar(agente) => self.tela = Tela::Tarefa { id, foco: agente },
                Pedido::Iniciar(agente) => self.iniciar_agente(agente, ui.ctx(), agora),
                Pedido::Remover(agente) => self.remover_agente(agente, agora),
                Pedido::Capturar(agente) => self.capturar(agente, id),
                Pedido::LimparHistorico(agente, guardadas) => {
                    let nome = agentes.iter().find(|a| a.id == agente).map(|a| a.nome_com_papel()).unwrap_or_default();
                    self.dialogo = Some(Dialogo::LimparHistorico { agente, nome, guardadas, erro: None });
                }
            }
        }
    }

    /// Os pedidos de consulta ao banco dos agentes da tarefa, entre o
    /// cabeçalho e o terminal; depois da decisão, a linha com o desfecho.
    fn pedidos_de_consulta(&mut self, ui: &mut egui::Ui, tarefa: i64, agentes: &[AgenteTela], agora: f64) {
        let pendentes: Vec<api::Aprovacao> = self.modelo.aprovacoes_da_tarefa(tarefa).cloned().collect();
        self.pedidos_consulta.retain(|id, _| self.modelo.aprovacoes.iter().any(|a| a.id == *id));
        let mut respostas = Vec::new();
        for a in &pendentes {
            let estado = self.pedidos_consulta.entry(a.id.clone()).or_insert_with(banco::aprovacao::Estado::novo);
            if let Some(r) = banco::aprovacao::cartao(ui, a, estado, self.banco.chaveiro) {
                respostas.push((a.id.clone(), r));
            }
            ui.add_space(8.0);
        }
        for agente in agentes {
            if pendentes.iter().any(|a| a.agente_id == agente.id) {
                continue;
            }
            if let Some(r) = self.modelo.resolvidas.get(&agente.id).filter(|r| r.tarefa == tarefa) {
                banco::aprovacao::resolvida(ui, r);
                ui.add_space(4.0);
            }
        }
        for (id, resposta) in respostas {
            if let Some(estado) = self.pedidos_consulta.get_mut(&id) {
                estado.enviando = true;
                estado.erro = None;
                estado.erro_senha = None;
            }
            let envio = self.respostas_pedido.0.clone();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let resultado = match resposta {
                    banco::aprovacao::Resposta::Aprovar { senha, guardar } => api::responder_aprovacao(&id, true, "", &senha, guardar),
                    banco::aprovacao::Resposta::Recusar { motivo } => api::responder_aprovacao(&id, false, &motivo, "", false),
                };
                let _ = envio.send((id, resultado));
                ctx.request_repaint();
            });
        }
        while let Ok((id, resultado)) = self.respostas_pedido.1.try_recv() {
            let Some(estado) = self.pedidos_consulta.get_mut(&id) else { continue };
            estado.enviando = false;
            match resultado {
                Ok(()) => {}
                // Senha errada: o pedido continua (mesmo prazo); o campo volta
                // vazio, com o foco, e o agente não fica sabendo.
                Err(e) if e.starts_with("Usuário ou senha recusados") => {
                    estado.senha.clear();
                    estado.erro_senha = Some(e);
                    estado.focar_senha = true;
                }
                Err(e) => {
                    estado.erro = Some(e.clone());
                    self.erro(format!("Não consegui responder o pedido: {e}"), agora);
                }
            }
        }
    }

    /// A faixa sobre a lousa de uma tarefa sem agente: o caminho para abrir
    /// um continua à vista, e "Fechar lousa" volta ao estado vazio.
    fn faixa_sem_agentes(&mut self, ui: &mut egui::Ui, faixa: egui::Rect, id: i64) {
        let p = cores();
        let tem_claude = !self.demo && self.opcoes_de_agente().iter().any(|o| o.id == "claude");
        let pode = self.pode_mudar();
        let (mut novo, mut retomar, mut fechar) = (false, false, false);
        ui.painter().rect(faixa, CornerRadius::same(tema::RAIO_SUPERFICIE), p.superficie_alta, Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
        let mut filho =
            ui.new_child(egui::UiBuilder::new().max_rect(faixa.shrink2(egui::vec2(14.0, 8.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        filho.label(RichText::new("Esta tarefa ainda não tem agentes.").color(p.texto).size(13.0));
        if !self.demo {
            filho.add_space(8.0);
            novo = tema::botao_principal(&mut filho, "Adicionar agente", pode).clicked();
            if tem_claude {
                retomar = tema::botao_secundario_com(&mut filho, "Retomar conversa do Claude Code", pode).clicked();
            }
        }
        filho.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            fechar = tema::botao_secundario(ui, "Fechar lousa").on_hover_text("Voltar à tarefa (Ctrl+Shift+Q)").clicked();
        });
        if fechar {
            self.alternar_lousa_da_tarefa(id);
        }
        if novo || retomar {
            self.adicionar_agente(id, retomar);
        }
    }

    /// Tarefa sem agentes: abrir um novo ou retomar uma conversa do Claude Code
    /// começada em outro lugar (no IntelliJ, por exemplo) na mesma pasta.
    fn sem_agentes(&mut self, ui: &mut egui::Ui, id: i64, pasta: &str) {
        let tem_claude = !self.demo && self.opcoes_de_agente().iter().any(|o| o.id == "claude");
        let pode = self.pode_mudar();
        let (mut novo, mut retomar) = (false, false);
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.2);
            ui.allocate_ui(egui::vec2(520.0, 0.0), |ui| {
                tema::moldura_janela().show(ui, |ui| {
                    ui.set_width(472.0);
                    let texto = if self.demo {
                        "Na demonstração, os agentes são terminais de teste.".to_string()
                    } else {
                        format!("Para falar com o Claude, adicione um agente: o terminal dele abre aqui, em {pasta}, com a caixa de mensagem embaixo. Se você já conversava com o Claude Code nessa pasta, dá para continuar a mesma conversa.")
                    };
                    tema::cabecalho(ui, "Esta tarefa ainda não tem agentes", &texto);
                    if self.demo {
                        return;
                    }
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        novo = tema::botao_principal(ui, "Adicionar agente", pode).clicked();
                        if tem_claude {
                            ui.add_space(8.0);
                            retomar = tema::botao_secundario_com(ui, "Retomar conversa do Claude Code", pode).clicked();
                        }
                    });
                });
            });
        });
        if novo || retomar {
            self.adicionar_agente(id, retomar);
        }
    }

    /// Desenha a caixa de um agente: cabeçalho com estado e menu, e o terminal.
    /// Um agente parado mostra o que ficou na tela e um cartão com o fim e os botões.
    #[allow(clippy::too_many_arguments)]
    fn caixa_terminal(
        &mut self,
        ui: &mut egui::Ui,
        caixa: egui::Rect,
        agente: &AgenteTela,
        tarefa: i64,
        focado: bool,
        fonte: f32,
        agora: f64,
    ) -> Option<Pedido> {
        let p = cores();
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa));
        let mut pedido = None;
        let demo = self.demo;
        let pode = self.pode_mudar();
        let visual = if demo { EstadoVisual::Trabalhando } else { agente.visual() };
        let parado = !demo && !agente.ativo;
        // A borda acompanha o selo: numa tarefa com vários agentes, dá para ver qual precisa de você.
        // Uma borda de destaque por vez: com o teclado na caixa de mensagem, o
        // terminal em foco fica só marcado de leve.
        let teclado_no_compositor = focado && Compositor::com_foco(ui.ctx());
        let borda = match visual {
            _ if teclado_no_compositor => Stroke::new(1.0, p.destaque.gamma_multiply(0.5)),
            _ if focado => Stroke::new(1.5, p.destaque),
            EstadoVisual::PedeAprovacao | EstadoVisual::PedeConsulta | EstadoVisual::SuaVez => Stroke::new(1.5, p.alerta),
            EstadoVisual::Erro => Stroke::new(1.5, p.erro),
            _ => Stroke::new(1.0, p.borda),
        };
        let resposta_caixa = egui::Frame::new()
            .fill(p.superficie_alta)
            .stroke(borda)
            .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
            .inner_margin(egui::Margin::symmetric(0, 6))
            .show(&mut filho, |ui| {
                // A moldura soma a margem de 6 em cima e embaixo e a borda: sem
                // descontar, a caixa passa 12 px da área e cobre o vão até a
                // caixa de mensagem.
                ui.set_min_size(caixa.size() - egui::vec2(3.0, 15.0));
                let cabecalho = ui.horizontal(|ui| {
                    ui.set_height(28.0);
                    ui.add_space(10.0);
                    let (ponto, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    tema::ponto(ui.painter(), ponto.center(), 3.5, visual);
                    ui.label(texto_forte(agente.nome(), 13.5).color(p.texto));
                    ui.label(RichText::new(format!("· {}", agente.papel)).color(p.suave));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        if !demo {
                            let mais = tema::botao_icone(ui, tema::Icone::Mais, 24.0).on_hover_text("Ações do agente");
                            egui::Popup::menu(&mais).show(|ui| {
                                ui.set_min_width(240.0);
                                let pode_capturar = focado && pode && !self.lousa_na_tarefa.contains(&tarefa);
                                let item = tema::opcao_menu_com(ui, "Capturar terminal", Some("Ctrl+Shift+S"), pode_capturar);
                                if !focado {
                                    ui.label(RichText::new("Foque o terminal para capturar").color(p.suave).size(11.5));
                                }
                                if item {
                                    pedido = Some(Pedido::Capturar(agente.id));
                                    ui.close();
                                }
                                let guardadas = self.compositor.tamanho_historico(agente.id);
                                if tema::opcao_menu_com(ui, "Limpar histórico de mensagens", None, pode && guardadas > 0) {
                                    pedido = Some(Pedido::LimparHistorico(agente.id, guardadas));
                                    ui.close();
                                }
                                if tema::opcao_menu_com(ui, "Parar e remover agente", None, pode) {
                                    pedido = Some(Pedido::Remover(agente.id));
                                    ui.close();
                                }
                            });
                            ui.add_space(6.0);
                        }
                        tema::etiqueta_ui(ui, visual.selo(), tema::fonte_etiqueta(), visual.cor()).on_hover_text(if focado {
                            agente.texto_estado()
                        } else {
                            format!("{} · clique para focar", agente.texto_estado())
                        });
                    });
                });
                if !demo && pode {
                    cabecalho.response.interact(egui::Sense::click()).context_menu(|ui| {
                        ui.set_min_width(220.0);
                        if tema::opcao_menu(ui, "Parar e remover agente", false) {
                            pedido = Some(Pedido::Remover(agente.id));
                            ui.close();
                        }
                    });
                }
                let lousa_por_cima = focado && self.lousa_na_tarefa.contains(&tarefa);
                match self.terminais.get_mut(&agente.id) {
                    // A lousa cobre o terminal: ele não é desenhado (nem recebe o teclado).
                    _ if lousa_por_cima => {
                        let (r, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                        // A linha de 1 px em cima separa a lousa do cabeçalho do agente.
                        ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, p.borda));
                        self.area_lousa = Some(egui::Rect::from_min_max(r.min + egui::vec2(0.0, 1.0), r.max));
                    }
                    Some(t) => {
                        if t.mostrar(ui, fonte, focado).clicked() && !focado {
                            pedido = Some(Pedido::Focar(agente.id));
                        }
                    }
                    None => {
                        let resposta = ui.allocate_response(ui.available_size(), egui::Sense::click());
                        if resposta.clicked() && !focado {
                            pedido = Some(Pedido::Focar(agente.id));
                        }
                    }
                }
                // Com a gaveta de arquivos por cima, o cartão de fim sairia em
                // tirinha ao lado dela: fica escondido até a gaveta fechar.
                if parado && !(focado && (self.gaveta == Some(tarefa) || lousa_por_cima)) {
                    let centro = ui.min_rect().center();
                    if let Some(p) = cartao_de_fim(ui, centro, agente, focado, pode) {
                        pedido = Some(p);
                    }
                }
            });
        // Clarão de 0,3 s na borda do terminal capturado: o único redesenho seguido da captura.
        if let Some((capturado, inicio)) = self.clarao
            && capturado == agente.id
        {
            let t = ((agora - inicio) / CLARAO) as f32;
            if t < 1.0 {
                let cor = p.texto.gamma_multiply(1.0 - t);
                ui.painter().rect_stroke(
                    resposta_caixa.response.rect,
                    CornerRadius::same(tema::RAIO_SUPERFICIE),
                    Stroke::new(3.0, cor),
                    egui::StrokeKind::Inside,
                );
                ui.ctx().request_repaint();
            } else {
                self.clarao = None;
            }
        }
        let _ = tarefa;
        pedido
    }

    /// Aviso do rodapé: some sozinho; pede um único redesenho para o momento de sumir.
    fn mostrar_aviso(&mut self, ctx: &egui::Context, agora: f64) {
        let Some(aviso) = &self.aviso else {
            return;
        };
        if agora >= aviso.ate {
            self.aviso = None;
            return;
        }
        let ancora = self.ancora_aviso.unwrap_or_else(|| ctx.content_rect().center_bottom() - egui::vec2(0.0, 24.0));
        // O aviso sem prazo (o agente respondeu) fica até ser visto ou fechado.
        let fixo = aviso.ate.is_infinite();
        let clique = tema::aviso(ctx, ancora, aviso.tipo, &aviso.texto, aviso.acao.map(|(t, _)| t), fixo);
        if !fixo {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(aviso.ate - agora));
        }
        match clique {
            tema::CliqueAviso::Nada => return,
            tema::CliqueAviso::Fechar => {
                self.aviso = None;
                return;
            }
            tema::CliqueAviso::Acao => {}
        }
        let Some((_, acao)) = aviso.acao else { return };
        self.aviso = None;
        match acao {
            AcaoAviso::DesfazerCaptura(anexo) => match api::remover_anexo(anexo) {
                Ok(()) => self.avisar(TipoAviso::Neutro, "Captura desfeita", agora),
                Err(e) => self.erro(format!("Não consegui desfazer a captura: {e}"), agora),
            },
            AcaoAviso::Abrir { tarefa, agente } => self.abrir_tarefa(tarefa, Some(agente)),
            AcaoAviso::Lousa(dono, acao) => {
                let mut nao_desfez = false;
                if let Some(l) = self.lousas.get_mut(&dono) {
                    match acao {
                        AcaoLousa::Desfazer(numero) => nao_desfez = !l.desfazer_comando(ctx, numero),
                        AcaoLousa::Ver => l.ver_novos(),
                        AcaoLousa::Tentar => l.tentar_de_novo(ctx),
                    }
                }
                if nao_desfez {
                    self.avisar(TipoAviso::Alerta, "Não dá mais para desfazer", agora);
                }
            }
            AcaoAviso::VerAbelha => self.abelha.resumo_aberto = true,
            AcaoAviso::VerPedido(tarefa) => {
                let aba = match self.tela {
                    Tela::Registro(registro::Aba::Sprint) => registro::Aba::Sprint,
                    _ => registro::Aba::Daily,
                };
                if let Some(projeto) = self.modelo.tarefas.iter().find(|t| t.id == tarefa).map(|t| t.projeto_id)
                    && !self.escopo.contem(projeto, &self.modelo.projetos)
                {
                    self.mudar_escopo(Escopo::Projeto(projeto));
                }
                self.tela = Tela::Registro(aba);
                self.registro.marcar_suja();
                self.registro.rolar_ate = Some(tarefa);
            }
        }
    }

    /// Atalhos da Colmeia: Ctrl+Shift+letra, tirados da fila antes de o
    /// terminal ler (como nos terminais do GNOME).
    fn atalhos(&mut self, ctx: &egui::Context) {
        if self.dialogo.is_some() || self.demo || self.apresentacao.is_some() || self.palco.is_some() || self.banco.modal_aberto() {
            return;
        }
        let atalho = |tecla| ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, tecla));
        // Ctrl+Shift+K: os bancos de dados; de novo, volta para onde estava.
        if atalho(Key::K) {
            self.alternar_banco(false);
        }
        if matches!(self.tela, Tela::Banco) && ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Escape)) {
            self.tela = Tela::Quadro;
            self.tela_antes_do_banco = None;
        }
        if atalho(Key::L) {
            // Da lousa do workspace, a linha do tempo de todos os projetos (não
            // a do projeto que estava escolhido antes de abrir a lousa).
            if matches!(self.tela, Tela::Lousa(_)) {
                self.escopo = Escopo::Perfil;
                self.registro.marcar_suja();
            }
            self.tela = if matches!(self.tela, Tela::Registro(registro::Aba::Linha)) { Tela::Quadro } else { Tela::Registro(registro::Aba::Linha) };
        }
        if atalho(Key::D) {
            self.tela = Tela::Registro(registro::Aba::Daily);
            self.registro.abrir_daily();
        }
        // Na lousa do workspace, F5 apresenta a lousa (Shift+F5: do item selecionado).
        if let Tela::Lousa(ws) = self.tela {
            let dono = api::DonoLousa { workspace_id: ws, tarefa_id: 0 };
            let desde = self.lousas.get(&dono).and_then(|l| l.selecao.first().copied());
            if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F5)) {
                self.apresentar_lousa(dono, desde, ctx);
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
                self.apresentar_lousa(dono, None, ctx);
            }
        }
        // F5 apresenta a daily ou a sprint (no quadro e na linha do tempo, a
        // daily); Shift+F5 retoma do último slide visto. Fora do terminal em foco.
        if !matches!(self.tela, Tela::Tarefa { .. } | Tela::Lousa(_)) {
            if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F5)) {
                self.apresentar(ctx, Inicio::Retomar, true);
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
                self.apresentar(ctx, Inicio::Capa, true);
            }
        }
        if let Tela::Registro(registro::Aba::Sprint) = self.tela
            && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S))
        {
            self.registro.salvar_sprint(ctx);
        }
        if atalho(Key::P) {
            self.proximo_que_precisa();
        }
        if atalho(Key::S)
            && let Tela::Tarefa { id, foco } = self.tela
            && foco != 0
        {
            if self.lousa_na_tarefa.contains(&id) {
                // A captura sairia com a lousa por cima.
                let agora = ctx.input(|i| i.time);
                self.avisar(TipoAviso::Alerta, "Feche a lousa para capturar o terminal", agora);
            } else {
                self.capturar(foco, id);
            }
        }
        // Ctrl+Shift+E: a gaveta de arquivos; Ctrl+Shift+B: capturar o navegador.
        if let Tela::Tarefa { id, .. } = self.tela {
            if atalho(Key::Q) {
                self.alternar_lousa_da_tarefa(id);
            }
            if atalho(Key::E) {
                self.alternar_gaveta(id, ctx);
            }
            if atalho(Key::B) && self.modelo.navegadores.contains(&id) {
                self.capturar_navegador(id, ctx);
            }
        }
    }

    // Apresentação

    /// F5 e "Apresentar": a daily (ou a sprint, na página da sprint). O deck
    /// já carregado na página vai junto; senão, a apresentação busca.
    fn apresentar(&mut self, ctx: &egui::Context, inicio: Inicio, tela_cheia: bool) {
        let sprint = matches!(self.tela, Tela::Registro(registro::Aba::Sprint));
        let aba = if sprint { registro::Aba::Sprint } else { registro::Aba::Daily };
        if self.registro.sem_o_que_apresentar(aba) {
            return;
        }
        let (deck, periodo) = if sprint {
            match self.registro.periodo_sprint() {
                Some(periodo) => (self.registro.deck(registro::Aba::Sprint).cloned(), Some(periodo)),
                None => return,
            }
        } else {
            (self.registro.deck(registro::Aba::Daily).cloned(), None)
        };
        let inicio = match (inicio, self.ultimo_slide) {
            (Inicio::Retomar, Some((daily, tarefa))) if daily != sprint => Inicio::Tarefa(tarefa),
            (Inicio::Retomar, _) => Inicio::Capa,
            (outro, _) => outro,
        };
        self.abrir_apresentacao(ctx, deck, periodo, inicio, tela_cheia);
    }

    fn abrir_apresentacao(&mut self, ctx: &egui::Context, deck: Option<api::Deck>, periodo: Option<api::PeriodoSprint>, inicio: Inicio, tela_cheia: bool) {
        let Some(perfil) = self.perfil.as_ref().map(|p| p.id) else { return };
        if self.demo || self.apresentacao.is_some() {
            return;
        }
        self.tela_cheia_antes = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if tela_cheia && !self.tela_cheia_antes {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        self.abelha.resumo_aberto = false;
        let mostrar_tempo = self.perfil.as_ref().is_some_and(|p| p.tempo_agentes);
        let a = Apresentacao::nova(ctx, perfil, self.escopo.recorte(), periodo, deck, inicio, tela_cheia || self.tela_cheia_antes, self.tema, mostrar_tempo);
        self.apresentacao = Some(Box::new(a));
    }

    /// Sai da apresentação: a nota pendente já foi salva; a tela cheia, o
    /// tema e a página voltam como estavam, e os avisos guardados aparecem.
    fn sair_da_apresentacao(&mut self, ctx: &egui::Context, agora: f64) {
        let Some(mut a) = self.apresentacao.take() else { return };
        a.encerrar(ctx);
        if a.tela_cheia != self.tela_cheia_antes {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.tela_cheia_antes));
        }
        self.tema.aplicar(ctx);
        if let Some(tarefa) = a.tarefa_atual() {
            self.ultimo_slide = Some((a.daily(), tarefa));
            self.registro.rolar_ate = Some(tarefa);
        }
        if let Some(aviso) = &mut self.aviso
            && aviso.ate.is_finite()
        {
            aviso.ate = agora + 8.0;
        }
        self.registro.marcar_suja();
        ctx.request_repaint();
    }

    // Lousa

    /// Uma mudança de lousa chegou: a lousa aberta aplica; a da tarefa fora
    /// da tela ganha o ponto no chip; o slide da apresentação busca de novo.
    fn lousa_mudou(&mut self, lousa: i64, dono: api::DonoLousa, elementos: Vec<api::ElementoLousa>, removidos: &[i64], agente: i64, agora: f64) {
        let novos = match self.lousas.get_mut(&dono) {
            Some(l) => l.aplicar_evento(lousa, elementos, removidos, agente),
            None if agente != 0 => elementos.len(),
            None => 0,
        };
        if dono.tarefa_id != 0
            && let Some(a) = &mut self.apresentacao
        {
            a.lousa_mudou(dono.tarefa_id);
        }
        if novos == 0 {
            return;
        }
        let visivel = match self.tela {
            Tela::Lousa(ws) => dono.workspace_id == ws,
            Tela::Tarefa { id, .. } => dono.tarefa_id == id && self.lousa_na_tarefa.contains(&id),
            _ => false,
        };
        if !visivel {
            if dono.tarefa_id != 0 {
                self.novidade_na_lousa.insert(dono.tarefa_id);
            }
            return;
        }
        if self.lousas.get(&dono).is_some_and(|l| l.novos_fora_da_vista()) {
            let quem = self.modelo.tarefas.iter().flat_map(|t| &t.agentes).find(|a| a.id == agente).map_or("O agente".to_string(), |a| a.nome());
            let itens = if novos == 1 { "1 item".to_string() } else { format!("{novos} itens") };
            self.avisar_com(TipoAviso::Neutro, format!("{quem} acrescentou {itens}"), "Ver", AcaoAviso::Lousa(dono, AcaoLousa::Ver), agora);
        }
    }

    fn abrir_lousa_do_workspace(&mut self, workspace: i64) {
        self.tela = Tela::Lousa(workspace);
        // Aberta, a lousa já tem o teclado (colar um print logo de cara);
        // uma vez só, sem pedir foco a cada quadro.
        let dono = api::DonoLousa { workspace_id: workspace, tarefa_id: 0 };
        self.lousa_ativa_ao_abrir = Some(dono);
        self.voltar_para_lousa = None;
        self.abelha.resumo_aberto = false;
        self.filtro = None;
    }

    /// Abre ou fecha a lousa por cima do terminal da tarefa (Ctrl+Shift+Q).
    fn alternar_lousa_da_tarefa(&mut self, tarefa: i64) {
        if self.lousa_na_tarefa.insert(tarefa) {
            self.novidade_na_lousa.remove(&tarefa);
            return;
        }
        self.lousa_na_tarefa.remove(&tarefa);
        // A lousa fechou: o teclado volta ao terminal.
        if let Tela::Tarefa { foco, .. } = self.tela
            && let Some(t) = self.terminais.get(&foco)
        {
            t.focar();
        }
    }

    /// Desenha a lousa do dono na área e devolve o que ela pediu.
    fn mostrar_lousa(&mut self, ui: &mut egui::Ui, area: egui::Rect, dono: api::DonoLousa, agora: f64) -> Vec<lousa::Acao> {
        let ctx = ui.ctx().clone();
        let perfil = self.perfil.as_ref().map_or(0, |p| p.id);
        let pode_mudar = self.pode_mudar();
        let (preferidos, tem_agente) = if dono.workspace_id != 0 {
            (self.modelo.projetos.iter().filter(|p| p.workspace_id == dono.workspace_id).map(|p| p.id).collect(), false)
        } else {
            let t = self.modelo.tarefas.iter().find(|t| t.id == dono.tarefa_id);
            (t.map(|t| vec![t.projeto_id]).unwrap_or_default(), t.is_some_and(|t| t.agentes.iter().any(|a| a.ferramenta != "shell")))
        };
        let l = self.lousas.entry(dono).or_insert_with(|| lousa::Lousa::nova(&ctx, dono, agora));
        if self.lousa_ativa_ao_abrir == Some(dono) {
            self.lousa_ativa_ao_abrir = None;
            l.ativa = true;
        }
        let faixa_global = !self.demo && matches!(self.conexao, Conexao::Fora | Conexao::Antigo);
        let c = lousa::Contexto { perfil, pode_mudar, faixa_global, modelo: &self.modelo, preferidos, tem_agente, agora };
        l.mostrar(ui, area, &c)
    }

    /// O que a lousa pediu: abrir tarefa, avisos, apresentar, pedir ao agente.
    fn tratar_lousa(&mut self, dono: api::DonoLousa, acoes: Vec<lousa::Acao>, ctx: &egui::Context, agora: f64) {
        for acao in acoes {
            match acao {
                lousa::Acao::AbrirTarefa(tarefa) => {
                    self.abrir_tarefa(tarefa, None);
                    // Aberta pelo cartão da lousa do workspace: o "‹ Lousa" volta para lá.
                    self.voltar_para_lousa = (dono.workspace_id != 0).then_some(dono.workspace_id);
                }
                lousa::Acao::Avisar(tipo, texto) => self.avisar(tipo, texto, agora),
                lousa::Acao::AvisarDesfazer(texto, numero) => {
                    self.avisar_com(TipoAviso::Neutro, texto, "Desfazer", AcaoAviso::Lousa(dono, AcaoLousa::Desfazer(numero)), agora)
                }
                lousa::Acao::AvisarTentar(texto) => self.avisar_com(TipoAviso::Erro, texto, "Tentar de novo", AcaoAviso::Lousa(dono, AcaoLousa::Tentar), agora),
                lousa::Acao::Apresentar(desde) => self.apresentar_lousa(dono, desde, ctx),
                lousa::Acao::PedirAoAgente(texto) => self.compositor.citar(&texto),
            }
        }
    }

    /// As lousas recebem as respostas e gravam o pendente, mesmo fora da tela.
    fn andar_lousas(&mut self, ctx: &egui::Context, agora: f64) {
        let pode = self.pode_mudar();
        let donos: Vec<api::DonoLousa> = self.lousas.keys().copied().collect();
        for dono in donos {
            let acoes = self.lousas.get_mut(&dono).map(|l| l.fundo(ctx, agora, pode)).unwrap_or_default();
            self.tratar_lousa(dono, acoes, ctx, agora);
        }
    }

    /// Apresenta a lousa no palco, em tela cheia (do item dado, ou do começo).
    fn apresentar_lousa(&mut self, dono: api::DonoLousa, desde: Option<i64>, ctx: &egui::Context) {
        let Some(l) = self.lousas.get(&dono) else { return };
        if l.cartoes() == 0 || self.palco.is_some() {
            return;
        }
        let palco = lousa::palco::Palco::novo(l.modelo.elementos.clone(), desde, self.tema);
        self.tela_cheia_antes = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if !self.tela_cheia_antes {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        self.abelha.resumo_aberto = false;
        self.palco = Some((dono, Box::new(palco)));
    }

    fn sair_do_palco(&mut self, ctx: &egui::Context) {
        if let Some((_, mut palco)) = self.palco.take() {
            palco.encerrar(ctx);
            if !self.tela_cheia_antes {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
            self.tema.aplicar(ctx);
            ctx.request_repaint();
        }
    }

    // Gaveta de arquivos

    fn alternar_gaveta(&mut self, tarefa: i64, ctx: &egui::Context) {
        if self.gaveta == Some(tarefa) {
            self.fechar_gaveta(tarefa);
            return;
        }
        self.gaveta = Some(tarefa);
        self.arvores.entry(tarefa).or_insert_with(|| gaveta::Arvore::nova(ctx, tarefa)).focar = true;
    }

    /// Fecha a gaveta e devolve o teclado ao terminal em foco.
    fn fechar_gaveta(&mut self, tarefa: i64) {
        self.gaveta = None;
        if let Tela::Tarefa { foco, .. } = self.tela
            && let Some(t) = self.terminais.get(&foco)
        {
            t.focar();
        }
        let _ = tarefa;
    }

    #[allow(clippy::too_many_arguments)]
    fn mostrar_gaveta(&mut self, ctx: &egui::Context, tarefa: i64, area: egui::Rect, largura: f32, pasta: &str, _foco: Option<i64>, agora: f64) {
        let editor = self.editor;
        let Some(arvore) = self.arvores.get_mut(&tarefa) else { return };
        match arvore.mostrar(ctx, area, largura, pasta, editor.map(|(nome, _)| nome)) {
            None => {}
            Some(gaveta::PedidoGaveta::Fechar) => self.fechar_gaveta(tarefa),
            Some(gaveta::PedidoGaveta::AbrirPasta) => {
                if let Err(e) = sistema::abrir_pasta(pasta) {
                    self.erro(format!("Não consegui abrir a pasta: {e}"), agora);
                }
            }
            Some(gaveta::PedidoGaveta::AbrirNoEditor(arquivo)) => {
                if let Some((nome, comando)) = editor
                    && let Err(e) = sistema::abrir_arquivo_com(comando, &arquivo)
                {
                    self.erro(format!("Não consegui abrir no {nome}: {e}"), agora);
                }
            }
            Some(gaveta::PedidoGaveta::AbrirNoSistema(arquivo)) => {
                if let Err(e) = sistema::abrir_no_sistema(&arquivo) {
                    self.erro(format!("Não consegui abrir: {e}"), agora);
                }
            }
            // A citação vai para a caixa de mensagem, que fica com o teclado.
            Some(gaveta::PedidoGaveta::Citar(texto)) => {
                self.compositor.citar(&texto);
                self.gaveta = None;
            }
        }
    }

    // Navegador da tarefa

    /// Há Chrome ou Chromium? Lido do núcleo uma vez.
    fn tem_navegador(&mut self) -> bool {
        if self.navegador.is_none() {
            self.navegador = api::info_navegador().ok();
        }
        self.navegador.as_ref().is_some_and(|n| n.instalado)
    }

    /// Clique no "Navegador": sem Chrome, explica; aberto, traz para frente;
    /// fechado, abre o último endereço da tarefa (ou pede um).
    fn clique_navegador(&mut self, tarefa: i64, botao: egui::Rect, ctx: &egui::Context, _agora: f64) {
        if !self.tem_navegador() {
            self.popover_navegador = Some(PopoverNavegador::SemChrome { botao, quadros: 0 });
            return;
        }
        if self.modelo.navegadores.contains(&tarefa) {
            self.abrir_navegador(tarefa, String::new(), ctx);
        } else if let Some(url) = self.enderecos.get(&tarefa).cloned() {
            self.abrir_navegador(tarefa, url, ctx);
        } else {
            self.pedir_endereco(tarefa, botao);
        }
    }

    fn pedir_endereco(&mut self, tarefa: i64, botao: egui::Rect) {
        if !self.tem_navegador() {
            self.popover_navegador = Some(PopoverNavegador::SemChrome { botao, quadros: 0 });
            return;
        }
        let url = self.enderecos.get(&tarefa).cloned().unwrap_or_default();
        self.popover_navegador = Some(PopoverNavegador::Endereco { tarefa, botao, url, erro: None, focar: true, quadros: 0 });
    }

    /// Conta ao núcleo, quando muda, se a tela mostra algo que pode estar
    /// compartilhado: a apresentação, a Daily e a Sprint. Enquanto isso, o
    /// navegador que o agente abrir fica fora da tela (a captura funciona) e
    /// não cobre os cartões nem o aviso "respondeu · Ver"; o botão
    /// "Navegador" da tarefa o traz para o lado. Vai junto a geometria ao
    /// lado da janela, para o agente não abrir num lugar inventado.
    fn avisar_tela(&mut self, ctx: &egui::Context) {
        let Some(perfil) = self.perfil.as_ref().map(|p| p.id) else { return };
        if self.demo || self.conexao != Conexao::Ligado {
            return;
        }
        let compartilhavel = self.apresentacao.is_some()
            || self.palco.is_some()
            || matches!(self.tela, Tela::Registro(registro::Aba::Daily) | Tela::Registro(registro::Aba::Sprint));
        let geometria = ctx.input(|i| gaveta::geometria_ao_lado(i.viewport().outer_rect, i.viewport().monitor_size, i.pixels_per_point, None));
        if self.tela_avisada == Some((perfil, compartilhavel, geometria)) {
            return;
        }
        self.tela_avisada = Some((perfil, compartilhavel, geometria));
        avisar_apresentando(perfil, compartilhavel, geometria);
    }

    /// Abre (ou traz para frente) a janela da tarefa ao lado da Colmeia.
    fn abrir_navegador(&mut self, tarefa: i64, url: String, ctx: &egui::Context) {
        let topo = self.topo_terminal;
        let geometria = ctx.input(|i| {
            let topo = topo.and_then(|t| Some(i.viewport().inner_rect?.top() + t));
            gaveta::geometria_ao_lado(i.viewport().outer_rect, i.viewport().monitor_size, i.pixels_per_point, topo)
        });
        let envio = self.respostas_navegador.0.clone();
        let ctx = ctx.clone();
        self.abrindo_navegador = Some(tarefa);
        std::thread::spawn(move || {
            let resultado = api::abrir_navegador(tarefa, &url, geometria);
            let _ = envio.send(RespostaNavegador::Aberto { tarefa, url, resultado });
            ctx.request_repaint();
        });
    }

    fn capturar_navegador(&mut self, tarefa: i64, ctx: &egui::Context) {
        let envio = self.respostas_navegador.0.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = envio.send(RespostaNavegador::Capturado { tarefa, resultado: api::capturar_navegador(tarefa) });
            ctx.request_repaint();
        });
    }

    fn receber_navegador(&mut self, agora: f64) {
        while let Ok(r) = self.respostas_navegador.1.try_recv() {
            match r {
                RespostaNavegador::Aberto { tarefa, url, resultado } => {
                    self.abrindo_navegador = None;
                    match resultado {
                        Ok(()) => {
                            if !url.is_empty() {
                                self.enderecos.insert(tarefa, url);
                            }
                            self.modelo.navegadores.insert(tarefa);
                            if matches!(self.popover_navegador, Some(PopoverNavegador::Endereco { tarefa: t, .. }) if t == tarefa) {
                                self.popover_navegador = None;
                            }
                        }
                        // A URL recusada aparece embaixo do campo; o resto, no aviso.
                        Err(e) => match &mut self.popover_navegador {
                            Some(PopoverNavegador::Endereco { tarefa: t, erro, .. }) if *t == tarefa => *erro = Some(e),
                            _ => self.erro(format!("O navegador não abriu: {e}"), agora),
                        },
                    }
                }
                RespostaNavegador::Capturado { tarefa, resultado } => match resultado {
                    Ok(anexo) => {
                        let titulo = self.modelo.tarefas.iter().find(|t| t.id == tarefa).map(|t| t.titulo.clone()).unwrap_or_default();
                        self.avisar_com(
                            TipoAviso::Neutro,
                            format!("Captura do navegador anexada a «{titulo}»"),
                            "Desfazer",
                            AcaoAviso::DesfazerCaptura(anexo),
                            agora,
                        );
                    }
                    Err(e) => self.erro(e, agora),
                },
                RespostaNavegador::Fechado(Err(e)) => self.erro(e, agora),
                RespostaNavegador::Fechado(Ok(())) => {}
            }
        }
    }

    /// A caixa de endereço ou o aviso de "sem Chrome", presos ao botão.
    fn mostrar_popover_navegador(&mut self, ctx: &egui::Context) {
        let p = cores();
        let Some(pop) = &mut self.popover_navegador else { return };
        let mut fechar = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let mut abrir = None;
        let abrindo = self.abrindo_navegador.is_some();
        let (botao, quadros) = match pop {
            PopoverNavegador::Endereco { botao, quadros, .. } | PopoverNavegador::SemChrome { botao, quadros } => (*botao, quadros),
        };
        *quadros = quadros.saturating_add(1);
        let primeiro = *quadros <= 1;
        let area = egui::Area::new(egui::Id::new("popover-navegador"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(botao.right_bottom() + egui::vec2(0.0, 6.0))
            .show(ctx, |ui| {
                tema::moldura_flutuante().show(ui, |ui| match pop {
                    PopoverNavegador::SemChrome { .. } => {
                        ui.set_width(360.0 - 32.0);
                        ui.horizontal(|ui| {
                            let titulo = ui.painter().layout_no_wrap("Nenhum Chrome ou Chromium instalado".into(), tema::forte(14.0), p.texto);
                            // O ponto no meio da linha do título (a altura da linha, não a do bloco).
                            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, titulo.size().y), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, p.alerta);
                            ui.label(texto_forte("Nenhum Chrome ou Chromium instalado", 14.0).color(p.texto));
                        });
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(RichText::new("Instale o Chromium ou aponte a variável").color(p.texto).size(13.0));
                            tema::tecla(ui, "COLMEIA_NAVEGADOR");
                            ui.label(RichText::new("para o executável.").color(p.texto).size(13.0));
                        });
                    }
                    PopoverNavegador::Endereco { tarefa, url, erro, focar, .. } => {
                        ui.set_width(380.0 - 32.0);
                        let resposta = tema::campo(ui, "Endereço", url, "localhost:5173 ou https://…");
                        if std::mem::take(focar) {
                            resposta.request_focus();
                        }
                        // O erro era do endereço anterior: some ao editar.
                        if resposta.changed() {
                            *erro = None;
                        }
                        let enter = resposta.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if let Some(e) = erro {
                            ui.add_space(4.0);
                            ui.label(RichText::new(e.as_str()).color(p.erro).size(12.5));
                        }
                        ui.add_space(10.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let rotulo = if abrindo { "Abrindo…" } else { "Abrir ao lado" };
                            let pode = !url.trim().is_empty() && !abrindo;
                            if (tema::botao_principal(ui, rotulo, pode).clicked() || enter) && pode {
                                *erro = None;
                                abrir = Some((*tarefa, gaveta::normalizar_endereco(url)));
                            }
                        });
                    }
                });
            });
        let fora = ctx.input(|i| i.pointer.any_released() && i.pointer.interact_pos().is_some_and(|pos| !area.response.rect.contains(pos)));
        fechar |= fora && !primeiro;
        if let Some((tarefa, url)) = abrir {
            self.abrir_navegador(tarefa, url, ctx);
        } else if fechar {
            self.popover_navegador = None;
        }
    }

    /// Fecha a caixa "Pedir ao agente", guardando o rascunho da tarefa.
    fn fechar_caixa_pedido(&mut self) {
        if let Some((caixa, _)) = self.caixa_pedido.take()
            && !caixa.texto.trim().is_empty()
        {
            self.rascunhos.insert(caixa.tarefa, caixa.texto);
        }
    }

    /// A caixa da Daily e da Sprint, presa ao botão do cartão: abre embaixo,
    /// ou em cima se não couber.
    fn mostrar_caixa_pedido(&mut self, ctx: &egui::Context, agora: f64) {
        let Some((caixa, botao)) = &mut self.caixa_pedido else { return };
        let tela = ctx.content_rect();
        let (ancora, pivo) = if botao.bottom() + 6.0 + 380.0 < tela.bottom() {
            (botao.right_bottom() + egui::vec2(0.0, 6.0), egui::Align2::RIGHT_TOP)
        } else {
            (botao.right_top() - egui::vec2(0.0, 6.0), egui::Align2::RIGHT_BOTTOM)
        };
        let resumos = pedido::resumir(&self.modelo);
        let tarefa = caixa.tarefa;
        let saida = caixa.mostrar(ctx, ancora, pivo, resumos.get(&tarefa));
        match saida {
            None => {}
            Some(pedido::Saida::Fechar) => self.fechar_caixa_pedido(),
            Some(pedido::Saida::Enviado(p)) => {
                self.caixa_pedido = None;
                self.rascunhos.remove(&tarefa);
                let titulo = self.modelo.tarefas.iter().find(|t| t.id == tarefa).map(|t| t.titulo.clone()).unwrap_or_default();
                if !self.modelo.pedidos.iter().any(|x| x.id == p.id) {
                    self.modelo.pedidos.push(*p);
                }
                self.avisar(TipoAviso::Neutro, format!("Pedido enviado ao agente de «{titulo}»"), agora);
            }
            Some(pedido::Saida::AbrirTarefa) => {
                self.fechar_caixa_pedido();
                let agente = resumos.get(&tarefa).map(|r| r.pedido.agente_id);
                self.abrir_tarefa(tarefa, agente);
            }
        }
    }

    /// Fechar a janela não para os agentes: com algum rodando, a Colmeia pergunta.
    fn ao_fechar(&mut self, ctx: &egui::Context) {
        // Fechar a janela grava a lousa na hora (esperando a resposta).
        if ctx.input(|i| i.viewport().close_requested()) {
            for l in self.lousas.values_mut() {
                l.gravar_ja();
            }
        }
        if self.pode_fechar || self.demo || !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        let rodando = self.modelo.tarefas.iter().flat_map(|t| &t.agentes).filter(|a| a.ativo).count();
        if rodando == 0 {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if !matches!(self.dialogo, Some(Dialogo::Fechar { .. })) {
            self.dialogo = Some(Dialogo::Fechar { rodando, erro: None });
        }
    }
}

/// O que a caixa de um agente pede ao painel da tarefa.
enum Pedido {
    Focar(i64),
    Iniciar(i64),
    Remover(i64),
    Capturar(i64),
    /// Apagar as mensagens guardadas do agente (quantas são).
    LimparHistorico(i64, usize),
}

/// Cartão de um agente parado, no meio da caixa: como terminou e o que fazer.
/// Na miniatura só o texto; o clique foca a caixa.
fn cartao_de_fim(ui: &mut egui::Ui, centro: egui::Pos2, agente: &AgenteTela, focado: bool, pode: bool) -> Option<Pedido> {
    let p = cores();
    let mut pedido = None;
    let visual = agente.visual();
    let largura = if focado { 340.0 } else { 220.0 };
    let area = egui::Rect::from_center_size(centro, egui::vec2(largura, if focado { 110.0 } else { 50.0 }));
    let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Center)));
    egui::Frame::new()
        .fill(p.superficie_alta)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
        .inner_margin(egui::Margin::same(if focado { 16 } else { 10 }))
        .shadow(tema::sombra(6, 18))
        .show(&mut filho, |ui| {
            ui.set_width(largura - if focado { 32.0 } else { 20.0 });
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(agente.texto_estado()).color(visual.cor()).size(if focado { 13.5 } else { 12.0 }));
                if !focado {
                    return;
                }
                ui.add_space(12.0);
                let rotulo = if agente.ferramenta == "claude" { "Retomar conversa" } else { "Iniciar de novo" };
                ui.horizontal(|ui| {
                    let largura_botoes = ui.painter().layout_no_wrap(rotulo.into(), tema::forte(13.5), p.texto).size().x + 32.0 + 8.0 + 90.0;
                    ui.add_space(((ui.available_width() - largura_botoes) / 2.0).max(0.0));
                    if tema::botao_principal(ui, rotulo, pode).clicked() {
                        pedido = Some(Pedido::Iniciar(agente.id));
                    }
                    ui.add_space(8.0);
                    if tema::botao_secundario_com(ui, "Remover", pode).clicked() {
                        pedido = Some(Pedido::Remover(agente.id));
                    }
                });
            });
        });
    pedido
}

/// Item da barra lateral: linha inteira clicável, fundo ao passar o mouse e um
/// ponto opcional na cor do estado do projeto. Com `menu`, o "⋯" aparece ao
/// passar o mouse no lugar do ponto (que vai um pouco para a esquerda).
fn item_lateral(
    ui: &mut egui::Ui,
    texto: &str,
    ativo: bool,
    ponto: Option<Color32>,
    menu: bool,
    icone: Option<tema::Icone>,
    recuo: f32,
) -> (egui::Response, Option<egui::Response>) {
    let p = cores();
    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
    // Dentro de um workspace, o item vai para a direita, embaixo do nome dele.
    let rect = rect.with_min_x(rect.min.x + recuo);
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let em_cima = ui.rect_contains_pointer(rect);
    if ativo || em_cima {
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_CONTROLE), if ativo { p.realce } else { p.realce.gamma_multiply(0.6) });
    }
    let fonte = if ativo { tema::forte(13.5) } else { egui::FontId::proportional(13.5) };
    let cor = if ativo { p.texto } else { p.suave };
    // Com ícone (a lousa do workspace), o texto vai a 32 da esquerda.
    let x = match icone {
        Some(i) => {
            tema::desenhar_icone(ui.painter(), rect.left_center() + egui::vec2(17.0, 0.0), i, cor);
            32.0
        }
        None => 10.0,
    };
    ui.painter().text(rect.left_center() + egui::vec2(x, 0.0), egui::Align2::LEFT_CENTER, texto, fonte, cor);
    let mostrar_menu = menu && em_cima;
    if let Some(cor) = ponto {
        let x = if mostrar_menu { 40.0 } else { 14.0 };
        ui.painter().circle_filled(rect.right_center() - egui::vec2(x, 0.0), 4.0, cor);
    }
    let mais = mostrar_menu.then(|| {
        let r = egui::Rect::from_center_size(rect.right_center() - egui::vec2(14.0, 0.0), egui::vec2(24.0, 24.0));
        tema::botao_icone_em(ui, r, ui.id().with(("mais-projeto", texto)), tema::Icone::Mais)
    });
    (resposta, mais)
}

/// Cabeçalho de um workspace na barra lateral: o nome, pequeno como antes,
/// agora clicável (mostra todos os projetos dele). Com o mouse em cima o
/// texto vai para a cor do texto (o suave sobre o realce não tem contraste
/// nos temas claros); ativo, fundo de realce e seminegrito. O número de
/// projetos aparece à direita só ativo ou com o mouse em cima.
/// A seta à esquerda recolhe ou abre o workspace (devolvida à parte: o
/// clique nela não abre a visão do workspace); recolhido, o ponto mostra o
/// estado mais urgente dos projetos escondidos.
fn cabecalho_lateral(ui: &mut egui::Ui, nome: &str, ativo: bool, projetos: usize, recolhido: bool, ponto: Option<Color32>) -> (egui::Response, egui::Response) {
    let p = cores();
    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    let area_seta = egui::Rect::from_min_size(rect.min, egui::vec2(24.0, rect.height()));
    let seta = ui.interact(area_seta, ui.id().with(("recolher-workspace", nome)), egui::Sense::click());
    let seta = seta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if recolhido { "Mostrar os projetos" } else { "Recolher os projetos" });
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let em_cima = resposta.hovered() || seta.hovered();
    if ativo || em_cima {
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_CONTROLE), if ativo { p.realce } else { p.realce.gamma_multiply(0.6) });
    }
    // O workspace se destaca dos projetos: seminegrito e na cor do texto
    // sempre, maior que antes, com a seta na cor de destaque.
    let (fonte, cor) = (tema::forte(12.5), p.texto);
    // O ativo leva também a marca de 3 px à esquerda (no Leitura o realce
    // do ativo e o do mouse em cima ficam quase iguais).
    if ativo {
        let marca = egui::Rect::from_min_size(rect.left_top() + egui::vec2(0.0, 5.0), egui::vec2(3.0, rect.height() - 10.0));
        ui.painter().rect_filled(marca, CornerRadius::same(2), p.destaque);
    }
    // A contagem sempre aparece: o cabeçalho parece um item da lista. Sobre o
    // realce ela vai na cor do texto (o suave ali fica abaixo de 4,5:1); a
    // diferença para o nome vem do peso.
    let quantos = if projetos == 1 { "1 projeto".to_string() } else { format!("{projetos} projetos") };
    let cor_quantos = if ativo || em_cima { p.texto } else { p.suave };
    let g_quantos = ui.painter().layout_no_wrap(quantos, egui::FontId::proportional(11.0), cor_quantos);
    // A seta desenhada (a fonte não tem ▸/▾): para a direita recolhido, para baixo aberto.
    let c = rect.left_center() + egui::vec2(12.0, 0.0);
    let cor_seta = if seta.hovered() { p.texto } else { p.destaque };
    let pontas = if recolhido {
        vec![c + egui::vec2(-2.0, -3.5), c + egui::vec2(2.5, 0.0), c + egui::vec2(-2.0, 3.5)]
    } else {
        vec![c + egui::vec2(-3.5, -2.0), c + egui::vec2(3.5, -2.0), c + egui::vec2(0.0, 2.5)]
    };
    ui.painter().add(egui::Shape::convex_polygon(pontas, cor_seta, Stroke::NONE));
    let reserva_ponto = if recolhido && ponto.is_some() { 14.0 } else { 0.0 };
    let largura = rect.width() - 24.0 - 14.0 - g_quantos.size().x - reserva_ponto - 8.0;
    let texto = tema::cortar(ui.painter(), nome, egui::text::TextFormat::simple(fonte, cor), largura, 1, false);
    ui.painter().galley(rect.left_center() + egui::vec2(24.0, -texto.size().y / 2.0), texto, cor);
    let pos = rect.right_center() - egui::vec2(14.0 + g_quantos.size().x, g_quantos.size().y / 2.0);
    ui.painter().galley(pos, g_quantos, cor_quantos);
    if recolhido && let Some(cor) = ponto {
        ui.painter().circle_filled(egui::pos2(pos.x - 10.0, rect.center().y), 4.0, cor);
    }
    if resposta.has_focus() {
        ui.painter().rect_stroke(rect, CornerRadius::same(tema::RAIO_CONTROLE), Stroke::new(1.5, p.destaque), egui::StrokeKind::Inside);
    }
    (resposta, seta)
}

/// Quanto os projetos e a lousa de um workspace entram na barra lateral.
const RECUO_WORKSPACE: f32 = 12.0;

/// Um passo do caminho no topo: suave, com a cor do texto e a mãozinha ao
/// passar o mouse, seguido do "›". Devolve se foi clicado.
fn migalha(ui: &mut egui::Ui, texto: &str) -> bool {
    let p = cores();
    let id = ui.id().with(("migalha", texto));
    let em_cima = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let r = ui
        .add(egui::Label::new(RichText::new(texto).color(if em_cima { p.texto } else { p.suave })).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if r.hovered() != em_cima {
        ui.ctx().data_mut(|d| d.insert_temp(id, r.hovered()));
        ui.ctx().request_repaint();
    }
    ui.label(RichText::new("›").color(p.suave));
    r.clicked()
}

/// A cor do texto (para medir antes de desenhar).
fn p_texto() -> Color32 {
    cores().texto
}

/// O nome de cada projeto do recorte nos cartões do quadro: com o workspace
/// na frente ("trabalho-x · loja-web") quando o nome se repete no recorte,
/// como na linha do tempo.
fn rotulos_de_projeto(projetos: &[Projeto], no_escopo: &HashSet<i64>) -> HashMap<i64, String> {
    let no_recorte: Vec<&Projeto> = projetos.iter().filter(|p| no_escopo.contains(&p.id)).collect();
    no_recorte
        .iter()
        .map(|p| {
            let repetido = no_recorte.iter().filter(|o| o.nome == p.nome).count() > 1;
            (p.id, if repetido && !p.workspace.is_empty() { format!("{} · {}", p.workspace, p.nome) } else { p.nome.clone() })
        })
        .collect()
}

/// "Mostrar tempo dos agentes" no "⋯", com a marca quando ligada. Sem o
/// núcleo, inativa e dizendo o último valor conhecido.
fn opcao_tempo(ui: &mut egui::Ui, tempo: bool, pode: bool) -> bool {
    if pode {
        tema::opcao_menu(ui, "Mostrar tempo dos agentes", tempo)
    } else {
        tema::opcao_menu_com(ui, "Mostrar tempo dos agentes", Some(if tempo { "ligado" } else { "desligado" }), false)
    }
}

/// O chip "Ver" nunca fica mais estreito que isto ("Ver: todos os p…").
const CHIP_VER_MINIMO: f32 = 140.0;

/// Como o grupo da direita da barra do registro cabe: a caixa do tempo na
/// barra (com o rótulo que coube) ou no "⋯", e "Copiar texto" na barra ou no
/// "⋯". `largura` é o que ele ocupa, para o chip "Ver" ficar com o resto.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ArranjoBarra {
    caixa: Option<&'static str>,
    copiar: bool,
    mais: bool,
    largura: f32,
}

impl Default for ArranjoBarra {
    fn default() -> Self {
        ArranjoBarra { caixa: Some("Mostrar tempo dos agentes"), copiar: true, mais: false, largura: 0.0 }
    }
}

/// Larguras das peças do grupo da direita (medidas com as fontes de verdade).
#[derive(Clone, Copy, Debug)]
struct PecasBarra {
    apresentar: f32,
    copiar: f32,
    mais: f32,
    caixa: f32,
    caixa_curta: f32,
}

/// Escolhe o arranjo que cabe em `livre` deixando ao menos o mínimo do chip
/// "Ver" (o natural, se der). Na linha do tempo só há a caixa (com o rótulo
/// curto, ou no "⋯"); na daily e na sprint a caixa sai primeiro para o "⋯",
/// depois "Copiar texto". A sprint sempre tem o "⋯" (Markdown e Salvar).
fn arranjar_barra(linha: bool, sprint: bool, tem_apresentar: bool, tem_copiar: bool, pecas: PecasBarra, livre: f32, chip: f32) -> ArranjoBarra {
    let cabe = |largura: f32, chip: f32| livre - largura >= chip;
    if linha {
        for (rotulo, largura) in [("Mostrar tempo dos agentes", pecas.caixa), ("Tempo dos agentes", pecas.caixa_curta)] {
            if cabe(largura, CHIP_VER_MINIMO.min(chip)) {
                return ArranjoBarra { caixa: Some(rotulo), copiar: false, mais: false, largura };
            }
        }
        return ArranjoBarra { caixa: None, copiar: false, mais: true, largura: pecas.mais };
    }
    let base = if tem_apresentar { pecas.apresentar } else { 0.0 };
    let copiar = if tem_copiar { pecas.copiar } else { 0.0 };
    let mais_sprint = if sprint { pecas.mais } else { 0.0 };
    let opcoes = [
        (Some("Mostrar tempo dos agentes"), true, sprint, base + copiar + pecas.caixa + mais_sprint),
        (None, true, true, base + copiar + pecas.mais),
        (None, false, true, base + pecas.mais),
    ];
    // Antes de tirar uma peça da barra, o chip encolhe até o mínimo.
    for (caixa, com_copiar, mais, largura) in opcoes {
        if cabe(largura, CHIP_VER_MINIMO.min(chip)) {
            return ArranjoBarra { caixa, copiar: com_copiar, mais, largura };
        }
    }
    let (caixa, copiar, mais, largura) = opcoes[2];
    ArranjoBarra { caixa, copiar, mais, largura }
}

/// O traço que separa partes de um menu: 1 px de borda, 8 px de margem e 4 px em volta.
fn linha_do_menu(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    // A largura do menu (não a disponível, que alargaria o menu inteiro).
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.min_rect().width(), 1.0), egui::Sense::hover());
    ui.painter().line_segment([r.left_center() + egui::vec2(8.0, 0.0), r.right_center() - egui::vec2(8.0, 0.0)], Stroke::new(1.0, cores().borda));
    ui.add_space(4.0);
}

/// Recorta o terminal da imagem da janela e codifica em PNG.
fn recortar_png(imagem: &egui::ColorImage, area: egui::Rect, ppp: f32) -> Result<Vec<u8>, String> {
    let [largura, altura] = imagem.size;
    let x0 = ((area.min.x * ppp).round().max(0.0) as usize).min(largura);
    let y0 = ((area.min.y * ppp).round().max(0.0) as usize).min(altura);
    let x1 = ((area.max.x * ppp).round().max(0.0) as usize).min(largura);
    let y1 = ((area.max.y * ppp).round().max(0.0) as usize).min(altura);
    if x1 <= x0 || y1 <= y0 {
        return Err("o terminal não está visível".into());
    }
    let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        for px in &imagem.pixels[y * largura + x0..y * largura + x1] {
            rgba.extend_from_slice(&px.to_srgba_unmultiplied());
        }
    }
    let mut png = Vec::new();
    let mut codificador = png::Encoder::new(&mut png, (x1 - x0) as u32, (y1 - y0) as u32);
    codificador.set_color(png::ColorType::Rgba);
    codificador.set_depth(png::BitDepth::Eight);
    let mut escritor = codificador.write_header().map_err(|e| e.to_string())?;
    escritor.write_image_data(&rgba).map_err(|e| e.to_string())?;
    escritor.finish().map_err(|e| e.to_string())?;
    Ok(png)
}

fn formatar_vazao(b: u64) -> String {
    match b {
        b if b >= 1024 * 1024 => format!("{:.1} MB/s", b as f64 / 1048576.0),
        b if b >= 1024 => format!("{} KB/s", b / 1024),
        b => format!("{b} B/s"),
    }
}

impl eframe::App for Colmeia {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let agora = ui.input(|i| i.time);
        self.medir(agora);
        let p = cores();
        let ctx = ui.ctx().clone();
        // Mede o atlas das letras antes de qualquer texto do quadro (o cache da lousa depende dele).
        lousa::desenho::geracao_das_fontes(&ctx);

        // Sem perfil: só a tela de entrada, sobre o favo.
        if let Tela::Entrada(entrada) = &mut self.tela {
            let mut escolhido = None;
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.fundo)).show(ui, |ui| {
                self.favo.desenhar(ui.painter(), ui.max_rect());
                egui::ScrollArea::vertical().show(ui, |ui| escolhido = entrada.mostrar(ui));
            });
            if let Some(perfil) = escolhido {
                self.entrar(perfil, &ctx);
            }
            return;
        }
        self.receber_eventos(&ctx, agora);
        self.receber_tempo(agora);
        self.receber_capturas(agora);
        self.receber_navegador(agora);
        self.atalhos(&ctx);
        self.ao_fechar(&ctx);
        self.ajustar_ritmos();
        self.ancora_aviso = None;
        self.avisar_tela(&ctx);

        // A apresentação ocupa a janela inteira: sem barra lateral, topo nem avisos
        // (a tela está sendo compartilhada); os avisos guardados aparecem ao sair.
        if self.apresentacao.is_some() {
            let mut pedido = None;
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.fundo)).show(ui, |ui| {
                if let Some(a) = &mut self.apresentacao {
                    a.pedidos = pedido::resumir(&self.modelo);
                    a.definir_tempo(ui.ctx(), self.perfil.as_ref().is_some_and(|p| p.tempo_agentes));
                    pedido = a.mostrar(ui, &mut self.favo, &self.modelo, agora);
                }
            });
            match pedido {
                Some(apresentacao::Pedido::Sair) => self.sair_da_apresentacao(&ctx, agora),
                Some(apresentacao::Pedido::AbrirTarefa(tarefa)) => {
                    self.sair_da_apresentacao(&ctx, agora);
                    self.abrir_tarefa(tarefa, None);
                }
                None => {}
            }
            self.andar_lousas(&ctx, agora);
            self.atualizar_titulo(&ctx);
            return;
        }

        // O palco da lousa também ocupa a janela inteira.
        if self.palco.is_some() {
            let mut pedido = None;
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.fundo)).show(ui, |ui| {
                if let Some((_, palco)) = &mut self.palco {
                    pedido = palco.mostrar(ui, &self.modelo, agora);
                }
            });
            match pedido {
                Some(lousa::palco::PedidoPalco::Sair) => self.sair_do_palco(&ctx),
                Some(lousa::palco::PedidoPalco::AbrirTarefa(tarefa)) => {
                    let dono = self.palco.as_ref().map(|(d, _)| *d);
                    self.sair_do_palco(&ctx);
                    self.abrir_tarefa(tarefa, None);
                    self.voltar_para_lousa = dono.filter(|d| d.workspace_id != 0).map(|d| d.workspace_id);
                }
                None => {}
            }
            self.andar_lousas(&ctx, agora);
            self.atualizar_titulo(&ctx);
            return;
        }

        // Abrir a tarefa conta como ver o erro: a abelha para de ficar bugada.
        if let Tela::Tarefa { id, .. } = self.tela
            && let Some(t) = self.modelo.tarefas.iter_mut().find(|t| t.id == id)
            && !t.erro_visto
        {
            t.marcar_erros_vistos();
        }

        // Na demonstração, "rodando" é a carga de teste; no uso normal, os agentes ativos.
        let rodando = !self.demo || self.carga != "parada";
        let no_escopo: std::collections::HashSet<i64> = self.projetos_no_escopo().iter().map(|p| p.id).collect();
        let base = abelha::estado_base(self.modelo.tarefas.iter().filter(|t| no_escopo.contains(&t.projeto_id)), rodando);
        let estado = self.abelha.atualizar(base, |p| no_escopo.contains(&p), agora);
        let linha = abelha::linha_de_estado(estado, self.modelo.tarefas.iter().filter(|t| no_escopo.contains(&t.projeto_id)));

        let mut abelha = (None, false);
        egui::Panel::left("lateral")
            .exact_size(236.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.lateral).inner_margin(egui::Margin::symmetric(14, 18)))
            .show(ui, |ui| abelha = self.lateral(ui, agora, &linha));
        if matches!(self.tela, Tela::Entrada(_)) {
            return;
        }
        let (caixa_abelha, abelha_clicada) = abelha;

        self.faixa_nucleo(ui);
        egui::Panel::top("topo")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.fundo).inner_margin(egui::Margin::symmetric(20, 10)))
            .show(ui, |ui| self.topo(ui, agora));

        let mut acoes = Vec::new();
        let mut acoes_linha = Vec::new();
        let mut acoes_lousa = None;
        let mut acoes_banco = Vec::new();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.fundo).inner_margin(egui::Margin { left: 20, right: 20, top: 6, bottom: 16 })).show(
            ui,
            |ui| {
                let area = ui.max_rect().expand2(egui::vec2(20.0, 16.0));
                self.favo.desenhar(ui.painter(), area);
                if !matches!(self.tela, Tela::Tarefa { .. }) {
                    self.tarefa_na_tela = None;
                }
                match self.tela {
                    Tela::Quadro if self.modelo.projetos.is_empty() && self.conexao == Conexao::Ligado => self.sem_projetos(ui),
                    Tela::Quadro => {
                        self.barra(ui, agora);
                        let pode = self.pode_mudar();
                        // O nome do projeto aparece no cartão fora da visão de um projeto só.
                        let rotulos = self.escopo.projeto().is_none().then(|| rotulos_de_projeto(&self.modelo.projetos, &no_escopo));
                        acoes = quadro::mostrar(ui, &mut self.modelo.tarefas, &no_escopo, rotulos.as_ref(), self.filtro.as_deref(), &self.terminais, pode);
                    }
                    Tela::Registro(aba) => {
                        self.barra(ui, agora);
                        let perfil = self.perfil.as_ref().map_or(0, |p| p.id);
                        self.registro.conectado = self.pode_mudar();
                        self.registro.mostrar_tempo = self.perfil.as_ref().is_some_and(|p| p.tempo_agentes);
                        self.registro.pedidos = pedido::resumir(&self.modelo);
                        self.registro.caixa_aberta = self.caixa_pedido.as_ref().map(|(c, _)| c.tarefa);
                        // O conteúdo das páginas tem as próprias margens (24 nas laterais).
                        ui.add_space(4.0);
                        acoes_linha = self.registro.mostrar(ui, aba, perfil, self.escopo.recorte(), &self.modelo.projetos);
                    }
                    Tela::Tarefa { id, foco } => self.painel_tarefa(ui, id, foco, agora),
                    Tela::Lousa(ws) => {
                        // A lousa ocupa o painel inteiro, sem as margens.
                        let m = ui.max_rect();
                        let area = egui::Rect::from_min_max(m.min - egui::vec2(20.0, 6.0), m.max + egui::vec2(20.0, 16.0));
                        let dono = api::DonoLousa { workspace_id: ws, tarefa_id: 0 };
                        // Os avisos da lousa ficam centrados nela, como na da tarefa.
                        self.ancora_aviso = Some(area.center_bottom() - egui::vec2(0.0, 16.0));
                        acoes_lousa = Some((dono, self.mostrar_lousa(ui, area, dono, agora)));
                    }
                    Tela::Banco => {
                        let modelo = &self.modelo;
                        let nomes = |agente: i64| {
                            modelo
                                .tarefas
                                .iter()
                                .flat_map(|t| &t.agentes)
                                .find(|a| a.id == agente)
                                .map_or_else(|| "Um agente".to_string(), |a| a.nome_com_papel())
                        };
                        acoes_banco = self.banco.mostrar(ui, &self.modelo.aprovacoes, &nomes, agora);
                    }
                    Tela::Entrada(_) => {}
                }
            },
        );
        for acao in acoes_banco {
            match acao {
                banco::Acao::AbrirTarefa { tarefa, agente } => self.abrir_tarefa(tarefa, Some(agente)),
                banco::Acao::Avisar(tipo, texto) => self.avisar(tipo, texto, agora),
            }
        }
        for acao in acoes {
            match acao {
                quadro::Acao::AbrirTarefa(id) => self.abrir_tarefa(id, None),
                quadro::Acao::Moveu(id, coluna) => self.mover(id, coluna, &ctx, agora),
                quadro::Acao::Remover(id) if self.demo => self.modelo.tarefas.retain(|t| t.id != id),
                quadro::Acao::Remover(id) => {
                    if let Some(t) = self.modelo.tarefas.iter().find(|t| t.id == id) {
                        self.dialogo = Some(Dialogo::RemoverTarefa { id, titulo: t.titulo.clone(), copia: t.em_copia, erro: None });
                    }
                }
            }
        }
        for acao in acoes_linha {
            match acao {
                registro::Acao::AbrirTarefa { tarefa, agente, lousa } => {
                    self.abrir_tarefa(tarefa, agente);
                    if lousa {
                        // O agente mexeu na lousa: ela abre junto, enquadrada no que ele pôs.
                        self.lousa_na_tarefa.insert(tarefa);
                        self.tarefa_na_tela = Some(tarefa);
                        self.novidade_na_lousa.remove(&tarefa);
                        let dono = api::DonoLousa { workspace_id: 0, tarefa_id: tarefa };
                        self.lousas.entry(dono).or_insert_with(|| lousa::Lousa::nova(&ctx, dono, agora)).enquadrar_agente = true;
                    }
                }
                registro::Acao::IrParaQuadro => self.tela = Tela::Quadro,
                registro::Acao::AbrirBanco(conexao) => {
                    self.banco.abrir_conexao(conexao);
                    self.alternar_banco(true);
                }
                registro::Acao::VerTodos => self.mudar_escopo(Escopo::Perfil),
                registro::Acao::Recorte(r) => self.mudar_escopo(Escopo::do_recorte(r)),
                registro::Acao::AbrirQuadro(projeto) => {
                    self.mudar_escopo(Escopo::Projeto(projeto));
                    self.tela = Tela::Quadro;
                }
                registro::Acao::Avisar(tipo, texto) => self.avisar(tipo, texto, agora),
                registro::Acao::Apresentar { deck, periodo, tarefa } => {
                    self.fechar_caixa_pedido();
                    self.abrir_apresentacao(&ctx, Some(*deck), periodo, tarefa.map_or(Inicio::Capa, Inicio::Tarefa), false)
                }
                registro::Acao::PedirAoAgente { tarefa, tipo, periodo, botao } => {
                    // O mesmo botão de novo fecha (o rascunho fica).
                    let mesma = self.caixa_pedido.as_ref().is_some_and(|(c, _)| c.tarefa == tarefa);
                    self.fechar_caixa_pedido();
                    if !mesma && self.pode_mudar() {
                        let rascunho = self.rascunhos.remove(&tarefa).unwrap_or_default();
                        self.caixa_pedido = Some((pedido::Caixa::nova(&ctx, tarefa, &tipo, &periodo, rascunho), botao));
                    }
                }
            }
        }
        if let Some((dono, acoes)) = acoes_lousa {
            self.tratar_lousa(dono, acoes, &ctx, agora);
        }
        if matches!(self.tela, Tela::Registro(_)) {
            self.mostrar_caixa_pedido(&ctx, agora);
        } else {
            self.fechar_caixa_pedido();
        }
        if let Tela::Tarefa { .. } = self.tela {
            self.mostrar_popover_navegador(&ctx);
        } else {
            self.popover_navegador = None;
            self.gaveta = None;
        }

        if let Some(dialogo) = &mut self.dialogo {
            let perfil = self.perfil.as_ref().map_or(0, |p| p.id);
            let resultado = dialogo.mostrar(&ctx, perfil);
            // Fechado por uma tecla: a tela sem o diálogo precisa de mais um quadro.
            if !matches!(resultado, Resultado::Continua) {
                ctx.request_repaint();
            }
            match resultado {
                Resultado::Continua => {}
                // O resto chega pelos eventos do núcleo.
                Resultado::Fechar | Resultado::Mudou => self.dialogo = None,
                Resultado::ProjetoCriado(id) => {
                    self.dialogo = None;
                    self.mudar_escopo(Escopo::Projeto(id));
                }
                Resultado::AgenteCriado(agente) => {
                    self.dialogo = None;
                    self.agente_criado(*agente, &ctx);
                }
                Resultado::Capturar { agente, tarefa, nao_mostrar } => {
                    self.dialogo = None;
                    if nao_mostrar && let Some(perfil) = &mut self.perfil {
                        perfil.aviso_captura = false;
                        let _ = api::definir_aviso_captura(perfil.id, false);
                    }
                    self.captura = Some(Captura::Agendada { agente, tarefa, quadros: 1 });
                }
                Resultado::HistoricoLimpo(agente) => {
                    self.dialogo = None;
                    self.compositor.historico_limpo(agente);
                    self.avisar(TipoAviso::Neutro, "Histórico de mensagens apagado", agora);
                }
                Resultado::FecharJanela => {
                    self.dialogo = None;
                    self.pode_fechar = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                // O núcleo encerra os agentes (cada um tem uns segundos para salvar) e desliga.
                Resultado::PararEFechar => {
                    self.dialogo = None;
                    self.pode_fechar = true;
                    self.ouvinte = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        if self.abelha.resumo_aberto
            && let Some(caixa) = caixa_abelha
        {
            let ancora = egui::pos2(caixa.right() + 20.0, caixa.bottom());
            let (clicada, area) = abelha::resumo(&ctx, ancora, &self.modelo.tarefas, |p| no_escopo.contains(&p), &self.abelha.conclusoes, rodando, agora);
            if let Some((tarefa, agente)) = clicada {
                self.abrir_tarefa(tarefa, agente);
            } else if area.clicked_elsewhere() && !abelha_clicada {
                self.abelha.resumo_aberto = false;
            }
        }
        self.andar_captura(&ctx);
        self.andar_lousas(&ctx, agora);
        self.mostrar_aviso(&ctx, agora);
        self.atualizar_titulo(&ctx);
    }
}

/// Avisa o núcleo, numa thread, se a tela mostra algo compartilhável e onde
/// o navegador deve abrir.
fn avisar_apresentando(perfil: i64, ativo: bool, geometria: Option<[i32; 4]>) {
    std::thread::spawn(move || {
        if let Err(e) = api::apresentando(perfil, ativo, geometria) {
            eprintln!("avisando a tela ao núcleo: {e}");
        }
    });
}

fn main() -> eframe::Result {
    // O núcleo é iniciado antes da janela; se não der, a tela abre e mostra o motivo.
    let problema = canal::garantir_nucleo().err().map(|e| format!("Núcleo: {e}"));
    if let Some(a) = &problema {
        eprintln!("{a}");
    }
    // COLMEIA_TAMANHO=1280x720 abre a janela nesse tamanho (para conferir telas menores).
    let tamanho = std::env::var("COLMEIA_TAMANHO")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(l, a)| Some([l.trim().parse::<f32>().ok()?, a.trim().parse::<f32>().ok()?])))
        .filter(|[l, a]| *l >= 640.0 && *a >= 480.0)
        .unwrap_or([1600.0, 900.0]);
    let opcoes = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title("Colmeia").with_inner_size(tamanho), ..Default::default() };
    eframe::run_native("colmeia", opcoes, Box::new(move |cc| Ok(Box::new(Colmeia::new(cc, problema)))))
}

#[cfg(test)]
mod testes {
    use super::*;

    const PECAS: PecasBarra = PecasBarra { apresentar: 120.0, copiar: 110.0, mais: 48.0, caixa: 220.0, caixa_curta: 160.0 };

    #[test]
    fn barra_do_registro_nunca_cobre_o_chip_ver() {
        // Larga: tudo na barra, o chip inteiro.
        let a = arranjar_barra(false, false, true, true, PECAS, 900.0, 200.0);
        assert_eq!((a.caixa.is_some(), a.copiar, a.mais), (true, true, false));
        // Meia tela: a caixa vai para o "⋯" (que aparece também na daily).
        let a = arranjar_barra(false, false, true, true, PECAS, 450.0, 200.0);
        assert_eq!((a.caixa, a.copiar, a.mais), (None, true, true));
        assert!(450.0 - a.largura >= CHIP_VER_MINIMO);
        // Mais estreita: "Copiar texto" também vai para o "⋯".
        let a = arranjar_barra(false, true, true, true, PECAS, 330.0, 200.0);
        assert_eq!((a.caixa, a.copiar, a.mais), (None, false, true));
        assert!(330.0 - a.largura >= CHIP_VER_MINIMO);
        // Na sprint o "⋯" está sempre lá.
        assert!(arranjar_barra(false, true, true, true, PECAS, 2000.0, 200.0).mais);
        // Linha do tempo: o rótulo curto antes de ir para o "⋯".
        assert_eq!(arranjar_barra(true, false, false, false, PECAS, 380.0, 200.0).caixa, Some("Mostrar tempo dos agentes"));
        assert_eq!(arranjar_barra(true, false, false, false, PECAS, 330.0, 200.0).caixa, Some("Tempo dos agentes"));
        let a = arranjar_barra(true, false, false, false, PECAS, 250.0, 200.0);
        assert_eq!((a.caixa, a.mais), (None, true));
    }

    #[test]
    fn rotulo_do_projeto_no_quadro_diz_o_workspace_quando_o_nome_se_repete() {
        let projeto = |id: i64, nome: &str, ws: &str| Projeto {
            id,
            nome: nome.into(),
            workspace_id: 0,
            workspace: ws.into(),
            recolhido: false,
            caminho: String::new(),
            sem_git: false,
            branch_padrao: String::new(),
        };
        let projetos = vec![projeto(1, "loja-web", "estudos"), projeto(2, "loja-web", "trabalho-x"), projeto(3, "pedidos-api", "trabalho-x")];
        let todos: HashSet<i64> = [1, 2, 3].into();
        let r = rotulos_de_projeto(&projetos, &todos);
        assert_eq!(r[&1], "estudos · loja-web");
        assert_eq!(r[&2], "trabalho-x · loja-web");
        assert_eq!(r[&3], "pedidos-api");
        // No workspace trabalho-x o nome não se repete.
        let r = rotulos_de_projeto(&projetos, &[2, 3].into());
        assert_eq!(r[&2], "loja-web");
    }
}
