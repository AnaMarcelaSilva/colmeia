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
    assert_eq!(linha(&mut saida), r#"{"evento":"erro","mensagem":"Pedido desconhecido para a voz."}"#);
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

#[test]
fn sem_a_voz_da_abelha_falar_responde_com_erro() {
    let (mut filho, mut saida) = abrir(60_000);
    assert_eq!(linha(&mut saida), r#"{"evento":"iniciado"}"#);
    let mut entrada = filho.stdin.take().unwrap();
    writeln!(entrada, r#"{{"comando":"falar","texto":"Oi."}}"#).unwrap();
    assert_eq!(linha(&mut saida), r#"{"evento":"erro","mensagem":"A voz da abelha não está ligada."}"#);
    // Aprender sem gravações e vigiar com um arquivo ruim também respondem, sem abrir o microfone.
    writeln!(entrada, r#"{{"comando":"aprender","nome":"Zuzu","destino":"/nao/existe/nome-1.json"}}"#).unwrap();
    assert!(linha(&mut saida).contains("Grave o nome pelo menos 3 vezes"));
    writeln!(entrada, r#"{{"comando":"vigiar","referencias":"/nao/existe/nome-1.json"}}"#).unwrap();
    assert!(linha(&mut saida).contains("Grave o nome de novo"));
    writeln!(entrada, r#"{{"comando":"desvigiar"}}"#).unwrap();
    assert_eq!(linha(&mut saida), r#"{"evento":"vigiando","ligado":false}"#);
    writeln!(entrada, r#"{{"comando":"calar"}}"#).unwrap();
    writeln!(entrada, r#"{{"comando":"sair"}}"#).unwrap();
    assert!(esperar_saida(&mut filho, Duration::from_secs(5)).success());
}

#[test]
fn a_voz_pela_metade_nem_abre() {
    let s = Command::new(env!("CARGO_BIN_EXE_colmeia-voz"))
        .args(["--modelo", "m", "--fala-modelo", "k.onnx"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(s.code(), Some(2));
}

/// O ONNX Runtime aceita um ambiente só por processo: falar, soltar o modelo
/// parado e falar de novo (e duas falas seguidas) não pode dar erro. Pede os
/// arquivos da voz baixados (COLMEIA_TESTE_KOKORO=<pasta voz>), toca o som e
/// ouve o nome (é o que deixa o processo vivo com o modelo solto): rode com
/// uma saída de som virtual e o monitor dela no lugar do microfone
/// (PULSE_SINK=x PULSE_SOURCE=x.monitor) e `-- --ignored`.
#[test]
#[ignore]
fn a_voz_fala_de_novo_depois_de_soltar_o_modelo() {
    let pasta = std::path::PathBuf::from(std::env::var("COLMEIA_TESTE_KOKORO").expect("COLMEIA_TESTE_KOKORO"));
    let mut filho = Command::new(env!("CARGO_BIN_EXE_colmeia-voz"))
        .arg("--modelo")
        .arg(pasta.join("ggml-base.bin"))
        .args(["--parado-ms", "1500"])
        .arg("--fala-modelo")
        .arg(pasta.join("kokoro-v1.0-q8.onnx"))
        .arg("--fala-voz")
        .arg(pasta.join("pf_dora.bin"))
        .arg("--fala-runtime")
        .arg(pasta.join("libonnxruntime.so.1.31.0"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("abrir o colmeia-voz");
    let mut saida = BufReader::new(filho.stdout.take().unwrap());
    let mut entrada = filho.stdin.take().unwrap();
    assert_eq!(linha(&mut saida), r#"{"evento":"iniciado"}"#);
    writeln!(entrada, r#"{{"comando":"vigiar"}}"#).unwrap();
    assert_eq!(linha(&mut saida), r#"{"evento":"vigiando","ligado":true}"#);
    let mut falar = |texto: &str| writeln!(entrada, r#"{{"comando":"falar","texto":"{texto}"}}"#).unwrap();
    falar("Primeira.");
    falar("Segunda, logo em seguida.");
    for _ in 0..2 {
        assert_eq!(linha(&mut saida), r#"{"evento":"falando"}"#);
        assert_eq!(linha(&mut saida), r#"{"evento":"falou"}"#);
    }
    // Parado mais que 1,5 s, o modelo sai da memória; ouvindo o nome, o processo fica.
    std::thread::sleep(Duration::from_millis(3500));
    falar("Depois de soltar o modelo.");
    assert_eq!(linha(&mut saida), r#"{"evento":"falando"}"#);
    assert_eq!(linha(&mut saida), r#"{"evento":"falou"}"#);
    drop(entrada);
    assert!(esperar_saida(&mut filho, Duration::from_secs(5)).success());
}
