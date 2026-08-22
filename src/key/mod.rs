//! Key representation for editor input

/// Represents a key press event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// Printable character
    Char(char),
    /// Control key combination (e.g., Ctrl+A)
    Ctrl(u8),
    /// Control+Shift combination (e.g., Ctrl+Shift+S)
    CtrlShift(u8),
    /// Alt key combination (e.g., Alt+A)
    Alt(u8),
    /// Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    CtrlArrowUp,
    CtrlArrowDown,
    CtrlArrowLeft,
    CtrlArrowRight,
    /// Navigation keys
    Home,
    End,
    CtrlHome,
    CtrlEnd,
    PageUp,
    PageDown,
    /// Editing keys
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    ShiftTab,
    /// Space pressed with the Shift modifier
    ShiftSpace,
    /// System events
    Resize(u16, u16),
}

impl Key {
    /// Convert key to VT100/xterm byte sequence for PTY input, following the
    /// CSI convention (cursor keys, modified keys, tilde keys, or a single byte).
    pub fn to_vt100_bytes(&self) -> Vec<u8> {
        match self {
            // Printable character -> UTF-8 encoding
            Key::Char(c) => {
                let mut buf = [0; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }

            // Ctrl+key -> mask to control range (0x00–0x1F)
            Key::Ctrl(c) => vec![c & 0x1f],

            // Ctrl+Shift+key
            Key::CtrlShift(c) => vec![c & 0x1f],

            // Alt+key -> ESC prefix followed by the character
            Key::Alt(c) => vec![0x1b, *c],

            // Single-byte control characters
            Key::Backspace => vec![0x7f],
            Key::Enter => vec![b'\r'],
            Key::Escape => vec![0x1b],
            Key::Tab => vec![b'\t'],
            Key::ShiftTab => vec![0x1b, b'[', b'Z'],
            Key::ShiftSpace => vec![b' '],

            // CSI cursor keys: ESC [ {suffix}
            Key::ArrowUp => csi(b'A', None),
            Key::ArrowDown => csi(b'B', None),
            Key::ArrowRight => csi(b'C', None),
            Key::ArrowLeft => csi(b'D', None),
            Key::Home => csi(b'H', None),
            Key::End => csi(b'F', None),

            // CSI cursor keys with Ctrl modifier (5)
            Key::CtrlArrowUp => csi(b'A', Some(5)),
            Key::CtrlArrowDown => csi(b'B', Some(5)),
            Key::CtrlArrowRight => csi(b'C', Some(5)),
            Key::CtrlArrowLeft => csi(b'D', Some(5)),
            Key::CtrlHome => csi(b'H', Some(5)),
            Key::CtrlEnd => csi(b'F', Some(5)),

            // CSI tilde keys: ESC [ {num} ~
            Key::Delete => csi_tilde(3),
            Key::PageUp => csi_tilde(5),
            Key::PageDown => csi_tilde(6),

            // Non-input events produce no bytes
            Key::Resize(..) => vec![],
        }
    }
}

/// Parse a vim-notation key sequence (e.g. `<Esc>`, `<C-x>`, bare chars) into
/// a list of `Key`s. Use `<lt>` for a literal `<`; `>` needs no escaping.
/// Returns `None` if any token is unrecognised.
pub fn parse_key_sequence(s: &str) -> Option<Vec<Key>> {
    let mut keys = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut token = String::new();
            loop {
                match chars.next() {
                    Some('>') => break,
                    Some(ch) => token.push(ch),
                    None => return None,
                }
            }
            let low = token.to_lowercase();
            let key = if low == "esc" || low == "escape" {
                Key::Escape
            } else if low == "cr" || low == "enter" || low == "return" {
                Key::Enter
            } else if low == "bs" || low == "backspace" {
                Key::Backspace
            } else if low == "tab" {
                Key::Tab
            } else if low == "s-tab" || low == "shifttab" {
                Key::ShiftTab
            } else if low == "del" || low == "delete" {
                Key::Delete
            } else if low == "up" {
                Key::ArrowUp
            } else if low == "down" {
                Key::ArrowDown
            } else if low == "left" {
                Key::ArrowLeft
            } else if low == "right" {
                Key::ArrowRight
            } else if low == "home" {
                Key::Home
            } else if low == "end" {
                Key::End
            } else if low == "pageup" {
                Key::PageUp
            } else if low == "pagedown" {
                Key::PageDown
            } else if low == "space" {
                Key::Char(' ')
            } else if low == "s-space" || low == "shiftspace" {
                Key::ShiftSpace
            } else if low == "lt" {
                Key::Char('<')
            } else if low.starts_with("c-s-") && low.len() == 5 {
                let ch = low.chars().nth(4)?;
                Key::CtrlShift(ch as u8)
            } else if low.starts_with("c-") && low.len() == 3 {
                let ch = low.chars().nth(2)?;
                Key::Ctrl(ch as u8)
            } else if low.starts_with("a-") && low.len() == 3 {
                let ch = token.chars().nth(2)?;
                Key::Alt(ch as u8)
            } else {
                return None;
            };
            keys.push(key);
        } else {
            keys.push(Key::Char(c));
        }
    }
    if keys.is_empty() {
        None
    } else {
        Some(keys)
    }
}

/// Build a CSI sequence: `ESC [ {suffix}` or `ESC [ 1 ; {modifier} {suffix}`
fn csi(suffix: u8, modifier: Option<u8>) -> Vec<u8> {
    match modifier {
        None => vec![0x1b, b'[', suffix],
        Some(m) => vec![0x1b, b'[', b'1', b';', b'0' + m, suffix],
    }
}

/// Build a CSI tilde sequence: `ESC [ {num} ~`
fn csi_tilde(num: u8) -> Vec<u8> {
    vec![0x1b, b'[', b'0' + num, b'~']
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lt_escapes_to_literal_char() {
        assert_eq!(parse_key_sequence("<lt>"), Some(vec![Key::Char('<')]));
    }

    #[test]
    fn gt_needs_no_escaping() {
        assert_eq!(parse_key_sequence(">"), Some(vec![Key::Char('>')]));
    }

    #[test]
    fn bare_lt_without_closing_gt_is_invalid() {
        assert_eq!(parse_key_sequence("<"), None);
    }
}
