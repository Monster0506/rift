//! Tests for viewport management

use crate::viewport::Viewport;

#[test]
fn test_viewport_new() {
    let viewport = Viewport::new(10, 80);
    assert_eq!(viewport.top_line(), 0);
    assert_eq!(viewport.visible_rows(), 10);
    assert_eq!(viewport.visible_cols(), 80);
}

#[test]
fn test_viewport_update_cursor_at_top() {
    let mut viewport = Viewport::new(10, 80);
    // Cursor at line 0, should stay at top
    let scrolled = viewport.update(0, 0, 100, 0);
    assert_eq!(viewport.top_line(), 0);
    assert!(scrolled); // First update returns true
                       // Second update should not scroll
    let scrolled2 = viewport.update(0, 0, 100, 0);
    assert!(!scrolled2); // Should not scroll if already at top
}

#[test]
fn test_viewport_update_cursor_scroll_down() {
    let mut viewport = Viewport::new(10, 80);
    // 10 visible rows -> 9 content rows, half = 4
    // cursor=10 -> ideal_top = 10 - 4 = 6
    let needs_full = viewport.update(10, 0, 100, 0);
    assert_eq!(viewport.top_line(), 6);
    // First update always requests a full redraw.
    assert!(needs_full);
}

#[test]
fn test_viewport_update_cursor_scroll_up() {
    let mut viewport = Viewport::new(10, 80);
    // cursor=20 -> ideal_top = 20 - 4 = 16
    let _ = viewport.update(20, 0, 100, 0);
    assert!(viewport.top_line() > 0);

    // cursor=5 -> ideal_top = 5 - 4 = 1
    let needs_full = viewport.update(5, 0, 100, 0);
    assert_eq!(viewport.top_line(), 1);
    // A plain scroll repaints through the cell diff, not a full redraw.
    assert!(!needs_full);
}

#[test]
fn test_viewport_update_small_buffer() {
    let mut viewport = Viewport::new(10, 80);
    // Buffer has only 5 lines, viewport can show 9 content lines
    let _ = viewport.update(3, 0, 5, 0);
    // Should start at top since buffer fits in viewport
    assert_eq!(viewport.top_line(), 0);
}

#[test]
fn test_viewport_update_cursor_at_bottom_of_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let total_lines = 50;
    // cursor=49, half=4 -> ideal_top=45, max_top=41 -> clamped to 41
    let _ = viewport.update(total_lines - 1, 0, total_lines, 0);
    assert_eq!(viewport.top_line(), 41);
}

#[test]
fn test_viewport_update_cursor_middle() {
    let mut viewport = Viewport::new(10, 80);
    // cursor=5, half=4 -> ideal_top = 1
    let _ = viewport.update(5, 0, 100, 0);
    assert_eq!(viewport.top_line(), 1);
}

#[test]
fn test_viewport_update_cursor_just_below_visible() {
    let mut viewport = Viewport::new(10, 80);
    // cursor=9, half=4 -> ideal_top = 5
    viewport.update(9, 0, 100, 0);
    assert_eq!(viewport.top_line(), 5);
}

#[test]
fn test_viewport_update_empty_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let _ = viewport.update(0, 0, 0, 0);
    // Should handle empty buffer gracefully
    assert_eq!(viewport.top_line(), 0);
}

#[test]
fn test_viewport_update_single_line_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let _ = viewport.update(0, 0, 1, 0);
    // Should stay at top
    assert_eq!(viewport.top_line(), 0);
}

#[test]
fn test_viewport_set_size() {
    let mut viewport = Viewport::new(10, 80);
    viewport.set_size(20, 100);
    assert_eq!(viewport.visible_rows(), 20);
    assert_eq!(viewport.visible_cols(), 100);
}

#[test]
fn test_viewport_scroll_sequence() {
    let mut viewport = Viewport::new(10, 80);
    let total_lines = 100;

    // Start at top
    let _ = viewport.update(0, 0, total_lines, 0);
    assert_eq!(viewport.top_line(), 0);

    // Cursor scrolls immediately: at line i, top_line = max(0, i - 4)
    for i in 1..20usize {
        let _ = viewport.update(i, 0, total_lines, 0);
        let expected = i.saturating_sub(4);
        assert_eq!(viewport.top_line(), expected);
    }

    // cursor=5, half=4 -> top_line = 1
    let _ = viewport.update(5, 0, total_lines, 0);
    assert_eq!(viewport.top_line(), 1);
}

#[test]
fn test_viewport_large_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let total_lines = 1000;
    // content_rows = 9, half = 4, max_top = 991

    // cursor=500 -> ideal_top = 496, clamped to 496
    let _ = viewport.update(500, 0, total_lines, 0);
    assert_eq!(viewport.top_line(), 496);

    // cursor=999 -> ideal_top = 995, clamped to max_top = 991
    let _ = viewport.update(total_lines - 1, 0, total_lines, 0);
    assert_eq!(viewport.top_line(), 991);
}

#[test]
fn test_viewport_cursor_at_exact_bottom() {
    let mut viewport = Viewport::new(10, 80);
    // content_rows=9, half=4
    // cursor=8 -> ideal_top = 4
    let _ = viewport.update(8, 0, 100, 0);
    assert_eq!(viewport.top_line(), 4);

    // cursor=9 -> ideal_top = 5
    viewport.update(9, 0, 100, 0);
    assert_eq!(viewport.top_line(), 5);
}

#[test]
fn test_viewport_horizontal_scrolling() {
    let mut viewport = Viewport::new(10, 20); // 20 cols wide
    let total_lines = 100;

    // 1. Initial state
    viewport.update(0, 0, total_lines, 0);
    assert_eq!(viewport.left_col(), 0);

    // 2. Cursor moves near right edge but still visible
    // Visible chars: 20. Last visible index: 19.
    viewport.update(0, 19, total_lines, 0);
    assert_eq!(viewport.left_col(), 0);

    // 3. Cursor moves just off screen to right
    viewport.update(0, 20, total_lines, 0);
    assert_eq!(viewport.left_col(), 1);

    // 4. Cursor moves way to right
    viewport.update(0, 50, total_lines, 0);
    // left_col = 50 - 20 + 1 = 31.
    assert_eq!(viewport.left_col(), 31);

    // 5. Cursor moves back left
    viewport.update(0, 10, total_lines, 0);
    // 10 < 31, so scroll left to show 10
    assert_eq!(viewport.left_col(), 10);
}

#[test]
fn test_viewport_horizontal_scrolling_with_gutter() {
    let mut viewport = Viewport::new(10, 20);
    let total_lines = 100;
    let gutter_width = 5; // "100 " + space

    // Content width = 15.

    // 1. Initial state
    viewport.update(0, 0, total_lines, gutter_width);
    assert_eq!(viewport.left_col(), 0);

    // 2. Cursor at end of visible content
    // content width 15, indices 0-14 visible from left_col 0.
    // so cursor at 14 should strictly fit.
    viewport.update(0, 14, total_lines, gutter_width);
    assert_eq!(viewport.left_col(), 0);

    // 3. Cursor moves to 15 (outside content area)
    viewport.update(0, 15, total_lines, gutter_width);
    // Should scroll
    // left_col = 15 - 15 + 1 = 1.
    assert_eq!(viewport.left_col(), 1);
}

#[test]
fn test_viewport_resize_scrolling() {
    let mut viewport = Viewport::new(10, 80);
    let total_lines = 100;

    // Scroll down to line 50
    viewport.update(50, 0, total_lines, 0);
    let initial_top = viewport.top_line();
    assert!(initial_top > 0);

    // Resize to be smaller (5 rows)
    viewport.set_size(5, 80);

    // Update with same cursor position
    viewport.update(50, 0, total_lines, 0);

    // Top line should have adjusted to keep cursor visible in smaller viewport
    // Cursor at 50. Visible rows 5. Content rows 4 (1 for status).
    // Max top line for cursor 50 is 50 - (4-1) = 47.
    // Min top line is 50 - 0 = 50 (if cursor at top).
    // So top line should be between 47 and 50.
    assert!(viewport.top_line() >= 47);
    assert!(viewport.top_line() <= 50);

    // Resize to be larger (20 rows)
    viewport.set_size(20, 80);
    viewport.update(50, 0, total_lines, 0);

    // Cursor should still be visible
    assert!(viewport.top_line() <= 50);
    assert!(viewport.top_line() + viewport.visible_rows() > 50);
}

#[test]
fn sub_line_offset_defaults_to_zero() {
    let viewport = Viewport::new(10, 80);
    assert_eq!(viewport.sub_line_offset(), 0.0);
}

#[test]
fn sub_line_offset_is_settable_and_clamped() {
    let mut viewport = Viewport::new(10, 80);
    viewport.set_sub_line_offset(0.5);
    assert_eq!(viewport.sub_line_offset(), 0.5);

    viewport.set_sub_line_offset(-1.0);
    assert_eq!(viewport.sub_line_offset(), 0.0);

    viewport.set_sub_line_offset(5.0);
    assert_eq!(viewport.sub_line_offset(), 1.0);
}

#[test]
fn integer_scroll_paths_reset_sub_line_offset() {
    let mut viewport = Viewport::new(10, 80);

    viewport.set_sub_line_offset(0.7);
    viewport.update(10, 0, 100, 0);
    assert_eq!(viewport.sub_line_offset(), 0.0);

    viewport.set_sub_line_offset(0.7);
    viewport.update_visual(10, 0, 100, 0);
    assert_eq!(viewport.sub_line_offset(), 0.0);

    viewport.set_sub_line_offset(0.7);
    viewport.set_scroll(3, 0);
    assert_eq!(viewport.sub_line_offset(), 0.0);

    viewport.set_sub_line_offset(0.7);
    viewport.center_on(5, 100);
    assert_eq!(viewport.sub_line_offset(), 0.0);
}
