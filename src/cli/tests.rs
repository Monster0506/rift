use super::*;

fn ok(args: &[&str]) -> Args {
    parse_args(args)
        .expect("expected Ok")
        .expect("expected Some")
}

fn err(args: &[&str]) -> String {
    parse_args(args).expect_err("expected Err")
}

// ── version ──────────────────────────────────────────────────────────────────

#[test]
fn version_short() {
    assert!(parse_args(&["-v"]).unwrap().is_none());
}

#[test]
fn version_long() {
    assert!(parse_args(&["--version"]).unwrap().is_none());
}

#[test]
fn version_before_file() {
    assert!(parse_args(&["-v", "file.txt"]).unwrap().is_none());
}

// ── empty ─────────────────────────────────────────────────────────────────────

#[test]
fn empty_args() {
    assert_eq!(ok(&[]), Args::default());
}

// ── file ──────────────────────────────────────────────────────────────────────

#[test]
fn bare_file() {
    assert_eq!(ok(&["file.txt"]).file.as_deref(), Some("file.txt"));
}

#[test]
fn two_files_is_error() {
    assert!(err(&["a.txt", "b.txt"]).contains("unexpected argument"));
}

// ── goto ──────────────────────────────────────────────────────────────────────

#[test]
fn plus_alone_is_last_line() {
    assert_eq!(ok(&["+"]).goto, Some(Goto::LastLine));
}

#[test]
fn plus_number() {
    assert_eq!(ok(&["+42"]).goto, Some(Goto::Line(42)));
}

#[test]
fn plus_one() {
    assert_eq!(ok(&["+1"]).goto, Some(Goto::Line(1)));
}

#[test]
fn plus_zero() {
    assert_eq!(ok(&["+0"]).goto, Some(Goto::Line(0)));
}

#[test]
fn goto_last_wins_number_then_plus() {
    assert_eq!(ok(&["+42", "+"]).goto, Some(Goto::LastLine));
}

#[test]
fn goto_last_wins_plus_then_number() {
    assert_eq!(ok(&["+", "+99"]).goto, Some(Goto::Line(99)));
}

#[test]
fn goto_last_wins_two_numbers() {
    assert_eq!(ok(&["+10", "+20"]).goto, Some(Goto::Line(20)));
}

#[test]
fn plus_invalid_number_is_error() {
    assert!(err(&["+abc"]).contains("invalid line number"));
}

#[test]
fn goto_after_file() {
    let a = ok(&["file.txt", "+5"]);
    assert_eq!(a.file.as_deref(), Some("file.txt"));
    assert_eq!(a.goto, Some(Goto::Line(5)));
}

#[test]
fn goto_before_file() {
    let a = ok(&["+5", "file.txt"]);
    assert_eq!(a.file.as_deref(), Some("file.txt"));
    assert_eq!(a.goto, Some(Goto::Line(5)));
}

// ── search ────────────────────────────────────────────────────────────────────

#[test]
fn plus_slash_pattern() {
    assert_eq!(ok(&["+/TODO"]).search.as_deref(), Some("TODO"));
}

#[test]
fn search_with_spaces_in_pattern() {
    assert_eq!(ok(&["+/fn main"]).search.as_deref(), Some("fn main"));
}

#[test]
fn search_last_wins() {
    assert_eq!(ok(&["+/foo", "+/bar"]).search.as_deref(), Some("bar"));
}

#[test]
fn search_empty_pattern() {
    assert_eq!(ok(&["+/"]).search.as_deref(), Some(""));
}

// ── -c / --cmd ────────────────────────────────────────────────────────────────

#[test]
fn dash_c() {
    assert_eq!(ok(&["-c", "set wrap"]).commands, vec!["set wrap"]);
}

#[test]
fn dash_dash_cmd() {
    assert_eq!(ok(&["--cmd", "set wrap"]).commands, vec!["set wrap"]);
}

#[test]
fn commands_accumulate() {
    let a = ok(&["-c", "set wrap", "-c", "set number"]);
    assert_eq!(a.commands, vec!["set wrap", "set number"]);
}

#[test]
fn commands_mixed_flags() {
    let a = ok(&["-c", "set wrap", "--cmd", "set number"]);
    assert_eq!(a.commands, vec!["set wrap", "set number"]);
}

#[test]
fn dash_c_missing_arg_is_error() {
    assert!(err(&["-c"]).contains("requires a command argument"));
}

#[test]
fn cmd_missing_arg_is_error() {
    assert!(err(&["--cmd"]).contains("requires a command argument"));
}

// ── unknown flags ─────────────────────────────────────────────────────────────

#[test]
fn unknown_flag_is_error() {
    assert!(err(&["--foo"]).contains("unknown flag"));
}

#[test]
fn unknown_short_flag_is_error() {
    assert!(err(&["-z"]).contains("unknown flag"));
}

// ── combinations ─────────────────────────────────────────────────────────────

#[test]
fn file_goto_search_cmd() {
    let a = ok(&["+42", "+/TODO", "-c", "set wrap", "main.rs"]);
    assert_eq!(a.file.as_deref(), Some("main.rs"));
    assert_eq!(a.goto, Some(Goto::Line(42)));
    assert_eq!(a.search.as_deref(), Some("TODO"));
    assert_eq!(a.commands, vec!["set wrap"]);
}
