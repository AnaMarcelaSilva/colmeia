//! O Whisper (whisper.cpp), só na CPU e só neste processo. O modelo é o que
//! o núcleo baixou e conferiu; carregar leva um instante e fica na memória
//! até o processo sair por falta de uso.

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const NAO_TRANSCREVEU: &str = "Não consegui entender o que foi dito agora. Tente de novo.";

pub struct Transcritor {
    contexto: WhisperContext,
}

pub fn carregar(modelo: &Path) -> Result<Transcritor, String> {
    if !modelo.is_file() {
        return Err("O modelo de voz não está no computador. Ligue o ditado de novo em Ajustes para baixá-lo.".into());
    }
    let mut parametros = WhisperContextParameters::default();
    parametros.use_gpu(false);
    WhisperContext::new_with_params(modelo, parametros)
        .map(|contexto| Transcritor { contexto })
        .map_err(|e| crate::falha("O modelo de voz não abriu. Desligue e ligue o ditado em Ajustes para baixá-lo de novo.", e))
}

/// Metade dos núcleos, de 1 a 4: transcrever não pode travar o resto do computador.
fn linhas_de_execucao() -> i32 {
    let nucleos = std::thread::available_parallelism().map_or(2, |n| n.get());
    (nucleos / 2).clamp(1, 4) as i32
}

/// A busca em feixe com 5 caminhos: com o vocabulário do perfil no prompt,
/// acerta mais os nomes que a busca gulosa (que fica com a primeira palavra
/// provável, como "go meia" no lugar de "Colmeia"). Custa mais tempo; a
/// medida está em docs/decisoes/0016.
pub const FEIXE: u8 = 5;

impl Transcritor {
    /// Transcreve áudio de 16 kHz em um canal, sempre em português, com o
    /// vocabulário do perfil (`prompt`) como contexto.
    pub fn transcrever(&self, audio: &[f32], prompt: &str) -> Result<String, String> {
        self.transcrever_com(audio, prompt, FEIXE)
    }

    /// Com `feixe` 0 ou 1, a busca gulosa (para medir a diferença).
    pub fn transcrever_com(&self, audio: &[f32], prompt: &str, feixe: u8) -> Result<String, String> {
        let mut estado = self.contexto.create_state().map_err(|e| crate::falha(NAO_TRANSCREVEU, e))?;
        let estrategia =
            if feixe > 1 { SamplingStrategy::BeamSearch { beam_size: feixe as i32, patience: -1.0 } } else { SamplingStrategy::Greedy { best_of: 1 } };
        let mut p = FullParams::new(estrategia);
        p.set_language(Some("pt"));
        p.set_detect_language(false);
        p.set_translate(false);
        p.set_n_threads(linhas_de_execucao());
        p.set_no_context(true);
        p.set_no_timestamps(true);
        p.set_single_segment(audio.len() < 30 * 16_000);
        p.set_suppress_blank(true);
        p.set_suppress_nst(true);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        let prompt = prompt.trim();
        if !prompt.is_empty() {
            p.set_initial_prompt(prompt);
        }
        estado.full(p, audio).map_err(|e| crate::falha(NAO_TRANSCREVEU, e))?;
        let partes: Vec<String> = estado.as_iter().filter_map(|s| s.to_str_lossy().ok().map(|t| t.into_owned())).collect();
        Ok(limpar(&partes))
    }
}

/// Junta os trechos num texto só, sem as marcas que o Whisper põe no lugar de
/// sons ("[Música]", "(risos)") nem espaços sobrando.
pub fn limpar(trechos: &[String]) -> String {
    let mut texto = String::new();
    for trecho in trechos {
        let t = trecho.trim();
        let marca = (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('*') && t.ends_with('*'));
        if t.is_empty() || marca {
            continue;
        }
        if !texto.is_empty() {
            texto.push(' ');
        }
        texto.push_str(t);
    }
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod testes {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn junta_os_trechos_sem_as_marcas_de_som() {
        assert_eq!(limpar(&s(&[" Abra a tarefa", "  de login.  "])), "Abra a tarefa de login.");
        assert_eq!(limpar(&s(&["[Música]", " (risos) ", "Rode os testes.", "*tosse*"])), "Rode os testes.");
        assert_eq!(limpar(&s(&["[BLANK_AUDIO]"])), "");
        assert_eq!(limpar(&s(&["um   dois\n três"])), "um dois três");
    }

    #[test]
    fn modelo_que_nao_existe_tem_mensagem_clara() {
        let Err(e) = carregar(Path::new("/nao/existe/ggml-base.bin")) else { panic!("carregou") };
        assert!(e.contains("Ajustes"), "{e}");
    }

    #[test]
    fn usa_no_maximo_quatro_nucleos() {
        assert!((1..=4).contains(&linhas_de_execucao()));
    }
}
