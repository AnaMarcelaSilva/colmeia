//! "Ouvir o nome": a abelha fica ouvindo o nome dela para abrir o microfone
//! sem clique. Do jeito do Rustpotter (referências por comparação), mas
//! aqui, pequeno e sem dependência: cada gravação do nome vira uma
//! sequência de MFCC (a referência, sem o áudio), e o som que chega é
//! comparado com elas por alinhamento dinâmico (DTW) só quando há fala.
//!
//! O nome padrão ("Colmeia") vem pronto, com referências feitas com a voz
//! sintética da própria abelha (veja examples/nome_padrao.rs); outro nome
//! aprende com 3 ou 4 gravações de quem usa.

pub mod mfcc;

use std::collections::VecDeque;
use std::path::Path;

use serde::{Deserialize, Serialize};

use mfcc::{COEFICIENTES, Extrator, Quadro};

/// As referências do nome padrão, geradas por examples/nome_padrao.rs.
pub const PADRAO: &str = include_str!("../../recursos/nome-colmeia.json");

/// Um arquivo de referências maior que isto não é nosso.
pub const LIMITE_ARQUIVO: u64 = 2 << 20;

/// Quantas gravações aprender pede.
pub const MINIMO_GRAVACOES: usize = 3;
pub const MAXIMO_GRAVACOES: usize = 4;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Referencias {
    pub nome: String,
    pub versao: u32,
    /// Cada gravação: um MFCC por 10 ms, já sem a média (CMN).
    pub referencias: Vec<Vec<Quadro>>,
}

const VERSAO: u32 = 1;

const GUARDAR: &str = "Não consegui guardar as gravações do nome. Confira o espaço em disco e grave de novo.";

/// O pedaço da gravação com fala: tira o silêncio do começo e do fim.
fn aparar(quadros: &[(Quadro, f32)]) -> &[(Quadro, f32)] {
    let pico = quadros.iter().map(|q| q.1).fold(0.0f32, f32::max);
    let limiar = (pico * 0.12).max(0.003);
    let inicio = quadros.iter().position(|q| q.1 > limiar).unwrap_or(0);
    let fim = quadros.iter().rposition(|q| q.1 > limiar).map_or(quadros.len(), |i| i + 1);
    &quadros[inicio..fim.max(inicio)]
}

/// Tira a média de cada coeficiente (o "timbre" do microfone e da sala).
fn sem_media(quadros: &[Quadro]) -> Vec<Quadro> {
    if quadros.is_empty() {
        return Vec::new();
    }
    let mut media = [0.0f32; COEFICIENTES];
    for q in quadros {
        for (m, v) in media.iter_mut().zip(q) {
            *m += v / quadros.len() as f32;
        }
    }
    quadros.iter().map(|q| std::array::from_fn(|i| q[i] - media[i])).collect()
}

impl Referencias {
    /// Aprende com gravações (16 kHz, um canal) que estão só na memória.
    pub fn de_gravacoes(nome: &str, gravacoes: &[Vec<f32>]) -> Result<Referencias, String> {
        if gravacoes.len() < MINIMO_GRAVACOES {
            return Err(format!("Grave o nome pelo menos {MINIMO_GRAVACOES} vezes."));
        }
        let mut referencias = Vec::new();
        for g in gravacoes {
            let quadros = mfcc::de_audio(g);
            let fala = aparar(&quadros);
            // Entre 0,25 s e 2 s de fala: menos é um estalo, mais não é um nome.
            if !(25..=200).contains(&fala.len()) {
                return Err("Uma das gravações ficou curta ou longa demais. Grave de novo, só o nome.".into());
            }
            referencias.push(sem_media(&fala.iter().map(|q| q.0).collect::<Vec<_>>()));
        }
        Ok(Referencias { nome: nome.trim().to_string(), versao: VERSAO, referencias })
    }

    pub fn padrao() -> Referencias {
        serde_json::from_str(PADRAO).expect("as referências do nome padrão vêm com o programa")
    }

    /// Gravações do nome padrão somam às referências sintéticas: a voz de
    /// quem usa entra sem perder o que já reconhecia.
    pub fn com_padrao(mut self) -> Referencias {
        let padrao = Referencias::padrao();
        if chave(&self.nome) == chave(&padrao.nome) {
            self.referencias.extend(padrao.referencias);
        }
        self
    }

    pub fn ler(caminho: &Path) -> Result<Referencias, String> {
        let erro = "As gravações do nome não abriram. Grave o nome de novo em Ajustes › Voz.";
        let tamanho = std::fs::metadata(caminho).map_err(|_| erro.to_string())?.len();
        if tamanho > LIMITE_ARQUIVO {
            return Err(erro.into());
        }
        let r: Referencias = serde_json::from_slice(&std::fs::read(caminho).map_err(|_| erro.to_string())?).map_err(|_| erro.to_string())?;
        if r.versao != VERSAO || r.referencias.is_empty() || r.referencias.iter().any(|q| q.len() < 10) {
            return Err(erro.into());
        }
        Ok(r)
    }

    /// Grava num temporário 0600 ao lado e troca de nome: nunca um arquivo pela metade.
    pub fn gravar(&self, destino: &Path) -> Result<(), String> {
        let erro = |e: std::io::Error| crate::falha(GUARDAR, e);
        let nome = destino.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Só o arquivo do perfil, na pasta que já existe (a do núcleo).
        let valido = nome.starts_with("nome-") && nome.ends_with(".json") && nome[5..nome.len() - 5].chars().all(|c| c.is_ascii_digit()) && nome.len() > 10;
        let pasta = destino.parent().filter(|p| p.is_dir());
        let (true, Some(pasta)) = (valido, pasta) else {
            return Err("Lugar inválido para as gravações do nome.".into());
        };
        let tmp = pasta.join(format!(".{nome}.{}", std::process::id()));
        let texto = serde_json::to_vec(self).map_err(|e| crate::falha(GUARDAR, e))?;
        let mut abrir = std::fs::OpenOptions::new();
        abrir.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut abrir, 0o600);
        let resultado = (|| {
            use std::io::Write;
            let mut f = abrir.open(&tmp)?;
            f.write_all(&texto)?;
            f.sync_all()?;
            std::fs::rename(&tmp, destino)
        })();
        if resultado.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        resultado.map_err(erro)
    }
}

/// O nome como soa: sem caixa, acento nem espaço (os acentos do português
/// bastam para comparar com o nome padrão).
fn chave(nome: &str) -> String {
    nome.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' | 'ë' => 'e',
            'í' | 'î' | 'ì' | 'ï' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' | 'ö' => 'o',
            'ú' | 'û' | 'ù' | 'ü' => 'u',
            'ç' => 'c',
            outra => outra,
        })
        .collect()
}

fn distancia(a: &Quadro, b: &Quadro) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

/// Quanto a referência se parece com o fim da janela: DTW com o começo livre
/// na janela e o fim no quadro mais recente, dividido pelo comprimento da
/// referência (média por quadro; menor é mais parecido).
pub fn custo(referencia: &[Quadro], janela: &[Quadro]) -> f32 {
    let (n, m) = (referencia.len(), janela.len());
    if n == 0 || m == 0 {
        return f32::INFINITY;
    }
    let mut antes = vec![f32::INFINITY; m];
    let mut agora = vec![f32::INFINITY; m];
    for (j, q) in janela.iter().enumerate() {
        // Começo livre: a referência pode começar em qualquer quadro.
        antes[j] = distancia(&referencia[0], q);
    }
    for r in referencia.iter().skip(1) {
        agora[0] = antes[0] + distancia(r, &janela[0]);
        for j in 1..m {
            agora[j] = distancia(r, &janela[j]) + antes[j].min(agora[j - 1]).min(antes[j - 1]);
        }
        std::mem::swap(&mut antes, &mut agora);
    }
    antes[m - 1] / n as f32
}

/// A sensibilidade (1 a 9) vira o custo máximo aceito: mais sensível ouve
/// mais fácil, e erra mais.
pub fn limiar(sensibilidade: u8) -> f32 {
    let s = sensibilidade.clamp(1, 9) as f32;
    LIMIAR_MEIO * (1.0 + (s - 5.0) * 0.05)
}

/// O custo em nota de 0 a 100 (maior é mais parecido; o meio da escala, 50,
/// é o limiar da sensibilidade 5), para o teste em Ajustes.
pub fn nota(custo: f32) -> u8 {
    if !custo.is_finite() {
        return 0;
    }
    (100.0 * (1.0 - custo / (2.0 * LIMIAR_MEIO))).round().clamp(0.0, 100.0) as u8
}

/// O custo do meio da escala, medido com as vozes sintéticas (veja
/// examples/avaliar_nome.rs).
pub const LIMIAR_MEIO: f32 = 19.0;

/// O que fica ouvindo: os quadros dos últimos segundos e quando comparar.
pub struct Detector {
    referencias: Vec<Vec<Quadro>>,
    limiar: f32,
    extrator: Extrator,
    janela: VecDeque<Quadro>,
    energias: VecDeque<f32>,
    tamanho: usize,
    ruido: f32,
    quadros: usize,
    descanso: usize,
    /// O melhor custo desde a última vez que alguém perguntou (para medir).
    pub melhor: f32,
    /// Quantas amostras já chegaram (para achar, no bloco, onde o nome acabou).
    amostras: usize,
}

/// Comparar a cada 5 quadros (50 ms; eram 30 ms, com o mesmo acerto e 40%
/// menos alinhamentos); depois de reconhecer, 1,5 s sem ouvir.
const A_CADA: usize = 5;
const DESCANSO: usize = 150;

impl Detector {
    pub fn novo(r: &Referencias, sensibilidade: u8) -> Detector {
        let maior = r.referencias.iter().map(|q| q.len()).max().unwrap_or(50);
        let tamanho = maior * 3 / 2 + 10;
        Detector {
            referencias: r.referencias.clone(),
            limiar: limiar(sensibilidade),
            extrator: Extrator::default(),
            janela: VecDeque::with_capacity(tamanho),
            energias: VecDeque::with_capacity(tamanho),
            tamanho,
            ruido: 0.0,
            quadros: 0,
            descanso: 0,
            melhor: f32::INFINITY,
            amostras: 0,
        }
    }

    /// Troca o limiar (para medir: o examples/avaliar_nome.rs passa um que nunca dispara).
    pub fn definir_limiar(&mut self, limiar: f32) {
        self.limiar = limiar;
    }

    /// Depois de ouvir o próprio som (a voz da abelha), começa de novo.
    pub fn esquecer(&mut self) {
        self.janela.clear();
        self.energias.clear();
    }

    /// Recebe um bloco do microfone. Ouvindo o nome, diz onde (no bloco) ele
    /// acabou: o que vem depois já é o comando.
    pub fn alimentar(&mut self, bloco: &[f32]) -> Option<usize> {
        // Em silêncio, nem a FFT: abaixo de 2 vezes o ruído o quadro repete o último silêncio.
        self.extrator.porta = (self.ruido * 2.0).max(0.0008);
        let inicio = self.amostras;
        self.amostras += bloco.len();
        let mut ouviu = None;
        for (q, energia) in self.extrator.alimentar(bloco) {
            self.quadros += 1;
            self.ruido = if self.quadros == 1 {
                energia.min(0.002)
            } else if energia < self.ruido {
                self.ruido * 0.9 + energia * 0.1
            } else {
                self.ruido * 0.999 + energia * 0.001
            };
            if self.janela.len() == self.tamanho {
                self.janela.pop_front();
                self.energias.pop_front();
            }
            self.janela.push_back(q);
            self.energias.push_back(energia);
            if self.descanso > 0 {
                self.descanso -= 1;
                continue;
            }
            if self.quadros.is_multiple_of(A_CADA) && ouviu.is_none() && self.tem_fala() && self.comparar() {
                // O fim do quadro em que o nome terminou, contado no fluxo inteiro.
                let fim = (self.quadros - 1) * mfcc::PASSO + mfcc::JANELA;
                ouviu = Some(fim.saturating_sub(inicio).min(bloco.len()));
                self.descanso = DESCANSO;
                self.esquecer();
            }
        }
        ouviu
    }

    /// O melhor custo desde a última pergunta (e começa de novo).
    pub fn tirar_melhor(&mut self) -> f32 {
        std::mem::replace(&mut self.melhor, f32::INFINITY)
    }

    /// Só compara com fala recente na janela. O piso é baixo: num microfone
    /// baixo a voz também é baixa, e o piso antigo (0,006) a ignorava.
    fn tem_fala(&self) -> bool {
        let limiar = (self.ruido * 3.0).max(0.0015);
        let com_fala = self.energias.iter().filter(|e| **e > limiar).count();
        com_fala >= 20
    }

    fn comparar(&mut self) -> bool {
        let janela: Vec<Quadro> = self.janela.iter().copied().collect();
        let mut melhor = f32::INFINITY;
        for r in &self.referencias {
            if janela.len() < r.len() * 2 / 3 {
                continue;
            }
            // A média tirada do trecho do tamanho da referência, no fim da janela.
            let trecho = r.len().min(janela.len());
            let mut media = [0.0f32; COEFICIENTES];
            for q in &janela[janela.len() - trecho..] {
                for (m, v) in media.iter_mut().zip(q) {
                    *m += v / trecho as f32;
                }
            }
            let centrada: Vec<Quadro> = janela.iter().map(|q| std::array::from_fn(|i| q[i] - media[i])).collect();
            melhor = melhor.min(custo(r, &centrada));
        }
        self.melhor = self.melhor.min(melhor);
        melhor <= self.limiar
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Uma "palavra" de teste: três tons em sequência, com silêncio em volta.
    fn palavra(tons: &[f32], ms_cada: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; 3200];
        for &f in tons {
            v.extend((0..ms_cada * 16).map(|i| (i as f32 * f * std::f32::consts::TAU / 16_000.0).sin() * 0.3));
        }
        v.extend(vec![0.0f32; 3200]);
        v
    }

    #[test]
    fn custo_menor_para_o_mesmo_som() {
        let a = sem_media(&mfcc::de_audio(&palavra(&[400.0, 900.0, 1600.0], 150)).iter().map(|q| q.0).collect::<Vec<_>>());
        let b = sem_media(&mfcc::de_audio(&palavra(&[400.0, 900.0, 1600.0], 180)).iter().map(|q| q.0).collect::<Vec<_>>());
        let c = sem_media(&mfcc::de_audio(&palavra(&[2000.0, 300.0, 700.0], 150)).iter().map(|q| q.0).collect::<Vec<_>>());
        assert!(custo(&a, &b) < custo(&a, &c), "{} {}", custo(&a, &b), custo(&a, &c));
    }

    #[test]
    fn aprende_e_reconhece_as_gravacoes() {
        let gravacoes: Vec<Vec<f32>> = [140, 150, 165].iter().map(|ms| palavra(&[500.0, 1100.0, 1900.0], *ms)).collect();
        let r = Referencias::de_gravacoes("Zuzu", &gravacoes).unwrap();
        assert_eq!(r.referencias.len(), 3);
        let mut d = Detector::novo(&r, 5);
        d.limiar = 6.0;
        // O nome no meio de silêncio: reconhece uma vez só.
        let mut ouviu = Vec::new();
        for (i, b) in palavra(&[500.0, 1100.0, 1900.0], 155).chunks(1600).enumerate() {
            if let Some(onde) = d.alimentar(b) {
                ouviu.push(i * 1600 + onde);
            }
        }
        // Uma vez só, e logo depois do fim da palavra (3200 de silêncio + 3 × 155 ms).
        assert_eq!(ouviu.len(), 1, "{ouviu:?}");
        let fim_da_palavra = 3200 + 3 * 155 * 16;
        assert!(ouviu[0].abs_diff(fim_da_palavra) < 1600, "{} {}", ouviu[0], fim_da_palavra);
        // Outro som não.
        let mut d = Detector::novo(&r, 5);
        d.limiar = 6.0;
        assert!(!palavra(&[2400.0, 300.0, 800.0], 155).chunks(1600).any(|b| d.alimentar(b).is_some()));
    }

    #[test]
    fn gravacoes_ruins_sao_recusadas() {
        assert!(Referencias::de_gravacoes("Zuzu", &[vec![0.0; 16_000], vec![0.0; 16_000]]).is_err());
        let curtas: Vec<Vec<f32>> = (0..3).map(|_| palavra(&[500.0], 60)).collect();
        assert!(Referencias::de_gravacoes("Zuzu", &curtas).is_err());
    }

    #[test]
    fn grava_so_no_arquivo_do_perfil_e_le_de_volta() {
        let dir = std::env::temp_dir().join(format!("colmeia-voz-nome-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gravacoes: Vec<Vec<f32>> = [140, 150, 165].iter().map(|ms| palavra(&[500.0, 1100.0, 1900.0], *ms)).collect();
        let r = Referencias::de_gravacoes("Zuzu", &gravacoes).unwrap();
        assert!(r.gravar(&dir.join("outro.json")).is_err());
        assert!(r.gravar(&dir.join("nome-.json")).is_err());
        assert!(r.gravar(&dir.join("nao-existe").join("nome-1.json")).is_err());
        let destino = dir.join("nome-1.json");
        r.gravar(&destino).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&destino).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(Referencias::ler(&destino).unwrap(), r);
        std::fs::write(&destino, b"{\"nome\":\"x\"}").unwrap();
        assert!(Referencias::ler(&destino).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn o_nome_padrao_vem_com_o_programa() {
        let r = Referencias::padrao();
        assert_eq!(r.nome, "Colmeia");
        assert!(r.referencias.len() >= 4, "{}", r.referencias.len());
    }

    #[test]
    fn gravacoes_do_nome_padrao_somam_as_sinteticas() {
        let gravacoes: Vec<Vec<f32>> = [140, 150, 165].iter().map(|ms| palavra(&[500.0, 1100.0, 1900.0], *ms)).collect();
        let sinteticas = Referencias::padrao().referencias.len();
        let r = Referencias::de_gravacoes("Colmeia", &gravacoes).unwrap().com_padrao();
        assert_eq!(r.referencias.len(), 3 + sinteticas);
        let r = Referencias::de_gravacoes(" colméia ", &gravacoes).unwrap().com_padrao();
        assert_eq!(r.referencias.len(), 3 + sinteticas);
        let r = Referencias::de_gravacoes("Zuzu", &gravacoes).unwrap().com_padrao();
        assert_eq!(r.referencias.len(), 3);
    }

    #[test]
    fn nota_do_teste() {
        assert_eq!(nota(f32::INFINITY), 0);
        assert_eq!(nota(LIMIAR_MEIO), 50);
        assert_eq!(nota(0.0), 100);
        assert_eq!(nota(100.0), 0);
        assert!(nota(limiar(9)) < nota(limiar(1)));
    }

    #[test]
    fn microfone_baixo_ainda_compara() {
        // A mesma palavra a um décimo do volume (pico 0,03): também reconhece.
        let gravacoes: Vec<Vec<f32>> = [140, 150, 165].iter().map(|ms| palavra(&[500.0, 1100.0, 1900.0], *ms)).collect();
        let r = Referencias::de_gravacoes("Zuzu", &gravacoes).unwrap();
        let mut d = Detector::novo(&r, 5);
        d.limiar = 6.0;
        let baixa: Vec<f32> = palavra(&[500.0, 1100.0, 1900.0], 155).iter().map(|s| s * 0.1).collect();
        assert!(baixa.chunks(1600).any(|b| d.alimentar(b).is_some()));
    }

    #[test]
    fn sensibilidade_muda_o_limiar() {
        assert!(limiar(1) < limiar(5) && limiar(5) < limiar(9));
        assert_eq!(limiar(0), limiar(1));
        assert_eq!(limiar(20), limiar(9));
    }
}
