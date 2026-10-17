//! Fala um texto com a voz da abelha num WAV (24 kHz), sem tocar: para
//! ouvir a voz, medir o tempo e a memória e conferir os fonemas.
//!
//! cargo run --release -p colmeia-voz --example falar -- <modelo.onnx> <voz.bin> <libonnxruntime> <saida.wav> "texto" [--efeito]

use std::time::Instant;

use colmeia_voz::fala::{self, efeito, kokoro};

fn gravar_wav(caminho: &str, onda: &[f32], taxa: u32) {
    let dados: Vec<u8> = onda.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut w = Vec::with_capacity(44 + dados.len());
    w.extend(b"RIFF");
    w.extend((36 + dados.len() as u32).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes());
    w.extend(1u16.to_le_bytes());
    w.extend(taxa.to_le_bytes());
    w.extend((taxa * 2).to_le_bytes());
    w.extend(2u16.to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend((dados.len() as u32).to_le_bytes());
    w.extend(dados);
    std::fs::write(caminho, w).expect("gravar o WAV");
}

fn memoria() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS")).map(|l| l.split_whitespace().nth(1).unwrap_or("").to_string() + " kB"))
        .unwrap_or_default()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let com_efeito = a.iter().any(|x| x == "--efeito");
    let t = Instant::now();
    let mut k = kokoro::carregar(a[1].as_ref(), a[2].as_ref(), a[3].as_ref(), 4).expect("carregar");
    println!("carregou em {:.2} s, memória {}", t.elapsed().as_secs_f32(), memoria());
    let mut onda = Vec::new();
    let t = Instant::now();
    for f in fala::preparar(&a[5]) {
        println!("fonemas: {f}");
        onda.extend(k.falar(&f, 1.05).expect("falar"));
    }
    let segundos = onda.len() as f32 / kokoro::TAXA as f32;
    println!("falou {segundos:.2} s de voz em {:.2} s, memória {}", t.elapsed().as_secs_f32(), memoria());
    if com_efeito {
        efeito::aplicar(&mut onda, kokoro::TAXA);
    }
    gravar_wav(&a[4], &onda, kokoro::TAXA);
}
