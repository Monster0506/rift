use super::common::*;

#[test]
fn test_calculate_cursor_column_single_line() {
    let mut buf = TextBuffer::new(100).unwrap();
    crate::document::ops::Edit::type_text("hello").apply_to(&mut buf);
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 5);
}

#[test]
fn test_calculate_cursor_column_multiline() {
    let mut buf = TextBuffer::new(100).unwrap();
    crate::document::ops::Edit::type_text("line1\nline2\nline3").apply_to(&mut buf);
    for _ in 0..18 {
        crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);

    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Down);
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 0);

    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Right);
    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Right);
    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Right);
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
    crate::document::ops::Edit::type_text("hello").apply_to(&mut buf);
    for _ in 0..3 {
        crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 2);
}

#[test]
fn test_calculate_cursor_column_multiline_complex() {
    let mut buf = TextBuffer::new(100).unwrap();
    crate::document::ops::Edit::type_text("hello\nworld\ntest").apply_to(&mut buf);

    for _ in 0..17 {
        crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 0);

    for _ in 0..5 {
        crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Right);
    }
    assert_eq!(calculate_cursor_column(&buf, 0, 8), 5);

    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Right); // Move past newline
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 0);

    for _ in 0..3 {
        crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Right);
    }
    assert_eq!(calculate_cursor_column(&buf, 1, 8), 3);
}

#[test]
fn test_cursor_column_wide_chars() {
    let mut buf = TextBuffer::new(100).unwrap();
    crate::document::ops::Edit::type_text("a中b").apply_to(&mut buf);

    assert_eq!(calculate_cursor_column(&buf, 0, 4), 4);
    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 3);
    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 1);
    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 0);
}

#[test]
fn test_cursor_column_combining_chars() {
    let mut buf = TextBuffer::new(100).unwrap();
    crate::document::ops::Edit::type_text("e\u{0301}").apply_to(&mut buf);

    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 1);
    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::Left);
    assert_eq!(calculate_cursor_column(&buf, 0, 4), 0);
}

#[test]
fn test_cursor_column_truncated_utf8() {
    let mut buf = TextBuffer::new(100).unwrap();
    crate::document::ops::Edit::type_text(crate::buffer::decode_bytes(&[0xE2, 0x80]))
        .apply_to(&mut buf);
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
    crate::document::ops::Edit::type_text("hello").apply_to(&mut buf);
    assert_eq!(
        calculate_cursor_column_at(&buf, 0, 4, buf.cursor()),
        calculate_cursor_column(&buf, 0, 4),
    );
}

#[test]
fn test_cursor_column_at_mid_text() {
    let mut buf = TextBuffer::new(64).unwrap();
    crate::document::ops::Edit::type_text("abcde").apply_to(&mut buf);
    let _ = crate::document::ops::standalone::place_cursor(&mut buf, 3);

    assert_eq!(calculate_cursor_column_at(&buf, 0, 4, buf.cursor()), 3,);
}
