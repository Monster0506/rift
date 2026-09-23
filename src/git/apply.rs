//! Patch synthesis and hunk-level `git apply` invocation for staging/unstaging individual hunks from a git-status buffer. Whole-file mutations (`git add`, `git restore`) need no patch; they're issued directly via [`super::run_checked`] by the editor layer. Only hunk-granularity actions go through.

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
#[path = "apply_tests.rs"]
mod apply_tests;
