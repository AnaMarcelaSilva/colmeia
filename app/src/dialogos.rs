//! Diálogos: novo projeto, nova tarefa, novo agente e confirmações de remoção. Cada um
//! valida o básico na tela, mas quem decide é o núcleo; a mensagem de erro
//! dele aparece no próprio diálogo.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, RichText};

use crate::api;
use crate::dados::{Coluna, Projeto};
use crate::tema::{self, cores};

/// Seletor de pasta nativo, aberto numa thread para a tela não travar.
#[derive(Default)]
pub struct SeletorPasta {
    recebendo: Option<Receiver<Option<PathBuf>>>,
}

impl SeletorPasta {
    pub fn abrir(&mut self) {
        let (envio, recebendo) = mpsc::channel();
        std::thread::spawn(move || {
            let pasta = rfd::FileDialog::new().set_title("Escolha a pasta do projeto").pick_folder();
            let _ = envio.send(pasta);
        });
        self.recebendo = Some(recebendo);
    }

    pub fn aberto(&self) -> bool {
        self.recebendo.is_some()
    }

    /// A pasta escolhida, uma única vez, quando a janela do seletor fecha.
    pub fn resultado(&mut self) -> Option<PathBuf> {
        let recebido = self.recebendo.as_ref()?.try_recv().ok()?;
        self.recebendo = None;
        recebido
    }
}

pub enum Dialogo {
    NovoProjeto(NovoProjeto),
    NovaTarefa(NovaTarefa),
    NovoAgente(NovoAgente),
    /// `copia`: a tarefa tem cópia isolada, que sai junto.
    RemoverTarefa {
        id: i64,
        titulo: String,
        copia: bool,
        erro: Option<String>,
    },
    RemoverProjeto {
        id: i64,
        nome: String,
        erro: Option<String>,
    },
    /// Primeira captura do perfil: avisa que a imagem guarda o que está visível.
    Captura {
        agente: i64,
        tarefa: i64,
        nao_mostrar: bool,
    },
    /// Fechar a janela com agentes rodando no núcleo.
    Fechar {
        rodando: usize,
        erro: Option<String>,
    },
    /// Você parou um agente rodando: mover a tarefa, excluir ou deixar.
    /// Nada é gravado até a escolha.
    AgenteParado {
        tarefa: i64,
        titulo: String,
        coluna: Coluna,
        copia: bool,
        erro: Option<String>,
    },
    /// Escolher o acabamento e o nome da abelha do perfil.
    Mascote(EscolhaMascote),
    /// Ligar o ditado por voz: o tamanho do modelo, o download e o progresso.
    Ditado(AjustesDitado),
    /// Os comandos falados, a voz da abelha e o ouvir o nome.
    Voz(Box<crate::ajustes_voz::AjustesVoz>),
    /// Apagar o histórico de mensagens de um agente.
    LimparHistorico {
        agente: i64,
        nome: String,
        guardadas: usize,
        erro: Option<String>,
    },
}

pub enum Resultado {
    Continua,
    Fechar,
    /// Algo mudou no núcleo: a tela recarrega projetos e tarefas.
    Mudou,
    ProjetoCriado(i64),
    /// O agente foi criado e o terminal dele já está rodando no núcleo.
    AgenteCriado(Box<api::Agente>),
    /// Pode capturar (e, se pediu, não mostrar o aviso de novo).
    Capturar {
        agente: i64,
        tarefa: i64,
        nao_mostrar: bool,
    },
    /// O histórico de mensagens do agente foi apagado no núcleo.
    HistoricoLimpo(i64),
    /// Excluir a tarefa: passa à confirmação de sempre.
    RemoverTarefa {
        id: i64,
        titulo: String,
        copia: bool,
    },
    /// Fechar só a janela: os agentes seguem no núcleo.
    FecharJanela,
    /// O ditado do perfil ligou ou desligou no núcleo.
    Ditado(bool),
    /// Ligar o ouvir o nome: a abelha fixa abre (é ela que ouve) e a opção liga.
    OuvirNome,
    /// O núcleo já foi encerrado (e os agentes com ele): fechar a janela.
    PararEFechar,
}

pub struct NovoProjeto {
    workspaces: Vec<api::Workspace>,
    workspace: Option<i64>,
    novo_workspace: String,
    pasta: String,
    nome: String,
    seletor: SeletorPasta,
    erro: Option<String>,
}

impl NovoProjeto {
    pub fn new(perfil: i64) -> Self {
        let workspaces = api::workspaces(perfil).unwrap_or_default();
        NovoProjeto {
            workspace: workspaces.first().map(|w| w.id),
            workspaces,
            novo_workspace: String::new(),
            pasta: String::new(),
            nome: String::new(),
            seletor: SeletorPasta::default(),
            erro: None,
        }
    }

    /// Já no workspace dado (o do painel); `None` começa num workspace novo.
    pub fn no_workspace(mut self, workspace: Option<i64>) -> Self {
        self.workspace = match workspace {
            Some(id) if self.workspaces.iter().any(|w| w.id == id) => Some(id),
            Some(_) => self.workspace,
            None => None,
        };
        self
    }
}

pub struct NovaTarefa {
    projeto: Projeto,
    /// Em "Todos os projetos": os projetos para escolher (vazio quando já há um em foco).
    projetos: Vec<Projeto>,
    branches: Vec<String>,
    /// Branch existente escolhida (ou a da pasta, sem cópia).
    branch: String,
    /// Com cópia isolada: branch nova (com nome e base) ou uma existente.
    copia: bool,
    nova: bool,
    nova_branch: String,
    base: String,
    titulo: String,
    erro: Option<String>,
    /// Foco no título uma vez só, ao abrir.
    focar: bool,
    /// Onde ela entra: o Backlog, ou a coluna do "+" no quadro.
    coluna: Coluna,
}

impl NovaTarefa {
    pub fn new(projeto: Projeto, projetos: Vec<Projeto>) -> Self {
        let branches = if projeto.sem_git { Vec::new() } else { api::branches(projeto.id).unwrap_or_else(|_| vec![projeto.branch_padrao.clone()]) };
        NovaTarefa {
            projetos,
            branch: projeto.branch_padrao.clone(),
            base: projeto.branch_padrao.clone(),
            copia: !projeto.sem_git,
            nova: true,
            nova_branch: String::new(),
            branches,
            projeto,
            titulo: String::new(),
            erro: None,
            focar: true,
            coluna: Coluna::Backlog,
        }
    }

    pub fn na_coluna(mut self, coluna: Coluna) -> Self {
        self.coluna = coluna;
        self
    }
}

/// Ferramenta que pode virar agente: id na API e nome para mostrar.
pub struct Opcao {
    pub id: String,
    pub nome: String,
}

pub struct NovoAgente {
    tarefa: i64,
    pasta: String,
    ferramentas: Vec<Opcao>,
    ferramenta: usize,
    papel: usize,
    conversas: Result<api::Sessoes, String>,
    /// Conversa do Claude Code a retomar; None começa uma nova.
    conversa: Option<usize>,
    erro: Option<String>,
}

pub const PAPEIS: [&str; 4] = ["líder", "dev", "revisor", "testador"];

impl NovoAgente {
    /// `retomar` já deixa escolhida a conversa mais recente do Claude Code na pasta.
    pub fn new(tarefa: i64, pasta: String, ferramentas: Vec<Opcao>, retomar: bool) -> Self {
        let conversas = api::sessoes(tarefa);
        let conversa = if retomar && conversas.as_ref().is_ok_and(|c| !c.sessoes.is_empty()) { Some(0) } else { None };
        let ferramenta = if retomar { ferramentas.iter().position(|f| f.id == "claude").unwrap_or(0) } else { 0 };
        NovoAgente { tarefa, pasta, ferramentas, ferramenta, papel: 1, conversas, conversa, erro: None }
    }
}

impl Dialogo {
    pub fn mostrar(&mut self, ctx: &egui::Context, perfil: i64) -> Resultado {
        let modal = egui::Modal::new(egui::Id::new("dialogo"))
            .frame(tema::moldura_janela())
            .backdrop_color(egui::Color32::from_black_alpha(if tema::claro() { 60 } else { 140 }))
            .show(ctx, |ui| {
                ui.set_width(480.0);
                match self {
                    Dialogo::NovoProjeto(d) => novo_projeto(ui, d, perfil),
                    Dialogo::NovaTarefa(d) => nova_tarefa(ui, d),
                    Dialogo::NovoAgente(d) => novo_agente(ui, d),
                    Dialogo::RemoverTarefa { id, titulo, copia, erro } => {
                        let texto = if *copia {
                            format!("\"{titulo}\" sai do quadro, os agentes dela são encerrados e a cópia isolada é apagada. A branch continua no repositório.")
                        } else {
                            format!("\"{titulo}\" sai do quadro e os agentes dela são encerrados. Nenhuma branch nem arquivo é alterado.")
                        };
                        confirmar(ui, "Remover tarefa", &texto, erro, || api::remover_tarefa(*id))
                    }
                    Dialogo::RemoverProjeto { id, nome, erro } => confirmar(
                        ui,
                        "Remover projeto da Colmeia",
                        &format!("\"{nome}\" e as tarefas dele saem da Colmeia. A pasta do projeto não é apagada."),
                        erro,
                        || api::remover_projeto(*id),
                    ),
                    Dialogo::Captura { agente, tarefa, nao_mostrar } => {
                        tema::cabecalho(
                            ui,
                            "Capturar terminal",
                            "A imagem guarda tudo o que está visível, inclusive senhas ou chaves que estiverem na tela. Ela fica só neste computador, anexada à tarefa.",
                        );
                        ui.add_space(14.0);
                        tema::caixa_marcar(ui, "Não mostrar de novo", nao_mostrar);
                        match rodape(ui, "Capturar", true) {
                            (true, _) => Resultado::Fechar,
                            (_, true) => Resultado::Capturar { agente: *agente, tarefa: *tarefa, nao_mostrar: *nao_mostrar },
                            _ => Resultado::Continua,
                        }
                    }
                    Dialogo::Fechar { rodando, erro } => fechar(ui, *rodando, erro),
                    Dialogo::LimparHistorico { agente, nome, guardadas, erro } => limpar_historico(ui, *agente, nome, *guardadas, erro),
                    Dialogo::Mascote(d) => escolher_mascote(ui, d, perfil),
                    Dialogo::Ditado(d) => ajustes_ditado(ui, d, perfil),
                    Dialogo::Voz(d) => crate::ajustes_voz::mostrar(ui, d),
                    Dialogo::AgenteParado { tarefa, titulo, coluna, copia, erro } => agente_parado(ui, *tarefa, titulo, *coluna, *copia, erro),
                }
            });
        match modal.inner {
            Resultado::Continua if modal.should_close() => Resultado::Fechar,
            outro => outro,
        }
    }
}

pub struct EscolhaMascote {
    modelo: mascote::Modelo,
    nome: String,
    erro: Option<String>,
}

impl EscolhaMascote {
    pub fn new(modelo: &str, nome: &str) -> Self {
        EscolhaMascote { modelo: mascote::Modelo::do_codigo(modelo), nome: nome.to_string(), erro: None }
    }
}

/// Um instante fixo para as prévias: o diálogo parado não redesenha.
const INSTANTE_DA_PREVIA: mascote::Tempo = mascote::Tempo { total: 1.3, no_estado: 0.6 };

/// O acabamento e o nome da abelha: os cinco modelos lado a lado, os cinco
/// estados do escolhido e o nome, que aparece no resumo, nos balões e na
/// abelha fixa. Grava no perfil; as outras telas recebem pelo evento.
fn escolher_mascote(ui: &mut egui::Ui, d: &mut EscolhaMascote, perfil: i64) -> Resultado {
    tema::cabecalho(ui, "A abelha deste perfil", "A mesma personagem em cinco acabamentos. O nome aparece no resumo, nos balões e na abelha fixa.");
    ui.add_space(16.0);
    let p = cores();
    let (vao, lado) = (6.0, (ui.available_width() - 4.0 * 6.0) / 5.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = vao;
        for modelo in mascote::Modelo::TODOS {
            let (rect, resposta) = ui.allocate_exact_size(egui::vec2(lado, lado + 18.0), egui::Sense::click());
            let escolhido = modelo == d.modelo;
            if escolhido {
                ui.painter().rect_filled(rect, 12, tema::fundo_escolhido());
                ui.painter().rect_stroke(rect, 12, egui::Stroke::new(1.5, p.destaque), egui::StrokeKind::Inside);
            } else {
                tema::fundo_ao_passar(ui, &resposta, rect, 12, true);
            }
            let caixa = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 4.0), egui::vec2(lado, lado)).shrink(10.0);
            mascote::desenhar(&ui.painter_at(rect), caixa.center(), caixa.height(), modelo, mascote::Estado::Trabalhando, INSTANTE_DA_PREVIA);
            let cor = if escolhido { p.texto } else { p.suave };
            ui.painter().text(
                egui::pos2(rect.center().x, rect.bottom() - 12.0),
                egui::Align2::CENTER_CENTER,
                modelo.nome(),
                egui::FontId::proportional(11.5),
                cor,
            );
            if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                d.modelo = modelo;
            }
        }
    });
    ui.add_space(14.0);
    rotulo(ui, "Os estados");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = vao;
        for estado in mascote::Estado::TODOS {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(lado, lado * 0.8 + 16.0), egui::Sense::hover());
            let caixa = egui::Rect::from_min_size(rect.min, egui::vec2(lado, lado * 0.8)).shrink(8.0);
            mascote::desenhar(&ui.painter_at(rect), caixa.center(), caixa.height(), d.modelo, estado, INSTANTE_DA_PREVIA);
            ui.painter().text(
                egui::pos2(rect.center().x, rect.bottom() - 8.0),
                egui::Align2::CENTER_CENTER,
                estado.nome(),
                egui::FontId::proportional(11.0),
                p.suave,
            );
        }
    });
    ui.add_space(14.0);
    tema::campo(ui, "Nome", &mut d.nome, api::NOME_DA_ABELHA);
    mostrar_erro(ui, &d.erro);
    let nome = d.nome.trim();
    match rodape(ui, "Salvar", !nome.is_empty()) {
        (true, _) => Resultado::Fechar,
        (_, true) => match api::definir_mascote(perfil, d.modelo.codigo(), nome) {
            Ok(()) => Resultado::Fechar,
            Err(e) => {
                d.erro = Some(format!("Não consegui salvar a abelha: {e}"));
                Resultado::Continua
            }
        },
        _ => Resultado::Continua,
    }
}

pub struct AjustesDitado {
    pub modelo: api::EstadoVoz,
    pub ligado: bool,
    erro: Option<String>,
}

impl AjustesDitado {
    /// O estado do modelo vem do núcleo ao abrir; depois, pelos eventos.
    pub fn new(ligado: bool) -> Self {
        let (modelo, erro) = match api::estado_voz() {
            Ok(m) => (m, None),
            Err(e) => (api::EstadoVoz::default(), Some(format!("Não consegui falar com o núcleo: {e}"))),
        };
        AjustesDitado { modelo, ligado, erro }
    }
}

/// "148 MB", como os gerenciadores de arquivos mostram.
pub fn megabytes(bytes: i64) -> String {
    format!("{} MB", (bytes as f64 / 1_000_000.0).round() as i64)
}

/// O ditado por voz: o que ele faz, o tamanho do modelo antes de baixar, o
/// progresso com Cancelar e, com o modelo no computador, ligar, desligar e apagar.
fn ajustes_ditado(ui: &mut egui::Ui, d: &mut AjustesDitado, perfil: i64) -> Resultado {
    tema::cabecalho(
        ui,
        "Ditado por voz",
        "Fale em vez de digitar na caixa de mensagem das tarefas. O texto aparece na caixa para você revisar e nunca é enviado sozinho. A transcrição roda neste computador, sem internet e sem tokens, e o áudio não é gravado.",
    );
    ui.add_space(14.0);
    let p = cores();
    let tamanho = megabytes(d.modelo.tamanho.max(1));
    let pronto = d.modelo.estado == "pronto";
    let baixando = d.modelo.estado == "baixando";
    let texto = match (d.ligado, pronto, baixando) {
        (true, true, _) => format!("Ligado. Clique no microfone da caixa de mensagem ou segure {} enquanto fala.", crate::voz::ATALHO),
        (_, _, true) => "Baixando o modelo. Pode fechar esta janela: o ditado liga sozinho quando o download terminar.".to_string(),
        (_, true, _) => format!("O modelo de voz ({tamanho}) já está neste computador."),
        _ => format!(
            "Para ligar, a Colmeia baixa uma vez o modelo de voz Whisper base (multilíngue), com {tamanho}, do repositório oficial do whisper.cpp, e confere o arquivo antes de usar. Depois disso, o ditado funciona sem internet."
        ),
    };
    ui.label(RichText::new(texto).color(p.texto).size(13.0));
    if baixando {
        ui.add_space(12.0);
        let fracao = d.modelo.baixados as f32 / d.modelo.tamanho.max(1) as f32;
        ui.add(egui::ProgressBar::new(fracao).fill(p.destaque).desired_height(8.0));
        ui.add_space(4.0);
        ui.label(RichText::new(format!("{} de {tamanho}", megabytes(d.modelo.baixados))).color(p.suave).size(12.0));
    }
    if d.modelo.estado == "erro" && !d.modelo.erro.is_empty() {
        ui.add_space(10.0);
        ui.label(RichText::new(&d.modelo.erro).color(p.erro).size(12.5));
    }
    mostrar_erro(ui, &d.erro);

    let (mut fechar, mut acao, mut apagar) = (false, false, false);
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        fechar = tema::botao_secundario(ui, if baixando || d.ligado { "Fechar" } else { "Cancelar" }).clicked();
        if pronto && !baixando {
            apagar = tema::botao_secundario(ui, &format!("Apagar o modelo ({tamanho})"))
                .on_hover_text("Libera o espaço; o ditado desliga em todos os perfis.")
                .clicked();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let rotulo = match (d.ligado && pronto, pronto, baixando) {
                (true, _, _) => "Desligar".to_string(),
                (_, _, true) => "Cancelar download".to_string(),
                (_, true, _) => "Ligar".to_string(),
                _ => format!("Baixar {tamanho} e ligar"),
            };
            acao = if d.ligado && pronto || baixando { tema::botao_alerta(ui, &rotulo).clicked() } else { tema::botao_principal(ui, &rotulo, true).clicked() };
        });
    });
    if fechar {
        return Resultado::Fechar;
    }
    let falhou = |d: &mut AjustesDitado, o_que: &str, e: String| {
        d.erro = Some(format!("Não consegui {o_que}: {e}"));
        Resultado::Continua
    };
    if apagar {
        return match api::remover_voz() {
            Ok(m) => {
                d.modelo = m;
                let estava = std::mem::replace(&mut d.ligado, false);
                if estava { Resultado::Ditado(false) } else { Resultado::Continua }
            }
            Err(e) => falhou(d, "apagar o modelo", e),
        };
    }
    if !acao {
        return Resultado::Continua;
    }
    d.erro = None;
    match (d.ligado && pronto, pronto, baixando) {
        (true, _, _) | (_, true, _) => {
            let ligar = !d.ligado;
            match api::definir_ditado(perfil, ligar) {
                Ok(()) => Resultado::Ditado(ligar),
                Err(e) => falhou(d, if ligar { "ligar o ditado" } else { "desligar o ditado" }, e),
            }
        }
        (_, _, true) => match api::cancelar_voz() {
            Ok(()) => Resultado::Continua,
            Err(e) => falhou(d, "cancelar o download", e),
        },
        _ => match api::baixar_voz(perfil) {
            Ok(m) => {
                d.modelo = m;
                Resultado::Continua
            }
            Err(e) => falhou(d, "começar o download", e),
        },
    }
}

fn mostrar_erro(ui: &mut egui::Ui, erro: &Option<String>) {
    if let Some(e) = erro {
        ui.add_space(10.0);
        ui.label(RichText::new(e).color(cores().erro).size(12.5));
    }
}

/// Linha de botões do rodapé: Cancelar à esquerda, a ação à direita. Em todos
/// os diálogos Enter confirma (se a ação estiver habilitada) e Esc cancela.
fn rodape(ui: &mut egui::Ui, acao: &str, habilitado: bool) -> (bool, bool) {
    let (mut cancelar, mut confirmar) = (false, false);
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            confirmar = tema::botao_principal(ui, acao, habilitado).clicked();
        });
    });
    confirmar |= habilitado && ui.input(|i| i.key_pressed(egui::Key::Enter));
    (cancelar, confirmar)
}

/// Fechar a janela com agentes rodando: eles continuam no núcleo e gastam a conta de IA.
fn fechar(ui: &mut egui::Ui, rodando: usize, erro: &mut Option<String>) -> Resultado {
    let titulo = if rodando == 1 { "1 agente continua rodando".to_string() } else { format!("{rodando} agentes continuam rodando") };
    tema::cabecalho(
        ui,
        &titulo,
        "Fechar a janela não para os agentes: eles seguem no núcleo e usam a conta de IA. Para parar todos, o núcleo é encerrado e cada agente tem uns segundos para salvar a conversa.",
    );
    mostrar_erro(ui, erro);
    let (mut cancelar, mut parar, mut fechar) = (false, false, false);
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        cancelar = tema::botao_secundario(ui, "Cancelar").on_hover_text("Esc").clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            fechar = tema::botao_principal(ui, "Fechar a janela", true).clicked();
            ui.add_space(8.0);
            // A ação que encerra os agentes, no estilo de alerta.
            parar = tema::botao_alerta(ui, "Parar todos e fechar").clicked();
        });
    });
    if cancelar {
        return Resultado::Fechar;
    }
    if fechar || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        return Resultado::FecharJanela;
    }
    if parar {
        return match api::encerrar_nucleo() {
            Ok(()) => Resultado::PararEFechar,
            Err(e) => {
                *erro = Some(format!("Não consegui encerrar o núcleo: {e}"));
                Resultado::Continua
            }
        };
    }
    Resultado::Continua
}

/// Apagar as mensagens guardadas de um agente (a seta para cima deixa de trazê-las).
fn limpar_historico(ui: &mut egui::Ui, agente: i64, nome: &str, guardadas: usize, erro: &mut Option<String>) -> Resultado {
    let texto =
        if guardadas == 1 { format!("Apagar a mensagem guardada de «{nome}»?") } else { format!("Apagar as {guardadas} mensagens guardadas de «{nome}»?") };
    tema::cabecalho(ui, "Limpar histórico", &texto);
    mostrar_erro(ui, erro);
    let (mut cancelar, mut apagar) = (false, false);
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        cancelar = tema::botao_secundario(ui, "Cancelar").on_hover_text("Esc").clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            apagar = tema::botao_alerta(ui, "Apagar").clicked();
        });
    });
    if cancelar {
        return Resultado::Fechar;
    }
    if apagar {
        return match api::limpar_mensagens(agente) {
            Ok(()) => Resultado::HistoricoLimpo(agente),
            Err(e) => {
                *erro = Some(format!("Não consegui apagar: {e}"));
                Resultado::Continua
            }
        };
    }
    Resultado::Continua
}

/// Depois de parar um agente pela tela: para onde vai a tarefa. As colunas
/// são botões; a atual fica de fora. Esc ou "Deixar como está" não mudam nada.
fn agente_parado(ui: &mut egui::Ui, tarefa: i64, titulo: &str, coluna: Coluna, copia: bool, erro: &mut Option<String>) -> Resultado {
    tema::cabecalho(ui, "Você parou o agente. O que fazer com a tarefa?", &format!("\"{titulo}\" está em {}. Nada muda até você escolher.", coluna.nome()));
    mostrar_erro(ui, erro);
    ui.add_space(14.0);
    ui.label(RichText::new("Mover para").color(cores().suave).size(12.5));
    ui.add_space(6.0);
    let mut destino = None;
    ui.horizontal_wrapped(|ui| {
        for c in Coluna::TODAS.into_iter().filter(|c| *c != coluna) {
            if tema::botao_secundario(ui, c.nome()).clicked() {
                destino = Some(c);
            }
        }
    });
    let (mut deixar, mut excluir) = (false, false);
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        deixar = tema::botao_secundario(ui, "Deixar como está").on_hover_text("Esc").clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            excluir = tema::botao_alerta(ui, "Excluir tarefa…").clicked();
        });
    });
    if deixar {
        return Resultado::Fechar;
    }
    if excluir {
        return Resultado::RemoverTarefa { id: tarefa, titulo: titulo.to_string(), copia };
    }
    match destino.map(|c| api::mover_tarefa(tarefa, c.chave())) {
        Some(Ok(_)) => Resultado::Mudou,
        Some(Err(e)) => {
            *erro = Some(format!("Não consegui mover a tarefa: {e}"));
            Resultado::Continua
        }
        None => Resultado::Continua,
    }
}

fn novo_projeto(ui: &mut egui::Ui, d: &mut NovoProjeto, perfil: i64) -> Resultado {
    tema::cabecalho(
        ui,
        "Novo projeto",
        "Aponte para um repositório git ou para qualquer pasta de trabalho (sem git, ela entra sem branches). A Colmeia não altera nada ao adicionar.",
    );
    ui.add_space(16.0);

    // Workspace: um dos existentes ou um novo.
    let atual = d.workspaces.iter().find(|w| Some(w.id) == d.workspace).map(|w| w.nome.clone()).unwrap_or_else(|| "novo".into());
    let resposta = tema::chip(ui, "Workspace", &atual, false);
    egui::Popup::menu(&resposta).show(|ui| {
        ui.set_min_width(240.0);
        for w in &d.workspaces {
            if tema::opcao_menu(ui, &w.nome, Some(w.id) == d.workspace) {
                d.workspace = Some(w.id);
                ui.close();
            }
        }
        if tema::opcao_menu(ui, "+ Novo workspace", d.workspace.is_none()) {
            d.workspace = None;
            ui.close();
        }
    });
    if d.workspace.is_none() {
        ui.add_space(10.0);
        tema::campo(ui, "Nome do novo workspace", &mut d.novo_workspace, "Ex.: Empresa X");
    }
    ui.add_space(12.0);

    if let Some(pasta) = d.seletor.resultado() {
        d.pasta = pasta.display().to_string();
    }
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 150.0);
            tema::campo(ui, "Pasta do projeto", &mut d.pasta, "/home/voce/projetos/meu-projeto");
        });
        ui.vertical(|ui| {
            ui.add_space(20.0);
            let rotulo = if d.seletor.aberto() { "Escolhendo…" } else { "Escolher pasta…" };
            if tema::botao_secundario(ui, rotulo).clicked() && !d.seletor.aberto() {
                d.seletor.abrir();
            }
        });
    });
    ui.add_space(12.0);
    let sugestao = std::path::Path::new(d.pasta.trim()).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    tema::campo(ui, "Nome do projeto", &mut d.nome, if sugestao.is_empty() { "Ex.: loja-web" } else { &sugestao });
    mostrar_erro(ui, &d.erro);

    let pronto = !d.pasta.trim().is_empty() && (d.workspace.is_some() || !d.novo_workspace.trim().is_empty());
    let (cancelar, criar) = rodape(ui, "Adicionar projeto", pronto);
    if cancelar {
        return Resultado::Fechar;
    }
    if !criar {
        return Resultado::Continua;
    }
    let workspace = match d.workspace {
        Some(id) => id,
        None => match api::criar_workspace(perfil, d.novo_workspace.trim()) {
            Ok(w) => {
                d.workspaces.push(w.clone());
                d.workspace = Some(w.id);
                w.id
            }
            Err(e) => {
                d.erro = Some(format!("Workspace: {e}"));
                return Resultado::Continua;
            }
        },
    };
    let nome = if d.nome.trim().is_empty() { sugestao } else { d.nome.trim().to_string() };
    match api::criar_projeto(workspace, &nome, d.pasta.trim()) {
        Ok(p) => Resultado::ProjetoCriado(p.id),
        Err(e) => {
            d.erro = Some(e);
            Resultado::Continua
        }
    }
}

fn nova_tarefa(ui: &mut egui::Ui, d: &mut NovaTarefa) -> Resultado {
    tema::cabecalho(ui, "Nova tarefa", &format!("Em {}. Ela entra em {}.", d.projeto.nome, d.coluna.nome()));
    ui.add_space(16.0);
    if !d.projetos.is_empty() {
        let resposta = tema::chip(ui, "Projeto", &d.projeto.nome, false);
        let mut escolhido = None;
        egui::Popup::menu(&resposta).show(|ui| {
            ui.set_min_width(240.0);
            for p in &d.projetos {
                if tema::opcao_menu(ui, &p.nome, p.id == d.projeto.id) {
                    escolhido = Some(p.clone());
                    ui.close();
                }
            }
        });
        if let Some(p) = escolhido {
            let titulo = std::mem::take(&mut d.titulo);
            *d = NovaTarefa::new(p, std::mem::take(&mut d.projetos)).na_coluna(d.coluna);
            d.titulo = titulo;
        }
        ui.add_space(12.0);
    }
    let resposta = tema::campo(ui, "Título", &mut d.titulo, "Ex.: Nova tela de pedidos");
    if std::mem::take(&mut d.focar) {
        resposta.request_focus();
    }
    ui.add_space(14.0);

    if d.projeto.sem_git {
        dica(ui, &format!("Pasta sem git: os agentes trabalham direto em {}.", d.projeto.caminho));
    } else {
        rotulo(ui, "Onde os agentes trabalham");
        let atual = if d.copia { 0 } else { 1 };
        if let Some(i) = tema::segmentado(ui, &["Cópia isolada", "Direto na pasta"], atual) {
            d.copia = i == 0;
        }
        ui.add_space(10.0);
        if d.copia {
            if let Some(i) = tema::segmentado(ui, &["Branch nova", "Branch existente"], if d.nova { 0 } else { 1 }) {
                d.nova = i == 0;
            }
            ui.add_space(10.0);
            if d.nova {
                let sugestao = sugerir_branch(&d.titulo);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() - 190.0);
                        tema::campo(ui, "Nome da branch", &mut d.nova_branch, &sugestao);
                    });
                    ui.vertical(|ui| {
                        ui.add_space(20.0);
                        escolher_branch(ui, "A partir de", &d.branches, &mut d.base);
                    });
                });
            } else {
                escolher_branch(ui, "Branch", &d.branches, &mut d.branch);
            }
            ui.add_space(6.0);
            dica(ui, "A cópia fica numa pasta própria, na branch da tarefa: os agentes não mexem na sua pasta de trabalho.");
        } else {
            escolher_branch(ui, "Branch", &d.branches, &mut d.branch);
            ui.add_space(6.0);
            dica(ui, &format!("Os agentes trabalham em {}, na branch que estiver nela.", d.projeto.caminho));
        }
    }
    mostrar_erro(ui, &d.erro);

    let pronto = !d.titulo.trim().is_empty();
    let (cancelar, criar) = rodape(ui, "Criar tarefa", pronto);
    if cancelar {
        return Resultado::Fechar;
    }
    if !criar {
        return Resultado::Continua;
    }
    let nova_branch = if d.nova_branch.trim().is_empty() { sugerir_branch(&d.titulo) } else { d.nova_branch.trim().to_string() };
    let branch = if d.projeto.sem_git {
        ""
    } else if d.copia && d.nova {
        &nova_branch
    } else {
        &d.branch
    };
    let pedido = api::NovaTarefa { titulo: d.titulo.trim(), branch, copia: d.copia, nova: d.nova, base: &d.base };
    match api::criar_tarefa(d.projeto.id, &pedido) {
        Ok(t) => {
            // Se não der para mover, ela fica no Backlog, à vista: criar de
            // novo daria uma tarefa repetida.
            if d.coluna != Coluna::Backlog {
                let _ = api::mover_tarefa(t.id, d.coluna.chave());
            }
            Resultado::Mudou
        }
        Err(e) => {
            d.erro = Some(e);
            Resultado::Continua
        }
    }
}

fn novo_agente(ui: &mut egui::Ui, d: &mut NovoAgente) -> Resultado {
    tema::cabecalho(ui, "Adicionar agente", &format!("Abre em {}, com a conta de IA do perfil.", d.pasta));
    ui.add_space(16.0);
    rotulo(ui, "Ferramenta");
    let nomes: Vec<&str> = d.ferramentas.iter().map(|f| f.nome.as_str()).collect();
    if let Some(i) = tema::segmentado(ui, &nomes, d.ferramenta) {
        d.ferramenta = i;
    }
    ui.add_space(12.0);
    rotulo(ui, "Papel");
    if let Some(i) = tema::segmentado(ui, &PAPEIS, d.papel) {
        d.papel = i;
    }

    let claude = d.ferramentas.get(d.ferramenta).is_some_and(|f| f.id == "claude");
    if claude {
        ui.add_space(14.0);
        rotulo(ui, "Conversa");
        match &d.conversas {
            Err(e) => {
                ui.label(RichText::new(format!("Não consegui ler as conversas guardadas: {e}")).color(cores().erro).size(12.5));
            }
            Ok(conversas) => {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    if tema::opcao_menu(ui, "Começar uma conversa nova", d.conversa.is_none()) {
                        d.conversa = None;
                    }
                    for (i, c) in conversas.sessoes.iter().enumerate() {
                        let titulo = if c.titulo.is_empty() { "(sem título)" } else { &c.titulo };
                        if tema::opcao_menu(ui, &format!("Retomar: {titulo}  ·  {}", quando(&c.alterada)), d.conversa == Some(i)) {
                            d.conversa = Some(i);
                        }
                    }
                });
                if conversas.sessoes.is_empty() {
                    dica(ui, "Nenhuma conversa do Claude Code guardada para esta pasta nesta conta.");
                }
                if conversas.aberto_fora && d.conversa.is_some() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "O Claude Code está aberto nesta pasta fora da Colmeia (no IntelliJ ou num terminal). Feche a conversa lá antes de retomar aqui, para as duas não se atropelarem.",
                        )
                        .color(cores().alerta)
                        .size(12.5),
                    );
                }
            }
        }
    }
    mostrar_erro(ui, &d.erro);

    let acao = if claude && d.conversa.is_some() { "Retomar conversa" } else { "Abrir agente" };
    let (cancelar, criar) = rodape(ui, acao, !d.ferramentas.is_empty());
    if cancelar {
        return Resultado::Fechar;
    }
    if !criar {
        return Resultado::Continua;
    }
    let ferramenta = &d.ferramentas[d.ferramenta].id;
    let sessao = match (&d.conversas, d.conversa) {
        (Ok(c), Some(i)) if claude => c.sessoes.get(i).map(|s| s.id.as_str()).unwrap_or(""),
        _ => "",
    };
    match api::criar_agente(d.tarefa, ferramenta, PAPEIS[d.papel], sessao) {
        Ok(agente) => Resultado::AgenteCriado(Box::new(agente)),
        Err(e) => {
            d.erro = Some(e);
            Resultado::Continua
        }
    }
}

fn rotulo(ui: &mut egui::Ui, texto: &str) {
    ui.label(RichText::new(texto).color(cores().suave).size(12.5));
    ui.add_space(4.0);
}

fn dica(ui: &mut egui::Ui, texto: &str) {
    ui.label(RichText::new(texto).color(cores().suave).size(12.0));
}

fn escolher_branch(ui: &mut egui::Ui, rotulo: &str, branches: &[String], escolhida: &mut String) {
    let resposta = tema::chip(ui, rotulo, escolhida, false);
    egui::Popup::menu(&resposta).show(|ui| {
        ui.set_min_width(240.0);
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for b in branches {
                if tema::opcao_menu(ui, b, b == escolhida) {
                    *escolhida = b.clone();
                    ui.close();
                }
            }
        });
    });
}

/// Nome de branch a partir do título: "Nova tela de pedidos" vira "tarefa/nova-tela-de-pedidos".
pub fn sugerir_branch(titulo: &str) -> String {
    let mut nome = String::new();
    for c in titulo.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            nome.push(c);
        } else if !nome.is_empty() && !nome.ends_with('-') {
            nome.push('-');
        }
    }
    let nome: String = nome.trim_end_matches('-').chars().take(50).collect();
    let nome = nome.trim_end_matches('-');
    if nome.is_empty() { "tarefa/nova".into() } else { format!("tarefa/{nome}") }
}

/// "2026-09-30T11:22:05.1-03:00" vira "30/09 11:22".
pub fn quando(momento: &str) -> String {
    match (momento.get(5..7), momento.get(8..10), momento.get(11..16)) {
        (Some(mes), Some(dia), Some(hora)) => format!("{dia}/{mes} {hora}"),
        _ => momento.to_string(),
    }
}

fn confirmar(ui: &mut egui::Ui, titulo: &str, texto: &str, erro: &mut Option<String>, acao: impl FnOnce() -> Result<(), String>) -> Resultado {
    tema::cabecalho(ui, titulo, texto);
    mostrar_erro(ui, erro);
    let (cancelar, remover) = rodape(ui, "Remover", true);
    if cancelar {
        return Resultado::Fechar;
    }
    if !remover {
        return Resultado::Continua;
    }
    match acao() {
        Ok(()) => Resultado::Mudou,
        Err(e) => {
            *erro = Some(e);
            Resultado::Continua
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn branch_sugerida_do_titulo() {
        assert_eq!(sugerir_branch("Nova tela de pedidos"), "tarefa/nova-tela-de-pedidos");
        assert_eq!(sugerir_branch("Correção: botão não salva!"), "tarefa/correcao-botao-nao-salva");
        assert_eq!(sugerir_branch("  "), "tarefa/nova");
    }

    #[test]
    fn data_curta() {
        assert_eq!(quando("2026-09-30T11:22:05.123-03:00"), "30/09 11:22");
    }
}
