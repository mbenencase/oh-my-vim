use std::sync::mpsc::Sender;

use crossterm::event::{
    Event as CrossEvent, KeyCode as CtKey, KeyEvent, KeyEventKind, KeyModifiers,
};
use omv_config::{Key, KeyCode};

use crate::picker::{Item, PickerKind};

/// Everything the main loop can wake up for. One channel, many producers.
pub enum AppEvent {
    Input(CrossEvent),
    Lsp(omv_lsp::Event),
    /// Results from a background file walk or grep, tagged with the picker
    /// generation that asked for them so stale replies can be discarded.
    PickerItems {
        kind: PickerKind,
        generation: u64,
        items: Vec<Item>,
        truncated: bool,
    },
    Error(String),
}

/// Read terminal input on a dedicated thread. `crossterm::event::read` blocks,
/// which is exactly what we want here — no polling, no wasted wakeups.
pub fn spawn_input_thread(tx: Sender<AppEvent>) {
    std::thread::Builder::new()
        .name("omv-input".into())
        .spawn(move || {
            loop {
                match crossterm::event::read() {
                    Ok(event) => {
                        if tx.send(AppEvent::Input(event)).is_err() {
                            break; // the app is gone
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::Error(format!("input: {e}")));
                        break;
                    }
                }
            }
        })
        .expect("spawning the input thread");
}

/// Translate a crossterm key into the backend-independent [`Key`] the keymap uses.
/// Returns `None` for key-release and repeat events, which we don't bind.
pub fn to_key(event: KeyEvent) -> Option<Key> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);

    let code = match event.code {
        // The shift is already baked into the character, so don't also report it
        // as a modifier — `A` must not need `<S-A>` in a keymap.
        CtKey::Char(c) => {
            return Some(Key {
                code: KeyCode::Char(c),
                ctrl,
                alt,
                shift: false,
            });
        }
        CtKey::Esc => KeyCode::Esc,
        CtKey::Enter => KeyCode::Enter,
        CtKey::Tab => KeyCode::Tab,
        CtKey::BackTab => KeyCode::BackTab,
        CtKey::Backspace => KeyCode::Backspace,
        CtKey::Delete => KeyCode::Delete,
        CtKey::Insert => KeyCode::Insert,
        CtKey::Left => KeyCode::Left,
        CtKey::Right => KeyCode::Right,
        CtKey::Up => KeyCode::Up,
        CtKey::Down => KeyCode::Down,
        CtKey::Home => KeyCode::Home,
        CtKey::End => KeyCode::End,
        CtKey::PageUp => KeyCode::PageUp,
        CtKey::PageDown => KeyCode::PageDown,
        CtKey::F(n) => KeyCode::F(n),
        _ => return None,
    };
    Some(Key {
        code,
        ctrl,
        alt,
        shift,
    })
}
