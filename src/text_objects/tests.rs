use super::*;
use crate::buffer::TextBuffer;

fn buf_from(s: &str) -> TextBuffer {
    let mut buf = TextBuffer::new(64).unwrap();
    for ch in s.chars() {
        buf.insert_char(ch).unwrap();
    }
    buf
}

fn make_spec(modifier: Modifier, kind: ObjectKind) -> TextObjectSpec {
    TextObjectSpec { modifier, kind }
}

fn inner(kind: ObjectKind) -> TextObjectSpec {
    make_spec(Modifier::Inner, kind)
}

fn around(kind: ObjectKind) -> TextObjectSpec {
    make_spec(Modifier::Around, kind)
}

// Helper: resolve with cursor at given position, return (anchor, new_cursor, inclusive).
fn res(spec: TextObjectSpec, s: &str, cursor: usize) -> Option<(usize, usize, bool)> {
    let mut buf = buf_from(s);
    buf.set_cursor(cursor).unwrap();
    resolve(spec, &buf).map(|r| (r.anchor, r.new_cursor, r.inclusive))
}

#[test]
fn inner_word_middle() {
    // "hello world", cursor on 'e' (pos 1) → selects "hello" [0,4]
    let r = res(inner(ObjectKind::Word), "hello world", 1).unwrap();
    assert_eq!(r, (0, 4, true));
}

#[test]
fn inner_word_start() {
    // cursor on 'h' (pos 0) → selects "hello" [0,4]
    let r = res(inner(ObjectKind::Word), "hello world", 0).unwrap();
    assert_eq!(r, (0, 4, true));
}

#[test]
fn inner_word_on_space() {
    // "hello world", cursor on space (pos 5) → selects " " [5,5]
    let r = res(inner(ObjectKind::Word), "hello world", 5).unwrap();
    assert_eq!(r, (5, 5, true));
}

#[test]
fn around_word_eats_trailing_space() {
    // "hello world", cursor on 'l' (pos 2) → "hello " [0,5]
    let r = res(around(ObjectKind::Word), "hello world", 2).unwrap();
    assert_eq!(r, (0, 5, true));
}

#[test]
fn inner_paren() {
    // "(hello)", cursor on 'e' (pos 2) → "hello" [1,5]
    let r = res(inner(ObjectKind::Paren), "(hello)", 2).unwrap();
    assert_eq!(r, (1, 5, true));
}

#[test]
fn around_paren() {
    // "(hello)", cursor on 'e' (pos 2) → "(hello)" [0,6]
    let r = res(around(ObjectKind::Paren), "(hello)", 2).unwrap();
    assert_eq!(r, (0, 6, true));
}

#[test]
fn inner_paren_cursor_on_open() {
    // "(hello)", cursor on '(' (pos 0) → "hello" [1,5]
    let r = res(inner(ObjectKind::Paren), "(hello)", 0).unwrap();
    assert_eq!(r, (1, 5, true));
}

#[test]
fn inner_paren_cursor_on_close() {
    // "(hello)", cursor on ')' (pos 6) → "hello" [1,5]
    let r = res(inner(ObjectKind::Paren), "(hello)", 6).unwrap();
    assert_eq!(r, (1, 5, true));
}

#[test]
fn inner_paren_nested() {
    // "((ab)c)", cursor on 'c' (pos 5) → inner of outer = "(ab)c" [1,5]
    let r = res(inner(ObjectKind::Paren), "((ab)c)", 5).unwrap();
    assert_eq!(r, (1, 5, true));
}

#[test]
fn inner_double_quote() {
    // `"hello"`, cursor on 'e' (pos 2) → "hello" [1,5]
    let r = res(inner(ObjectKind::DoubleQuote), "\"hello\"", 2).unwrap();
    assert_eq!(r, (1, 5, true));
}

#[test]
fn around_double_quote() {
    // `"hello"`, cursor on 'e' (pos 2) → `"hello"` [0,6]
    let r = res(around(ObjectKind::DoubleQuote), "\"hello\"", 2).unwrap();
    assert_eq!(r, (0, 6, true));
}

#[test]
fn inner_line() {
    // "hello\nworld", cursor at pos 0 → "hello" [0,4]
    let r = res(inner(ObjectKind::Line), "hello\nworld", 0).unwrap();
    assert_eq!(r, (0, 4, true));
}

#[test]
fn around_line() {
    // "hello\nworld", cursor at pos 0 → "hello\n" [0,5]
    let r = res(around(ObjectKind::Line), "hello\nworld", 0).unwrap();
    assert_eq!(r, (0, 5, true));
}

#[test]
fn inner_buffer() {
    let r = res(inner(ObjectKind::Buffer), "hello", 0).unwrap();
    assert_eq!(r, (0, 4, true));
}

#[test]
fn empty_paren_inner_is_none() {
    // "()", inner parens = nothing
    assert!(res(inner(ObjectKind::Paren), "()", 0).is_none());
}

#[test]
fn paren_not_inside_is_none() {
    // No parens on the line at all → None
    assert!(res(inner(ObjectKind::Paren), "hello", 2).is_none());
}

#[test]
fn inner_paren_cursor_before_on_same_line() {
    // "ab(cd)" cursor at 'a' (pos 0): not inside any paren, but '(' is ahead
    // on the same line → forward search finds it → selects "cd" [3,4]
    let r = res(inner(ObjectKind::Paren), "ab(cd)", 0).unwrap();
    assert_eq!(r, (3, 4, true));
}

#[test]
fn inner_curly() {
    // "{abc}", cursor at 'a' (pos 1) → "abc" [1,3]
    let r = res(inner(ObjectKind::CurlyBrace), "{abc}", 1).unwrap();
    assert_eq!(r, (1, 3, true));
}

#[test]
fn inner_square() {
    // "[ab]", cursor at 'a' (pos 1) → "ab" [1,2]
    let r = res(inner(ObjectKind::SquareBracket), "[ab]", 1).unwrap();
    assert_eq!(r, (1, 2, true));
}
