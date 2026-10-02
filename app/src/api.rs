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
    /// Mostrar o aviso de segredos antes de capturar um terminal.
    #[serde(default = "verdadeiro")]
    pub aviso_captura: bool,
}

fn verdadeiro() -> bool {
    true
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
    /// A última mudança de coluna foi do núcleo (o agente parece esperar você).
    #[serde(default)]
    pub coluna_auto: bool,
}

/// Como um agente terminou.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Fim {
    pub codigo: i64,
    pub erro: bool,
    /// "terminou", "erro", "interrompido", "removido" ou "nucleo_encerrado".
    pub motivo: String,
    /// Hora local, "14:40".
    pub hora: String,
    pub texto: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Agente {
    pub id: i64,
    pub tarefa_id: i64,
    pub ferramenta: String,
    pub papel: String,
    #[serde(default)]
    pub ativo: bool,
    /// "trabalhando", "ocioso" ou "aguardando" (só com o agente rodando).
    #[serde(default)]
    pub estado: String,
    #[serde(default)]
    pub motivo: String,
    #[serde(default)]
    pub desde_hora: String,
    #[serde(default)]
    pub ultimo_fim: Option<Fim>,
}

/// Retrato do perfil numa resposta só, com o número da última mensagem de
/// eventos que ele já inclui.
#[derive(Clone, Debug, Deserialize)]
pub struct Quadro {
    pub seq: u64,
    pub projetos: Vec<Projeto>,
    pub tarefas: Vec<Tarefa>,
    pub agentes: Vec<Agente>,
}

// Linha do tempo, daily e sprint: os textos vêm prontos do núcleo.

#[derive(Clone, Debug, Deserialize)]
pub struct ItemLinha {
    pub evento: i64,
    pub hora: String,
    pub tipo: String,
    pub texto: String,
    #[serde(default)]
    pub projeto: String,
    #[serde(default)]
    pub tarefa_id: i64,
    #[serde(default)]
    pub agente_id: i64,
    #[serde(default)]
    pub removida: bool,
    #[serde(default)]
    pub anexos: Vec<i64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Dia {
    pub dia: String,
    pub titulo: String,
    #[serde(default)]
    pub resumo: String,
    pub itens: Vec<ItemLinha>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaginaLinha {
    pub dias: Vec<Dia>,
    /// Próxima página (eventos mais antigos); 0 quando acabou.
    pub proximo: i64,
    pub perfil_criado_em: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ItemResumo {
    pub texto: String,
    #[serde(default)]
    pub tipo: String,
    #[serde(default)]
    pub tarefa_id: i64,
    #[serde(default)]
    pub agente_id: i64,
    #[serde(default)]
    pub removida: bool,
}

/// Uma lista que pode vir como `null` (um núcleo antigo manda assim quando
/// está vazia): vira lista vazia em vez de quebrar a resposta inteira.
fn lista_ou_nulo<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

#[derive(Clone, Debug, Deserialize)]
pub struct Bloco {
    pub titulo: String,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub itens: Vec<ItemResumo>,
    #[serde(default)]
    pub mais: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ParteDaily {
    pub titulo: String,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub blocos: Vec<Bloco>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Daily {
    pub periodo: String,
    pub ontem: Option<ParteDaily>,
    pub hoje: ParteDaily,
    pub texto: String,
    pub vazio: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Captura {
    pub anexo: i64,
    #[serde(default)]
    pub tarefa_id: i64,
    pub texto: String,
    pub dia: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Sprint {
    /// DD/MM/AAAA.
    pub de: String,
    pub ate: String,
    pub periodo: String,
    pub capturas: Vec<Captura>,
    pub texto: String,
    pub markdown: String,
    pub vazio: bool,
}

/// O que o núcleo devolve ao receber uma imagem.
#[derive(Clone, Debug, Deserialize)]
pub struct Anexo {
    pub id: i64,
    /// Onde a imagem ficou, para mandar ao agente numa mensagem.
    pub caminho: String,
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

/// Começo da mensagem quando o núcleo responde algo que a tela não entende.
/// O detalhe (do serde, em inglês) vai só para o log, nunca para a tela.
const INESPERADA: &str = "resposta inesperada do núcleo";

/// O erro é de conexão (o núcleo não está lá): a faixa do topo já avisa.
pub fn erro_de_conexao(e: &str) -> bool {
    e.starts_with("núcleo indisponível") || e.starts_with("sem token do núcleo")
}

/// O núcleo respondeu algo que a tela não entende.
pub fn erro_inesperado(e: &str) -> bool {
    e.starts_with(INESPERADA)
}

fn inesperada(caminho: &str, e: impl std::fmt::Display) -> String {
    let caminho = caminho.split('?').next().unwrap_or_default();
    eprintln!("{INESPERADA} em {caminho}: {e}");
    format!("{INESPERADA}.")
}

fn chamar<T: DeserializeOwned>(metodo: &str, caminho: &str, corpo: Option<serde_json::Value>) -> Result<T, String> {
    let corpo = corpo.map(|c| c.to_string());
    let (status, resposta) = canal::pedir_com_corpo(metodo, caminho, corpo.as_deref())?;
    if !(200..300).contains(&status) {
        // O núcleo explica o problema em português; é essa mensagem que a tela mostra.
        return Err(serde_json::from_str::<Erro>(&resposta).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo")));
    }
    serde_json::from_str(&resposta).map_err(|e| inesperada(caminho, e))
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

pub fn definir_aviso_captura(perfil: i64, mostrar: bool) -> Result<(), String> {
    chamar::<Ok>("PATCH", &format!("/v1/perfis/{perfil}"), Some(json!({ "aviso_captura": mostrar }))).map(|_| ())
}

pub fn quadro(perfil: i64) -> Result<Quadro, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/quadro"), None)
}

/// Uma página da linha do tempo; `antes` é o `proximo` da anterior (0 = a primeira).
pub fn linha_do_tempo(perfil: i64, projeto: Option<i64>, antes: i64) -> Result<PaginaLinha, String> {
    let mut caminho = format!("/v1/perfis/{perfil}/linha-do-tempo?limite=200");
    if let Some(p) = projeto {
        caminho += &format!("&projeto={p}");
    }
    if antes > 0 {
        caminho += &format!("&antes={antes}");
    }
    chamar("GET", &caminho, None)
}

pub fn daily(perfil: i64, projeto: Option<i64>) -> Result<Daily, String> {
    let projeto = projeto.map(|p| format!("&projeto={p}")).unwrap_or_default();
    chamar("GET", &format!("/v1/perfis/{perfil}/resumo?tipo=daily{projeto}"), None)
}

/// Período de uma sprint. As datas relativas são calculadas pelo núcleo, que sabe o fuso.
#[derive(Clone, Debug, PartialEq)]
pub enum PeriodoSprint {
    Ultimos(u32),
    MesAtual,
    /// AAAA-MM-DD.
    Datas(String, String),
}

pub fn sprint(perfil: i64, projeto: Option<i64>, periodo: &PeriodoSprint) -> Result<Sprint, String> {
    let projeto = projeto.map(|p| format!("&projeto={p}")).unwrap_or_default();
    let periodo = match periodo {
        PeriodoSprint::Ultimos(n) => format!("ultimos={n}"),
        PeriodoSprint::MesAtual => "mes=atual".into(),
        PeriodoSprint::Datas(de, ate) => format!("de={de}&ate={ate}"),
    };
    chamar("GET", &format!("/v1/perfis/{perfil}/resumo?tipo=sprint&{periodo}{projeto}"), None)
}

/// Manda um PNG ao núcleo, anexado à tarefa. `origem`: captura ou mensagem;
/// `agente`: de qual terminal veio a captura.
pub fn anexar(tarefa: i64, origem: &str, agente: Option<i64>, png: &[u8]) -> Result<Anexo, String> {
    let agente = agente.map(|a| format!("&agente={a}")).unwrap_or_default();
    let (status, corpo) = canal::pedir_bytes("POST", &format!("/v1/tarefas/{tarefa}/anexos?origem={origem}{agente}"), "image/png", png)?;
    let corpo = String::from_utf8_lossy(&corpo);
    if !(200..300).contains(&status) {
        return Err(serde_json::from_str::<Erro>(&corpo).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo")));
    }
    serde_json::from_str(&corpo).map_err(|e| inesperada("/v1/tarefas/anexos", e))
}

/// O PNG de um anexo.
pub fn ler_anexo(id: i64) -> Result<Vec<u8>, String> {
    match canal::pedir_bytes("GET", &format!("/v1/anexos/{id}"), "application/json", &[])? {
        (200, corpo) => Ok(corpo),
        (status, _) => Err(format!("erro {status} do núcleo")),
    }
}

pub fn remover_anexo(id: i64) -> Result<(), String> {
    chamar::<Ok>("DELETE", &format!("/v1/anexos/{id}"), None).map(|_| ())
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

pub fn criar_projeto(workspace: i64, nome: &str, caminho: &str) -> Result<Projeto, String> {
    chamar("POST", &format!("/v1/workspaces/{workspace}/projetos"), Some(json!({ "nome": nome, "caminho": caminho })))
}

pub fn remover_projeto(projeto: i64) -> Result<(), String> {
    chamar::<Ok>("DELETE", &format!("/v1/projetos/{projeto}"), None).map(|_| ())
}

pub fn branches(projeto: i64) -> Result<Vec<String>, String> {
    chamar("GET", &format!("/v1/projetos/{projeto}/branches"), None)
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

/// Cria o agente e abre o terminal dele. `sessao` retoma uma conversa do Claude Code.
/// O terminal do agente nasce do tamanho do terminal em foco na tela, se ela já sabe.
fn tamanho() -> serde_json::Value {
    match crate::terminal::tamanho_em_foco() {
        Some((cols, rows)) => json!({ "cols": cols, "rows": rows }),
        None => json!({}),
    }
}

pub fn criar_agente(tarefa: i64, ferramenta: &str, papel: &str, sessao: &str) -> Result<Agente, String> {
    let mut corpo = tamanho();
    corpo["ferramenta"] = json!(ferramenta);
    corpo["papel"] = json!(papel);
    corpo["sessao"] = json!(sessao);
    chamar("POST", &format!("/v1/tarefas/{tarefa}/agentes"), Some(corpo))
}

pub fn iniciar_agente(agente: i64) -> Result<Agente, String> {
    chamar("POST", &format!("/v1/agentes/{agente}/iniciar"), Some(tamanho()))
}

pub fn remover_agente(agente: i64) -> Result<(), String> {
    chamar::<Ok>("DELETE", &format!("/v1/agentes/{agente}"), None).map(|_| ())
}

pub fn sessoes(tarefa: i64) -> Result<Sessoes, String> {
    chamar("GET", &format!("/v1/tarefas/{tarefa}/sessoes"), None)
}

/// Desliga o núcleo, que encerra os agentes antes (como `colmeia-nucleo --encerrar`).
pub fn encerrar_nucleo() -> Result<(), String> {
    match canal::pedir_com_corpo("POST", "/v1/encerrar", None)? {
        (202, _) => Ok(()),
        (status, corpo) => Err(serde_json::from_str::<Erro>(&corpo).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo"))),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn daily_com_blocos_nulos_vira_lista_vazia() {
        // Um núcleo da 0.2.0 mandava "blocos":null num projeto sem nada hoje.
        let d: Daily = serde_json::from_str(r#"{"periodo":"Hoje","hoje":{"titulo":"Hoje","blocos":null},"texto":"x","vazio":true}"#).unwrap();
        assert!(d.hoje.blocos.is_empty() && d.ontem.is_none());
    }

    #[test]
    fn erros_de_conexao_e_inesperados() {
        assert!(erro_de_conexao("núcleo indisponível: No such file or directory (os error 2)"));
        assert!(!erro_de_conexao("O período pode ter no máximo 92 dias."));
        assert!(erro_inesperado(&inesperada("/v1/x?y=1", "invalid type")));
    }
}
