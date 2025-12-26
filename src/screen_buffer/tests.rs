//! Tests for the screen_buffer module

use super::*;
use crate::color::Color;

#[test]
fn test_double_buffer_creation() {
    let buffer = DoubleBuffer::new(24, 80);
    assert_eq!(buffer.rows(), 24);
    assert_eq!(buffer.cols(), 80);
    assert!(buffer.needs_full_redraw()); // First frame needs full redraw
}

#[test]
fn test_double_buffer_set_and_get_cell() {
    let mut buffer = DoubleBuffer::new(10, 10);

    // Set a cell
    assert!(buffer.set_cell(5, 5, Cell::new(b'X')));

    // Get the cell back
    let cell = buffer.get_cell(5, 5);
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().content, vec![b'X']);
}

#[test]
fn test_double_buffer_out_of_bounds() {
    let mut buffer = DoubleBuffer::new(5, 5);

    // Out of bounds set should return false
    assert!(!buffer.set_cell(10, 10, Cell::new(b'X')));
    assert!(!buffer.set_cell(5, 3, Cell::new(b'X')));
    assert!(!buffer.set_cell(3, 5, Cell::new(b'X')));

    // Out of bounds get should return None
    assert!(buffer.get_cell(10, 10).is_none());
}

#[test]
fn test_double_buffer_first_frame_full_redraw() {
    let mut buffer = DoubleBuffer::new(3, 3);

    // Set some cells
    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.set_cell(1, 1, Cell::new(b'B'));

    // First frame should report all cells as changed
    let (batches, stats) = buffer.get_batched_changes();
    assert!(stats.full_redraw);
    assert_eq!(stats.total_cells, 9);
    assert_eq!(stats.changed_cells, 9);

    // All cells should be in batches
    let total_cells_in_batches: usize = batches.iter().map(|b| b.cells.len()).sum();
    assert_eq!(total_cells_in_batches, 9);
}

#[test]
fn test_double_buffer_swap_clears_full_redraw() {
    let mut buffer = DoubleBuffer::new(3, 3);
    assert!(buffer.needs_full_redraw());

    buffer.swap();
    assert!(!buffer.needs_full_redraw());
}

#[test]
fn test_double_buffer_detects_changes() {
    let mut buffer = DoubleBuffer::new(3, 3);

    // First frame
    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.swap();

    // Second frame - change one cell
    buffer.set_cell(0, 0, Cell::new(b'B'));

    let (batches, stats) = buffer.get_batched_changes();
    assert!(!stats.full_redraw);
    assert_eq!(stats.changed_cells, 1);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].row, 0);
    assert_eq!(batches[0].start_col, 0);
    assert_eq!(batches[0].cells[0].content, vec![b'B']);
}

#[test]
fn test_double_buffer_no_changes_detected() {
    let mut buffer = DoubleBuffer::new(3, 3);

    // First frame
    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.swap();

    // Second frame - same content
    buffer.set_cell(0, 0, Cell::new(b'A'));

    let (batches, stats) = buffer.get_batched_changes();
    assert!(!stats.full_redraw);
    assert_eq!(stats.changed_cells, 0);
    assert!(batches.is_empty());
}

#[test]
fn test_double_buffer_batches_consecutive_changes() {
    let mut buffer = DoubleBuffer::new(3, 10);
    buffer.swap(); // Clear first frame flag

    // Change consecutive cells
    buffer.set_cell(1, 3, Cell::new(b'H'));
    buffer.set_cell(1, 4, Cell::new(b'E'));
    buffer.set_cell(1, 5, Cell::new(b'L'));
    buffer.set_cell(1, 6, Cell::new(b'L'));
    buffer.set_cell(1, 7, Cell::new(b'O'));

    let (batches, stats) = buffer.get_batched_changes();
    assert_eq!(stats.changed_cells, 5);
    assert_eq!(batches.len(), 1); // All in one batch
    assert_eq!(batches[0].row, 1);
    assert_eq!(batches[0].start_col, 3);
    assert_eq!(batches[0].cells.len(), 5);
}

#[test]
fn test_double_buffer_separate_batches_for_gaps() {
    let mut buffer = DoubleBuffer::new(3, 10);
    buffer.swap(); // Clear first frame flag

    // Change non-consecutive cells
    buffer.set_cell(1, 1, Cell::new(b'A'));
    buffer.set_cell(1, 2, Cell::new(b'B'));
    // gap at 3, 4, 5
    buffer.set_cell(1, 6, Cell::new(b'C'));
    buffer.set_cell(1, 7, Cell::new(b'D'));

    let (batches, stats) = buffer.get_batched_changes();
    assert_eq!(stats.changed_cells, 4);
    assert_eq!(batches.len(), 2); // Two separate batches

    assert_eq!(batches[0].row, 1);
    assert_eq!(batches[0].start_col, 1);
    assert_eq!(batches[0].cells.len(), 2);

    assert_eq!(batches[1].row, 1);
    assert_eq!(batches[1].start_col, 6);
    assert_eq!(batches[1].cells.len(), 2);
}

#[test]
fn test_double_buffer_multiple_rows() {
    let mut buffer = DoubleBuffer::new(5, 5);
    buffer.swap();

    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.set_cell(2, 2, Cell::new(b'B'));
    buffer.set_cell(4, 4, Cell::new(b'C'));

    let (batches, stats) = buffer.get_batched_changes();
    assert_eq!(stats.changed_cells, 3);
    assert_eq!(batches.len(), 3); // One batch per row

    assert_eq!(batches[0].row, 0);
    assert_eq!(batches[1].row, 2);
    assert_eq!(batches[2].row, 4);
}

#[test]
fn test_double_buffer_resize() {
    let mut buffer = DoubleBuffer::new(3, 3);
    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.swap();

    // Resize larger
    buffer.resize(5, 5);
    assert_eq!(buffer.rows(), 5);
    assert_eq!(buffer.cols(), 5);
    assert!(buffer.needs_full_redraw()); // Resize forces full redraw

    // Content preserved
    assert_eq!(buffer.get_cell(0, 0).unwrap().content, vec![b'A']);
}

#[test]
fn test_double_buffer_resize_smaller() {
    let mut buffer = DoubleBuffer::new(5, 5);
    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.set_cell(4, 4, Cell::new(b'B'));
    buffer.swap();

    // Resize smaller
    buffer.resize(2, 2);
    assert_eq!(buffer.rows(), 2);
    assert_eq!(buffer.cols(), 2);

    // Content preserved where possible
    assert_eq!(buffer.get_cell(0, 0).unwrap().content, vec![b'A']);
    // Out of bounds now
    assert!(buffer.get_cell(4, 4).is_none());
}

#[test]
fn test_double_buffer_invalidate() {
    let mut buffer = DoubleBuffer::new(3, 3);
    buffer.swap();
    assert!(!buffer.needs_full_redraw());

    buffer.invalidate();
    assert!(buffer.needs_full_redraw());
}

#[test]
fn test_double_buffer_clear() {
    let mut buffer = DoubleBuffer::new(3, 3);
    buffer.set_cell(0, 0, Cell::new(b'X'));
    buffer.set_cell(1, 1, Cell::new(b'Y'));

    buffer.clear();

    // All cells should be empty (space)
    assert_eq!(buffer.get_cell(0, 0).unwrap().content, vec![b' ']);
    assert_eq!(buffer.get_cell(1, 1).unwrap().content, vec![b' ']);
}

#[test]
fn test_double_buffer_copy_from() {
    let mut buffer = DoubleBuffer::new(3, 3);

    let source = vec![
        vec![Cell::new(b'A'), Cell::new(b'B'), Cell::new(b'C')],
        vec![Cell::new(b'D'), Cell::new(b'E'), Cell::new(b'F')],
        vec![Cell::new(b'G'), Cell::new(b'H'), Cell::new(b'I')],
    ];

    buffer.copy_from(&source);

    assert_eq!(buffer.get_cell(0, 0).unwrap().content, vec![b'A']);
    assert_eq!(buffer.get_cell(1, 1).unwrap().content, vec![b'E']);
    assert_eq!(buffer.get_cell(2, 2).unwrap().content, vec![b'I']);
}

#[test]
fn test_double_buffer_iter_changes() {
    let mut buffer = DoubleBuffer::new(3, 3);
    buffer.swap();

    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.set_cell(2, 2, Cell::new(b'B'));

    let changes: Vec<_> = buffer.iter_changes().collect();
    assert_eq!(changes.len(), 2);

    assert_eq!(changes[0].row, 0);
    assert_eq!(changes[0].col, 0);
    assert_eq!(changes[0].cell.content, vec![b'A']);

    assert_eq!(changes[1].row, 2);
    assert_eq!(changes[1].col, 2);
    assert_eq!(changes[1].cell.content, vec![b'B']);
}

#[test]
fn test_double_buffer_cell_changed() {
    let mut buffer = DoubleBuffer::new(3, 3);
    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.swap();

    // Same content - not changed
    buffer.set_cell(0, 0, Cell::new(b'A'));
    assert!(!buffer.cell_changed(0, 0));

    // Different content - changed
    buffer.set_cell(0, 0, Cell::new(b'B'));
    assert!(buffer.cell_changed(0, 0));
}

#[test]
fn test_double_buffer_color_changes_detected() {
    let mut buffer = DoubleBuffer::new(3, 3);
    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.swap();

    // Same char, different color - should be detected as change
    buffer.set_cell(0, 0, Cell::new(b'A').with_fg(Color::Red));

    let (batches, stats) = buffer.get_batched_changes();
    assert_eq!(stats.changed_cells, 1);
    assert_eq!(batches.len(), 1);
}

#[test]
fn test_frame_stats_change_percentage() {
    let stats = FrameStats {
        total_cells: 100,
        changed_cells: 25,
        full_redraw: false,
    };
    assert!((stats.change_percentage() - 25.0).abs() < 0.01);
}

#[test]
fn test_frame_stats_zero_cells() {
    let stats = FrameStats {
        total_cells: 0,
        changed_cells: 0,
        full_redraw: false,
    };
    assert_eq!(stats.change_percentage(), 0.0);
}

#[test]
fn test_cell_batch_end_col() {
    let cell = Cell::new(b'X');
    let batch = CellBatch {
        row: 0,
        start_col: 5,
        cells: vec![&cell, &cell, &cell],
    };
    assert_eq!(batch.end_col(), 8);
}

#[test]
fn test_double_buffer_get_stats() {
    let mut buffer = DoubleBuffer::new(3, 3);
    buffer.swap();

    buffer.set_cell(0, 0, Cell::new(b'A'));
    buffer.set_cell(1, 1, Cell::new(b'B'));

    let stats = buffer.get_stats();
    assert_eq!(stats.total_cells, 9);
    assert_eq!(stats.changed_cells, 2);
    assert!(!stats.full_redraw);
}
