//! Se a janela está à vista, pelo X11. O egui só fica sabendo que minimizou
//! quando desenha um quadro, e minimizar no X11 não traz quadro nenhum (e
//! trocar de área de trabalho, nem isso): a abelha fixa não reaparecia. Aqui
//! uma thread com a própria conexão ao X11 escuta, por eventos (sem consulta
//! periódica), o mapear e desmapear da janela, o _NET_WM_STATE dela (o
//! HIDDEN de minimizada), a área de trabalho dela e a atual, e qual janela
//! está ativa (_NET_ACTIVE_WINDOW): sair da Colmeia por Alt+Tab deixa a
//! janela mapeada, mas ela já não é a que está em uso, e a abelha fixa tem
//! que aparecer. Diz ao núcleo quando a janela some ou volta. Fora do X11
//! (Wayland, Windows, macOS), segue o que o egui informa.

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

/// À vista: mapeada, sem o HIDDEN, na área de trabalho atual (ou em todas)
/// e ativa (a janela em uso, ou um diálogo dela). Sem gerenciador de
/// janelas as propriedades não existem e valem como à vista.
pub fn a_vista(mapeada: bool, oculta: bool, area_da_janela: Option<u32>, area_atual: Option<u32>, ativa: bool) -> bool {
    const TODAS: u32 = 0xFFFF_FFFF;
    let na_atual = match (area_da_janela, area_atual) {
        (Some(j), Some(a)) => j == a || j == TODAS,
        _ => true,
    };
    mapeada && !oculta && na_atual && ativa
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
        ativa: Atom,
        pid: Atom,
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
            let ativa = atomo("_NET_ACTIVE_WINDOW")?;
            let pid = atomo("_NET_WM_PID")?;
            // Cada conexão tem a sua máscara: escutar a janela daqui não tira nada do winit.
            let na_janela = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY);
            conexao.change_window_attributes(janela, &na_janela).map_err(|e| e.to_string())?;
            conexao.change_window_attributes(raiz, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE)).map_err(|e| e.to_string())?;
            conexao.flush().map_err(|e| e.to_string())?;
            Ok(Vigia { conexao, janela, raiz, estado, oculta, area, area_atual, ativa, pid })
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

        /// A janela ativa é esta, ou um diálogo dela (do mesmo processo ou
        /// transitório dela, como o seletor de arquivos). Sem a propriedade na
        /// raiz (sem gerenciador de janelas), vale como ativa.
        fn ativa(&self) -> bool {
            // Tipo ANY: a propriedade é WINDOW pela especificação, mas vale aceitar o que o gerenciador puser.
            let Some(r) = self.conexao.get_property(false, self.raiz, self.ativa, AtomEnum::ANY, 0, 1).ok().and_then(|c| c.reply().ok()) else {
                return true;
            };
            let Some(ativa) = r.value32().and_then(|mut v| v.next()) else {
                return true;
            };
            if ativa == self.janela {
                return true;
            }
            if ativa == 0 {
                return false;
            }
            if self.numero(ativa, self.pid).is_some_and(|p| p == std::process::id()) {
                return true;
            }
            let transitoria = self.conexao.get_property(false, ativa, AtomEnum::WM_TRANSIENT_FOR, AtomEnum::ANY, 0, 1).ok().and_then(|c| c.reply().ok());
            transitoria.and_then(|r| r.value32().and_then(|mut v| v.next())).is_some_and(|dona| dona == self.janela)
        }

        fn mapeada(&self) -> bool {
            self.conexao.get_window_attributes(self.janela).ok().and_then(|c| c.reply().ok()).is_some_and(|a| a.map_state != MapState::UNMAPPED)
        }

        fn agora(&self, mapeada: bool) -> bool {
            super::a_vista(mapeada, self.oculta(), self.numero(self.janela, self.area), self.numero(self.raiz, self.area_atual), self.ativa())
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
                    Event::PropertyNotify(e) if e.window == self.raiz => e.atom == self.area_atual || e.atom == self.ativa,
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
    fn a_vista_so_mapeada_sem_hidden_na_area_atual_e_ativa() {
        assert!(a_vista(true, false, None, None, true));
        assert!(!a_vista(false, false, None, None, true));
        assert!(!a_vista(true, true, Some(0), Some(0), true));
        assert!(a_vista(true, false, Some(1), Some(1), true));
        // Em outra área de trabalho, não está à vista; fixada em todas, está.
        assert!(!a_vista(true, false, Some(0), Some(1), true));
        assert!(a_vista(true, false, Some(0xFFFF_FFFF), Some(1), true));
        // Sem gerenciador (sem as propriedades), vale o mapeamento.
        assert!(a_vista(true, false, Some(0), None, true));
        // Mapeada mas em segundo plano (Alt+Tab para outro programa): fora de vista.
        assert!(!a_vista(true, false, Some(1), Some(1), false));
    }
}
