//! Tests for floating window

use crate::floating_window::{BorderChars, FloatingWindow, WindowPosition, WindowStyle};
use crate::layer::{Layer, LayerPriority};

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
fn test_floating_window_content_dimensions() {
    // With border: content is 2 smaller in each dimension
    let window = FloatingWindow::new(WindowPosition::Center, 40, 10).with_border(true);
    assert_eq!(window.content_width(), 38);
    assert_eq!(window.content_height(), 8);

    // Without border: content matches window size
    let window = FloatingWindow::new(WindowPosition::Center, 40, 10).with_border(false);
    assert_eq!(window.content_width(), 40);
    assert_eq!(window.content_height(), 10);
}

#[test]
fn test_floating_window_render_single_line() {
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let window = FloatingWindow::new(WindowPosition::Bottom, 50, 1).with_border(false);

    window.render_single_line(&mut layer, ":", "test command");

    // Check that content was written to the layer
    // Window is at bottom, so row 23 (24-1)
    let pos = window.calculate_position(24, 80);
    assert_eq!(pos.0, 23); // Bottom row

    // Check content cells
    let start_col = pos.1 as usize;
    let cell = layer.get_cell(pos.0 as usize, start_col);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b':']);
}

#[test]
fn test_floating_window_render_multiline() {
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 30, 5).with_border(true);

    let content = vec![b"Line 1".to_vec(), b"Line 2".to_vec(), b"Line 3".to_vec()];

    window.render(&mut layer, &content);

    // Window is centered at row (24-5)/2 = 9, col (80-30)/2 = 25
    let (row, col) = window.calculate_position(24, 80);
    assert_eq!(row, 9);
    assert_eq!(col, 25);

    // Check top-left corner has border character
    let cell = layer.get_cell(row as usize, col as usize);
    assert!(cell.is_some());
    // Should be top-left corner: ╭
    assert_eq!(cell.unwrap().content, "╭".as_bytes());

    // Check content row (first content at row+1, col+1)
    let cell = layer.get_cell(row as usize + 1, col as usize + 1);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'L']);
}

#[test]
fn test_floating_window_position_center() {
    let window = FloatingWindow::new(WindowPosition::Center, 40, 10);

    // Center of 24 rows with height 10: (24-10)/2 = 7
    // Center of 80 cols with width 40: (80-40)/2 = 20
    let pos = window.calculate_position(24, 80);
    assert_eq!(pos.0, 7);
    assert_eq!(pos.1, 20);
}

#[test]
fn test_floating_window_position_bottom() {
    let window = FloatingWindow::new(WindowPosition::Bottom, 50, 3);

    // Bottom: 24-3 = 21
    // Center horizontal: (80-50)/2 = 15
    let pos = window.calculate_position(24, 80);
    assert_eq!(pos.0, 21);
    assert_eq!(pos.1, 15);
}

#[test]
fn test_floating_window_position_top() {
    let window = FloatingWindow::new(WindowPosition::Top, 50, 3);

    // Top: row 0
    // Center horizontal: (80-50)/2 = 15
    let pos = window.calculate_position(24, 80);
    assert_eq!(pos.0, 0);
    assert_eq!(pos.1, 15);
}

#[test]
fn test_floating_window_position_absolute() {
    let window = FloatingWindow::new(WindowPosition::Absolute { row: 10, col: 20 }, 30, 5);

    let pos = window.calculate_position(24, 80);
    assert_eq!(pos.0, 10);
    assert_eq!(pos.1, 20);
}

#[test]
fn test_floating_window_position_absolute_clamped() {
    // Window would go past screen bounds
    let window = FloatingWindow::new(WindowPosition::Absolute { row: 20, col: 70 }, 30, 10);

    let pos = window.calculate_position(24, 80);
    // Should be clamped: 24-10 = 14 max row, 80-30 = 50 max col
    assert_eq!(pos.0, 14);
    assert_eq!(pos.1, 50);
}

#[test]
fn test_floating_window_content_truncation() {
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 10, 1).with_border(false);

    // Content longer than window width
    let long_content = b"This is a very long line that should be truncated".to_vec();
    window.render(&mut layer, &[long_content]);

    let pos = window.calculate_position(24, 80);

    // Only first 10 characters should be rendered
    let cell = layer.get_cell(pos.0 as usize, pos.1 as usize);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'T']);

    // Cell at position 10 should be 'i' (10th char, 0-indexed = 9)
    let cell = layer.get_cell(pos.0 as usize, pos.1 as usize + 9);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b' ']); // 'a' from "a very" - wait, let me check

    // Actually "This is a " is 10 chars, so position 9 (0-indexed) = ' ' (space after 'a')
}

#[test]
fn test_floating_window_with_custom_border_chars() {
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let custom_border = BorderChars::from_ascii(b'+', b'+', b'+', b'+', b'-', b'|');
    let window = FloatingWindow::new(WindowPosition::Center, 10, 3)
        .with_border(true)
        .with_border_chars(custom_border);

    window.render(&mut layer, &[b"Hi".to_vec()]);

    let pos = window.calculate_position(24, 80);

    // Top-left should be '+' (ASCII)
    let cell = layer.get_cell(pos.0 as usize, pos.1 as usize);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'+']);

    // Horizontal border should be '-'
    let cell = layer.get_cell(pos.0 as usize, pos.1 as usize + 1);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'-']);
}

#[test]
fn test_window_style_defaults() {
    let style = WindowStyle::default();
    assert!(style.border);
    assert!(style.reverse_video);
    assert!(style.border_chars.is_none());
}

#[test]
fn test_floating_window_no_border() {
    let mut layer = Layer::new(LayerPriority::FLOATING_WINDOW, 24, 80);
    let window = FloatingWindow::new(WindowPosition::Center, 10, 3).with_border(false);

    window.render(&mut layer, &[b"Hello".to_vec(), b"World".to_vec()]);

    let pos = window.calculate_position(24, 80);

    // First character should be 'H' (no border)
    let cell = layer.get_cell(pos.0 as usize, pos.1 as usize);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'H']);

    // Second row should start with 'W'
    let cell = layer.get_cell(pos.0 as usize + 1, pos.1 as usize);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'W']);
}
