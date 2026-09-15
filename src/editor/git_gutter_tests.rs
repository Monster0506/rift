//! End-to-end tests of the debounced git-gutter-diff pipeline (Phase 5): edit a live buffer, drive ticks past the debounce window, and confirm the resulting `git.gutter` annotations classify add/change/delete lines correctly against a real temp repository.

use super::Editor;
use crate::test_utils::MockTerminal;
use std::time::{Duration, Instant};

fn create_editor() -> Editor<MockTerminal> {
    let term = MockTerminal::new(24, 80);
    Editor::new(term).unwrap()
}

fn drain_jobs(editor: &mut Editor<MockTerminal>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match editor
            .job_manager
            .receiver()
            .recv_timeout(Duration::from_millis(50))
        {
            Ok(msg) => {
                let _ = editor.handle_job_message(msg);
            }
            Err(_) => {
                if !editor.job_manager.any_job_thread_alive() {
                    break;
                }
            }
        }
        if Instant::now() >= deadline {
            break;
        }
    }
}

fn init_repo_with_commit(dir: &std::path::Path) {
    crate::git::run_checked(dir, &["init", "--quiet"]).unwrap();
    crate::git::run_checked(dir, &["config", "user.email", "t@example.com"]).unwrap();
    crate::git::run_checked(dir, &["config", "user.name", "T"]).unwrap();
    std::fs::write(
        dir.join("tracked.txt"),
        "line one\nline two\nline three\nline four\n",
    )
    .unwrap();
    crate::git::run_checked(dir, &["add", "tracked.txt"]).unwrap();
    crate::git::run_checked(dir, &["commit", "-m", "init", "--quiet"]).unwrap();
}

/// Drive `tick()` until `condition` is true or a 5s deadline passes, draining job messages along the way (the debounce fires on the main loop's own clock, so real sleeping between ticks is unavoidable here).
fn tick_until(
    editor: &mut Editor<MockTerminal>,
    mut condition: impl FnMut(&mut Editor<MockTerminal>) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        editor.tick().unwrap();
        if condition(editor) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn edit_then_debounce_produces_gutter_signs_for_changed_lines() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    editor
        .open_file(
            Some(dir.path().join("tracked.txt").display().to_string()),
            false,
        )
        .unwrap();
    drain_jobs(&mut editor);
    let doc_id = editor.active_document_id();

    // Unsaved edit: line 2 modified ("change"), a brand-new line 5 appended ("add").
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.replace_buffer_content("line one\nline TWO\nline three\nline four\nline five\n");
    }

    tick_until(&mut editor, |ed| {
        drain_jobs(ed);
        !ed.document_manager
            .get_document(doc_id)
            .unwrap()
            .annotations
            .git_gutter_signs()
            .is_empty()
    });

    let signs = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_gutter_signs();
    assert!(
        !signs.is_empty(),
        "expected gutter signs after edit + debounce"
    );

    use crate::git::diff::GutterSignKind;
    assert!(
        signs
            .iter()
            .any(|&(line, kind)| line == 1 && kind == GutterSignKind::Change),
        "line 1 (0-indexed) should be signed Change: {signs:?}"
    );
    assert!(
        signs.iter().any(|&(_, kind)| kind == GutterSignKind::Add),
        "the appended line should be signed Add: {signs:?}"
    );
}

#[test]
fn opening_an_already_dirty_file_produces_gutter_signs_without_any_in_editor_edit() {
    // Regression: a freshly-opened file's buffer starts at revision 0 (the bulk `from_bytes` loader skips `insert_str`), the same default a brand-new debounce tracker entry reports as "last seen"; so the very first poll saw revision 0 == 0 and never armed. A file that already differs from the git index on disk.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(
        dir.path().join("tracked.txt"),
        "line one\nline TWO\nline three\nline four\n",
    )
    .unwrap();

    let mut editor = create_editor();
    editor
        .open_file(
            Some(dir.path().join("tracked.txt").display().to_string()),
            false,
        )
        .unwrap();
    drain_jobs(&mut editor);
    let doc_id = editor.active_document_id();

    tick_until(&mut editor, |ed| {
        drain_jobs(ed);
        !ed.document_manager
            .get_document(doc_id)
            .unwrap()
            .annotations
            .git_gutter_signs()
            .is_empty()
    });

    let signs = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_gutter_signs();
    use crate::git::diff::GutterSignKind;
    assert!(
        signs
            .iter()
            .any(|&(line, kind)| line == 1 && kind == GutterSignKind::Change),
        "line 1 (0-indexed) should be signed Change on first poll, no edit needed: {signs:?}"
    );
}

#[test]
fn a_file_in_a_subdirectory_still_gets_gutter_signs() {
    // Regression: the diff job read `PathBuf::to_string_lossy()` directly into git's `:<path>` index-blob syntax. On Windows that yields a backslash path ("src\\tracked.txt"), but the index always stores forward slashes; the lookup silently missed, producing an empty baseline and thus zero signs for any file not.
    let dir = tempfile::tempdir().unwrap();
    crate::git::run_checked(dir.path(), &["init", "--quiet"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.email", "t@example.com"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.name", "T"]).unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/tracked.txt"),
        "line one\nline two\nline three\nline four\n",
    )
    .unwrap();
    crate::git::run_checked(dir.path(), &["add", "src/tracked.txt"]).unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-m", "init", "--quiet"]).unwrap();
    std::fs::write(
        dir.path().join("src/tracked.txt"),
        "line one\nline TWO\nline three\nline four\n",
    )
    .unwrap();

    let mut editor = create_editor();
    editor
        .open_file(
            Some(dir.path().join("src/tracked.txt").display().to_string()),
            false,
        )
        .unwrap();
    drain_jobs(&mut editor);
    let doc_id = editor.active_document_id();

    tick_until(&mut editor, |ed| {
        drain_jobs(ed);
        !ed.document_manager
            .get_document(doc_id)
            .unwrap()
            .annotations
            .git_gutter_signs()
            .is_empty()
    });

    let signs = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_gutter_signs();
    use crate::git::diff::GutterSignKind;
    assert!(
        signs
            .iter()
            .any(|&(line, kind)| line == 1 && kind == GutterSignKind::Change),
        "a subdirectory file must still get signs: {signs:?}"
    );
}

#[test]
fn edit_then_debounce_produces_delete_sign_for_removed_lines() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    editor
        .open_file(
            Some(dir.path().join("tracked.txt").display().to_string()),
            false,
        )
        .unwrap();
    drain_jobs(&mut editor);
    let doc_id = editor.active_document_id();

    // Remove "line two" entirely (a pure deletion, no corresponding addition).
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.replace_buffer_content("line one\nline three\nline four\n");
    }

    tick_until(&mut editor, |ed| {
        drain_jobs(ed);
        !ed.document_manager
            .get_document(doc_id)
            .unwrap()
            .annotations
            .git_gutter_signs()
            .is_empty()
    });

    let signs = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_gutter_signs();
    use crate::git::diff::GutterSignKind;
    assert!(
        signs
            .iter()
            .any(|&(_, kind)| kind == GutterSignKind::Delete),
        "expected a Delete gutter sign: {signs:?}"
    );
}

#[test]
fn unsaved_edit_matching_the_index_produces_no_signs() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    editor
        .open_file(
            Some(dir.path().join("tracked.txt").display().to_string()),
            false,
        )
        .unwrap();
    drain_jobs(&mut editor);
    let doc_id = editor.active_document_id();

    // Force a revision bump with no real content change.
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let text = doc.buffer.to_string();
        doc.replace_buffer_content(&text);
    }

    // Drive ticks past the debounce window; nothing should ever appear.
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        editor.tick().unwrap();
        drain_jobs(&mut editor);
        std::thread::sleep(Duration::from_millis(20));
    }

    let signs = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_gutter_signs();
    assert!(
        signs.is_empty(),
        "identical content should produce no signs: {signs:?}"
    );
}
