use crate::action::{Action, EditorAction};
use crate::editor::Editor;
use crate::test_utils::MockTerminal;

fn create_editor() -> Editor<MockTerminal> {
    Editor::new(MockTerminal::new(24, 80)).unwrap()
}

fn drain_jobs(editor: &mut Editor<MockTerminal>) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match editor
            .job_manager
            .receiver()
            .recv_timeout(Duration::from_millis(20))
        {
            Ok(msg) => {
                let _ = editor.handle_job_message(msg);
            }
            Err(_) => {
                if Instant::now() >= deadline {
                    break;
                }
            }
        }
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
    editor.state.settings.persistent_undo = true;
    editor.state.settings.undo_dir = Some(undo_dir.clone());
    editor.open_file(Some(path_str.clone()), false).unwrap();
    drain_jobs(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.do_save();
    drain_jobs(&mut editor);

    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "Xhello");
    assert!(
        std::fs::read_dir(&undo_dir).unwrap().next().is_some(),
        "undofile must be written under undodir on save"
    );
    drop(editor);

    let mut editor2 = create_editor();
    editor2.state.settings.persistent_undo = true;
    editor2.state.settings.undo_dir = Some(undo_dir);
    editor2.open_file(Some(path_str), false).unwrap();
    drain_jobs(&mut editor2);

    assert!(
        editor2.active_document().can_undo(),
        "reopened document should have its undo history restored"
    );
    assert!(editor2.active_document().undo());
    assert_eq!(editor2.active_document().buffer.to_string(), "hello");
}

#[test]
fn undo_file_is_not_written_when_persistent_undo_is_explicitly_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("notes.txt");
    std::fs::write(&file_path, "hello").unwrap();
    let undo_dir = dir.path().join("undo");
    let path_str = file_path.to_string_lossy().into_owned();

    let mut editor = create_editor();
    editor.state.settings.persistent_undo = false; // undofile defaults to on; turn it off
    editor.state.settings.undo_dir = Some(undo_dir.clone());
    editor.open_file(Some(path_str), false).unwrap();
    drain_jobs(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.do_save();
    drain_jobs(&mut editor);

    assert!(
        !undo_dir.exists(),
        "undofile explicitly off; no undodir should be created"
    );
}
