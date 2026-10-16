//! O microfone: a captura pelo cpal e a conversão para o que o Whisper
//! espera (16 kHz, um canal). O áudio fica só na memória deste processo e
//! some quando a transcrição termina; nada é gravado em disco.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

pub const TAXA_WHISPER: u32 = 16_000;
/// Uma fala mais longa que isto para sozinha (2 min a 16 kHz são 7,7 MB).
pub const LIMITE_SEGUNDOS: u32 = 120;

pub const SEM_MICROFONE: &str = "Não achei um microfone. Confira se ele está ligado e liberado para a Colmeia nas configurações de som.";

/// Uma gravação em andamento: o fluxo do microfone e o que ele já mandou.
pub struct Gravacao {
    fluxo: cpal::Stream,
    amostras: Arc<Mutex<Vec<f32>>>,
    erro: Arc<Mutex<Option<String>>>,
    canais: u16,
    taxa: u32,
}

/// Abre o microfone padrão do sistema e começa a guardar.
pub fn ouvir() -> Result<Gravacao, String> {
    let dispositivo = microfone().ok_or(SEM_MICROFONE)?;
    let config = dispositivo.default_input_config().map_err(|e| format!("{SEM_MICROFONE} ({e})"))?;
    let (canais, taxa) = (config.channels(), config.sample_rate());
    let limite = taxa as usize * canais as usize * LIMITE_SEGUNDOS as usize;
    let amostras = Arc::new(Mutex::new(Vec::with_capacity(taxa as usize * canais as usize * 10)));
    let erro = Arc::new(Mutex::new(None));
    let formato = config.sample_format();
    let quadros = quadros_por_vez(config.buffer_size(), taxa);
    let mut config: cpal::StreamConfig = config.into();
    // Blocos de ~100 ms: o padrão do sistema acorda o processo centenas de
    // vezes por segundo, e para ditado não precisa.
    if let Some(n) = quadros {
        config.buffer_size = cpal::BufferSize::Fixed(n);
    }
    let fluxo = match formato {
        SampleFormat::I16 => abrir::<i16>(&dispositivo, config, &amostras, &erro, limite),
        SampleFormat::I32 => abrir::<i32>(&dispositivo, config, &amostras, &erro, limite),
        SampleFormat::U16 => abrir::<u16>(&dispositivo, config, &amostras, &erro, limite),
        SampleFormat::F32 => abrir::<f32>(&dispositivo, config, &amostras, &erro, limite),
        outro => return Err(format!("O microfone usa um formato que a Colmeia não lê ({outro}).")),
    }
    .map_err(|e| format!("Não consegui abrir o microfone: {e}"))?;
    fluxo.play().map_err(|e| format!("Não consegui abrir o microfone: {e}"))?;
    Ok(Gravacao { fluxo, amostras, erro, canais, taxa })
}

/// O microfone padrão. No Linux, pelo PulseAudio quando ele responde (e a
/// fonte de PULSE_SOURCE, se houver, como nos programas dele); senão, o padrão
/// do sistema (ALSA, ou WASAPI no Windows).
fn microfone() -> Option<cpal::Device> {
    #[cfg(target_os = "linux")]
    if let Ok(host) = cpal::host_from_id(cpal::HostId::PulseAudio) {
        let escolhida = std::env::var("PULSE_SOURCE").ok().and_then(|nome| host.input_devices().ok()?.find(|d| d.id().is_ok_and(|id| id.id() == nome)));
        if let Some(d) = escolhida.or_else(|| host.default_input_device()) {
            return Some(d);
        }
    }
    cpal::default_host().default_input_device()
}

fn abrir<T>(
    dispositivo: &cpal::Device,
    config: cpal::StreamConfig,
    amostras: &Arc<Mutex<Vec<f32>>>,
    erro: &Arc<Mutex<Option<String>>>,
    limite: usize,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let destino = Arc::clone(amostras);
    let falha = Arc::clone(erro);
    dispositivo.build_input_stream(
        config,
        move |dados: &[T], _: &_| {
            if let Ok(mut v) = destino.lock() {
                let cabe = limite.saturating_sub(v.len()).min(dados.len());
                v.extend(dados[..cabe].iter().map(|s| s.to_sample::<f32>()));
            }
        },
        move |e| {
            if let Ok(mut f) = falha.lock() {
                f.get_or_insert_with(|| format!("O microfone parou: {e}"));
            }
        },
        None,
    )
}

impl Gravacao {
    /// Fecha o microfone e devolve o áudio pronto para o Whisper.
    pub fn terminar(self) -> Result<Vec<f32>, String> {
        let Gravacao { fluxo, amostras, erro, canais, taxa } = self;
        drop(fluxo);
        if let Some(e) = erro.lock().ok().and_then(|mut e| e.take()) {
            return Err(e);
        }
        let bruto = std::mem::take(&mut *amostras.lock().map_err(|_| "O microfone parou.".to_string())?);
        Ok(para_whisper(&bruto, canais, taxa))
    }
}

/// Quantos quadros por bloco pedir (~100 ms), dentro do que o dispositivo aceita.
fn quadros_por_vez(aceito: &cpal::SupportedBufferSize, taxa: u32) -> Option<u32> {
    match aceito {
        cpal::SupportedBufferSize::Range { min, max } => Some((taxa / 10).clamp(*min, *max)),
        cpal::SupportedBufferSize::Unknown => None,
    }
}

/// Junta os canais e muda a taxa para 16 kHz.
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

/// Para baixo, cada amostra nova é a média da janela que ela cobre (um
/// filtro simples contra o serrilhado); para cima, interpolação linear.
fn reamostrar(entrada: &[f32], de: u32, para: u32) -> Vec<f32> {
    if de == para || entrada.is_empty() || de == 0 {
        return entrada.to_vec();
    }
    let passo = de as f64 / para as f64;
    let total = (entrada.len() as f64 / passo).floor() as usize;
    (0..total)
        .map(|i| {
            let inicio = i as f64 * passo;
            if passo >= 1.0 {
                let (a, b) = (inicio.floor() as usize, ((inicio + passo).ceil() as usize).min(entrada.len()));
                let janela = &entrada[a..b.max(a + 1)];
                janela.iter().sum::<f32>() / janela.len() as f32
            } else {
                let a = inicio.floor() as usize;
                let b = (a + 1).min(entrada.len() - 1);
                let t = (inicio - a as f64) as f32;
                entrada[a] * (1.0 - t) + entrada[b] * t
            }
        })
        .collect()
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
        let subida = reamostrar(&[0.0, 1.0], 8_000, 16_000);
        assert_eq!(subida, vec![0.0, 0.5, 1.0, 1.0]);
        assert_eq!(para_whisper(&[0.2; 3200], 2, 16_000).len(), 1600);
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
