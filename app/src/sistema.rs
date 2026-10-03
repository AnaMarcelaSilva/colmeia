//! Abrir a pasta da tarefa fora da Colmeia: no editor (IntelliJ ou VS Code) ou
//! no gerenciador de arquivos; e um vídeo anexado, no reprodutor do sistema.
//! Os programas são chamados direto, sem shell, e a pasta (ou o arquivo) vai
//! como um argumento só.

use std::path::Path;
use std::process::{Command, Stdio};

/// Editores procurados no PATH, na ordem: nome para mostrar e comando.
const EDITORES: [(&str, &str); 2] = [("IntelliJ", "idea"), ("VS Code", "code")];

/// O primeiro editor instalado, se houver. `COLMEIA_EDITOR=comando` escolhe outro.
pub fn editor() -> Option<(&'static str, &'static str)> {
    if let Ok(comando) = std::env::var("COLMEIA_EDITOR")
        && !comando.is_empty()
    {
        return Some(("editor", Box::leak(comando.into_boxed_str())));
    }
    EDITORES.into_iter().find(|(_, comando)| no_path(comando))
}

fn no_path(comando: &str) -> bool {
    let Some(caminhos) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&caminhos).any(|dir| {
        let caminho = dir.join(comando);
        caminho.is_file() || (cfg!(windows) && dir.join(format!("{comando}.cmd")).is_file())
    })
}

/// Abre `pasta` com o programa, sem esperar ele terminar.
pub fn abrir_com(programa: &str, pasta: &str) -> Result<(), String> {
    if !Path::new(pasta).is_dir() {
        return Err(format!("a pasta {pasta} não existe"));
    }
    let mut comando = Command::new(programa);
    comando.arg(pasta).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Em grupo próprio: fechar a Colmeia não fecha o editor.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut comando, 0);
    let mut filho = comando.spawn().map_err(|e| e.to_string())?;
    // Recolhe o processo quando ele terminar, para não sobrar processo zumbi.
    std::thread::spawn(move || filho.wait());
    Ok(())
}

pub fn abrir_pasta(pasta: &str) -> Result<(), String> {
    abrir_com(abridor(), pasta)
}

/// Programa que abre um arquivo com o aplicativo padrão do sistema.
fn abridor() -> &'static str {
    if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Abre um arquivo (um vídeo anexado) com o aplicativo padrão, sem esperar.
/// O caminho vem do núcleo, montado pelo hash e com extensão de vídeo.
/// `ao_falhar` é chamado (numa thread) se o abridor terminar com erro, o que
/// quer dizer que não há programa para o tipo do arquivo.
pub fn abrir_arquivo(arquivo: &str, ao_falhar: impl FnOnce(String) + Send + 'static) -> Result<(), String> {
    if !Path::new(arquivo).is_file() {
        return Err("o arquivo não está mais lá".into());
    }
    let mut comando = Command::new(abridor());
    comando.arg(arquivo).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut comando, 0);
    let mut filho = comando.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => SEM_REPRODUTOR.to_string(),
        _ => e.to_string(),
    })?;
    // Recolhe o processo; um código de erro do xdg-open quer dizer que ele
    // não achou um programa para o tipo do arquivo.
    std::thread::spawn(move || {
        if filho.wait().is_ok_and(|s| !s.success()) {
            ao_falhar(SEM_REPRODUTOR.to_string());
        }
    });
    Ok(())
}

pub const SEM_REPRODUTOR: &str = "Não achei um reprodutor de vídeo. Instale um, por exemplo mpv ou VLC.";
