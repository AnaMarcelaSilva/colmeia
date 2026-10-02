//! Cliente da API /v1 do núcleo. A tela não guarda regra de negócio: pede ao
//! núcleo e mostra o que ele responde, inclusive a mensagem de erro.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canal;

#[derive(Clone, Debug, Deserialize)]
pub struct Perfil {
    pub id: i64,
    pub nome: String,
    pub tema: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conta {
    pub ferramenta: String,
    pub modo: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Ferramenta {
    pub id: String,
    pub nome: String,
    pub instalada: bool,
    #[serde(default)]
    pub versao: String,
    pub conta_separada: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Workspace {
    pub id: i64,
    pub nome: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Projeto {
    pub id: i64,
    pub workspace: String,
    pub nome: String,
    pub caminho: String,
    /// "git" ou "pasta" (pasta de trabalho sem git).
    #[serde(default)]
    pub tipo: String,
    pub branch_padrao: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Tarefa {
    pub id: i64,
    pub projeto_id: i64,
    pub titulo: String,
    pub coluna: String,
    pub branch: String,
    /// "pasta" (a do projeto) ou "copia" (cópia isolada em `copia`).
    #[serde(default)]
    pub local: String,
    #[serde(default)]
    pub copia: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Agente {
    pub id: i64,
    pub tarefa_id: i64,
    pub ferramenta: String,
    pub papel: String,
    pub ativo: bool,
}

/// Uma conversa do Claude Code guardada para a pasta da tarefa.
#[derive(Clone, Debug, Deserialize)]
pub struct Sessao {
    pub id: String,
    pub titulo: String,
    pub alterada: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Sessoes {
    pub sessoes: Vec<Sessao>,
    /// O Claude Code está aberto nesta pasta fora da Colmeia.
    pub aberto_fora: bool,
}

/// Como criar a tarefa: onde os agentes trabalham e, numa cópia isolada, de qual branch.
pub struct NovaTarefa<'a> {
    pub titulo: &'a str,
    pub branch: &'a str,
    pub copia: bool,
    /// Com cópia: criar a branch a partir de `base`, ou usar uma que já existe.
    pub nova: bool,
    pub base: &'a str,
}

#[derive(Deserialize)]
struct Erro {
    erro: String,
}

fn chamar<T: DeserializeOwned>(metodo: &str, caminho: &str, corpo: Option<serde_json::Value>) -> Result<T, String> {
    let corpo = corpo.map(|c| c.to_string());
    let (status, resposta) = canal::pedir_com_corpo(metodo, caminho, corpo.as_deref())?;
    if !(200..300).contains(&status) {
        // O núcleo explica o problema em português; é essa mensagem que a tela mostra.
        return Err(serde_json::from_str::<Erro>(&resposta).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo")));
    }
    serde_json::from_str(&resposta).map_err(|e| format!("resposta inesperada do núcleo: {e}"))
}

#[derive(Deserialize)]
struct Ok {}

pub fn ferramentas() -> Result<Vec<Ferramenta>, String> {
    chamar("GET", "/v1/ferramentas", None)
}

pub fn perfis() -> Result<Vec<Perfil>, String> {
    chamar("GET", "/v1/perfis", None)
}

pub fn criar_perfil(nome: &str, tema: &str) -> Result<Perfil, String> {
    chamar("POST", "/v1/perfis", Some(json!({ "nome": nome, "tema": tema })))
}

pub fn definir_tema(perfil: i64, tema: &str) -> Result<(), String> {
    chamar::<Ok>("PATCH", &format!("/v1/perfis/{perfil}"), Some(json!({ "tema": tema }))).map(|_| ())
}

pub fn definir_contas(perfil: i64, contas: &[Conta]) -> Result<(), String> {
    let corpo = serde_json::to_value(contas).map_err(|e| e.to_string())?;
    chamar::<Ok>("PUT", &format!("/v1/perfis/{perfil}/contas"), Some(corpo)).map(|_| ())
}

pub fn workspaces(perfil: i64) -> Result<Vec<Workspace>, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/workspaces"), None)
}

pub fn criar_workspace(perfil: i64, nome: &str) -> Result<Workspace, String> {
    chamar("POST", &format!("/v1/perfis/{perfil}/workspaces"), Some(json!({ "nome": nome })))
}

pub fn projetos(perfil: i64) -> Result<Vec<Projeto>, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/projetos"), None)
}

pub fn criar_projeto(workspace: i64, nome: &str, caminho: &str) -> Result<Projeto, String> {
    chamar("POST", &format!("/v1/workspaces/{workspace}/projetos"), Some(json!({ "nome": nome, "caminho": caminho })))
}

pub fn remover_projeto(projeto: i64) -> Result<(), String> {
    chamar::<Ok>("DELETE", &format!("/v1/projetos/{projeto}"), None).map(|_| ())
}

pub fn branches(projeto: i64) -> Result<Vec<String>, String> {
    chamar("GET", &format!("/v1/projetos/{projeto}/branches"), None)
}

pub fn tarefas(projeto: i64) -> Result<Vec<Tarefa>, String> {
    chamar("GET", &format!("/v1/projetos/{projeto}/tarefas"), None)
}

pub fn criar_tarefa(projeto: i64, t: &NovaTarefa) -> Result<Tarefa, String> {
    let local = if t.copia { "copia" } else { "pasta" };
    chamar(
        "POST",
        &format!("/v1/projetos/{projeto}/tarefas"),
        Some(json!({ "titulo": t.titulo, "branch": t.branch, "local": local, "nova": t.nova, "base": t.base })),
    )
}

pub fn mover_tarefa(tarefa: i64, coluna: &str) -> Result<Tarefa, String> {
    chamar("PATCH", &format!("/v1/tarefas/{tarefa}"), Some(json!({ "coluna": coluna })))
}

pub fn remover_tarefa(tarefa: i64) -> Result<(), String> {
    chamar::<Ok>("DELETE", &format!("/v1/tarefas/{tarefa}"), None).map(|_| ())
}

pub fn contas(perfil: i64) -> Result<Vec<Conta>, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/contas"), None)
}

pub fn agentes_do_projeto(projeto: i64) -> Result<Vec<Agente>, String> {
    chamar("GET", &format!("/v1/projetos/{projeto}/agentes"), None)
}

/// Cria o agente e abre o terminal dele. `sessao` retoma uma conversa do Claude Code.
pub fn criar_agente(tarefa: i64, ferramenta: &str, papel: &str, sessao: &str) -> Result<Agente, String> {
    chamar("POST", &format!("/v1/tarefas/{tarefa}/agentes"), Some(json!({ "ferramenta": ferramenta, "papel": papel, "sessao": sessao })))
}

pub fn iniciar_agente(agente: i64) -> Result<Agente, String> {
    chamar("POST", &format!("/v1/agentes/{agente}/iniciar"), None)
}

pub fn remover_agente(agente: i64) -> Result<(), String> {
    chamar::<Ok>("DELETE", &format!("/v1/agentes/{agente}"), None).map(|_| ())
}

pub fn sessoes(tarefa: i64) -> Result<Sessoes, String> {
    chamar("GET", &format!("/v1/tarefas/{tarefa}/sessoes"), None)
}
