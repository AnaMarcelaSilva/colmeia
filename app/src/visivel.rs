//! Se a janela está à vista, pelo X11. O egui só fica sabendo que minimizou
//! quando desenha um quadro, e minimizar no X11 não traz quadro nenhum (e
//! trocar de área de trabalho, nem isso): a abelha fixa não reaparecia. Aqui
//! uma thread com a própria conexão ao X11 escuta, por eventos (sem consulta
//! periódica), o mapear e desmapear da janela, o _NET_WM_STATE dela (o
//! HIDDEN de minimizada), a área de trabalho dela e a atual, e diz ao núcleo
//! quando a janela some ou volta. Fora do X11 (Wayland, Windows, macOS),
//! segue o que o egui informa.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

/// O perfil de agora (a tela atualiza; a thread lê na hora de avisar).
pub type PerfilAtual = Arc<AtomicI64>;

/// Começa a vigiar a janela; `Ok` quer dizer que o X11 cuida disso (e a
/// tela não usa o que o egui informa).
#[cfg(target_os = "linux")]
pub fn vigiar(frame: &eframe::Frame, perfil: PerfilAtual) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let janela = match frame.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Xlib(h) => h.window as u32,
        RawWindowHandle::Xcb(h) => h.window.get(),
        _ => return Err("fora do X11".into()),
    };
    let x = x11::Vigia::conectar(janela)?;
    std::thread::Builder::new()
        .name("janela-visivel".into())
        .spawn(move || {
            x.laco(|visivel| {
                let p = perfil.load(Ordering::Relaxed);
                if p > 0 {
                    crate::eventos::avisar_minimizada(p, !visivel);
                }
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn vigiar(_: &eframe::Frame, _: PerfilAtual) -> Result<(), String> {
    Err("só no X11".into())
}

/// À vista: mapeada, sem o HIDDEN e na área de trabalho atual (ou em todas).
/// Sem gerenciador de janelas as propriedades não existem e valem como à vista.
pub fn a_vista(mapeada: bool, oculta: bool, area_da_janela: Option<u32>, area_atual: Option<u32>) -> bool {
    const TODAS: u32 = 0xFFFF_FFFF;
    let na_atual = match (area_da_janela, area_atual) {
        (Some(j), Some(a)) => j == a || j == TODAS,
        _ => true,
    };
    mapeada && !oculta && na_atual
}

#[cfg(target_os = "linux")]
mod x11 {
    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask, MapState, Window};
    use x11rb::rust_connection::RustConnection;

    pub struct Vigia {
        conexao: RustConnection,
        janela: Window,
        raiz: Window,
        estado: Atom,
        oculta: Atom,
        area: Atom,
        area_atual: Atom,
    }

    impl Vigia {
        pub fn conectar(janela: Window) -> Result<Vigia, String> {
            let (conexao, tela) = x11rb::connect(None).map_err(|e| e.to_string())?;
            let raiz = conexao.setup().roots[tela].root;
            let atomo = |nome: &str| -> Result<Atom, String> {
                Ok(conexao.intern_atom(false, nome.as_bytes()).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?.atom)
            };
            let estado = atomo("_NET_WM_STATE")?;
            let oculta = atomo("_NET_WM_STATE_HIDDEN")?;
            let area = atomo("_NET_WM_DESKTOP")?;
            let area_atual = atomo("_NET_CURRENT_DESKTOP")?;
            // Cada conexão tem a sua máscara: escutar a janela daqui não tira nada do winit.
            let na_janela = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY);
            conexao.change_window_attributes(janela, &na_janela).map_err(|e| e.to_string())?;
            conexao.change_window_attributes(raiz, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE)).map_err(|e| e.to_string())?;
            conexao.flush().map_err(|e| e.to_string())?;
            Ok(Vigia { conexao, janela, raiz, estado, oculta, area, area_atual })
        }

        fn numero(&self, janela: Window, atomo: Atom) -> Option<u32> {
            let r = self.conexao.get_property(false, janela, atomo, AtomEnum::CARDINAL, 0, 1).ok()?.reply().ok()?;
            r.value32()?.next()
        }

        fn oculta(&self) -> bool {
            let Some(r) = self.conexao.get_property(false, self.janela, self.estado, AtomEnum::ATOM, 0, 64).ok().and_then(|c| c.reply().ok()) else {
                return false;
            };
            r.value32().is_some_and(|mut v| v.any(|a| a == self.oculta))
        }

        fn mapeada(&self) -> bool {
            self.conexao.get_window_attributes(self.janela).ok().and_then(|c| c.reply().ok()).is_some_and(|a| a.map_state != MapState::UNMAPPED)
        }

        fn agora(&self, mapeada: bool) -> bool {
            super::a_vista(mapeada, self.oculta(), self.numero(self.janela, self.area), self.numero(self.raiz, self.area_atual))
        }

        /// Espera os eventos e chama `avisar` só quando muda (e uma vez no começo).
        /// Sai quando a janela some ou a conexão cai.
        pub fn laco(self, avisar: impl Fn(bool)) {
            let mut mapeada = self.mapeada();
            let mut antes = self.agora(mapeada);
            avisar(antes);
            loop {
                let Ok(evento) = self.conexao.wait_for_event() else { return };
                let importa = match evento {
                    Event::MapNotify(e) if e.window == self.janela => {
                        mapeada = true;
                        true
                    }
                    Event::UnmapNotify(e) if e.window == self.janela => {
                        mapeada = false;
                        true
                    }
                    Event::DestroyNotify(e) if e.window == self.janela => return,
                    Event::PropertyNotify(e) if e.window == self.janela => e.atom == self.estado || e.atom == self.area,
                    Event::PropertyNotify(e) if e.window == self.raiz => e.atom == self.area_atual,
                    _ => false,
                };
                if !importa {
                    continue;
                }
                let visivel = self.agora(mapeada);
                if visivel != antes {
                    antes = visivel;
                    avisar(visivel);
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::a_vista;

    #[test]
    fn a_vista_so_mapeada_sem_hidden_e_na_area_atual() {
        assert!(a_vista(true, false, None, None));
        assert!(!a_vista(false, false, None, None));
        assert!(!a_vista(true, true, Some(0), Some(0)));
        assert!(a_vista(true, false, Some(1), Some(1)));
        // Em outra área de trabalho, não está à vista; fixada em todas, está.
        assert!(!a_vista(true, false, Some(0), Some(1)));
        assert!(a_vista(true, false, Some(0xFFFF_FFFF), Some(1)));
        // Sem gerenciador (sem as propriedades), vale o mapeamento.
        assert!(a_vista(true, false, Some(0), None));
    }
}
