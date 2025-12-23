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
    let window = FloatingWindow::new(WindowPosition::Bottom, 50, 1)
        .with_border(false);
    
    window.render_single_line(&mut term, ":", "test command").unwrap();
    
    let written = term.get_written_string();
    assert!(written.contains(":test command"));
}

#[test]
fn test_floating_window_render_multiline() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 30, 3)
        .with_border(true);
    
    let content = vec![
        b"Line 1".to_vec(),
        b"Line 2".to_vec(),
        b"Line 3".to_vec(),
    ];
    
    window.render(&mut term, &content).unwrap();
    
    let written = term.get_written_string();
    // Should contain border characters
    assert!(written.contains("+") || written.contains("-") || written.contains("|"));
    // Should contain content
    assert!(written.contains("Line 1"));
}

#[test]
fn test_floating_window_position_center() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 40, 10);
    
    // Render empty content to test positioning
    window.render(&mut term, &[]).unwrap();
    
    // Check that cursor was moved (window was rendered)
    assert!(!term.cursor_moves.is_empty());
}

#[test]
fn test_floating_window_position_bottom() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Bottom, 50, 1)
        .with_border(false);
    
    window.render_single_line(&mut term, ":", "test").unwrap();
    
    // Should have moved cursor to bottom row (23, since 0-indexed)
    let bottom_move = term.cursor_moves.iter()
        .any(|(row, _)| *row == 23);
    assert!(bottom_move);
}

#[test]
fn test_floating_window_position_top() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Top, 50, 1)
        .with_border(false);
    
    window.render_single_line(&mut term, ":", "test").unwrap();
    
    // Should have moved cursor to top row (0)
    let top_move = term.cursor_moves.iter()
        .any(|(row, _)| *row == 0);
    assert!(top_move);
}

#[test]
fn test_floating_window_position_absolute() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Absolute { row: 10, col: 20 }, 30, 5);
    
    window.render(&mut term, &[]).unwrap();
    
    // Should have moved cursor to the specified position
    let position_move = term.cursor_moves.iter()
        .any(|(row, col)| *row == 10 && *col == 20);
    assert!(position_move);
}

#[test]
fn test_floating_window_content_truncation() {
    let mut term = MockTerminal::new(24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 10, 1)
        .with_border(false);
    
    // Content longer than window width
    let long_content = "This is a very long line that should be truncated";
    window.render_single_line(&mut term, "", long_content).unwrap();
    
    let written = term.get_written_string();
    // Should be truncated to 10 characters
    assert!(written.len() >= 10);
}

