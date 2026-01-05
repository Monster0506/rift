use crate::buffer::TextBuffer;

// Helper to create a buffer
fn create_buffer(text: &str) -> TextBuffer {
    let mut buffer = TextBuffer::new(1024).unwrap();
    buffer.insert_str(text).unwrap();
    buffer.move_to_start();
    buffer
}

#[test]
fn test_move_word_right_basic() {
    let mut buffer = create_buffer("hello world");
    // "hello" -> "world"
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 6); // 'w'

    // "world" -> end
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 11); // end of buffer
}

#[test]
fn test_move_word_right_symbols() {
    let mut buffer = create_buffer("hello, world!");
    // "hello" -> ","
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 5); // ','

    // "," -> " " (skip) -> "world"
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 7); // 'w' of "world"

    // "world" -> "!"
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 12); // '!'
}

#[test]
fn test_move_word_left_basic() {
    let mut buffer = create_buffer("hello world");
    buffer.move_to_end();

    // end -> "world"
    assert!(buffer.move_word_left());
    assert_eq!(buffer.cursor(), 6); // 'w'

    // "world" -> "hello"
    assert!(buffer.move_word_left());
    assert_eq!(buffer.cursor(), 0); // 'h'
}

#[test]
fn test_move_sentence_user_spec() {
    // Spec: "move to next sentence punctuation if any on the current line, else move to the end of the line or next line."
    let mut buffer = create_buffer("Hello world. This is test\nNo dot here\nAnd final line.");

    // Start at 0 ('H')
    // Should move to '.' at index 11? Or after it?
    // Standard vim ')' moves TO the start of the next sentence.
    // User phrasing "move to next sentence punctuation" implies moving TO the dot?
    // Let's assume standard behavior first: Move to start of next sentence (after dot+space).

    // Current impl moves to start of next sentence.
    assert!(buffer.move_sentence_forward());
    // "Hello world. " is 13 chars. 'T' is at 13.
    assert_eq!(buffer.cursor(), 13);

    // "This is test\n" -> No dot. User says "else move to the end of the line or next line".
    // 13 + "This is test".len() (12) = 25.
    // \n is at 25.
    assert!(buffer.move_sentence_forward());
    // If it stops at newline (end of line/next line start):
    assert_eq!(buffer.cursor(), 26);
}

#[test]
fn test_big_word_vs_word() {
    let mut buffer = create_buffer("foo-bar. baz");

    // Normal word: "foo" -> "-" -> "bar" -> "." -> "baz"
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 3); // '-'
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 4); // 'b'
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 7); // '.'
    assert!(buffer.move_word_right());
    assert_eq!(buffer.cursor(), 9); // 'b' of "baz" (skip space)

    // Reset
    buffer.move_to_start();

    // Big word: "foo-bar." -> "baz"
    assert!(buffer.move_big_word_right());
    assert_eq!(buffer.cursor(), 9); // 'b' of "baz" (skip space)
}

#[test]
fn test_big_word_left() {
    let mut buffer = create_buffer("foo-bar. baz");
    buffer.move_to_end();

    // End -> "baz"
    assert!(buffer.move_big_word_left());
    assert_eq!(buffer.cursor(), 9); // 'b'

    // "baz" -> "foo-bar."
    assert!(buffer.move_big_word_left());
    assert_eq!(buffer.cursor(), 0); // 'f'
}
