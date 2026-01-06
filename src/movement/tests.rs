use super::boundaries::*;
use super::buffer::*;
use super::classify::*;
use crate::buffer::TextBuffer;

#[test]
fn test_next_word_basic() {
    let text = "hello world";
    assert_eq!(next_word(text, 0), 6); // "hello " -> "world"
    assert_eq!(next_word(text, 6), 11); // "world" -> end
}

#[test]
fn test_next_word_symbols() {
    let text = "foo->bar";
    assert_eq!(next_word(text, 0), 3); // "foo" -> "->"
    assert_eq!(next_word(text, 3), 5); // "->" -> "bar"
    assert_eq!(next_word(text, 5), 8); // "bar" -> end
}

#[test]
fn test_next_word_underscore() {
    let text = "hello_world";
    assert_eq!(next_word(text, 0), 11); // "hello_world" -> end (one word)
}

#[test]
fn test_next_word_multiple_spaces() {
    let text = "hello    world";
    assert_eq!(next_word(text, 0), 9); // "hello    " -> "world"
}

#[test]
fn test_prev_word_basic() {
    let text = "hello world";
    assert_eq!(prev_word(text, 11), 6); // end -> "world"
    assert_eq!(prev_word(text, 6), 0); // "world" -> "hello"
}

#[test]
fn test_prev_word_symbols() {
    let text = "foo->bar";
    assert_eq!(prev_word(text, 8), 5); // end -> "bar"
    assert_eq!(prev_word(text, 5), 3); // "bar" -> "->"
    assert_eq!(prev_word(text, 3), 0); // "->" -> "foo"
}

#[test]
fn test_prev_word_underscore() {
    let text = "hello_world";
    assert_eq!(prev_word(text, 11), 0); // end -> start (one word)
}

#[test]
fn test_prev_word_multiple_spaces() {
    let text = "hello    world";
    assert_eq!(prev_word(text, 14), 9); // end -> "world"
    assert_eq!(prev_word(text, 9), 0); // "world" -> "hello"
}

#[test]
fn test_edge_cases() {
    assert_eq!(next_word("", 0), 0);
    assert_eq!(prev_word("", 0), 0);
    assert_eq!(next_word("a", 1), 1);
    assert_eq!(prev_word("a", 0), 0);
}

#[test]
fn test_buffer_word_right_basic() {
    let mut buffer = TextBuffer::new(0).unwrap();
    buffer.insert_str("hello world").unwrap();
    buffer.move_to_start();

    assert!(move_word_right(&mut buffer));
    assert_eq!(buffer.cursor(), 6);

    assert!(move_word_right(&mut buffer));
    assert_eq!(buffer.cursor(), 11);

    assert!(!move_word_right(&mut buffer));
}

#[test]
fn test_buffer_word_right_symbols() {
    let mut buffer = TextBuffer::new(0).unwrap();
    buffer.insert_str("foo->bar").unwrap();
    buffer.move_to_start();

    assert!(move_word_right(&mut buffer));
    assert_eq!(buffer.cursor(), 3); // "foo" -> "->"

    assert!(move_word_right(&mut buffer));
    assert_eq!(buffer.cursor(), 5); // "->" -> "bar"

    assert!(move_word_right(&mut buffer));
    assert_eq!(buffer.cursor(), 8); // "bar" -> end
}

#[test]
fn test_buffer_word_left_basic() {
    let mut buffer = TextBuffer::new(0).unwrap();
    buffer.insert_str("hello world").unwrap();
    assert_eq!(buffer.cursor(), 11); // After insert, cursor is at end

    assert!(move_word_left(&mut buffer));
    assert_eq!(buffer.cursor(), 6);

    assert!(move_word_left(&mut buffer));
    assert_eq!(buffer.cursor(), 0);

    assert!(!move_word_left(&mut buffer));
}

#[test]
fn test_buffer_word_left_symbols() {
    let mut buffer = TextBuffer::new(0).unwrap();
    buffer.insert_str("foo->bar").unwrap();
    assert_eq!(buffer.cursor(), 8); // After insert, cursor is at end

    assert!(move_word_left(&mut buffer));
    assert_eq!(buffer.cursor(), 5); // end -> "bar"

    assert!(move_word_left(&mut buffer));
    assert_eq!(buffer.cursor(), 3); // "bar" -> "->"

    assert!(move_word_left(&mut buffer));
    assert_eq!(buffer.cursor(), 0); // "->" -> "foo"
}

#[test]
fn test_buffer_word_underscore() {
    let mut buffer = TextBuffer::new(0).unwrap();
    buffer.insert_str("hello_world").unwrap();
    buffer.move_to_start();

    assert!(move_word_right(&mut buffer));
    assert_eq!(buffer.cursor(), 11); // One word
}

#[test]
fn test_buffer_empty() {
    let mut buffer = TextBuffer::new(0).unwrap();
    assert!(!move_word_right(&mut buffer));
    assert!(!move_word_left(&mut buffer));
}

#[test]
fn test_classify_char() {
    assert_eq!(classify_char(' '), CharClass::Whitespace);
    assert_eq!(classify_char('\t'), CharClass::Whitespace);
    assert_eq!(classify_char('\n'), CharClass::Whitespace);

    assert_eq!(classify_char('a'), CharClass::Alphanumeric);
    assert_eq!(classify_char('Z'), CharClass::Alphanumeric);
    assert_eq!(classify_char('5'), CharClass::Alphanumeric);
    assert_eq!(classify_char('_'), CharClass::Alphanumeric);

    assert_eq!(classify_char('-'), CharClass::Symbol);
    assert_eq!(classify_char('>'), CharClass::Symbol);
    assert_eq!(classify_char('('), CharClass::Symbol);
    assert_eq!(classify_char('.'), CharClass::Symbol);
}

#[test]
fn test_is_word_char() {
    assert!(!is_word_char(' '));
    assert!(is_word_char('a'));
    assert!(is_word_char('_'));
    assert!(is_word_char('-'));
}

#[test]
fn test_is_sentence_end() {
    assert!(is_sentence_end('.'));
    assert!(is_sentence_end('!'));
    assert!(is_sentence_end('?'));
    assert!(!is_sentence_end(','));
    assert!(!is_sentence_end(' '));
}

#[test]
fn test_is_paragraph_boundary() {
    assert!(is_paragraph_boundary(""));
    assert!(is_paragraph_boundary("   "));
    assert!(is_paragraph_boundary("\t\t"));
    assert!(!is_paragraph_boundary("hello"));
    assert!(!is_paragraph_boundary("  hello  "));
}
