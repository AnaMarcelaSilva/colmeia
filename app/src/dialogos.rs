//! Diálogos: novo projeto, nova tarefa e confirmações de remoção. Cada um
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
    RemoverTarefa { id: i64, titulo: String, erro: Option<String> },
    RemoverProjeto { id: i64, nome: String, erro: Option<String> },
}

pub enum Resultado {
    Continua,
    Fechar,
    /// Algo mudou no núcleo: a tela recarrega projetos e tarefas.
    Mudou,
    ProjetoCriado(i64),
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
    branch: String,
    titulo: String,
    erro: Option<String>,
    /// Foco no título uma vez só, ao abrir.
    focar: bool,
}

impl NovaTarefa {
    pub fn new(projeto: Projeto) -> Self {
        let branches = api::branches(projeto.id).unwrap_or_else(|_| vec![projeto.branch_padrao.clone()]);
        NovaTarefa { branch: projeto.branch_padrao.clone(), branches, projeto, titulo: String::new(), erro: None, focar: true }
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
                    Dialogo::RemoverTarefa { id, titulo, erro } => confirmar(
                        ui,
                        "Remover tarefa",
                        &format!("\"{titulo}\" sai do quadro. Isso não mexe em nenhuma branch nem arquivo."),
                        erro,
                        || api::remover_tarefa(*id),
                    ),
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
    tema::cabecalho(ui, "Novo projeto", "Aponte para a pasta de um repositório git. A Colmeia só lê o repositório; nada é alterado.");
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
    ui.add_space(12.0);
    ui.label(RichText::new("Branch").color(cores().suave).size(12.5));
    ui.add_space(2.0);
    let resposta = tema::chip(ui, "Branch", &d.branch, false);
    egui::Popup::menu(&resposta).show(|ui| {
        ui.set_min_width(240.0);
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for b in &d.branches {
                if tema::opcao_menu(ui, b, *b == d.branch) {
                    d.branch = b.clone();
                    ui.close();
                }
            }
        });
    });
    ui.add_space(4.0);
    ui.label(RichText::new("Na próxima versão, a tarefa ganha uma cópia isolada do repositório nessa branch.").color(cores().suave).size(12.0));
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
    match api::criar_tarefa(d.projeto.id, d.titulo.trim(), &d.branch) {
        Ok(_) => Resultado::Mudou,
        Err(e) => {
            d.erro = Some(e);
            Resultado::Continua
        }
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
