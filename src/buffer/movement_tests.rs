use crate::action::Motion;
use crate::buffer::TextBuffer;
use crate::wrap::{DisplayMap, OperatorContext};

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

fn apply_motion_wrap(motion: Motion, buf: &mut TextBuffer, wrap_width: usize) {
    let dm = DisplayMap::build(buf, wrap_width, 4);
    motion.apply(buf, Some(&dm), OperatorContext::Move, 4, 20, None);
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

// desired_col tests from design.md

#[test]
fn test_desired_col_basic_restore() {
    // "hello world" row=0 (col 10 reachable)
    // "hi"          row=1
    // "hello world" row=2
    let mut buf = create_buffer("hello world\nhi\nhello world");
    buf.set_cursor(10).unwrap(); // col 10 on row 0
    assert_eq!(buf.desired_col(), None);

    buf.move_down(); // j: clamp to col 1, latch desired_col=10
    assert_eq!(buf.get_col(), 1);
    assert_eq!(buf.desired_col(), Some(10));

    buf.move_up(); // k: restore to col 10
    assert_eq!(buf.get_col(), 10);
    assert_eq!(buf.desired_col(), Some(10));
}

#[test]
fn test_desired_col_persists_across_multiple_short_lines() {
    // "long line here" row=0 (col 13 reachable)
    // "x"             row=1
    // "y"             row=2
    // "long line here" row=3
    let mut buf = create_buffer("long line here\nx\ny\nlong line here");
    buf.set_cursor(13).unwrap(); // col 13 on row 0
    assert_eq!(buf.desired_col(), None);

    buf.move_down(); // j -> row=1, col=0, desired_col=Some(13)
    assert_eq!(buf.get_col(), 0);
    assert_eq!(buf.desired_col(), Some(13));

    buf.move_down(); // j -> row=2, col=0, desired_col still Some(13)
    assert_eq!(buf.get_col(), 0);
    assert_eq!(buf.desired_col(), Some(13));

    buf.move_down(); // j -> row=3, col=13 (restored)
    assert_eq!(buf.get_col(), 13);
    assert_eq!(buf.desired_col(), Some(13));
}

#[test]
fn test_desired_col_horizontal_resets() {
    let mut buf = create_buffer("hello world\nhi\nhello world");
    buf.set_cursor(10).unwrap();

    buf.move_down(); // latch desired_col=Some(10)
    assert_eq!(buf.desired_col(), Some(10));

    buf.move_left(); // h: clear desired_col
    assert_eq!(buf.desired_col(), None);

    // next j uses real col (now 0 after clamping)
    buf.move_down();
    assert_eq!(buf.get_col(), 0); // col=0 on "hello world" row 2 (cursor was at 0)
}

#[test]
fn test_desired_col_dollar_sets_max() {
    // "short"          row=0
    // "much longer line" row=1
    let mut buf = create_buffer("short\nmuch longer line");
    buf.move_to_start();

    buf.move_to_line_end(); // $: col=4, desired_col=Some(usize::MAX)
    assert_eq!(buf.desired_col(), Some(usize::MAX));

    buf.move_down(); // j: should land at EOL of longer line
                     // "much longer line" is 16 chars, line_end is at position 16 (after '\n' offset)
                     // col should be 15 (last char index)
    assert_eq!(buf.get_col(), 15);
}

#[test]
fn test_desired_col_zero_clears() {
    let mut buf = create_buffer("hello world\nhi\nhello world");
    buf.set_cursor(10).unwrap();
    buf.move_down(); // latch desired_col=Some(10)
    assert_eq!(buf.desired_col(), Some(10));

    buf.move_to_line_start(); // 0: clear desired_col
    assert_eq!(buf.desired_col(), None);
    assert_eq!(buf.get_col(), 0);

    buf.move_down(); // uses real col (0)
    assert_eq!(buf.get_col(), 0);
}

// Soft-wrap path regression tests (visual_up_to_col / visual_down_to_col)

#[test]
fn test_desired_col_wrap_dollar_jk_restores_eol() {
    // Regression: after $jk the cursor landed at line 2's visual EOL col
    // instead of at EOL of line 0. wrap_width=80 keeps lines unbroken so the
    // test exercises the visual path without confounding intra-line wrapping.
    let mut buf = create_buffer("hello world\nhi\nhello world");
    apply_motion(Motion::EndOfLine, &mut buf);
    let eol = buf.cursor();
    assert_eq!(buf.desired_col(), Some(usize::MAX));

    apply_motion_wrap(Motion::Down, &mut buf, 80); // j -> "hi", desired_col stays MAX
    assert_eq!(buf.desired_col(), Some(usize::MAX));

    apply_motion_wrap(Motion::Up, &mut buf, 80); // k -> must return to EOL of line 0
    assert_eq!(
        buf.cursor(),
        eol,
        "k after $j must restore the EOL position"
    );
}

#[test]
fn test_desired_col_wrap_persists_across_short_lines() {
    // Same as the non-wrap test but through the visual path.
    let mut buf = create_buffer("long line here\nx\ny\nlong line here");
    buf.set_cursor(13).unwrap();

    apply_motion_wrap(Motion::Down, &mut buf, 80);
    let latched = buf.desired_col();
    assert!(latched.is_some(), "first j must latch desired_col");

    apply_motion_wrap(Motion::Down, &mut buf, 80);
    assert_eq!(
        buf.desired_col(),
        latched,
        "second j must preserve desired_col"
    );

    apply_motion_wrap(Motion::Down, &mut buf, 80);
    assert_eq!(
        buf.desired_col(),
        latched,
        "third j must preserve desired_col"
    );
    assert_eq!(buf.get_col(), 13, "j past short lines must restore col 13");
}

#[test]
fn test_desired_col_wrap_horizontal_clears() {
    // h in the visual path clears desired_col via move_left.
    let mut buf = create_buffer("hello world\nhi\nhello world");
    buf.set_cursor(10).unwrap();

    apply_motion_wrap(Motion::Down, &mut buf, 80); // j -> "hi"
    assert!(buf.desired_col().is_some());

    // h clears desired_col; cursor moves off the clamped '\n' to 'i'
    apply_motion(Motion::Left, &mut buf);
    assert_eq!(buf.desired_col(), None);
    let col_after_h = buf.get_col(); // actual col after the left move

    let pre_j = buf.cursor();
    apply_motion_wrap(Motion::Down, &mut buf, 80); // j must NOT use old desired_col
                                                   // If desired_col were still set the cursor would differ from using real col.
                                                   // Verify by checking the cursor moved down from pre_j, using the real col.
    assert!(buf.cursor() != pre_j, "j must move down, not stay put");
    // The latched col matches the real col at the time of the move (col_after_h).
    assert_eq!(buf.desired_col(), Some(col_after_h));
}

#[test]
fn test_desired_col_wrap_intra_line_wrapping() {
    // A long logical line wraps into two visual rows. j/k move between visual
    // rows (same logical line) and desired_col is preserved.
    // "hello world" with wrap_width=6 produces rows: ["hello ", "world"].
    let mut buf = create_buffer("hello world\nshort");
    buf.set_cursor(2).unwrap(); // visual col 2 ('l') on first segment

    let start = buf.cursor();
    apply_motion_wrap(Motion::Down, &mut buf, 6); // j -> "world" segment, visual col 2
    assert_eq!(
        buf.desired_col(),
        Some(2),
        "desired_col latched at visual col 2"
    );
    assert_ne!(buf.cursor(), start, "j must move to the next visual row");

    apply_motion_wrap(Motion::Up, &mut buf, 6); // k -> back to first segment
    assert_eq!(
        buf.cursor(),
        start,
        "k must return to the original position"
    );
}
