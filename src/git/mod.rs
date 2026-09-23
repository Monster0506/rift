//! Git backend: subprocess wrapper, repository discovery, and porcelain/diff parsers.

use crate::error::{ErrorType, RiftError};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub mod apply;
pub mod blame;
pub mod diff;
pub mod log;
pub mod rebase;
pub mod status;

/// Captured result of running a `git` subprocess: exit status plus stdout/stderr.
#[derive(Debug, Clone)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// Run `git <args>` in `cwd`, capturing stdout/stderr. A non-zero exit is not itself surfaced as an `Err` here; some callers need to distinguish "ran, but git said no" (e.g. `git diff --exit-code`, `git merge-base --is-ancestor`) from "couldn't even spawn git". Use [`run_checked`] when any non-zero exit should.
pub fn run(cwd: &Path, args: &[&str]) -> Result<GitOutput, RiftError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        // Every git invocation from this editor is plumbing: never let git block waiting for an interactive editor or page output through a pager (e.g. `cherry-pick --continue`'s default commit-message prompt, or `log`'s pager); `true` exits 0 immediately, keeping whatever message/content git already had.
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            RiftError::new(
                ErrorType::Execution,
                "GIT_SPAWN_FAILED",
                format!("failed to run `git {}`: {e}", args.join(" ")),
            )
        })?;

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    })
}

/// Run `git <args>` in `cwd`, returning stdout on success or an `Err`
/// carrying stderr's text on a non-zero exit or spawn failure.
pub fn run_checked(cwd: &Path, args: &[&str]) -> Result<String, RiftError> {
    let out = run(cwd, args)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(RiftError::new(
            ErrorType::Execution,
            "GIT_COMMAND_FAILED",
            format!("git {} failed: {}", args.join(" "), out.stderr.trim()),
        ))
    }
}

/// The two roots that matter for a repository: its worktree root and its git-dir. Both come from `git rev-parse`, which already resolves worktrees, submodules, and `GIT_DIR` overrides correctly; no bespoke worktree handling is needed on top of this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoPaths {
    /// Top-level worktree directory (`git rev-parse --show-toplevel`).
    pub root: PathBuf,
    /// Absolute git directory. `git rev-parse --git-dir` prints a path relative to `start` when invoked with a relative cwd, so this is resolved against `start` to always be absolute.
    pub git_dir: PathBuf,
}

/// Discover the repository containing `start` (a directory). Fails with a
/// descriptive error if `start` is not inside a git repository at all.
pub fn discover_repo(start: &Path) -> Result<RepoPaths, RiftError> {
    let root = run_checked(start, &["rev-parse", "--show-toplevel"])?;
    let git_dir_raw = run_checked(start, &["rev-parse", "--git-dir"])?;

    let root = PathBuf::from(root.trim());
    let git_dir_raw = PathBuf::from(git_dir_raw.trim());
    let git_dir = if git_dir_raw.is_absolute() {
        git_dir_raw
    } else {
        start.join(git_dir_raw)
    };

    Ok(RepoPaths { root, git_dir })
}

/// Resolve `repo_root`'s real git-dir alone (one subprocess call, not the two `discover_repo` needs); for callers that already trust `repo_root` and just need a real, writable git-dir for scratch files (commit message temp files) or in-progress-state checks (`CHERRY_PICK_HEAD`). **Never** hardcode.
pub fn git_dir(repo_root: &Path) -> Result<PathBuf, RiftError> {
    let raw = run_checked(repo_root, &["rev-parse", "--git-dir"])?;
    let raw = PathBuf::from(raw.trim());
    Ok(if raw.is_absolute() {
        raw
    } else {
        repo_root.join(raw)
    })
}

/// Format a Unix timestamp as a UTC `YYYY-MM-DD` date. No timezone library dependency (this crate has none); good enough for blame/log display; deliberately not adjusted by a commit's own `author_tz`/`committer_tz`.
pub fn format_unix_date(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Format a Unix timestamp as a UTC `YYYY-MM-DD HH:MM` timestamp.
pub fn format_unix_datetime(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86400);
    let seconds = unix_seconds.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hour = seconds / 3600;
    let minute = seconds % 3600 / 60;
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02}")
}

/// Howard Hinnant's `civil_from_days`: days-since-epoch (1970-01-01) ->
/// `(year, month, day)`. Proleptic Gregorian calendar, valid for any `i64`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
