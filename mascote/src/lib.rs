//! Mascote da Colmeia: a abelha-robô, em cinco acabamentos com os mesmos estados.

mod mascote;
mod modelos;
mod pincel;

pub use mascote::{Estado, Modelo, Tempo, desenhar};
pub use pincel::{arredondar, hexagono};
