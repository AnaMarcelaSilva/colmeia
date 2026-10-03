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
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub anexos: Vec<i64>,
    /// Os anexos que são vídeos (não têm miniatura).
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub videos: Vec<i64>,
    /// O texto sem o nome da tarefa (para o cartão da tarefa).
    #[serde(default)]
    pub curto: String,
    /// Título e coluna da tarefa agora.
    #[serde(default)]
    pub titulo: String,
    #[serde(default)]
    pub coluna: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Dia {
    pub dia: String,
    pub titulo: String,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub itens: Vec<ItemLinha>,
    #[serde(default)]
    pub concluidas: usize,
    #[serde(default)]
    pub erros: usize,
    #[serde(default)]
    pub tempo_s: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaginaLinha {
    pub dias: Vec<Dia>,
    /// Próxima página (eventos mais antigos); 0 quando acabou.
    pub proximo: i64,
    pub perfil_criado_em: String,
}

/// Uma lista que pode vir como `null` (um núcleo antigo manda assim quando
/// está vazia): vira lista vazia em vez de quebrar a resposta inteira.
fn lista_ou_nulo<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

/// O texto da daily (os blocos que o núcleo também manda não são usados:
/// os cartões da página vêm do deck da apresentação).
#[derive(Clone, Debug, Deserialize)]
pub struct Daily {
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

/// O que é um anexo e onde ele está (para abrir um vídeo no reprodutor).
#[derive(Clone, Debug, Deserialize)]
pub struct InfoAnexo {
    pub caminho: String,
}

/// Uma mensagem do histórico de um agente.
#[derive(Clone, Debug, Deserialize)]
pub struct MensagemGuardada {
    pub texto: String,
}

// Apresentação da daily e da sprint (um slide por tarefa).

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TempoFerramenta {
    pub nome: String,
    pub segundos: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct NumerosCapa {
    #[serde(default)]
    pub concluidas: usize,
    #[serde(default)]
    pub revisao: usize,
    #[serde(default)]
    pub aguardando: usize,
    #[serde(default)]
    pub trabalhando: usize,
    /// Sessões que pararam com erro no período.
    #[serde(default)]
    pub erros: usize,
    /// Todas as tarefas criadas no período.
    #[serde(default)]
    pub novas: usize,
    #[serde(default)]
    pub tempo_s: i64,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub por_ferramenta: Vec<TempoFerramenta>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ParteCapa {
    pub titulo: String,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub tarefas: Vec<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Capa {
    #[serde(default)]
    pub numeros: NumerosCapa,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub destaques: Vec<String>,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub partes: Vec<ParteCapa>,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub novas: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Feito {
    #[serde(default)]
    pub parte: String,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub itens: Vec<String>,
    #[serde(default)]
    pub mais: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct NumerosSlide {
    #[serde(default)]
    pub tempo_s: i64,
    #[serde(default)]
    pub sessoes: usize,
    #[serde(default)]
    pub erros: usize,
    #[serde(default)]
    pub capturas: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct AnexoSlide {
    pub id: i64,
    /// "imagem" ou "video".
    pub tipo: String,
    #[serde(default)]
    pub nome: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub largura: u32,
    #[serde(default)]
    pub altura: u32,
}

impl AnexoSlide {
    pub fn video(&self) -> bool {
        self.tipo == "video"
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct NotaAnterior {
    pub texto: String,
    pub periodo: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Slide {
    pub tarefa_id: i64,
    pub titulo: String,
    #[serde(default)]
    pub projeto: String,
    #[serde(default)]
    pub coluna: String,
    /// concluidas, revisao, aguardando, trabalhando, erros ou outras.
    #[serde(default)]
    pub grupo: String,
    #[serde(default)]
    pub removida: bool,
    #[serde(default)]
    pub secao: String,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub partes: Vec<String>,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub feito: Vec<Feito>,
    #[serde(default)]
    pub numeros: NumerosSlide,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub anexos: Vec<AnexoSlide>,
    #[serde(default)]
    pub nota: String,
    #[serde(default)]
    pub nota_anterior: Option<NotaAnterior>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Deck {
    /// "daily" ou "sprint".
    pub tipo: String,
    pub titulo: String,
    #[serde(default)]
    pub periodo: String,
    /// Período das notas deste deck (o dia, ou "de..ate").
    #[serde(default)]
    pub chave_nota: String,
    #[serde(default)]
    pub capa: Capa,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub slides: Vec<Slide>,
    #[serde(default)]
    pub mais: usize,
    #[serde(default)]
    pub vazio: bool,
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
    chamar("GET", &format!("/v1/perfis/{perfil}/resumo?tipo=sprint&{}{projeto}", consulta_periodo(periodo)), None)
}

fn consulta_periodo(periodo: &PeriodoSprint) -> String {
    match periodo {
        PeriodoSprint::Ultimos(n) => format!("ultimos={n}"),
        PeriodoSprint::MesAtual => "mes=atual".into(),
        PeriodoSprint::Datas(de, ate) => format!("de={de}&ate={ate}"),
    }
}

/// Manda um PNG ao núcleo, anexado à tarefa. `origem`: captura ou mensagem;
/// `agente`: de qual terminal veio a captura.
pub fn anexar(tarefa: i64, origem: &str, agente: Option<i64>, png: &[u8]) -> Result<Anexo, String> {
    let agente = agente.map(|a| format!("&agente={a}")).unwrap_or_default();
    let (status, corpo) = canal::pedir_bytes("POST", &format!("/v1/tarefas/{tarefa}/anexos?origem={origem}{agente}"), "image/png", png)?;
    resposta_anexo(status, &corpo)
}

fn resposta_anexo(status: u16, corpo: &[u8]) -> Result<Anexo, String> {
    let corpo = String::from_utf8_lossy(corpo);
    if !(200..300).contains(&status) {
        return Err(serde_json::from_str::<Erro>(&corpo).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo")));
    }
    serde_json::from_str(&corpo).map_err(|e| inesperada("/v1/tarefas/anexos", e))
}

/// Formatos que dá para anexar na apresentação, pela extensão.
pub const FORMATOS_ANEXO: [&str; 7] = ["png", "jpg", "jpeg", "mp4", "webm", "mkv", "mov"];
/// Maior vídeo aceito pelo núcleo.
pub const MAIOR_VIDEO: u64 = 512 << 20;

/// O tipo (Content-Type) de um arquivo pela extensão, e se é vídeo.
pub fn tipo_do_arquivo(caminho: &std::path::Path) -> Option<(&'static str, bool)> {
    let extensao = caminho.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extensao.as_str() {
        "png" => ("image/png", false),
        "jpg" | "jpeg" => ("image/jpeg", false),
        "mp4" => ("video/mp4", true),
        "webm" => ("video/webm", true),
        "mkv" => ("video/x-matroska", true),
        "mov" => ("video/quicktime", true),
        _ => return None,
    })
}

/// Codifica um valor para ir na URL (o nome de um arquivo, com espaço ou acento).
pub fn codificar_url(valor: &str) -> String {
    let mut saida = String::with_capacity(valor.len());
    for b in valor.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => saida.push(b as char),
            _ => saida.push_str(&format!("%{b:02X}")),
        }
    }
    saida
}

/// Anexa uma foto ou um vídeo do disco à tarefa, enviado direto do arquivo.
/// `progresso` recebe os bytes enviados e o total.
pub fn anexar_arquivo(tarefa: i64, arquivo: &std::path::Path, progresso: impl FnMut(u64, u64)) -> Result<Anexo, String> {
    let (tipo, video) = tipo_do_arquivo(arquivo).ok_or("formato não aceito")?;
    let nome = arquivo.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let nome = codificar_url(&nome.chars().take(200).collect::<String>());
    let caminho = if video { format!("/v1/tarefas/{tarefa}/videos?nome={nome}") } else { format!("/v1/tarefas/{tarefa}/anexos?origem=arquivo&nome={nome}") };
    let (status, corpo) = canal::pedir_arquivo("POST", &caminho, tipo, arquivo, progresso)?;
    resposta_anexo(status, &corpo)
}

/// Anexa uma imagem já em PNG (colada na apresentação).
pub fn anexar_png(tarefa: i64, png: &[u8]) -> Result<Anexo, String> {
    let (status, corpo) = canal::pedir_bytes("POST", &format!("/v1/tarefas/{tarefa}/anexos?origem=colagem"), "image/png", png)?;
    resposta_anexo(status, &corpo)
}

pub fn info_anexo(id: i64) -> Result<InfoAnexo, String> {
    chamar("GET", &format!("/v1/anexos/{id}/info"), None)
}

/// O histórico de mensagens do agente, da mais nova para a mais antiga.
pub fn mensagens(agente: i64) -> Result<Vec<MensagemGuardada>, String> {
    chamar("GET", &format!("/v1/agentes/{agente}/mensagens"), None)
}

#[derive(Deserialize)]
struct Guardada {
    guardada: bool,
}

/// Guarda o que foi mandado ao agente; diz se ficou no histórico (um texto
/// que parece ter senha ou chave não fica).
pub fn guardar_mensagem(agente: i64, texto: &str) -> Result<bool, String> {
    chamar::<Guardada>("POST", &format!("/v1/agentes/{agente}/mensagens"), Some(json!({ "texto": texto }))).map(|g| g.guardada)
}

pub fn limpar_mensagens(agente: i64) -> Result<(), String> {
    chamar::<serde_json::Value>("DELETE", &format!("/v1/agentes/{agente}/mensagens"), None).map(|_| ())
}

/// O deck da daily ou da sprint (`periodo` só na sprint).
pub fn apresentacao(perfil: i64, projeto: Option<i64>, periodo: Option<&PeriodoSprint>) -> Result<Deck, String> {
    let projeto = projeto.map(|p| format!("&projeto={p}")).unwrap_or_default();
    let caminho = match periodo {
        None => format!("/v1/perfis/{perfil}/apresentacao?tipo=daily{projeto}"),
        Some(p) => format!("/v1/perfis/{perfil}/apresentacao?tipo=sprint&{}{projeto}", consulta_periodo(p)),
    };
    chamar("GET", &caminho, None)
}

/// Grava a nota da tarefa na daily ou na sprint (texto vazio apaga).
pub fn definir_nota(tarefa: i64, tipo: &str, periodo: &str, texto: &str) -> Result<(), String> {
    chamar::<serde_json::Value>("PUT", &format!("/v1/tarefas/{tarefa}/notas"), Some(json!({ "tipo": tipo, "periodo": periodo, "texto": texto }))).map(|_| ())
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
    fn daily_com_blocos_nulos_ainda_e_lida() {
        // Um núcleo da 0.2.0 mandava "blocos":null num projeto sem nada hoje.
        let d: Daily = serde_json::from_str(r#"{"periodo":"Hoje","hoje":{"titulo":"Hoje","blocos":null},"texto":"x","vazio":true}"#).unwrap();
        assert!(d.vazio && d.texto == "x");
    }

    #[test]
    fn deck_com_listas_nulas() {
        let d: Deck = serde_json::from_str(
            r#"{"tipo":"daily","titulo":"Daily","capa":{"numeros":{"por_ferramenta":null},"destaques":null,"partes":null,"novas":null},
                "slides":[{"tarefa_id":3,"titulo":"T","partes":null,"feito":[{"parte":"Hoje","itens":null}],"anexos":null}],"mais":0}"#,
        )
        .unwrap();
        assert!(d.capa.destaques.is_empty() && d.capa.partes.is_empty() && d.capa.numeros.por_ferramenta.is_empty());
        assert_eq!(d.slides[0].tarefa_id, 3);
        assert!(d.slides[0].anexos.is_empty() && d.slides[0].feito[0].itens.is_empty());
    }

    #[test]
    fn nome_de_arquivo_vai_codificado_na_url() {
        assert_eq!(codificar_url("demo da tela.mp4"), "demo%20da%20tela.mp4");
        assert_eq!(codificar_url("ação&x=1"), "a%C3%A7%C3%A3o%26x%3D1");
        assert_eq!(tipo_do_arquivo(std::path::Path::new("/x/Foto.JPG")), Some(("image/jpeg", false)));
        assert_eq!(tipo_do_arquivo(std::path::Path::new("/x/demo.mov")), Some(("video/quicktime", true)));
        assert_eq!(tipo_do_arquivo(std::path::Path::new("/x/foto.heic")), None);
    }

    #[test]
    fn erros_de_conexao_e_inesperados() {
        assert!(erro_de_conexao("núcleo indisponível: No such file or directory (os error 2)"));
        assert!(!erro_de_conexao("O período pode ter no máximo 92 dias."));
        assert!(erro_inesperado(&inesperada("/v1/x?y=1", "invalid type")));
    }
}
