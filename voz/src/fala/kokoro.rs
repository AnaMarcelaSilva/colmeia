//! O Kokoro-82M pelo ONNX Runtime: dos fonemas à onda, a 24 kHz. O runtime
//! (uma biblioteca da Microsoft, MIT) é carregado do arquivo que o núcleo
//! baixou e conferiu, só quando a abelha fala pela primeira vez; o modelo
//! (quantizado em 8 bits, ~92 MB) sai da memória quando a fala fica parada.

use std::path::Path;

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

pub const TAXA: u32 = 24_000;

/// Os fonemas que o Kokoro conhece e o número de cada um (o tokenizer.json
/// do modelo). O 0 abre e fecha a frase.
const TABELA: [(char, i64); 114] = [
    (';', 1),
    (':', 2),
    (',', 3),
    ('.', 4),
    ('!', 5),
    ('?', 6),
    ('—', 9),
    ('…', 10),
    ('"', 11),
    ('(', 12),
    (')', 13),
    ('“', 14),
    ('”', 15),
    (' ', 16),
    ('\u{303}', 17),
    ('ʣ', 18),
    ('ʥ', 19),
    ('ʦ', 20),
    ('ʨ', 21),
    ('ᵝ', 22),
    ('ꭧ', 23),
    ('A', 24),
    ('I', 25),
    ('O', 31),
    ('Q', 33),
    ('S', 35),
    ('T', 36),
    ('W', 39),
    ('Y', 41),
    ('ᵊ', 42),
    ('a', 43),
    ('b', 44),
    ('c', 45),
    ('d', 46),
    ('e', 47),
    ('f', 48),
    ('h', 50),
    ('i', 51),
    ('j', 52),
    ('k', 53),
    ('l', 54),
    ('m', 55),
    ('n', 56),
    ('o', 57),
    ('p', 58),
    ('q', 59),
    ('r', 60),
    ('s', 61),
    ('t', 62),
    ('u', 63),
    ('v', 64),
    ('w', 65),
    ('x', 66),
    ('y', 67),
    ('z', 68),
    ('ɑ', 69),
    ('ɐ', 70),
    ('ɒ', 71),
    ('æ', 72),
    ('β', 75),
    ('ɔ', 76),
    ('ɕ', 77),
    ('ç', 78),
    ('ɖ', 80),
    ('ð', 81),
    ('ʤ', 82),
    ('ə', 83),
    ('ɚ', 85),
    ('ɛ', 86),
    ('ɜ', 87),
    ('ɟ', 90),
    ('ɡ', 92),
    ('ɥ', 99),
    ('ɨ', 101),
    ('ɪ', 102),
    ('ʝ', 103),
    ('ɯ', 110),
    ('ɰ', 111),
    ('ŋ', 112),
    ('ɳ', 113),
    ('ɲ', 114),
    ('ɴ', 115),
    ('ø', 116),
    ('ɸ', 118),
    ('θ', 119),
    ('œ', 120),
    ('ɹ', 123),
    ('ɾ', 125),
    ('ɻ', 126),
    ('ʁ', 128),
    ('ɽ', 129),
    ('ʂ', 130),
    ('ʃ', 131),
    ('ʈ', 132),
    ('ʧ', 133),
    ('ʊ', 135),
    ('ʋ', 136),
    ('ʌ', 138),
    ('ɣ', 139),
    ('ɤ', 140),
    ('χ', 142),
    ('ʎ', 143),
    ('ʒ', 147),
    ('ʔ', 148),
    ('ˈ', 156),
    ('ˌ', 157),
    ('ː', 158),
    ('ʰ', 162),
    ('ʲ', 164),
    ('↓', 169),
    ('→', 171),
    ('↗', 172),
    ('↘', 173),
    ('ᵻ', 177),
];

/// Só os caracteres, para conferir o que os fonemas produzem.
#[cfg(test)]
pub const VOCABULARIO: [char; 114] = {
    let mut v = [' '; 114];
    let mut i = 0;
    while i < 114 {
        v[i] = TABELA[i].0;
        i += 1;
    }
    v
};

/// O modelo aceita até 512 números por vez, contando os dois zeros.
pub const MAXIMO_FONEMAS: usize = 510;

/// Uma voz: 510 estilos de 256 números, um para cada comprimento de frase.
const ESTILO: usize = 256;

pub fn numeros(fonemas: &str) -> Vec<i64> {
    let mut v = vec![0];
    v.extend(fonemas.chars().filter_map(|c| TABELA.iter().find(|(t, _)| *t == c).map(|(_, n)| *n)).take(MAXIMO_FONEMAS));
    v.push(0);
    v
}

pub struct Kokoro {
    sessao: Session,
    estilos: Vec<f32>,
}

/// Abre o runtime uma vez por processo (ele não sai mais da memória até o
/// colmeia-voz sair) e o modelo, que sai junto com o Kokoro.
pub fn carregar(modelo: &Path, voz: &Path, runtime: &Path, linhas: usize) -> Result<Kokoro, String> {
    let ajuda = "Desligue e ligue a voz da abelha em Ajustes › Voz para baixá-la de novo.";
    for arquivo in [modelo, voz, runtime] {
        if !arquivo.is_file() {
            return Err(format!("A voz da abelha não está completa neste computador. {ajuda}"));
        }
    }
    let estilos = ler_voz(voz).map_err(|e| format!("{e} {ajuda}"))?;
    let ambiente = ort::init_from(runtime)
        .map_err(|e| format!("O ONNX Runtime não abriu ({e}). {ajuda}"))?
        .with_name("colmeia-voz")
        .build()
        .map_err(|e| format!("O ONNX Runtime não abriu ({e})."))?;
    let falhou = |e: ort::Error| format!("O modelo da voz não abriu ({e}). {ajuda}");
    let sessao = Session::builder(&ambiente)
        .map_err(falhou)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| falhou(e.into()))?
        .with_intra_threads(linhas)
        .map_err(|e| falhou(e.into()))?
        .with_inter_threads(1)
        .map_err(|e| falhou(e.into()))?
        // Sem girar à toa entre uma frase e outra: a CPU volta a zero.
        .with_intra_op_spinning(false)
        .map_err(|e| falhou(e.into()))?
        .with_inter_op_spinning(false)
        .map_err(|e| falhou(e.into()))?
        .commit_from_file(modelo)
        .map_err(falhou)?;
    Ok(Kokoro { sessao, estilos })
}

/// Os estilos de uma voz: 510 linhas de 256 números (float32, little-endian).
fn ler_voz(voz: &Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(voz).map_err(|e| format!("Não consegui ler a voz ({e})."))?;
    if bytes.len() % (ESTILO * 4) != 0 || bytes.is_empty() {
        return Err("O arquivo da voz não é o esperado.".into());
    }
    Ok(bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
}

impl Kokoro {
    /// Troca a voz sem abrir o modelo de novo (para gerar as referências do
    /// nome padrão com várias vozes).
    pub fn trocar_voz(&mut self, voz: &Path) -> Result<(), String> {
        self.estilos = ler_voz(voz)?;
        Ok(())
    }

    /// A onda (24 kHz, um canal) de uma frase em fonemas.
    pub fn falar(&mut self, fonemas: &str, velocidade: f32) -> Result<Vec<f32>, String> {
        let ids = numeros(fonemas);
        let total = self.estilos.len() / ESTILO;
        let linha = (ids.len() - 2).min(total - 1);
        let estilo = self.estilos[linha * ESTILO..(linha + 1) * ESTILO].to_vec();
        let falhou = |e: ort::Error| format!("Não consegui falar ({e}).");
        let entradas = ort::inputs![
            "input_ids" => Tensor::from_array(([1usize, ids.len()], ids)).map_err(falhou)?,
            "style" => Tensor::from_array(([1usize, ESTILO], estilo)).map_err(falhou)?,
            "speed" => Tensor::from_array(([1usize], vec![velocidade])).map_err(falhou)?
        ];
        let saidas = self.sessao.run(entradas).map_err(falhou)?;
        let (_, onda) = saidas[0].try_extract_tensor::<f32>().map_err(falhou)?;
        Ok(onda.to_vec())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn numeros_dos_fonemas() {
        assert_eq!(numeros("ɐ̃ ˈa."), vec![0, 70, 17, 16, 156, 43, 4, 0]);
        // O que não está na tabela fica de fora.
        assert_eq!(numeros("a#b"), vec![0, 43, 44, 0]);
        assert_eq!(numeros(&"a".repeat(600)).len(), MAXIMO_FONEMAS + 2);
    }

    #[test]
    fn sem_arquivos_a_mensagem_e_clara() {
        let Err(e) = carregar(Path::new("/nao/existe.onnx"), Path::new("/nao/voz.bin"), Path::new("/nao/libonnxruntime.so"), 1) else { panic!() };
        assert!(e.contains("Ajustes"), "{e}");
    }
}
