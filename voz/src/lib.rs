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
