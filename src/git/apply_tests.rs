use super::*;
use crate::git::diff::DiffLine;

fn sample_hunk() -> Hunk {
    Hunk {
        old_start: 1,
        old_lines: 3,
        new_start: 1,
        new_lines: 3,
        header: String::new(),
        lines: vec![
            DiffLine {
                kind: DiffLineKind::Context,
                content: "fn main() {".to_string(),
                old_lineno: Some(1),
                new_lineno: Some(1),
            },
            DiffLine {
                kind: DiffLineKind::Deletion,
                content: "    old();".to_string(),
                old_lineno: Some(2),
                new_lineno: None,
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                content: "    new();".to_string(),
                old_lineno: None,
                new_lineno: Some(2),
            },
            DiffLine {
                kind: DiffLineKind::Context,
                content: "}".to_string(),
                old_lineno: Some(3),
                new_lineno: Some(3),
            },
        ],
    }
}

#[test]
fn render_patch_produces_valid_unified_diff_shape() {
    let hunk = sample_hunk();
    let patch = render_patch("src/main.rs", &hunk, false);
    assert!(patch.starts_with("diff --git a/src/main.rs b/src/main.rs\n"));
    assert!(patch.contains("--- a/src/main.rs\n"));
    assert!(patch.contains("+++ b/src/main.rs\n"));
    assert!(patch.contains("@@ -1,3 +1,3 @@\n"));
    assert!(patch.contains(" fn main() {\n"));
    assert!(patch.contains("-    old();\n"));
    assert!(patch.contains("+    new();\n"));
    assert!(patch.contains(" }\n"));
}

#[test]
fn render_patch_includes_section_heading_when_present() {
    let mut hunk = sample_hunk();
    hunk.header = "fn main() {".to_string();
    let patch = render_patch("f.rs", &hunk, false);
    assert!(patch.contains("@@ -1,3 +1,3 @@ fn main() {\n"));
}

#[test]
fn stage_hunk_round_trips_through_a_real_repo() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    super::super::run_checked(root, &["init", "--quiet"]).unwrap();
    super::super::run_checked(root, &["config", "user.email", "t@example.com"]).unwrap();
    super::super::run_checked(root, &["config", "user.name", "T"]).unwrap();

    std::fs::write(root.join("f.txt"), "fn main() {\n    old();\n}\n").unwrap();
    super::super::run_checked(root, &["add", "f.txt"]).unwrap();
    super::super::run_checked(root, &["commit", "-m", "init", "--quiet"]).unwrap();

    std::fs::write(root.join("f.txt"), "fn main() {\n    new();\n}\n").unwrap();
    let diff_out = super::super::run_checked(root, &["diff", "--no-ext-diff", "-U3"]).unwrap();
    let files = crate::git::diff::parse_unified_diff(&diff_out);
    assert_eq!(files.len(), 1);
    let hunk = &files[0].hunks[0];

    stage_hunk(root, "f.txt", hunk, false).unwrap();

    let staged = super::super::run_checked(root, &["diff", "--no-ext-diff", "--cached"]).unwrap();
    assert!(staged.contains("-    old();"));
    assert!(staged.contains("+    new();"));

    let worktree_diff = super::super::run_checked(root, &["diff", "--no-ext-diff"]).unwrap();
    assert!(
        worktree_diff.trim().is_empty(),
        "worktree diff should be empty after the only hunk was staged: {worktree_diff}"
    );
}
