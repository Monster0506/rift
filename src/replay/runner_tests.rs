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
fn tick_percentiles_are_none_without_any_keys() {
    let ops = parse("new\nmark only\n").unwrap();
    let report = run(&ops, Vec::new()).unwrap();
    assert!(report.tick_percentiles().is_none());
}

#[test]
fn tick_percentiles_cover_every_scripted_key() {
    let ops = parse("new\nkeys ihello<Esc>\n").unwrap();
    let report = run(&ops, Vec::new()).unwrap();
    assert_eq!(report.ticks.len(), 7); // i h e l l o <Esc>
    let p = report.tick_percentiles().unwrap();
    assert!(p.p50 <= p.p95);
    assert!(p.p95 <= p.max);
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

#[test]
fn assertions_pass_against_matching_state() {
    let script = "new\nkeys ihello<Esc>\n\
                  assert mode normal\nassert cursor 0 5\nassert line 0 \"hello\"\n\
                  assert buffer\n<<<\nhello\n>>>\n";
    let ops = parse(script).unwrap();
    run(&ops, Vec::new()).unwrap();
}

#[test]
fn assert_cursor_fails_with_actual_position_in_the_message() {
    let ops = parse("new\nkeys ihello<Esc>\nassert cursor 0 0\n").unwrap();
    let err = run(&ops, Vec::new()).unwrap_err();
    assert_eq!(err.code, "REPLAY_ASSERT");
    assert!(err.message.contains("0:0"));
    assert!(err.message.contains("0:5"));
}

#[test]
fn assert_mode_fails_when_still_in_insert() {
    let ops = parse("new\nkeys ihello\nassert mode normal\n").unwrap();
    let err = run(&ops, Vec::new()).unwrap_err();
    assert_eq!(err.code, "REPLAY_ASSERT");
    assert!(err.message.contains("insert"));
}

#[test]
fn assert_line_fails_on_mismatched_text() {
    let ops = parse("new\nkeys ihello<Esc>\nassert line 0 \"goodbye\"\n").unwrap();
    let err = run(&ops, Vec::new()).unwrap_err();
    assert_eq!(err.code, "REPLAY_ASSERT");
}
