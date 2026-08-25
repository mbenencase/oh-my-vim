use std::fmt;

use serde::{Deserialize, Deserializer};

/// A key, independent of any terminal backend. The UI layer converts crossterm
/// events into this so the keymap can be parsed and tested without a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Esc,
    Enter,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    /// Placeholder replaced with the configured leader key at load time.
    Leader,
}

impl Key {
    pub fn plain(code: KeyCode) -> Self {
        Key {
            code,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    pub fn char(c: char) -> Self {
        Key::plain(KeyCode::Char(c))
    }

    pub fn ctrl(c: char) -> Self {
        Key {
            code: KeyCode::Char(c),
            ctrl: true,
            alt: false,
            shift: false,
        }
    }

    /// The character this key would type, if any. Modified keys type nothing.
    pub fn as_typed_char(self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if !self.ctrl && !self.alt => Some(c),
            _ => None,
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let named = match self.code {
            // A literal space is invisible in a keymap listing, so name it.
            KeyCode::Char(' ') if !self.ctrl && !self.alt => "Space",
            KeyCode::Char(c) if !self.ctrl && !self.alt => return write!(f, "{c}"),
            KeyCode::Char(c) => {
                return write!(
                    f,
                    "<{}{}{c}>",
                    if self.ctrl { "C-" } else { "" },
                    if self.alt { "A-" } else { "" }
                );
            }
            KeyCode::Esc => "Esc",
            KeyCode::Enter => "CR",
            KeyCode::Tab => "Tab",
            KeyCode::BackTab => "S-Tab",
            KeyCode::Backspace => "BS",
            KeyCode::Delete => "Del",
            KeyCode::Insert => "Ins",
            KeyCode::Left => "Left",
            KeyCode::Right => "Right",
            KeyCode::Up => "Up",
            KeyCode::Down => "Down",
            KeyCode::Home => "Home",
            KeyCode::End => "End",
            KeyCode::PageUp => "PageUp",
            KeyCode::PageDown => "PageDown",
            KeyCode::Leader => "leader",
            KeyCode::F(n) => return write!(f, "<F{n}>"),
        };
        write!(f, "<{named}>")
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyParseError {
    #[error("unknown key name `<{0}>`")]
    UnknownName(String),
    #[error("empty key sequence")]
    Empty,
}

/// Parse vim-style notation into a key sequence.
///
/// `"dd"` → two `d` presses. `"<C-p>"` → ctrl+p. `"<leader>ff"` → leader, f, f.
/// Bare characters are literal, so a sequence is just its characters unless
/// wrapped in angle brackets.
pub fn parse_sequence(spec: &str) -> Result<Vec<Key>, KeyParseError> {
    let chars: Vec<char> = spec.chars().collect();
    let mut keys = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // A `<` only opens a key name if something closes it; otherwise it is the
        // literal character, which is what makes `<<` and `<` bindable at all.
        // A *closed* group with an unknown name is still an error, so `<C-pp>`
        // is caught rather than silently becoming five literal keys.
        if chars[i] == '<'
            && let Some(close) = chars[i..].iter().position(|c| *c == '>')
        {
            let inner: String = chars[i + 1..i + close].iter().collect();
            keys.push(parse_named(&inner)?);
            i += close + 1;
        } else {
            keys.push(Key::char(chars[i]));
            i += 1;
        }
    }
    if keys.is_empty() {
        return Err(KeyParseError::Empty);
    }
    Ok(keys)
}

fn parse_named(inner: &str) -> Result<Key, KeyParseError> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut rest = inner;

    loop {
        let (flag, tail) = match rest.get(..2).map(str::to_ascii_uppercase).as_deref() {
            Some("C-") => (&mut ctrl, &rest[2..]),
            Some("A-") | Some("M-") => (&mut alt, &rest[2..]),
            Some("S-") => (&mut shift, &rest[2..]),
            _ => break,
        };
        *flag = true;
        rest = tail;
    }

    let code = match rest.to_ascii_lowercase().as_str() {
        "esc" | "escape" => KeyCode::Esc,
        "cr" | "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "bs" | "backspace" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "ins" | "insert" => KeyCode::Insert,
        "space" => KeyCode::Char(' '),
        "lt" => KeyCode::Char('<'),
        "gt" => KeyCode::Char('>'),
        "bar" => KeyCode::Char('|'),
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "leader" => KeyCode::Leader,
        other => {
            if let Some(n) = other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                KeyCode::F(n)
            } else {
                let mut it = other.chars();
                match (it.next(), it.next()) {
                    (Some(c), None) => KeyCode::Char(c),
                    _ => return Err(KeyParseError::UnknownName(inner.to_string())),
                }
            }
        }
    };

    // `<S-Tab>` is its own terminal code rather than Tab-with-shift.
    if shift && code == KeyCode::Tab {
        return Ok(Key::plain(KeyCode::BackTab));
    }
    Ok(Key {
        code,
        ctrl,
        alt,
        shift,
    })
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        let keys = parse_sequence(&s).map_err(serde::de::Error::custom)?;
        keys.into_iter()
            .next()
            .ok_or_else(|| serde::de::Error::custom("empty key"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_chars_become_a_sequence() {
        assert_eq!(
            parse_sequence("dd").unwrap(),
            vec![Key::char('d'), Key::char('d')]
        );
    }

    #[test]
    fn modifiers_parse() {
        assert_eq!(parse_sequence("<C-p>").unwrap(), vec![Key::ctrl('p')]);
        assert!(parse_sequence("<A-x>").unwrap()[0].alt);
    }

    #[test]
    fn leader_mixes_with_literals() {
        let seq = parse_sequence("<leader>ff").unwrap();
        assert_eq!(seq.len(), 3);
        assert_eq!(seq[0].code, KeyCode::Leader);
        assert_eq!(seq[2], Key::char('f'));
    }

    #[test]
    fn shift_tab_is_backtab() {
        assert_eq!(parse_sequence("<S-Tab>").unwrap()[0].code, KeyCode::BackTab);
    }

    #[test]
    fn lone_angle_bracket_is_literal() {
        // `<<` (outdent) and `<` (visual outdent) must be bindable.
        assert_eq!(
            parse_sequence("<<").unwrap(),
            vec![Key::char('<'), Key::char('<')]
        );
        assert_eq!(parse_sequence("<").unwrap(), vec![Key::char('<')]);
        assert_eq!(parse_sequence("<lt>").unwrap(), vec![Key::char('<')]);
    }

    #[test]
    fn a_closed_group_with_an_unknown_name_is_an_error() {
        assert!(matches!(
            parse_sequence("<C-pp>"),
            Err(KeyParseError::UnknownName(_))
        ));
    }
}
