//! Canal local com o núcleo: socket Unix num diretório só do usuário e um token
//! lido do arquivo a cada conexão (nunca de variável de ambiente ou argumento,
//! que apareceriam para outros processos). Nenhuma porta de rede é usada.

use std::io;
use std::path::PathBuf;

const NOME_SOCKET: &str = "nucleo.sock";
const NOME_TOKEN: &str = "token";

/// O mesmo diretório que o núcleo usa: COLMEIA_DIR, ou $XDG_RUNTIME_DIR/colmeia,
/// ou o cache do usuário.
pub fn diretorio() -> PathBuf {
    if let Some(d) = std::env::var_os("COLMEIA_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("colmeia")
}

fn ler_token() -> io::Result<String> {
    Ok(std::fs::read_to_string(diretorio().join(NOME_TOKEN))?.trim().to_string())
}

#[cfg(unix)]
pub use unix::*;

#[cfg(unix)]
mod unix {
    use std::io::{self, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::HeaderValue;
    use tungstenite::WebSocket;

    use super::{NOME_SOCKET, diretorio, ler_token};

    fn conectar() -> io::Result<UnixStream> {
        UnixStream::connect(diretorio().join(NOME_SOCKET))
    }

    /// Abre o WebSocket de um caminho da API (ex.: "/v1/terminais/0").
    pub fn websocket(caminho: &str) -> Result<WebSocket<UnixStream>, String> {
        let fluxo = conectar().map_err(|e| format!("núcleo indisponível: {e}"))?;
        let token = ler_token().map_err(|e| format!("sem token do núcleo: {e}"))?;
        let mut pedido = format!("ws://colmeia{caminho}").into_client_request().map_err(|e| e.to_string())?;
        let valor = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| e.to_string())?;
        pedido.headers_mut().insert("Authorization", valor);
        tungstenite::client(pedido, fluxo).map(|(ws, _)| ws).map_err(|e| format!("recusado pelo núcleo: {e}"))
    }

    /// Pedido HTTP simples ao núcleo; devolve o corpo da resposta.
    pub fn pedir(metodo: &str, caminho: &str) -> Result<String, String> {
        let mut fluxo = conectar().map_err(|e| e.to_string())?;
        let token = ler_token().map_err(|e| e.to_string())?;
        fluxo.set_read_timeout(Some(Duration::from_secs(3))).ok();
        write!(
            fluxo,
            "{metodo} {caminho} HTTP/1.1\r\nHost: colmeia\r\nAuthorization: Bearer {token}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .map_err(|e| e.to_string())?;
        let mut resposta = String::new();
        fluxo.read_to_string(&mut resposta).map_err(|e| e.to_string())?;
        let (cabecalho, corpo) = resposta.split_once("\r\n\r\n").unwrap_or((&resposta, ""));
        if !cabecalho.starts_with("HTTP/1.1 2") {
            return Err(cabecalho.lines().next().unwrap_or_default().to_string());
        }
        Ok(corpo.to_string())
    }

    /// Procura o executável do núcleo: COLMEIA_NUCLEO, ao lado do app ou no PATH.
    fn executavel_nucleo() -> Option<PathBuf> {
        if let Some(c) = std::env::var_os("COLMEIA_NUCLEO") {
            return Some(PathBuf::from(c));
        }
        let vizinho = std::env::current_exe().ok()?.with_file_name("colmeia-nucleo");
        if vizinho.is_file() {
            return Some(vizinho);
        }
        std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("colmeia-nucleo")).find(|c| c.is_file()))
    }

    /// Se o núcleo não estiver rodando, inicia um em segundo plano e espera o
    /// canal ficar pronto. Ele continua rodando depois que a tela fecha: é dono
    /// dos terminais. COLMEIA_DEMO=1 inicia com as cargas de teste ligadas.
    pub fn garantir_nucleo() -> Result<(), String> {
        if pedir("GET", "/v1/versao").is_ok() {
            return Ok(());
        }
        let caminho = executavel_nucleo().ok_or("não encontrei o executável colmeia-nucleo (defina COLMEIA_NUCLEO)")?;
        let mut comando = Command::new(&caminho);
        if std::env::var("COLMEIA_DEMO").is_ok_and(|v| v == "1") {
            comando.arg("--demo");
        }
        use std::os::unix::process::CommandExt;
        comando.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
        comando.spawn().map_err(|e| format!("iniciando {}: {e}", caminho.display()))?;
        let limite = Instant::now() + Duration::from_secs(5);
        while Instant::now() < limite {
            if pedir("GET", "/v1/versao").is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("o núcleo não respondeu a tempo".into())
    }
}

#[cfg(not(unix))]
pub use outros::*;

#[cfg(not(unix))]
mod outros {
    //! No Windows o canal será um named pipe (ainda não feito).
    pub type Fluxo = std::net::TcpStream;
    const AVISO: &str = "canal local no Windows ainda não implementado (named pipe)";

    pub fn websocket(_: &str) -> Result<tungstenite::WebSocket<Fluxo>, String> {
        Err(AVISO.into())
    }
    pub fn pedir(_: &str, _: &str) -> Result<String, String> {
        Err(AVISO.into())
    }
    pub fn garantir_nucleo() -> Result<(), String> {
        Err(AVISO.into())
    }
}
