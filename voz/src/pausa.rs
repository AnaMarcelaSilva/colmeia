//! Quando a fala termina: para os comandos e as gravações do nome, o
//! microfone fecha sozinho depois de uma pausa, sem precisar clicar de novo.
//! Pela energia de cada trecho de 20 ms contra o ruído de fundo, que a
//! própria escuta vai medindo (sem modelo nenhum).

use crate::audio::TAXA_WHISPER;

/// 20 ms a 16 kHz.
const TRECHO: usize = TAXA_WHISPER as usize / 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Situacao {
    /// Ainda não ouviu fala.
    Esperando,
    Falando,
    /// Ouviu fala e depois a pausa: terminou.
    Terminou,
    /// Passou o tempo sem ouvir fala nenhuma.
    NadaOuvido,
    /// Falou até o limite: termina aqui.
    Limite,
}

pub struct Pausa {
    /// Quanto silêncio depois da fala encerra (em trechos).
    pausa: usize,
    /// Quanto esperar a fala começar, e o máximo de fala.
    espera: usize,
    limite: usize,
    ruido: f32,
    trechos: usize,
    com_fala: usize,
    silencio: usize,
    resto: Vec<f32>,
    situacao: Situacao,
}

impl Pausa {
    /// `pausa_ms`: o silêncio que encerra; `espera_ms`: quanto esperar a fala
    /// começar; `limite_ms`: o máximo, falando ou não.
    pub fn nova(pausa_ms: u32, espera_ms: u32, limite_ms: u32) -> Pausa {
        let trechos = |ms: u32| (ms as usize / 20).max(1);
        Pausa {
            pausa: trechos(pausa_ms),
            espera: trechos(espera_ms),
            limite: trechos(limite_ms),
            ruido: 0.0,
            trechos: 0,
            com_fala: 0,
            silencio: 0,
            resto: Vec::new(),
            situacao: Situacao::Esperando,
        }
    }

    pub fn situacao(&self) -> Situacao {
        self.situacao
    }

    /// Ouviu fala em algum momento (mais de 150 ms dela).
    pub fn ouviu(&self) -> bool {
        self.com_fala * 20 >= 150
    }

    pub fn alimentar(&mut self, bloco: &[f32]) -> Situacao {
        if matches!(self.situacao, Situacao::Terminou | Situacao::NadaOuvido | Situacao::Limite) {
            return self.situacao;
        }
        self.resto.extend_from_slice(bloco);
        let mut inicio = 0;
        while inicio + TRECHO <= self.resto.len() {
            let energia = rms(&self.resto[inicio..inicio + TRECHO]);
            inicio += TRECHO;
            self.trecho(energia);
            if !matches!(self.situacao, Situacao::Esperando | Situacao::Falando) {
                break;
            }
        }
        self.resto.drain(..inicio);
        self.situacao
    }

    fn trecho(&mut self, energia: f32) {
        self.trechos += 1;
        if self.trechos == 1 {
            // Se a fala já começou junto com o microfone, o ruído não é ela.
            self.ruido = energia.min(0.004);
        }
        let fala = energia > (self.ruido * 3.0).max(0.008);
        // O ruído de fundo: desce rápido e sobe devagar, e só fora da fala.
        if energia < self.ruido {
            self.ruido = self.ruido * 0.7 + energia * 0.3;
        } else if !fala {
            self.ruido = self.ruido * 0.98 + energia * 0.02;
        }
        if fala {
            self.com_fala += 1;
            self.silencio = 0;
            if self.ouviu() {
                self.situacao = Situacao::Falando;
            }
        } else {
            self.silencio += 1;
        }
        self.situacao = if self.trechos >= self.limite {
            if self.ouviu() { Situacao::Limite } else { Situacao::NadaOuvido }
        } else if self.situacao == Situacao::Falando && self.silencio >= self.pausa {
            Situacao::Terminou
        } else if !self.ouviu() && self.trechos >= self.espera {
            Situacao::NadaOuvido
        } else {
            self.situacao
        };
    }
}

pub fn rms(trecho: &[f32]) -> f32 {
    if trecho.is_empty() {
        return 0.0;
    }
    (trecho.iter().map(|s| s * s).sum::<f32>() / trecho.len() as f32).sqrt()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn silencio(ms: usize) -> Vec<f32> {
        // Um chiado bem baixo, como o de um microfone de verdade.
        (0..ms * 16).map(|i| ((i * 7919) % 101) as f32 / 101.0 * 0.004 - 0.002).collect()
    }

    fn fala(ms: usize) -> Vec<f32> {
        (0..ms * 16).map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / 16_000.0).sin() * 0.3).collect()
    }

    fn alimentar_em_blocos(p: &mut Pausa, audio: &[f32]) -> Situacao {
        let mut s = p.situacao();
        for b in audio.chunks(1600) {
            s = p.alimentar(b);
        }
        s
    }

    #[test]
    fn termina_depois_da_pausa() {
        let mut p = Pausa::nova(800, 5000, 10_000);
        assert_eq!(alimentar_em_blocos(&mut p, &silencio(500)), Situacao::Esperando);
        assert_eq!(alimentar_em_blocos(&mut p, &fala(1200)), Situacao::Falando);
        // Uma pausa curta no meio da frase não encerra.
        assert_eq!(alimentar_em_blocos(&mut p, &silencio(400)), Situacao::Falando);
        assert_eq!(alimentar_em_blocos(&mut p, &fala(600)), Situacao::Falando);
        assert_eq!(alimentar_em_blocos(&mut p, &silencio(900)), Situacao::Terminou);
        assert!(p.ouviu());
    }

    #[test]
    fn sem_fala_desiste_e_um_estalo_nao_conta() {
        let mut p = Pausa::nova(800, 3000, 10_000);
        let mut audio = silencio(1000);
        audio.extend(fala(60));
        audio.extend(silencio(2500));
        assert_eq!(alimentar_em_blocos(&mut p, &audio), Situacao::NadaOuvido);
        assert!(!p.ouviu());
    }

    #[test]
    fn fala_sem_fim_para_no_limite() {
        let mut p = Pausa::nova(800, 3000, 4000);
        assert_eq!(alimentar_em_blocos(&mut p, &fala(5000)), Situacao::Limite);
    }
}
