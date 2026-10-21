use super::common::*;
use crate::action::{Action, EditorAction, Motion};

fn act(editor: &mut crate::editor::Editor<crate::test_utils::MockTerminal>, a: EditorAction) {
    editor.handle_action(&Action::Editor(a));
}

#[test]
fn esc_after_append_moves_cursor_left() {
    let mut editor = create_editor();
    load_text(&mut editor, "abc");

    act(&mut editor, EditorAction::EnterInsertModeAtLineEnd);
    act(&mut editor, EditorAction::InsertChar('x'));
    act(&mut editor, EditorAction::EnterNormalMode);

    assert_eq!(editor.active_document().buffer.to_string(), "abcx");
    assert_eq!(editor.active_document().buffer.cursor(), 3);
}

#[test]
fn esc_on_empty_line_stays_at_column_zero() {
    let mut editor = create_editor();
    load_text(&mut editor, "");

    act(&mut editor, EditorAction::EnterInsertModeAtLineEnd);
    act(&mut editor, EditorAction::EnterNormalMode);

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn esc_at_line_start_of_second_line_does_not_cross_lines() {
    let mut editor = create_editor();
    load_text(&mut editor, "abc\n");

    act(&mut editor, EditorAction::Move(Motion::Down));
    act(&mut editor, EditorAction::EnterInsertModeAtLineStart);
    act(&mut editor, EditorAction::EnterNormalMode);

    assert_eq!(editor.active_document().buffer.cursor(), 4);
}

#[test]
fn append_then_esc_then_down_keeps_cursor_column() {
    let mut editor = create_editor();
    load_text(&mut editor, "abc\nabcdefgh");

    act(&mut editor, EditorAction::EnterInsertModeAtLineEnd);
    act(&mut editor, EditorAction::InsertChar('x'));
    act(&mut editor, EditorAction::EnterNormalMode);
    act(&mut editor, EditorAction::Move(Motion::Down));

    assert_eq!(editor.active_document().buffer.get_col(), 3);
}

#[test]
fn esc_from_visual_does_not_move_cursor() {
    let mut editor = create_editor();
    load_text(&mut editor, "abc");

    act(&mut editor, EditorAction::EnterVisualChar);
    act(&mut editor, EditorAction::Move(Motion::Right));
    act(&mut editor, EditorAction::EnterNormalMode);

    assert_eq!(editor.active_document().buffer.cursor(), 1);
}
