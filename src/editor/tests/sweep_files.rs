use super::common::*;
use crate::editor::Editor;
use crate::test_utils::MockTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn settle(editor: &mut Editor<MockTerminal>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let idle = !editor.job_manager.any_job_thread_alive();
        while let Ok(msg) = editor.job_manager.receiver().try_recv() {
            editor.handle_job_message(msg);
        }
        if idle || Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn open(editor: &mut Editor<MockTerminal>, path: &Path) {
    editor
        .open_file(Some(path.to_string_lossy().into_owned()), false)
        .unwrap();
    settle(editor);
}

fn active_path(editor: &mut Editor<MockTerminal>) -> Option<PathBuf> {
    editor.active_document().path().map(Path::to_path_buf)
}

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    (dir, a, b)
}

#[test]
fn bug_w_existing_other_path_overwrites() {
    let (_dir, a, b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    let before = active_path(&mut editor);

    editor.execute_command_line(&format!(":w {}", b.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
    assert_eq!(active_path(&mut editor), before);
    assert!(!editor.state.error_manager.notifications().is_empty());
}

#[test]
fn w_bang_existing_other_path_overwrites() {
    let (_dir, a, b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);

    editor.execute_command_line(&format!(":w! {}", b.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&b).unwrap(), "a");
    assert!(active_path(&mut editor).unwrap().ends_with("b.txt"));
}

#[test]
fn bug_wq_existing_other_path_overwrites() {
    let (_dir, a, b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);

    editor.execute_command_line(&format!(":wq {}", b.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
    assert!(!editor.should_quit);
}

#[test]
fn bug_w_failed_save_renames_buffer() {
    let (dir, a, _b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    let before = active_path(&mut editor);
    let target = dir.path().join("missing").join("c.txt");

    editor.execute_command_line(&format!(":w {}", target.display()));
    settle(&mut editor);

    assert!(!target.exists());
    assert_eq!(active_path(&mut editor), before);
}

#[test]
fn bug_w_path_with_space_rejected() {
    let (dir, a, _b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    let target = dir.path().join("sp ace.txt");

    editor.execute_command_line(&format!(":w {}", target.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "a");
}

#[test]
fn bug_e_path_with_space_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("sp ace.txt");
    std::fs::write(&target, "x").unwrap();
    let mut editor = create_editor();

    editor.execute_command_line(&format!(":e {}", target.display()));
    settle(&mut editor);

    assert!(active_path(&mut editor).unwrap().ends_with("sp ace.txt"));
    assert_eq!(editor.active_document().buffer.to_string(), "x");
}

#[test]
fn w_own_path_writes() {
    let (_dir, a, _b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    load_text(&mut editor, "z");

    editor.execute_command_line(&format!(":w {}", a.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&a).unwrap(), "za");
}

#[test]
fn bug_wq_in_split_quits_editor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    editor.execute_command_line("split");
    assert_eq!(editor.split_tree.window_count(), 2);

    type_text(&mut editor, "hello");
    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        ":wq in a split must only close the split"
    );
    assert_eq!(editor.split_tree.window_count(), 1);
    assert!(std::fs::read_to_string(&path).unwrap().contains("hello"));
}

#[test]
fn bug_wq_quits_discarding_other_dirty_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "").unwrap();
    std::fs::write(&b, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &a);
    let a_id = editor.active_document_id();
    type_text(&mut editor, "unsaved");
    open_and_load(&mut editor, &b);

    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(!editor.should_quit, ":wq must not exit with a.txt dirty");
    let a_doc = editor
        .document_manager
        .get_document(a_id)
        .expect("a.txt must stay open");
    assert!(a_doc.is_dirty());
}

#[test]
fn bug_explorer_wq_quits_editor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    editor.open_explorer(dir.path().to_path_buf());
    drain_jobs(&mut editor);
    assert!(editor.panel_layout.is_some());

    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        ":wq in the explorer must only close it"
    );
    assert!(editor.panel_layout.is_none());
}
