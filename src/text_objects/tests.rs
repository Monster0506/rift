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
    resolve(spec, &buf, count, None).map(|r| (r.anchor, r.new_cursor, r.inclusive))
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
fn inner_quote_cursor_before_quotes_on_line_forward_fallback() {
    // `let x = "abc"`, cursor on 'x' (pos 4), before any quote on the line.
    // Real vim selects the next quoted string on the line ("abc").
    let r = res(inner(ObjectKind::DoubleQuote), "let x = \"abc\"", 4).unwrap();
    assert_eq!(r, (9, 11, true));
}

#[test]
fn inner_quote_cursor_on_closing_quote_pairs_with_preceding_opener() {
    // `"a" "b"`, cursor on the closing quote of "a" (pos 2).
    // Must select "a" [1,1], not the gap between the two strings.
    let r = res(inner(ObjectKind::DoubleQuote), "\"a\" \"b\"", 2).unwrap();
    assert_eq!(r, (1, 1, true));
}

#[test]
fn inner_quote_escaped_backslash_before_real_delimiter() {
    // `"a\\"`, the content is a\ followed by a real closing quote: the
    // backslash before the final `"` is itself escaped (\\), so the quote is
    // NOT escaped and IS a real delimiter.
    let r = res(inner(ObjectKind::DoubleQuote), "\"a\\\\\"", 2).unwrap();
    assert_eq!(r, (1, 3, true));
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
fn inner_sentence_cursor_on_terminator_selects_sentence_it_closes() {
    // "One. Two.", cursor on the '.' that ends "One" (pos 3): selects the
    // sentence the terminator closes ("One" [0,2], consistent with this
    // resolver's convention of excluding the final terminator -- see the
    // count=2 case below), not the next sentence, and must not no-op.
    let r = res(inner(ObjectKind::Sentence), "One. Two.", 3).unwrap();
    assert_eq!(r, (0, 2, true));
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

// Phase 3: tree-sitter backed objects.

#[cfg(feature = "treesitter")]
mod treesitter_tests {
    use super::*;
    use crate::text_objects::SyntaxContext;

    fn rust_tree(src: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(src, None).unwrap()
    }

    fn html_tree(src: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_html::LANGUAGE.into())
            .unwrap();
        parser.parse(src, None).unwrap()
    }

    fn res_ts(
        spec: TextObjectSpec,
        tree: &tree_sitter::Tree,
        src: &str,
        cursor: usize,
    ) -> Option<(usize, usize, bool)> {
        let mut buf = buf_from(src);
        buf.set_cursor(cursor).unwrap();
        let ctx = SyntaxContext {
            tree,
            source: src.as_bytes(),
        };
        resolve(spec, &buf, 1, Some(ctx)).map(|r| (r.anchor, r.new_cursor, r.inclusive))
    }

    #[test]
    fn no_syntax_context_is_none() {
        let r = res(inner(ObjectKind::FunctionCall), "foo(a, b)", 0);
        assert!(r.is_none());
    }

    #[test]
    fn inner_function_call_selects_args_without_parens() {
        let src = "foo(a, b)";
        let tree = rust_tree(src);
        let r = res_ts(inner(ObjectKind::FunctionCall), &tree, src, 0).unwrap();
        assert_eq!(r, (4, 7, true)); // "a, b"
    }

    #[test]
    fn around_function_call_selects_args_with_parens() {
        let src = "foo(a, b)";
        let tree = rust_tree(src);
        let r = res_ts(around(ObjectKind::FunctionCall), &tree, src, 0).unwrap();
        assert_eq!(r, (3, 8, true)); // "(a, b)"
    }

    #[test]
    fn inner_argument_selects_single_arg() {
        let src = "foo(a, b)";
        let tree = rust_tree(src);
        let r = res_ts(inner(ObjectKind::Argument), &tree, src, 7).unwrap();
        assert_eq!(r, (7, 7, true)); // "b"
    }

    #[test]
    fn around_first_argument_eats_trailing_comma() {
        let src = "foo(a, b)";
        let tree = rust_tree(src);
        let r = res_ts(around(ObjectKind::Argument), &tree, src, 4).unwrap();
        assert_eq!(r, (4, 6, true)); // "a, "
    }

    #[test]
    fn around_last_argument_eats_leading_comma() {
        let src = "foo(a, b)";
        let tree = rust_tree(src);
        let r = res_ts(around(ObjectKind::Argument), &tree, src, 7).unwrap();
        assert_eq!(r, (5, 7, true)); // ", b"
    }

    #[test]
    fn inner_function_def_selects_body_without_braces() {
        let src = "fn outer() {\n    foo();\n}\n";
        let tree = rust_tree(src);
        let r = res_ts(inner(ObjectKind::FunctionDef), &tree, src, 18).unwrap();
        assert_eq!(&src[r.0..=r.1], "\n    foo();\n");
    }

    #[test]
    fn around_function_def_selects_whole_item() {
        let src = "fn outer() {\n    foo();\n}\n";
        let tree = rust_tree(src);
        let r = res_ts(around(ObjectKind::FunctionDef), &tree, src, 18).unwrap();
        assert_eq!(&src[r.0..=r.1], "fn outer() {\n    foo();\n}");
    }

    #[test]
    fn inner_class_selects_struct_body() {
        let src = "struct Point { x: i32 }\n";
        let tree = rust_tree(src);
        let r = res_ts(inner(ObjectKind::Class), &tree, src, 16).unwrap();
        assert_eq!(&src[r.0..=r.1], " x: i32 ");
    }

    #[test]
    fn inner_block_selects_nearest_enclosing_block() {
        let src = "fn outer() {\n    let n = 1;\n}\n";
        let tree = rust_tree(src);
        let r = res_ts(inner(ObjectKind::Block), &tree, src, 18).unwrap();
        assert_eq!(&src[r.0..=r.1], "\n    let n = 1;\n");
    }

    #[test]
    fn nest_count_walks_to_outer_block() {
        let src = "fn outer() {\n    if true {\n        foo();\n    }\n}\n";
        let tree = rust_tree(src);
        let spec = TextObjectSpec {
            modifier: Modifier::Inner,
            direction: Direction::Current,
            nesting: 2,
            kind: ObjectKind::Block,
        };
        let inner_cursor = src.find("foo()").unwrap();
        let r = res_ts(spec, &tree, src, inner_cursor).unwrap();
        assert_eq!(&src[r.0..=r.1], "\n    if true {\n        foo();\n    }\n");
    }

    #[test]
    fn inner_number_selects_integer_literal() {
        let src = "let n = 42;";
        let tree = rust_tree(src);
        let r = res_ts(inner(ObjectKind::Number), &tree, src, 9).unwrap();
        assert_eq!(&src[r.0..=r.1], "42");
    }

    #[test]
    fn inner_tag_selects_content_between_start_and_end() {
        let src = "<div><p>hello</p></div>";
        let tree = html_tree(src);
        let r = res_ts(inner(ObjectKind::Tag), &tree, src, 9).unwrap();
        assert_eq!(&src[r.0..=r.1], "hello");
    }

    #[test]
    fn around_tag_selects_whole_element() {
        let src = "<div><p>hello</p></div>";
        let tree = html_tree(src);
        let r = res_ts(around(ObjectKind::Tag), &tree, src, 9).unwrap();
        assert_eq!(&src[r.0..=r.1], "<p>hello</p>");
    }

    #[test]
    fn around_tag_count_exceeding_nesting_depth_clamps_to_outermost() {
        // Only 2 tag levels exist (div, p); a count of 3 must clamp to the
        // outermost ancestor (div) instead of no-op'ing.
        let src = "<div><p>hello</p></div>";
        let tree = html_tree(src);
        let spec = TextObjectSpec {
            modifier: Modifier::Around,
            direction: Direction::Current,
            nesting: 3,
            kind: ObjectKind::Tag,
        };
        let r = res_ts(spec, &tree, src, 9).unwrap();
        assert_eq!(&src[r.0..=r.1], "<div><p>hello</p></div>");
    }
}
