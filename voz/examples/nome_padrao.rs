//! Gera as referências do nome padrão ("Colmeia") que vão dentro do
//! colmeia-voz (recursos/nome-colmeia.json): a palavra falada pela própria
//! voz sintética da abelha, com vozes e velocidades diferentes, vira MFCC
//! pelo mesmo caminho das gravações de quem usa. Nenhum áudio é guardado.
//!
//! cargo run --release -p colmeia-voz --example nome_padrao -- <modelo.onnx> <libonnxruntime> <saida.json> <voz.bin:velocidade>...

use std::path::Path;

use colmeia_voz::audio::{self, TAXA_WHISPER};
use colmeia_voz::fala::{fonemas, kokoro};
use colmeia_voz::nome::Referencias;

/// A palavra no meio de um pouco de silêncio, como numa gravação.
fn falar(k: &mut kokoro::Kokoro, texto: &str, velocidade: f32) -> Vec<f32> {
    let onda = k.falar(&fonemas::palavra(texto), velocidade).expect("falar");
    let mut v = vec![0.0; TAXA_WHISPER as usize / 4];
    v.extend(audio::reamostrar(&onda, kokoro::TAXA, TAXA_WHISPER));
    v.extend(vec![0.0; TAXA_WHISPER as usize / 4]);
    v
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (modelo, runtime, saida) = (&a[1], &a[2], &a[3]);
    let vozes: Vec<(&str, f32)> = a[4..].iter().map(|v| v.split_once(':').map(|(p, s)| (p, s.parse().expect("velocidade"))).expect("voz:velocidade")).collect();
    let mut k = kokoro::carregar(modelo.as_ref(), Path::new(vozes[0].0), runtime.as_ref(), 4).expect("carregar");
    let mut gravacoes = Vec::new();
    for (voz, velocidade) in &vozes {
        k.trocar_voz(Path::new(voz)).expect("voz");
        gravacoes.push(falar(&mut k, "Colmeia", *velocidade));
    }
    let r = Referencias::de_gravacoes("Colmeia", &gravacoes).expect("referências");
    std::fs::write(saida, serde_json::to_string(&r).expect("json")).expect("gravar");
    println!("{} referências, {} quadros", r.referencias.len(), r.referencias.iter().map(|q| q.len()).sum::<usize>());
}
