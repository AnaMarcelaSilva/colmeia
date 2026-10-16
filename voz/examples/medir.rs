//! Mede o Whisper com uma fala de teste, sem microfone: lê um WAV (16 bits,
//! um canal) só para medir, transcreve como o ditado faz e fica parado
//! alguns segundos com o modelo na memória e depois sem ele, para olhar a
//! memória e a CPU de fora (em /proc). O ditado de verdade nunca lê arquivo
//! de áudio: isto é só para medir.
//!
//! cargo run --release -p colmeia-voz --example medir -- <modelo> <fala.wav>

use std::time::{Duration, Instant};

use colmeia_voz::{audio, transcrever};

fn ler_wav(caminho: &str) -> (Vec<f32>, u32) {
    let bytes = std::fs::read(caminho).expect("ler o WAV");
    assert_eq!(&bytes[0..4], b"RIFF", "não é WAV");
    let taxa = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1, "só um canal");
    let dados = bytes.windows(4).position(|j| j == b"data").expect("sem dados") + 8;
    (bytes[dados..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect(), taxa)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (bruto, taxa) = ler_wav(&args[2]);
    let audio = audio::para_whisper(&bruto, 1, taxa);
    println!("fala: {:.1} s", audio.len() as f32 / audio::TAXA_WHISPER as f32);
    let t = Instant::now();
    let modelo = transcrever::carregar(args[1].as_ref()).expect("carregar");
    println!("marca carregado {:.2} s", t.elapsed().as_secs_f32());
    std::thread::sleep(Duration::from_secs(2));
    println!("marca transcrevendo");
    let t = Instant::now();
    let texto = modelo.transcrever(&audio).expect("transcrever");
    println!("marca transcreveu {:.2} s: {texto}", t.elapsed().as_secs_f32());
    std::thread::sleep(Duration::from_secs(3));
    drop(modelo);
    println!("marca liberou");
    std::thread::sleep(Duration::from_secs(2));
}
