use super::common::*;
use super::*;
use crate::action::{Action, EditorAction, Motion, OperatorType};
use crate::test_utils::MockTerminal;

fn act(editor: &mut Editor<MockTerminal>, action: EditorAction) {
    editor.handle_action(&Action::Editor(action));
}

fn text_after(text: &str, op: OperatorType, motion: EditorAction) -> String {
    let mut editor = create_editor();
    load_text(&mut editor, text);
    act(&mut editor, EditorAction::Operator(op));
    act(&mut editor, motion);
    editor.active_document().buffer.to_string()
}

#[test]
fn bug_linewise_motion_to_current_line_dropped() {
    let goto_last = EditorAction::GotoLine(0);
    let goto_first = EditorAction::Move(Motion::StartOfFile);

    assert_eq!(
        text_after("abc", OperatorType::Delete, goto_last.clone()),
        ""
    );
    assert_eq!(
        text_after("abc\ndef\n", OperatorType::Delete, goto_first),
        "def\n"
    );
    assert_eq!(text_after("abc", OperatorType::Change, goto_last), "");
}

#[test]
fn failed_vertical_motion_aborts_operator() {
    let up = EditorAction::Move(Motion::Up);

    assert_eq!(
        text_after("abc\ndef\n", OperatorType::Delete, up),
        "abc\ndef\n"
    );
}
