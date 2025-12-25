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
    let scrolled = viewport.update(0, 100);
    assert_eq!(viewport.top_line(), 0);
    assert!(scrolled); // First update returns true
                       // Second update should not scroll
    let scrolled2 = viewport.update(0, 100);
    assert!(!scrolled2); // Should not scroll if already at top
}

#[test]
fn test_viewport_update_cursor_scroll_down() {
    let mut viewport = Viewport::new(10, 80);
    // 10 visible rows = 9 content rows (1 for status bar)
    // Move cursor to line 10 (beyond visible area)
    let scrolled = viewport.update(10, 100);
    // Should scroll so cursor is visible
    // top_line should be 10 - (9 - 1) = 10 - 8 = 2
    assert_eq!(viewport.top_line(), 2);
    assert!(scrolled); // Should have scrolled
                       // Cursor line 10 should now be visible (lines 2-10 shown)
}

#[test]
fn test_viewport_update_cursor_scroll_up() {
    let mut viewport = Viewport::new(10, 80);
    // Start scrolled down
    let _ = viewport.update(20, 100);
    assert!(viewport.top_line() > 0);

    // Move cursor back up
    let scrolled = viewport.update(5, 100);
    // Should scroll up to show cursor
    assert_eq!(viewport.top_line(), 5);
    assert!(scrolled); // Should have scrolled
}

#[test]
fn test_viewport_update_small_buffer() {
    let mut viewport = Viewport::new(10, 80);
    // Buffer has only 5 lines, viewport can show 9 content lines
    let _ = viewport.update(3, 5);
    // Should start at top since buffer fits in viewport
    assert_eq!(viewport.top_line(), 0);
}

#[test]
fn test_viewport_update_cursor_at_bottom_of_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let total_lines = 50;
    // Move cursor to last line
    let _ = viewport.update(total_lines - 1, total_lines);
    // Should scroll to show last line
    // top_line should be (total_lines - 1) - (content_rows - 1)
    // = 49 - 8 = 41, showing lines 41-49
    let content_rows = viewport.visible_rows() - 1;
    let expected_top = (total_lines - 1).saturating_sub(content_rows - 1);
    assert_eq!(viewport.top_line(), expected_top);
}

#[test]
fn test_viewport_update_cursor_middle() {
    let mut viewport = Viewport::new(10, 80);
    // Cursor in middle of visible area, shouldn't scroll
    let _ = viewport.update(5, 100);
    // Should stay at top since cursor is visible
    assert_eq!(viewport.top_line(), 0);
}

#[test]
fn test_viewport_update_cursor_just_below_visible() {
    let mut viewport = Viewport::new(10, 80);
    // 9 content rows, so lines 0-8 visible
    // Cursor at line 9 (just below)
    viewport.update(9, 100);
    // Should scroll to show line 9
    // top_line = 9 - (9 - 1) = 9 - 8 = 1
    assert_eq!(viewport.top_line(), 1);
}

#[test]
fn test_viewport_update_empty_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let _ = viewport.update(0, 0);
    // Should handle empty buffer gracefully
    assert_eq!(viewport.top_line(), 0);
}

#[test]
fn test_viewport_update_single_line_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let _ = viewport.update(0, 1);
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
    let _ = viewport.update(0, total_lines);
    assert_eq!(viewport.top_line(), 0);

    // Scroll down gradually
    for i in 1..20 {
        let _ = viewport.update(i, total_lines);
        // Should scroll when cursor goes beyond visible area
        if i > 8 {
            // Beyond 9 content rows
            assert!(viewport.top_line() > 0);
        }
    }

    // Scroll back up
    let _ = viewport.update(5, total_lines);
    assert_eq!(viewport.top_line(), 5);
}

#[test]
fn test_viewport_large_buffer() {
    let mut viewport = Viewport::new(10, 80);
    let total_lines = 1000;

    // Move to middle
    let _ = viewport.update(500, total_lines);
    let content_rows = viewport.visible_rows() - 1;
    let expected_top = 500usize.saturating_sub(content_rows - 1);
    assert_eq!(viewport.top_line(), expected_top);

    // Move to end
    let _ = viewport.update(total_lines - 1, total_lines);
    let expected_top_end = (total_lines - 1).saturating_sub(content_rows - 1);
    assert_eq!(viewport.top_line(), expected_top_end);
}

#[test]
fn test_viewport_cursor_at_exact_bottom() {
    let mut viewport = Viewport::new(10, 80);
    // 9 content rows, so bottom visible is line 8 when top_line = 0
    // Cursor at line 8 should be visible, no scroll needed
    let _ = viewport.update(8, 100);
    assert_eq!(viewport.top_line(), 0);

    // Cursor at line 9 should trigger scroll
    viewport.update(9, 100);
    assert_eq!(viewport.top_line(), 1);
}
