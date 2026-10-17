//! Os coeficientes mel-cepstrais (MFCC) de cada 10 ms de som: o resumo do
//! timbre que o reconhecimento do nome compara. Janela de 25 ms, FFT de 512
//! pontos, 26 filtros na escala mel e 12 coeficientes (sem o de energia, que
//! muda com a distância do microfone). Tudo aqui, sem biblioteca.

use crate::audio::TAXA_WHISPER;

pub const JANELA: usize = 400;
pub const PASSO: usize = 160;
const FFT: usize = 512;
const FILTROS: usize = 26;
pub const COEFICIENTES: usize = 12;

pub type Quadro = [f32; COEFICIENTES];

pub struct Mfcc {
    hamming: Vec<f32>,
    filtros: Vec<Vec<(usize, f32)>>,
    dct: Vec<[f32; FILTROS]>,
    giros: Vec<(f32, f32)>,
}

fn mel(f: f32) -> f32 {
    2595.0 * (1.0 + f / 700.0).log10()
}

fn hz(m: f32) -> f32 {
    700.0 * (10f32.powf(m / 2595.0) - 1.0)
}

impl Default for Mfcc {
    fn default() -> Self {
        Self::novo()
    }
}

impl Mfcc {
    pub fn novo() -> Mfcc {
        let hamming = (0..JANELA).map(|i| 0.54 - 0.46 * (std::f32::consts::TAU * i as f32 / (JANELA - 1) as f32).cos()).collect();
        // Filtros triangulares de 60 Hz a 7600 Hz, espaçados na escala mel.
        let (baixo, alto) = (mel(60.0), mel(7600.0));
        let pontos: Vec<usize> = (0..FILTROS + 2)
            .map(|i| {
                let f = hz(baixo + (alto - baixo) * i as f32 / (FILTROS + 1) as f32);
                ((FFT as f32 + 1.0) * f / TAXA_WHISPER as f32).floor() as usize
            })
            .collect();
        let filtros = (1..=FILTROS)
            .map(|m| {
                let (a, b, c) = (pontos[m - 1], pontos[m], pontos[m + 1]);
                let mut pesos = Vec::new();
                for k in a..=c {
                    let p = if k <= b { (k - a) as f32 / (b - a).max(1) as f32 } else { (c - k) as f32 / (c - b).max(1) as f32 };
                    if p > 0.0 {
                        pesos.push((k, p));
                    }
                }
                pesos
            })
            .collect();
        let dct = (1..=COEFICIENTES)
            .map(|k| {
                let mut linha = [0.0; FILTROS];
                for (n, v) in linha.iter_mut().enumerate() {
                    *v = (std::f32::consts::PI * k as f32 * (n as f32 + 0.5) / FILTROS as f32).cos();
                }
                linha
            })
            .collect();
        let giros = (0..FFT / 2)
            .map(|k| {
                let a = -std::f32::consts::TAU * k as f32 / FFT as f32;
                (a.cos(), a.sin())
            })
            .collect();
        Mfcc { hamming, filtros, dct, giros }
    }

    /// Os coeficientes de 25 ms de som (com a pré-ênfase já aplicada).
    pub fn quadro(&self, amostras: &[f32]) -> Quadro {
        let mut re = vec![0.0f32; FFT];
        let mut im = vec![0.0f32; FFT];
        for (i, s) in amostras.iter().take(JANELA).enumerate() {
            re[i] = s * self.hamming[i];
        }
        self.fft(&mut re, &mut im);
        let mut energias = [0.0f32; FILTROS];
        for (e, filtro) in energias.iter_mut().zip(&self.filtros) {
            let soma: f32 = filtro.iter().map(|&(k, p)| p * (re[k] * re[k] + im[k] * im[k])).sum();
            *e = (soma + 1e-10).ln();
        }
        let mut q = [0.0; COEFICIENTES];
        for (c, linha) in q.iter_mut().zip(&self.dct) {
            *c = linha.iter().zip(&energias).map(|(a, b)| a * b).sum();
        }
        q
    }

    /// FFT de raiz 2, no lugar.
    fn fft(&self, re: &mut [f32], im: &mut [f32]) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j |= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut tamanho = 2;
        while tamanho <= n {
            let passo = n / tamanho;
            for inicio in (0..n).step_by(tamanho) {
                for k in 0..tamanho / 2 {
                    let (c, s) = self.giros[k * passo];
                    let (a, b) = (inicio + k, inicio + k + tamanho / 2);
                    let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            tamanho *= 2;
        }
    }
}

/// Tira MFCC de um fluxo, bloco a bloco: cada quadro com a energia dele
/// (para saber se há fala).
pub struct Extrator {
    mfcc: Mfcc,
    guardado: Vec<f32>,
    anterior: f32,
}

impl Default for Extrator {
    fn default() -> Self {
        Extrator { mfcc: Mfcc::novo(), guardado: Vec::new(), anterior: 0.0 }
    }
}

impl Extrator {
    pub fn alimentar(&mut self, bloco: &[f32]) -> Vec<(Quadro, f32)> {
        // Pré-ênfase: realça os agudos, onde as consoantes se distinguem.
        for &s in bloco {
            self.guardado.push(s - 0.97 * self.anterior);
            self.anterior = s;
        }
        let mut saida = Vec::new();
        let mut inicio = 0;
        while inicio + JANELA <= self.guardado.len() {
            let trecho = &self.guardado[inicio..inicio + JANELA];
            let energia = crate::pausa::rms(trecho);
            saida.push((self.mfcc.quadro(trecho), energia));
            inicio += PASSO;
        }
        self.guardado.drain(..inicio);
        saida
    }
}

/// Os quadros de uma gravação inteira.
pub fn de_audio(audio: &[f32]) -> Vec<(Quadro, f32)> {
    Extrator::default().alimentar(audio)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn seno(f: f32, ms: usize) -> Vec<f32> {
        (0..ms * 16).map(|i| (i as f32 * f * std::f32::consts::TAU / 16_000.0).sin() * 0.3).collect()
    }

    #[test]
    fn fft_acha_a_frequencia() {
        let m = Mfcc::novo();
        let mut re: Vec<f32> = (0..FFT).map(|i| (i as f32 * 32.0 * std::f32::consts::TAU / FFT as f32).sin()).collect();
        let mut im = vec![0.0; FFT];
        m.fft(&mut re, &mut im);
        let pico = (0..FFT / 2).max_by(|&a, &b| (re[a].hypot(im[a])).total_cmp(&re[b].hypot(im[b]))).unwrap();
        assert_eq!(pico, 32);
    }

    #[test]
    fn um_quadro_a_cada_10_ms_e_sons_diferentes_dao_quadros_diferentes() {
        let q = de_audio(&seno(300.0, 1000));
        assert_eq!(q.len(), (16_000 - JANELA) / PASSO + 1);
        let em_blocos: Vec<_> = {
            let mut e = Extrator::default();
            seno(300.0, 1000).chunks(1000).flat_map(|b| e.alimentar(b)).collect()
        };
        assert_eq!(em_blocos.len(), q.len());
        let (a, b) = (de_audio(&seno(300.0, 100))[0].0, de_audio(&seno(2500.0, 100))[0].0);
        let d: f32 = a.iter().zip(&b).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt();
        assert!(d > 5.0, "{d}");
    }
}
