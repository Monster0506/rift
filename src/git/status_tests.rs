use super::*;

#[test]
fn parses_branch_header_with_upstream_and_ahead_behind() {
    let snapshot = parse_status(
        "# branch.oid abc123\n\
         # branch.head main\n\
         # branch.upstream origin/main\n\
         # branch.ab +2 -1\n",
    );
    assert_eq!(snapshot.branch.oid.as_deref(), Some("abc123"));
    assert_eq!(snapshot.branch.head.as_deref(), Some("main"));
    assert_eq!(snapshot.branch.upstream.as_deref(), Some("origin/main"));
    assert_eq!(snapshot.branch.ahead, 2);
    assert_eq!(snapshot.branch.behind, 1);
}

#[test]
fn parses_initial_commit_and_detached_head_as_none() {
    let snapshot = parse_status("# branch.oid (initial)\n# branch.head (detached)\n");
    assert_eq!(snapshot.branch.oid, None);
    assert_eq!(snapshot.branch.head, None);
}

#[test]
fn parses_ordinary_staged_and_unstaged_entries() {
    let snapshot = parse_status(
        "1 M. N... 100644 100644 100644 aaa bbb staged.rs\n\
         1 .M N... 100644 100644 100644 aaa bbb unstaged.rs\n\
         1 MM N... 100644 100644 100644 aaa bbb both.rs\n",
    );
    assert_eq!(snapshot.entries.len(), 3);

    let staged = &snapshot.entries[0];
    assert_eq!(staged.path, PathBuf::from("staged.rs"));
    assert_eq!(staged.index_state, FileState::Modified);
    assert_eq!(staged.worktree_state, FileState::Unmodified);
    assert!(staged.is_staged());
    assert!(!staged.is_unstaged());

    let unstaged = &snapshot.entries[1];
    assert!(!unstaged.is_staged());
    assert!(unstaged.is_unstaged());

    let both = &snapshot.entries[2];
    assert!(both.is_staged());
    assert!(both.is_unstaged());
}

#[test]
fn parses_untracked_and_ignored_paths_with_spaces() {
    let snapshot = parse_status(
        "? some file with spaces.txt\n\
         ! build/\n",
    );
    assert_eq!(snapshot.entries.len(), 2);
    assert!(snapshot.entries[0].is_untracked());
    assert_eq!(
        snapshot.entries[0].path,
        PathBuf::from("some file with spaces.txt")
    );
    assert!(snapshot.entries[1].is_ignored());
    assert_eq!(snapshot.entries[1].path, PathBuf::from("build/"));
}

#[test]
fn parses_renamed_entry_with_score_and_orig_path() {
    let snapshot =
        parse_status("2 R. N... 100644 100644 100644 aaa bbb R100 new_name.rs\told_name.rs\n");
    assert_eq!(snapshot.entries.len(), 1);
    let entry = &snapshot.entries[0];
    assert_eq!(entry.path, PathBuf::from("new_name.rs"));
    assert_eq!(entry.orig_path, Some(PathBuf::from("old_name.rs")));
    assert_eq!(entry.index_state, FileState::Renamed);
    assert_eq!(
        entry.kind,
        EntryKind::RenamedOrCopied {
            score: "R100".to_string()
        }
    );
}

#[test]
fn parses_unmerged_entry() {
    let snapshot =
        parse_status("u UU N... 100644 100644 100644 100644 aaa bbb ccc conflicted.txt\n");
    assert_eq!(snapshot.entries.len(), 1);
    let entry = &snapshot.entries[0];
    assert!(entry.is_unmerged());
    assert!(!entry.is_staged());
    assert!(!entry.is_unstaged());
    assert_eq!(entry.index_state, FileState::UpdatedUnmerged);
    assert_eq!(entry.worktree_state, FileState::UpdatedUnmerged);
}

#[test]
fn skips_unrecognized_lines_without_panicking() {
    let snapshot = parse_status("# some future header\ngarbage line\n? real.txt\n");
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].path, PathBuf::from("real.txt"));
}

#[test]
fn empty_input_yields_default_snapshot() {
    let snapshot = parse_status("");
    assert_eq!(snapshot, StatusSnapshot::default());
}
