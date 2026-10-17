//! O efeito leve da voz da abelha, opcional: um brilho (os agudos um pouco
//! mais presentes) e um eco curto, como se ela falasse de dentro da colmeia.
//! Feito na própria onda, sem biblioteca; a fala continua clara.

/// Atraso do eco e quanto dele volta (cada repetição volta com o mesmo tanto
/// da anterior: 25%, 6%, 1,6%…).
const ECO_MS: f32 = 95.0;
const ECO_VOLTA: f32 = 0.25;
/// Quanto dos agudos (acima de ~3 kHz) soma ao som.
const BRILHO: f32 = 0.45;

pub fn aplicar(onda: &mut Vec<f32>, taxa: u32) {
    brilho(onda, taxa);
    eco(onda, taxa);
    normalizar(onda);
}

/// Soma os agudos: o som menos a versão passada por um passa-baixa de 1ª ordem.
fn brilho(onda: &mut [f32], taxa: u32) {
    let corte = 3000.0_f32;
    let rc = 1.0 / (std::f32::consts::TAU * corte);
    let dt = 1.0 / taxa as f32;
    let alfa = dt / (rc + dt);
    let mut grave = 0.0;
    for s in onda.iter_mut() {
        grave += alfa * (*s - grave);
        *s += BRILHO * (*s - grave);
    }
}

/// Um eco que se repete baixinho e some; a onda cresce o bastante para ele terminar.
fn eco(onda: &mut Vec<f32>, taxa: u32) {
    let atraso = (taxa as f32 * ECO_MS / 1000.0) as usize;
    if atraso == 0 {
        return;
    }
    let cauda = atraso * 4;
    onda.resize(onda.len() + cauda, 0.0);
    // A onda de trás já tem o eco dela: cada volta repete a anterior, mais baixa.
    for i in atraso..onda.len() {
        onda[i] += ECO_VOLTA * onda[i - atraso];
    }
}

/// Sem estourar: o pico fica em 0,95.
fn normalizar(onda: &mut [f32]) {
    let pico = onda.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if pico > 0.95 {
        let k = 0.95 / pico;
        onda.iter_mut().for_each(|s| *s *= k);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn eco_aparece_depois_do_atraso_e_some() {
        let taxa = 24_000;
        let mut onda = vec![0.0; 100];
        onda[0] = 0.5;
        eco(&mut onda, taxa);
        let atraso = (taxa as f32 * ECO_MS / 1000.0) as usize;
        assert!(onda.len() > atraso * 4);
        assert!(onda[atraso] > 0.05 && onda[atraso] < 0.5, "{}", onda[atraso]);
        assert!(onda[2 * atraso].abs() < onda[atraso].abs());
        assert!(onda.last().unwrap().abs() < 0.01);
    }

    #[test]
    fn brilho_mantem_o_grave_e_realca_o_agudo() {
        let taxa = 24_000;
        let seno = |f: f32| (0..taxa).map(|i| (i as f32 * f * std::f32::consts::TAU / taxa as f32).sin() * 0.3).collect::<Vec<f32>>();
        let energia = |v: &[f32]| v.iter().map(|s| s * s).sum::<f32>();
        let (mut grave, mut agudo) = (seno(150.0), seno(6000.0));
        let (g0, a0) = (energia(&grave), energia(&agudo));
        brilho(&mut grave, taxa);
        brilho(&mut agudo, taxa);
        assert!((energia(&grave) / g0 - 1.0).abs() < 0.1);
        assert!(energia(&agudo) / a0 > 1.5);
    }

    #[test]
    fn nunca_estoura() {
        let mut onda = vec![0.9; 24_000];
        aplicar(&mut onda, 24_000);
        assert!(onda.iter().all(|s| s.abs() <= 0.95 + 1e-6));
    }
}
