//! Eventos em tempo real do núcleo: uma thread por perfil aberto, que dorme
//! numa leitura sem tempo limite até chegar uma mensagem. Nada de consulta
//! periódica: a tela só redesenha quando algo mudou.
//!
//! Para não perder nada entre o retrato do quadro e as mensagens: abre o
//! WebSocket, pede o retrato (que traz o número da última mensagem incluída
//! nele) e, daí em diante, só repassa as mensagens com número maior. Se a
//! conexão cair, tenta de novo em 1, 2, 4… até 30 s, só enquanto estiver fora.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use tungstenite::Message;

use crate::api;
use crate::canal;
use crate::dados::Evento;

/// O que a thread conta para a tela.
pub enum Mensagem {
    /// Ligado de novo (ou pela primeira vez), com o retrato do quadro.
    Quadro(api::Quadro),
    Evento(Evento),
    /// O núcleo saiu do ar; a thread segue tentando.
    Desconectado,
    /// O núcleo em execução não tem eventos (versão anterior).
    Antigo,
    /// Aviso do núcleo ao conectar (o histórico foi alterado por fora, por exemplo).
    Aviso(String),
}

pub struct Ouvinte {
    pub recebe: Receiver<Mensagem>,
    acordar: Sender<()>,
    estado: Arc<Compartilhado>,
}

#[derive(Default)]
struct Compartilhado {
    parar: Mutex<bool>,
    /// Cópia do socket aberto, para fechar a leitura bloqueada ao sair.
    #[cfg(any(unix, windows))]
    socket: Mutex<Option<canal::Fluxo>>,
}

const ESPERA_MAXIMA: Duration = Duration::from_secs(30);

impl Ouvinte {
    pub fn iniciar(perfil: i64, ctx: egui::Context) -> Ouvinte {
        let (envio, recebe) = mpsc::channel();
        let (acordar, acordado) = mpsc::channel();
        let estado = Arc::new(Compartilhado::default());
        let compartilhado = estado.clone();
        thread::Builder::new()
            .name(format!("eventos-{perfil}"))
            .spawn(move || ouvir(perfil, &ctx, &envio, &acordado, &compartilhado))
            .expect("thread de eventos");
        Ouvinte { recebe, acordar, estado }
    }

    /// "Tentar agora": tenta de novo já e, se o núcleo não estiver rodando,
    /// inicia um (a thread faz isso, para a tela não travar esperando).
    pub fn tentar_agora(&self) {
        let _ = self.acordar.send(());
    }
}

impl Drop for Ouvinte {
    /// Trocar de perfil encerra a thread: fecha o socket, o que solta a leitura.
    fn drop(&mut self) {
        *self.estado.parar.lock().unwrap() = true;
        #[cfg(any(unix, windows))]
        if let Some(s) = self.estado.socket.lock().unwrap().take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        let _ = self.acordar.send(());
    }
}

fn ouvir(perfil: i64, ctx: &egui::Context, envio: &Sender<Mensagem>, acordado: &Receiver<()>, estado: &Compartilhado) {
    let mut espera = Duration::from_secs(1);
    // A tela já sabe que está fora: não repete o aviso a cada tentativa.
    let mut fora = false;
    // Só "Tentar agora" inicia o núcleo: as tentativas sozinhas apenas
    // reconectam, para não ressuscitar um núcleo que alguém encerrou de
    // propósito ("Parar todos e fechar", `--encerrar`).
    let mut iniciar = false;
    loop {
        if *estado.parar.lock().unwrap() {
            return;
        }
        if std::mem::take(&mut iniciar) {
            if let Err(e) = canal::garantir_nucleo() {
                let _ = envio.send(Mensagem::Aviso(format!("Não consegui iniciar o núcleo: {e}.")));
                ctx.request_repaint();
            }
            // Cliques repetidos enquanto iniciava valem pelo mesmo pedido.
            descartar_pedidos(acordado);
        }
        match conectar(perfil, ctx, envio, estado) {
            // Esteve ligado: a próxima queda começa a contar de novo. Pedido
            // que sobrou na fila é antigo e não pode iniciar o núcleo sozinho
            // depois de um `--encerrar`.
            Resultado::Caiu => {
                (espera, fora) = (Duration::from_secs(1), false);
                descartar_pedidos(acordado);
            }
            Resultado::Recusado => {}
            Resultado::Antigo if !fora => {
                let _ = envio.send(Mensagem::Antigo);
                ctx.request_repaint();
                fora = true;
            }
            Resultado::Antigo => {}
            Resultado::TelaFechou => return,
        }
        if *estado.parar.lock().unwrap() {
            return;
        }
        // Só redesenha quando muda para fora; as tentativas seguintes não
        // mudam nada na tela.
        if !fora {
            let _ = envio.send(Mensagem::Desconectado);
            ctx.request_repaint();
            fora = true;
        }
        match acordado.recv_timeout(espera) {
            // "Tentar agora" volta a tentar já e recomeça a contagem.
            Ok(()) => (espera, iniciar) = (Duration::from_secs(1), true),
            Err(RecvTimeoutError::Timeout) => espera = (espera * 2).min(ESPERA_MAXIMA),
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn descartar_pedidos(acordado: &Receiver<()>) {
    while acordado.try_recv().is_ok() {}
}

enum Resultado {
    Caiu,
    Recusado,
    Antigo,
    TelaFechou,
}

fn conectar(perfil: i64, ctx: &egui::Context, envio: &Sender<Mensagem>, estado: &Compartilhado) -> Resultado {
    let mut ws = match canal::websocket(&format!("/v1/perfis/{perfil}/eventos")) {
        Ok(ws) => ws,
        // Um núcleo da entrega B não tem a rota: responde 404.
        Err(e) if e.contains("404") => return Resultado::Antigo,
        Err(_) => return Resultado::Recusado,
    };
    #[cfg(any(unix, windows))]
    {
        *estado.socket.lock().unwrap() = ws.get_ref().try_clone().ok();
        // Pode ter mandado parar enquanto conectava.
        if *estado.parar.lock().unwrap() {
            return Resultado::TelaFechou;
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = estado;
    let mut base = None;
    loop {
        let texto = match ws.read() {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => return Resultado::Caiu,
            Ok(_) => continue,
        };
        let Ok(evento) = serde_json::from_str::<Evento>(&texto) else {
            continue;
        };
        if let Evento::Ola { aviso: Some(a), .. } = &evento {
            let _ = envio.send(Mensagem::Aviso(a.clone()));
        }
        // Ao ligar e quando ficou para trás: um retrato novo do quadro.
        let mensagem = if matches!(evento, Evento::Ola { .. } | Evento::Recarregar) {
            match api::quadro(perfil) {
                Ok(q) => {
                    base = Some(q.seq);
                    Mensagem::Quadro(q)
                }
                Err(_) => return Resultado::Caiu,
            }
        } else if base.is_none_or(|b| evento.seq() <= b) {
            // Já incluído no retrato.
            continue;
        } else {
            Mensagem::Evento(evento)
        };
        if envio.send(mensagem).is_err() {
            return Resultado::TelaFechou;
        }
        ctx.request_repaint();
    }
}
