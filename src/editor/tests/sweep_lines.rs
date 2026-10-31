use super::common::*;
use super::*;
use crate::action::{Action, EditorAction};
use crate::test_utils::MockTerminal;

const TEXT: &str = "abc\ndef\nghi\n";

/// Offset of `ghi`, the last line holding text.
const LAST_LINE_START: usize = 8;

/// Offset of the final `i`.
const LAST_CHAR: usize = 10;

/// Screen rows that can show `TEXT` and the row past it.
const TEXT_ROWS: usize = 5;

fn editor_with(text: &str) -> Editor<MockTerminal> {
    let mut editor = create_editor();
    load_text(&mut editor, text);
    editor
}

fn cursor_after(text: &str, count: usize, action: EditorAction) -> usize {
    let mut editor = editor_with(text);
    editor.pending_count = count;
    editor.handle_action(&Action::Editor(action));
    editor.active_document().buffer.cursor()
}

fn text_after(count: usize, keys: &str) -> String {
    let mut editor = editor_with(TEXT);
    feed_keys(&mut editor, "G");
    editor.pending_count = count;
    feed_keys(&mut editor, keys);
    editor.active_document().buffer.to_string()
}

#[test]
fn bug_trailing_newline_makes_phantom_last_line() {
    let goto_last = EditorAction::GotoLine(0);
    let next_paragraph = EditorAction::Move(crate::action::Motion::NextParagraph);

    assert_eq!(cursor_after(TEXT, 0, goto_last.clone()), LAST_LINE_START);
    assert_eq!(cursor_after(TEXT, 99, goto_last), LAST_LINE_START);
    assert_eq!(
        cursor_after(TEXT, 0, EditorAction::GotoLine(4)),
        LAST_LINE_START
    );
    assert_eq!(cursor_after(TEXT, 0, next_paragraph), LAST_CHAR);
}

#[test]
fn bug_dd_on_last_line_trailing_newline_deletes_newline_not_text() {
    assert_eq!(text_after(0, "dd"), "abc\ndef\n");
    assert_eq!(text_after(5, "dd"), "abc\ndef\n");
    assert_eq!(text_after(0, "x"), "abc\ndef\nhi\n");
    assert_eq!(text_after(0, "cc"), "abc\ndef\n\n");
    assert_eq!(text_after(5, "cc"), "abc\ndef\n\n");
}

#[test]
fn bug_phantom_line_numbered_past_eof() {
    let mut editor = editor_with(TEXT);
    editor.state.settings.show_line_numbers = true;

    let screen = render_ascii(&mut editor);
    let numbered: Vec<usize> = screen
        .lines()
        .take(TEXT_ROWS)
        .filter_map(|row| row.split_whitespace().next()?.parse().ok())
        .collect();

    assert_eq!(numbered, [1, 2, 3], "{screen}");
}
