use std::process::ExitCode;

fn main() -> ExitCode {
    let opcoes = match colmeia_voz::ciclo::Opcoes::ler(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("colmeia-voz: {e}");
            return ExitCode::from(2);
        }
    };
    colmeia_voz::ciclo::rodar(opcoes)
}
