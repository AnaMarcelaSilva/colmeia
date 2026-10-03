//! Abrir a pasta da tarefa fora da Colmeia: no editor (IntelliJ ou VS Code) ou
//! no gerenciador de arquivos; e um vídeo anexado, no reprodutor do sistema.
//! Os programas são chamados direto, sem shell, e a pasta (ou o arquivo) vai
//! como um argumento só.

use std::path::Path;
use std::process::{Command, Stdio};

/// Um comando sem as variáveis da Colmeia (COLMEIA_DIR, COLMEIA_DADOS,
/// COLMEIA_NUCLEO…): o programa aberto não precisa saber onde ficam o
/// núcleo e os dados.
fn comando_limpo(programa: &str) -> Command {
    let mut comando = Command::new(programa);
    for (nome, _) in std::env::vars_os() {
        if nome.to_str().is_some_and(|n| n.starts_with("COLMEIA_")) {
            comando.env_remove(&nome);
        }
    }
    comando
}

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
    let mut comando = comando_limpo(programa);
    comando.arg(pasta).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Em grupo próprio: fechar a Colmeia não fecha o editor.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut comando, 0);
    let mut filho = comando.spawn().map_err(|e| e.to_string())?;
    // Recolhe o processo quando ele terminar, para não sobrar processo zumbi.
    std::thread::spawn(move || filho.wait());
    Ok(())
}

/// Abre um arquivo da pasta da tarefa no editor, sem esperar.
pub fn abrir_arquivo_com(programa: &str, arquivo: &str) -> Result<(), String> {
    if !Path::new(arquivo).is_file() {
        return Err("o arquivo não está mais lá".into());
    }
    let mut comando = comando_limpo(programa);
    comando.arg(arquivo).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut comando, 0);
    let mut filho = comando.spawn().map_err(|e| e.to_string())?;
    std::thread::spawn(move || filho.wait());
    Ok(())
}

/// Extensões que o "Abrir no sistema" aceita: documentos, imagens e mídia.
/// Fora da lista (scripts, .desktop, executáveis, arquivos sem extensão)
/// só abre no editor: o abridor do sistema poderia executar.
const EXTENSOES_DO_SISTEMA: [&str; 24] = [
    "pdf", "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "txt", "md", "csv", "json", "xml", "log", "odt", "ods", "odp", "docx", "xlsx", "pptx", "mp4",
    "webm", "mkv", "mov",
];

/// Diz se o arquivo pode ir para o abridor do sistema (pela extensão, de uma lista fixa).
pub fn abre_no_sistema(nome: &str) -> bool {
    let Some((base, extensao)) = nome.rsplit_once('.') else { return false };
    !base.is_empty() && EXTENSOES_DO_SISTEMA.contains(&extensao.to_ascii_lowercase().as_str())
}

/// Abre um arquivo da pasta da tarefa com o aplicativo padrão (só os da lista).
pub fn abrir_no_sistema(arquivo: &str) -> Result<(), String> {
    let nome = Path::new(arquivo).file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if !abre_no_sistema(nome) {
        return Err("este tipo de arquivo só abre no editor".into());
    }
    abrir_arquivo_com(abridor(), arquivo)
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
    let mut comando = comando_limpo(abridor());
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

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_documentos_e_imagens_vao_para_o_abridor_do_sistema() {
        for nome in ["relatorio.PDF", "tela.png", "README.md", "dados.csv"] {
            assert!(abre_no_sistema(nome), "{nome}");
        }
        for nome in ["instalar.sh", "atalho.desktop", "programa", "Makefile", ".env", "app.exe", "script.py", "pagina.html", ".png"] {
            assert!(!abre_no_sistema(nome), "{nome}");
        }
    }
}
