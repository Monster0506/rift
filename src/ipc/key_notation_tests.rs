use super::*;

#[test]
fn char_round_trips() {
    assert_eq!(vim_to_key(Key::Char('j')), "j");
    assert_eq!(key_to_vim("j"), Some(Key::Char('j')));
}

#[test]
fn lt_is_escaped() {
    assert_eq!(vim_to_key(Key::Char('<')), "<lt>");
    assert_eq!(key_to_vim("<lt>"), Some(Key::Char('<')));
}

#[test]
fn ctrl_shift_round_trips() {
    assert_eq!(vim_to_key(Key::CtrlShift(b't')), "<C-S-t>");
    assert_eq!(key_to_vim("<C-S-t>"), Some(Key::CtrlShift(b't')));
    assert_eq!(key_to_vim("<S-C-t>"), Some(Key::CtrlShift(b't')));
}

#[test]
fn special_keys_round_trip() {
    let pairs: &[(Key, &str)] = &[
        (Key::Escape, "<Esc>"),
        (Key::Enter, "<Enter>"),
        (Key::Tab, "<Tab>"),
        (Key::ShiftTab, "<S-Tab>"),
        (Key::ShiftSpace, "<S-Space>"),
        (Key::Backspace, "<BS>"),
        (Key::Delete, "<Del>"),
        (Key::ArrowUp, "<Up>"),
        (Key::ArrowDown, "<Down>"),
        (Key::ArrowLeft, "<Left>"),
        (Key::ArrowRight, "<Right>"),
        (Key::Home, "<Home>"),
        (Key::End, "<End>"),
        (Key::PageUp, "<PageUp>"),
        (Key::PageDown, "<PageDown>"),
    ];
    for (key, notation) in pairs {
        assert_eq!(&vim_to_key(key.clone()), notation);
        assert_eq!(key_to_vim(notation), Some(key.clone()));
    }
}

#[test]
fn ctrl_and_alt_round_trip() {
    assert_eq!(vim_to_key(Key::Ctrl(b'w')), "<C-w>");
    assert_eq!(key_to_vim("<C-w>"), Some(Key::Ctrl(b'w')));
    assert_eq!(vim_to_key(Key::Alt(b'p')), "<A-p>");
    assert_eq!(key_to_vim("<A-p>"), Some(Key::Alt(b'p')));
}

#[test]
fn alt_shift_round_trips() {
    assert_eq!(vim_to_key(Key::AltShift(b't')), "<A-S-t>");
    assert_eq!(key_to_vim("<A-S-t>"), Some(Key::AltShift(b't')));
    assert_eq!(key_to_vim("<S-A-t>"), Some(Key::AltShift(b't')));
}

#[test]
fn alt_notation_normalizes_case() {
    assert_eq!(key_to_vim("<A-P>"), Some(Key::Alt(b'p')));
    assert_eq!(key_to_vim("<A-S-P>"), Some(Key::AltShift(b'p')));
}

#[test]
fn unknown_notation_returns_none() {
    assert_eq!(key_to_vim("<F13>"), None);
}

#[test]
fn resize_is_not_sendable() {
    assert!(vim_to_key_sendable(Key::Resize(80, 24)).is_none());
}
