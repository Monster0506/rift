use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

#[test]
fn test_explorer_toggle_hidden_flips_show_hidden() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();

    let tmp = std::env::temp_dir();
    editor.open_explorer(tmp.clone());

    let layout = editor
        .panel_layout
        .as_ref()
        .expect("panel layout should exist after open_explorer");
    let dir_doc_id = layout.dir_doc_id;
    {
        let doc = editor.document_manager.get_document(dir_doc_id).unwrap();
        assert_eq!(doc.directory_show_hidden(), Some(false));
    }

    editor.handle_action(&Action::Editor(EditorAction::ExplorerToggleHidden));

    {
        let doc = editor.document_manager.get_document(dir_doc_id).unwrap();
        assert_eq!(doc.directory_show_hidden(), Some(true));
    }

    editor.handle_action(&Action::Editor(EditorAction::ExplorerToggleHidden));

    {
        let doc = editor.document_manager.get_document(dir_doc_id).unwrap();
        assert_eq!(doc.directory_show_hidden(), Some(false));
    }
}

#[cfg(any(unix, windows))]
fn symlink_dir_for_test(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
}

#[cfg(any(unix, windows))]
fn symlink_file_for_test(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
}

#[cfg(any(unix, windows))]
#[test]
fn test_explorer_split_select_does_not_follow_swapped_symlink() {
    use crate::document::DirEntry;

    let base = std::env::temp_dir().join(format!(
        "rift_explorer_toctou_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dir_b = base.join("dir_b");
    let target_file = base.join("target_file.txt");
    let link = base.join("link");

    std::fs::create_dir_all(&dir_b).unwrap();
    std::fs::write(&target_file, "a").unwrap();
    if symlink_file_for_test(&target_file, &link).is_err() {
        let _ = std::fs::remove_dir_all(&base);
        return;
    }

    let mut editor = create_editor();
    editor.open_explorer(base.clone());
    let layout = editor.panel_layout.clone().expect("panel layout");

    let entries = vec![DirEntry {
        path: link.clone(),
        is_dir: false,
        id: 0,
    }];
    {
        let doc = editor
            .document_manager
            .get_document_mut(layout.dir_doc_id)
            .unwrap();
        doc.populate_directory_buffer(entries);
        doc.annotations
            .clear_by_kind_prefix(crate::annotations::well_known::FS_ENTRY);
        doc.annotations.create_directory_entry(1, 1);
    }

    std::fs::remove_file(&link).unwrap();
    if symlink_dir_for_test(&dir_b, &link).is_err() {
        let _ = std::fs::remove_dir_all(&base);
        return;
    }

    {
        let doc = editor
            .document_manager
            .get_document_mut(layout.dir_doc_id)
            .unwrap();
        let line_start = doc.buffer.line_index.get_start(1).unwrap();
        doc.buffer.set_cursor(line_start).unwrap();
    }
    editor.handle_explorer_split_select();

    let descended = match editor.document_manager.get_document(layout.dir_doc_id) {
        Some(doc) => doc.directory_path().is_some_and(|path| *path != base),
        None => false,
    };

    assert!(
        !descended,
        "explorer followed a symlink that changed from file to directory after listing"
    );

    let _ = std::fs::remove_dir_all(&base);
}

fn move_explorer_cursor_to_line(
    editor: &mut Editor<MockTerminal>,
    dir_doc_id: DocumentId,
    line: usize,
) {
    let doc = editor
        .document_manager
        .get_document_mut(dir_doc_id)
        .unwrap();
    let pos = doc.buffer.line_index.get_start(line).unwrap_or(0);
    let _ = doc.buffer.set_cursor(pos);
}

#[test]
fn test_explorer_preview_debounces_rapid_cursor_moves() {
    let dir = std::env::temp_dir().join(format!("rift_preview_debounce_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    for i in 0..6 {
        std::fs::write(dir.join(format!("file{i}.txt")), "hello").unwrap();
    }

    let mut editor = create_editor();
    editor.open_explorer(dir.clone());
    drain_jobs(&mut editor);

    let dir_doc_id = editor.panel_layout.as_ref().unwrap().dir_doc_id;

    let mut spawned_job_ids = Vec::new();
    for line in 1..=6 {
        move_explorer_cursor_to_line(&mut editor, dir_doc_id, line);
        let before = editor.job_manager.total_spawned();
        editor.update_explorer_preview();
        let after = editor.job_manager.total_spawned();
        if after > before {
            spawned_job_ids.push(after);
        }
    }

    assert_eq!(
        spawned_job_ids.len(),
        6,
        "expected one job spawned per cursor move (each targets a distinct entry)"
    );

    let cancelled_count = spawned_job_ids[..spawned_job_ids.len() - 1]
        .iter()
        .filter(|id| {
            editor.job_manager.job_state(**id) == Some(crate::job_manager::JobState::Cancelled)
        })
        .count();
    assert_eq!(
        cancelled_count,
        spawned_job_ids.len() - 1,
        "every superseded preview job should be cancelled instead of left to run unbounded"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_explorer_preview_discards_stale_result() {
    let dir = std::env::temp_dir().join(format!("rift_preview_stale_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("aaa_old.txt"), "old content").unwrap();
    std::fs::write(dir.join("zzz_new.txt"), "new content").unwrap();

    let mut editor = create_editor();
    editor.open_explorer(dir.clone());
    drain_jobs(&mut editor);

    let layout = editor.panel_layout.as_ref().unwrap().clone();
    let dir_doc_id = layout.dir_doc_id;
    let preview_doc_id = layout.preview_doc_id;

    move_explorer_cursor_to_line(&mut editor, dir_doc_id, 1);
    let stale_path = dir.join("aaa_old.txt");

    move_explorer_cursor_to_line(&mut editor, dir_doc_id, 2);
    let current_path = dir.join("zzz_new.txt");

    let fresh_result = Box::new(
        crate::job_manager::jobs::explorer_preview::ExplorerPreviewResult {
            right_doc_id: preview_doc_id,
            path: current_path,
            dir_entries: None,
            file_text: Some("new content".to_string()),
            undo_file: None,
        },
    );
    editor
        .handle_job_message(crate::job_manager::JobMessage::Custom(10000, fresh_result))
        .unwrap();

    let stale_result = Box::new(
        crate::job_manager::jobs::explorer_preview::ExplorerPreviewResult {
            right_doc_id: preview_doc_id,
            path: stale_path,
            dir_entries: None,
            file_text: Some("old content".to_string()),
            undo_file: None,
        },
    );
    editor
        .handle_job_message(crate::job_manager::JobMessage::Custom(9999, stale_result))
        .unwrap();

    let preview_doc = editor
        .document_manager
        .get_document(preview_doc_id)
        .unwrap();
    let text = String::from_utf8_lossy(&preview_doc.buffer.to_logical_bytes()).to_string();
    assert!(
        text.contains("new content"),
        "preview should show the entry currently under the cursor, got: {text:?}"
    );
    assert!(
        !text.contains("old content"),
        "stale preview result must not clobber the current preview, got: {text:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_explorer_preview_updates_on_real_j_keypress_navigation() {
    use crate::replay::ReplayBackend;

    let dir = std::env::temp_dir().join(format!("rift_replay_preview_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("z_file.rs"), "fn main() {}\n").unwrap();

    let backend = ReplayBackend::new(Vec::<u8>::new(), 30, 100);
    let mut editor = Editor::with_file(backend, None).unwrap();
    editor.open_explorer(dir.clone());
    for _ in 0..100 {
        editor.tick().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    editor.term.push_keys([crate::key::Key::Char('j')]);
    editor.tick().unwrap();
    for _ in 0..100 {
        editor.tick().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let preview_doc_id = editor.panel_layout.as_ref().unwrap().preview_doc_id;
    let preview_doc = editor
        .document_manager
        .get_document(preview_doc_id)
        .unwrap();
    let text = String::from_utf8_lossy(&preview_doc.buffer.to_logical_bytes()).to_string();

    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        text.contains("fn main"),
        "expected a real 'j' keypress to update the preview pane, got: {text:?}"
    );
}

#[test]
fn test_explorer_preview_populates_for_real_file_and_directory_targets() {
    let dir = std::env::temp_dir().join(format!("rift_preview_e2e_{}", std::process::id()));
    let sub = dir.join("a_subdir");
    let _ = std::fs::create_dir_all(&sub);
    std::fs::write(dir.join("b_file.rs"), "fn main() {}\n").unwrap();

    let mut editor = create_editor();
    editor.open_explorer(dir.clone());
    drain_jobs(&mut editor);

    let layout = editor.panel_layout.as_ref().unwrap().clone();
    let dir_doc_id = layout.dir_doc_id;
    let preview_doc_id = layout.preview_doc_id;

    move_explorer_cursor_to_line(&mut editor, dir_doc_id, 1);
    editor.update_explorer_preview();
    drain_jobs(&mut editor);

    let preview_doc = editor
        .document_manager
        .get_document(preview_doc_id)
        .unwrap();
    assert!(
        preview_doc.is_directory(),
        "expected a directory preview for the subdir entry"
    );

    editor.term.clear();
    move_explorer_cursor_to_line(&mut editor, dir_doc_id, 2);
    editor.update_explorer_preview();
    drain_jobs(&mut editor);

    let preview_doc = editor
        .document_manager
        .get_document(preview_doc_id)
        .unwrap();
    let text = String::from_utf8_lossy(&preview_doc.buffer.to_logical_bytes()).to_string();
    assert!(
        text.contains("fn main"),
        "expected the file preview to show its contents, got: {text:?}"
    );

    let screen = render_ascii(&mut editor);
    assert!(
        screen.contains("fn main"),
        "expected the preview pane to actually render its contents on screen, got:\n{screen}"
    );

    let written = editor.term.get_written_string();
    assert!(
        written.contains("fn") && written.contains("main"),
        "expected the incremental terminal write to contain the preview text, got:\n{written}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_explorer_preview_renders_a_persisted_undo_file_not_as_binary() {
    let dir =
        std::env::temp_dir().join(format!("rift_preview_undofile_e2e_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let source_path = dir.join("notes.txt");
    let undo_dir = dir.join("undo");

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

    let mut editor = create_editor();
    editor.open_explorer(undo_dir.clone());
    drain_jobs(&mut editor);

    let layout = editor.panel_layout.as_ref().unwrap().clone();
    let dir_doc_id = layout.dir_doc_id;
    let preview_doc_id = layout.preview_doc_id;

    move_explorer_cursor_to_line(&mut editor, dir_doc_id, 1);
    editor.update_explorer_preview();
    drain_jobs(&mut editor);

    let preview_doc = editor
        .document_manager
        .get_document(preview_doc_id)
        .unwrap();
    assert_eq!(
        preview_doc.buffer_kind_id(),
        crate::document::BufferKindId::UNDO_FILE_VIEW,
        "previewing a .undo file should render it with its custom buffer type"
    );
    let text = preview_doc.buffer.to_string();
    assert!(text.contains("insert 'hi'"), "text was: {text}");
    assert!(!text.contains("<binary file>"), "text was: {text}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_explorer_preview_first_move_from_blank_writes_to_terminal() {
    let dir = std::env::temp_dir().join(format!("rift_preview_firstmove_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("z_file.rs"), "fn main() {}\n").unwrap();

    let mut editor = create_editor();
    editor.open_explorer(dir.clone());
    drain_jobs(&mut editor);

    let layout = editor.panel_layout.as_ref().unwrap().clone();
    let dir_doc_id = layout.dir_doc_id;

    editor.term.clear();
    move_explorer_cursor_to_line(&mut editor, dir_doc_id, 1);
    editor.update_explorer_preview();
    drain_jobs(&mut editor);

    let written = editor.term.get_written_string();
    assert!(
        written.contains("fn") && written.contains("main"),
        "expected the first incremental preview render to write its content \
         to the terminal, got:\n{written}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
