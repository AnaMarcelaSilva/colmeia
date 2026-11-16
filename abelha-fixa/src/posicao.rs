//! Onde a abelha fica na tela, o tamanho da conversa, o da letra dela e o tema,
//! lembrados entre uma vez e outra: arquivos pequenos na pasta de dados da Colmeia (0600, numa
//! pasta 0700).

use std::path::PathBuf;

use eframe::egui::{Pos2, Vec2, pos2};

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

fn arquivo_tamanho(perfil: i64) -> Option<PathBuf> {
    Some(pasta()?.join(format!("abelha-{perfil}.tamanho")))
}

fn arquivo_letra(perfil: i64) -> Option<PathBuf> {
    Some(pasta()?.join(format!("abelha-{perfil}.letra")))
}

fn arquivo_tema(perfil: i64) -> Option<PathBuf> {
    Some(pasta()?.join(format!("abelha-{perfil}.tema")))
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
    if let Some(caminho) = arquivo(perfil) {
        gravar_em(&caminho, p);
    }
}

/// O tamanho da conversa (largura e altura em pontos), se já foi puxada.
pub fn ler_tamanho(perfil: i64) -> Option<Vec2> {
    ler_texto(&std::fs::read_to_string(arquivo_tamanho(perfil)?).ok()?).map(|p| p.to_vec2())
}

pub fn gravar_tamanho(perfil: i64, t: Vec2) {
    if let Some(caminho) = arquivo_tamanho(perfil) {
        gravar_em(&caminho, t.to_pos2());
    }
}

/// A escala da letra da conversa (1 é a normal), se já foi mudada.
pub fn ler_letra(perfil: i64) -> Option<f32> {
    ler_numero(&std::fs::read_to_string(arquivo_letra(perfil)?).ok()?)
}

pub fn gravar_letra(perfil: i64, escala: f32) {
    if let Some(caminho) = arquivo_letra(perfil) {
        gravar_texto(&caminho, &format!("{escala:.2}\n"));
    }
}

/// O nome do tema da abelha, se já foi escolhido (só letras, até 20).
pub fn ler_tema(perfil: i64) -> Option<String> {
    let texto = std::fs::read_to_string(arquivo_tema(perfil)?).ok()?;
    let nome = texto.trim();
    (!nome.is_empty() && nome.chars().count() <= 20 && nome.chars().all(char::is_alphabetic)).then(|| nome.to_string())
}

pub fn gravar_tema(perfil: i64, nome: &str) {
    if let Some(caminho) = arquivo_tema(perfil) {
        gravar_texto(&caminho, &format!("{nome}\n"));
    }
}

/// Um número só, finito e entre 0,5 e 3; qualquer outra coisa é ignorada.
fn ler_numero(texto: &str) -> Option<f32> {
    let mut partes = texto.split_whitespace();
    let v = partes.next()?.parse::<f32>().ok()?;
    (partes.next().is_none() && v.is_finite() && (0.5..=3.0).contains(&v)).then_some(v)
}

fn gravar_em(caminho: &std::path::Path, p: Pos2) {
    gravar_texto(caminho, &format!("{:.0} {:.0}\n", p.x, p.y));
}

fn gravar_texto(caminho: &std::path::Path, texto: &str) {
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
    if let Ok(mut f) = abrir.open(caminho) {
        let _ = std::io::Write::write_all(&mut f, texto.as_bytes());
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

    #[test]
    fn letra_e_um_numero_so_dentro_do_razoavel() {
        assert_eq!(ler_numero("1.15\n"), Some(1.15));
        for ruim in ["", "0.4", "4", "1 2", "a", "NaN"] {
            assert_eq!(ler_numero(ruim), None, "{ruim:?}");
        }
    }
}
