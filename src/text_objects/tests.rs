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
    TextObjectSpec {
        modifier,
        direction: Direction::Current,
        nesting: 1,
        kind,
    }
}

fn inner(kind: ObjectKind) -> TextObjectSpec {
    make_spec(Modifier::Inner, kind)
}

fn around(kind: ObjectKind) -> TextObjectSpec {
    make_spec(Modifier::Around, kind)
}

// Helper: resolve with cursor at given position, return (anchor, new_cursor, inclusive).
fn res(spec: TextObjectSpec, s: &str, cursor: usize) -> Option<(usize, usize, bool)> {
    res_count(spec, s, cursor, 1)
}

fn res_count(
    spec: TextObjectSpec,
    s: &str,
    cursor: usize,
    count: usize,
) -> Option<(usize, usize, bool)> {
    let mut buf = buf_from(s);
    buf.set_cursor(cursor).unwrap();
    resolve(spec, &buf, count).map(|r| (r.anchor, r.new_cursor, r.inclusive))
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

// Phase 2: I/A modifiers, AnyBracket/AnyQuote, direction, nest-count.

#[test]
fn inner_strict_trims_inner_whitespace() {
    // "(  ab  )", cursor on 'a' (pos 3) → InnerStrict trims to "ab" [3,4]
    let r = res(
        make_spec(Modifier::InnerStrict, ObjectKind::Paren),
        "(  ab  )",
        3,
    )
    .unwrap();
    assert_eq!(r, (3, 4, true));
}

#[test]
fn around_loose_eats_trailing_whitespace_outside() {
    // "(ab)  cd", cursor on 'a' (pos 1) → AroundLoose eats trailing spaces too
    let r = res(
        make_spec(Modifier::AroundLoose, ObjectKind::Paren),
        "(ab)  cd",
        1,
    )
    .unwrap();
    assert_eq!(r, (0, 5, true));
}

#[test]
fn any_bracket_matches_nearest_type() {
    // "[ab]", cursor at 'a' (pos 1) → AnyBracket finds the square brackets
    let r = res(inner(ObjectKind::AnyBracket), "[ab]", 1).unwrap();
    assert_eq!(r, (1, 2, true));
}

#[test]
fn any_quote_matches_nearest_type() {
    // "'ab'", cursor at 'a' (pos 1) → AnyQuote finds the single quotes
    let r = res(inner(ObjectKind::AnyQuote), "'ab'", 1).unwrap();
    assert_eq!(r, (1, 2, true));
}

#[test]
fn direction_next_finds_forward_pair_past_unrelated_text() {
    // "ab (cd) ef (gh)", cursor at 'e' (pos 9, outside both parens):
    // current-line scan would hit the first "(cd)"; Next must skip to "(gh)".
    let spec = TextObjectSpec {
        modifier: Modifier::Inner,
        direction: Direction::Next,
        nesting: 1,
        kind: ObjectKind::Paren,
    };
    let r = res(spec, "ab (cd) ef (gh)", 9).unwrap();
    assert_eq!(r, (12, 13, true));
}

#[test]
fn direction_last_finds_backward_pair() {
    // "(ab) cd (ef)", cursor at 'd' (pos 6): Last must find the first "(ab)".
    let spec = TextObjectSpec {
        modifier: Modifier::Inner,
        direction: Direction::Last,
        nesting: 1,
        kind: ObjectKind::Paren,
    };
    let r = res(spec, "(ab) cd (ef)", 6).unwrap();
    assert_eq!(r, (1, 2, true));
}

#[test]
fn nest_count_selects_grandparent() {
    // "((ab)c)", cursor at 'a' (pos 2): nesting=2 selects the outer parens'
    // content "(ab)c" [1,5].
    let spec = TextObjectSpec {
        modifier: Modifier::Inner,
        direction: Direction::Current,
        nesting: 2,
        kind: ObjectKind::Paren,
    };
    let r = res(spec, "((ab)c)", 2).unwrap();
    assert_eq!(r, (1, 5, true));
}

#[test]
fn leading_count_composes_with_nest_count_for_brackets() {
    // "((((ab))))", cursor at 'a' (pos 4): a leading count of 2 composed
    // with a typed nest-count of 2 (2di2() reaches composed nesting 4, the
    // outermost pair.
    let spec = TextObjectSpec {
        modifier: Modifier::Inner,
        direction: Direction::Current,
        nesting: 2,
        kind: ObjectKind::Paren,
    };
    let r = res_count(spec, "((((ab))))", 4, 2).unwrap();
    assert_eq!(r, (1, 8, true));
}

#[test]
fn leading_count_extends_inner_word_across_n_words() {
    // "foo bar baz", cursor on 'f' (pos 0): 2iw selects "foo bar" [0,6].
    let r = res_count(inner(ObjectKind::Word), "foo bar baz", 0, 2).unwrap();
    assert_eq!(r, (0, 6, true));
}

#[test]
fn leading_count_extends_around_word_across_n_words_with_trailing_space() {
    // "foo bar baz", cursor on 'f' (pos 0): 2aw selects "foo bar " [0,7].
    let r = res_count(around(ObjectKind::Word), "foo bar baz", 0, 2).unwrap();
    assert_eq!(r, (0, 7, true));
}

#[test]
fn leading_count_extends_sentence_across_n_sentences() {
    // "One. Two. Three.", cursor at 'O' (pos 0): 2is selects "One. Two" [0,7].
    let r = res_count(inner(ObjectKind::Sentence), "One. Two. Three.", 0, 2).unwrap();
    assert_eq!(r, (0, 7, true));
}

#[test]
fn leading_count_extends_paragraph_across_n_groups() {
    // Two single-line paragraphs separated by a blank line; a blank-line run
    // counts as its own group, so 3ip from the first line is needed to reach
    // into "second" (group 1 = "first", group 2 = blank line, group 3 = "second").
    let r = res_count(inner(ObjectKind::Paragraph), "first\n\nsecond\n", 0, 3).unwrap();
    let buf = buf_from("first\n\nsecond\n");
    let second_line_start = buf.line_index.get_start(2).unwrap();
    assert_eq!(r.0, 0);
    assert!(r.1 >= second_line_start);
}
