use super::common::*;

#[test]
fn test_format_key_char() {
    assert_eq!(StatusBar::format_key(Key::Char('a')), "a");
    assert_eq!(StatusBar::format_key(Key::Char('Z')), "Z");
    assert_eq!(StatusBar::format_key(Key::Char(' ')), " ");
    assert_eq!(StatusBar::format_key(Key::Char('0')), "0");
}

#[test]
fn test_format_key_non_printable() {
    assert_eq!(StatusBar::format_key(Key::Char('\0')), "\\u{0000}");
    assert_eq!(StatusBar::format_key(Key::Char('\x1f')), "\\u{001f}");
    assert_eq!(StatusBar::format_key(Key::Char('\x7f')), "\\u{007f}");
}

#[test]
fn test_format_key_ctrl() {
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'a')), "Ctrl+A");
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'c')), "Ctrl+C");
    assert_eq!(StatusBar::format_key(Key::Ctrl(b'z')), "Ctrl+Z");
}

#[test]
fn test_format_key_arrows() {
    assert_eq!(StatusBar::format_key(Key::ArrowUp), "Up");
    assert_eq!(StatusBar::format_key(Key::ArrowDown), "Down");
    assert_eq!(StatusBar::format_key(Key::ArrowLeft), "Left");
    assert_eq!(StatusBar::format_key(Key::ArrowRight), "Right");
}

#[test]
fn test_format_key_special() {
    assert_eq!(StatusBar::format_key(Key::Backspace), "Backspace");
    assert_eq!(StatusBar::format_key(Key::Delete), "Delete");
    assert_eq!(StatusBar::format_key(Key::Enter), "Enter");
    assert_eq!(StatusBar::format_key(Key::Escape), "Esc");
    assert_eq!(StatusBar::format_key(Key::Tab), "Tab");
    assert_eq!(StatusBar::format_key(Key::Home), "Home");
    assert_eq!(StatusBar::format_key(Key::End), "End");
    assert_eq!(StatusBar::format_key(Key::PageUp), "PageUp");
    assert_eq!(StatusBar::format_key(Key::PageDown), "PageDown");
}
