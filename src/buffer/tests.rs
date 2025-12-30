use super::*;

#[test]
fn test_new_buffer() {
    let buffer = TextBuffer::new(10).unwrap();
    assert!(buffer.is_empty());
    assert_eq!(buffer.len(), 0);
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(buffer.get_total_lines(), 1);
}

#[test]
fn test_insert_char() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_char('a').unwrap();
    buffer.insert_char('b').unwrap();
    buffer.insert_char('c').unwrap();

    assert_eq!(buffer.to_string(), "abc");
    assert_eq!(buffer.cursor(), 3);
    assert_eq!(buffer.len(), 3);
}

#[test]
fn test_insert_str() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("Hello").unwrap();
    buffer.insert_str(" World").unwrap();

    assert_eq!(buffer.to_string(), "Hello World");
    assert_eq!(buffer.cursor(), 11);
}

#[test]
fn test_move_cursor() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("123").unwrap();

    // Start at end
    assert_eq!(buffer.cursor(), 3);

    // Move left
    assert!(buffer.move_left());
    assert_eq!(buffer.cursor(), 2);
    assert!(buffer.move_left());
    assert_eq!(buffer.cursor(), 1);
    assert!(buffer.move_left());
    assert_eq!(buffer.cursor(), 0);
    assert!(!buffer.move_left()); // Can't move past start

    // Move right
    assert!(buffer.move_right());
    assert_eq!(buffer.cursor(), 1);
    assert!(buffer.move_right());
    assert_eq!(buffer.cursor(), 2);
    assert!(buffer.move_right());
    assert_eq!(buffer.cursor(), 3);
    assert!(!buffer.move_right()); // Can't move past end
}

#[test]
fn test_delete_backward() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("abc").unwrap();

    assert!(buffer.delete_backward()); // Delete 'c'
    assert_eq!(buffer.to_string(), "ab");
    assert_eq!(buffer.cursor(), 2);

    buffer.move_left(); // Cursor at 'b'
    assert!(buffer.delete_backward()); // Delete 'a'
    assert_eq!(buffer.to_string(), "b");
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn test_delete_forward() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("abc").unwrap();
    buffer.move_to_start();

    assert!(buffer.delete_forward()); // Delete 'a'
    assert_eq!(buffer.to_string(), "bc");
    assert_eq!(buffer.cursor(), 0);

    buffer.move_right(); // Cursor after 'b'
    assert!(buffer.delete_forward()); // Delete 'c'
    assert_eq!(buffer.to_string(), "b");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn test_lines() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("Line 1\nLine 2\nLine 3").unwrap();

    assert_eq!(buffer.get_total_lines(), 3);

    // Check line contents
    assert_eq!(buffer.get_line_bytes(0), b"Line 1");
    assert_eq!(buffer.get_line_bytes(1), b"Line 2");
    assert_eq!(buffer.get_line_bytes(2), b"Line 3");

    // Check cursor line tracking
    buffer.move_to_start();
    assert_eq!(buffer.get_line(), 0);

    buffer.move_down();
    assert_eq!(buffer.get_line(), 1);

    buffer.move_down();
    assert_eq!(buffer.get_line(), 2);
}

#[test]
fn test_move_up_down() {
    let mut buffer = TextBuffer::new(10).unwrap();
    // "012\n456\n89"
    buffer.insert_str("012\n456\n89").unwrap();

    // Start at end (after '9', line 2, col 2)
    assert_eq!(buffer.get_line(), 2);

    // Move up to line 1
    assert!(buffer.move_up());
    assert_eq!(buffer.get_line(), 1);
    assert_eq!(buffer.get_after_gap()[0], b'6');

    // Cursor was at 10.
    // Move up:
    // Prev line (1) start: 4. End: 7.
    // Col: 2 (since line 2 start is 8, cursor 10 -> col 2).
    // Target: min(4+2, 7) = 6.
    // Index 6 is '6'.
    assert_eq!(buffer.cursor(), 6);

    // Move up to line 0
    assert!(buffer.move_up());
    assert_eq!(buffer.get_line(), 0);
    // Prev line (0) start: 0. End: 3.
    // Col: 2 (from previous step, cursor 6 - line start 4 = 2).
    // Target: min(0+2, 3) = 2.
    // Index 2 is '2'.
    assert_eq!(buffer.cursor(), 2);

    // Move down
    assert!(buffer.move_down());
    assert_eq!(buffer.get_line(), 1);
    assert_eq!(buffer.cursor(), 6);
}

#[test]
fn test_line_start_end() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("Hello World").unwrap();
    buffer.move_left(); // At 'd'
    buffer.move_left(); // At 'l'

    buffer.move_to_line_start();
    assert_eq!(buffer.cursor(), 0);

    buffer.move_to_line_end();
    assert_eq!(buffer.cursor(), 11);
}

#[test]
fn test_utf8_movement() {
    let mut buffer = TextBuffer::new(10).unwrap();
    // "a" (1 byte), "€" (3 bytes), "b" (1 byte)
    buffer.insert_str("a€b").unwrap();

    buffer.move_to_start();
    assert_eq!(buffer.cursor(), 0);

    buffer.move_right(); // Skip 'a'
    assert_eq!(buffer.cursor(), 1);

    buffer.move_right(); // Skip '€' (3 bytes)
    assert_eq!(buffer.cursor(), 4);

    buffer.move_right(); // Skip 'b'
    assert_eq!(buffer.cursor(), 5);

    buffer.move_left(); // Back over 'b'
    assert_eq!(buffer.cursor(), 4);

    buffer.move_left(); // Back over '€'
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn test_get_before_after_gap() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("Hello World").unwrap();
    buffer.move_to_start();
    for _ in 0..5 {
        buffer.move_right();
    }
    // Cursor at 5 ("Hello| World")

    let before = buffer.get_before_gap();
    assert_eq!(String::from_utf8(before).unwrap(), "Hello");

    let after = buffer.get_after_gap();
    assert_eq!(String::from_utf8(after).unwrap(), " World");
}

#[test]
fn test_move_word_right() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("hello world  test").unwrap();
    buffer.move_to_start();

    // "hello" -> " world"
    assert!(buffer.move_word_right());
    // "hello " is 6 chars. "world" starts at 6.
    assert_eq!(buffer.cursor(), 6);

    // "world" -> "  test"
    assert!(buffer.move_word_right());
    // "hello world  " is 6 + 5 + 2 = 13 chars. "test" starts at 13.
    assert_eq!(buffer.cursor(), 13);

    // "test" -> end
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 17);

    // end -> false
    assert!(!buffer.move_word_right());
}

#[test]
fn test_move_word_left() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("hello world").unwrap();

    // Start at end (11)

    // "world" -> "world" (start)
    assert!(buffer.move_word_left());
    assert_eq!(buffer.cursor(), 6); // Start of "world"

    // "world" -> "hello" (start)
    assert!(buffer.move_word_left());
    assert_eq!(buffer.cursor(), 0); // Start of "hello"

    // start -> false
    assert!(!buffer.move_word_left());
}

#[test]
fn test_move_paragraph() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("P1\n\nP2\n\nP3").unwrap();
    buffer.move_to_start();

    // P1 -> empty line
    assert!(buffer.move_paragraph_forward());
    assert_eq!(buffer.get_line(), 1);

    // empty line -> next empty line
    assert!(buffer.move_paragraph_forward());
    assert_eq!(buffer.get_line(), 3);

    // next empty line -> end
    assert!(buffer.move_paragraph_forward());
    assert_eq!(buffer.cursor(), buffer.len());

    // Backward
    assert!(buffer.move_paragraph_backward());
    assert_eq!(buffer.get_line(), 3);

    assert!(buffer.move_paragraph_backward());
    assert_eq!(buffer.get_line(), 1);

    assert!(buffer.move_paragraph_backward());
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn test_move_sentence_forward() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer
        .insert_str("Hello world. This is a test! And another one? Yes.")
        .unwrap();
    buffer.move_to_start();

    // "Hello world. " -> "This is a test! "
    assert!(buffer.move_sentence_forward());
    // "Hello world. " is 13 chars.
    assert_eq!(buffer.cursor(), 13);

    // "This is a test! " -> "And another one? "
    assert!(buffer.move_sentence_forward());
    // "This is a test! " is 16 chars. 13 + 16 = 29.
    assert_eq!(buffer.cursor(), 29);

    // "And another one? " -> "Yes."
    assert!(buffer.move_sentence_forward());
    // "And another one? " is 17 chars. 29 + 17 = 46.
    assert_eq!(buffer.cursor(), 46);

    // "Yes." -> end
    assert!(buffer.move_sentence_forward());
    assert_eq!(buffer.cursor(), buffer.len());
}

#[test]
fn test_move_sentence_backward() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer.insert_str("One. Two. Three.").unwrap();
    buffer.move_to_end();

    // End -> "Three."
    assert!(buffer.move_sentence_backward());
    // "One. Two. " is 5 + 5 = 10.
    assert_eq!(buffer.cursor(), 10);

    // "Three." -> "Two."
    assert!(buffer.move_sentence_backward());
    // "One. " is 5.
    assert_eq!(buffer.cursor(), 5);

    // "Two." -> "One."
    assert!(buffer.move_sentence_backward());
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn test_move_sentence_forward_multiline() {
    let mut buffer = TextBuffer::new(10).unwrap();
    buffer
        .insert_str("Line 1 no dot\nLine 2 with dot.\nLine 3")
        .unwrap();
    buffer.move_to_start();

    // "Line 1 no dot" is 13 chars. '\n' is at 13.
    // Should stop at newline if no dot found
    assert!(buffer.move_sentence_forward());
    assert_eq!(buffer.cursor(), 13); // At '\n'

    // Should move past newline and find sentence end on next line
    assert!(buffer.move_sentence_forward());
    // "Line 2 with dot." is 16 chars.
    // 14 (start of line 2) + 16 = 30.
    // Dot is at 29. Next char is '\n' (at 30).
    // Skips whitespace (newline).
    // Should end up at start of Line 3 (31).
    assert_eq!(buffer.cursor(), 31);
}
