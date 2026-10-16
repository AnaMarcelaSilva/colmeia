//! O ditado por voz da Colmeia num processo à parte: a tela abre o
//! `colmeia-voz` quando você aperta o microfone, conversa com ele pela entrada
//! e pela saída padrão e ele sai sozinho depois de um tempo sem uso, levando
//! o modelo junto. Quem não usa a voz não carrega nada disso.

pub mod audio;
pub mod ciclo;
pub mod protocolo;
pub mod transcrever;
