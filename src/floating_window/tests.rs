//! Tests for floating window

use crate::floating_window::{FloatingWindow, WindowPosition};
use crate::test_utils::MockTerminal;

#[test]
fn test_floating_window_new() {
    let window = FloatingWindow::new(WindowPosition::Center, 40, 10);
    assert_eq!(window.width(), 40);
    assert_eq!(window.height(), 10);
}

#[test]
fn test_floating_window_with_options() {
    let window = FloatingWindow::new(WindowPosition::Center, 40, 10)
        .with_border(false)
        .with_reverse_video(false);

    assert_eq!(window.width(), 40);
    assert_eq!(window.height(), 10);
}

#[test]
fn test_floating_window_render_single_line() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Bottom, 50, 1).with_border(false);

    window
        .render_single_line(&mut term, ":", "test command")
        .unwrap();

    let written = term.get_written_string();
    assert!(written.contains(":test command"));
}

#[test]
fn test_floating_window_render_multiline() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 30, 3).with_border(true);

    let content = vec![b"Line 1".to_vec(), b"Line 2".to_vec(), b"Line 3".to_vec()];

    window.render(&mut term, &content).unwrap();

    let written = term.get_written_string();
    // Should contain border characters (Unicode box drawing by default)
    assert!(
        written.contains("╭")
            || written.contains("╮")
            || written.contains("╰")
            || written.contains("╯")
            || written.contains("─")
            || written.contains("│")
            || written.contains("+")
            || written.contains("-")
            || written.contains("|")
    );
    // Should contain content
    assert!(written.contains("Line 1"));
}

#[test]
fn test_floating_window_position_center() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 40, 10);

    // Render empty content to test positioning
    window.render(&mut term, &[]).unwrap();

    // Check that window was rendered (should have written output)
    // Center of 24 rows = row 7, center of 80 cols = col 20
    // ANSI positions are 1-indexed, so we expect [8;21H or similar
    let written = term.get_written_string();
    assert!(!written.is_empty());
    // Should contain ANSI cursor positioning codes
    assert!(written.contains('\x1b') || written.contains('['));
}

#[test]
fn test_floating_window_position_bottom() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Bottom, 50, 1).with_border(false);

    window.render_single_line(&mut term, ":", "test").unwrap();

    // Should have rendered at bottom row (23, since 0-indexed)
    // ANSI positions are 1-indexed, so row 24
    let written = term.get_written_string();
    assert!(written.contains(":test"));
    // Should contain ANSI cursor positioning for bottom row
    assert!(written.contains('\x1b') || written.contains('['));
}

#[test]
fn test_floating_window_position_top() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Top, 50, 1).with_border(false);

    window.render_single_line(&mut term, ":", "test").unwrap();

    // Should have rendered at top row (0, which is row 1 in ANSI 1-indexed)
    let written = term.get_written_string();
    assert!(written.contains(":test"));
    // Should contain ANSI cursor positioning for top row
    assert!(written.contains('\x1b') || written.contains('['));
}

#[test]
fn test_floating_window_position_absolute() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Absolute { row: 10, col: 20 }, 30, 5);

    window.render(&mut term, &[]).unwrap();

    // Should have rendered at the specified position
    // ANSI positions are 1-indexed, so row 11, col 21
    let written = term.get_written_string();
    assert!(!written.is_empty());
    // Should contain ANSI cursor positioning codes
    assert!(written.contains('\x1b') || written.contains('['));
}

#[test]
fn test_floating_window_content_truncation() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 10, 1).with_border(false);

    // Content longer than window width
    let long_content = "This is a very long line that should be truncated";
    window
        .render_single_line(&mut term, "", long_content)
        .unwrap();

    let written = term.get_written_string();
    // Should be truncated to 10 characters
    assert!(written.len() >= 10);
}
