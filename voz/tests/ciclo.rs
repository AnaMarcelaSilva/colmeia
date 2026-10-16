//! A vida do processo, sem microfone: ele sai sozinho depois de parado, sai
//! quando a tela fecha a entrada e responde com erro ao que não entende.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

fn abrir(parado_ms: u64) -> (Child, BufReader<ChildStdout>) {
    let mut filho = Command::new(env!("CARGO_BIN_EXE_colmeia-voz"))
        .args(["--modelo", "/nao/existe/ggml-base.bin", "--parado-ms", &parado_ms.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("abrir o colmeia-voz");
    let saida = BufReader::new(filho.stdout.take().unwrap());
    (filho, saida)
}

fn linha(saida: &mut BufReader<ChildStdout>) -> String {
    let mut l = String::new();
    saida.read_line(&mut l).unwrap();
    l.trim().to_string()
}

/// Espera o processo sair, até o prazo; depois do prazo, mata e falha.
fn esperar_saida(filho: &mut Child, prazo: Duration) -> std::process::ExitStatus {
    let inicio = Instant::now();
    loop {
        if let Some(s) = filho.try_wait().unwrap() {
            return s;
        }
        if inicio.elapsed() > prazo {
            let _ = filho.kill();
            panic!("o colmeia-voz não saiu em {prazo:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn sai_sozinho_depois_de_um_tempo_parado() {
    let (mut filho, mut saida) = abrir(300);
    assert_eq!(linha(&mut saida), r#"{"evento":"iniciado"}"#);
    let inicio = Instant::now();
    assert_eq!(linha(&mut saida), r#"{"evento":"saindo","motivo":"parado"}"#);
    assert!(inicio.elapsed() >= Duration::from_millis(250), "{:?}", inicio.elapsed());
    assert!(esperar_saida(&mut filho, Duration::from_secs(5)).success());
}

#[test]
fn sai_quando_a_tela_fecha_a_entrada() {
    let (mut filho, mut saida) = abrir(60_000);
    assert_eq!(linha(&mut saida), r#"{"evento":"iniciado"}"#);
    drop(filho.stdin.take());
    assert!(esperar_saida(&mut filho, Duration::from_secs(5)).success());
}

#[test]
fn responde_com_erro_ao_que_nao_entende_e_sai_quando_pedem() {
    let (mut filho, mut saida) = abrir(60_000);
    assert_eq!(linha(&mut saida), r#"{"evento":"iniciado"}"#);
    let mut entrada = filho.stdin.take().unwrap();
    writeln!(entrada, r#"{{"comando":"gravar","arquivo":"/tmp/a.wav"}}"#).unwrap();
    assert_eq!(linha(&mut saida), r#"{"evento":"erro","mensagem":"Pedido desconhecido para o ditado."}"#);
    writeln!(entrada, r#"{{"comando":"parar"}}"#).unwrap();
    assert_eq!(linha(&mut saida), r#"{"evento":"erro","mensagem":"O microfone não estava aberto."}"#);
    writeln!(entrada, r#"{{"comando":"cancelar"}}"#).unwrap();
    writeln!(entrada, r#"{{"comando":"sair"}}"#).unwrap();
    assert!(esperar_saida(&mut filho, Duration::from_secs(5)).success());
}

#[test]
fn sem_o_modelo_na_linha_de_comando_nem_abre() {
    let s = Command::new(env!("CARGO_BIN_EXE_colmeia-voz")).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
    assert_eq!(s.code(), Some(2));
}
