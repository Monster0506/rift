use crate::key::Key;

/// Returns `None` for keys with no single wire token (Resize is local-only;
/// Paste's text doesn't fit vim notation - callers decompose it first).
pub fn vim_to_key_sendable(k: Key) -> Option<String> {
    match k {
        Key::Resize(_, _) => None,
        Key::Paste(_) => None,
        other => Some(vim_to_key(other)),
    }
}

pub fn vim_to_key(k: Key) -> String {
    match k {
        Key::Char('<') => "<lt>".into(),
        Key::Char(c) => c.to_string(),
        Key::Ctrl(c) => format!("<C-{}>", (c as char).to_ascii_lowercase()),
        Key::CtrlShift(c) => format!("<C-S-{}>", (c as char).to_ascii_lowercase()),
        Key::Alt(c) => format!("<A-{}>", (c as char).to_ascii_lowercase()),
        Key::AltShift(c) => format!("<A-S-{}>", (c as char).to_ascii_lowercase()),
        Key::Escape => "<Esc>".into(),
        Key::Enter => "<Enter>".into(),
        Key::Tab => "<Tab>".into(),
        Key::ShiftTab => "<S-Tab>".into(),
        Key::ShiftSpace => "<S-Space>".into(),
        Key::Backspace => "<BS>".into(),
        Key::Delete => "<Del>".into(),
        Key::ArrowUp => "<Up>".into(),
        Key::ArrowDown => "<Down>".into(),
        Key::ArrowLeft => "<Left>".into(),
        Key::ArrowRight => "<Right>".into(),
        Key::CtrlArrowUp => "<C-Up>".into(),
        Key::CtrlArrowDown => "<C-Down>".into(),
        Key::CtrlArrowLeft => "<C-Left>".into(),
        Key::CtrlArrowRight => "<C-Right>".into(),
        Key::Home => "<Home>".into(),
        Key::End => "<End>".into(),
        Key::CtrlHome => "<C-Home>".into(),
        Key::CtrlEnd => "<C-End>".into(),
        Key::PageUp => "<PageUp>".into(),
        Key::PageDown => "<PageDown>".into(),
        Key::Resize(_, _) => "<Resize>".into(),
        Key::Paste(text) => format!("<Paste:{}b>", text.len()),
    }
}

pub fn key_to_vim(s: &str) -> Option<Key> {
    if s.is_empty() {
        return None;
    }
    if !s.starts_with('<') {
        let mut chars = s.chars();
        let c = chars.next()?;
        return if chars.next().is_none() {
            Some(Key::Char(c))
        } else {
            None
        };
    }
    match s {
        "<lt>" => return Some(Key::Char('<')),
        "<Esc>" => return Some(Key::Escape),
        "<Enter>" => return Some(Key::Enter),
        "<Tab>" => return Some(Key::Tab),
        "<S-Tab>" => return Some(Key::ShiftTab),
        "<S-Space>" => return Some(Key::ShiftSpace),
        "<BS>" => return Some(Key::Backspace),
        "<Del>" => return Some(Key::Delete),
        "<Up>" => return Some(Key::ArrowUp),
        "<Down>" => return Some(Key::ArrowDown),
        "<Left>" => return Some(Key::ArrowLeft),
        "<Right>" => return Some(Key::ArrowRight),
        "<C-Up>" => return Some(Key::CtrlArrowUp),
        "<C-Down>" => return Some(Key::CtrlArrowDown),
        "<C-Left>" => return Some(Key::CtrlArrowLeft),
        "<C-Right>" => return Some(Key::CtrlArrowRight),
        "<Home>" => return Some(Key::Home),
        "<End>" => return Some(Key::End),
        "<C-Home>" => return Some(Key::CtrlHome),
        "<C-End>" => return Some(Key::CtrlEnd),
        "<PageUp>" => return Some(Key::PageUp),
        "<PageDown>" => return Some(Key::PageDown),
        _ => {}
    }
    let inner = s.strip_prefix('<')?.strip_suffix('>')?;
    if let Some(rest) = inner
        .strip_prefix("C-S-")
        .or_else(|| inner.strip_prefix("S-C-"))
    {
        if rest.len() == 1 {
            let c = rest.chars().next()?.to_ascii_lowercase() as u8;
            return Some(Key::CtrlShift(c));
        }
    }
    if let Some(rest) = inner.strip_prefix("C-") {
        if rest.len() == 1 {
            let c = rest.chars().next()?.to_ascii_lowercase() as u8;
            return Some(Key::Ctrl(c));
        }
    }
    if let Some(rest) = inner
        .strip_prefix("A-S-")
        .or_else(|| inner.strip_prefix("S-A-"))
    {
        if rest.len() == 1 {
            let c = rest.chars().next()?.to_ascii_lowercase() as u8;
            return Some(Key::AltShift(c));
        }
    }
    if let Some(rest) = inner.strip_prefix("A-") {
        if rest.len() == 1 {
            let c = rest.chars().next()?.to_ascii_lowercase() as u8;
            return Some(Key::Alt(c));
        }
    }
    None
}

#[cfg(test)]
#[path = "key_notation_tests.rs"]
mod key_notation_tests;
