use crate::action::{Action, EditorAction};
use crate::editor::Editor;
use crate::test_utils::MockTerminal;

pub(in crate::editor) fn create_editor() -> Editor<MockTerminal> {
    Editor::new(MockTerminal::new(24, 80)).unwrap()
}

pub(in crate::editor) fn drain_jobs(editor: &mut Editor<MockTerminal>) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let receiver = editor.job_manager.receiver();
        let next = if editor.job_manager.any_job_thread_alive() {
            receiver.recv_timeout(Duration::from_millis(20)).ok()
        } else {
            match receiver.try_recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            }
        };
        if let Some(message) = next {
            editor.handle_job_message(message);
        }
        assert!(Instant::now() < deadline, "editor jobs did not finish");
    }
}

#[test]
fn undo_history_survives_a_simulated_editor_restart_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("notes.txt");
    std::fs::write(&file_path, "hello").unwrap();
    let undo_dir = dir.path().join("undo");
    let path_str = file_path.to_string_lossy().into_owned();

    let mut editor = create_editor();
    editor
        .set_option(
            "undofile",
            Some("true"),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor
        .set_option(
            "undodir",
            Some(undo_dir.to_str().unwrap()),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor.open_file(Some(path_str.clone()), false).unwrap();
    drain_jobs(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.save_buffer(editor.active_document_id(), None, false);
    drain_jobs(&mut editor);

    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "Xhello");
    assert!(
        std::fs::read_dir(&undo_dir).unwrap().next().is_some(),
        "undofile must be written under undodir on save"
    );
    drop(editor);

    let mut editor2 = create_editor();
    editor2
        .set_option(
            "undofile",
            Some("true"),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor2
        .set_option(
            "undodir",
            Some(undo_dir.to_str().unwrap()),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor2.open_file(Some(path_str), false).unwrap();
    drain_jobs(&mut editor2);

    assert!(
        editor2.active_document().can_undo(),
        "reopened document should have its undo history restored"
    );
    assert!(editor2.active_document().undo());
    assert_eq!(editor2.active_document().buffer().to_string(), "hello");
}

#[test]
fn undo_file_is_not_written_when_persistent_undo_is_explicitly_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("notes.txt");
    std::fs::write(&file_path, "hello").unwrap();
    let undo_dir = dir.path().join("undo");
    let path_str = file_path.to_string_lossy().into_owned();

    let mut editor = create_editor();
    editor
        .set_option(
            "undofile",
            Some("false"),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor
        .set_option(
            "undodir",
            Some(undo_dir.to_str().unwrap()),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor.open_file(Some(path_str), false).unwrap();
    drain_jobs(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.save_buffer(editor.active_document_id(), None, false);
    drain_jobs(&mut editor);

    assert!(
        !undo_dir.exists(),
        "undofile explicitly off; no undodir should be created"
    );
}

#[test]
fn persisted_undo_matches_the_saved_snapshot_after_later_edits() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("base.txt");
    std::fs::write(&file_path, "BASE_TEXT").unwrap();
    let undo_dir = dir.path().join("undo");

    let mut editor = create_editor();
    editor
        .set_option(
            "undofile",
            Some("true"),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor
        .set_option(
            "undodir",
            Some(undo_dir.to_str().unwrap()),
            crate::command_line::settings::resolver::OptionScope::Global,
        )
        .unwrap();
    editor
        .open_file(Some(file_path.to_string_lossy().into_owned()), false)
        .unwrap();
    drain_jobs(&mut editor);
    let buf = editor.active_document_id();

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('A')));
    editor.save_buffer(buf, None, false);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('Z')));
    drain_jobs(&mut editor);
    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "ABASE_TEXT");

    editor.reload_document(buf, true).unwrap();
    drain_jobs(&mut editor);
    assert_eq!(editor.active_document().buffer().to_string(), "ABASE_TEXT");
    editor.handle_action(&Action::Editor(EditorAction::Undo));
    assert_eq!(editor.active_document().buffer().to_string(), "BASE_TEXT");
}
