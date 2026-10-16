//! Onde a abelha fica na tela, lembrado entre uma vez e outra: um arquivo
//! pequeno na pasta de dados da Colmeia (0600, numa pasta 0700).

use std::path::PathBuf;

use eframe::egui::{Pos2, pos2};

/// A mesma pasta do núcleo: COLMEIA_DADOS, $XDG_DATA_HOME/colmeia ou
/// ~/.local/share/colmeia (no Windows, %LOCALAPPDATA%\Colmeia\dados).
fn pasta() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("COLMEIA_DADOS") {
        return Some(PathBuf::from(d));
    }
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Colmeia").join("dados"));
    }
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(d).join("colmeia"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share").join("colmeia"))
}

fn arquivo(perfil: i64) -> Option<PathBuf> {
    Some(pasta()?.join(format!("abelha-{perfil}.posicao")))
}

/// "x y" em pontos da tela; qualquer outra coisa é ignorada.
fn ler_texto(texto: &str) -> Option<Pos2> {
    let mut partes = texto.split_whitespace().map(|p| p.parse::<f32>().ok().filter(|v| v.is_finite()));
    let (x, y) = (partes.next()??, partes.next()??);
    partes.next().is_none().then_some(pos2(x, y))
}

pub fn ler(perfil: i64) -> Option<Pos2> {
    ler_texto(&std::fs::read_to_string(arquivo(perfil)?).ok()?)
}

pub fn gravar(perfil: i64, p: Pos2) {
    let Some(caminho) = arquivo(perfil) else { return };
    if let Some(pasta) = caminho.parent() {
        let mut criar = std::fs::DirBuilder::new();
        criar.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut criar, 0o700);
        if criar.create(pasta).is_err() {
            return;
        }
    }
    let mut abrir = std::fs::OpenOptions::new();
    abrir.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut abrir, 0o600);
    if let Ok(mut f) = abrir.open(&caminho) {
        let _ = std::io::Write::write_all(&mut f, format!("{:.0} {:.0}\n", p.x, p.y).as_bytes());
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_so_dois_numeros() {
        assert_eq!(ler_texto("120 840\n"), Some(pos2(120.0, 840.0)));
        assert_eq!(ler_texto("-4.5 10"), Some(pos2(-4.5, 10.0)));
        for ruim in ["", "12", "a b", "1 2 3", "NaN 2", "inf 3"] {
            assert_eq!(ler_texto(ruim), None, "{ruim:?}");
        }
    }
}
