//! End-to-end tests of the plumbing rebase engine (Phase 6) against real, disposable temp repositories: pick/reorder/drop/fixup/squash/reword/edit, conflict pause+resume, and abort.

use super::Editor;
use crate::test_utils::MockTerminal;
use std::path::Path;
use std::time::{Duration, Instant};

fn create_editor() -> Editor<MockTerminal> {
    let term = MockTerminal::new(24, 80);
    Editor::new(term).unwrap()
}

/// Drains pending job messages, blocking until every spawned job thread has
/// actually finished (not just "no message arrived in the last 50ms").
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

fn git(dir: &Path, args: &[&str]) -> String {
    crate::git::run_checked(dir, args).unwrap()
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-m", message, "--quiet"]);
}

/// A repo with three commits on `main` past `origin/main` (its own upstream, a local bare-ish setup good enough for `@{upstream}` resolution) touching three different files, so cherry-picks/reorders/drops never conflict with each other unless a test explicitly sets that up.
fn init_repo_with_three_commits_ahead_of_upstream(dir: &Path) -> (String, String, String) {
    git(dir, &["init", "--quiet", "-b", "main"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "T"]);
    commit_file(dir, "base.txt", "base\n", "base");

    // A local "upstream" remote-tracking setup: a second local clone acting
    // as the remote, so `@{upstream}` resolves without needing a network.
    let upstream_dir = dir.parent().unwrap().join(format!(
        "{}-upstream",
        dir.file_name().unwrap().to_string_lossy()
    ));
    git(
        dir,
        &["clone", "--quiet", ".", upstream_dir.to_str().unwrap()],
    );
    git(
        dir,
        &["remote", "add", "origin", upstream_dir.to_str().unwrap()],
    );
    git(dir, &["fetch", "origin", "--quiet"]);
    git(dir, &["branch", "--set-upstream-to=origin/main", "main"]);

    commit_file(dir, "a.txt", "a\n", "add a");
    let sha_a = git(dir, &["rev-parse", "HEAD"]).trim().to_string();
    commit_file(dir, "b.txt", "b\n", "add b");
    let sha_b = git(dir, &["rev-parse", "HEAD"]).trim().to_string();
    commit_file(dir, "c.txt", "c\n", "add c");
    let sha_c = git(dir, &["rev-parse", "HEAD"]).trim().to_string();
    (sha_a, sha_b, sha_c)
}

/// `git log`'s own default order is newest-first; reverse to oldest-first
/// so assertions read as "in application order".
fn subjects(dir: &Path, range: &str) -> Vec<String> {
    let mut subs: Vec<String> = git(dir, &["log", "--format=%s", range])
        .lines()
        .map(|s| s.to_string())
        .collect();
    subs.reverse();
    subs
}

#[test]
fn open_git_rebase_populates_todo_oldest_first() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_git_rebase_todo());
    let text = doc.buffer.to_string();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(
        lines[0].starts_with("pick") && lines[0].ends_with("add a"),
        "{text}"
    );
    assert!(lines[1].ends_with("add b"), "{text}");
    assert!(lines[2].ends_with("add c"), "{text}");
}

#[test]
fn wq_on_default_todo_completes_a_no_op_rebase() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    editor.apply_git_rebase_todo();

    assert!(
        editor.document_manager.get_document(doc_id).is_none(),
        "todo buffer should close once the rebase completes"
    );
    assert_eq!(
        subjects(&dir, "origin/main..main"),
        vec!["add a", "add b", "add c"]
    );
    let branch = git(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(branch.trim(), "main", "must return to the original branch");
}

#[test]
fn reordering_and_dropping_lines_changes_final_history() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (_sha_a, sha_b, sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    // Reorder b before a (K on b), and drop c entirely (dd on c).
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.move_git_rebase_step(&sha_b, false);
        doc.remove_git_rebase_step(&sha_c);
    }

    editor.apply_git_rebase_todo();

    assert_eq!(subjects(&dir, "origin/main..main"), vec!["add b", "add a"]);
    assert!(
        !dir.join("c.txt").exists() || {
            // c.txt should never have been (re)created since its commit was dropped.
            git(&dir, &["log", "--all", "--format=%s", "--", "c.txt"]).is_empty()
        }
    );
    assert!(!git(&dir, &["log", "--format=%s", "main"]).contains("add c"));
}

#[test]
fn fixup_folds_into_previous_commit_keeping_its_message() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (_sha_a, sha_b, sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    // Move c up once (from [a,b,c] to [a,c,b]) and mark it fixup, so it
    // folds into a while b stays its own commit.
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.move_git_rebase_step(&sha_c, false);
        doc.set_git_rebase_verb(&sha_c, crate::git::rebase::RebaseVerb::Fixup);
    }
    let _ = sha_b;

    editor.apply_git_rebase_todo();

    let subs = subjects(&dir, "origin/main..main");
    assert_eq!(
        subs,
        vec!["add a", "add b"],
        "fixup commit disappears, folded silently"
    );
    // Both a.txt and c.txt's content should be present in the folded commit.
    assert!(dir.join("a.txt").exists());
    assert!(dir.join("c.txt").exists());
}

#[test]
fn squash_combines_messages_of_both_commits() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (_sha_a, sha_b, sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    // Mark b as squash (folds into a, immediately before it) and drop c.
    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.set_git_rebase_verb(&sha_b, crate::git::rebase::RebaseVerb::Squash);
        doc.remove_git_rebase_step(&sha_c);
    }

    editor.apply_git_rebase_todo();

    let subs = subjects(&dir, "origin/main..main");
    assert_eq!(
        subs,
        vec!["add a"],
        "squashed commit disappears as its own entry"
    );
    let final_message = git(&dir, &["log", "-1", "--format=%B", "main"]);
    assert!(final_message.contains("add a"), "{final_message}");
    assert!(final_message.contains("add b"), "{final_message}");
}

#[test]
fn squash_succeeds_inside_a_real_worktree_checkout() {
    // Regression: `amend_with_message`'s (and `apply_git_commit_message`'s, and `cherry_pick_in_progress`'s) temp-file path used to hardcode `repo_root.join(".git")`. In a `git worktree` checkout, `.git` is a plain *file* pointing at the real gitdir elsewhere, not a directory; writing "into" it as a path prefix.
    let root = tempfile::tempdir().unwrap();
    let main_dir = root.path().join("main");
    std::fs::create_dir(&main_dir).unwrap();
    let (_sha_a, sha_b, sha_c) = init_repo_with_three_commits_ahead_of_upstream(&main_dir);

    let worktree_dir = root.path().join("worktree");
    git(
        &main_dir,
        &[
            "worktree",
            "add",
            worktree_dir.to_str().unwrap(),
            "-b",
            "wt-test",
            "HEAD",
        ],
    );
    git(
        &worktree_dir,
        &["branch", "--set-upstream-to=origin/main", "wt-test"],
    );
    assert!(
        !worktree_dir.join(".git").is_dir(),
        "sanity: .git must be the worktree pointer file, not a directory"
    );

    let mut editor = create_editor();
    editor
        .open_file(
            Some(worktree_dir.join("c.txt").display().to_string()),
            false,
        )
        .unwrap();
    editor.open_git_rebase(worktree_dir.to_path_buf());
    let doc_id = editor.active_document_id();

    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.set_git_rebase_verb(&sha_b, crate::git::rebase::RebaseVerb::Squash);
        doc.remove_git_rebase_step(&sha_c);
    }
    editor.apply_git_rebase_todo();

    assert!(
        editor.document_manager.get_document(doc_id).is_none(),
        "the rebase must complete, not error out mid-squash inside a worktree"
    );
    assert_eq!(
        git(&worktree_dir, &["status", "--porcelain"]).trim(),
        "",
        "must not leave staged-but-uncommitted changes behind"
    );
    let final_message = git(&worktree_dir, &["log", "-1", "--format=%B", "wt-test"]);
    assert!(final_message.contains("add a"), "{final_message}");
    assert!(
        final_message.contains("add b"),
        "squash must combine both messages even inside a worktree: {final_message}"
    );
}

#[test]
fn reword_opens_commit_buffer_and_completes_on_save() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (_sha_a, sha_b, _sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let rebase_doc_id = editor.active_document_id();

    {
        let doc = editor
            .document_manager
            .get_document_mut(rebase_doc_id)
            .unwrap();
        doc.set_git_rebase_verb(&sha_b, crate::git::rebase::RebaseVerb::Reword);
    }

    editor.apply_git_rebase_todo();

    // The rebase should now be paused, and a commit-message buffer active.
    assert!(
        editor
            .document_manager
            .get_document(rebase_doc_id)
            .is_some(),
        "rebase todo stays open while paused for reword"
    );
    let msg_doc_id = editor.active_document_id();
    assert_ne!(msg_doc_id, rebase_doc_id);
    {
        let doc = editor.document_manager.get_document(msg_doc_id).unwrap();
        assert_eq!(doc.buffer.to_string().trim(), "add b");
    }

    {
        let doc = editor
            .document_manager
            .get_document_mut(msg_doc_id)
            .unwrap();
        doc.replace_buffer_content("add b (reworded)");
    }
    editor.apply_git_commit_message();

    assert!(
        editor
            .document_manager
            .get_document(rebase_doc_id)
            .is_none(),
        "rebase completes and closes the todo buffer"
    );
    assert_eq!(
        subjects(&dir, "origin/main..main"),
        vec!["add a", "add b (reworded)", "add c"]
    );
}

#[test]
fn edit_pauses_after_cherry_pick_and_resumes_on_save() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (sha_a, _sha_b, _sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.set_git_rebase_verb(&sha_a, crate::git::rebase::RebaseVerb::Edit);
    }

    editor.apply_git_rebase_todo();

    // Paused: "add a" already cherry-picked, HEAD should be that commit.
    assert!(editor.document_manager.get_document(doc_id).is_some());
    let head_subject = git(&dir, &["log", "-1", "--format=%s"]);
    assert_eq!(head_subject.trim(), "add a");

    // Make an additional change to the worktree while paused, matching
    // real "edit" usage (amend the paused commit before continuing).
    std::fs::write(dir.join("a.txt"), "a-edited\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "--amend", "--no-edit", "--quiet"]);

    editor.apply_git_rebase_todo();

    assert!(
        editor.document_manager.get_document(doc_id).is_none(),
        "rebase completes after resuming from an edit pause"
    );
    assert_eq!(
        subjects(&dir, "origin/main..main"),
        vec!["add a", "add b", "add c"]
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("a.txt")).unwrap(),
        "a-edited\n"
    );
}

#[test]
fn edit_pause_auto_resumes_after_amending_via_ca_in_status_buffer() {
    // Regression: previously, finishing an "edit" pause required leaving Rift's own git UI entirely (raw `git add`/`git commit --amend` from outside the editor) and then a separate `:w` on the todo. `ca`/`cw` (the Status buffer's own amend action) should stage/amend AND immediately resume the paused rebase in one.
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (sha_a, _sha_b, _sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    {
        let doc = editor.document_manager.get_document_mut(doc_id).unwrap();
        doc.set_git_rebase_verb(&sha_a, crate::git::rebase::RebaseVerb::Edit);
    }
    editor.apply_git_rebase_todo();

    // Paused: the pause banner should be rendered in the same rich view,
    // not a plain-text dump;  confirm it still carries real annotations.
    {
        let doc = editor.document_manager.get_document(doc_id).unwrap();
        let text = doc.buffer.to_string();
        assert!(text.starts_with("# Paused for edit"), "{text}");
        assert!(
            doc.annotations.git_rebase_step_at_line(1).is_some(),
            "{text}"
        );
    }

    // Edit a file normally in Rift and stage it via the Status buffer's
    // machinery (not a raw shell git command).
    std::fs::write(dir.join("a.txt"), "a-edited\n").unwrap();
    editor.open_git_status();
    drain_jobs(&mut editor);
    {
        let status_doc = editor.document_manager.active_document_mut().unwrap();
        let entry_line = (0..status_doc.buffer.get_total_lines())
            .find(|&l| status_doc.annotations.git_status_entry_at_line(l).is_some())
            .expect("must find the a.txt entry line");
        let start = status_doc.buffer.line_index.get_start(entry_line).unwrap();
        let _ = status_doc.buffer.set_cursor(start);
    }
    editor.git_status_cursor_action("stage");
    drain_jobs(&mut editor);

    // `ca` (amend) should stage/amend AND auto-resume the paused rebase.
    editor.open_git_commit_amend();
    editor.apply_git_commit_message();

    assert!(
        editor.document_manager.get_document(doc_id).is_none(),
        "the rebase must complete: ca's amend should have auto-resumed it"
    );
    assert_eq!(
        subjects(&dir, "origin/main..main"),
        vec!["add a", "add b", "add c"]
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("a.txt")).unwrap(),
        "a-edited\n"
    );
}

#[test]
fn conflicting_pick_pauses_and_resolving_then_saving_resumes() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    git(&dir, &["init", "--quiet", "-b", "main"]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "T"]);
    commit_file(&dir, "shared.txt", "line1\nline2\nline3\n", "base");

    let upstream_dir = root.path().join("repo-upstream");
    git(
        &dir,
        &["clone", "--quiet", ".", upstream_dir.to_str().unwrap()],
    );
    git(
        &dir,
        &["remote", "add", "origin", upstream_dir.to_str().unwrap()],
    );
    git(&dir, &["fetch", "origin", "--quiet"]);
    git(&dir, &["branch", "--set-upstream-to=origin/main", "main"]);

    // Locally, change line 1 one way.
    commit_file(
        &dir,
        "shared.txt",
        "line1-LOCAL\nline2\nline3\n",
        "change local",
    );

    // Upstream diverges, changing the very same line a different way, then main fetches that divergence; so replaying "change local" onto the new `origin/main` conflicts on that line.
    std::fs::write(
        upstream_dir.join("shared.txt"),
        "line1-UPSTREAM\nline2\nline3\n",
    )
    .unwrap();
    git(&upstream_dir, &["add", "shared.txt"]);
    git(
        &upstream_dir,
        &["commit", "-m", "upstream change", "--quiet"],
    );
    git(&dir, &["fetch", "origin", "--quiet"]);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("shared.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    let doc = editor.document_manager.active_document().unwrap();
    let text = doc.buffer.to_string();
    assert_eq!(
        text.lines().count(),
        1,
        "only \"change local\" is ahead of the new origin/main: {text}"
    );

    editor.apply_git_rebase_todo();

    // Paused on the conflict.
    assert!(
        editor.document_manager.get_document(doc_id).is_some(),
        "rebase should still be open, paused on the conflict"
    );
    let status = git(&dir, &["status", "--porcelain=v2"]);
    assert!(status.contains("shared.txt"), "{status}");
    assert!(
        std::fs::read_to_string(dir.join("shared.txt"))
            .unwrap()
            .contains("<<<<<<<"),
        "worktree should show conflict markers"
    );

    // Resolve by combining both changes, then stage it.
    std::fs::write(
        dir.join("shared.txt"),
        "line1-LOCAL-and-UPSTREAM\nline2\nline3\n",
    )
    .unwrap();
    git(&dir, &["add", "shared.txt"]);

    editor.apply_git_rebase_todo();

    assert!(
        editor.document_manager.get_document(doc_id).is_none(),
        "rebase completes after the conflict is resolved and :w resumes it"
    );
    assert_eq!(subjects(&dir, "origin/main..main"), vec!["change local"]);
    assert_eq!(
        std::fs::read_to_string(dir.join("shared.txt")).unwrap(),
        "line1-LOCAL-and-UPSTREAM\nline2\nline3\n"
    );
}

#[test]
fn abort_returns_to_the_original_branch_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);
    let original_tip = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    editor.abort_git_rebase();

    assert!(editor.document_manager.get_document(doc_id).is_none());
    let branch = git(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(branch.trim(), "main");
    let tip = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        tip, original_tip,
        "aborting must not move the original branch"
    );
}

#[test]
fn rebase_todo_key_bindings_resolve_to_the_new_actions() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let editor = create_editor();
    let cases: &[(Key, EditorAction)] = &[
        (Key::Char('K'), EditorAction::GitRebaseMoveUp),
        (Key::Char('J'), EditorAction::GitRebaseMoveDown),
        (Key::Char('p'), EditorAction::GitRebaseSetPick),
        (Key::Char('s'), EditorAction::GitRebaseSetSquash),
        (Key::Char('f'), EditorAction::GitRebaseSetFixup),
        (Key::Char('e'), EditorAction::GitRebaseSetEdit),
        (Key::Char('c'), EditorAction::GitRebaseOpenMessage),
        (Key::Char('r'), EditorAction::GitRebaseOpenMessage),
        (Key::Char('='), EditorAction::GitRebaseToggleFold),
        (Key::Enter, EditorAction::GitRebaseToggleFold),
    ];
    for (key, expected) in cases {
        let result = editor.keymap.lookup(
            KeyContext::Buffer(crate::document::BufferKindId::GIT_REBASE_TODO),
            std::slice::from_ref(key),
        );
        match result {
            MatchResult::Exact(Action::Editor(a)) => {
                assert_eq!(*a, *expected, "key {key:?}")
            }
            other => panic!("key {key:?}: expected {expected:?}, got {other:?}"),
        }
    }
    let dd = editor.keymap.lookup(
        KeyContext::Buffer(crate::document::BufferKindId::GIT_REBASE_TODO),
        &[Key::Char('d'), Key::Char('d')],
    );
    match dd {
        MatchResult::Exact(Action::Editor(EditorAction::GitRebaseDrop)) => {}
        other => panic!("expected 'dd' to resolve to GitRebaseDrop, got {other:?}"),
    }
}

#[test]
fn git_rebase_todo_is_read_only_and_blocks_insert_mode() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());

    let doc = editor.document_manager.active_document().unwrap();
    assert!(doc.is_read_only(), "GitRebaseTodo must be read-only");
    let before = doc.buffer.to_string();
    editor.handle_mode_management(crate::command::Command::EnterInsertMode);
    assert_eq!(
        editor.current_mode,
        crate::mode::Mode::Normal,
        "i must not enter Insert on GitRebaseTodo"
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
        "dd's plain vim meaning must not mutate text directly"
    );
}

#[test]
fn verb_keys_set_pick_squash_fixup_edit_directly_no_insert_mode() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (sha_a, _sha_b, _sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());

    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        let start = doc.buffer.line_index.get_start(0).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Squash);
    let text = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(text.lines().next().unwrap().starts_with("squash"), "{text}");

    editor.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Fixup);
    let text = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(text.lines().next().unwrap().starts_with("fixup"), "{text}");

    editor.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Edit);
    let text = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(text.lines().next().unwrap().starts_with("edit"), "{text}");

    editor.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Pick);
    let text = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(text.lines().next().unwrap().starts_with("pick"), "{text}");
    assert!(text.contains(&sha_a[..8]), "{text}");
}

#[test]
fn reorder_via_dedicated_action_is_a_single_undoable_edit() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());

    let before = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert_eq!(before.lines().count(), 3, "sanity: three commits: {before}");

    // Land on the 2nd commit ("add b") and move it up one slot.
    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        let start = doc.buffer.line_index.get_start(1).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_rebase_move(false);
    let moved = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    let moved_lines: Vec<&str> = moved.lines().collect();
    assert!(moved_lines[0].ends_with("add b"), "{moved}");
    assert!(moved_lines[1].ends_with("add a"), "{moved}");

    // One undo fully reverses the move (a single transaction, not per-char).
    let doc = editor.document_manager.active_document_mut().unwrap();
    assert!(doc.undo(), "the reorder must be undoable");
    let restored = doc.buffer.to_string();
    let restored_lines: Vec<&str> = restored.lines().collect();
    assert!(restored_lines[0].ends_with("add a"), "{restored}");
    assert!(restored_lines[1].ends_with("add b"), "{restored}");
}

#[test]
fn toggle_fold_shows_and_hides_the_body_preview() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);
    // `--amend` amends HEAD, i.e. the LAST commit ("add c");  give that
    // one the multi-line body to preview.
    git(
        &dir,
        &["commit", "--amend", "-m", "add c\n\nSome body text."],
    );

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());

    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        let start = doc.buffer.line_index.get_start(2).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    let collapsed = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(
        !collapsed.contains("Some body text."),
        "starts folded: {collapsed}"
    );

    editor.git_rebase_toggle_fold();
    let expanded = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(expanded.contains("Some body text."), "{expanded}");

    editor.git_rebase_toggle_fold();
    let collapsed_again = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(
        !collapsed_again.contains("Some body text."),
        "{collapsed_again}"
    );
}

#[test]
fn message_editor_round_trip_updates_the_plan_without_touching_git() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let rebase_doc_id = editor.active_document_id();

    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        let start = doc.buffer.line_index.get_start(0).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_rebase_open_message_editor();

    let msg_doc_id = editor.active_document_id();
    assert_ne!(msg_doc_id, rebase_doc_id);
    {
        let doc = editor.document_manager.active_document().unwrap();
        assert_eq!(doc.buffer.to_string().trim(), "add a");
    }
    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        doc.replace_buffer_content("reworded a");
    }
    editor.apply_git_commit_message();

    // Back on the rebase todo, nothing committed yet.
    assert_eq!(editor.active_document_id(), rebase_doc_id);
    let text = editor
        .document_manager
        .active_document()
        .unwrap()
        .buffer
        .to_string();
    assert!(
        text.lines().next().unwrap().ends_with("reworded a"),
        "{text}"
    );
    let real_subject = git(&dir, &["log", "-1", "--format=%s", "HEAD~2"]);
    assert_eq!(
        real_subject.trim(),
        "add a",
        "the real commit must be untouched until :w"
    );
}

#[test]
fn reword_via_pre_edited_message_applies_without_pausing() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        let start = doc.buffer.line_index.get_start(0).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_rebase_open_message_editor();
    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        doc.replace_buffer_content("reworded a");
    }
    editor.apply_git_commit_message();

    editor.apply_git_rebase_todo();

    assert!(
        editor.document_manager.get_document(doc_id).is_none(),
        "a pre-edited pick/reword completes in one pass, no pause"
    );
    assert_eq!(
        subjects(&dir, "origin/main..main"),
        vec!["reworded a", "add b", "add c"]
    );
}

#[test]
fn squash_with_a_pre_edited_message_uses_it_as_the_incoming_half() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    let (_sha_a, sha_b, sha_c) = init_repo_with_three_commits_ahead_of_upstream(&dir);

    let mut editor = create_editor();
    editor
        .open_file(Some(dir.join("c.txt").display().to_string()), false)
        .unwrap();
    editor.open_git_rebase(dir.clone());
    let doc_id = editor.active_document_id();

    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        doc.set_git_rebase_verb(&sha_b, crate::git::rebase::RebaseVerb::Squash);
        doc.remove_git_rebase_step(&sha_c);
        let start = doc.buffer.line_index.get_start(1).unwrap();
        let _ = doc.buffer.set_cursor(start);
    }
    editor.git_rebase_open_message_editor();
    {
        let doc = editor.document_manager.active_document_mut().unwrap();
        doc.replace_buffer_content("tidied up incoming message");
    }
    editor.apply_git_commit_message();

    editor.apply_git_rebase_todo();

    assert!(editor.document_manager.get_document(doc_id).is_none());
    let final_message = git(&dir, &["log", "-1", "--format=%B", "main"]);
    assert!(final_message.contains("add a"), "{final_message}");
    assert!(
        final_message.contains("tidied up incoming message"),
        "expected the edited incoming message, not the original 'add b': {final_message}"
    );
    assert!(!final_message.contains("add b"), "{final_message}");
}
