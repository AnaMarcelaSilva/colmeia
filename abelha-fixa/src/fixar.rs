//! Sempre por cima e em todas as áreas de trabalho. O "por cima" vem do
//! egui (AlwaysOnTop); "todas as áreas de trabalho" não existe nele, então
//! no X11 a abelha pede ao gerenciador de janelas, como manda o EWMH:
//! _NET_WM_STATE com STICKY (e ABOVE de novo, para gerenciadores que só
//! leem a mensagem), fora da barra de tarefas, e _NET_WM_DESKTOP = todas.
//! No Wayland não há como pedir isso: a abelha vira uma janela comum.

use eframe::Frame;

#[cfg(target_os = "linux")]
pub fn em_todas_as_areas(frame: &Frame) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask};

    let janela = match frame.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Xlib(h) => h.window as u32,
        RawWindowHandle::Xcb(h) => h.window.get(),
        _ => return Err("fora do X11 (Wayland?): fica como janela comum".into()),
    };
    let (conexao, tela) = x11rb::connect(None).map_err(|e| e.to_string())?;
    let raiz = conexao.setup().roots[tela].root;
    let atomo = |nome: &str| -> Result<u32, String> {
        Ok(conexao.intern_atom(false, nome.as_bytes()).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?.atom)
    };
    let (estado, desktop) = (atomo("_NET_WM_STATE")?, atomo("_NET_WM_DESKTOP")?);
    let (acima, fixa) = (atomo("_NET_WM_STATE_ABOVE")?, atomo("_NET_WM_STATE_STICKY")?);
    let (sem_barra, sem_paginador) = (atomo("_NET_WM_STATE_SKIP_TASKBAR")?, atomo("_NET_WM_STATE_SKIP_PAGER")?);
    // 1 = acrescentar; o último 1 diz que o pedido vem de um programa comum.
    const ACRESCENTAR: u32 = 1;
    const TODAS: u32 = 0xFFFF_FFFF;
    let mensagens = [
        ClientMessageEvent::new(32, janela, estado, [ACRESCENTAR, fixa, acima, 1, 0]),
        ClientMessageEvent::new(32, janela, estado, [ACRESCENTAR, sem_barra, sem_paginador, 1, 0]),
        ClientMessageEvent::new(32, janela, desktop, [TODAS, 1, 0, 0, 0]),
    ];
    for m in mensagens {
        conexao.send_event(false, raiz, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, m).map_err(|e| e.to_string())?;
    }
    // Uma ida e volta antes de fechar a conexão: o servidor só a encerra
    // depois de ter lido os pedidos (sem isso, às vezes eles se perdiam).
    conexao.get_input_focus().map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn em_todas_as_areas(_: &Frame) -> Result<(), String> {
    Err("só no X11: aqui a abelha fica por cima, mas na área de trabalho atual".into())
}

/// Onde está o ponteiro na tela (em pixels) e se o botão esquerdo está
/// apertado, pelo X11: arrastando a abelha, o ponteiro sai da janela antes
/// de ela chegar, e a janela deixa de receber o movimento e o soltar.
#[cfg(target_os = "linux")]
pub fn ponteiro() -> Option<(f32, f32, bool)> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};
    let (conexao, tela) = x11rb::connect(None).ok()?;
    let raiz = conexao.setup().roots[tela].root;
    let r = conexao.query_pointer(raiz).ok()?.reply().ok()?;
    Some((r.root_x as f32, r.root_y as f32, r.mask.contains(KeyButMask::BUTTON1)))
}

#[cfg(not(target_os = "linux"))]
pub fn ponteiro() -> Option<(f32, f32, bool)> {
    None
}

/// O tamanho da tela em pixels, pelo X11: o egui às vezes não sabe o
/// monitor (um servidor sem RandR, como o Xephyr, diz 1 x 1).
#[cfg(target_os = "linux")]
pub fn tamanho_da_tela() -> Option<(u16, u16)> {
    use x11rb::connection::Connection;
    let (conexao, tela) = x11rb::connect(None).ok()?;
    let raiz = &conexao.setup().roots[tela];
    Some((raiz.width_in_pixels, raiz.height_in_pixels))
}

#[cfg(not(target_os = "linux"))]
pub fn tamanho_da_tela() -> Option<(u16, u16)> {
    None
}

/// Se há quem componha as janelas (e a transparência funcione). No X11 sem
/// compositor, uma janela transparente sai preta (ou nem aparece): a abelha
/// fica num fundo escuro.
#[cfg(target_os = "linux")]
pub fn tem_compositor() -> bool {
    use x11rb::protocol::xproto::ConnectionExt;
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return true;
    }
    let Ok((conexao, tela)) = x11rb::connect(None) else { return true };
    let nome = format!("_NET_WM_CM_S{tela}");
    let Some(atomo) = conexao.intern_atom(false, nome.as_bytes()).ok().and_then(|c| c.reply().ok()) else { return true };
    conexao.get_selection_owner(atomo.atom).ok().and_then(|c| c.reply().ok()).is_none_or(|r| r.owner != 0)
}

#[cfg(not(target_os = "linux"))]
pub fn tem_compositor() -> bool {
    true
}
