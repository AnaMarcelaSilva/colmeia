//! Mascote da Colmeia: a abelha-robô, em cinco acabamentos com os mesmos estados.

mod fone;
mod mascote;
mod modelos;
mod pincel;

pub use fone::Fone;
pub use mascote::{Acessorios, Estado, Modelo, Tempo, desenhar, desenhar_com};
pub use pincel::{arredondar, hexagono};
