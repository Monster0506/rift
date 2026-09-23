use super::resolve_link_path_in;
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
fn open_file_on_nonexistent_path_opens_empty_buffer_with_no_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brand_new.txt");
    let path_str = path.to_string_lossy().into_owned();

    let mut editor = create_editor();
    editor.open_file(Some(path_str), false).unwrap();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert_eq!(doc.path(), Some(path.as_path()));
    assert_eq!(doc.buffer.len(), 0);
    assert!(!path.exists(), "opening must not touch disk");
    assert!(
        editor.state.error_manager.notifications().is_empty(),
        "opening a brand-new file should not raise an error notification"
    );
}

#[test]
fn write_after_opening_nonexistent_path_creates_the_file_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brand_new.txt");
    let path_str = path.to_string_lossy().into_owned();

    let mut editor = create_editor();
    editor.open_file(Some(path_str), false).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('h')));
    editor.do_save();
    drain_jobs(&mut editor);

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "h");
}

#[test]
fn open_file_rejects_path_whose_parent_directory_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no_such_subdir").join("file.txt");
    let path_str = path.to_string_lossy().into_owned();

    let mut editor = create_editor();
    let err = editor.open_file(Some(path_str), false).unwrap_err();

    assert_eq!(err.code, crate::constants::errors::PARENT_DIR_MISSING);
    assert!(
        editor
            .document_manager
            .active_document()
            .unwrap()
            .path()
            .is_none(),
        "no document should be created for a path needing a missing directory"
    );
}

#[test]
fn resolve_link_rebases_onto_document_dir() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("sibling_note_xyz.md");
    std::fs::write(&target, "x").unwrap();

    // A relative target that does not exist in the cwd resolves against the dir.
    let got = resolve_link_path_in("sibling_note_xyz.md".to_string(), Some(dir.path()));
    assert_eq!(got, target.to_string_lossy());

    // A target that resolves against neither is returned unchanged.
    assert_eq!(
        resolve_link_path_in("missing_zzz.md".to_string(), Some(dir.path())),
        "missing_zzz.md"
    );

    // An absolute path is never rebased.
    let abs = target.to_string_lossy().into_owned();
    assert_eq!(resolve_link_path_in(abs.clone(), Some(dir.path())), abs);

    // With no document directory, the target is left as-is.
    assert_eq!(
        resolve_link_path_in("sibling_note_xyz.md".to_string(), None),
        "sibling_note_xyz.md"
    );
}
