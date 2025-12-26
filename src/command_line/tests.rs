//! Tests for command line

use crate::command_line::CommandLine;
use crate::layer::{Layer, LayerPriority};
use crate::state::CommandLineWindowSettings;
use crate::viewport::Viewport;

#[test]
fn test_command_line_render_to_layer() {
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let viewport = Viewport::new(24, 80);
    let window_settings = CommandLineWindowSettings::default();

    let (window_row, window_col, cmd_width) = CommandLine::render_to_layer(
        &mut layer,
        &viewport,
        "test command",
        None,
        &window_settings,
    );

    // Check window position is centered
    // Default width ratio is 0.5, so width = 40, centered at (80-40)/2 = 20
    // Default height is 3, so centered at (24-3)/2 = 10 (or close)
    assert!(window_col >= 15 && window_col <= 25);
    assert!(window_row >= 8 && window_row <= 12);
    assert!(cmd_width >= 30);

    // Check content was rendered to layer
    // The `:` prompt should be at window_row+1 (content row), window_col+1 (after left border)
    let cell = layer.get_cell(window_row as usize + 1, window_col as usize + 1);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b':']);

    // The 't' from 'test' should be at window_col + 2
    let cell = layer.get_cell(window_row as usize + 1, window_col as usize + 2);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b't']);
}

#[test]
fn test_command_line_calculate_cursor_position() {
    // Window at (10, 20) with width 50
    let window_pos = (10, 20);
    let cmd_width = 50;
    let command_line = "test";

    let (cursor_row, cursor_col) =
        CommandLine::calculate_cursor_position(window_pos, cmd_width, command_line);

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

    let (cursor_row, cursor_col) =
        CommandLine::calculate_cursor_position(window_pos, cmd_width, command_line);

    // Content row should be window_row + 1
    assert_eq!(cursor_row, 11);

    // Cursor should be clamped to content_end_col = window_col + cmd_width - 2
    // = 20 + 10 - 2 = 28
    assert_eq!(cursor_col, 28);
}

#[test]
fn test_command_line_with_custom_border_chars() {
    use crate::floating_window::BorderChars;

    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let viewport = Viewport::new(24, 80);
    let window_settings = CommandLineWindowSettings::default();
    let custom_border = BorderChars::from_ascii(b'+', b'+', b'+', b'+', b'-', b'|');

    let (window_row, window_col, _) = CommandLine::render_to_layer(
        &mut layer,
        &viewport,
        "test",
        Some(custom_border),
        &window_settings,
    );

    // Check top-left corner has custom border character
    let cell = layer.get_cell(window_row as usize, window_col as usize);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'+']);
}
