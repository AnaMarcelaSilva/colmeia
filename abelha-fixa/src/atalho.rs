//! O atalho global dos comandos falados (Ctrl+Alt+Espaço): no X11, a abelha
//! pede a tecla ao servidor na janela raiz e é avisada mesmo com outro
//! programa em foco. Só enquanto os comandos falados estão ligados (soltar o
//! `Atalho` devolve a tecla); se outro programa já tiver o atalho, fica só o
//! clique. No Wayland e no Windows, só o clique.

#[cfg(target_os = "linux")]
mod x11 {
    use std::sync::Arc;

    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{ConnectionExt, GrabMode, ModMask};
    use x11rb::rust_connection::RustConnection;

    pub struct Atalho {
        conexao: Arc<RustConnection>,
        raiz: u32,
        tecla: u8,
    }

    /// Com o Caps Lock e o Num Lock ligados o atalho vale também.
    fn combinacoes() -> [ModMask; 4] {
        let base = ModMask::CONTROL | ModMask::M1;
        [base, base | ModMask::LOCK, base | ModMask::M2, base | ModMask::LOCK | ModMask::M2]
    }

    pub fn ouvir(apertou: impl Fn() + Send + 'static) -> Result<Atalho, String> {
        let (conexao, tela) = x11rb::connect(None).map_err(|e| format!("sem X11: {e}"))?;
        let raiz = conexao.setup().roots[tela].root;
        let (min, max) = (conexao.setup().min_keycode, conexao.setup().max_keycode);
        let mapa = conexao.get_keyboard_mapping(min, max - min + 1).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
        let por_tecla = mapa.keysyms_per_keycode.max(1) as usize;
        // A tecla do espaço (keysym 0x20).
        let tecla = mapa.keysyms.chunks(por_tecla).position(|s| s.contains(&0x20)).map(|i| min + i as u8).ok_or("sem a tecla de espaço no teclado")?;
        for m in combinacoes() {
            conexao
                .grab_key(false, raiz, m, tecla, GrabMode::ASYNC, GrabMode::ASYNC)
                .map_err(|e| e.to_string())?
                .check()
                .map_err(|_| "o Ctrl+Alt+Espaço já é de outro programa: fica só o clique na abelha".to_string())?;
        }
        conexao.flush().map_err(|e| e.to_string())?;
        let conexao = Arc::new(conexao);
        let leitura = Arc::clone(&conexao);
        std::thread::Builder::new()
            .name("atalho".into())
            .spawn(move || {
                // Dorme até o servidor mandar a tecla; nada de consulta periódica.
                while let Ok(evento) = leitura.wait_for_event() {
                    if let Event::KeyPress(_) = evento {
                        apertou();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Atalho { conexao, raiz, tecla })
    }

    impl Drop for Atalho {
        fn drop(&mut self) {
            for m in combinacoes() {
                let _ = self.conexao.ungrab_key(self.tecla, self.raiz, m);
            }
            let _ = self.conexao.flush();
        }
    }
}

#[cfg(target_os = "linux")]
pub use x11::{Atalho, ouvir};

#[cfg(not(target_os = "linux"))]
pub struct Atalho;

#[cfg(not(target_os = "linux"))]
pub fn ouvir(_: impl Fn() + Send + 'static) -> Result<Atalho, String> {
    Err("o atalho global só existe no X11: fica o clique na abelha".into())
}
