//! The one key notation: `<C-S-A-Left>`, `<Space>`, `<F5>`, any Unicode char.
//! Shared by plugin `map` strings, IPC, replay scripts and the status bar.

use super::{Key, KeyCode, Mods};

const MAX_F_KEY: u8 = 24;
const CODEPOINT_PREFIX: &str = "u+";

const NAMED: &[(&str, KeyCode)] = &[
    ("esc", KeyCode::Esc),
    ("escape", KeyCode::Esc),
    ("cr", KeyCode::Enter),
    ("enter", KeyCode::Enter),
    ("return", KeyCode::Enter),
    ("bs", KeyCode::Backspace),
    ("backspace", KeyCode::Backspace),
    ("tab", KeyCode::Tab),
    ("del", KeyCode::Delete),
    ("delete", KeyCode::Delete),
    ("ins", KeyCode::Insert),
    ("insert", KeyCode::Insert),
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
    ("space", KeyCode::Char(' ')),
    ("lt", KeyCode::Char('<')),
    ("gt", KeyCode::Char('>')),
];

/// Format a key. Round-trips through [`parse_key`].
#[must_use]
pub fn format_key(key: Key) -> String {
    if let KeyCode::Char(c) = key.code {
        let bare = key.mods.is_empty() && c != ' ' && c != '<' && !c.is_control();
        if bare {
            return c.to_string();
        }
    }

    let mut out = String::from("<");
    for (flag, prefix) in [(Mods::CTRL, "C-"), (Mods::SHIFT, "S-"), (Mods::ALT, "A-")] {
        if key.mods.contains(flag) {
            out.push_str(prefix);
        }
    }
    out.push_str(&code_name(key.code));
    out.push('>');
    out
}

fn code_name(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "Space".into(),
        KeyCode::Char('<') => "lt".into(),
        KeyCode::Char('>') => "gt".into(),
        KeyCode::Char(c) if c.is_control() => format!("{CODEPOINT_PREFIX}{:04x}", c as u32),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Insert => "Insert".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Backspace => "BS".into(),
        KeyCode::Delete => "Del".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
    }
}

/// Parse exactly one key: a bare character or a `<...>` token.
#[must_use]
pub fn parse_key(s: &str) -> Option<Key> {
    let mut chars = s.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return (first != '<').then(|| Key::char(first));
    }
    parse_token(s.strip_prefix('<')?.strip_suffix('>')?)
}

/// Parse key notation into a key sequence. Use `<lt>` for a literal `<`.
/// Returns None if any token is unrecognised.
#[must_use]
pub fn parse_key_sequence(s: &str) -> Option<Vec<Key>> {
    let mut keys = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '<' {
            keys.push(Key::char(c));
            continue;
        }

        let mut token = String::new();
        loop {
            match chars.next()? {
                '>' => break,
                ch => token.push(ch),
            }
        }
        keys.push(parse_token(&token)?);
    }

    (!keys.is_empty()).then_some(keys)
}

fn parse_token(inner: &str) -> Option<Key> {
    let mut mods = Mods::NONE;
    let mut rest = inner;
    while let Some((flag, tail)) = split_mod(rest) {
        mods = mods | flag;
        rest = tail;
    }

    Some(Key::new(parse_code(rest)?, mods))
}

fn split_mod(s: &str) -> Option<(Mods, &str)> {
    let mut chars = s.chars();
    let flag = match chars.next()?.to_ascii_lowercase() {
        'c' => Mods::CTRL,
        's' => Mods::SHIFT,
        'a' => Mods::ALT,
        _ => return None,
    };
    let tail = chars.as_str().strip_prefix('-')?;
    (!tail.is_empty()).then_some((flag, tail))
}

fn parse_code(name: &str) -> Option<KeyCode> {
    let mut chars = name.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return Some(KeyCode::Char(first));
    }

    let low = name.to_lowercase();
    if let Some((_, code)) = NAMED.iter().find(|(n, _)| *n == low) {
        return Some(*code);
    }
    if let Some(digits) = low.strip_prefix('f') {
        let n: u8 = digits.parse().ok()?;
        return (1..=MAX_F_KEY).contains(&n).then_some(KeyCode::F(n));
    }
    let hex = low.strip_prefix(CODEPOINT_PREFIX)?;
    char::from_u32(u32::from_str_radix(hex, 16).ok()?).map(KeyCode::Char)
}
