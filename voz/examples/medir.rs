//! Mede o Whisper com uma fala de teste, sem microfone: lê um WAV (16 bits,
//! um canal) só para medir e transcreve como o ditado faz, com a busca
//! gulosa e com a busca em feixe, com e sem o vocabulário no prompt. Mostra
//! o tempo de cada uma e a memória do processo (em /proc). O ditado de
//! verdade nunca lê arquivo de áudio: isto é só para medir.
//!
//! cargo run --release -p colmeia-voz --example medir -- <modelo> <fala.wav> [vocabulário]

use std::time::Instant;

use colmeia_voz::{audio, transcrever};

fn ler_wav(caminho: &str) -> (Vec<f32>, u32) {
    let bytes = std::fs::read(caminho).expect("ler o WAV");
    assert_eq!(&bytes[0..4], b"RIFF", "não é WAV");
    let taxa = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1, "só um canal");
    let dados = bytes.windows(4).position(|j| j == b"data").expect("sem dados") + 8;
    (bytes[dados..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect(), taxa)
}

/// A memória de agora e o pico, em MB (só no Linux).
fn memoria() -> String {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let campo = |nome: &str| {
        status.lines().find(|l| l.starts_with(nome)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 1024.0
    };
    format!("{:.0} MB (pico {:.0} MB)", campo("VmRSS:"), campo("VmHWM:"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (bruto, taxa) = ler_wav(&args[2]);
    let vocabulario = args.get(3).map_or("", String::as_str);
    let audio = audio::para_whisper(&bruto, 1, taxa);
    println!("fala: {:.1} s · antes do modelo: {}", audio.len() as f32 / audio::TAXA_WHISPER as f32, memoria());
    let t = Instant::now();
    let modelo = transcrever::carregar(args[1].as_ref()).expect("carregar");
    println!("carregado em {:.2} s · {}", t.elapsed().as_secs_f32(), memoria());
    for (feixe, prompt) in [(1, ""), (1, vocabulario), (transcrever::FEIXE, ""), (transcrever::FEIXE, vocabulario)] {
        let t = Instant::now();
        let texto = modelo.transcrever_com(&audio, prompt, feixe).expect("transcrever");
        let busca = if feixe > 1 { format!("feixe {feixe}") } else { "gulosa".into() };
        let com = if prompt.is_empty() { "sem vocabulário" } else { "com vocabulário" };
        println!("{busca}, {com}: {:.2} s · {} · {texto}", t.elapsed().as_secs_f32(), memoria());
    }
}
