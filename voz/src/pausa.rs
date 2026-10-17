//! Quando a fala termina: para os comandos e as gravações do nome, o
//! microfone fecha sozinho depois de uma pausa, sem precisar clicar de novo.
//! Pela energia de cada trecho de 20 ms contra o ruído de fundo, medido no
//! começo da escuta e acompanhado depois (sem modelo nenhum): o ruído é o
//! trecho mais baixo dos últimos 2 s (a fala sempre tem vãos entre as
//! sílabas; o ventilador, não), com uma folga pela média ficar acima do mínimo.
//!
//! O limiar se adapta ao ruído: num microfone baixo e silencioso a fala
//! também é baixa, e um piso fixo alto a confundia com silêncio (o microfone
//! fechava com "Não ouvi o pedido"). Para entrar na fala o trecho precisa
//! passar bem do ruído; para continuar nela, basta um limiar menor (as
//! sílabas fracas do fim da palavra não contam como pausa).

use crate::audio::TAXA_WHISPER;

/// 20 ms a 16 kHz.
const TRECHO: usize = TAXA_WHISPER as usize / 50;
/// Os primeiros 300 ms medem o ruído de fundo.
const CALIBRAR: usize = 15;
/// O ruído é o mínimo dos últimos 2 s…
const JANELA_RUIDO: usize = 100;
/// …vezes isto (o RMS médio do ruído fica acima do mínimo).
const FOLGA_RUIDO: f32 = 1.5;
/// Fala de verdade: mais de 160 ms dela (um estalo não conta).
const FALA_MINIMA: usize = 8;
/// Abaixo disto é o microfone mudo, não ruído: o limiar não desce mais.
const PISO: f32 = 0.0012;
/// Entrar na fala: ~10 dB acima do ruído; continuar nela: ~5 dB.
const ENTRAR: f32 = 3.2;
const FICAR: f32 = 1.8;

/// Os tempos de uma escuta que termina na pausa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tempos {
    /// O silêncio depois da fala que encerra.
    pub pausa_ms: u32,
    /// Quanto esperar a fala começar.
    pub espera_ms: u32,
    /// O máximo, falando ou não.
    pub limite_ms: u32,
}

impl Tempos {
    /// A pausa que encerra um comando: 1,2 s, sem cortar quem respira no meio da frase.
    pub const PAUSA_PADRAO: u32 = 1200;

    /// Um comando falado: `pausa_ms` de silêncio encerra (de 0,6 a 3 s); 4 s
    /// para começar a falar; 30 s no máximo.
    pub fn de_comando(pausa_ms: u32) -> Tempos {
        Tempos { pausa_ms: pausa_ms.clamp(600, 3000), espera_ms: 4000, limite_ms: 30_000 }
    }

    /// Uma gravação do nome: curta.
    pub fn de_nome() -> Tempos {
        Tempos { pausa_ms: 500, espera_ms: 4000, limite_ms: 3000 }
    }
}

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
    /// Em trechos de 20 ms.
    pausa: usize,
    espera: usize,
    limite: usize,
    /// As energias da calibração, guardadas até medir o ruído (depois, `None`).
    inicio: Option<Vec<f32>>,
    /// As energias dos últimos 2 s.
    recentes: std::collections::VecDeque<f32>,
    ruido: f32,
    na_fala: bool,
    trechos: usize,
    com_fala: usize,
    silencio: usize,
    resto: Vec<f32>,
    situacao: Situacao,
}

impl Pausa {
    pub fn nova(t: Tempos) -> Pausa {
        let trechos = |ms: u32| (ms as usize / 20).max(1);
        Pausa {
            pausa: trechos(t.pausa_ms),
            espera: trechos(t.espera_ms),
            limite: trechos(t.limite_ms),
            inicio: Some(Vec::with_capacity(CALIBRAR)),
            recentes: std::collections::VecDeque::with_capacity(JANELA_RUIDO),
            ruido: 0.0,
            na_fala: false,
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

    /// O ruído de fundo medido (RMS), para conferir nos testes.
    pub fn ruido(&self) -> f32 {
        self.ruido
    }

    /// Ouviu fala em algum momento.
    pub fn ouviu(&self) -> bool {
        self.com_fala >= FALA_MINIMA
    }

    pub fn alimentar(&mut self, bloco: &[f32]) -> Situacao {
        if self.acabou() {
            return self.situacao;
        }
        self.resto.extend_from_slice(bloco);
        let mut inicio = 0;
        while inicio + TRECHO <= self.resto.len() && !self.acabou() {
            let energia = rms(&self.resto[inicio..inicio + TRECHO]);
            inicio += TRECHO;
            self.trecho(energia);
        }
        self.resto.drain(..inicio);
        self.situacao
    }

    fn acabou(&self) -> bool {
        !matches!(self.situacao, Situacao::Esperando | Situacao::Falando)
    }

    fn trecho(&mut self, energia: f32) {
        if self.recentes.len() == JANELA_RUIDO {
            self.recentes.pop_front();
        }
        self.recentes.push_back(energia);
        self.ruido = self.recentes.iter().copied().fold(f32::INFINITY, f32::min) * FOLGA_RUIDO;
        let Some(inicio) = self.inicio.as_mut() else { return self.decidir(energia) };
        inicio.push(energia);
        if inicio.len() < CALIBRAR {
            return;
        }
        let inicio = self.inicio.take().unwrap_or_default();
        // A calibração também conta: a fala pode ter começado junto.
        for e in inicio {
            self.decidir(e);
            if self.acabou() {
                break;
            }
        }
    }

    fn decidir(&mut self, energia: f32) {
        self.trechos += 1;
        let entrar = (self.ruido * ENTRAR).max(PISO);
        let ficar = (self.ruido * FICAR).max(PISO * 0.75);
        self.na_fala = if self.na_fala { energia > ficar } else { energia > entrar };
        if self.na_fala {
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

    /// Um chiado de amplitude `nivel`, como o de um microfone de verdade.
    fn chiado(ms: usize, nivel: f32) -> Vec<f32> {
        let mut x: u32 = 12345;
        (0..ms * 16)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                ((x >> 16) as f32 / 32768.0 - 1.0) * nivel
            })
            .collect()
    }

    /// Uma fala sintética: sílabas de 200 ms (subindo e descendo, com o vão
    /// entre elas ~25 dB abaixo do pico, como entre as sílabas de verdade)
    /// sobre o chiado, com a voz em `nivel` de pico.
    fn fala(ms: usize, nivel: f32, ruido: f32) -> Vec<f32> {
        let base = chiado(ms, ruido);
        base.iter()
            .enumerate()
            .map(|(i, r)| {
                let t = i as f32 / 16_000.0;
                let silaba = (t * std::f32::consts::PI / 0.2).sin().abs();
                let voz = (t * 180.0 * std::f32::consts::TAU).sin() * 0.6 + (t * 540.0 * std::f32::consts::TAU).sin() * 0.4;
                r + voz * nivel * (0.05 + 0.95 * silaba * silaba)
            })
            .collect()
    }

    fn em_blocos(p: &mut Pausa, audio: &[f32]) -> Situacao {
        let mut s = p.situacao();
        for b in audio.chunks(1600) {
            s = p.alimentar(b);
        }
        s
    }

    fn comando() -> Pausa {
        Pausa::nova(Tempos::de_comando(Tempos::PAUSA_PADRAO))
    }

    #[test]
    fn pausa_curta_no_meio_da_frase_nao_encerra_e_a_longa_sim() {
        let mut p = comando();
        assert_eq!(em_blocos(&mut p, &chiado(500, 0.003)), Situacao::Esperando);
        assert_eq!(em_blocos(&mut p, &fala(1200, 0.2, 0.003)), Situacao::Falando);
        // Respirar no meio da frase (0,9 s) não fecha o microfone.
        assert_eq!(em_blocos(&mut p, &chiado(900, 0.003)), Situacao::Falando);
        assert_eq!(em_blocos(&mut p, &fala(800, 0.2, 0.003)), Situacao::Falando);
        assert_eq!(em_blocos(&mut p, &chiado(1100, 0.003)), Situacao::Falando);
        assert_eq!(em_blocos(&mut p, &chiado(200, 0.003)), Situacao::Terminou);
    }

    #[test]
    fn a_pausa_e_configuravel_e_tem_limites() {
        assert_eq!(Tempos::de_comando(100).pausa_ms, 600);
        assert_eq!(Tempos::de_comando(9000).pausa_ms, 3000);
        let mut p = Pausa::nova(Tempos::de_comando(2000));
        em_blocos(&mut p, &fala(800, 0.2, 0.002));
        assert_eq!(em_blocos(&mut p, &chiado(1600, 0.002)), Situacao::Falando);
        assert_eq!(em_blocos(&mut p, &chiado(500, 0.002)), Situacao::Terminou);
    }

    #[test]
    fn microfone_baixo_e_silencioso_ainda_ouve_a_fala() {
        // Fala com pico de 0,01 num ruído de 0,0005: antes, abaixo do piso fixo.
        let mut p = comando();
        em_blocos(&mut p, &chiado(400, 0.0005));
        assert_eq!(em_blocos(&mut p, &fala(1500, 0.01, 0.0005)), Situacao::Falando);
        assert_eq!(em_blocos(&mut p, &chiado(1300, 0.0005)), Situacao::Terminou);
    }

    #[test]
    fn sala_barulhenta_nao_vira_fala_e_a_voz_mais_alta_sim() {
        let mut p = comando();
        // Um ventilador constante não é fala…
        assert_eq!(em_blocos(&mut p, &chiado(3000, 0.04)), Situacao::Esperando);
        assert!(p.ruido() > 0.01, "ruído medido {}", p.ruido());
        // …e a voz por cima dele é.
        assert_eq!(em_blocos(&mut p, &fala(1500, 0.3, 0.04)), Situacao::Falando);
        assert_eq!(em_blocos(&mut p, &chiado(1300, 0.04)), Situacao::Terminou);
    }

    #[test]
    fn fala_que_comeca_junto_com_o_microfone() {
        let mut p = comando();
        assert_eq!(em_blocos(&mut p, &fala(2000, 0.2, 0.002)), Situacao::Falando);
        assert_eq!(em_blocos(&mut p, &chiado(1300, 0.002)), Situacao::Terminou);
    }

    #[test]
    fn espera_quatro_segundos_para_comecar() {
        let mut p = comando();
        assert_eq!(em_blocos(&mut p, &chiado(3500, 0.002)), Situacao::Esperando);
        assert_eq!(em_blocos(&mut p, &fala(1000, 0.2, 0.002)), Situacao::Falando);
        let mut p = comando();
        assert_eq!(em_blocos(&mut p, &chiado(4100, 0.002)), Situacao::NadaOuvido);
    }

    #[test]
    fn um_estalo_nao_conta_como_fala() {
        let mut p = comando();
        let mut audio = chiado(1000, 0.002);
        audio.extend(fala(60, 0.4, 0.002));
        audio.extend(chiado(3500, 0.002));
        assert_eq!(em_blocos(&mut p, &audio), Situacao::NadaOuvido);
        assert!(!p.ouviu());
    }

    #[test]
    fn fala_sem_fim_para_no_limite() {
        let mut p = comando();
        assert_eq!(em_blocos(&mut p, &fala(31_000, 0.2, 0.002)), Situacao::Limite);
    }

    #[test]
    fn microfone_mudo_nao_inventa_fala() {
        let mut p = comando();
        assert_eq!(em_blocos(&mut p, &vec![0.0; 16_000 * 5]), Situacao::NadaOuvido);
    }
}
