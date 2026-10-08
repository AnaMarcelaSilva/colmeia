//! Cliente da API /v1 do núcleo. A tela não guarda regra de negócio: pede ao
//! núcleo e mostra o que ele responde, inclusive a mensagem de erro.

use std::collections::HashMap;

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
    /// Mostrar o tempo dos agentes na daily, na sprint, na linha do tempo e
    /// na apresentação (desligado por padrão; um núcleo antigo não manda).
    #[serde(default)]
    pub tempo_agentes: bool,
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
    /// O workspace (para a lousa dele na barra lateral).
    #[serde(default)]
    pub workspace_id: i64,
    pub workspace: String,
    /// O workspace está recolhido na barra lateral.
    #[serde(default)]
    pub workspace_recolhido: bool,
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
    /// O mesmo momento, em RFC 3339 (UTC).
    #[serde(default)]
    pub desde: String,
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
    /// Pedidos ao agente abertos e os fechados nas últimas horas.
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub pedidos: Vec<Pedido>,
    /// Tarefas com o navegador da Colmeia aberto.
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub navegadores: Vec<i64>,
    /// Pedidos de consulta dos agentes esperando você.
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub aprovacoes: Vec<Aprovacao>,
}

/// Algo a mais pedido ao agente da tarefa pela daily, pela sprint ou pela
/// apresentação; a resposta volta para a nota (tipo e período).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Pedido {
    pub id: i64,
    pub tarefa_id: i64,
    #[serde(default)]
    pub agente_id: i64,
    pub tipo: String,
    pub periodo: String,
    pub texto: String,
    /// "fila", "entregue", "respondido", "cancelado" ou "falhou".
    pub estado: String,
    #[serde(default)]
    pub motivo: String,
    #[serde(default)]
    pub criado_em: String,
    #[serde(default)]
    pub entregue_em: String,
    /// Quando fechou (respondido, cancelado ou falhou).
    #[serde(default)]
    pub respondido_em: String,
    /// As mesmas horas, locais ("21:40").
    #[serde(default)]
    pub entregue_hora: String,
    #[serde(default)]
    pub respondido_hora: String,
}

impl Pedido {
    pub fn aberto(&self) -> bool {
        self.estado == "fila" || self.estado == "entregue"
    }
}

/// Para quem o pedido vai, dito pelo núcleo antes do Enviar.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Destino {
    /// "ativo", "reiniciar", "novo" ou "bloqueado".
    pub acao: String,
    #[serde(default)]
    pub agente: Option<Agente>,
    /// Título da conversa que um agente novo retoma ("" é uma conversa nova).
    #[serde(default)]
    pub conversa: String,
    /// A tarefa tem agentes, mas nenhum é Claude Code.
    #[serde(default)]
    pub outros: bool,
    #[serde(default)]
    pub motivo: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PedidoCriado {
    pub pedido: Pedido,
}

/// Se há um Chrome ou Chromium para a Colmeia controlar.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct InfoNavegador {
    pub instalado: bool,
}

/// Um item da pasta da tarefa.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Entrada {
    pub nome: String,
    #[serde(default)]
    pub pasta: bool,
    #[serde(default)]
    pub link: bool,
    #[serde(default)]
    pub ignorada: bool,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub sensivel: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ListaArquivos {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub entradas: Vec<Entrada>,
    #[serde(default)]
    pub mais: bool,
}

/// A pré-visualização de um arquivo da pasta da tarefa.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Previa {
    /// "texto", "imagem", "binario" ou "sensivel".
    pub tipo: String,
    pub caminho_absoluto: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub texto: String,
    #[serde(default)]
    pub cortado: bool,
    #[serde(default)]
    pub linhas: usize,
    #[serde(default)]
    pub largura: u32,
    #[serde(default)]
    pub altura: u32,
    #[serde(default)]
    pub formato: String,
}

// Linha do tempo, daily e sprint: os textos vêm prontos do núcleo.

#[derive(Clone, Debug, Deserialize)]
pub struct ItemLinha {
    #[serde(default)]
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
    /// O item é de uma conexão de banco (o clique abre ela).
    #[serde(default)]
    pub conexao_id: i64,
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
    /// A página pode ter o tempo dos agentes (a opção do perfil, quando foi montada).
    #[serde(default)]
    pub tempo_agentes: bool,
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
    #[serde(default)]
    pub tempo_agentes: bool,
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
    #[serde(default)]
    pub tempo_agentes: bool,
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
    /// O título da tarefa, quando ela ganhou outro na sprint.
    #[serde(default)]
    pub titulo_original: String,
    #[serde(default)]
    pub projeto: String,
    #[serde(default)]
    pub coluna: String,
    /// concluidas, revisao, aguardando, trabalhando, erros ou outras.
    #[serde(default)]
    pub grupo: String,
    #[serde(default)]
    pub removida: bool,
    /// O projeto da tarefa ("estudos · loja-web" com mais de um workspace):
    /// com mais de uma seção, a página e a apresentação separam por ela.
    #[serde(default)]
    pub secao: String,
    /// O id do projeto da seção (dois workspaces podem ter o mesmo nome).
    #[serde(default)]
    pub secao_id: i64,
    /// O workspace, quando o recorte tem mais de um.
    #[serde(default)]
    pub workspace: String,
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
    /// Versão da nota (quando foi gravada): vai junto ao salvar.
    #[serde(default)]
    pub nota_versao: String,
    #[serde(default)]
    pub nota_anterior: Option<NotaAnterior>,
    /// A lousa da tarefa, quando tem itens (os itens vêm sob demanda).
    #[serde(default)]
    pub lousa: Option<ResumoLousa>,
}

/// A lousa de uma tarefa no slide: o id e quantos itens ela tem.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct ResumoLousa {
    pub id: i64,
    #[serde(default)]
    pub elementos: usize,
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
    /// O deck é de uma sprint fixa: os títulos dela valem e podem mudar.
    #[serde(default)]
    pub sprint_id: i64,
    #[serde(default)]
    pub capa: Capa,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub slides: Vec<Slide>,
    #[serde(default)]
    pub mais: usize,
    #[serde(default)]
    pub vazio: bool,
    /// Os números e os textos podem ter o tempo dos agentes.
    #[serde(default)]
    pub tempo_agentes: bool,
    /// As tarefas tiradas da daily de hoje (só na daily), para trazer de volta.
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub fora: Vec<TarefaFora>,
}

/// Uma tarefa tirada da daily de hoje.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TarefaFora {
    pub tarefa_id: i64,
    pub titulo: String,
    #[serde(default)]
    pub projeto: String,
}

// Lousa (quadro livre) do workspace e da tarefa.

/// De quem é a lousa: um workspace ou uma tarefa.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct DonoLousa {
    #[serde(default, skip_serializing_if = "zero")]
    pub workspace_id: i64,
    #[serde(default, skip_serializing_if = "zero")]
    pub tarefa_id: i64,
}

fn zero(v: &i64) -> bool {
    *v == 0
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct InfoLousa {
    pub id: i64,
    #[serde(default)]
    pub dono: DonoLousa,
}

/// O tipo de um item da lousa.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TipoElemento {
    #[default]
    Nota,
    Texto,
    Codigo,
    Imagem,
    Video,
    Tarefa,
    Ligacao,
}

impl TipoElemento {
    /// Nota, texto e código: os que têm texto editável e trocam de tipo entre si.
    pub fn de_texto(self) -> bool {
        matches!(self, TipoElemento::Nota | TipoElemento::Texto | TipoElemento::Codigo)
    }
}

/// O que a tela precisa do anexo de uma imagem ou de um vídeo.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct AnexoDoElemento {
    #[serde(default)]
    pub nome: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub largura: u32,
    #[serde(default)]
    pub altura: u32,
}

/// Um item da lousa, em unidades do quadro (1 = 1 px a 100%).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ElementoLousa {
    pub id: i64,
    #[serde(default)]
    pub lousa_id: i64,
    pub tipo: TipoElemento,
    pub x: f32,
    pub y: f32,
    pub largura: f32,
    pub altura: f32,
    #[serde(default)]
    pub z: i64,
    #[serde(default)]
    pub cor: String,
    #[serde(default)]
    pub titulo: String,
    #[serde(default)]
    pub texto: String,
    #[serde(default)]
    pub anexo_id: i64,
    #[serde(default)]
    pub anexo: Option<AnexoDoElemento>,
    #[serde(default)]
    pub tarefa_ref: i64,
    #[serde(default)]
    pub de: i64,
    #[serde(default)]
    pub para: i64,
    /// "voce" ou "agente".
    #[serde(default)]
    pub autor: String,
    #[serde(default)]
    pub agente_id: i64,
    #[serde(default)]
    pub versao: i64,
    #[serde(default)]
    pub atualizado_em: String,
    /// Revisão local: muda a cada mudança (sua ou do núcleo) e invalida o
    /// cache de desenho do item. Não vai para o núcleo.
    #[serde(skip)]
    pub rev: u64,
}

impl ElementoLousa {
    pub fn do_agente(&self) -> bool {
        self.autor == "agente"
    }
}

/// A lousa aberta e os itens dela.
#[derive(Clone, Debug, Deserialize)]
pub struct LousaAberta {
    pub lousa: InfoLousa,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub elementos: Vec<ElementoLousa>,
}

/// Uma ponta de ligação num lote: o id de um item ou o ref de um criado no mesmo lote.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Ponta {
    Id(i64),
    Ref(String),
}

/// Um item a criar.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct NovoElemento {
    pub tipo: TipoElemento,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largura: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altura: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cor: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub titulo: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub texto: String,
    #[serde(skip_serializing_if = "zero")]
    pub anexo_id: i64,
    #[serde(skip_serializing_if = "zero")]
    pub tarefa_ref: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub de: Option<Ponta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub para: Option<Ponta>,
}

/// Só os campos que mudaram.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CamposElemento {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largura: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altura: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub titulo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texto: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tipo: Option<TipoElemento>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub de: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub para: Option<i64>,
}

/// Uma operação de um lote da lousa.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Operacao {
    Criar { r#ref: String, elemento: NovoElemento },
    Alterar { id: i64, versao: i64, campos: CamposElemento },
    Remover { id: i64, versao: i64 },
}

/// O que o núcleo respondeu a um lote.
#[derive(Clone, Debug, PartialEq)]
pub enum LoteGravado {
    /// Gravou: os itens como ficaram, os removidos (inclusive as ligações
    /// levadas junto) e o id de cada ref criado.
    Ok { elementos: Vec<ElementoLousa>, removidos: Vec<i64>, refs: HashMap<String, i64> },
    /// Alguma versão não bateu: nada foi gravado; o estado atual do que mudou.
    Mudou { elementos: Vec<ElementoLousa>, removidos: Vec<i64> },
}

#[derive(Deserialize)]
struct RespostaLote {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    elementos: Vec<ElementoLousa>,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    removidos: Vec<i64>,
    #[serde(default)]
    refs: HashMap<String, i64>,
}

pub fn abrir_lousa(dono: DonoLousa) -> Result<LousaAberta, String> {
    let caminho = if dono.workspace_id != 0 { format!("/v1/workspaces/{}/lousa", dono.workspace_id) } else { format!("/v1/tarefas/{}/lousa", dono.tarefa_id) };
    chamar("POST", &caminho, None)
}

pub fn ler_lousa(id: i64) -> Result<LousaAberta, String> {
    chamar("GET", &format!("/v1/lousas/{id}"), None)
}

/// Grava um lote (tudo ou nada).
pub fn gravar_lousa(id: i64, operacoes: &[Operacao]) -> Result<LoteGravado, String> {
    let corpo = json!({ "operacoes": operacoes }).to_string();
    let (status, resposta) = canal::pedir_com_corpo("POST", &format!("/v1/lousas/{id}/operacoes"), Some(&corpo))?;
    let ler = |r: &str| serde_json::from_str::<RespostaLote>(r).map_err(|e| inesperada("/v1/lousas/operacoes", e));
    match status {
        200 => ler(&resposta).map(|r| LoteGravado::Ok { elementos: r.elementos, removidos: r.removidos, refs: r.refs }),
        409 => ler(&resposta).map(|r| LoteGravado::Mudou { elementos: r.elementos, removidos: r.removidos }),
        _ => Err(serde_json::from_str::<Erro>(&resposta).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo"))),
    }
}

/// O anexo de uma lousa que acabou de chegar ao núcleo.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AnexoLousa {
    pub id: i64,
    #[serde(default)]
    pub largura: u32,
    #[serde(default)]
    pub altura: u32,
    #[serde(default)]
    pub bytes: u64,
}

/// Uma imagem já em PNG (colada) para a lousa: anexo do perfil, fora da linha do tempo.
pub fn anexar_png_na_lousa(perfil: i64, png: &[u8]) -> Result<AnexoLousa, String> {
    let (status, corpo) = canal::pedir_bytes("POST", &format!("/v1/perfis/{perfil}/anexos?origem=colagem&lousa=1"), "image/png", png)?;
    resposta_anexo_lousa(status, &corpo)
}

/// Uma foto ou um vídeo do disco para a lousa.
pub fn anexar_arquivo_na_lousa(perfil: i64, arquivo: &std::path::Path) -> Result<AnexoLousa, String> {
    let (tipo, video) = tipo_do_arquivo(arquivo).ok_or("formato não aceito")?;
    let nome = arquivo.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let nome = codificar_url(&nome.chars().take(200).collect::<String>());
    let caminho = if video {
        format!("/v1/perfis/{perfil}/videos?lousa=1&nome={nome}")
    } else {
        format!("/v1/perfis/{perfil}/anexos?origem=arquivo&lousa=1&nome={nome}")
    };
    let (status, corpo) = canal::pedir_arquivo("POST", &caminho, tipo, arquivo, |_, _| {})?;
    resposta_anexo_lousa(status, &corpo)
}

fn resposta_anexo_lousa(status: u16, corpo: &[u8]) -> Result<AnexoLousa, String> {
    let corpo = String::from_utf8_lossy(corpo);
    if !(200..300).contains(&status) {
        return Err(serde_json::from_str::<Erro>(&corpo).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo")));
    }
    serde_json::from_str(&corpo).map_err(|e| inesperada("/v1/perfis/anexos", e))
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

/// Recolhe ou abre o workspace na barra lateral (guardado no núcleo).
pub fn recolher_workspace(workspace: i64, recolhido: bool) -> Result<(), String> {
    chamar::<Ok>("PATCH", &format!("/v1/workspaces/{workspace}"), Some(json!({ "recolhido": recolhido }))).map(|_| ())
}

pub fn definir_tempo_agentes(perfil: i64, mostrar: bool) -> Result<(), String> {
    chamar::<Ok>("PATCH", &format!("/v1/perfis/{perfil}"), Some(json!({ "tempo_agentes": mostrar }))).map(|_| ())
}

/// De onde a linha do tempo, a daily, a sprint e a apresentação olham: o
/// perfil inteiro, um workspace (todos os projetos dele) ou um projeto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Recorte {
    Perfil,
    Workspace(i64),
    Projeto(i64),
}

impl Recorte {
    /// O pedaço da consulta ("&workspace=2"), vazio no perfil.
    pub fn consulta(&self) -> String {
        match self {
            Recorte::Perfil => String::new(),
            Recorte::Workspace(id) => format!("&workspace={id}"),
            Recorte::Projeto(id) => format!("&projeto={id}"),
        }
    }
}

pub fn quadro(perfil: i64) -> Result<Quadro, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/quadro"), None)
}

/// Uma página da linha do tempo; `antes` é o `proximo` da anterior (0 = a primeira).
pub fn linha_do_tempo(perfil: i64, recorte: Recorte, antes: i64) -> Result<PaginaLinha, String> {
    let mut caminho = format!("/v1/perfis/{perfil}/linha-do-tempo?limite=200{}", recorte.consulta());
    if antes > 0 {
        caminho += &format!("&antes={antes}");
    }
    chamar("GET", &caminho, None)
}

pub fn daily(perfil: i64, recorte: Recorte) -> Result<Daily, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/resumo?tipo=daily{}", recorte.consulta()), None)
}

/// Período de uma sprint. As datas relativas são calculadas pelo núcleo, que sabe o fuso.
#[derive(Clone, Debug, PartialEq)]
pub enum PeriodoSprint {
    Ultimos(u32),
    MesAtual,
    /// AAAA-MM-DD.
    Datas(String, String),
    /// Uma sprint fixa do perfil (com os títulos dela).
    Fixa(i64),
}

/// Uma sprint: um período fixo do perfil (AAAA-MM-DD, inclusive).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct SprintFixa {
    pub id: i64,
    pub inicio: String,
    pub fim: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ListaSprints {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub sprints: Vec<SprintFixa>,
    /// A que tem hoje (o núcleo cria se ainda não existir).
    #[serde(default)]
    pub atual: i64,
}

pub fn listar_sprints(perfil: i64) -> Result<ListaSprints, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/sprints"), None)
}

pub fn editar_sprint(id: i64, inicio: &str, fim: &str) -> Result<SprintFixa, String> {
    chamar("PATCH", &format!("/v1/sprints/{id}"), Some(json!({ "inicio": inicio, "fim": fim })))
}

/// O título da tarefa só nesta sprint; vazio volta ao da tarefa.
pub fn titulo_sprint(sprint: i64, tarefa: i64, titulo: &str) -> Result<(), String> {
    sem_conteudo("PUT", &format!("/v1/sprints/{sprint}/titulos/{tarefa}"), Some(json!({ "titulo": titulo })))
}

pub fn sprint(perfil: i64, recorte: Recorte, periodo: &PeriodoSprint) -> Result<Sprint, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/resumo?tipo=sprint&{}{}", consulta_periodo(periodo), recorte.consulta()), None)
}

fn consulta_periodo(periodo: &PeriodoSprint) -> String {
    match periodo {
        PeriodoSprint::Ultimos(n) => format!("ultimos={n}"),
        PeriodoSprint::MesAtual => "mes=atual".into(),
        PeriodoSprint::Datas(de, ate) => format!("de={de}&ate={ate}"),
        PeriodoSprint::Fixa(id) => format!("sprint={id}"),
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
pub fn apresentacao(perfil: i64, recorte: Recorte, periodo: Option<&PeriodoSprint>) -> Result<Deck, String> {
    let projeto = recorte.consulta();
    let caminho = match periodo {
        None => format!("/v1/perfis/{perfil}/apresentacao?tipo=daily{projeto}"),
        Some(p) => format!("/v1/perfis/{perfil}/apresentacao?tipo=sprint&{}{projeto}", consulta_periodo(p)),
    };
    chamar("GET", &caminho, None)
}

/// Resultado de gravar a nota com a versão lida.
#[derive(Clone, Debug, PartialEq)]
pub enum NotaGravada {
    /// Gravou; a versão nova.
    Ok(String),
    /// A nota mudou desde a versão lida (o agente complementou): o texto e a versão de agora.
    Mudou { texto: String, versao: String },
}

#[derive(Deserialize)]
struct NotaLida {
    #[serde(default)]
    atualizada_em: String,
}

#[derive(Deserialize)]
struct NotaMudou {
    #[serde(default)]
    texto: String,
    #[serde(default)]
    versao: String,
}

/// Grava a nota só se ela ainda está na `versao` lida ("" = não havia nota).
pub fn gravar_nota(tarefa: i64, tipo: &str, periodo: &str, texto: &str, versao: &str) -> Result<NotaGravada, String> {
    let corpo = json!({ "tipo": tipo, "periodo": periodo, "texto": texto, "versao": versao }).to_string();
    let (status, resposta) = canal::pedir_com_corpo("PUT", &format!("/v1/tarefas/{tarefa}/notas"), Some(&corpo))?;
    match status {
        200 => Ok(NotaGravada::Ok(serde_json::from_str::<NotaLida>(&resposta).map(|n| n.atualizada_em).unwrap_or_default())),
        409 => serde_json::from_str::<NotaMudou>(&resposta)
            .map(|m| NotaGravada::Mudou { texto: m.texto, versao: m.versao })
            .map_err(|e| inesperada("/v1/tarefas/notas", e)),
        _ => Err(serde_json::from_str::<Erro>(&resposta).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo"))),
    }
}

/// Tira a tarefa da daily do `dia` (AAAA-MM-DD) ou, com `fora` falso, traz
/// de volta. A sprint e a linha do tempo não mudam.
pub fn tirar_da_daily(tarefa: i64, dia: &str, fora: bool) -> Result<(), String> {
    let corpo = json!({ "dia": dia, "fora": fora }).to_string();
    let (status, resposta) = canal::pedir_com_corpo("PUT", &format!("/v1/tarefas/{tarefa}/daily"), Some(&corpo))?;
    match status {
        204 => Ok(()),
        _ => Err(serde_json::from_str::<Erro>(&resposta).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo"))),
    }
}

// Play: configurações de execução dos projetos

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Variavel {
    pub nome: String,
    pub valor: String,
}

/// O que se escreve numa configuração de execução.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct CamposComando {
    pub nome: String,
    pub comando: String,
    #[serde(default)]
    pub pasta: String,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub ambiente: Vec<Variavel>,
}

/// Como terminou a última execução.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct FimExecucao {
    #[serde(default)]
    pub codigo: i32,
    #[serde(default)]
    pub parada: bool,
    #[serde(default)]
    pub hora: String,
}

/// Uma configuração de execução e o estado dela agora.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Comando {
    pub id: i64,
    pub projeto_id: i64,
    #[serde(flatten)]
    pub campos: CamposComando,
    #[serde(default)]
    pub origem: String,
    #[serde(default)]
    pub rodando: bool,
    #[serde(default)]
    pub desde: String,
    #[serde(default)]
    pub onde: String,
    /// A tarefa em cuja pasta rodou pela última vez (0: a do projeto).
    #[serde(default)]
    pub tarefa_id: i64,
    #[serde(default)]
    pub fim: Option<FimExecucao>,
}

/// Uma configuração achada no IntelliJ ou nos arquivos do projeto.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Sugestao {
    #[serde(flatten)]
    pub campos: CamposComando,
    #[serde(default)]
    pub origem: String,
    #[serde(default)]
    pub fonte: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Sugestoes {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub sugestoes: Vec<Sugestao>,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub nao_suportadas: Vec<String>,
}

pub fn comandos(projeto: i64) -> Result<Vec<Comando>, String> {
    chamar::<Option<Vec<Comando>>>("GET", &format!("/v1/projetos/{projeto}/comandos"), None).map(Option::unwrap_or_default)
}

pub fn sugestoes_de_comandos(projeto: i64) -> Result<Sugestoes, String> {
    chamar("GET", &format!("/v1/projetos/{projeto}/comandos/sugestoes"), None)
}

pub fn criar_comando(projeto: i64, campos: &CamposComando, origem: &str) -> Result<Comando, String> {
    let mut corpo = serde_json::to_value(campos).map_err(|e| e.to_string())?;
    corpo["origem"] = json!(origem);
    chamar("POST", &format!("/v1/projetos/{projeto}/comandos"), Some(corpo))
}

pub fn editar_comando(id: i64, campos: &CamposComando) -> Result<Comando, String> {
    chamar("PATCH", &format!("/v1/comandos/{id}"), Some(serde_json::to_value(campos).map_err(|e| e.to_string())?))
}

fn sem_conteudo(metodo: &str, caminho: &str, corpo: Option<serde_json::Value>) -> Result<(), String> {
    let corpo = corpo.map(|c| c.to_string());
    let (status, resposta) = canal::pedir_com_corpo(metodo, caminho, corpo.as_deref())?;
    match status {
        200..300 => Ok(()),
        _ => Err(serde_json::from_str::<Erro>(&resposta).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo"))),
    }
}

pub fn remover_comando(id: i64) -> Result<(), String> {
    sem_conteudo("DELETE", &format!("/v1/comandos/{id}"), None)
}

/// Roda a configuração (na pasta da `tarefa`, ou na do projeto com 0), no
/// tamanho do terminal; rodar de novo para a execução anterior.
pub fn rodar_comando(id: i64, tarefa: i64, tamanho: Option<(u16, u16)>) -> Result<Comando, String> {
    let (cols, rows) = tamanho.unwrap_or((0, 0));
    chamar("POST", &format!("/v1/comandos/{id}/rodar"), Some(json!({ "tarefa_id": tarefa, "cols": cols, "rows": rows })))
}

pub fn parar_comando(id: i64) -> Result<(), String> {
    sem_conteudo("POST", &format!("/v1/comandos/{id}/parar"), None)
}

// Pedidos ao agente

pub fn destino_do_pedido(tarefa: i64) -> Result<Destino, String> {
    chamar("GET", &format!("/v1/tarefas/{tarefa}/pedidos/destino"), None)
}

/// Põe o pedido na fila do agente da tarefa (o núcleo escolhe, inicia ou cria o agente).
pub fn criar_pedido(tarefa: i64, texto: &str, tipo: &str, periodo: &str) -> Result<Pedido, String> {
    let mut corpo = tamanho();
    corpo["texto"] = json!(texto);
    corpo["tipo"] = json!(tipo);
    corpo["periodo"] = json!(periodo);
    chamar::<PedidoCriado>("POST", &format!("/v1/tarefas/{tarefa}/pedidos"), Some(corpo)).map(|c| c.pedido)
}

pub fn cancelar_pedido(pedido: i64) -> Result<(), String> {
    chamar::<serde_json::Value>("DELETE", &format!("/v1/pedidos/{pedido}"), None).map(|_| ())
}

// Navegador da tarefa

pub fn info_navegador() -> Result<InfoNavegador, String> {
    chamar("GET", "/v1/navegador", None)
}

/// Abre (ou traz para frente) o navegador da tarefa na geometria dada; com
/// `url`, vai para o endereço.
pub fn abrir_navegador(tarefa: i64, url: &str, geometria: Option<[i32; 4]>) -> Result<(), String> {
    let mut corpo = json!({ "url": url });
    if let Some([x, y, largura, altura]) = geometria {
        corpo["x"] = json!(x);
        corpo["y"] = json!(y);
        corpo["largura"] = json!(largura);
        corpo["altura"] = json!(altura);
    }
    chamar::<serde_json::Value>("POST", &format!("/v1/tarefas/{tarefa}/navegador"), Some(corpo)).map(|_| ())
}

/// Avisa o núcleo que a tela passou (ou deixou de) mostrar algo que pode
/// estar compartilhado (apresentação, Daily, Sprint): o navegador que o
/// agente abrir não cobre a tela. A geometria é onde ele abre quando o usuário pede.
pub fn apresentando(perfil: i64, ativo: bool, geometria: Option<[i32; 4]>) -> Result<(), String> {
    let mut corpo = json!({ "ativo": ativo });
    if let Some([x, y, largura, altura]) = geometria {
        corpo["geometria"] = json!({ "x": x, "y": y, "largura": largura, "altura": altura });
    }
    chamar::<serde_json::Value>("PUT", &format!("/v1/perfis/{perfil}/apresentando"), Some(corpo)).map(|_| ())
}

pub fn fechar_navegador(tarefa: i64) -> Result<(), String> {
    chamar::<serde_json::Value>("DELETE", &format!("/v1/tarefas/{tarefa}/navegador"), None).map(|_| ())
}

#[derive(Deserialize)]
struct Id {
    id: i64,
}

/// Captura o navegador da tarefa e anexa; devolve o id do anexo.
pub fn capturar_navegador(tarefa: i64) -> Result<i64, String> {
    chamar::<Id>("POST", &format!("/v1/tarefas/{tarefa}/navegador/captura"), None).map(|i| i.id)
}

// Arquivos da tarefa (só leitura)

pub fn arquivos(tarefa: i64, caminho: &str) -> Result<ListaArquivos, String> {
    chamar("GET", &format!("/v1/tarefas/{tarefa}/arquivos?caminho={}", codificar_url(caminho)), None)
}

pub fn ver_arquivo(tarefa: i64, caminho: &str, mostrar: bool) -> Result<Previa, String> {
    let mostrar = if mostrar { "&mostrar=1" } else { "" };
    chamar("GET", &format!("/v1/tarefas/{tarefa}/arquivo?caminho={}{mostrar}", codificar_url(caminho)), None)
}

/// A imagem do arquivo, como PNG reduzido pelo núcleo.
pub fn imagem_do_arquivo(tarefa: i64, caminho: &str) -> Result<Vec<u8>, String> {
    match canal::pedir_bytes("GET", &format!("/v1/tarefas/{tarefa}/arquivo/imagem?caminho={}", codificar_url(caminho)), "application/json", &[])? {
        (200, corpo) => Ok(corpo),
        (status, corpo) => Err(serde_json::from_slice::<Erro>(&corpo).map(|e| e.erro).unwrap_or_else(|_| format!("erro {status} do núcleo"))),
    }
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

// Bancos de dados

/// Uma conexão de banco do perfil (a senha nunca vem: só onde ela está).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ConexaoBanco {
    pub id: i64,
    #[serde(default)]
    pub pasta: String,
    pub nome: String,
    /// "postgres", "mysql", "sqlserver" ou "sqlite".
    pub tipo: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub porta: u16,
    #[serde(default)]
    pub usuario: String,
    #[serde(default)]
    pub banco: String,
    #[serde(default)]
    pub arquivo: String,
    /// "desligado", "preferir", "exigir" ou "verificar".
    #[serde(default)]
    pub ssl: String,
    #[serde(default)]
    pub ssl_ca: String,
    #[serde(default)]
    pub escrita: bool,
    #[serde(default)]
    pub agentes: bool,
    /// Onde a senha está: "chaveiro", "memoria" ou "nenhuma".
    #[serde(default)]
    pub senha: String,
    /// A senha está à mão agora (sem ela, a tela pergunta ao conectar).
    #[serde(default)]
    pub senha_disponivel: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ListaConexoes {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub conexoes: Vec<ConexaoBanco>,
    #[serde(default)]
    pub chaveiro_disponivel: bool,
}

/// Os campos do diálogo de conexão.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct CamposConexao {
    pub pasta: String,
    pub nome: String,
    pub tipo: String,
    pub host: String,
    pub porta: u16,
    pub usuario: String,
    pub banco: String,
    pub arquivo: String,
    pub ssl: String,
    pub ssl_ca: String,
    pub escrita: bool,
    pub agentes: bool,
}

/// O resultado do "Testar".
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Teste {
    pub ok: bool,
    #[serde(default)]
    pub ms: i64,
    #[serde(default)]
    pub servidor: String,
    #[serde(default)]
    pub tls: bool,
    #[serde(default)]
    pub erro: String,
    #[serde(default)]
    pub detalhe: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ColunaResultado {
    pub nome: String,
    #[serde(default)]
    pub tipo: String,
    #[serde(default)]
    pub numero: bool,
}

/// Uma página de resultado: cada célula é texto ou NULL.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Resultado {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub colunas: Vec<ColunaResultado>,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub linhas: Vec<Vec<Option<String>>>,
    #[serde(default)]
    pub mais: bool,
    #[serde(default)]
    pub ms: i64,
    #[serde(default)]
    pub afetadas: Option<i64>,
    #[serde(default)]
    pub verbo: String,
    #[serde(default)]
    pub altera: bool,
}

/// O que deu errado numa chamada ao banco, já do jeito que a tela mostra.
#[derive(Clone, Debug, PartialEq)]
pub enum ErroBanco {
    /// A conexão precisa da senha (sem chaveiro ou esquecida).
    Senha,
    /// A alteração precisa da sua confirmação: a instrução e o nonce.
    Confirmar(Confirmacao),
    Falhou(FalhaBanco),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Confirmacao {
    pub verbo: String,
    pub sql: String,
    #[serde(default)]
    pub banco: String,
    #[serde(default)]
    pub sem_where: bool,
    pub confirmacao: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct FalhaBanco {
    #[serde(default)]
    pub erro: String,
    #[serde(default)]
    pub detalhe: String,
    #[serde(default)]
    pub linha: usize,
    #[serde(default)]
    pub tempo_esgotado: bool,
    #[serde(default)]
    pub cancelada: bool,
    #[serde(default)]
    pub somente_leitura: bool,
    #[serde(default)]
    pub varias: bool,
    #[serde(default)]
    pub fechada: bool,
    /// O servidor recusou usuário ou senha (senha trocada lá, ou conexão
    /// salva sem senha): a tela oferece trocar a senha.
    #[serde(default)]
    pub senha_recusada: bool,
    /// A mensagem é a do servidor de banco (a tela mostra em mono).
    #[serde(default)]
    pub do_servidor: bool,
}

impl ErroBanco {
    fn de(texto: String) -> ErroBanco {
        ErroBanco::Falhou(FalhaBanco { erro: texto, ..Default::default() })
    }

    /// O texto principal do erro.
    pub fn texto(&self) -> String {
        match self {
            ErroBanco::Senha => "Digite a senha da conexão.".into(),
            ErroBanco::Confirmar(c) => format!("{} precisa de confirmação.", c.verbo),
            ErroBanco::Falhou(f) => f.erro.clone(),
        }
    }
}

/// Chamada às rotas de banco: a resposta de erro vira `ErroBanco`.
fn chamar_banco<T: DeserializeOwned>(metodo: &str, caminho: &str, corpo: Option<serde_json::Value>, espera: u64) -> Result<T, ErroBanco> {
    let corpo = corpo.map(|c| c.to_string());
    let (status, resposta) = canal::pedir_com_corpo_ate(metodo, caminho, corpo.as_deref(), std::time::Duration::from_secs(espera)).map_err(ErroBanco::de)?;
    if (200..300).contains(&status) {
        return serde_json::from_str(&resposta).map_err(|e| ErroBanco::de(inesperada(caminho, e)));
    }
    let valor: serde_json::Value = serde_json::from_str(&resposta).unwrap_or_default();
    if status == 428 || valor["precisa_senha"] == true {
        return Err(ErroBanco::Senha);
    }
    if valor["precisa_confirmar"] == true
        && let Ok(c) = serde_json::from_value::<Confirmacao>(valor.clone())
    {
        return Err(ErroBanco::Confirmar(c));
    }
    match serde_json::from_value::<FalhaBanco>(valor) {
        Ok(f) if !f.erro.is_empty() => Err(ErroBanco::Falhou(f)),
        _ => Err(ErroBanco::de(format!("erro {status} do núcleo"))),
    }
}

pub fn conexoes(perfil: i64) -> Result<ListaConexoes, String> {
    chamar("GET", &format!("/v1/perfis/{perfil}/conexoes"), None)
}

fn corpo_conexao(campos: &CamposConexao, senha: Option<&str>, guardar: bool) -> serde_json::Value {
    let mut corpo = serde_json::to_value(campos).unwrap_or_default();
    if let Some(s) = senha {
        corpo["senha"] = json!(s);
    }
    corpo["guardar"] = json!(guardar);
    corpo
}

pub fn criar_conexao(perfil: i64, campos: &CamposConexao, senha: Option<&str>, guardar: bool) -> Result<ConexaoBanco, String> {
    chamar("POST", &format!("/v1/perfis/{perfil}/conexoes"), Some(corpo_conexao(campos, senha, guardar)))
}

pub fn editar_conexao(id: i64, campos: &CamposConexao, senha: Option<&str>, guardar: bool) -> Result<ConexaoBanco, String> {
    chamar("PATCH", &format!("/v1/conexoes/{id}"), Some(corpo_conexao(campos, senha, guardar)))
}

pub fn remover_conexao(id: i64) -> Result<(), String> {
    chamar::<serde_json::Value>("DELETE", &format!("/v1/conexoes/{id}"), None).map(|_| ())
}

#[derive(Deserialize)]
struct Onde {
    senha: String,
}

/// Guarda a senha (no chaveiro, se pedido e se houver) e diz onde ficou.
pub fn definir_senha(id: i64, senha: &str, guardar: bool) -> Result<String, String> {
    chamar::<Onde>("PUT", &format!("/v1/conexoes/{id}/senha"), Some(json!({ "senha": senha, "guardar": guardar }))).map(|o| o.senha)
}

pub fn esquecer_senha(id: i64) -> Result<(), String> {
    chamar::<serde_json::Value>("DELETE", &format!("/v1/conexoes/{id}/senha"), None).map(|_| ())
}

/// Testa os campos do diálogo (sem senha e com `conexao`, a senha salva).
pub fn testar_rascunho(perfil: i64, campos: &CamposConexao, senha: &str, conexao: i64) -> Result<Teste, String> {
    let mut corpo = serde_json::to_value(campos).unwrap_or_default();
    corpo["senha"] = json!(senha);
    corpo["conexao_id"] = json!(conexao);
    chamar_banco("POST", &format!("/v1/perfis/{perfil}/conexoes/testar"), Some(corpo), 40).map_err(|e| e.texto())
}

pub fn testar_conexao(id: i64) -> Result<Teste, ErroBanco> {
    chamar_banco("POST", &format!("/v1/conexoes/{id}/testar"), None, 40)
}

pub fn desconectar(id: i64) -> Result<(), String> {
    chamar::<serde_json::Value>("POST", &format!("/v1/conexoes/{id}/desconectar"), None).map(|_| ())
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Nomes {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub nomes: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Objetos {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub tabelas: Vec<String>,
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    pub views: Vec<String>,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub cortado: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ColunaTabela {
    pub nome: String,
    #[serde(default)]
    pub tipo: String,
    #[serde(default)]
    pub pk: bool,
    #[serde(default)]
    pub fk: bool,
}

#[derive(Deserialize)]
struct Colunas {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    colunas: Vec<ColunaTabela>,
}

fn caminho_arvore(id: i64, nivel: &str, banco: &str, esquema: &str, objeto: &str) -> String {
    format!("/v1/conexoes/{id}/arvore?nivel={nivel}&banco={}&esquema={}&objeto={}", codificar_url(banco), codificar_url(esquema), codificar_url(objeto))
}

pub fn bancos_do_servidor(id: i64) -> Result<Nomes, ErroBanco> {
    chamar_banco("GET", &caminho_arvore(id, "bancos", "", "", ""), None, 70)
}

pub fn esquemas(id: i64, banco: &str) -> Result<Nomes, ErroBanco> {
    chamar_banco("GET", &caminho_arvore(id, "esquemas", banco, "", ""), None, 70)
}

pub fn objetos(id: i64, banco: &str, esquema: &str) -> Result<Objetos, ErroBanco> {
    chamar_banco("GET", &caminho_arvore(id, "objetos", banco, esquema, ""), None, 70)
}

pub fn colunas_da_tabela(id: i64, banco: &str, esquema: &str, objeto: &str) -> Result<Vec<ColunaTabela>, ErroBanco> {
    chamar_banco::<Colunas>("GET", &caminho_arvore(id, "colunas", banco, esquema, objeto), None, 70).map(|c| c.colunas)
}

#[derive(Deserialize)]
pub struct PreviaTabela {
    pub resultado: Resultado,
}

pub fn previa_tabela(id: i64, banco: &str, esquema: &str, objeto: &str) -> Result<PreviaTabela, ErroBanco> {
    chamar_banco("POST", &format!("/v1/conexoes/{id}/previa"), Some(json!({ "banco": banco, "esquema": esquema, "objeto": objeto })), 45)
}

/// Uma execução no console: espera até o tempo-limite (mais uma folga).
pub struct Execucao<'a> {
    pub ficha: &'a str,
    pub sql: &'a str,
    pub banco: &'a str,
    pub limite: u32,
    pub tempo_s: u32,
    pub confirmar: &'a str,
}

pub fn executar(id: i64, e: &Execucao) -> Result<Resultado, ErroBanco> {
    let corpo = json!({ "ficha": e.ficha, "sql": e.sql, "banco": e.banco, "limite": e.limite, "tempo_s": e.tempo_s, "confirmar": e.confirmar });
    chamar_banco("POST", &format!("/v1/conexoes/{id}/execucoes"), Some(corpo), e.tempo_s as u64 + 10)
}

pub fn carregar_mais(ficha: &str, limite: u32, tempo_s: u32) -> Result<Resultado, ErroBanco> {
    chamar_banco("GET", &format!("/v1/execucoes/{}/mais?limite={limite}", codificar_url(ficha)), None, tempo_s as u64 + 10)
}

pub fn cancelar_execucao(ficha: &str) -> Result<(), String> {
    chamar::<serde_json::Value>("DELETE", &format!("/v1/execucoes/{}", codificar_url(ficha)), None).map(|_| ())
}

/// Uma linha do histórico de consultas da conexão.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ConsultaFeita {
    pub id: i64,
    pub sql: String,
    #[serde(default)]
    pub banco: String,
    /// "voce" ou "agente".
    #[serde(default)]
    pub origem: String,
    #[serde(default)]
    pub agente_id: i64,
    #[serde(default)]
    pub momento: String,
    #[serde(default)]
    pub duracao_ms: i64,
    #[serde(default)]
    pub linhas: i64,
    #[serde(default)]
    pub erro: bool,
    #[serde(default)]
    pub altera: bool,
    /// Pedido do agente: "aprovada", "recusada", "expirou" ou "cancelada".
    #[serde(default)]
    pub resultado: String,
    /// "14:32" hoje, "02/10" antes.
    #[serde(default)]
    pub hora: String,
}

#[derive(Deserialize)]
struct Historico {
    #[serde(default, deserialize_with = "lista_ou_nulo")]
    consultas: Vec<ConsultaFeita>,
}

pub fn historico(id: i64) -> Result<Vec<ConsultaFeita>, String> {
    chamar::<Historico>("GET", &format!("/v1/conexoes/{id}/historico"), None).map(|h| h.consultas)
}

pub fn limpar_historico(id: i64) -> Result<(), String> {
    chamar::<serde_json::Value>("DELETE", &format!("/v1/conexoes/{id}/historico"), None).map(|_| ())
}

/// Um pedido de consulta de um agente, esperando você.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Aprovacao {
    pub id: String,
    pub conexao_id: i64,
    pub conexao: String,
    #[serde(default)]
    pub tipo: String,
    pub agente_id: i64,
    pub tarefa_id: i64,
    /// "Claude Code (dev)".
    #[serde(default)]
    pub agente: String,
    #[serde(default)]
    pub tarefa: String,
    pub sql: String,
    #[serde(default)]
    pub banco: String,
    #[serde(default)]
    pub limite: i64,
    #[serde(default)]
    pub criada_hora: String,
    #[serde(default)]
    pub expira_hora: String,
    #[serde(default)]
    pub precisa_senha: bool,
}

/// Aprovar (com a senha, se faltar) ou recusar (com um motivo opcional).
pub fn responder_aprovacao(id: &str, aprovar: bool, motivo: &str, senha: &str, guardar: bool) -> Result<(), String> {
    let corpo = json!({ "aprovar": aprovar, "motivo": motivo, "senha": senha, "guardar": guardar });
    chamar::<serde_json::Value>("POST", &format!("/v1/aprovacoes/{}", codificar_url(id)), Some(corpo)).map(|_| ())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn recorte_vira_consulta() {
        assert_eq!(Recorte::Perfil.consulta(), "");
        assert_eq!(Recorte::Workspace(2).consulta(), "&workspace=2");
        assert_eq!(Recorte::Projeto(7).consulta(), "&projeto=7");
    }

    #[test]
    fn sem_tempo_os_campos_ficam_em_zero() {
        // O núcleo com a opção desligada não manda os tempos.
        let deck: Deck = serde_json::from_value(json!({"tipo": "daily", "titulo": "Daily", "capa": {"numeros": {"concluidas": 1}},
            "slides": [{"tarefa_id": 1, "titulo": "A", "numeros": {"sessoes": 2}, "secao": "estudos · loja-web", "secao_id": 3, "workspace": "estudos"}]}))
        .unwrap();
        assert!(!deck.tempo_agentes && deck.capa.numeros.tempo_s == 0 && deck.capa.numeros.por_ferramenta.is_empty());
        assert_eq!((deck.slides[0].numeros.tempo_s, deck.slides[0].numeros.sessoes, deck.slides[0].secao_id), (0, 2, 3));
        let perfil: Perfil = serde_json::from_value(json!({"id": 1, "nome": "P", "tema": "escuro"})).unwrap();
        assert!(!perfil.tempo_agentes && perfil.aviso_captura);
    }

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
        assert!(d.fora.is_empty());
        let d: Deck = serde_json::from_str(r#"{"tipo":"daily","titulo":"Daily","fora":[{"tarefa_id":4,"titulo":"Tela nova","projeto":"loja-web"}]}"#).unwrap();
        assert_eq!((d.fora[0].tarefa_id, d.fora[0].titulo.as_str()), (4, "Tela nova"));
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

#[cfg(test)]
mod testes_bancos {
    use super::*;

    #[test]
    fn resultado_com_null_e_afetadas() {
        let r: Resultado =
            serde_json::from_str(r#"{"colunas":[{"nome":"id","tipo":"int4","numero":true}],"linhas":[["1"],[null]],"mais":true,"ms":3,"verbo":"SELECT"}"#)
                .unwrap();
        assert_eq!(r.linhas[1][0], None);
        assert!(r.mais && r.colunas[0].numero && r.afetadas.is_none());
        let r: Resultado = serde_json::from_str(r#"{"colunas":[],"linhas":[],"mais":false,"ms":3,"afetadas":12,"verbo":"UPDATE","altera":true}"#).unwrap();
        assert_eq!(r.afetadas, Some(12));
    }

    #[test]
    fn senha_recusada_vem_marcada() {
        let f: FalhaBanco = serde_json::from_str(r#"{"erro":"Usuário ou senha recusados.","detalhe":"Error 1045","senha_recusada":true}"#).unwrap();
        assert!(f.senha_recusada);
        let f: FalhaBanco = serde_json::from_str(r#"{"erro":"Não conectou.","detalhe":"x"}"#).unwrap();
        assert!(!f.senha_recusada);
    }

    #[test]
    fn campos_vao_no_formato_do_nucleo() {
        let c = CamposConexao { nome: "loja-web-dev".into(), tipo: "postgres".into(), porta: 5432, ..Default::default() };
        let v = corpo_conexao(&c, Some("x"), true);
        assert_eq!(v["nome"], "loja-web-dev");
        assert_eq!(v["ssl_ca"], "");
        assert_eq!(v["senha"], "x");
        assert!(corpo_conexao(&c, None, false).get("senha").is_none());
    }
}
