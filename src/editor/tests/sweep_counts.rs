use super::common::*;
use super::*;
use crate::action::{Action, EditorAction, Motion, OperatorType};
use crate::test_utils::MockTerminal;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(5);
const TEXT: &str = "alpha beta\ngamma delta\nepsilon\n";

/// What typing `99999999999999999999` saturates `pending_count` to.
const HUGE_COUNT: usize = usize::MAX;

/// Large enough to exhaust every motion on `TEXT`.
const EXHAUSTING_COUNT: usize = 100;

fn run_bounded<F: FnOnce() + Send + 'static>(scenario: F) {
    let (done_tx, done_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        scenario();
        let _ = done_tx.send(());
    });

    if let Err(RecvTimeoutError::Timeout) = done_rx.recv_timeout(DEADLINE) {
        panic!("scenario did not finish within {DEADLINE:?}");
    }
    if let Err(panic) = handle.join() {
        std::panic::resume_unwind(panic);
    }
}

fn act(editor: &mut Editor<MockTerminal>, action: EditorAction) {
    editor.handle_action(&Action::Editor(action));
}

fn cursor_after_move(motion: Motion, count: usize) -> usize {
    let mut editor = create_editor();
    load_text(&mut editor, TEXT);
    editor.pending_count = count;
    act(&mut editor, EditorAction::Move(motion));
    editor.active_document().buffer.cursor()
}

fn text_after_visual_delete(count: usize) -> String {
    let mut editor = create_editor();
    load_text(&mut editor, TEXT);
    act(&mut editor, EditorAction::EnterVisualChar);
    editor.pending_count = count;
    act(&mut editor, EditorAction::Move(Motion::Right));
    act(&mut editor, EditorAction::Operator(OperatorType::Delete));
    editor.active_document().buffer.to_string()
}

fn text_after_dot(count: usize) -> String {
    let mut editor = create_editor();
    load_text(&mut editor, TEXT);
    act(&mut editor, EditorAction::Delete(Motion::Right));
    editor.pending_count = count;
    act(&mut editor, EditorAction::DotRepeat);
    editor.active_document().buffer.to_string()
}

#[test]
fn bug_huge_count_motion_hangs() {
    run_bounded(|| {
        for motion in [Motion::Right, Motion::Down, Motion::NextWord] {
            assert_eq!(
                cursor_after_move(motion, HUGE_COUNT),
                cursor_after_move(motion, EXHAUSTING_COUNT),
                "{motion:?}"
            );
        }
    });
}

#[test]
fn bug_huge_count_dot_and_visual_hang() {
    run_bounded(|| {
        assert_eq!(
            text_after_visual_delete(HUGE_COUNT),
            text_after_visual_delete(EXHAUSTING_COUNT)
        );
        assert_eq!(text_after_dot(HUGE_COUNT), text_after_dot(EXHAUSTING_COUNT));
    });
}
