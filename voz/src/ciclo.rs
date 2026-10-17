//! A vida do processo: espera pedidos da tela, ouve (um ditado, um comando
//! que termina na pausa, uma gravação do nome ou o próprio nome), transcreve,
//! fala com a voz da abelha e sai depois de um tempo parado (ou quando a tela
//! fecha a entrada). Ouvindo o nome, o processo fica, mas os modelos saem da
//! memória quando ficam parados.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::audio::{self, Microfone};
use crate::fala::{self, Aviso, Voz};
use crate::nome::{self, Detector, Referencias};
use crate::pausa::{Pausa, Situacao};
use crate::protocolo::{self, Comando, Evento};
use crate::transcrever::{self, Transcritor};

/// Sem pedidos por este tempo, o processo sai e o modelo sai da memória.
pub const PARADO_PADRAO: Duration = Duration::from_secs(120);
/// Depois da voz da abelha, um instante sem ouvir o nome (o eco dela).
const SURDO_DEPOIS_DE_FALAR: Duration = Duration::from_millis(600);
/// Ouvindo o nome, o último segundo fica guardado (na memória): o comando
/// dito de uma vez com o nome não perde o começo se o nome for percebido tarde.
const ANTES_DO_NOME: usize = audio::TAXA_WHISPER as usize;

#[derive(Debug, PartialEq)]
pub struct Opcoes {
    pub modelo: PathBuf,
    pub fala: Option<fala::Arquivos>,
    pub parado: Duration,
}

impl Opcoes {
    /// `--modelo <arquivo>` e, opcionais, `--parado-ms <n>` (de 100 ms a 1 h)
    /// e a voz da abelha: `--fala-modelo`, `--fala-voz` e `--fala-runtime`, juntos.
    pub fn ler(mut args: impl Iterator<Item = String>) -> Result<Opcoes, String> {
        let (mut modelo, mut parado) = (None, PARADO_PADRAO);
        let (mut fm, mut fv, mut fr) = (None, None, None);
        while let Some(a) = args.next() {
            let mut valor = |nome: &str| args.next().map(PathBuf::from).ok_or(format!("falta o arquivo depois de {nome}"));
            match a.as_str() {
                "--modelo" => modelo = Some(valor("--modelo")?),
                "--fala-modelo" => fm = Some(valor("--fala-modelo")?),
                "--fala-voz" => fv = Some(valor("--fala-voz")?),
                "--fala-runtime" => fr = Some(valor("--fala-runtime")?),
                "--parado-ms" => {
                    let ms: u64 = args.next().and_then(|v| v.parse().ok()).ok_or("--parado-ms precisa de um número")?;
                    parado = Duration::from_millis(ms.clamp(100, 3_600_000));
                }
                outro => return Err(format!("opção desconhecida: {outro}")),
            }
        }
        let fala = match (fm, fv, fr) {
            (Some(modelo), Some(voz), Some(runtime)) => Some(fala::Arquivos { modelo, voz, runtime }),
            (None, None, None) => None,
            _ => return Err("a voz da abelha precisa de --fala-modelo, --fala-voz e --fala-runtime juntos".into()),
        };
        Ok(Opcoes { modelo: modelo.ok_or("falta --modelo")?, fala, parado })
    }
}

/// O Whisper carrega numa thread quando o microfone abre: quando a fala
/// termina, ele já está pronto.
enum Modelo {
    Carregando(JoinHandle<Result<Transcritor, String>>),
    Pronto(Transcritor),
    Falhou(String),
}

impl Modelo {
    fn carregar(caminho: PathBuf) -> Modelo {
        Modelo::Carregando(std::thread::spawn(move || transcrever::carregar(&caminho)))
    }

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

/// O que chega ao laço principal: os pedidos, o microfone e a voz.
enum Entrada {
    Pedido(Result<Comando, String>),
    Audio(Vec<f32>),
    MicrofoneFalhou(String),
    Voz(Aviso),
}

/// O que o microfone está fazendo agora.
enum Escuta {
    Nada,
    /// O ditado: até a tela pedir "parar" (ou o limite).
    Ditado(Vec<f32>),
    /// Um comando: até a pausa depois da fala.
    Comando(Vec<f32>, Pausa),
    /// Uma gravação do nome: até a pausa.
    Amostra(Vec<f32>, Pausa),
}

fn emitir(evento: &Evento) -> bool {
    let mut saida = std::io::stdout().lock();
    writeln!(saida, "{}", protocolo::linha(evento)).and_then(|_| saida.flush()).is_ok()
}

/// Lê a entrada numa thread; o fim da entrada (a tela fechou) vira `Sair`.
fn ler_entrada(envio: Sender<Entrada>) {
    std::thread::spawn(move || {
        for linha in std::io::stdin().lock().lines() {
            let Ok(linha) = linha else { break };
            if envio.send(Entrada::Pedido(protocolo::ler(&linha))).is_err() {
                return;
            }
        }
        let _ = envio.send(Entrada::Pedido(Ok(Comando::Sair)));
    });
}

/// Metade dos núcleos, de 1 a 4: falar não pode travar o resto do computador.
fn linhas() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).div_ceil(2).clamp(1, 4)
}

/// Um comando falado: 0,9 s de pausa encerra; sem fala em 5 s, desiste; 12 s no máximo.
fn pausa_de_comando() -> Pausa {
    Pausa::nova(900, 5000, 12_000)
}

struct Processo {
    opcoes: Opcoes,
    envio: Sender<Entrada>,
    microfone: Option<Microfone>,
    escuta: Escuta,
    vigia: Option<Detector>,
    /// O último segundo ouvido enquanto espera o nome.
    antes: std::collections::VecDeque<f32>,
    amostras: Vec<Vec<f32>>,
    whisper: Option<Modelo>,
    voz: Option<Voz>,
    falando: bool,
    surdo_ate: Instant,
    ultimo_uso: Instant,
}

pub fn rodar(opcoes: Opcoes) -> ExitCode {
    // Sem isto o whisper.cpp escreve o carregamento inteiro na saída de erro.
    whisper_rs::install_logging_hooks();
    let (envio, recebe) = mpsc::channel();
    ler_entrada(envio.clone());
    if !emitir(&Evento::Iniciado) {
        return ExitCode::FAILURE;
    }
    let agora = Instant::now();
    let mut p = Processo {
        opcoes,
        envio,
        microfone: None,
        escuta: Escuta::Nada,
        vigia: None,
        antes: std::collections::VecDeque::with_capacity(ANTES_DO_NOME),
        amostras: Vec::new(),
        whisper: None,
        voz: None,
        falando: false,
        surdo_ate: agora,
        ultimo_uso: agora,
    };
    p.laco(recebe)
}

impl Processo {
    fn laco(&mut self, recebe: Receiver<Entrada>) -> ExitCode {
        loop {
            let espera = match self.escuta {
                // Ouvindo, olha o limite do ditado de tempos em tempos.
                Escuta::Ditado(_) => Duration::from_millis(500),
                _ => self.opcoes.parado.saturating_sub(self.ultimo_uso.elapsed()).max(Duration::from_millis(50)),
            };
            let continua = match recebe.recv_timeout(espera) {
                Ok(Entrada::Pedido(Ok(Comando::Sair))) | Err(RecvTimeoutError::Disconnected) => return ExitCode::SUCCESS,
                Ok(Entrada::Pedido(Ok(c))) => {
                    self.ultimo_uso = Instant::now();
                    self.atender(c)
                }
                Ok(Entrada::Pedido(Err(mensagem))) => emitir(&Evento::Erro { mensagem }),
                Ok(Entrada::Audio(bloco)) => self.ouvir_bloco(bloco),
                Ok(Entrada::MicrofoneFalhou(mensagem)) => self.microfone_falhou(mensagem),
                Ok(Entrada::Voz(aviso)) => self.aviso_da_voz(aviso),
                Err(RecvTimeoutError::Timeout) => match self.parado() {
                    Some(codigo) => return codigo,
                    None => true,
                },
            };
            if !continua {
                return ExitCode::FAILURE;
            }
            if let Err(mensagem) = self.ajustar_microfone() {
                self.escuta = Escuta::Nada;
                let estava = self.vigia.take().is_some();
                if !emitir(&Evento::Erro { mensagem }) || estava && !emitir(&Evento::Vigiando { ligado: false }) {
                    return ExitCode::FAILURE;
                }
            }
        }
    }

    /// O tempo parado passou: com o ditado no limite, transcreve; sem nada
    /// ouvindo, sai; ouvindo o nome (ou falando), só solta os modelos.
    fn parado(&mut self) -> Option<ExitCode> {
        if let Escuta::Ditado(audio) = &self.escuta {
            if audio.len() >= audio::TAXA_WHISPER as usize * audio::LIMITE_SEGUNDOS as usize {
                let Escuta::Ditado(audio) = std::mem::replace(&mut self.escuta, Escuta::Nada) else { unreachable!() };
                return (!self.transcrever(audio)).then_some(ExitCode::FAILURE);
            }
            return None;
        }
        if self.ultimo_uso.elapsed() < self.opcoes.parado || !matches!(self.escuta, Escuta::Nada) {
            return None;
        }
        if self.vigia.is_some() || self.falando {
            if self.whisper.take().is_some() {
                audio::devolver_memoria();
            }
            self.ultimo_uso = Instant::now();
            return None;
        }
        emitir(&Evento::Saindo { motivo: "parado" });
        Some(ExitCode::SUCCESS)
    }

    /// O microfone fica aberto enquanto algo ouve: o ditado, um comando, uma
    /// gravação do nome ou o próprio nome. Fora disso, fecha.
    fn ajustar_microfone(&mut self) -> Result<(), String> {
        let precisa = !matches!(self.escuta, Escuta::Nada) || self.vigia.is_some();
        match (precisa, self.microfone.is_some()) {
            (true, false) => {
                let (audio, falha) = (self.envio.clone(), self.envio.clone());
                self.microfone = Some(Microfone::abrir(
                    move |b| {
                        let _ = audio.send(Entrada::Audio(b));
                    },
                    move |e| {
                        let _ = falha.send(Entrada::MicrofoneFalhou(e));
                    },
                )?);
            }
            (false, true) => self.microfone = None,
            _ => {}
        }
        Ok(())
    }

    fn garantir_whisper(&mut self) {
        if self.whisper.is_none() {
            self.whisper = Some(Modelo::carregar(self.opcoes.modelo.clone()));
        }
    }

    /// Abre o microfone na hora (para o "ouvindo" sair só com ele aberto).
    fn comecar(&mut self, escuta: Escuta) -> bool {
        self.escuta = escuta;
        match self.ajustar_microfone() {
            Ok(()) => emitir(&Evento::Ouvindo),
            Err(mensagem) => {
                self.escuta = Escuta::Nada;
                emitir(&Evento::Erro { mensagem })
            }
        }
    }

    fn atender(&mut self, c: Comando) -> bool {
        match c {
            Comando::Ouvir { .. } | Comando::Amostra if !matches!(self.escuta, Escuta::Nada) => true,
            Comando::Ouvir { ate_silencio } => {
                self.garantir_whisper();
                if let Some(v) = &self.voz {
                    v.calar();
                }
                self.comecar(if ate_silencio { Escuta::Comando(Vec::new(), pausa_de_comando()) } else { Escuta::Ditado(Vec::new()) })
            }
            Comando::Amostra => self.comecar(Escuta::Amostra(Vec::new(), Pausa::nova(500, 4000, 3000))),
            // O áudio vai embora junto com a gravação.
            Comando::Cancelar => {
                self.escuta = Escuta::Nada;
                true
            }
            Comando::Parar => match std::mem::replace(&mut self.escuta, Escuta::Nada) {
                Escuta::Ditado(audio) | Escuta::Comando(audio, _) => self.transcrever(audio),
                Escuta::Amostra(audio, _) => self.guardar_amostra(audio),
                Escuta::Nada => emitir(&Evento::Erro { mensagem: "O microfone não estava aberto.".into() }),
            },
            Comando::Falar { texto, efeito } => {
                let Some(arquivos) = self.opcoes.fala.clone() else {
                    return emitir(&Evento::Erro { mensagem: "A voz da abelha não está ligada.".into() });
                };
                let envio = self.envio.clone();
                let parado = self.opcoes.parado;
                self.voz
                    .get_or_insert_with(|| {
                        Voz::nova(arquivos, linhas(), parado, move |a| {
                            let _ = envio.send(Entrada::Voz(a));
                        })
                    })
                    .falar(&texto, efeito);
                true
            }
            Comando::Calar => {
                if let Some(v) = &self.voz {
                    v.calar();
                }
                true
            }
            Comando::Vigiar { referencias, sensibilidade } => {
                let r = match referencias {
                    Some(caminho) => Referencias::ler(&caminho),
                    None => Ok(Referencias::padrao()),
                };
                match r {
                    Ok(r) => {
                        self.vigia = Some(Detector::novo(&r, sensibilidade));
                        match self.ajustar_microfone() {
                            Ok(()) => emitir(&Evento::Vigiando { ligado: true }),
                            Err(mensagem) => {
                                self.vigia = None;
                                emitir(&Evento::Erro { mensagem }) && emitir(&Evento::Vigiando { ligado: false })
                            }
                        }
                    }
                    Err(mensagem) => emitir(&Evento::Erro { mensagem }),
                }
            }
            Comando::Desvigiar => {
                self.vigia = None;
                self.antes.clear();
                emitir(&Evento::Vigiando { ligado: false })
            }
            Comando::Aprender { nome, destino } => {
                let feito = Referencias::de_gravacoes(&nome, &self.amostras).and_then(|r| r.gravar(&destino));
                match feito {
                    Ok(()) => {
                        self.amostras.clear();
                        emitir(&Evento::Aprendido)
                    }
                    Err(mensagem) => emitir(&Evento::Erro { mensagem }),
                }
            }
            Comando::EsquecerAmostras => {
                self.amostras.clear();
                true
            }
            Comando::Sair => true,
        }
    }

    fn ouvir_bloco(&mut self, bloco: Vec<f32>) -> bool {
        match &mut self.escuta {
            Escuta::Ditado(audio) => {
                let limite = audio::TAXA_WHISPER as usize * audio::LIMITE_SEGUNDOS as usize;
                let cabe = limite.saturating_sub(audio.len()).min(bloco.len());
                audio.extend_from_slice(&bloco[..cabe]);
                true
            }
            Escuta::Comando(audio, pausa) | Escuta::Amostra(audio, pausa) => {
                audio.extend_from_slice(&bloco);
                match pausa.alimentar(&bloco) {
                    Situacao::Esperando | Situacao::Falando => true,
                    Situacao::NadaOuvido => {
                        let era_nome = matches!(self.escuta, Escuta::Amostra(..));
                        self.escuta = Escuta::Nada;
                        let mensagem = if era_nome {
                            "Não ouvi o nome. Fale perto do microfone e tente de novo."
                        } else {
                            "Não ouvi o pedido. Fale logo depois de abrir o microfone."
                        };
                        emitir(&Evento::Erro { mensagem: mensagem.into() })
                    }
                    Situacao::Terminou | Situacao::Limite => match std::mem::replace(&mut self.escuta, Escuta::Nada) {
                        Escuta::Amostra(audio, _) => self.guardar_amostra(audio),
                        Escuta::Comando(audio, _) => self.transcrever(audio),
                        _ => true,
                    },
                }
            }
            Escuta::Nada => {
                // Ouvindo o nome: nada enquanto a própria abelha fala.
                if self.falando || Instant::now() < self.surdo_ate {
                    return true;
                }
                let Some(d) = self.vigia.as_mut() else { return true };
                let Some(onde) = d.alimentar(&bloco) else {
                    guardar_antes(&mut self.antes, &bloco);
                    return true;
                };
                self.ultimo_uso = Instant::now();
                self.garantir_whisper();
                // A fala é o último segundo antes e o bloco inteiro (o núcleo
                // tira o nome do começo); a pausa conta só o que veio depois do nome.
                let mut fala: Vec<f32> = self.antes.drain(..).collect();
                fala.extend_from_slice(&bloco);
                let mut pausa = pausa_de_comando();
                pausa.alimentar(&bloco[onde..]);
                self.escuta = Escuta::Comando(fala, pausa);
                emitir(&Evento::Chamado)
            }
        }
    }

    fn guardar_amostra(&mut self, audio: Vec<f32>) -> bool {
        if audio::curta(&audio) || audio::silencio(&audio) {
            return emitir(&Evento::Erro { mensagem: "Não ouvi o nome. Fale perto do microfone e tente de novo.".into() });
        }
        if self.amostras.len() == nome::MAXIMO_GRAVACOES {
            self.amostras.remove(0);
        }
        let segundos = audio.len() as f32 / audio::TAXA_WHISPER as f32;
        self.amostras.push(audio);
        emitir(&Evento::Amostra { numero: self.amostras.len(), segundos })
    }

    fn microfone_falhou(&mut self, mensagem: String) -> bool {
        self.microfone = None;
        self.escuta = Escuta::Nada;
        let estava = self.vigia.take().is_some();
        emitir(&Evento::Erro { mensagem }) && (!estava || emitir(&Evento::Vigiando { ligado: false }))
    }

    fn aviso_da_voz(&mut self, aviso: Aviso) -> bool {
        self.ultimo_uso = Instant::now();
        let evento = match aviso {
            Aviso::Falando => {
                self.falando = true;
                Evento::Falando
            }
            Aviso::Falou => Evento::Falou,
            Aviso::Erro(mensagem) => Evento::Erro { mensagem },
        };
        if !matches!(evento, Evento::Falando) {
            self.falando = false;
            self.surdo_ate = Instant::now() + SURDO_DEPOIS_DE_FALAR;
            if let Some(d) = self.vigia.as_mut() {
                d.esquecer();
            }
        }
        emitir(&evento)
    }

    fn transcrever(&mut self, audio: Vec<f32>) -> bool {
        if audio::curta(&audio) {
            return emitir(&Evento::Erro { mensagem: "Foi rápido demais. Segure o botão (ou o atalho) enquanto fala.".into() });
        }
        if audio::silencio(&audio) {
            return emitir(&Evento::Erro { mensagem: "Não ouvi nada. Confira se o microfone está ligado e sem o mudo.".into() });
        }
        if !emitir(&Evento::Transcrevendo { segundos: audio.len() as f32 / audio::TAXA_WHISPER as f32 }) {
            return false;
        }
        self.garantir_whisper();
        let resultado = match self.whisper.as_mut() {
            Some(m) => m.pronto().and_then(|t| t.transcrever(&audio)),
            None => Err("O modelo de voz não abriu.".into()),
        };
        // Um modelo que falhou não fica: o próximo pedido tenta de novo.
        if resultado.is_err() {
            self.whisper = None;
        }
        self.ultimo_uso = Instant::now();
        emitir(&match resultado {
            Ok(texto) => Evento::Texto { texto },
            Err(mensagem) => Evento::Erro { mensagem },
        })
    }
}

/// Guarda o bloco no fim do último segundo (o mais antigo sai).
fn guardar_antes(antes: &mut std::collections::VecDeque<f32>, bloco: &[f32]) {
    antes.extend(bloco.iter().copied());
    let sobra = antes.len().saturating_sub(ANTES_DO_NOME);
    antes.drain(..sobra);
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn guarda_so_o_ultimo_segundo() {
        let mut antes = std::collections::VecDeque::new();
        guardar_antes(&mut antes, &vec![0.1; ANTES_DO_NOME - 100]);
        guardar_antes(&mut antes, &[0.5; 300]);
        assert_eq!(antes.len(), ANTES_DO_NOME);
        assert_eq!(antes.back(), Some(&0.5));
        assert_eq!(antes.iter().filter(|s| **s == 0.5).count(), 300);
    }

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn opcoes_da_linha_de_comando() {
        let o = Opcoes::ler(args(&["--modelo", "/m/ggml-base.bin"])).unwrap();
        assert_eq!(o, Opcoes { modelo: "/m/ggml-base.bin".into(), fala: None, parado: PARADO_PADRAO });
        let o = Opcoes::ler(args(&["--parado-ms", "5", "--modelo", "m"])).unwrap();
        assert_eq!(o.parado, Duration::from_millis(100));
        let o = Opcoes::ler(args(&["--modelo", "m", "--fala-modelo", "k.onnx", "--fala-voz", "v.bin", "--fala-runtime", "ort.so"])).unwrap();
        assert_eq!(o.fala, Some(fala::Arquivos { modelo: "k.onnx".into(), voz: "v.bin".into(), runtime: "ort.so".into() }));
        assert!(Opcoes::ler(args(&[])).is_err());
        assert!(Opcoes::ler(args(&["--modelo"])).is_err());
        assert!(Opcoes::ler(args(&["--modelo", "m", "--parado-ms", "x"])).is_err());
        assert!(Opcoes::ler(args(&["--modelo", "m", "--arquivo", "a.wav"])).is_err());
        assert!(Opcoes::ler(args(&["--modelo", "m", "--fala-modelo", "k.onnx"])).is_err());
    }

    #[test]
    fn usa_no_maximo_quatro_linhas() {
        assert!((1..=4).contains(&linhas()));
    }
}
