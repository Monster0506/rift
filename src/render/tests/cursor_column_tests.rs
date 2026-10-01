use super::common::*;

#[test]
fn test_calculate_cursor_column_single_line() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 5);
}

#[test]
fn test_calculate_cursor_column_multiline() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2\nline3").unwrap();
    for _ in 0..18 {
        buf.move_left();
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);

    buf.move_down();
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 0);

    buf.move_right();
    buf.move_right();
    buf.move_right();
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 3);
}

#[test]
fn test_calculate_cursor_column_empty_buffer() {
    let buf = TextBuffer::new(100).unwrap();
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);
}

#[test]
fn test_calculate_cursor_column_at_gap() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    for _ in 0..3 {
        buf.move_left();
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 2);
}

#[test]
fn test_calculate_cursor_column_multiline_complex() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello\nworld\ntest").unwrap();

    for _ in 0..17 {
        buf.move_left();
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);

    for _ in 0..5 {
        buf.move_right();
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 5);

    buf.move_right(); // Move past newline
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 0);

    for _ in 0..3 {
        buf.move_right();
    }
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 3);
}

#[test]
fn test_cursor_column_wide_chars() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("a中b").unwrap();

    assert_eq!(calculate_cursor_column(&buf, 0, 4), 4);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 3);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 1);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 0);
}

#[test]
fn test_cursor_column_combining_chars() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("e\u{0301}").unwrap();

    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 1);
    buf.move_left();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 0);
}

#[test]
fn test_cursor_column_truncated_utf8() {
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_bytes(&[0xE2, 0x80]).unwrap();
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 8);
}

#[test]
fn test_wrap_text_cjk_counts_as_two_columns() {
    use crate::render::wrap_text;

    let lines = wrap_text("你好", 4);
    assert_eq!(
        lines.len(),
        1,
        "2 CJK chars (display width 4) should fit in width 4"
    );
    assert_eq!(lines[0], "你好");

    let lines = wrap_text("AB 你好", 5);
    assert_eq!(
        lines.len(),
        2,
        "\"AB 你好\" should wrap at width 5 (display widths: 2+1+4=7 > 5)"
    );
    assert_eq!(lines[0], "AB");
    assert_eq!(lines[1], "你好");
}

#[test]
fn test_cursor_column_at_matches_plain() {
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("hello").unwrap();
    assert_eq!(
        calculate_cursor_column_at(&buf, 0, 4, buf.cursor()),
        calculate_cursor_column(&buf, 0, 4),
    );
}

#[test]
fn test_cursor_column_at_mid_text() {
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("abcde").unwrap();
    let _ = buf.set_cursor(3);

    assert_eq!(calculate_cursor_column_at(&buf, 0, 4, buf.cursor()), 3,);
}
