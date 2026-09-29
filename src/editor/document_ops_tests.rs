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

    let got = resolve_link_path_in("sibling_note_xyz.md".to_string(), Some(dir.path()));
    assert_eq!(got, target.to_string_lossy());

    assert_eq!(
        resolve_link_path_in("missing_zzz.md".to_string(), Some(dir.path())),
        "missing_zzz.md"
    );

    let abs = target.to_string_lossy().into_owned();
    assert_eq!(resolve_link_path_in(abs.clone(), Some(dir.path())), abs);

    assert_eq!(
        resolve_link_path_in("sibling_note_xyz.md".to_string(), None),
        "sibling_note_xyz.md"
    );
}

#[test]
fn open_file_on_a_persisted_undo_file_shows_a_readonly_view() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("notes.txt");
    let undo_dir = dir.path().join("undo");

    let mut tree = crate::history::UndoTree::new();
    let mut tx = crate::history::EditTransaction::new("insert 'hi'");
    tx.record(crate::history::EditOperation::Insert {
        position: crate::history::Position::new(0, 0),
        text: "hi"
            .chars()
            .map(crate::character::Character::from)
            .collect(),
        len: 2,
    });
    tree.push(tx, None);
    let hash = crate::history::persist::sha256(b"hi");
    crate::history::persist::save(&undo_dir, &source_path, &tree, hash).unwrap();
    let undo_file_path = crate::history::persist::undo_file_path(&undo_dir, &source_path);

    let mut editor = create_editor();
    editor
        .open_file(Some(undo_file_path.to_string_lossy().into_owned()), false)
        .unwrap();

    let doc = editor.document_manager.active_document().unwrap();
    assert_eq!(
        doc.buffer_kind_id(),
        crate::document::BufferKindId::UNDO_FILE_VIEW
    );
    assert!(doc.is_read_only());
    let text = doc.buffer.to_string();
    assert!(text.contains("insert 'hi'"), "text was: {text}");
    assert!(text.contains(&source_path.display().to_string()));
}

#[test]
fn open_file_on_a_corrupt_undo_file_surfaces_an_error_instead_of_raw_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.undo");
    let mut bytes = crate::history::persist::MAGIC.to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(b"not a valid bincode payload");
    std::fs::write(&path, &bytes).unwrap();

    let mut editor = create_editor();
    let err = editor
        .open_file(Some(path.to_string_lossy().into_owned()), false)
        .unwrap_err();

    assert_eq!(err.code, crate::constants::errors::UNDOFILE_CORRUPT);
}

#[test]
fn reloading_an_open_undo_file_view_re_renders_instead_of_showing_raw_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("notes.txt");
    let undo_dir = dir.path().join("undo");

    let mut tree = crate::history::UndoTree::new();
    let mut tx = crate::history::EditTransaction::new("insert 'hi'");
    tx.record(crate::history::EditOperation::Insert {
        position: crate::history::Position::new(0, 0),
        text: "hi"
            .chars()
            .map(crate::character::Character::from)
            .collect(),
        len: 2,
    });
    tree.push(tx, None);
    let hash = crate::history::persist::sha256(b"hi");
    crate::history::persist::save(&undo_dir, &source_path, &tree, hash).unwrap();
    let undo_file_path = crate::history::persist::undo_file_path(&undo_dir, &source_path);

    let mut editor = create_editor();
    editor
        .open_file(Some(undo_file_path.to_string_lossy().into_owned()), false)
        .unwrap();
    let doc_id_before = editor.active_document_id();

    tree.push(
        crate::history::EditTransaction::new("insert ' there'"),
        None,
    );
    crate::history::persist::save(&undo_dir, &source_path, &tree, hash).unwrap();

    editor.open_file(None, false).unwrap();

    assert_eq!(
        editor.active_document_id(),
        doc_id_before,
        "reload must update the same document in place, not replace it"
    );
    let doc = editor.document_manager.active_document().unwrap();
    assert_eq!(
        doc.buffer_kind_id(),
        crate::document::BufferKindId::UNDO_FILE_VIEW
    );
    let text = doc.buffer.to_string();
    assert!(text.contains("insert 'hi'"), "text was: {text}");
    assert!(
        text.contains("insert ' there'"),
        "reload should reflect the file's new content; text was: {text}"
    );
    assert!(
        text.contains("SHA-256"),
        "reload must keep rendering the informational header, not raw bytes; text was: {text:?}"
    );
}

#[test]
fn reloading_a_kind_with_no_reload_dispatch_returns_a_clear_error() {
    let mut editor = create_editor();
    editor
        .create_scratch_buffer("scratch".to_string(), &["hello".to_string()])
        .unwrap();

    let err = editor.open_file(None, false).unwrap_err();

    assert_eq!(err.code, crate::constants::errors::RELOAD_UNSUPPORTED);
}

#[test]
fn reload_on_a_directory_buffer_uses_its_own_refresh_mechanism() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();

    let mut editor = create_editor();
    editor.open_explorer(dir.path().to_path_buf());
    drain_jobs(&mut editor);

    std::fs::write(dir.path().join("b.txt"), "").unwrap();

    editor.open_file(None, false).unwrap();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    let text = doc.buffer.to_string();
    assert!(
        text.contains("b.txt"),
        "reload should re-read the directory listing; text was: {text}"
    );
}

#[test]
fn reload_on_a_buffer_list_panel_refreshes_listing() {
    let mut editor = create_editor();
    editor
        .open_file(Some("doc1.txt".to_string()), false)
        .unwrap();

    editor.execute_command_line("buffer".to_string());
    let layout = editor
        .panel_layout_of(crate::editor::PanelKind::BufferList)
        .unwrap();
    let text_before = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap()
        .buffer
        .to_string();
    assert!(!text_before.contains("[3]"));

    let new_doc = crate::document::Document::new(99).unwrap();
    editor.document_manager.add_document_inactive(new_doc);

    editor.open_file(None, false).unwrap();

    let text_after = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap()
        .buffer
        .to_string();
    assert!(
        text_after.contains("[3]"),
        "buffer list should now include the 3rd document; text was: {text_after}"
    );
}
