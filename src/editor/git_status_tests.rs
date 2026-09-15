//! End-to-end tests of the git status/commit workflow against a real,
//! disposable temp repository â€” the async job round-trip (`GitStatusJob`,
//! `GitDiffJob`), stage/unstage/discard via the cursor-action fast paths,
//! and the commit/fixup buffers, all exercised through the real `Editor`.

use super::Editor;
#[allow(unused_imports)]
use crate::buffer::api::BufferView;
use crate::test_utils::MockTerminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn create_editor() -> Editor<MockTerminal> {
    let term = MockTerminal::new(24, 80);
    Editor::new(term).unwrap()
}

/// Drains pending job messages, blocking until every spawned job thread has
/// actually finished (not just "no message arrived in the last 50ms" â€” under
/// heavy parallel test load, spawning a `git` subprocess can take longer
/// than that, so a fixed short window is unreliable here).
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
    std::fs::write(dir.join("tracked.txt"), "line one\nline two\n").unwrap();
    crate::git::run_checked(dir, &["add", "tracked.txt"]).unwrap();
    crate::git::run_checked(dir, &["commit", "-m", "init", "--quiet"]).unwrap();
}

/// Open `path` as the active document and drain its (possibly async) load job.
fn open_and_load(editor: &mut Editor<MockTerminal>, path: &std::path::Path) {
    editor
        .open_file(Some(path.display().to_string()), false)
        .unwrap();
    drain_jobs(editor);
}

#[test]
fn open_git_status_populates_sections_from_a_real_repo() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line one\nline TWO\n").unwrap();
    std::fs::write(dir.path().join("new.txt"), "hello\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));

    editor.open_git_status();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_git_status());
    let text = doc.buffer.to_string();
    assert!(text.contains("Unstaged changes"), "{text}");
    assert!(text.contains("tracked.txt"), "{text}");
    assert!(text.contains("Untracked files"), "{text}");
    assert!(text.contains("new.txt"), "{text}");
}

#[test]
fn cursor_stage_action_runs_git_add_and_refreshes_the_buffer() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("new.txt"), "hello\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    // Move the cursor onto the "new.txt" untracked entry line and stage it.
    let doc_id = editor.active_document_id();
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let target_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_status_entry_at_line(l)
                    .map(|(p, ..)| p == "new.txt")
                    .unwrap_or(false)
            })
            .expect("must find new.txt line");
        let start = doc.buffer.line_index.get_start(target_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_cursor_action("stage");
    drain_jobs(&mut editor);

    let status = crate::git::run_checked(dir.path(), &["status", "--porcelain=v2"]).unwrap();
    assert!(
        status.contains("1 A. N...") && status.contains("new.txt"),
        "expected new.txt staged as added: {status}"
    );
    assert!(status.contains("new.txt"), "{status}");
}

#[test]
fn git_status_buffer_is_read_only_and_wq_reports_cannot_be_saved() {
    // Regression: `GitStatus` used to be editable via cut/paste-between-
    // sections + `:w` reconciliation. That model let plain `dd`/insert-mode
    // keys silently mutate displayed text with zero git effect while
    // desyncing the line-anchored annotations `s`/`u`/`X`/`=` rely on.
    // Changes now only happen through those dedicated actions.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_read_only, "GitStatus must be read-only");

    // `dd` must not remove the branch header line from the buffer.
    let before = doc.buffer.to_string();
    assert!(!editor.execute_buffer_command(crate::command::Command::DeleteLine(1)));
    let after = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert_eq!(
        before, after,
        "dd must not mutate a read-only status buffer"
    );

    editor.do_save();
    assert!(
        editor
            .state
            .error_manager
            .notifications()
            .iter_active()
            .any(|n| n.message.contains("cannot be saved")),
        "wq on the status buffer must report it cannot be saved, not silently reconcile edits"
    );
}

#[test]
fn wq_on_a_git_commit_message_buffer_commits_and_quits_instead_of_erroring() {
    // Regression: `do_save_and_quit` (`:wq`/`EditorAction::SaveAndQuit`) used
    // to always take the `File`-only async-save path regardless of
    // `BufferKind`, so `:wq` on any non-`File` special buffer (this commit
    // message buffer, but also pre-existing Directory/Clipboard buffers)
    // failed with "No file name" instead of dispatching through `do_save()`.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line one\nline TWO\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "tracked.txt"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_commit_new();
    let commit_doc_id = editor.active_document_id();
    {
        let doc = editor
            .document_manager
            .get_document_mut(commit_doc_id)
            .unwrap();
        doc.replace_buffer_content("wq commits this buffer");
    }

    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::SaveAndQuit,
    ));

    let log = crate::git::run_checked(dir.path(), &["log", "-1", "--format=%s"]).unwrap();
    assert_eq!(log.trim(), "wq commits this buffer");
    assert!(
        editor
            .document_manager
            .get_document(commit_doc_id)
            .is_none(),
        "commit buffer should close after :wq"
    );
    assert!(editor.should_quit, ":wq must still quit the editor");
}

#[test]
fn commit_new_writes_a_real_commit_and_closes_the_buffer() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line one\nline TWO\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "tracked.txt"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_commit_new();

    let commit_doc_id = editor.active_document_id();
    {
        let doc = editor
            .document_manager
            .get_document_mut(commit_doc_id)
            .unwrap();
        doc.replace_buffer_content("Update tracked.txt line two");
    }
    editor.apply_git_commit_message();

    let log = crate::git::run_checked(dir.path(), &["log", "-1", "--format=%s"]).unwrap();
    assert_eq!(log.trim(), "Update tracked.txt line two");
    assert!(
        editor
            .document_manager
            .get_document(commit_doc_id)
            .is_none(),
        "commit message buffer should close after a successful commit"
    );
}

#[test]
fn commit_amend_prefills_head_message_and_amends_in_place() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_commit_amend();

    let doc = editor.document_manager.active_document().unwrap();
    assert_eq!(doc.buffer.to_string().trim(), "init");

    let commit_doc_id = editor.active_document_id();
    {
        let doc = editor
            .document_manager
            .get_document_mut(commit_doc_id)
            .unwrap();
        doc.replace_buffer_content("init (amended)");
    }
    editor.apply_git_commit_message();

    let log = crate::git::run_checked(dir.path(), &["log", "--format=%s"]).unwrap();
    let subjects: Vec<&str> = log.lines().collect();
    assert_eq!(
        subjects,
        vec!["init (amended)"],
        "amend must not add a second commit"
    );
}

#[test]
fn commit_fixup_creates_a_fixup_commit_from_staged_changes() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line one\nline TWO\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "tracked.txt"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_commit_fixup();

    let log = crate::git::run_checked(dir.path(), &["log", "--format=%s"]).unwrap();
    let subjects: Vec<&str> = log.lines().collect();
    assert_eq!(
        subjects.len(),
        2,
        "expected init + fixup commit: {subjects:?}"
    );
    assert!(subjects[0].starts_with("fixup! init"), "{subjects:?}");
}

#[test]
fn discard_untracked_entry_deletes_the_file_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    let new_path = dir.path().join("scratch.txt");
    std::fs::write(&new_path, "temp\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    let doc_id = editor.active_document_id();
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let target_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_status_entry_at_line(l)
                    .map(|(p, ..)| p == "scratch.txt")
                    .unwrap_or(false)
            })
            .expect("scratch.txt entry line");
        let start = doc.buffer.line_index.get_start(target_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_cursor_action("discard");
    drain_jobs(&mut editor);

    assert!(
        !new_path.exists(),
        "discarding an untracked file must delete it"
    );
}

#[test]
fn expand_and_stage_hunk_leaves_other_hunks_unstaged() {
    let dir = tempfile::tempdir().unwrap();
    crate::git::run_checked(dir.path(), &["init", "--quiet"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.email", "t@example.com"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.name", "T"]).unwrap();
    std::fs::write(
        dir.path().join("multi.txt"),
        "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n",
    )
    .unwrap();
    crate::git::run_checked(dir.path(), &["add", "multi.txt"]).unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-m", "init", "--quiet"]).unwrap();
    std::fs::write(
        dir.path().join("multi.txt"),
        "A\nb\nc\nd\ne\nf\ng\nh\ni\nJ\n",
    )
    .unwrap();

    let diff_out = crate::git::run_checked(dir.path(), &["diff", "--no-ext-diff", "-U1"]).unwrap();
    let files = crate::git::diff::parse_unified_diff(&diff_out);
    assert_eq!(files[0].hunks.len(), 2, "expected two separate hunks");
    let first_hunk = files[0].hunks[0].clone();

    // Stage exactly the first hunk via the pure apply.rs primitive (already
    // covered end-to-end in git::apply::tests; here we confirm the second
    // hunk survives untouched in the worktree).
    crate::git::apply::stage_hunk(dir.path(), "multi.txt", &first_hunk, false).unwrap();

    let staged =
        crate::git::run_checked(dir.path(), &["diff", "--no-ext-diff", "--cached"]).unwrap();
    assert!(staged.contains("-a"), "{staged}");
    assert!(staged.contains("+A"), "{staged}");
    assert!(
        !staged.contains("-j"),
        "second hunk must stay unstaged: {staged}"
    );

    let worktree = crate::git::run_checked(dir.path(), &["diff", "--no-ext-diff"]).unwrap();
    assert!(worktree.contains("-j"), "{worktree}");
    assert!(worktree.contains("+J"), "{worktree}");
    assert!(
        !worktree.contains("-a"),
        "first hunk must be gone from the worktree diff: {worktree}"
    );
}

#[test]
fn cursor_stage_action_on_a_single_diff_line_stages_only_that_line() {
    // Two changes close enough together to land in one hunk; staging via
    // the cursor on just ONE of the two `+` lines must leave the other
    // change unstaged, both in the index and in the worktree.
    let dir = tempfile::tempdir().unwrap();
    crate::git::run_checked(dir.path(), &["init", "--quiet"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.email", "t@example.com"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.name", "T"]).unwrap();
    std::fs::write(dir.path().join("multi.txt"), "a\nb\nc\nd\ne\nf\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "multi.txt"]).unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-m", "init", "--quiet"]).unwrap();
    std::fs::write(dir.path().join("multi.txt"), "A\nb\nc\nD\ne\nf\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("multi.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);
    {
        let doc = editor
            .document_manager
            .get_document_mut(editor.active_document_id())
            .unwrap();
        let entry_line = (0..doc.buffer.get_total_lines())
            .find(|&l| doc.annotations.git_status_entry_at_line(l).is_some())
            .expect("must find the multi.txt entry line");
        let start = doc.buffer.line_index.get_start(entry_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_toggle_expand();
    drain_jobs(&mut editor);

    // Land the cursor on the "+A" line specifically (hunk_line_index 1: the
    // pair is [-a, +A, ctx b, ctx c, -d, +D, ctx e, ctx f]).
    let doc_id = editor.active_document_id();
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let target_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_hunk_line_at_line(l)
                    .map(|(_, _, _, line_index)| line_index == 1)
                    .unwrap_or(false)
            })
            .expect("must find the '+A' hunk line");
        let start = doc.buffer.line_index.get_start(target_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_cursor_action("stage");
    drain_jobs(&mut editor);

    let staged =
        crate::git::run_checked(dir.path(), &["diff", "--no-ext-diff", "--cached"]).unwrap();
    assert!(
        staged.contains("-a") && staged.contains("+A"),
        "expected 'a'->'A' staged: {staged}"
    );
    assert!(
        !staged.contains("-d") && !staged.contains("+D"),
        "'d'->'D' must stay unstaged: {staged}"
    );

    let worktree = crate::git::run_checked(dir.path(), &["diff", "--no-ext-diff"]).unwrap();
    assert!(
        worktree.contains("-d") && worktree.contains("+D"),
        "expected 'd'->'D' still in the worktree diff: {worktree}"
    );
    assert!(
        !worktree.contains("-a") && !worktree.contains("+A"),
        "'a'->'A' must be gone from the worktree diff: {worktree}"
    );
}

#[test]
fn expanding_an_untracked_files_entry_shows_its_hunk_diff() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("new.txt"), "one\ntwo\nthree\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    let doc_id = editor.active_document_id();
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let entry_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_status_entry_at_line(l)
                    .map(|(path, section, _)| path == "new.txt" && section == "untracked")
                    .unwrap_or(false)
            })
            .expect("must find the untracked new.txt entry line");
        let start = doc.buffer.line_index.get_start(entry_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_toggle_expand();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let text = doc.buffer.to_string();
    assert!(
        text.contains("+one"),
        "expected new.txt's content as additions: {text}"
    );
    assert!(text.contains("+two"), "{text}");
    assert!(text.contains("+three"), "{text}");
}

#[test]
fn cursor_stage_action_on_an_untracked_files_hunk_line_stages_only_that_line() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("new.txt"), "one\ntwo\nthree\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    let doc_id = editor.active_document_id();
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let entry_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_status_entry_at_line(l)
                    .map(|(path, section, _)| path == "new.txt" && section == "untracked")
                    .unwrap_or(false)
            })
            .expect("must find the untracked new.txt entry line");
        let start = doc.buffer.line_index.get_start(entry_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_toggle_expand();
    drain_jobs(&mut editor);

    // Land on the "+two" line specifically (hunk_line_index 1 of
    // [+one, +two, +three]) and stage just that one line.
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let target_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_hunk_line_at_line(l)
                    .map(|(_, _, _, line_index)| line_index == 1)
                    .unwrap_or(false)
            })
            .expect("must find the '+two' hunk line");
        let start = doc.buffer.line_index.get_start(target_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_cursor_action("stage");
    drain_jobs(&mut editor);

    // git's own status parser reclassifies a partially-staged untracked file
    // as `AM` (added in index, modified in worktree) once any part of it is
    // staged â€” confirm the index has exactly the staged line, worktree has
    // the full original file untouched.
    let status = crate::git::run_checked(dir.path(), &["status", "--porcelain=v2"]).unwrap();
    assert!(
        status.contains("AM") && status.contains("new.txt"),
        "{status}"
    );

    let staged = crate::git::run_checked(
        dir.path(),
        &["diff", "--no-ext-diff", "--cached", "--", "new.txt"],
    )
    .unwrap();
    assert!(staged.contains("new file mode"), "{staged}");
    assert!(staged.contains("+two"), "{staged}");
    assert!(
        !staged.contains("+one"),
        "only the selected line should be staged: {staged}"
    );
    assert!(
        !staged.contains("+three"),
        "only the selected line should be staged: {staged}"
    );

    let worktree_content = std::fs::read_to_string(dir.path().join("new.txt")).unwrap();
    assert_eq!(
        worktree_content, "one\ntwo\nthree\n",
        "worktree must stay untouched by staging"
    );
}

#[test]
fn discard_on_an_untracked_files_hunk_line_is_not_supported_and_leaves_it_untouched() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("new.txt"), "one\ntwo\nthree\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    let doc_id = editor.active_document_id();
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let entry_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_status_entry_at_line(l)
                    .map(|(path, section, _)| path == "new.txt" && section == "untracked")
                    .unwrap_or(false)
            })
            .expect("must find the untracked new.txt entry line");
        let start = doc.buffer.line_index.get_start(entry_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_toggle_expand();
    drain_jobs(&mut editor);

    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let target_line = (0..doc.buffer.get_total_lines())
            .find(|&l| {
                doc.annotations
                    .git_hunk_line_at_line(l)
                    .map(|(_, _, _, line_index)| line_index == 1)
                    .unwrap_or(false)
            })
            .expect("must find the '+two' hunk line");
        let start = doc.buffer.line_index.get_start(target_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_cursor_action("discard");
    drain_jobs(&mut editor);

    let status = crate::git::run_checked(dir.path(), &["status", "--porcelain=v2"]).unwrap();
    assert!(
        status.contains("? new.txt"),
        "must stay fully untracked and untouched: {status}"
    );
    let worktree_content = std::fs::read_to_string(dir.path().join("new.txt")).unwrap();
    assert_eq!(worktree_content, "one\ntwo\nthree\n");
}

#[test]
fn open_git_blame_populates_one_line_per_source_line() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_blame(dir.path().join("tracked.txt"), dir.path().to_path_buf());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_git_blame());
    let text = doc.buffer.to_string();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "tracked.txt has 2 lines: {text}");
    for line in &lines {
        assert!(line.contains("line "), "{line}");
        assert!(line.contains('('), "expected author/date parens: {line}");
    }
}

#[test]
fn git_blame_walk_back_re_blames_at_the_parent_commit() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line ONE\nline two\n").unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-am", "edit line one", "--quiet"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_blame(dir.path().join("tracked.txt"), dir.path().to_path_buf());
    drain_jobs(&mut editor);

    // Cursor starts at line 0 (the "line ONE" line, attributed to the 2nd commit).
    let doc_id = editor.active_document_id();
    let before_sha = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_blame_sha_at_line(0)
        .unwrap();

    // Walk back via the dispatched buffer action (mirrors pressing Enter).
    editor.handle_git_blame_buffer_action("git_blame:walk_back");
    drain_jobs(&mut editor);

    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let after_sha = doc.annotations.git_blame_sha_at_line(0).unwrap();
    assert_ne!(
        before_sha, after_sha,
        "walk-back must re-blame at an earlier commit"
    );
    let text = doc.buffer.to_string();
    assert!(
        text.contains("line one"),
        "should show the original pre-edit content: {text}"
    );
}

#[test]
fn git_blame_walk_back_on_the_root_commit_shows_a_notice_instead_of_a_raw_git_error() {
    // Regression: walking back from a line the root commit introduced tried
    // `git blame <root-sha>^`, which git rejects ("bad revision") since a
    // root commit has no parent â€” surfaced to the user as a raw subprocess
    // error instead of a sensible "nothing earlier" notice.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_blame(dir.path().join("tracked.txt"), dir.path().to_path_buf());
    drain_jobs(&mut editor);
    let doc_id = editor.active_document_id();

    let before_sha = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_blame_sha_at_line(0)
        .unwrap();

    editor.handle_git_blame_buffer_action("git_blame:walk_back");
    drain_jobs(&mut editor);

    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let after_sha = doc.annotations.git_blame_sha_at_line(0).unwrap();
    assert_eq!(
        before_sha, after_sha,
        "the root commit's blame must be unchanged"
    );
    let (at_commit_is_none,) = match &doc.kind {
        crate::document::BufferKind::GitBlame { at_commit, .. } => (at_commit.is_none(),),
        _ => panic!("expected GitBlame kind"),
    };
    assert!(
        at_commit_is_none,
        "must not record a walk-back target that was refused"
    );
}

#[test]
fn git_blame_walk_back_from_a_middle_line_re_blames_that_lines_own_history() {
    // A multi-line file where only ONE line was touched by a later commit;
    // walking back with the cursor on THAT line (not line 0) must re-blame
    // using that line's own sha, not silently do nothing.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line one\nline TWO\n").unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-am", "edit line two", "--quiet"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_blame(dir.path().join("tracked.txt"), dir.path().to_path_buf());
    drain_jobs(&mut editor);
    let doc_id = editor.active_document_id();

    let before_sha = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .annotations
        .git_blame_sha_at_line(1)
        .unwrap();

    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let start = doc.buffer.line_index.get_start(1).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.handle_git_blame_buffer_action("git_blame:walk_back");
    drain_jobs(&mut editor);

    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let after_sha = doc.annotations.git_blame_sha_at_line(1).unwrap();
    assert_ne!(
        before_sha, after_sha,
        "walk-back from line 1 must re-blame at its parent commit"
    );
    let text = doc.buffer.to_string();
    assert!(
        text.contains("line two"),
        "should show the pre-edit content of line two: {text}"
    );
}

#[test]
fn open_git_log_lists_commits_and_expand_shows_git_show_body() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_log(dir.path().to_path_buf(), None);
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_git_log());
    let text = doc.buffer.to_string();
    assert!(text.contains("init"), "{text}");

    editor.git_log_toggle_expand();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    let text = doc.buffer.to_string();
    assert!(
        text.contains("tracked.txt"),
        "expanded git show should mention the changed file: {text}"
    );
}

#[test]
fn git_command_escape_hatch_runs_and_shows_output() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("log -1".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert_eq!(
        doc.display_name(),
        "[Git: log -1]",
        "multi-line git output should open a scratch buffer, not just a notification"
    );
    let text = doc.buffer.to_string();
    assert!(
        text.contains("init"),
        "expected git log output in a scratch buffer: {text}"
    );
}

#[test]
fn bare_git_command_opens_the_status_buffer() {
    // Fugitive convention: `:Git`/`:G` with no arguments opens Git Status.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command(String::new());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        doc.is_git_status(),
        "bare :Git must open the status buffer: {:?}",
        doc.kind
    );
}

#[test]
fn bare_git_log_command_opens_the_log_buffer() {
    // Fugitive convention: `:Git log` with no other args opens the
    // structured log browser.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("log".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        doc.is_git_log(),
        "bare :Git log must open the log buffer: {:?}",
        doc.kind
    );
}

#[test]
fn bare_git_diff_command_opens_status_with_unstaged_hunks_expanded() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line ONE\nline two\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("diff".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        doc.is_git_status(),
        "bare :Git diff must open the status buffer: {:?}",
        doc.kind
    );
    assert!(
        doc.is_git_status_expanded(&PathBuf::from("tracked.txt"), false),
        "the unstaged hunk must already be expanded: {}",
        doc.buffer.to_string()
    );
}

#[test]
fn bare_git_diff_cached_command_opens_status_with_staged_hunks_expanded() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line ONE\nline two\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "tracked.txt"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("diff --cached".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        doc.is_git_status(),
        "bare :Git diff --cached must open the status buffer: {:?}",
        doc.kind
    );
    assert!(
        doc.is_git_status_expanded(&PathBuf::from("tracked.txt"), true),
        "the staged hunk must already be expanded: {}",
        doc.buffer.to_string()
    );
}

#[test]
fn bare_git_blame_command_blames_the_active_file() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("blame".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        doc.is_git_blame(),
        "bare :Git blame must open the blame buffer: {:?}",
        doc.kind
    );
}

#[test]
fn git_blame_command_with_explicit_path_blames_that_file_not_the_active_one() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("other.txt"), "line one\nline two\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "other.txt"]).unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-m", "add other", "--quiet"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("blame other.txt".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_git_blame(), "{:?}", doc.kind);
    assert_eq!(
        doc.display_name(),
        "[Git Blame] other.txt",
        "must blame the argument path, not the active file"
    );
}

#[test]
fn git_blame_command_with_a_flag_falls_through_to_raw_output() {
    // Regression: `blame <path>` is intercepted for the structured view,
    // but `blame -C` (or any other flag-shaped argument) must still reach
    // the raw escape hatch, same as any other subcommand+flags combo.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("blame -C".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        !doc.is_git_blame(),
        "a flag argument must not open the structured blame view"
    );
}

#[test]
fn git_log_command_with_a_flag_falls_through_to_raw_output() {
    // Regression: adding path-argument support for `blame` must not also
    // make `log -1` (a pre-existing, tested raw-passthrough case) get
    // hijacked as "log scoped to a file named -1".
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("log -1".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        !doc.is_git_log(),
        "a flag argument must not open the structured log view"
    );
    assert_eq!(doc.display_name(), "[Git: log -1]");
}

#[test]
fn b_key_in_status_buffer_blames_the_file_under_cursor() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "line ONE\nline two\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);
    {
        let doc = editor
            .document_manager
            .get_document_mut(editor.active_document_id())
            .unwrap();
        let entry_line = (0..doc.buffer.get_total_lines())
            .find(|&l| doc.annotations.git_status_entry_at_line(l).is_some())
            .expect("must find an entry line");
        let start = doc.buffer.line_index.get_start(entry_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }

    editor.git_status_blame_cursor();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_git_blame(), "{:?}", doc.kind);
}

#[test]
fn r_key_in_status_buffer_starts_a_rebase_onto_upstream() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());
    let branch = crate::git::run_checked(dir.path(), &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    let upstream_root = tempfile::tempdir().unwrap();
    let upstream_dir = upstream_root.path().join("upstream-clone");
    crate::git::run_checked(
        dir.path(),
        &["clone", "--quiet", ".", upstream_dir.to_str().unwrap()],
    )
    .unwrap();
    crate::git::run_checked(
        dir.path(),
        &["remote", "add", "origin", upstream_dir.to_str().unwrap()],
    )
    .unwrap();
    crate::git::run_checked(dir.path(), &["fetch", "origin", "--quiet"]).unwrap();
    crate::git::run_checked(
        dir.path(),
        &[
            "branch",
            &format!("--set-upstream-to=origin/{branch}"),
            &branch,
        ],
    )
    .unwrap();
    std::fs::write(dir.path().join("new.txt"), "x\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "new.txt"]).unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-m", "add new", "--quiet"]).unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    editor.git_status_rebase_cursor();

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_git_rebase_todo(), "{:?}", doc.kind);
}

#[test]
fn status_buffer_shows_head_summary_line_and_enter_opens_log() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_status();
    drain_jobs(&mut editor);

    let doc_id = editor.active_document_id();
    let (head_line, text) = {
        let doc = editor.document_manager.get_document(doc_id).unwrap();
        let text = doc.buffer.to_string();
        let head_line = (0..doc.buffer.get_total_lines())
            .find(|&l| doc.annotations.is_git_status_head_at_line(l));
        (head_line, text)
    };
    assert!(
        text.contains("HEAD"),
        "expected a HEAD summary line: {text}"
    );
    let head_line = head_line.expect("HEAD line must be annotated as interactive");

    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        let start = doc.buffer.line_index.get_start(head_line).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_status_select();
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(
        doc.is_git_log(),
        "Enter on the HEAD line must open the Log browser: {:?}",
        doc.kind
    );
}

#[test]
fn space_g_key_resolves_to_git_status_with_no_prefix_ambiguity() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let editor = create_editor();
    match editor
        .keymap
        .lookup(KeyContext::Normal, &[Key::Char(' '), Key::Char('g')])
    {
        MatchResult::Exact(Action::Editor(EditorAction::GitStatus)) => {}
        other => panic!("expected '<Space>g' to resolve to GitStatus, got {other:?}"),
    }
    // Bare 'gr' (no leading space) must remain LSP references, unaffected
    // by the rebase consolidation.
    match editor
        .keymap
        .lookup(KeyContext::Normal, &[Key::Char('g'), Key::Char('r')])
    {
        MatchResult::Exact(Action::Editor(EditorAction::LspReferences)) => {}
        other => panic!("expected bare 'gr' to still resolve to LspReferences, got {other:?}"),
    }
}

#[test]
fn git_blame_and_log_buffers_block_insert_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_blame(dir.path().join("tracked.txt"), dir.path().to_path_buf());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_read_only, "GitBlame must be read-only");
    let before = doc.buffer.to_string();
    editor.handle_mode_management(crate::command::Command::EnterInsertMode);
    assert_eq!(
        editor.current_mode,
        crate::mode::Mode::Normal,
        "i must not enter Insert on GitBlame"
    );
    assert!(!editor.execute_buffer_command(crate::command::Command::DeleteLine(1)));
    assert_eq!(
        editor
            .document_manager
            .active_document()
            .unwrap()
            .buffer
            .to_string(),
        before,
        "dd must not mutate GitBlame"
    );

    editor.open_git_log(dir.path().to_path_buf(), None);
    drain_jobs(&mut editor);
    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_read_only, "GitLog must be read-only");
    let before = doc.buffer.to_string();
    editor.handle_mode_management(crate::command::Command::EnterInsertMode);
    assert_eq!(
        editor.current_mode,
        crate::mode::Mode::Normal,
        "i must not enter Insert on GitLog"
    );
    assert!(!editor.execute_buffer_command(crate::command::Command::DeleteLine(1)));
    assert_eq!(
        editor
            .document_manager
            .active_document()
            .unwrap()
            .buffer
            .to_string(),
        before,
        "dd must not mutate GitLog"
    );
}

#[test]
fn bare_git_show_command_opens_log_with_head_expanded() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.run_git_command("show".to_string());
    drain_jobs(&mut editor);

    let doc = editor.document_manager.active_document().unwrap();
    let crate::document::BufferKind::GitLog {
        expanded,
        expanded_body,
        ..
    } = &doc.kind
    else {
        panic!("bare :Git show must open the log buffer: {:?}", doc.kind);
    };
    assert!(expanded.is_some(), "HEAD's commit must already be expanded");
    assert!(
        expanded_body.is_some(),
        "the expanded commit's git show body must be populated"
    );
}

#[test]
fn running_a_git_command_from_the_command_line_returns_to_normal_mode() {
    // Regression: `ExecutionResult::RunGit`'s handler returned early,
    // skipping the shared cleanup that resets `Mode::Command` back to
    // `Mode::Normal` â€” the floating command-line window stayed open after
    // `:G`/`:Git log` (or any `:Git ...`) even though the target buffer
    // (status/log/scratch) had already opened underneath it.
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));

    for cmd in ["G", "Git log", "Git log -1"] {
        editor.current_mode = crate::mode::Mode::Command;
        editor.execute_command_line(cmd.to_string());
        drain_jobs(&mut editor);
        assert_eq!(
            editor.current_mode,
            crate::mode::Mode::Normal,
            "':{cmd}' must return to Normal mode, closing the command-line window"
        );
    }
}

#[test]
fn g_question_mark_opens_a_read_only_help_buffer_for_each_git_buffer_kind() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_commit(dir.path());

    let mut editor = create_editor();
    open_and_load(&mut editor, &dir.path().join("tracked.txt"));

    editor.open_git_status();
    drain_jobs(&mut editor);
    editor.open_git_help();
    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_read_only, "help buffer must be read-only");
    let text = doc.buffer.to_string();
    assert!(text.contains("Git Status"), "expected status help: {text}");
    assert!(
        text.contains("s       stage"),
        "expected an 's' entry: {text}"
    );

    open_and_load(&mut editor, &dir.path().join("tracked.txt"));
    editor.open_git_blame(dir.path().join("tracked.txt"), dir.path().to_path_buf());
    drain_jobs(&mut editor);
    editor.open_git_help();
    let text = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(text.contains("Git Blame"), "expected blame help: {text}");
    assert!(
        text.contains("blame parent"),
        "expected the walk-back entry: {text}"
    );

    editor.open_git_log(dir.path().to_path_buf(), None);
    drain_jobs(&mut editor);
    editor.open_git_help();
    let text = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(text.contains("Git Log"), "expected log help: {text}");
}

#[test]
fn g_question_mark_key_sequence_resolves_to_git_help() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let editor = create_editor();
    for ctx in [
        KeyContext::GitStatus,
        KeyContext::GitBlame,
        KeyContext::GitLog,
        KeyContext::GitRebaseTodo,
    ] {
        let result = editor.keymap.lookup(ctx, &[Key::Char('g'), Key::Char('?')]);
        match result {
            MatchResult::Exact(Action::Editor(EditorAction::GitHelp)) => {}
            other => panic!("expected 'g?' to resolve to GitHelp in {ctx:?}, got {other:?}"),
        }
    }
}

#[test]
fn expanding_a_hunk_via_real_keys_lets_one_j_reach_the_first_hunk_line() {
    // Regression: `render_git_status` unconditionally reset the cursor to
    // buffer offset 0 on every rebuild (expand/collapse/stage/unstage/
    // discard). Pressing `=` to expand then `j` once looked like it should
    // land inside the hunk, but the reset-to-0 meant `j` actually landed
    // back on the file entry (the first interactive line after 0) â€” so the
    // next `s`/`u` staged/unstaged the whole file instead of the hunk line
    // the user thought they were on.
    use crate::replay::backend::ReplayBackend;

    fn send<W: std::io::Write>(editor: &mut Editor<ReplayBackend<W>>, seq: &str) {
        let keys = crate::key::parse_key_sequence(seq).unwrap();
        editor.term.push_keys(keys.clone());
        for _ in &keys {
            editor.tick().unwrap();
        }
        drain_jobs_replay(editor);
    }

    let dir = tempfile::tempdir().unwrap();
    crate::git::run_checked(dir.path(), &["init", "--quiet"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.email", "t@example.com"]).unwrap();
    crate::git::run_checked(dir.path(), &["config", "user.name", "T"]).unwrap();
    std::fs::write(dir.path().join("multi.txt"), "a\nb\nc\nd\ne\nf\n").unwrap();
    crate::git::run_checked(dir.path(), &["add", "multi.txt"]).unwrap();
    crate::git::run_checked(dir.path(), &["commit", "-m", "init", "--quiet"]).unwrap();
    std::fs::write(dir.path().join("multi.txt"), "A\nb\nc\nD\ne\nf\n").unwrap();

    let backend = ReplayBackend::new(Vec::new(), 40, 120);
    let mut editor = Editor::with_file(
        backend,
        Some(dir.path().join("multi.txt").display().to_string()),
    )
    .unwrap();
    drain_jobs_replay(&mut editor);

    send(&mut editor, "<Space>g");
    send(&mut editor, "j"); // skip the interactive HEAD summary line
    send(&mut editor, "j"); // land on the multi.txt entry
    let entry_line = {
        let doc = editor.active_document();
        doc.buffer.line_index.get_line_at(doc.buffer.cursor())
    };

    send(&mut editor, "=");
    let line_after_expand = {
        let doc = editor.active_document();
        doc.buffer.line_index.get_line_at(doc.buffer.cursor())
    };
    assert_eq!(
        line_after_expand, entry_line,
        "expand must leave the cursor on the entry it just expanded, not reset to line 0"
    );

    send(&mut editor, "j");
    let doc = editor.active_document();
    let line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
    assert!(
        doc.annotations.git_hunk_line_at_line(line).is_some(),
        "a single 'j' after expand must land directly on the first hunk line: line={line}, buffer=\n{}",
        doc.buffer.to_string()
    );

    send(&mut editor, "s");
    let staged =
        crate::git::run_checked(dir.path(), &["diff", "--no-ext-diff", "--cached"]).unwrap();
    let worktree = crate::git::run_checked(dir.path(), &["diff", "--no-ext-diff"]).unwrap();
    assert!(
        staged.contains("-a") && staged.contains("+A"),
        "expected 'a'->'A' staged: {staged}"
    );
    assert!(
        !staged.contains("-d") && !staged.contains("+D"),
        "'d'->'D' must stay unstaged: {staged}"
    );
    assert!(
        worktree.contains("-d") && worktree.contains("+D"),
        "expected 'd'->'D' still unstaged: {worktree}"
    );
}

fn drain_jobs_replay<W: std::io::Write>(
    editor: &mut Editor<crate::replay::backend::ReplayBackend<W>>,
) {
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
