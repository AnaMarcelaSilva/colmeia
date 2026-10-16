//! A vida do processo: espera pedidos da tela, ouve, transcreve e sai
//! depois de um tempo parado (ou quando a tela fecha a entrada).

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::audio::{self, Gravacao};
use crate::protocolo::{self, Comando, Evento};
use crate::transcrever::{self, Transcritor};

/// Sem pedidos por este tempo, o processo sai e o modelo sai da memória.
pub const PARADO_PADRAO: Duration = Duration::from_secs(120);

#[derive(Debug, PartialEq)]
pub struct Opcoes {
    pub modelo: PathBuf,
    pub parado: Duration,
}

impl Opcoes {
    /// `--modelo <arquivo>` e, opcional, `--parado-ms <n>` (de 100 ms a 1 h).
    pub fn ler(mut args: impl Iterator<Item = String>) -> Result<Opcoes, String> {
        let (mut modelo, mut parado) = (None, PARADO_PADRAO);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--modelo" => modelo = Some(PathBuf::from(args.next().ok_or("falta o arquivo depois de --modelo")?)),
                "--parado-ms" => {
                    let ms: u64 = args.next().and_then(|v| v.parse().ok()).ok_or("--parado-ms precisa de um número")?;
                    parado = Duration::from_millis(ms.clamp(100, 3_600_000));
                }
                outro => return Err(format!("opção desconhecida: {outro}")),
            }
        }
        Ok(Opcoes { modelo: modelo.ok_or("falta --modelo")?, parado })
    }
}

/// O modelo carrega numa thread desde o início: quando você solta o botão,
/// ele já está pronto.
enum Modelo {
    Carregando(JoinHandle<Result<Transcritor, String>>),
    Pronto(Transcritor),
    Falhou(String),
}

impl Modelo {
    fn pronto(&mut self) -> Result<&Transcritor, String> {
        if let Modelo::Carregando(_) = self {
            let Modelo::Carregando(h) = std::mem::replace(self, Modelo::Falhou(String::new())) else { unreachable!() };
            *self = match h.join() {
                Ok(Ok(t)) => Modelo::Pronto(t),
                Ok(Err(e)) => Modelo::Falhou(e),
                Err(_) => Modelo::Falhou("O modelo de voz não abriu.".into()),
            };
        }
        match self {
            Modelo::Pronto(t) => Ok(t),
            Modelo::Falhou(e) => Err(e.clone()),
            Modelo::Carregando(_) => unreachable!(),
        }
    }
}

fn emitir(evento: &Evento) -> bool {
    let mut saida = std::io::stdout().lock();
    writeln!(saida, "{}", protocolo::linha(evento)).and_then(|_| saida.flush()).is_ok()
}

/// Lê a entrada numa thread; o fim da entrada (a tela fechou) vira `Sair`.
fn ler_entrada() -> mpsc::Receiver<Result<Comando, String>> {
    let (envio, recebe) = mpsc::channel();
    std::thread::spawn(move || {
        for linha in std::io::stdin().lock().lines() {
            let Ok(linha) = linha else { break };
            if envio.send(protocolo::ler(&linha)).is_err() {
                return;
            }
        }
        let _ = envio.send(Ok(Comando::Sair));
    });
    recebe
}

pub fn rodar(opcoes: Opcoes) -> ExitCode {
    // Sem isto o whisper.cpp escreve o carregamento inteiro na saída de erro.
    whisper_rs::install_logging_hooks();
    let caminho = opcoes.modelo.clone();
    let mut modelo = Modelo::Carregando(std::thread::spawn(move || transcrever::carregar(&caminho)));
    let pedidos = ler_entrada();
    if !emitir(&Evento::Iniciado) {
        return ExitCode::FAILURE;
    }
    let mut gravacao: Option<(Gravacao, Instant)> = None;
    loop {
        // Ouvindo, o limite é o fim da fala mais longa; parado, a falta de uso.
        let espera = match &gravacao {
            Some((_, desde)) => Duration::from_secs(audio::LIMITE_SEGUNDOS.into()).saturating_sub(desde.elapsed()),
            None => opcoes.parado,
        };
        let pedido = match pedidos.recv_timeout(espera) {
            Ok(p) => p,
            Err(RecvTimeoutError::Timeout) if gravacao.is_some() => Ok(Comando::Parar),
            Err(RecvTimeoutError::Timeout) => {
                emitir(&Evento::Saindo { motivo: "parado" });
                return ExitCode::SUCCESS;
            }
            Err(RecvTimeoutError::Disconnected) => Ok(Comando::Sair),
        };
        let evento = match pedido {
            Err(mensagem) => Evento::Erro { mensagem },
            Ok(Comando::Sair) => return ExitCode::SUCCESS,
            Ok(Comando::Ouvir) if gravacao.is_some() => continue,
            Ok(Comando::Ouvir) => match audio::ouvir() {
                Ok(g) => {
                    gravacao = Some((g, Instant::now()));
                    Evento::Ouvindo
                }
                Err(mensagem) => Evento::Erro { mensagem },
            },
            // O áudio vai embora junto com a gravação.
            Ok(Comando::Cancelar) => {
                gravacao = None;
                continue;
            }
            Ok(Comando::Parar) => match gravacao.take() {
                None => Evento::Erro { mensagem: "O microfone não estava aberto.".into() },
                Some((g, _)) => transcrever_gravacao(g, &mut modelo),
            },
        };
        if !emitir(&evento) {
            return ExitCode::FAILURE;
        }
    }
}

fn transcrever_gravacao(g: Gravacao, modelo: &mut Modelo) -> Evento {
    let audio = match g.terminar() {
        Ok(a) => a,
        Err(mensagem) => return Evento::Erro { mensagem },
    };
    if audio::curta(&audio) {
        return Evento::Erro { mensagem: "Foi rápido demais. Segure o botão (ou o atalho) enquanto fala.".into() };
    }
    if audio::silencio(&audio) {
        return Evento::Erro { mensagem: "Não ouvi nada. Confira se o microfone está ligado e sem o mudo.".into() };
    }
    if !emitir(&Evento::Transcrevendo { segundos: audio.len() as f32 / audio::TAXA_WHISPER as f32 }) {
        return Evento::Erro { mensagem: String::new() };
    }
    match modelo.pronto().and_then(|t| t.transcrever(&audio)) {
        Ok(texto) => Evento::Texto { texto },
        Err(mensagem) => Evento::Erro { mensagem },
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn opcoes_da_linha_de_comando() {
        let o = Opcoes::ler(args(&["--modelo", "/m/ggml-base.bin"])).unwrap();
        assert_eq!(o, Opcoes { modelo: "/m/ggml-base.bin".into(), parado: PARADO_PADRAO });
        let o = Opcoes::ler(args(&["--parado-ms", "5", "--modelo", "m"])).unwrap();
        assert_eq!(o.parado, Duration::from_millis(100));
        assert!(Opcoes::ler(args(&[])).is_err());
        assert!(Opcoes::ler(args(&["--modelo"])).is_err());
        assert!(Opcoes::ler(args(&["--modelo", "m", "--parado-ms", "x"])).is_err());
        assert!(Opcoes::ler(args(&["--modelo", "m", "--arquivo", "a.wav"])).is_err());
    }
}
