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
fn test_move_sentence() {
    let mut buffer = create_buffer("Hello world. This is test\nNo dot here\nAnd final line.");

    assert!(buffer.move_sentence_forward());
    assert_eq!(buffer.cursor(), 13);

    assert!(buffer.move_sentence_forward());
    assert_eq!(buffer.cursor(), 26);
}

#[test]
fn test_word_left_symbols() {
    let mut buffer = create_buffer("foo-bar. baz");
    buffer.move_to_end();

    assert!(buffer.move_word_left());
    assert_eq!(buffer.cursor(), 9); // 'b'

    assert!(buffer.move_word_left());
    assert_eq!(buffer.cursor(), 7); // '.'

    // "." -> "bar"
    assert!(buffer.move_word_left());
    assert_eq!(buffer.cursor(), 4); // 'b'
}
