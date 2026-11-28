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
fn cut_moves_a_file_into_a_child_directory_only_on_write() {
    let dir = fixture();
    let source = dir.path().join("a.txt");
    let destination = dir.path().join("sub/a.txt");
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("dd"));
    goto_entry(&mut editor, "sub/");
    press(&mut editor, &keys("<CR>"));
    settle(&mut editor);
    press(&mut editor, &keys("pp"));

    assert_eq!(std::fs::read(&source).unwrap(), b"AAAA");
    assert!(!destination.exists());
    assert_eq!(
        listing_text(&editor)
            .lines()
            .filter(|line| *line == "a.txt")
            .count(),
        1
    );

    press(&mut editor, &keys("u"));
    editor.execute_command_line("w");
    assert!(source.exists());
    press(&mut editor, &keys("<C-r>"));
    editor.execute_command_line("wq");
    settle(&mut editor);

    assert!(editor.panel_layout.is_none());
    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"AAAA");
}

#[test]
fn counted_cut_moves_a_directory_and_file_through_the_parent() {
    let dir = fixture();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::rename(dir.path().join("sub"), source.join("sub")).unwrap();
    std::fs::rename(dir.path().join("a.txt"), source.join("a.txt")).unwrap();
    let mut editor = explorer(&source);
    goto_entry(&mut editor, "sub/");
    press(&mut editor, &keys("2dd-"));
    settle(&mut editor);
    press(&mut editor, &keys("P"));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert!(!source.join("sub").exists());
    assert!(!source.join("a.txt").exists());
    assert_eq!(
        std::fs::read(dir.path().join("sub/inner.txt")).unwrap(),
        b"I"
    );
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"AAAA");
}

#[test]
fn a_name_collision_keeps_a_pasted_move_pending_without_overwriting() {
    let dir = fixture();
    let source = dir.path().join("a.txt");
    let destination = dir.path().join("sub/a.txt");
    std::fs::write(&destination, b"existing").unwrap();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("dd"));
    goto_entry(&mut editor, "sub/");
    press(&mut editor, &keys("<CR>"));
    settle(&mut editor);
    press(&mut editor, &keys("p"));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert_eq!(std::fs::read(&source).unwrap(), b"AAAA");
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    assert!(notified(&editor, "destination already exists"));

    press(&mut editor, &keys("G0cwrenamed<Esc>"));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    assert_eq!(
        std::fs::read(dir.path().join("sub/renamed.txt")).unwrap(),
        b"AAAA"
    );
}

#[test]
fn yank_uses_the_same_paste_flow_but_preserves_the_source() {
    let dir = fixture();
    let source = dir.path().join("a.txt");
    let folder = dir.path().join("sub");
    let destination = folder.join("a.txt");
    std::fs::write(&destination, b"existing").unwrap();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("yy"));
    goto_entry(&mut editor, "sub/");
    press(&mut editor, &keys("<CR>"));
    settle(&mut editor);
    press(&mut editor, &keys("p"));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    press(&mut editor, &keys("G0cwcopy<Esc>"));
    assert!(!folder.join("copy.txt").exists());
    editor.execute_command_line("w");
    settle(&mut editor);

    assert_eq!(std::fs::read(&source).unwrap(), b"AAAA");
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    assert_eq!(std::fs::read(folder.join("copy.txt")).unwrap(), b"AAAA");
}

#[test]
fn yanking_a_directory_copies_its_tree_without_removing_the_source() {
    let dir = fixture();
    std::fs::create_dir(dir.path().join("target")).unwrap();
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "sub/");
    press(&mut editor, &keys("yy"));
    goto_entry(&mut editor, "target/");
    press(&mut editor, &keys("<CR>"));
    settle(&mut editor);
    press(&mut editor, &keys("P"));
    editor.execute_command_line("w");
    settle(&mut editor);

    assert_eq!(
        std::fs::read(dir.path().join("sub/inner.txt")).unwrap(),
        b"I"
    );
    assert_eq!(
        std::fs::read(dir.path().join("target/sub/inner.txt")).unwrap(),
        b"I"
    );
}

#[test]
fn visual_explorer_transfers_nonadjacent_rows_with_file_identity() {
    for (op, moved) in [("Y", false), ("D", true)] {
        let dir = fixture();
        let destination = dir.path().join("sub");
        std::fs::write(dir.path().join("c.txt"), b"CCC").unwrap();
        std::fs::write(destination.join("stay.txt"), b"stay").unwrap();
        let mut editor = explorer(dir.path());

        goto_entry(&mut editor, "a.txt");
        press(&mut editor, &keys("v<Esc>"));
        goto_entry(&mut editor, "c.txt");
        press(&mut editor, &keys(&format!("v{op}")));
        assert_eq!(pending(&editor).deletes.len(), if moved { 2 } else { 0 });
        goto_entry(&mut editor, "sub/");
        press(&mut editor, &keys("<CR>"));
        settle(&mut editor);

        goto_entry(&mut editor, "inner.txt");
        press(&mut editor, &keys("v<Esc>"));
        goto_entry(&mut editor, "stay.txt");
        press(&mut editor, &keys("vpp"));
        let staged = pending(&editor);
        assert_eq!(staged.transfers.len(), 2, "{op}: {staged:?}");
        assert!(staged.creates.is_empty() && staged.deletes.is_empty());
        assert!(dir.path().join("a.txt").exists());
        assert!(dir.path().join("c.txt").exists());
        assert!(!destination.join("a.txt").exists());
        assert!(!destination.join("c.txt").exists());

        press(&mut editor, &keys(":w<CR>"));
        settle(&mut editor);

        assert_eq!(dir.path().join("a.txt").exists(), !moved);
        assert_eq!(dir.path().join("c.txt").exists(), !moved);
        assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"BB");
        assert_eq!(std::fs::read(destination.join("a.txt")).unwrap(), b"AAAA");
        assert_eq!(std::fs::read(destination.join("c.txt")).unwrap(), b"CCC");
        assert_eq!(std::fs::read(destination.join("inner.txt")).unwrap(), b"I");
        assert_eq!(
            std::fs::read(destination.join("stay.txt")).unwrap(),
            b"stay"
        );
        if moved {
            press(&mut editor, &keys("p"));
            assert!(pending(&editor).is_empty());
        }
    }
}

#[test]
fn linewise_normal_motion_captures_explorer_entries_for_copy() {
    let dir = fixture();
    let destination = dir.path().join("sub");
    let mut editor = explorer(dir.path());
    goto_entry(&mut editor, "a.txt");
    press(&mut editor, &keys("yj"));
    goto_entry(&mut editor, "sub/");
    press(&mut editor, &keys("<CR>"));
    settle(&mut editor);
    press(&mut editor, &keys("p:w<CR>"));
    settle(&mut editor);

    for (name, text) in [("a.txt", b"AAAA".as_slice()), ("b.txt", b"BB".as_slice())] {
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), text);
        assert_eq!(std::fs::read(destination.join(name)).unwrap(), text);
    }
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

    editor.execute_command_line("wq");
    assert!(editor.panel_layout.is_some());
    assert!(dir.path().join("sub/inner.txt").exists());

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
