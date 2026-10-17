//! Mede o reconhecimento do nome sem microfone: frases faladas pela voz
//! sintética (vozes e velocidades que não estão nas referências), com e sem
//! o nome, passam pelo detector como se viessem do microfone (em blocos de
//! 100 ms). Mostra o menor custo de cada frase, para escolher o limiar, e o
//! que cada sensibilidade acertaria.
//!
//! cargo run --release -p colmeia-voz --example avaliar_nome -- <modelo.onnx> <libonnxruntime> <referencias.json|padrao> <voz.bin:velocidade>...

use std::path::Path;

use colmeia_voz::audio::{self, TAXA_WHISPER};
use colmeia_voz::fala::{self, kokoro};
use colmeia_voz::nome::{self, Detector, Referencias};

const COM_NOME: [&str; 6] = [
    "Colmeia, rodar a loja web.",
    "Colmeia, o que espera por mim?",
    "Ei, Colmeia! Como está a tarefa de login?",
    "Colmeia. Abrir a tarefa do cache.",
    "Olha, Colmeia, parar o servidor.",
    "Colmeia",
];
const SEM_NOME: [&str; 10] = [
    "A colher está em cima da mesa.",
    "Rodar a loja web.",
    "O que espera por mim?",
    "Comeu a meia toda e foi embora.",
    "Carmem e Amélia foram ao mercado.",
    "Como é que é? Não ouvi direito.",
    "A colmeia de abelhas fica no jardim.",
    "Coma a maçã e depois a pera.",
    "Vamos ver o cinema amanhã cedo.",
    "Colocar a meia no pé esquerdo.",
];

fn falar(k: &mut kokoro::Kokoro, texto: &str, velocidade: f32) -> Vec<f32> {
    let mut v = vec![0.0f32; TAXA_WHISPER as usize / 2];
    for f in fala::preparar(texto) {
        let onda = k.falar(&f, velocidade).expect("falar");
        v.extend(audio::reamostrar(&onda, kokoro::TAXA, TAXA_WHISPER));
    }
    v.extend(vec![0.0f32; TAXA_WHISPER as usize]);
    // Um chiado de fundo, como o de um microfone.
    let mut x: u32 = 12345;
    for s in v.iter_mut() {
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        *s += ((x >> 16) as f32 / 65536.0 - 0.5) * 0.004;
    }
    v
}

/// O menor custo da frase e quantas vezes o detector dispararia com o limiar dado.
fn passar(r: &Referencias, audio: &[f32], limiar: f32) -> (f32, usize) {
    let mut d = Detector::novo(r, 5);
    d.definir_limiar(limiar);
    let mut vezes = 0;
    for b in audio.chunks(1600) {
        vezes += d.alimentar(b).is_some() as usize;
    }
    (d.melhor, vezes)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let r = if a[3] == "padrao" { Referencias::padrao() } else { Referencias::ler(Path::new(&a[3])).expect("referências") };
    let vozes: Vec<(&str, f32)> = a[4..].iter().map(|v| v.split_once(':').map(|(p, s)| (p, s.parse().expect("velocidade"))).expect("voz:velocidade")).collect();
    let mut k = kokoro::carregar(a[1].as_ref(), Path::new(vozes[0].0), a[2].as_ref(), 4).expect("carregar");
    let mut positivos = Vec::new();
    let mut negativos = Vec::new();
    for (voz, velocidade) in &vozes {
        k.trocar_voz(Path::new(voz)).expect("voz");
        for f in COM_NOME {
            let (c, _) = passar(&r, &falar(&mut k, f, *velocidade), f32::NEG_INFINITY);
            println!("com nome  {c:6.2}  {voz}:{velocidade}  {f}");
            positivos.push(c);
        }
        for f in SEM_NOME {
            let (c, _) = passar(&r, &falar(&mut k, f, *velocidade), f32::NEG_INFINITY);
            println!("sem nome  {c:6.2}  {voz}:{velocidade}  {f}");
            negativos.push(c);
        }
    }
    println!("sensibilidade  limiar  ouviu o nome  ouviu sem o nome");
    for s in 1..=9u8 {
        let l = nome::limiar(s);
        let acertos = positivos.iter().filter(|c| **c <= l).count();
        let enganos = negativos.iter().filter(|c| **c <= l).count();
        println!("{s:>13}  {l:6.2}  {acertos:>4}/{:<7}  {enganos:>4}/{}", positivos.len(), negativos.len());
    }
}
