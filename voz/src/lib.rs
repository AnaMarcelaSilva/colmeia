//! A voz da Colmeia num processo à parte: a tela (ou a abelha fixa) abre o
//! `colmeia-voz` quando precisa, conversa com ele pela entrada e pela saída
//! padrão e ele sai sozinho depois de um tempo sem uso, levando os modelos
//! junto. Faz o ditado e os comandos falados (Whisper), a voz da abelha
//! (Kokoro) e o "ouvir o nome". Quem não usa a voz não carrega nada disso.

pub mod audio;
pub mod ciclo;
pub mod fala;
pub mod nome;
pub mod pausa;
pub mod protocolo;
pub mod transcrever;

/// A mensagem para quem usa, em português e com o que fazer; o detalhe
/// técnico (o erro da biblioteca) vai só para o log, a saída de erro.
pub fn falha(mensagem: &str, detalhe: impl std::fmt::Display) -> String {
    eprintln!("colmeia-voz: {mensagem} [{detalhe}]");
    mensagem.to_string()
}
