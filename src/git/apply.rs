//! Patch synthesis and hunk-level `git apply` invocation for staging/unstaging individual hunks from a `BufferKind::GitStatus` buffer. Whole-file mutations (`git add`, `git restore`) need no patch; they're issued directly via [`super::run_checked`] by the editor layer. Only hunk-granularity actions go through.

use super::diff::{DiffLineKind, Hunk};
use crate::error::{ErrorType, RiftError};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Render `hunk` as a standalone unified-diff patch for `path`, suitable for `git apply`. A hunk never changes a file's path (renames are a whole-file operation elsewhere), so `path` is used verbatim on both sides. `is_new_file` selects the header `git apply --cached` needs to create a brand-new index entry for.
pub fn render_patch(path: &str, hunk: &Hunk, is_new_file: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("diff --git a/{path} b/{path}\n"));
    if is_new_file {
        out.push_str("new file mode 100644\n");
        out.push_str("--- /dev/null\n");
    } else {
        out.push_str(&format!("--- a/{path}\n"));
    }
    out.push_str(&format!("+++ b/{path}\n"));
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@{}\n",
        hunk.old_start,
        hunk.old_lines,
        hunk.new_start,
        hunk.new_lines,
        if hunk.header.is_empty() {
            String::new()
        } else {
            format!(" {}", hunk.header)
        }
    ));
    for line in &hunk.lines {
        let marker = match line.kind {
            DiffLineKind::Context => ' ',
            DiffLineKind::Addition => '+',
            DiffLineKind::Deletion => '-',
        };
        out.push(marker);
        out.push_str(&line.content);
        out.push('\n');
    }
    out
}

/// Run `git apply <extra_args>` in `repo_root`, piping `patch` over stdin. (`--no-ext-diff` is a `git diff` flag; `git apply` has no such option; it always applies the literal patch text it's given.)
fn apply_patch(repo_root: &Path, patch: &str, extra_args: &[&str]) -> Result<(), RiftError> {
    let mut args = vec!["apply"];
    args.extend_from_slice(extra_args);

    let mut child = Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            RiftError::new(
                ErrorType::Execution,
                "GIT_APPLY_SPAWN_FAILED",
                format!("failed to run git apply: {e}"),
            )
        })?;

    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(patch.as_bytes())
        .map_err(|e| {
            RiftError::new(
                ErrorType::Execution,
                "GIT_APPLY_WRITE_FAILED",
                e.to_string(),
            )
        })?;

    let output = child.wait_with_output().map_err(|e| {
        RiftError::new(ErrorType::Execution, "GIT_APPLY_WAIT_FAILED", e.to_string())
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(RiftError::new(
            ErrorType::Execution,
            "GIT_APPLY_FAILED",
            format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ))
    }
}

/// Stages exactly `hunk` with `git apply --cached`. `is_new_file` emits a new-file patch header.
pub fn stage_hunk(
    repo_root: &Path,
    path: &str,
    hunk: &Hunk,
    is_new_file: bool,
) -> Result<(), RiftError> {
    apply_patch(
        repo_root,
        &render_patch(path, hunk, is_new_file),
        &["--cached"],
    )
}

/// Unstage exactly `hunk`, leaving the worktree untouched (`git apply --cached -R`).
pub fn unstage_hunk(repo_root: &Path, path: &str, hunk: &Hunk) -> Result<(), RiftError> {
    apply_patch(
        repo_root,
        &render_patch(path, hunk, false),
        &["--cached", "-R"],
    )
}

/// Reverse-apply `hunk` from the worktree only (`git apply -R`), discarding
/// it from an unstaged diff.
pub fn discard_hunk_worktree(repo_root: &Path, path: &str, hunk: &Hunk) -> Result<(), RiftError> {
    apply_patch(repo_root, &render_patch(path, hunk, false), &["-R"])
}

#[cfg(test)]
mod tests {
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

        let staged =
            super::super::run_checked(root, &["diff", "--no-ext-diff", "--cached"]).unwrap();
        assert!(staged.contains("-    old();"));
        assert!(staged.contains("+    new();"));

        let worktree_diff = super::super::run_checked(root, &["diff", "--no-ext-diff"]).unwrap();
        assert!(
            worktree_diff.trim().is_empty(),
            "worktree diff should be empty after the only hunk was staged: {worktree_diff}"
        );
    }
}
