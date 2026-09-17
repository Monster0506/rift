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
mod tests {
    use super::*;

    // Tests use temporary repositories for commands and fixture strings for parsers.

    fn init_repo(dir: &Path) {
        run_checked(dir, &["init", "--quiet"]).unwrap();
        run_checked(dir, &["config", "user.email", "test@example.com"]).unwrap();
        run_checked(dir, &["config", "user.name", "Test"]).unwrap();
    }

    #[test]
    fn format_unix_date_epoch_is_1970_01_01() {
        assert_eq!(format_unix_date(0), "1970-01-01");
    }

    #[test]
    fn format_unix_date_handles_a_known_recent_date() {
        // 2024-01-15T00:00:00Z
        assert_eq!(format_unix_date(1_705_276_800), "2024-01-15");
    }

    #[test]
    fn format_unix_date_handles_leap_day() {
        // 2024-02-29T12:00:00Z
        assert_eq!(format_unix_date(1_709_208_000), "2024-02-29");
    }

    #[test]
    fn format_unix_date_handles_pre_epoch_timestamps() {
        // 1969-12-31T00:00:00Z
        assert_eq!(format_unix_date(-86400), "1969-12-31");
    }

    #[test]
    fn format_unix_datetime_includes_time() {
        // 2024-01-15T13:45:00Z
        assert_eq!(format_unix_datetime(1_705_326_300), "2024-01-15 13:45");
    }

    #[test]
    fn run_captures_stdout_on_success() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let out = run(dir.path(), &["rev-parse", "--is-inside-work-tree"]).unwrap();
        assert!(out.success);
        assert_eq!(out.stdout.trim(), "true");
    }

    #[test]
    fn run_reports_failure_without_erroring() {
        let dir = tempfile::tempdir().unwrap();
        // Not a repo: git exits non-zero but the process still runs fine.
        let out = run(dir.path(), &["rev-parse", "--show-toplevel"]).unwrap();
        assert!(!out.success);
        assert!(!out.stderr.is_empty());
    }

    #[test]
    fn run_checked_errors_on_non_repo() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_checked(dir.path(), &["rev-parse", "--show-toplevel"]);
        assert!(err.is_err());
    }

    #[test]
    fn discover_repo_finds_toplevel_and_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());

        let paths = discover_repo(dir.path()).unwrap();
        assert_eq!(
            crate::fs_backend::backend().canonicalize(&paths.root),
            crate::fs_backend::backend().canonicalize(dir.path())
        );
        assert!(paths.git_dir.is_absolute());
        assert!(paths.git_dir.ends_with(".git"));
    }

    #[test]
    fn discover_repo_from_subdirectory_finds_same_root() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let sub = dir.path().join("nested");
        std::fs::create_dir(&sub).unwrap();

        let paths = discover_repo(&sub).unwrap();
        assert_eq!(
            crate::fs_backend::backend().canonicalize(&paths.root),
            crate::fs_backend::backend().canonicalize(dir.path())
        );
    }

    #[test]
    fn discover_repo_errors_outside_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        assert!(discover_repo(dir.path()).is_err());
    }
}
