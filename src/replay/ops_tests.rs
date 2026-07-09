use super::*;

#[test]
fn blank_lines_and_comments_are_skipped() {
    let ops = parse("# a comment\n\n  \nmark start\n").unwrap();
    assert_eq!(ops, vec![ScriptOp::Mark("start".to_string())]);
}

#[test]
fn open_and_size_parse() {
    let ops = parse("open fixtures/sample.rs\nsize 40 120\n").unwrap();
    assert_eq!(
        ops,
        vec![
            ScriptOp::Open("fixtures/sample.rs".to_string()),
            ScriptOp::Size {
                rows: 40,
                cols: 120
            },
        ]
    );
}

#[test]
fn new_parses_with_no_args() {
    assert_eq!(parse("new\n").unwrap(), vec![ScriptOp::New]);
}

#[test]
fn keys_parse_vim_notation() {
    let ops = parse("keys ihello<Esc>\n").unwrap();
    let ScriptOp::Keys(keys) = &ops[0] else {
        panic!("expected Keys op");
    };
    assert_eq!(keys.first(), Some(&Key::Char('i')));
    assert_eq!(keys.last(), Some(&Key::Escape));
}

#[test]
fn keys_reject_unknown_notation() {
    let err = parse("keys <bogus>\n").unwrap_err();
    assert_eq!(err.line, 1);
}

#[test]
fn wait_idle_defaults_timeout_when_omitted() {
    let ops = parse("wait idle\n").unwrap();
    assert_eq!(
        ops,
        vec![ScriptOp::WaitIdle {
            timeout_ms: DEFAULT_WAIT_IDLE_MS
        }]
    );
}

#[test]
fn wait_idle_accepts_explicit_timeout() {
    let ops = parse("wait idle 500\n").unwrap();
    assert_eq!(ops, vec![ScriptOp::WaitIdle { timeout_ms: 500 }]);
}

#[test]
fn wait_rejects_unknown_kind() {
    let err = parse("wait forever\n").unwrap_err();
    assert_eq!(err.line, 1);
}

#[test]
fn assert_cursor_parses_row_and_col() {
    let ops = parse("assert cursor 3 7\n").unwrap();
    assert_eq!(
        ops,
        vec![ScriptOp::Assert(Assertion::Cursor { row: 3, col: 7 })]
    );
}

#[test]
fn assert_mode_parses_bare_name() {
    let ops = parse("assert mode insert\n").unwrap();
    assert_eq!(
        ops,
        vec![ScriptOp::Assert(Assertion::Mode("insert".to_string()))]
    );
}

#[test]
fn assert_line_unquotes_expected_text() {
    let ops = parse("assert line 5 \"hello world\"\n").unwrap();
    assert_eq!(
        ops,
        vec![ScriptOp::Assert(Assertion::Line {
            row: 5,
            text: "hello world".to_string(),
        })]
    );
}

#[test]
fn assert_buffer_reads_fenced_block() {
    let script = "assert buffer\n<<<\nfn main() {}\nline two\n>>>\n";
    let ops = parse(script).unwrap();
    assert_eq!(
        ops,
        vec![ScriptOp::Assert(Assertion::Buffer(
            "fn main() {}\nline two".to_string()
        ))]
    );
}

#[test]
fn assert_buffer_without_open_fence_errors() {
    let err = parse("assert buffer\nnot a fence\n>>>\n").unwrap_err();
    assert_eq!(err.line, 1);
}

#[test]
fn assert_buffer_without_close_fence_errors() {
    let err = parse("assert buffer\n<<<\nunterminated\n").unwrap_err();
    assert_eq!(err.line, 1);
}

#[test]
fn unknown_directive_errors_with_line_number() {
    let err = parse("open a.txt\nbogus\n").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.message.contains("bogus"));
}

#[test]
fn parse_error_display_includes_line_and_message() {
    let err = ParseError {
        line: 4,
        message: "boom".to_string(),
    };
    assert_eq!(err.to_string(), "line 4: boom");
}
