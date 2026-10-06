//! A regra do Enter nas caixas de mensagem para o agente (a do painel da
//! tarefa e a "Pedir ao agente"): só o Enter puro envia; Shift+Enter e
//! Ctrl+Enter quebram a linha, como no terminal do Claude Code. Alt+Enter
//! não faz nada e o Enter que confirma uma composição de acento (IME) nunca
//! envia.
//!
//! O egui casa `consume_key(Modifiers::NONE, Enter)` também com Shift+Enter
//! (`matches_logically` ignora o Shift), então os eventos são lidos à mão.
//! Os editores de várias linhas (console SQL, lousa, nota do slide, texto da
//! daily) ficam com Ctrl+Enter e não passam por aqui.

use eframe::egui::{self, Event, Key, KeyboardShortcut, Modifiers};

/// O que um Enter faz na caixa de mensagem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcaoEnter {
    Enviar,
    QuebrarLinha,
    Ignorar,
}

/// A regra pura: `compondo` é uma composição de acento (IME) em andamento.
pub fn regra_enter(m: Modifiers, compondo: bool) -> AcaoEnter {
    if compondo || m.alt {
        AcaoEnter::Ignorar
    } else if m.shift || m.ctrl || m.command || m.mac_cmd {
        AcaoEnter::QuebrarLinha
    } else {
        AcaoEnter::Enviar
    }
}

/// A tecla que o campo usa para quebrar a linha (o Enter puro não quebra).
pub fn quebra_de_linha() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)
}

/// Composição de acento (IME) em andamento: liga com um texto em composição
/// e desliga quando ela termina. Guardado entre quadros por quem tem o campo.
#[derive(Default, Debug, Clone, Copy)]
pub struct Composicao {
    pub compondo: bool,
}

impl Composicao {
    /// Tira da fila os Enter que não são quebra de linha (antes de o campo
    /// ver) e diz se algum deles envia. Os de quebra ficam para o campo, como
    /// Shift+Enter (a tecla que ele conhece).
    /// Um Enter no mesmo quadro de um evento de IME também é ignorado.
    pub fn tirar_enters(&mut self, eventos: &mut Vec<Event>) -> bool {
        let mut ime_no_quadro = false;
        for e in eventos.iter() {
            if let Event::Ime(ime) = e {
                ime_no_quadro = true;
                match ime {
                    egui::ImeEvent::Preedit { text, .. } => self.compondo = !text.is_empty(),
                    egui::ImeEvent::Commit(_) => self.compondo = false,
                    _ => {}
                }
            }
        }
        let compondo = self.compondo || ime_no_quadro;
        let mut enviar = false;
        eventos.retain(|e| match e {
            Event::Key { key: Key::Enter, pressed, modifiers, .. } => match regra_enter(*modifiers, compondo) {
                AcaoEnter::QuebrarLinha => true,
                AcaoEnter::Enviar => {
                    enviar |= *pressed;
                    false
                }
                AcaoEnter::Ignorar => false,
            },
            _ => true,
        });
        for e in eventos.iter_mut() {
            if let Event::Key { key: Key::Enter, modifiers, .. } = e {
                *modifiers = Modifiers::SHIFT;
            }
        }
        enviar
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn enter(modifiers: Modifiers) -> Event {
        Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn regra_de_cada_combinacao() {
        assert_eq!(regra_enter(Modifiers::NONE, false), AcaoEnter::Enviar);
        assert_eq!(regra_enter(Modifiers::SHIFT, false), AcaoEnter::QuebrarLinha);
        assert_eq!(regra_enter(Modifiers::CTRL, false), AcaoEnter::QuebrarLinha);
        assert_eq!(regra_enter(Modifiers::COMMAND, false), AcaoEnter::QuebrarLinha);
        assert_eq!(regra_enter(Modifiers::CTRL | Modifiers::SHIFT, false), AcaoEnter::QuebrarLinha);
        assert_eq!(regra_enter(Modifiers::ALT, false), AcaoEnter::Ignorar);
        assert_eq!(regra_enter(Modifiers::ALT | Modifiers::SHIFT, false), AcaoEnter::Ignorar);
        for m in [Modifiers::NONE, Modifiers::SHIFT, Modifiers::CTRL] {
            assert_eq!(regra_enter(m, true), AcaoEnter::Ignorar);
        }
    }

    #[test]
    fn tira_da_fila_o_que_nao_e_quebra() {
        let mut c = Composicao::default();
        let mut eventos = vec![enter(Modifiers::SHIFT), Event::Text("a".into()), enter(Modifiers::NONE), enter(Modifiers::ALT), enter(Modifiers::CTRL)];
        assert!(c.tirar_enters(&mut eventos));
        assert_eq!(eventos.len(), 3);
        assert!(matches!(eventos[0], Event::Key { modifiers, .. } if modifiers.shift));
        // O Ctrl+Enter chega ao campo como Shift+Enter, a tecla de quebra dele.
        assert!(matches!(eventos[2], Event::Key { modifiers, .. } if modifiers == Modifiers::SHIFT));
        // Só a soltura do Enter não envia.
        let mut soltou = vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }];
        assert!(!c.tirar_enters(&mut soltou));
        assert!(soltou.is_empty());
    }

    #[test]
    fn composicao_de_acento_nao_envia() {
        let mut c = Composicao::default();
        let mut eventos = vec![Event::Ime(egui::ImeEvent::Preedit { text: "´".into(), active_range_chars: None })];
        assert!(!c.tirar_enters(&mut eventos));
        assert!(c.compondo);
        let mut eventos = vec![enter(Modifiers::NONE)];
        assert!(!c.tirar_enters(&mut eventos));
        // O Enter que confirma chega no mesmo quadro do Commit: também não envia.
        let mut eventos = vec![Event::Ime(egui::ImeEvent::Commit("á".into())), enter(Modifiers::NONE)];
        assert!(!c.tirar_enters(&mut eventos));
        assert!(!c.compondo);
        let mut eventos = vec![enter(Modifiers::NONE)];
        assert!(c.tirar_enters(&mut eventos));
    }
}
