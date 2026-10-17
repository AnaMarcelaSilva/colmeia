//! A voz da abelha: o texto da resposta vira fonemas (regras de pt-BR daqui),
//! o Kokoro faz a onda com a voz pf_dora e ela toca na saída de som, frase a
//! frase (a primeira toca enquanto a segunda é feita). Numa thread própria,
//! que carrega o modelo no primeiro pedido e o solta depois de parada.

pub mod efeito;
pub mod fonemas;
pub mod kokoro;
pub mod texto;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use crate::audio::Tocador;

/// Os três arquivos que o núcleo baixou e conferiu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arquivos {
    pub modelo: PathBuf,
    pub voz: PathBuf,
    pub runtime: PathBuf,
}

/// Um pouco mais rápida que o normal: respostas curtas soam mais naturais assim.
const VELOCIDADE: f32 = 1.05;
/// Frases até este tamanho (em letras) vão inteiras ao modelo.
const FRASE: usize = 220;
/// Um respiro entre uma frase e outra.
const RESPIRO_MS: usize = 140;

/// Os fonemas de cada frase do texto, prontos para o Kokoro.
pub fn preparar(t: &str) -> Vec<String> {
    texto::frases(t, FRASE).iter().map(|f| fonemas::frase(&texto::pedacos(f))).filter(|f| f.chars().any(char::is_alphabetic)).collect()
}

#[derive(Debug, PartialEq)]
pub enum Aviso {
    Falando,
    Falou,
    Erro(String),
}

struct Pedido {
    texto: String,
    efeito: bool,
}

pub struct Voz {
    envio: Sender<Pedido>,
    calar: Arc<AtomicBool>,
}

impl Voz {
    /// A thread da voz. `parado`: sem falar por esse tempo, o modelo sai da memória.
    pub fn nova(arquivos: Arquivos, linhas: usize, parado: Duration, avisar: impl Fn(Aviso) + Send + 'static) -> Voz {
        let (envio, recebe) = mpsc::channel::<Pedido>();
        let calar = Arc::new(AtomicBool::new(false));
        let calada = Arc::clone(&calar);
        std::thread::Builder::new()
            .name("voz da abelha".into())
            .spawn(move || {
                let mut modelo: Option<kokoro::Kokoro> = None;
                loop {
                    let pedido = match recebe.recv_timeout(parado) {
                        Ok(p) => p,
                        Err(RecvTimeoutError::Timeout) => {
                            if modelo.take().is_some() {
                                crate::audio::devolver_memoria();
                            }
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    };
                    calada.store(false, Ordering::SeqCst);
                    if modelo.is_none() {
                        match kokoro::carregar(&arquivos.modelo, &arquivos.voz, &arquivos.runtime, linhas) {
                            Ok(k) => modelo = Some(k),
                            Err(e) => {
                                avisar(Aviso::Erro(e));
                                continue;
                            }
                        }
                    }
                    let Some(k) = modelo.as_mut() else { continue };
                    avisar(Aviso::Falando);
                    avisar(match falar(k, &pedido, &calada) {
                        Ok(()) => Aviso::Falou,
                        Err(e) => Aviso::Erro(e),
                    });
                }
            })
            .expect("thread da voz");
        Voz { envio, calar }
    }

    pub fn falar(&self, texto: &str, efeito: bool) {
        self.calar.store(false, Ordering::SeqCst);
        let _ = self.envio.send(Pedido { texto: texto.to_string(), efeito });
    }

    /// Para na hora o que está falando.
    pub fn calar(&self) {
        self.calar.store(true, Ordering::SeqCst);
    }
}

fn falar(k: &mut kokoro::Kokoro, pedido: &Pedido, calar: &AtomicBool) -> Result<(), String> {
    let frases = preparar(&pedido.texto);
    if frases.is_empty() {
        return Ok(());
    }
    let mut tocador = Tocador::abrir(kokoro::TAXA)?;
    let respiro = vec![0.0f32; kokoro::TAXA as usize * RESPIRO_MS / 1000];
    for f in &frases {
        if calar.load(Ordering::SeqCst) {
            break;
        }
        let mut onda = k.falar(f, VELOCIDADE)?;
        if pedido.efeito {
            efeito::aplicar(&mut onda, kokoro::TAXA);
        }
        tocador.somar(&onda);
        tocador.somar(&respiro);
    }
    // Espera tocar até o fim (o servidor de som guarda um pedaço antes de
    // tocar), ou o "calar".
    while !calar.load(Ordering::SeqCst) {
        let falta = tocador.falta();
        if falta.is_zero() {
            break;
        }
        std::thread::sleep(falta.min(Duration::from_millis(50)));
    }
    tocador.calar();
    // Um respiro para a última amostra sair do dispositivo.
    std::thread::sleep(Duration::from_millis(80));
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn prepara_frase_a_frase() {
        let f = preparar("Rodando o projeto loja-web. Parei às 14:30!");
        assert_eq!(f.len(), 2);
        assert!(f[0].starts_with("xodˈɐ̃dʊ"), "{}", f[0]);
        assert!(f[1].contains("katˈoɾzi"), "{}", f[1]);
        assert!(preparar("...").is_empty());
    }
}
