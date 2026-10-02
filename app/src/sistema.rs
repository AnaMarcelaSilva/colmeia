//! Abrir a pasta da tarefa fora da Colmeia: no editor (IntelliJ ou VS Code) ou
//! no gerenciador de arquivos. Os programas são chamados direto, sem shell, e
//! a pasta vai como um argumento só.

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
    let programa = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    abrir_com(programa, pasta)
}
