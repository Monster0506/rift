//! Key representation for editor input

/// Represents a key press event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// Printable character
    Char(char),
    /// Control key combination (e.g., Ctrl+A)
    Ctrl(u8),
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
    /// System events
    Resize(u16, u16),
}

impl Key {
    /// Convert key to VT100/xterm byte sequence for PTY input.
    ///
    /// Sequences follow the CSI (Control Sequence Introducer) convention:
    ///   - Cursor keys:  ESC [ {suffix}          e.g. ESC [ A  (up)
    ///   - Modified keys: ESC [ 1 ; {mod} {suffix}  where mod: 2=Shift, 3=Alt, 5=Ctrl
    ///   - Tilde keys:   ESC [ {num} ~            e.g. ESC [ 3 ~  (delete)
    ///   - Single byte:  direct control character
    pub fn to_vt100_bytes(&self) -> Vec<u8> {
        match self {
            // Printable character → UTF-8 encoding
            Key::Char(c) => {
                let mut buf = [0; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }

            // Ctrl+key → mask to control range (0x00–0x1F)
            Key::Ctrl(c) => vec![c & 0x1f],

            // Single-byte control characters
            Key::Backspace => vec![0x7f],
            Key::Enter => vec![b'\r'],
            Key::Escape => vec![0x1b],
            Key::Tab => vec![b'\t'],

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
