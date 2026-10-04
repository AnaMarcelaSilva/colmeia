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

    use tungstenite::WebSocket;
    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::HeaderValue;

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

    /// Pedido HTTP simples ao núcleo; devolve o corpo se a resposta for 2xx.
    pub fn pedir(metodo: &str, caminho: &str) -> Result<String, String> {
        match pedir_com_corpo(metodo, caminho, None)? {
            (200..=299, corpo) => Ok(corpo),
            (status, corpo) => Err(format!("{status}: {corpo}")),
        }
    }

    /// Pedido HTTP com corpo JSON opcional; devolve o status e o corpo.
    pub fn pedir_com_corpo(metodo: &str, caminho: &str, corpo: Option<&str>) -> Result<(u16, String), String> {
        pedir_com_corpo_ate(metodo, caminho, corpo, Duration::from_secs(10))
    }

    /// O mesmo, esperando a resposta até `espera` (uma consulta ao banco pode
    /// levar o tempo-limite inteiro dela).
    pub fn pedir_com_corpo_ate(metodo: &str, caminho: &str, corpo: Option<&str>, espera: Duration) -> Result<(u16, String), String> {
        let (status, corpo) = pedir_bytes_ate(metodo, caminho, "application/json", corpo.unwrap_or("").as_bytes(), espera)?;
        Ok((status, String::from_utf8_lossy(&corpo).into_owned()))
    }

    /// Pedido HTTP com corpo em bytes (uma imagem, por exemplo); devolve o
    /// status e o corpo da resposta, também em bytes.
    pub fn pedir_bytes(metodo: &str, caminho: &str, tipo: &str, corpo: &[u8]) -> Result<(u16, Vec<u8>), String> {
        pedir_bytes_ate(metodo, caminho, tipo, corpo, Duration::from_secs(10))
    }

    fn pedir_bytes_ate(metodo: &str, caminho: &str, tipo: &str, corpo: &[u8], espera: Duration) -> Result<(u16, Vec<u8>), String> {
        let mut fluxo = conectar().map_err(|e| format!("núcleo indisponível: {e}"))?;
        let token = ler_token().map_err(|e| format!("sem token do núcleo: {e}"))?;
        fluxo.set_read_timeout(Some(espera)).ok();
        let cabecalho = format!(
            "{metodo} {caminho} HTTP/1.0\r\nHost: colmeia\r\nAuthorization: Bearer {token}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            corpo.len()
        );
        fluxo.write_all(cabecalho.as_bytes()).and_then(|_| fluxo.write_all(corpo)).map_err(|e| e.to_string())?;
        // Em HTTP/1.0 o servidor não divide a resposta em blocos: o corpo é o resto.
        ler_resposta(&mut fluxo)
    }

    /// Envia um arquivo como corpo, direto do disco e em partes (um vídeo de
    /// centenas de MB não passa pela memória). `progresso` recebe os bytes já
    /// enviados e o total. Devolve o status e o corpo da resposta.
    pub fn pedir_arquivo(
        metodo: &str,
        caminho: &str,
        tipo: &str,
        arquivo: &std::path::Path,
        mut progresso: impl FnMut(u64, u64),
    ) -> Result<(u16, Vec<u8>), String> {
        let mut origem = std::fs::File::open(arquivo).map_err(|e| format!("não consegui abrir o arquivo: {e}"))?;
        let total = origem.metadata().map_err(|e| e.to_string())?.len();
        let mut fluxo = conectar().map_err(|e| format!("núcleo indisponível: {e}"))?;
        let token = ler_token().map_err(|e| format!("sem token do núcleo: {e}"))?;
        // O núcleo só responde depois de gravar tudo: espera mais que nos outros pedidos.
        fluxo.set_read_timeout(Some(Duration::from_secs(120))).ok();
        let cabecalho = format!(
            "{metodo} {caminho} HTTP/1.0\r\nHost: colmeia\r\nAuthorization: Bearer {token}\r\nContent-Type: {tipo}\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
        );
        fluxo.write_all(cabecalho.as_bytes()).map_err(|e| e.to_string())?;
        let mut parte = vec![0u8; 256 << 10];
        let mut enviados = 0u64;
        progresso(0, total);
        while enviados < total {
            let n = origem.read(&mut parte).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("o arquivo mudou durante o envio".into());
            }
            let n = n.min((total - enviados) as usize);
            if let Err(e) = fluxo.write_all(&parte[..n]) {
                // O núcleo recusou no meio (tipo errado, grande demais): a resposta explica.
                return ler_resposta(&mut fluxo).map_err(|_| e.to_string());
            }
            enviados += n as u64;
            progresso(enviados, total);
        }
        ler_resposta(&mut fluxo)
    }

    fn ler_resposta(fluxo: &mut UnixStream) -> Result<(u16, Vec<u8>), String> {
        let mut resposta = Vec::new();
        fluxo.read_to_end(&mut resposta).map_err(|e| e.to_string())?;
        let fim = resposta.windows(4).position(|j| j == b"\r\n\r\n").ok_or("resposta inválida do núcleo")?;
        let cabecalho = String::from_utf8_lossy(&resposta[..fim]);
        let status = cabecalho.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or("resposta inválida do núcleo")?;
        Ok((status, resposta[fim + 4..].to_vec()))
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

    /// Os casos comuns em português; o resto como o sistema descreve.
    fn erro_ao_iniciar(e: &std::io::Error) -> String {
        match e.kind() {
            std::io::ErrorKind::NotFound => "arquivo não encontrado".into(),
            std::io::ErrorKind::PermissionDenied => "sem permissão para executar".into(),
            _ => e.to_string(),
        }
    }

    /// Se o núcleo não estiver rodando, inicia um em segundo plano e espera o
    /// canal ficar pronto (até 5 s; chame fora da thread da tela). Ele
    /// continua rodando depois que a tela fecha: é dono dos terminais.
    /// COLMEIA_DEMO=1 inicia com as cargas de teste ligadas.
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
        let mut filho = comando.spawn().map_err(|e| format!("iniciando {}: {}", caminho.display(), erro_ao_iniciar(&e)))?;
        // Recolhe o filho quando ele sair; sem isso, um núcleo que caiu fica
        // como zumbi até a tela fechar. A thread só espera e não segura a saída.
        let _ = std::thread::Builder::new().name("nucleo-filho".into()).spawn(move || {
            let _ = filho.wait();
        });
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
    pub fn pedir_com_corpo(_: &str, _: &str, _: Option<&str>) -> Result<(u16, String), String> {
        Err(AVISO.into())
    }
    pub fn pedir_com_corpo_ate(_: &str, _: &str, _: Option<&str>, _: std::time::Duration) -> Result<(u16, String), String> {
        Err(AVISO.into())
    }
    pub fn pedir_bytes(_: &str, _: &str, _: &str, _: &[u8]) -> Result<(u16, Vec<u8>), String> {
        Err(AVISO.into())
    }
    pub fn pedir_arquivo(_: &str, _: &str, _: &str, _: &std::path::Path, _: impl FnMut(u64, u64)) -> Result<(u16, Vec<u8>), String> {
        Err(AVISO.into())
    }
    pub fn garantir_nucleo() -> Result<(), String> {
        Err(AVISO.into())
    }
}
