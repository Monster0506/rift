use super::*;

fn repo_with_branch() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| crate::git::run_checked(root, args).unwrap();
    git(&["init", "--quiet", "--initial-branch=main"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "T"]);
    std::fs::write(root.join("tracked.txt"), "one\n").unwrap();
    git(&["add", "tracked.txt"]);
    git(&["commit", "-m", "base", "--quiet"]);
    git(&["checkout", "--quiet", "-b", "feature"]);
    std::fs::write(root.join("tracked.txt"), "one\ntwo\n").unwrap();
    git(&["commit", "-am", "feature work", "--quiet"]);
    dir
}

fn open(root: &Path, line: &str) -> GitOutput {
    let query = GitQuery::parse(line);
    run(root, &query, query.view()).unwrap()
}

#[test]
fn a_revision_range_opens_the_structured_log_of_that_range() {
    let dir = repo_with_branch();
    let GitOutput::Log(commits) = open(dir.path(), "log main..feature") else {
        panic!("expected a structured log");
    };
    let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["feature work"]);
}

#[test]
fn flags_that_change_the_output_shape_fall_back_to_git_text() {
    let dir = repo_with_branch();
    for line in [
        "log -p -1",
        "log --graph",
        "status -s",
        "show HEAD:tracked.txt",
    ] {
        let GitOutput::Text { text, success } = open(dir.path(), line) else {
            panic!("`{line}` must open as text");
        };
        assert!(success, "`{line}`: {text}");
        assert!(
            !text.contains('\u{1e}'),
            "`{line}` leaked rift's format: {text}"
        );
    }
    let GitOutput::Text { text, .. } = open(dir.path(), "show HEAD:tracked.txt") else {
        unreachable!();
    };
    assert_eq!(text, "one\ntwo\n");
}

#[test]
fn a_failing_query_reports_gits_own_error() {
    let dir = repo_with_branch();
    let GitOutput::Text { text, success } = open(dir.path(), "log no-such-rev") else {
        panic!("a failing query must report text");
    };
    assert!(!success);
    assert!(text.contains("no-such-rev"), "{text}");
}

#[test]
fn show_lists_the_named_commits_and_status_parses_porcelain() {
    let dir = repo_with_branch();
    let GitOutput::Log(commits) = open(dir.path(), "show main") else {
        panic!("expected show to list its commit");
    };
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "base");
    std::fs::write(dir.path().join("new.txt"), "x").unwrap();
    let GitOutput::Status { snapshot, .. } = open(dir.path(), "") else {
        panic!("a bare query must open status");
    };
    assert!(snapshot.entries.iter().any(|e| e.path.ends_with("new.txt")));
}

#[test]
fn blame_target_separates_flags_revision_and_file() {
    let target = GitQuery::parse("blame -w HEAD~2 src/main.rs").blame_target();
    assert_eq!(target.flags, ["-w"]);
    assert_eq!(target.rev.as_deref(), Some("HEAD~2"));
    assert_eq!(target.path.as_deref(), Some("src/main.rs"));

    let target = GitQuery::parse("blame -C -- -odd-name").blame_target();
    assert_eq!(target.flags, ["-C"]);
    assert_eq!(target.rev, None);
    assert_eq!(target.path.as_deref(), Some("-odd-name"));

    assert_eq!(GitQuery::parse("blame -C").blame_target().path, None);
}
