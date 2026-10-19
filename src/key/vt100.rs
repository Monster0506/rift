//! xterm byte encoding of keys for PTY input.

use super::{Key, KeyCode, Mods};

const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;
const BACKSPACE_CTRL: u8 = 0x08;
const CTRL_MASK: u8 = 0x1f;

/// xterm tilde codes (`CSI <n> ~`) for the keys that use them.
const TILDE_F_KEYS: &[(u8, u8)] = &[
    (5, 15),
    (6, 17),
    (7, 18),
    (8, 19),
    (9, 20),
    (10, 21),
    (11, 23),
    (12, 24),
    (13, 25),
    (14, 26),
    (15, 28),
    (16, 29),
    (17, 31),
    (18, 32),
    (19, 33),
    (20, 34),
];

/// Keys with no encoding yield no bytes.
pub(super) fn encode(key: Key) -> Vec<u8> {
    let mods = key.mods;
    let (bytes, alt_prefix) = match key.code {
        KeyCode::Char(c) => (char_bytes(c, mods), true),
        KeyCode::Enter => (vec![b'\r'], true),
        KeyCode::Esc => (vec![ESC], true),
        KeyCode::Tab if mods.contains(Mods::SHIFT) => (csi_final(b'Z', Mods::NONE), false),
        KeyCode::Tab => (vec![b'\t'], true),
        KeyCode::Backspace if mods.contains(Mods::CTRL) => (vec![BACKSPACE_CTRL], true),
        KeyCode::Backspace => (vec![DEL], true),
        KeyCode::Up => (csi_final(b'A', mods), false),
        KeyCode::Down => (csi_final(b'B', mods), false),
        KeyCode::Right => (csi_final(b'C', mods), false),
        KeyCode::Left => (csi_final(b'D', mods), false),
        KeyCode::Home => (csi_final(b'H', mods), false),
        KeyCode::End => (csi_final(b'F', mods), false),
        KeyCode::Insert => (csi_tilde(2, mods), false),
        KeyCode::Delete => (csi_tilde(3, mods), false),
        KeyCode::PageUp => (csi_tilde(5, mods), false),
        KeyCode::PageDown => (csi_tilde(6, mods), false),
        KeyCode::F(n) => (f_key(n, mods), false),
    };

    if alt_prefix && mods.contains(Mods::ALT) && !bytes.is_empty() {
        let mut prefixed = Vec::with_capacity(bytes.len() + 1);
        prefixed.push(ESC);
        prefixed.extend(bytes);
        return prefixed;
    }
    bytes
}

fn char_bytes(c: char, mods: Mods) -> Vec<u8> {
    if mods.contains(Mods::CTRL) {
        return ctrl_byte(c).map(|b| vec![b]).unwrap_or_default();
    }

    let c = if mods.contains(Mods::SHIFT) {
        c.to_ascii_uppercase()
    } else {
        c
    };
    let mut buf = [0; 4];
    c.encode_utf8(&mut buf).as_bytes().to_vec()
}

/// Control byte for Ctrl+`c`; digits and other symbols have none.
fn ctrl_byte(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        'a'..='z' | '[' | '\\' | ']' | '^' | '_' => Some(c.to_ascii_lowercase() as u8 & CTRL_MASK),
        ' ' | '@' => Some(0),
        '?' => Some(DEL),
        _ => None,
    }
}

/// xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
fn mod_param(mods: Mods) -> u8 {
    let mut param = 1;
    if mods.contains(Mods::SHIFT) {
        param += 1;
    }
    if mods.contains(Mods::ALT) {
        param += 2;
    }
    if mods.contains(Mods::CTRL) {
        param += 4;
    }
    param
}

/// `ESC [ <final>`, or `ESC [ 1 ; <mod> <final>` when modified.
fn csi_final(final_byte: u8, mods: Mods) -> Vec<u8> {
    if mods.is_empty() {
        return vec![ESC, b'[', final_byte];
    }
    let mut out = format!("\x1b[1;{}", mod_param(mods)).into_bytes();
    out.push(final_byte);
    out
}

/// `ESC [ <n> ~`, or `ESC [ <n> ; <mod> ~` when modified.
fn csi_tilde(n: u8, mods: Mods) -> Vec<u8> {
    if mods.is_empty() {
        return format!("\x1b[{n}~").into_bytes();
    }
    format!("\x1b[{n};{}~", mod_param(mods)).into_bytes()
}

fn f_key(n: u8, mods: Mods) -> Vec<u8> {
    if (1..=4).contains(&n) {
        let final_byte = b'P' + (n - 1);
        if mods.is_empty() {
            return vec![ESC, b'O', final_byte];
        }
        return csi_final(final_byte, mods);
    }

    match TILDE_F_KEYS.iter().find(|(key, _)| *key == n) {
        Some((_, code)) => csi_tilde(*code, mods),
        None => Vec::new(),
    }
}
