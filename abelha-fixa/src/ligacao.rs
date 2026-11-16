//! A ligação com o núcleo: uma thread que dorme na leitura do WebSocket de
//! eventos do perfil (como a janela, sem consulta periódica) e manda para a
//! tela o retrato do quadro e os eventos. Se o núcleo sair, tenta de novo
//! em 1, 2, 4… até 30 s. A abelha não inicia o núcleo: quem inicia é a janela.

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use eframe::egui;
use tungstenite::Message;

use crate::resumo::{Evento, OpcoesVoz, Quadro};

pub enum Mensagem {
    /// O perfil (a abelha e o nome), lido a cada vez que liga.
    Perfil(Option<Perfil>),
    Quadro(Quadro),
    Evento(Evento),
    /// O núcleo saiu do ar (ou recusou): a abelha dorme até voltar.
    Fora,
    /// O atalho global dos comandos falados foi apertado.
    Atalho,
    /// Quantas janelas da Colmeia do perfil estão à vista (abertas, não
    /// minimizadas e com o foco): com alguma, a abelha se esconde.
    Janelas(usize),
    /// As últimas linhas do terminal de um agente (a conversa aberta).
    Texto {
        agente: i64,
        resultado: Result<TextoAgente, String>,
    },
    /// O que foi escrito no agente chegou (ou não).
    Entrada(Result<(), String>),
    /// A tarefa nova foi criada (com o agente, se pedido).
    Criada(Result<(i64, Option<i64>), String>),
    /// O agente da conversa foi ligado (retomado ou criado), ou não.
    Iniciado {
        tarefa: i64,
        resultado: Result<i64, String>,
    },
    /// Você voltou depois de uma pausa (sem mexer no computador) desse tamanho.
    Voltou(Duration),
}

/// O texto recente do terminal de um agente, como o núcleo entrega.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct TextoAgente {
    #[serde(default)]
    pub linhas: Vec<String>,
    #[serde(default)]
    pub rodando: bool,
    #[serde(default)]
    pub estado: String,
    #[serde(default)]
    pub motivo: String,
}

/// O "erro" de uma resposta do núcleo, ou o status.
fn erro_de(status: u16, corpo: &str) -> String {
    serde_json::from_str::<serde_json::Value>(corpo)
        .ok()
        .and_then(|v| v["erro"].as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("o núcleo respondeu {status}"))
}

/// As últimas linhas do terminal do agente, em texto puro.
pub fn texto_do_agente(agente: i64, linhas: usize) -> Result<TextoAgente, String> {
    let (status, corpo) = canal::pedir_com_corpo("GET", &format!("/v1/agentes/{agente}/texto?linhas={linhas}"), None)?;
    if status != 200 {
        return Err(erro_de(status, &corpo));
    }
    serde_json::from_str(&corpo).map_err(|e| e.to_string())
}

/// Escreve no terminal do agente: com `enter`, uma mensagem; sem, só as teclas.
pub fn entrada_do_agente(agente: i64, texto: &str, enter: bool) -> Result<(), String> {
    let corpo = serde_json::json!({ "texto": texto, "enter": enter }).to_string();
    let (status, resposta) = canal::pedir_com_corpo("POST", &format!("/v1/agentes/{agente}/entrada"), Some(&corpo))?;
    if status != 200 {
        return Err(erro_de(status, &resposta));
    }
    Ok(())
}

/// Cria uma tarefa no projeto, na pasta dele e na branch dada (a base do
/// projeto; vazia sem git), e, se pedido, já sobe um Claude Code nela.
pub fn criar_tarefa(projeto: i64, titulo: &str, branch: &str, com_agente: bool) -> Result<(i64, Option<i64>), String> {
    let corpo = serde_json::json!({ "titulo": titulo, "branch": branch, "local": "pasta" }).to_string();
    let (status, resposta) = canal::pedir_com_corpo("POST", &format!("/v1/projetos/{projeto}/tarefas"), Some(&corpo))?;
    if status != 200 {
        return Err(erro_de(status, &resposta));
    }
    let tarefa = serde_json::from_str::<serde_json::Value>(&resposta).ok().and_then(|v| v["id"].as_i64()).ok_or("a tarefa veio sem número")?;
    if !com_agente {
        return Ok((tarefa, None));
    }
    match criar_agente(tarefa) {
        Ok(agente) => Ok((tarefa, Some(agente))),
        Err(e) => Err(format!("a tarefa foi criada, mas o agente não subiu: {e}")),
    }
}

/// Sobe um Claude Code (dev) na tarefa e devolve o número dele. Sem
/// tamanho: o núcleo abre o terminal no tamanho da última tela da Colmeia
/// (a conversa daqui mostra só o texto), senão o desenho embaralha lá.
pub fn criar_agente(tarefa: i64) -> Result<i64, String> {
    let corpo = serde_json::json!({ "ferramenta": "claude", "papel": "dev" }).to_string();
    let (status, resposta) = canal::pedir_com_corpo("POST", &format!("/v1/tarefas/{tarefa}/agentes"), Some(&corpo))?;
    if status != 200 {
        return Err(erro_de(status, &resposta));
    }
    serde_json::from_str::<serde_json::Value>(&resposta).ok().and_then(|v| v["id"].as_i64()).ok_or_else(|| "o agente veio sem número".to_string())
}

/// Liga de novo um agente parado (o núcleo retoma a sessão dele, se houver).
pub fn iniciar_agente(agente: i64) -> Result<i64, String> {
    let (status, resposta) = canal::pedir_com_corpo("POST", &format!("/v1/agentes/{agente}/iniciar"), None)?;
    if status != 200 {
        return Err(erro_de(status, &resposta));
    }
    Ok(agente)
}

/// As janelas à vista, no olá e no aviso "tela.janelas".
fn janelas_visiveis(texto: &str) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_str(texto).ok()?;
    let n = match v["tipo"].as_str()? {
        "tela.janelas" => v["visiveis"].as_u64()?,
        "ola" => v["janelas_visiveis"].as_u64().unwrap_or(0),
        _ => return None,
    };
    Some(n as usize)
}

const ESPERA_MAXIMA: Duration = Duration::from_secs(30);

/// A Colmeia avisou que vai reiniciar numa versão nova: quando o núcleo
/// voltar, a abelha se reabre com o programa novo.
fn reiniciando(texto: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(texto).is_ok_and(|v| v["tipo"] == "atualizacao" && v["estado"] == "reiniciando")
}

/// Troca este processo pelo programa que está agora no caminho de quando a
/// abelha abriu (a atualização pôs a versão nova lá), com os mesmos
/// argumentos. A trava do perfil fecha junto e a abelha nova pega de novo.
#[cfg(unix)]
fn reabrir(programa: &std::path::Path) {
    use std::os::unix::process::CommandExt;
    let erro = std::process::Command::new(programa).args(std::env::args_os().skip(1)).exec();
    eprintln!("abelha: não consegui abrir a versão nova: {erro}");
}

#[cfg(not(unix))]
fn reabrir(_: &std::path::Path) {}

pub fn iniciar(perfil: i64, ctx: egui::Context, envio: Sender<Mensagem>) {
    // O caminho é lido já: depois da troca, o do processo aponta para o programa apagado.
    let programa = std::env::current_exe().ok();
    thread::Builder::new()
        .name("eventos".into())
        .spawn(move || {
            let mut espera = Duration::from_secs(1);
            let mut reabrir_ao_voltar = false;
            loop {
                let ligou = ouvir(perfil, &ctx, &envio, &mut reabrir_ao_voltar, programa.as_deref());
                if envio.send(Mensagem::Fora).is_err() {
                    return;
                }
                ctx.request_repaint();
                espera = if ligou { Duration::from_secs(1) } else { (espera * 2).min(ESPERA_MAXIMA) };
                thread::sleep(espera);
            }
        })
        .expect("thread de eventos");
}

/// Lê os eventos até a conexão cair; diz se chegou a ligar.
fn ouvir(perfil_id: i64, ctx: &egui::Context, envio: &Sender<Mensagem>, reabrir_ao_voltar: &mut bool, programa: Option<&std::path::Path>) -> bool {
    // `quem=abelha`: recebe o mesmo que a janela, mas não conta como janela aberta.
    let Ok(mut ws) = canal::websocket(&format!("/v1/perfis/{perfil_id}/eventos?quem=abelha")) else {
        return false;
    };
    // Ligou de novo depois do aviso: o núcleo novo está no ar.
    if *reabrir_ao_voltar && let Some(programa) = programa {
        reabrir(programa);
        *reabrir_ao_voltar = false;
    }
    loop {
        let texto = match ws.read() {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => return true,
            Ok(_) => continue,
        };
        if reiniciando(&texto) {
            *reabrir_ao_voltar = true;
        }
        if let Some(n) = janelas_visiveis(&texto) {
            if envio.send(Mensagem::Janelas(n)).is_err() {
                return true;
            }
            ctx.request_repaint();
        }
        let Ok(evento) = serde_json::from_str::<Evento>(&texto) else {
            continue;
        };
        // Ao ligar, o perfil de novo (pode ter mudado com a abelha desligada).
        if matches!(evento, Evento::Ola {}) {
            let p = match perfil(perfil_id) {
                Ok(p) => Some(p),
                Err(Erro::NaoExiste) => None,
                Err(Erro::Canal) => return true,
            };
            if envio.send(Mensagem::Perfil(p)).is_err() {
                return true;
            }
        }
        // Ao ligar e quando ficou para trás: um retrato novo.
        let mensagem = if matches!(evento, Evento::Ola {} | Evento::Recarregar {}) {
            match canal::pedir("GET", &format!("/v1/perfis/{perfil_id}/quadro")).ok().and_then(|c| serde_json::from_str(&c).ok()) {
                Some(q) => Mensagem::Quadro(q),
                None => return true,
            }
        } else if matches!(evento, Evento::Outro) {
            continue;
        } else {
            Mensagem::Evento(evento)
        };
        if envio.send(mensagem).is_err() {
            return true;
        }
        ctx.request_repaint();
    }
}

/// O perfil como o núcleo guarda: o nome e o acabamento da abelha.
#[derive(serde::Deserialize)]
pub struct Perfil {
    id: i64,
    #[serde(default)]
    pub mascote_modelo: String,
    #[serde(default)]
    pub mascote_nome: String,
    #[serde(flatten)]
    pub voz: OpcoesVoz,
}

enum Erro {
    NaoExiste,
    Canal,
}

fn perfil(id: i64) -> Result<Perfil, Erro> {
    let corpo = canal::pedir("GET", "/v1/perfis").map_err(|_| Erro::Canal)?;
    let perfis: Vec<Perfil> = serde_json::from_str(&corpo).map_err(|_| Erro::Canal)?;
    perfis.into_iter().find(|p| p.id == id).ok_or(Erro::NaoExiste)
}

/// "Abrir a Colmeia": pede às janelas abertas que mostrem a tarefa e, se não
/// houver nenhuma, abre o app já nela. Fora da thread da tela.
/// Com `ditar` (o "responder" falado), a janela liga o ditado na caixa do agente.
pub fn abrir_colmeia(perfil: i64, tarefa: i64, agente: Option<i64>, ditar: bool) {
    let _ = thread::Builder::new().name("abrir".into()).spawn(move || {
        let corpo = serde_json::json!({ "tarefa_id": tarefa, "agente_id": agente.unwrap_or(0), "ditar": ditar }).to_string();
        let janelas = canal::pedir_com_corpo("POST", &format!("/v1/perfis/{perfil}/abrir"), Some(&corpo))
            .ok()
            .filter(|(status, _)| *status == 200)
            .and_then(|(_, c)| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| v["janelas"].as_u64());
        if janelas.is_some_and(|n| n > 0) {
            crate::fixar::trazer_a_colmeia();
            return;
        }
        let Some(app) = canal::executavel("colmeia", "COLMEIA_APP") else {
            eprintln!("não encontrei o programa colmeia");
            return;
        };
        let mut comando = std::process::Command::new(app);
        comando.arg("--perfil").arg(perfil.to_string());
        if tarefa != 0 {
            comando.arg("--tarefa").arg(tarefa.to_string());
            if let Some(a) = agente {
                comando.arg("--agente").arg(a.to_string());
                if ditar {
                    comando.arg("--ditar");
                }
            }
        }
        if let Err(e) = canal::iniciar_solto(comando) {
            eprintln!("abrindo a Colmeia: {}", canal::erro_ao_iniciar(&e));
        }
    });
}

/// "Desafixar": grava no perfil; o evento de volta fecha a abelha (e
/// desliga a opção na janela). Sem o núcleo, a abelha sai do mesmo jeito.
pub fn desafixar(perfil: i64) -> bool {
    canal::pedir_com_corpo("PATCH", &format!("/v1/perfis/{perfil}"), Some(r#"{"abelha_fixa":false}"#)).is_ok_and(|(s, _)| s == 200)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn janelas_a_vista_no_ola_e_no_aviso() {
        assert_eq!(janelas_visiveis(r#"{"tipo":"ola","seq":3,"janelas_visiveis":2}"#), Some(2));
        // Um núcleo antigo não manda: nenhuma (a abelha aparece).
        assert_eq!(janelas_visiveis(r#"{"tipo":"ola","seq":3}"#), Some(0));
        assert_eq!(janelas_visiveis(r#"{"tipo":"tela.janelas","seq":9,"visiveis":0}"#), Some(0));
        assert_eq!(janelas_visiveis(r#"{"tipo":"tarefa.atualizada","seq":9}"#), None);
        assert_eq!(janelas_visiveis("lixo"), None);
    }
}
