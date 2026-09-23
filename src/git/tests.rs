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
