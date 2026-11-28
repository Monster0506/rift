use super::common::*;
use crate::editor::Editor;
use std::path::Path;

fn settle(editor: &mut Editor<KeyQueue>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let alive = editor.job_manager.any_job_thread_alive();
        while let Ok(msg) = editor.job_manager.receiver().try_recv() {
            editor.handle_job_message(msg);
        }
        if !alive {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "jobs did not finish");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"AAAA").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"BB").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub").join("inner.txt"), b"I").unwrap();
    dir
}

fn explorer(dir: &Path) -> Editor<KeyQueue> {
    let mut editor = editor_with_text("");
    editor.open_explorer(dir.to_path_buf());
    settle(&mut editor);
    editor
}

fn listing(editor: &Editor<KeyQueue>) -> &crate::document::Document {
    let id = editor.panel_layout.as_ref().unwrap().dir_doc_id;
    editor.document_manager.get_document(id).unwrap()
}

fn listing_text(editor: &Editor<KeyQueue>) -> String {
    listing(editor).buffer().to_string()
}

fn pending(editor: &Editor<KeyQueue>) -> crate::document::DirectoryDiff {
    listing(editor).parse_directory_diff()
}

fn goto_entry(editor: &mut Editor<KeyQueue>, name: &str) {
    let doc = editor.active_document();
    let line = doc
        .buffer()
        .to_string()
        .lines()
        .position(|l| l == name)
        .unwrap();
    let pos = doc.buffer().line_index.get_start(line).unwrap();
    doc.set_cursor(pos, crate::document::ops::CursorCause::User)
        .unwrap();
}

fn notified(editor: &Editor<KeyQueue>, needle: &str) -> bool {
    editor
        .state
        .notifications
        .iter_active()
        .any(|n| n.message.contains(needle))
}

#[test]
fn moving_an_entry_line_keeps_the_file_contents() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("ddp"));

    assert!(pending(&editor).is_empty(), "{:?}", pending(&editor));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"AAAA");
}

#[test]
fn leaving_a_directory_with_unsaved_changes_is_refused() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("cwz<Esc>"));
    press(&mut editor, &keys("-"));
    settle(&mut editor);

    assert_eq!(
        listing(&editor).directory_path().map(|p| p.as_path()),
        Some(dir.path())
    );
    assert!(listing_text(&editor).contains("z.txt"));
    assert!(notified(&editor, "Unsaved changes"));
}

#[test]
fn closing_the_explorer_with_unsaved_changes_needs_a_forced_quit() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("cwz<Esc>"));

    press(&mut editor, &keys("<Esc>"));
    assert!(editor.panel_layout.is_some());
    editor.execute_command_line("q");
    assert!(editor.panel_layout.is_some());
    assert!(notified(&editor, "Explorer has unsaved changes"));

    editor.execute_command_line("q!");
    assert!(editor.panel_layout.is_none());
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn edits_made_before_a_listing_lands_survive_it() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    let dir_doc = editor.panel_layout.as_ref().unwrap().dir_doc_id;
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("cwz<Esc>"));
    goto_entry(&mut editor, "b.txt");
    press(&mut editor, &keys("dd"));
    press(&mut editor, &keys("Gonew.txt<Esc>"));
    let before = pending(&editor);
    editor
        .job_manager
        .spawn(crate::job_manager::jobs::explorer::DirectoryListJob::new(
            dir_doc,
            dir.path().to_path_buf(),
            false,
        ));
    settle(&mut editor);

    assert_eq!(pending(&editor), before);
    assert_eq!(before.renames.len(), 1);
    assert_eq!(before.deletes.len(), 1);
    assert_eq!(before.creates, vec!["new.txt".to_string()]);
}

#[test]
fn failed_operations_stay_pending_after_a_write() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("cwb<Esc>"));
    press(&mut editor, &keys("Gonew.txt<Esc>"));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert!(dir.path().join("new.txt").exists());
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"AAAA");
    let left = pending(&editor);
    assert_eq!(
        left.renames,
        vec![(dir.path().join("a.txt"), "b.txt".to_string())]
    );
    assert!(
        left.deletes.is_empty() && left.creates.is_empty(),
        "{left:?}"
    );
}

#[test]
fn a_successful_write_leaves_nothing_pending() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("cwmoved<Esc>"));
    press(&mut editor, &keys("Goa.txt<Esc>"));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert!(pending(&editor).is_empty(), "{:?}", pending(&editor));
    assert_eq!(
        std::fs::read(dir.path().join("moved.txt")).unwrap(),
        b"AAAA"
    );
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"");
}

#[test]
fn deleting_a_non_empty_directory_needs_a_forced_write() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "sub/");
    press(&mut editor, &keys("dd"));

    editor.execute_command_line("w");
    settle(&mut editor);
    assert!(dir.path().join("sub").join("inner.txt").exists());
    assert_eq!(pending(&editor).deletes, vec![dir.path().join("sub")]);
    assert!(notified(&editor, ":w!"));

    editor.execute_command_line("w!");
    settle(&mut editor);
    assert!(!dir.path().join("sub").exists());
    assert!(pending(&editor).is_empty());
}

#[test]
fn the_preview_follows_the_entry_not_its_edited_name() {
    let dir = fixture();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("cwzz<Esc>"));

    assert_eq!(
        editor.current_explorer_target_path(),
        Some(dir.path().join("a.txt"))
    );
}

#[test]
fn the_preview_pane_cannot_change_what_it_shows() {
    let dir = fixture();
    let long: String = (0..300).map(|i| format!("line {i}\n")).collect();
    std::fs::write(dir.path().join("b.txt"), &long).unwrap();
    let mut editor = explorer(dir.path());
    let layout = editor.panel_layout.clone().unwrap();

    goto_entry(&mut editor, "b.txt");
    editor.update_explorer_preview();
    settle(&mut editor);
    editor.focus_window(layout.preview_win_id);
    press(&mut editor, &keys("ggdd"));
    editor.execute_command_line("w");
    settle(&mut editor);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        long
    );

    editor.focus_window(layout.dir_win_id);
    goto_entry(&mut editor, "sub/");
    editor.update_explorer_preview();
    settle(&mut editor);
    editor.focus_window(layout.preview_win_id);
    press(&mut editor, &keys("ggjdd"));
    editor.execute_command_line("w");
    settle(&mut editor);
    assert!(dir.path().join("sub").join("inner.txt").exists());
}
