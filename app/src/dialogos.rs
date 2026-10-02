//! Diálogos: novo projeto, nova tarefa, novo agente e confirmações de remoção. Cada um
//! valida o básico na tela, mas quem decide é o núcleo; a mensagem de erro
//! dele aparece no próprio diálogo.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, RichText};

use crate::api;
use crate::dados::Projeto;
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
}

pub enum Resultado {
    Continua,
    Fechar,
    /// Algo mudou no núcleo: a tela recarrega projetos e tarefas.
    Mudou,
    ProjetoCriado(i64),
    /// O agente foi criado e o terminal dele já está rodando no núcleo.
    AgenteCriado(api::Agente),
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
}

pub struct NovaTarefa {
    projeto: Projeto,
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
}

impl NovaTarefa {
    pub fn new(projeto: Projeto) -> Self {
        let branches = if projeto.sem_git { Vec::new() } else { api::branches(projeto.id).unwrap_or_else(|_| vec![projeto.branch_padrao.clone()]) };
        NovaTarefa {
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
        }
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
                }
            });
        match modal.inner {
            Resultado::Continua if modal.should_close() => Resultado::Fechar,
            outro => outro,
        }
    }
}

fn mostrar_erro(ui: &mut egui::Ui, erro: &Option<String>) {
    if let Some(e) = erro {
        ui.add_space(10.0);
        ui.label(RichText::new(e).color(cores().erro).size(13.0));
    }
}

/// Linha de botões do rodapé: Cancelar à esquerda, a ação à direita.
fn rodape(ui: &mut egui::Ui, acao: &str, habilitado: bool) -> (bool, bool) {
    let (mut cancelar, mut confirmar) = (false, false);
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            confirmar = tema::botao_principal(ui, acao, habilitado).clicked();
        });
    });
    (cancelar, confirmar)
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
    tema::cabecalho(ui, "Nova tarefa", &format!("Em {}. Ela entra no Backlog.", d.projeto.nome));
    ui.add_space(16.0);
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
    let enter = pronto && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let (cancelar, criar) = rodape(ui, "Criar tarefa", pronto);
    if cancelar {
        return Resultado::Fechar;
    }
    if !(criar || enter) {
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
        Ok(_) => Resultado::Mudou,
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
        Ok(agente) => Resultado::AgenteCriado(agente),
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
