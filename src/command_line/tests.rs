//! Tests for command line

use crate::command_line::CommandLine;
use crate::test_utils::MockTerminal;
use crate::viewport::Viewport;
use crate::state::CommandLineWindowSettings;

#[test]
fn test_command_line_render() {
    let mut term = MockTerminal::new(24, 80);
    let viewport = Viewport::new(24, 80);
    let window_settings = CommandLineWindowSettings::default();
    
    let result = CommandLine::render(&mut term, &viewport, "test command", None, &window_settings).unwrap();
    assert!(result.is_some());
    
    let written = term.get_written_string();
    // Should contain border characters (Unicode box drawing by default)
    assert!(written.contains("╭") || written.contains("╮") || written.contains("╰") || 
            written.contains("╯") || written.contains("─") || written.contains("│") ||
            written.contains("+") || written.contains("-") || written.contains("|"));
    // Should contain prompt and content
    assert!(written.contains(":test command"));
}

#[test]
fn test_command_line_calculate_cursor_position() {
    // Window at (10, 20) with width 50
    let window_pos = (10, 20);
    let cmd_width = 50;
    let command_line = "test";
    
    let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
        window_pos,
        cmd_width,
        command_line,
    );
    
    // Content row should be window_row + 1 (middle row)
    assert_eq!(cursor_row, 11);
    
    // Cursor column: window_col + 1 (left border) + 1 (":") + command_line.len()
    // = 20 + 1 + 1 + 4 = 26
    assert_eq!(cursor_col, 26);
}

#[test]
fn test_command_line_cursor_position_clamped() {
    // Test that cursor position is clamped to content area
    let window_pos = (10, 20);
    let cmd_width = 10; // Small width
    let command_line = "very long command line that exceeds width";
    
    let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
        window_pos,
        cmd_width,
        command_line,
    );
    
    // Content row should be window_row + 1
    assert_eq!(cursor_row, 11);
    
    // Cursor should be clamped to content_end_col = window_col + cmd_width - 2
    // = 20 + 10 - 2 = 28
    assert_eq!(cursor_col, 28);
}

