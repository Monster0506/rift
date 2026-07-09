use super::*;
use crate::replay::ops::parse;

#[test]
fn typing_into_a_new_buffer_records_marks_in_order() {
    let ops = parse("new\nmark before\nkeys ihello<Esc>\nmark after\n").unwrap();
    let report = run(&ops, Vec::new()).unwrap();
    let labels: Vec<&str> = report.marks.iter().map(|m| m.label.as_str()).collect();
    assert_eq!(labels, vec!["before", "after"]);
    assert!(report.marks[0].at <= report.marks[1].at);
}

#[test]
fn keys_are_encoded_through_the_real_crossterm_write_path() {
    let ops = parse("new\nkeys ihello<Esc>\n").unwrap();
    let mut captured = Vec::new();
    run(&ops, &mut captured).unwrap();
    assert!(
        !captured.is_empty(),
        "expected real ANSI bytes to be written"
    );
}

#[test]
fn size_after_session_start_is_rejected() {
    let ops = parse("new\nkeys ihello<Esc>\nsize 10 10\n").unwrap();
    let err = run(&ops, Vec::new()).unwrap_err();
    assert_eq!(err.code, "REPLAY_ORDER");
}

#[test]
fn open_only_script_still_starts_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, "hello\n").unwrap();
    let script = format!("open {}\n", path.display());
    let ops = parse(&script).unwrap();
    let report = run(&ops, Vec::new()).unwrap();
    assert!(report.marks.is_empty());
}

#[test]
fn wait_idle_returns_after_roughly_its_timeout() {
    let ops = parse("new\nwait idle 20\nmark done\n").unwrap();
    let start = std::time::Instant::now();
    let report = run(&ops, Vec::new()).unwrap();
    assert_eq!(report.marks.len(), 1);
    assert!(start.elapsed() >= Duration::from_millis(20));
}
