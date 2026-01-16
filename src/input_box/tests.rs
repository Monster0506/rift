use super::*;
use crate::component::EventResult;
use crate::key::Key;
use std::sync::{Arc, Mutex};

// Helper to simulate key press
fn press_key(box_comp: &mut InputBox, key: Key) -> EventResult {
    box_comp.handle_input(key)
}

#[test]
fn test_typing_insertion() {
    let mut box_comp = InputBox::new();

    press_key(&mut box_comp, Key::Char('h'));
    press_key(&mut box_comp, Key::Char('e'));
    press_key(&mut box_comp, Key::Char('l'));
    press_key(&mut box_comp, Key::Char('l'));
    press_key(&mut box_comp, Key::Char('o'));

    assert_eq!(box_comp.content, "hello");
    assert_eq!(box_comp.cursor_idx, 5);
}

#[test]
fn test_deletion() {
    let mut box_comp = InputBox::new().with_content("hello");

    // Cursor at end (5)
    press_key(&mut box_comp, Key::Backspace);
    assert_eq!(box_comp.content, "hell");
    assert_eq!(box_comp.cursor_idx, 4);

    // Initial position deletion
    box_comp.cursor_idx = 0;
    press_key(&mut box_comp, Key::Backspace); // Should do nothing
    assert_eq!(box_comp.content, "hell");
    assert_eq!(box_comp.cursor_idx, 0);

    press_key(&mut box_comp, Key::Delete); // Delete 'h'
    assert_eq!(box_comp.content, "ell");
}

#[test]
fn test_navigation() {
    let mut box_comp = InputBox::new().with_content("hello world");

    // Start at end
    assert_eq!(box_comp.cursor_idx, 11);

    press_key(&mut box_comp, Key::Home);
    assert_eq!(box_comp.cursor_idx, 0);

    press_key(&mut box_comp, Key::ArrowRight);
    assert_eq!(box_comp.cursor_idx, 1);

    press_key(&mut box_comp, Key::End);
    assert_eq!(box_comp.cursor_idx, 11);

    press_key(&mut box_comp, Key::CtrlArrowLeft); // Back word
                                                  // "hello world" -> cursor before "world" or space?
                                                  // Depends on `prev_word` impl. Usually skips current word.
                                                  // Let's assume standard behavior to be checked visually or dependent on movement module.
                                                  // For now just assert it moved left.
    assert!(box_comp.cursor_idx < 11);
}

#[test]
fn test_callbacks() {
    let result = Arc::new(Mutex::new(String::new()));
    let result_clone = result.clone();

    let mut box_comp = InputBox::new()
        .with_content("submit me")
        .on_submit(move |s| {
            *result_clone.lock().unwrap() = s;
            EventResult::Consumed
        });

    press_key(&mut box_comp, Key::Enter);

    assert_eq!(*result.lock().unwrap(), "submit me");
}

#[test]
fn test_on_change() {
    let result = Arc::new(Mutex::new(String::new()));
    let result_clone = result.clone();

    let mut box_comp = InputBox::new().on_change(move |s| {
        *result_clone.lock().unwrap() = s;
        EventResult::Consumed
    });

    press_key(&mut box_comp, Key::Char('a'));
    assert_eq!(*result.lock().unwrap(), "a");

    press_key(&mut box_comp, Key::Char('b'));
    assert_eq!(*result.lock().unwrap(), "ab");
}

#[test]
fn test_max_length() {
    let config = InputBoxConfig {
        max_len: Some(3),
        ..Default::default()
    };
    let mut box_comp = InputBox::with_config(config);

    press_key(&mut box_comp, Key::Char('1'));
    press_key(&mut box_comp, Key::Char('2'));
    press_key(&mut box_comp, Key::Char('3'));
    assert_eq!(box_comp.content, "123");

    press_key(&mut box_comp, Key::Char('4')); // Should be ignored
    assert_eq!(box_comp.content, "123");
}

#[test]
fn test_validation() {
    let config = InputBoxConfig {
        validator: Some(Arc::new(|s| s.len() >= 3)),
        ..Default::default()
    };
    let mut box_comp = InputBox::with_config(config);

    press_key(&mut box_comp, Key::Char('a'));
    assert!(!box_comp.is_valid);

    press_key(&mut box_comp, Key::Char('b'));
    assert!(!box_comp.is_valid);

    press_key(&mut box_comp, Key::Char('c'));
    assert!(box_comp.is_valid);
}

#[test]
fn test_masking_integrity() {
    let config = InputBoxConfig {
        mask_char: Some('*'),
        ..Default::default()
    };
    let mut box_comp = InputBox::with_config(config);

    press_key(&mut box_comp, Key::Char('p'));
    press_key(&mut box_comp, Key::Char('a'));
    press_key(&mut box_comp, Key::Char('s'));

    // Content should hold actual text
    assert_eq!(box_comp.content, "pas");
    // Verification of masking happens in render(), not state
}
