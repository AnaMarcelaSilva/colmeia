//! O som: o microfone pelo cpal, já convertido para o que o Whisper e o
//! reconhecimento do nome esperam (16 kHz, um canal), e a saída de som da
//! voz da abelha. O áudio do microfone fica só na memória deste processo e
//! vai embora quando a transcrição termina; nada é gravado em disco.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

pub const TAXA_WHISPER: u32 = 16_000;
/// Uma fala mais longa que isto para sozinha (2 min a 16 kHz são 7,7 MB).
pub const LIMITE_SEGUNDOS: u32 = 120;

pub const SEM_MICROFONE: &str = "Não achei um microfone. Confira se ele está ligado e liberado para a Colmeia nas configurações de som.";
pub const SEM_SAIDA: &str = "Não achei uma saída de som para a voz da abelha.";

/// O microfone aberto: a cada ~100 ms, o callback recebe um bloco a 16 kHz
/// em um canal. Fechar é soltar o `Microfone`.
pub struct Microfone {
    _fluxo: cpal::Stream,
}

impl Microfone {
    /// Abre o microfone padrão. `bloco` recebe o áudio; `falha`, um erro do
    /// dispositivo (desligado no meio, por exemplo).
    pub fn abrir(bloco: impl FnMut(Vec<f32>) + Send + 'static, falha: impl FnMut(String) + Send + 'static) -> Result<Microfone, String> {
        let dispositivo = microfone().ok_or(SEM_MICROFONE)?;
        let config = dispositivo.default_input_config().map_err(|e| format!("{SEM_MICROFONE} ({e})"))?;
        let (canais, taxa, formato) = (config.channels(), config.sample_rate(), config.sample_format());
        let quadros = quadros_por_vez(config.buffer_size(), taxa);
        let mut config: cpal::StreamConfig = config.into();
        // Blocos de ~100 ms: o padrão do ALSA acorda o processo centenas de
        // vezes por segundo, e o do PulseAudio entrega o som de 2 em 2 s (o
        // fim da fala e o nome seriam percebidos tarde).
        if let Some(n) = quadros {
            config.buffer_size = cpal::BufferSize::Fixed(n);
        }
        let conversor = Conversor { canais, reamostrador: Reamostrador::novo(taxa, TAXA_WHISPER), bloco };
        let fluxo = match formato {
            SampleFormat::I16 => abrir_entrada::<i16, _>(&dispositivo, config, conversor, falha),
            SampleFormat::I32 => abrir_entrada::<i32, _>(&dispositivo, config, conversor, falha),
            SampleFormat::U16 => abrir_entrada::<u16, _>(&dispositivo, config, conversor, falha),
            SampleFormat::F32 => abrir_entrada::<f32, _>(&dispositivo, config, conversor, falha),
            outro => return Err(format!("O microfone usa um formato que a Colmeia não lê ({outro}).")),
        }
        .map_err(|e| format!("Não consegui abrir o microfone: {e}"))?;
        fluxo.play().map_err(|e| format!("Não consegui abrir o microfone: {e}"))?;
        Ok(Microfone { _fluxo: fluxo })
    }
}

/// O que roda no callback do microfone: junta os canais e muda a taxa.
struct Conversor<B> {
    canais: u16,
    reamostrador: Reamostrador,
    bloco: B,
}

/// Uma ligação com o PulseAudio a cada abertura (o microfone e a saída
/// abrem poucas vezes; nada fica ligado ao servidor de som entre elas).
#[cfg(target_os = "linux")]
fn pulse() -> Option<cpal::Host> {
    cpal::host_from_id(cpal::HostId::PulseAudio).ok()
}

/// O microfone padrão. No Linux, pelo PulseAudio quando ele responde (e a
/// fonte de PULSE_SOURCE, se houver, como nos programas dele); senão, o padrão
/// do sistema (ALSA, ou WASAPI no Windows).
fn microfone() -> Option<cpal::Device> {
    #[cfg(target_os = "linux")]
    if let Some(d) = pulse().and_then(|host| {
        let escolhida = std::env::var("PULSE_SOURCE").ok().and_then(|nome| host.input_devices().ok()?.find(|d| d.id().is_ok_and(|id| id.id() == nome)));
        escolhida.or_else(|| host.default_input_device())
    }) {
        return Some(d);
    }
    cpal::default_host().default_input_device()
}

/// A saída de som padrão (no Linux, pelo PulseAudio; PULSE_SINK escolhe outra).
fn saida() -> Option<cpal::Device> {
    #[cfg(target_os = "linux")]
    if let Some(d) = pulse().and_then(|host| {
        let escolhida = std::env::var("PULSE_SINK").ok().and_then(|nome| host.output_devices().ok()?.find(|d| d.id().is_ok_and(|id| id.id() == nome)));
        escolhida.or_else(|| host.default_output_device())
    }) {
        return Some(d);
    }
    cpal::default_host().default_output_device()
}

fn abrir_entrada<T, B>(
    dispositivo: &cpal::Device,
    config: cpal::StreamConfig,
    mut conversor: Conversor<B>,
    mut falha: impl FnMut(String) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
    B: FnMut(Vec<f32>) + Send + 'static,
{
    dispositivo.build_input_stream(
        config,
        move |dados: &[T], _: &_| {
            let bruto: Vec<f32> = dados.iter().map(|s| s.to_sample::<f32>()).collect();
            let saida = conversor.reamostrador.processar(&mono(&bruto, conversor.canais));
            if !saida.is_empty() {
                (conversor.bloco)(saida);
            }
        },
        move |e| falha(format!("O microfone parou: {e}")),
        None,
    )
}

/// Quantos quadros por bloco pedir (~100 ms), dentro do que o dispositivo aceita.
fn quadros_por_vez(aceito: &cpal::SupportedBufferSize, taxa: u32) -> Option<u32> {
    match aceito {
        cpal::SupportedBufferSize::Range { min, max } => Some((taxa / 10).clamp(*min, *max)),
        cpal::SupportedBufferSize::Unknown => None,
    }
}

/// Junta os canais e muda a taxa para 16 kHz (uma gravação inteira).
pub fn para_whisper(intercalado: &[f32], canais: u16, taxa: u32) -> Vec<f32> {
    reamostrar(&mono(intercalado, canais), taxa, TAXA_WHISPER)
}

fn mono(intercalado: &[f32], canais: u16) -> Vec<f32> {
    let n = canais.max(1) as usize;
    if n == 1 {
        return intercalado.to_vec();
    }
    intercalado.chunks_exact(n).map(|quadro| quadro.iter().sum::<f32>() / n as f32).collect()
}

/// Muda a taxa de uma gravação inteira.
pub fn reamostrar(entrada: &[f32], de: u32, para: u32) -> Vec<f32> {
    let mut r = Reamostrador::novo(de, para);
    r.processar(entrada)
}

/// Muda a taxa de um fluxo, bloco a bloco, sem emendas entre os blocos.
/// Para baixo, cada amostra nova é a média da janela que ela cobre (um filtro
/// simples contra o serrilhado); para cima, interpolação linear. As posições
/// são contas inteiras: em blocos ou de uma vez, o resultado é o mesmo.
pub struct Reamostrador {
    de: u64,
    para: u64,
    /// Quantas amostras de saída já saíram.
    feitas: u64,
    /// A posição (na entrada) da primeira amostra guardada.
    base: u64,
    guardado: Vec<f32>,
}

impl Reamostrador {
    pub fn novo(de: u32, para: u32) -> Reamostrador {
        let (de, para) = if de == 0 || para == 0 { (1, 1) } else { (de as u64, para as u64) };
        Reamostrador { de, para, feitas: 0, base: 0, guardado: Vec::new() }
    }

    pub fn processar(&mut self, entrada: &[f32]) -> Vec<f32> {
        if self.de == self.para {
            return entrada.to_vec();
        }
        self.guardado.extend_from_slice(entrada);
        let fim = self.base + self.guardado.len() as u64;
        let mut saida = Vec::with_capacity((entrada.len() as u64 * self.para / self.de) as usize + 1);
        loop {
            let inicio = self.feitas * self.de / self.para;
            if self.de > self.para {
                let termino = ((self.feitas + 1) * self.de).div_ceil(self.para);
                if termino > fim {
                    break;
                }
                let janela = &self.guardado[(inicio - self.base) as usize..(termino - self.base) as usize];
                saida.push(janela.iter().sum::<f32>() / janela.len() as f32);
            } else {
                if inicio + 1 >= fim {
                    break;
                }
                let t = ((self.feitas * self.de) % self.para) as f32 / self.para as f32;
                let a = (inicio - self.base) as usize;
                saida.push(self.guardado[a] * (1.0 - t) + self.guardado[a + 1] * t);
            }
            self.feitas += 1;
        }
        // Guarda só o que ainda vai ser usado.
        let proxima = (self.feitas * self.de / self.para).min(fim);
        self.guardado.drain(..(proxima - self.base) as usize);
        self.base = proxima;
        saida
    }
}

/// Depois de soltar um modelo, devolve ao sistema a memória livre (no
/// Linux, o malloc guarda as páginas para reusar e o processo continuaria
/// com a memória do modelo que já saiu).
pub fn devolver_memoria() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim só olha as listas de memória livre do próprio malloc.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Curta demais para ser uma fala (menos de 0,3 s).
pub fn curta(audio: &[f32]) -> bool {
    audio.len() < TAXA_WHISPER as usize * 3 / 10
}

/// Só silêncio: o microfone mudo ou desligado manda zeros ou quase. Sem
/// isso, o Whisper inventa uma frase a partir do nada.
pub fn silencio(audio: &[f32]) -> bool {
    let pico = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    pico < 0.01
}

/// A saída de som da voz da abelha: o que entra numa fila (a 24 kHz) toca
/// na ordem; parado, o callback só escreve silêncio.
pub struct Tocador {
    _fluxo: cpal::Stream,
    saida: Arc<Mutex<Saida>>,
    taxa: u32,
    reamostrador: Reamostrador,
}

/// O que o callback da saída divide com quem fala: a fila e quando o que já
/// saiu dela vai terminar de tocar (o servidor de som guarda um bom pedaço
/// antes de tocar; soltar o fluxo antes disso cortaria o fim da fala).
#[derive(Default)]
struct Saida {
    fila: VecDeque<f32>,
    /// Quando a primeira amostra de verdade toca (o callback avisa a demora).
    primeira: Option<std::time::Instant>,
    /// Quantas amostras (por canal) saíram desde a primeira, contando o silêncio.
    posicao: u64,
    /// A posição da última amostra de verdade.
    ultima: u64,
}

impl Tocador {
    pub fn abrir(taxa_entrada: u32) -> Result<Tocador, String> {
        let dispositivo = saida().ok_or(SEM_SAIDA)?;
        let config = dispositivo.default_output_config().map_err(|e| format!("{SEM_SAIDA} ({e})"))?;
        let (canais, taxa, formato) = (config.channels(), config.sample_rate(), config.sample_format());
        let quadros = quadros_por_vez(config.buffer_size(), taxa / 2);
        let saida = Arc::new(Mutex::new(Saida::default()));
        let mut config: cpal::StreamConfig = config.into();
        // Blocos de ~50 ms: sem pedir, o PulseAudio guarda até 2 s antes de
        // tocar e a abelha falaria bem depois do balão.
        if let Some(n) = quadros {
            config.buffer_size = cpal::BufferSize::Fixed(n);
        }
        let fluxo = match formato {
            SampleFormat::I16 => abrir_saida::<i16>(&dispositivo, config, canais, &saida),
            SampleFormat::I32 => abrir_saida::<i32>(&dispositivo, config, canais, &saida),
            SampleFormat::U16 => abrir_saida::<u16>(&dispositivo, config, canais, &saida),
            SampleFormat::F32 => abrir_saida::<f32>(&dispositivo, config, canais, &saida),
            outro => return Err(format!("A saída de som usa um formato que a Colmeia não toca ({outro}).")),
        }
        .map_err(|e| format!("Não consegui abrir a saída de som: {e}"))?;
        fluxo.play().map_err(|e| format!("Não consegui abrir a saída de som: {e}"))?;
        Ok(Tocador { _fluxo: fluxo, saida, taxa, reamostrador: Reamostrador::novo(taxa_entrada, taxa) })
    }

    /// Põe um trecho na fila (na taxa dada ao abrir).
    pub fn somar(&mut self, onda: &[f32]) {
        let pronto = self.reamostrador.processar(onda);
        if let Ok(mut s) = self.saida.lock() {
            s.fila.extend(pronto);
        }
    }

    /// Quanto falta para terminar de tocar o que foi posto (zero: já tocou).
    pub fn falta(&self) -> std::time::Duration {
        let Ok(s) = self.saida.lock() else { return std::time::Duration::ZERO };
        let na_fila = std::time::Duration::from_secs_f64(s.fila.len() as f64 / self.taxa as f64);
        let Some(primeira) = s.primeira else { return na_fila };
        let fim = primeira + std::time::Duration::from_secs_f64(s.ultima as f64 / self.taxa as f64);
        na_fila + fim.saturating_duration_since(std::time::Instant::now())
    }

    /// Para na hora: o que estava na fila não toca.
    pub fn calar(&self) {
        if let Ok(mut s) = self.saida.lock() {
            s.fila.clear();
            s.primeira = None;
        }
    }
}

fn abrir_saida<T>(dispositivo: &cpal::Device, config: cpal::StreamConfig, canais: u16, saida: &Arc<Mutex<Saida>>) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let saida = Arc::clone(saida);
    let canais = canais.max(1) as usize;
    dispositivo.build_output_stream(
        config,
        move |dados: &mut [T], info: &cpal::OutputCallbackInfo| {
            let Ok(mut s) = saida.lock() else { return };
            let t = info.timestamp();
            let demora = t.playback.duration_since(t.callback);
            for quadro in dados.chunks_mut(canais) {
                let amostra = s.fila.pop_front();
                if amostra.is_some() && s.primeira.is_none() {
                    s.primeira = Some(std::time::Instant::now() + demora);
                    s.posicao = 0;
                }
                if s.primeira.is_some() {
                    s.posicao += 1;
                    if amostra.is_some() {
                        s.ultima = s.posicao;
                    }
                }
                let v = amostra.unwrap_or(0.0);
                quadro.iter_mut().for_each(|x| *x = T::from_sample(v));
            }
        },
        |e| eprintln!("colmeia-voz: saída de som: {e}"),
        None,
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    fn seno(freq: f32, taxa: u32, segundos: f32) -> Vec<f32> {
        (0..(taxa as f32 * segundos) as usize).map(|i| (i as f32 * freq * std::f32::consts::TAU / taxa as f32).sin() * 0.5).collect()
    }

    #[test]
    fn junta_os_canais_pela_media() {
        assert_eq!(mono(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2), vec![0.5, 0.5, 0.0]);
        assert_eq!(mono(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn de_48_para_16_khz_fica_com_um_terco_e_mantem_o_nivel() {
        let constante = vec![0.25; 48_000];
        let saida = reamostrar(&constante, 48_000, 16_000);
        assert_eq!(saida.len(), 16_000);
        assert!(saida.iter().all(|s| (s - 0.25).abs() < 1e-6));
        // Uma voz (300 Hz) passa quase inteira; a energia fica parecida.
        let voz = reamostrar(&seno(300.0, 48_000, 1.0), 48_000, 16_000);
        let energia = |v: &[f32]| v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32;
        assert!((energia(&voz) - energia(&seno(300.0, 16_000, 1.0))).abs() < 0.01, "{}", energia(&voz));
    }

    #[test]
    fn de_44_1_khz_e_para_cima_tambem() {
        assert_eq!(reamostrar(&vec![0.0; 44_100], 44_100, 16_000).len(), 16_000);
        let subida = reamostrar(&[0.0, 1.0, 1.0], 8_000, 16_000);
        assert_eq!(subida, vec![0.0, 0.5, 1.0, 1.0]);
        assert_eq!(para_whisper(&[0.2; 3200], 2, 16_000).len(), 1600);
    }

    #[test]
    fn em_blocos_da_o_mesmo_que_de_uma_vez() {
        for (de, para) in [(48_000, 16_000), (44_100, 16_000), (24_000, 48_000), (24_000, 44_100)] {
            let sinal = seno(440.0, de, 0.5);
            let inteiro = reamostrar(&sinal, de, para);
            let mut r = Reamostrador::novo(de, para);
            let mut blocos = Vec::new();
            for pedaco in sinal.chunks(1234) {
                blocos.extend(r.processar(pedaco));
            }
            assert!((inteiro.len() as i64 - blocos.len() as i64).abs() <= 1, "{de}->{para}: {} {}", inteiro.len(), blocos.len());
            let diferenca = inteiro.iter().zip(&blocos).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(diferenca < 1e-4, "{de}->{para}: {diferenca}");
        }
    }

    #[test]
    fn reconhece_silencio_e_fala_curta() {
        assert!(silencio(&vec![0.0; 16_000]));
        assert!(silencio(&vec![0.005; 16_000]));
        assert!(!silencio(&seno(200.0, 16_000, 1.0)));
        assert!(curta(&vec![0.1; 4000]));
        assert!(!curta(&vec![0.1; 8000]));
    }
}
