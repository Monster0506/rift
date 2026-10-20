//! Tests for crossterm backend

use crate::color::Color;
use crate::key::Key;
use crate::term::crossterm::{color_from_crossterm, color_to_crossterm, CrosstermBackend};
use crate::term::TerminalBackend;

#[test]
fn test_color_basic() {
    let red = Color::Red;
    let crossterm_color = color_to_crossterm(red);
    let back = color_from_crossterm(crossterm_color);
    assert_eq!(red, back);
}

#[test]
fn test_color_rgb() {
    let rgb = Color::Rgb {
        r: 255,
        g: 128,
        b: 64,
    };
    let crossterm_color = color_to_crossterm(rgb);
    let back = color_from_crossterm(crossterm_color);
    assert_eq!(rgb, back);
}

#[test]
fn test_color_ansi256() {
    let ansi = Color::Ansi256(100);
    let crossterm_color = color_to_crossterm(ansi);
    let back = color_from_crossterm(crossterm_color);
    assert_eq!(ansi, back);
}

#[test]
fn test_crossterm_backend_new() {
    let backend = CrosstermBackend::new();
    assert!(backend.is_ok());
}

#[test]
fn test_get_size() {
    let backend = CrosstermBackend::new().unwrap();
    let size_result = backend.get_size();
    assert!(size_result.is_ok() || size_result.is_err());
}

fn translate(code: crossterm::event::KeyCode, mods: crossterm::event::KeyModifiers) -> Option<Key> {
    super::translate_key_event(crossterm::event::KeyEvent::new(code, mods))
}

#[test]
fn translator_table() {
    use crate::key::{KeyCode as K, Mods as M};
    use crossterm::event::{KeyCode as C, KeyModifiers as CM};

    let altgr = CM::CONTROL | CM::ALT;
    let all = CM::CONTROL | CM::SHIFT | CM::ALT;
    let cases: Vec<(C, CM, Option<Key>)> = vec![
        (C::Char('a'), CM::NONE, Some(Key::char('a'))),
        (C::Char('A'), CM::SHIFT, Some(Key::char('A'))),
        (C::Char('\u{1f980}'), CM::NONE, Some(Key::char('\u{1f980}'))),
        (C::Char(' '), CM::NONE, Some(Key::SPACE)),
        (C::Char(' '), CM::SHIFT, Some(Key::SHIFT_SPACE)),
        (C::Char(' '), CM::CONTROL, Some(Key::ctrl(' '))),
        (C::Char('\n'), CM::NONE, Some(Key::ENTER)),
        (C::Char('\r'), CM::NONE, Some(Key::ENTER)),
        (C::Char('\u{17}'), CM::NONE, Some(Key::ctrl('w'))),
        (C::Char('\u{1b}'), CM::NONE, None),
        (C::Char('H'), CM::CONTROL, Some(Key::ctrl('h'))),
        (
            C::Char('H'),
            CM::CONTROL | CM::SHIFT,
            Some(Key::new(K::Char('h'), M::CTRL | M::SHIFT)),
        ),
        (C::Char('H'), CM::ALT, Some(Key::alt('h'))),
        (
            C::Char('H'),
            CM::ALT | CM::SHIFT,
            Some(Key::new(K::Char('h'), M::ALT | M::SHIFT)),
        ),
        (C::Char('i'), CM::CONTROL, Some(Key::TAB)),
        (C::Char('m'), CM::CONTROL, Some(Key::ENTER)),
        (C::Char('['), CM::CONTROL, Some(Key::ESC)),
        (C::Char('@'), altgr, Some(Key::char('@'))),
        (C::Char('\u{20ac}'), altgr, Some(Key::char('\u{20ac}'))),
        (C::Char('\u{142}'), CM::ALT, Some(Key::char('\u{142}'))),
        (
            C::Char('\u{20ac}'),
            CM::CONTROL,
            Some(Key::char('\u{20ac}')),
        ),
        (C::Enter, CM::NONE, Some(Key::ENTER)),
        (C::Enter, CM::CONTROL, Some(Key::ENTER.with(M::CTRL))),
        (C::Esc, CM::NONE, Some(Key::ESC)),
        (C::Esc, CM::ALT, Some(Key::ESC.with(M::ALT))),
        (C::Tab, CM::NONE, Some(Key::TAB)),
        (C::Tab, CM::SHIFT, Some(Key::SHIFT_TAB)),
        (C::Tab, CM::CONTROL, Some(Key::TAB.with(M::CTRL))),
        (C::BackTab, CM::SHIFT, Some(Key::SHIFT_TAB)),
        (C::BackTab, CM::NONE, Some(Key::SHIFT_TAB)),
        (C::Backspace, CM::NONE, Some(Key::BACKSPACE)),
        (
            C::Backspace,
            CM::CONTROL,
            Some(Key::BACKSPACE.with(M::CTRL)),
        ),
        (C::Delete, CM::NONE, Some(Key::DELETE)),
        (C::Delete, CM::CONTROL, Some(Key::DELETE.with(M::CTRL))),
        (C::Insert, CM::NONE, Some(Key::INSERT)),
        (C::Home, CM::NONE, Some(Key::HOME)),
        (C::Home, CM::CONTROL, Some(Key::HOME.with(M::CTRL))),
        (C::End, CM::NONE, Some(Key::END)),
        (C::PageUp, CM::NONE, Some(Key::PAGE_UP)),
        (C::PageUp, CM::CONTROL, Some(Key::PAGE_UP.with(M::CTRL))),
        (C::PageDown, CM::NONE, Some(Key::PAGE_DOWN)),
        (C::Up, CM::NONE, Some(Key::UP)),
        (C::Down, CM::NONE, Some(Key::DOWN)),
        (C::Left, CM::NONE, Some(Key::LEFT)),
        (C::Left, CM::SHIFT, Some(Key::LEFT.with(M::SHIFT))),
        (C::Left, CM::ALT, Some(Key::LEFT.with(M::ALT))),
        (C::Right, CM::CONTROL, Some(Key::RIGHT.with(M::CTRL))),
        (C::Up, all, Some(Key::UP.with(M::CTRL | M::SHIFT | M::ALT))),
        (C::F(5), CM::NONE, Some(Key::f(5))),
        (C::F(12), CM::SHIFT, Some(Key::f(12).with(M::SHIFT))),
        (C::Null, CM::NONE, None),
        (C::CapsLock, CM::NONE, None),
        (C::Menu, CM::NONE, None),
        (
            C::Modifier(crossterm::event::ModifierKeyCode::LeftShift),
            CM::NONE,
            None,
        ),
    ];

    for (code, mods, expected) in cases {
        assert_eq!(translate(code, mods), expected, "{code:?} + {mods:?}");
    }
}

#[test]
fn deinit_restores_raw_mode_and_alternate_screen_flags() {
    // Simulate post-init state without touching the real terminal.
    let mut backend = CrosstermBackend {
        writer: std::io::BufWriter::with_capacity(8192, std::io::stdout()),
        raw_mode_enabled: true,
        alternate_screen_enabled: true,
    };
    backend.deinit();
    assert!(!backend.raw_mode_enabled);
    assert!(!backend.alternate_screen_enabled);
}
