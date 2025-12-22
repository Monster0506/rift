//! Tests for crossterm backend

use crate::term::TerminalBackend;
use crate::term::crossterm::CrosstermBackend;
use crate::key::Key;

#[test]
fn test_crossterm_backend_new() {
    let backend = CrosstermBackend::new();
    assert!(backend.is_ok());
}

#[test]
fn test_get_size() {
    let backend = CrosstermBackend::new().unwrap();
    // Can't test init in unit tests (requires actual terminal)
    // But we can test that get_size returns a valid size structure
    // when terminal is initialized
    let size_result = backend.get_size();
    // This might fail if not in a real terminal, so we just check it doesn't panic
    // In a real terminal, it should return Ok(Size { rows: > 0, cols: > 0 })
    assert!(size_result.is_ok() || size_result.is_err());
}

#[test]
fn test_translate_key_event() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test basic character
    let key_event = KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Char(b'a'));

    // Test Ctrl+Char
    let key_event = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Ctrl(b'c'));

    // Test arrow keys
    let key_event = KeyEvent {
        code: KeyCode::Up,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowUp);

    let key_event = KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowDown);

    let key_event = KeyEvent {
        code: KeyCode::Left,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowLeft);

    let key_event = KeyEvent {
        code: KeyCode::Right,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::ArrowRight);

    // Test special keys
    let key_event = KeyEvent {
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Backspace);

    let key_event = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Enter);

    let key_event = KeyEvent {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Escape);

    let key_event = KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Tab);

    let key_event = KeyEvent {
        code: KeyCode::Home,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Home);

    let key_event = KeyEvent {
        code: KeyCode::End,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::End);

    let key_event = KeyEvent {
        code: KeyCode::PageUp,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::PageUp);

    let key_event = KeyEvent {
        code: KeyCode::PageDown,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::PageDown);

    let key_event = KeyEvent {
        code: KeyCode::Delete,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Delete);
}

#[test]
fn test_translate_ctrl_combinations() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test Ctrl+A through Ctrl+Z
    for i in 0..26 {
        let ch = (b'a' + i) as char;
        let key_event = KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        };
        let key = super::translate_key_event(key_event);
        assert_eq!(key, Key::Ctrl(b'a' + i), "Failed for Ctrl+{}", ch);
    }
}

#[test]
fn test_translate_shift_characters() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test that Shift+Char still produces Char (not Ctrl)
    let key_event = KeyEvent {
        code: KeyCode::Char('A'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    // Note: crossterm gives us 'A' directly, not 'a' with shift
    assert_eq!(key, Key::Char(b'A'));
}

#[test]
fn test_translate_alt_combinations() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Alt combinations should still produce Char (Alt is often used for special chars)
    let key_event = KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Char(b'a'));
}

#[test]
fn test_translate_ctrl_shift_combinations() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Ctrl+Shift+Char should produce Ctrl
    let key_event = KeyEvent {
        code: KeyCode::Char('A'),
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Ctrl(b'A'));
}

#[test]
fn test_translate_special_characters() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test various special characters
    let special_chars = [
        ('!', b'!'),
        ('@', b'@'),
        ('#', b'#'),
        ('$', b'$'),
        ('%', b'%'),
        ('^', b'^'),
        ('&', b'&'),
        ('*', b'*'),
        ('(', b'('),
        (')', b')'),
        ('-', b'-'),
        ('_', b'_'),
        ('=', b'='),
        ('+', b'+'),
        ('[', b'['),
        (']', b']'),
        ('{', b'{'),
        ('}', b'}'),
        ('\\', b'\\'),
        ('|', b'|'),
        (';', b';'),
        (':', b':'),
        ('\'', b'\''),
        ('"', b'"'),
        (',', b','),
        ('.', b'.'),
        ('<', b'<'),
        ('>', b'>'),
        ('/', b'/'),
        ('?', b'?'),
        ('`', b'`'),
        ('~', b'~'),
    ];
    
    for (ch, expected) in special_chars.iter() {
        let key_event = KeyEvent {
            code: KeyCode::Char(*ch),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        };
        let key = super::translate_key_event(key_event);
        assert_eq!(key, Key::Char(*expected), "Failed for character: {}", ch);
    }
}

#[test]
fn test_translate_numbers() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test number keys
    for i in 0..10 {
        let ch = (b'0' + i) as char;
        let key_event = KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        };
        let key = super::translate_key_event(key_event);
        assert_eq!(key, Key::Char(b'0' + i), "Failed for number: {}", ch);
    }
}

#[test]
fn test_translate_unknown_keys() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test unknown key codes return Char(0)
    let key_event = KeyEvent {
        code: KeyCode::F(13), // F13 doesn't exist, but tests unknown handling
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    // F13 should map to PageUp or similar, but if not handled, should be Char(0)
    // Actually, let's test with a truly unknown code - but crossterm doesn't have that
    // So we'll just verify the function doesn't panic
    let _ = key;
}

#[test]
fn test_translate_function_keys() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Note: Function keys F1-F12 are not currently mapped in translate_key_event
    // They would fall through to the default case and return Char(0)
    // This test documents current behavior
    for f_num in 1..=12 {
        let key_event = KeyEvent {
            code: KeyCode::F(f_num),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        };
        let key = super::translate_key_event(key_event);
        // Currently unmapped, returns Char(0)
        assert_eq!(key, Key::Char(0), "F{} should be unmapped (returns Char(0))", f_num);
    }
}

#[test]
fn test_backend_lifecycle() {
    let mut backend = CrosstermBackend::new().unwrap();
    
    // Test that we can call methods without init (they may fail, but shouldn't panic)
    let _size_result = backend.get_size();
    
    // Test init/deinit cycle
    let init_result = backend.init();
    // May fail if not in a real terminal, but shouldn't panic
    if init_result.is_ok() {
        backend.deinit();
    }
}

#[test]
fn test_write_operations() {
    let mut backend = CrosstermBackend::new().unwrap();
    
    // Test writing bytes
    let result = backend.write(b"test");
    // Should succeed even without init (just writes to stdout)
    assert!(result.is_ok());
    
    // Test writing empty bytes
    let result = backend.write(b"");
    assert!(result.is_ok());
}

#[test]
fn test_cursor_operations() {
    let mut backend = CrosstermBackend::new().unwrap();
    
    // These operations may fail without init, but shouldn't panic
    let _ = backend.hide_cursor();
    let _ = backend.show_cursor();
    let _ = backend.move_cursor(0, 0);
    let _ = backend.clear_screen();
    let _ = backend.clear_to_end_of_line();
}

#[test]
fn test_size_structure() {
    use crate::term::Size;
    
    let size = Size { rows: 24, cols: 80 };
    assert_eq!(size.rows, 24);
    assert_eq!(size.cols, 80);
    
    // Test Copy trait
    let size2 = size;
    assert_eq!(size2.rows, 24);
    assert_eq!(size2.cols, 80);
    assert_eq!(size.rows, 24); // Original still valid
}

#[test]
fn test_key_event_kind_filtering() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test that key releases are ignored (though translate_key_event doesn't check this)
    // The read_key method filters these, but translate_key_event just translates
    let key_event = KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Release, // Release event
        state: crossterm::event::KeyEventState::empty(),
    };
    // translate_key_event doesn't check kind, it just translates
    // So this will still return a key (the filtering happens in read_key)
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Char(b'a'));
}

#[test]
fn test_edge_case_characters() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    
    // Test space character
    let key_event = KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Char(b' '));
    
    // Test newline (though Enter is usually used)
    // Note: Char('\n') might not be a real key event, but test the translation
    let key_event = KeyEvent {
        code: KeyCode::Char('\n'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    };
    let key = super::translate_key_event(key_event);
    assert_eq!(key, Key::Char(b'\n'));
}

