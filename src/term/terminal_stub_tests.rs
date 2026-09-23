use super::*;

#[test]
fn new_always_errs() {
    let result = Terminal::new(24, 80, None);
    assert!(result.is_err());
}

#[test]
fn new_errs_regardless_of_shell_cmd() {
    let result = Terminal::new(1, 1, Some("bash".to_string()));
    assert!(result.is_err());
}

#[test]
fn resize_updates_size() {
    let mut term = Terminal {
        size: (24, 80),
        name: "stub".to_string(),
    };
    term.resize(30, 100).unwrap();
    assert_eq!(term.size, (30, 100));
}

#[test]
fn write_is_a_silent_no_op() {
    let mut term = Terminal {
        size: (24, 80),
        name: "stub".to_string(),
    };
    assert!(term.write(b"ignored").is_ok());
}

#[test]
fn scroll_methods_are_no_ops() {
    let term = Terminal {
        size: (24, 80),
        name: "stub".to_string(),
    };
    term.scroll_display(5);
    term.scroll_to_bottom();
}

#[test]
fn read_screen_is_empty() {
    let term = Terminal {
        size: (24, 80),
        name: "stub".to_string(),
    };
    let (text, cursor_row, cursor_col, spans) = term.read_screen();
    assert_eq!(text, "");
    assert_eq!(cursor_row, 0);
    assert_eq!(cursor_col, 0);
    assert!(spans.is_empty());
}
