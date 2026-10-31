use super::*;
use crate::buffer::api::BufferView;
use crate::replay::backend::ReplayBackend;

fn run(text: &str, keys: &str) -> (String, Mode, usize) {
    let backend = ReplayBackend::new(std::io::sink(), 24, 80);
    let mut editor = Editor::new(backend).unwrap();
    editor.active_document().buffer.insert_str(text).unwrap();
    editor.active_document().buffer.move_to_start();
    editor
        .term
        .push_keys(crate::key::notation::parse_key_sequence(keys).unwrap());

    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }

    let mode = editor.current_mode;
    let doc = editor.active_document();
    let text = doc
        .buffer
        .chars(0..doc.buffer.len())
        .map(crate::character::Character::to_char_lossy)
        .collect();
    (text, mode, doc.buffer.cursor())
}

#[test]
fn replace_preserves_newlines() {
    assert_eq!(
        run("abc\ndef\nghi", "vjrX"),
        ("XXX\nXef\nghi".to_string(), Mode::Normal, 0)
    );
}

#[test]
fn replace_backward_selection() {
    assert_eq!(run("abc", "$vhrX"), ("aXX".to_string(), Mode::Normal, 1));
}

#[test]
fn replace_cancel_keeps_selection() {
    assert_eq!(run("abc", "vlr<Esc>"), ("abc".to_string(), Mode::Visual, 1));
}

#[test]
fn replace_tab_is_literal() {
    assert_eq!(
        run("abc", "vlr<Tab>"),
        ("\t\tc".to_string(), Mode::Normal, 0)
    );
}

#[test]
fn replace_enter_is_literal() {
    assert_eq!(
        run("abc", "vlr<CR>"),
        ("\r\rc".to_string(), Mode::Normal, 0)
    );
}

#[test]
fn replace_empty_buffer() {
    assert_eq!(run("", "vrX"), (String::new(), Mode::Normal, 0));
}

#[test]
fn replace_dot_keeps_character() {
    assert_eq!(
        run("abc def", "vlrXw."),
        ("XXc XXf".to_string(), Mode::Normal, 4)
    );
}

#[test]
fn replace_single_undo() {
    assert_eq!(
        run("abc\ndef", "vjrXu"),
        ("abc\ndef".to_string(), Mode::Normal, 0)
    );
}

#[test]
fn normal_replace_keeps_count() {
    let (text, mode, _) = run("abc", "2rX");
    assert_eq!((text, mode), ("XXc".to_string(), Mode::Normal));
}

#[test]
fn replace_linewise_keeps_empty_lines() {
    assert_eq!(
        run("abc\n\nxyz", "VjjrX"),
        ("XXX\n\nXXX".to_string(), Mode::Normal, 0)
    );
}

#[test]
fn replace_uses_character_offsets() {
    assert_eq!(
        run("a\u{e9}\u{4e2d}", "vlrX"),
        ("XX\u{4e2d}".to_string(), Mode::Normal, 0)
    );
}
