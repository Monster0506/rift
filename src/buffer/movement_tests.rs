use crate::action::Motion;
use crate::buffer::TextBuffer;
use crate::wrap::OperatorContext;

// Helper to create a buffer
fn create_buffer(text: &str) -> TextBuffer {
    let mut buffer = TextBuffer::new(1024).unwrap();
    buffer.insert_str(text).unwrap();
    buffer.move_to_start();
    buffer
}

fn apply_motion(motion: Motion, buf: &mut TextBuffer) {
    motion.apply(buf, None, OperatorContext::Move, 4, 20, None);
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

#[test]
fn test_find_char_forward_basic() {
    let mut buf = create_buffer("hello world");
    apply_motion(Motion::FindCharForward('l'), &mut buf);
    assert_eq!(buf.cursor(), 2);
}

#[test]
fn test_find_char_forward_skips_current_position() {
    let mut buf = create_buffer("hello world");
    buf.set_cursor(2).unwrap();
    apply_motion(Motion::FindCharForward('l'), &mut buf);
    assert_eq!(buf.cursor(), 3);
}

#[test]
fn test_find_char_forward_not_found_stays_put() {
    let mut buf = create_buffer("hello");
    apply_motion(Motion::FindCharForward('z'), &mut buf);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_find_char_forward_does_not_cross_line_boundary() {
    let mut buf = create_buffer("hello\nworld");
    apply_motion(Motion::FindCharForward('o'), &mut buf);
    assert_eq!(buf.cursor(), 4);
}

#[test]
fn test_find_char_forward_last_char_on_line() {
    let mut buf = create_buffer("hello\nworld");
    apply_motion(Motion::FindCharForward('o'), &mut buf);
    assert_eq!(buf.cursor(), 4);
}

#[test]
fn test_find_char_backward_basic() {
    let mut buf = create_buffer("hello world");
    buf.set_cursor(10).unwrap();
    apply_motion(Motion::FindCharBackward('o'), &mut buf);
    assert_eq!(buf.cursor(), 7);
}

#[test]
fn test_find_char_backward_skips_current_position() {
    let mut buf = create_buffer("hello world");
    buf.set_cursor(7).unwrap();
    apply_motion(Motion::FindCharBackward('o'), &mut buf);
    assert_eq!(buf.cursor(), 4);
}

#[test]
fn test_find_char_backward_not_found_stays_put() {
    let mut buf = create_buffer("hello world");
    buf.set_cursor(5).unwrap();
    apply_motion(Motion::FindCharBackward('z'), &mut buf);
    assert_eq!(buf.cursor(), 5);
}

#[test]
fn test_find_char_backward_does_not_cross_line_boundary() {
    let mut buf = create_buffer("hello\nworld");
    buf.set_cursor(8).unwrap();
    apply_motion(Motion::FindCharBackward('l'), &mut buf);
    assert_eq!(buf.cursor(), 8);
}

#[test]
fn test_find_char_backward_finds_first_char_on_line() {
    let mut buf = create_buffer("hello world");
    buf.set_cursor(10).unwrap();
    apply_motion(Motion::FindCharBackward('h'), &mut buf);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_till_char_forward_stops_one_before_target() {
    let mut buf = create_buffer("hello world");
    apply_motion(Motion::TillCharForward('o'), &mut buf);
    assert_eq!(buf.cursor(), 3);
}

#[test]
fn test_till_char_forward_not_found_stays_put() {
    let mut buf = create_buffer("hello world");
    apply_motion(Motion::TillCharForward('z'), &mut buf);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_till_char_forward_does_not_cross_line_boundary() {
    let mut buf = create_buffer("hello\nworld");
    apply_motion(Motion::TillCharForward('w'), &mut buf);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_till_char_forward_adjacent_target_stays_put() {
    let mut buf = create_buffer("ab");
    apply_motion(Motion::TillCharForward('b'), &mut buf);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_till_char_backward_stops_one_after_target() {
    let mut buf = create_buffer("hello world");
    buf.set_cursor(10).unwrap();
    apply_motion(Motion::TillCharBackward('o'), &mut buf);
    assert_eq!(buf.cursor(), 8);
}

#[test]
fn test_till_char_backward_not_found_stays_put() {
    let mut buf = create_buffer("hello world");
    buf.set_cursor(10).unwrap();
    apply_motion(Motion::TillCharBackward('z'), &mut buf);
    assert_eq!(buf.cursor(), 10);
}

#[test]
fn test_till_char_backward_does_not_cross_line_boundary() {
    let mut buf = create_buffer("hello\nworld");
    buf.set_cursor(6).unwrap();
    apply_motion(Motion::TillCharBackward('l'), &mut buf);
    assert_eq!(buf.cursor(), 6);
}

#[test]
fn test_till_char_backward_adjacent_target_stays_put() {
    let mut buf = create_buffer("ba");
    buf.set_cursor(1).unwrap();
    apply_motion(Motion::TillCharBackward('b'), &mut buf);
    assert_eq!(buf.cursor(), 1);
}
