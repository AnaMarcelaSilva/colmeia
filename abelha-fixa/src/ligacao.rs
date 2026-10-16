//! A ligação com o núcleo: uma thread que dorme na leitura do WebSocket de
//! eventos do perfil (como a janela, sem consulta periódica) e manda para a
//! tela o retrato do quadro e os eventos. Se o núcleo sair, tenta de novo
//! em 1, 2, 4… até 30 s. A abelha não inicia o núcleo: quem inicia é a janela.

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use eframe::egui;
use tungstenite::Message;

use crate::resumo::{Evento, Quadro};

pub enum Mensagem {
    /// O perfil (a abelha e o nome), lido a cada vez que liga.
    Perfil(Option<Perfil>),
    Quadro(Quadro),
    Evento(Evento),
    /// O núcleo saiu do ar (ou recusou): a abelha dorme até voltar.
    Fora,
}

const ESPERA_MAXIMA: Duration = Duration::from_secs(30);

pub fn iniciar(perfil: i64, ctx: egui::Context, envio: Sender<Mensagem>) {
    thread::Builder::new()
        .name("eventos".into())
        .spawn(move || {
            let mut espera = Duration::from_secs(1);
            loop {
                let ligou = ouvir(perfil, &ctx, &envio);
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
fn ouvir(perfil_id: i64, ctx: &egui::Context, envio: &Sender<Mensagem>) -> bool {
    // `quem=abelha`: recebe o mesmo que a janela, mas não conta como janela aberta.
    let Ok(mut ws) = canal::websocket(&format!("/v1/perfis/{perfil_id}/eventos?quem=abelha")) else {
        return false;
    };
    loop {
        let texto = match ws.read() {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => return true,
            Ok(_) => continue,
        };
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
pub fn abrir_colmeia(perfil: i64, tarefa: i64, agente: Option<i64>) {
    let _ = thread::Builder::new().name("abrir".into()).spawn(move || {
        let corpo = serde_json::json!({ "tarefa_id": tarefa, "agente_id": agente.unwrap_or(0) }).to_string();
        let janelas = canal::pedir_com_corpo("POST", &format!("/v1/perfis/{perfil}/abrir"), Some(&corpo))
            .ok()
            .filter(|(status, _)| *status == 200)
            .and_then(|(_, c)| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| v["janelas"].as_u64());
        if janelas.is_some_and(|n| n > 0) {
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
