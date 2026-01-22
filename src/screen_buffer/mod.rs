//! Double-buffered screen buffer for efficient terminal rendering
//!
//! This module provides a generic double-buffering implementation that tracks
//! changes between frames and produces minimal diffs for rendering.
//!
//! ## screen_buffer/ Invariants
//!
//! - The buffer maintains two frames: current and previous
//! - Only cells that differ between frames are marked for rendering
//! - First frame always produces a full-screen diff
//! - Resize operations force a full redraw on next diff

use crate::color::Color;
use crate::layer::Cell;

/// Statistics about a rendered frame
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    /// Total cells in the buffer
    pub total_cells: usize,
    /// Cells that changed this frame
    pub changed_cells: usize,
    /// Whether this was a full redraw
    pub full_redraw: bool,
}

impl FrameStats {
    /// Calculate the percentage of cells that changed
    pub fn change_percentage(&self) -> f32 {
        if self.total_cells == 0 {
            0.0
        } else {
            (self.changed_cells as f32 / self.total_cells as f32) * 100.0
        }
    }
}

/// A change to a single cell that needs to be rendered
#[derive(Debug, Clone)]
pub struct CellChange<'a> {
    /// Row position (0-indexed)
    pub row: usize,
    /// Column position (0-indexed)
    pub col: usize,
    /// The cell content to render
    pub cell: &'a Cell,
}

/// A batch of consecutive cell changes on the same row
#[derive(Debug, Clone)]
pub struct CellBatch<'a> {
    /// Row position (0-indexed)
    pub row: usize,
    /// Starting column position (0-indexed)
    pub start_col: usize,
    /// The cells to render (consecutive)
    pub cells: Vec<&'a Cell>,
}

impl<'a> CellBatch<'a> {
    /// Get the ending column (exclusive)
    pub fn end_col(&self) -> usize {
        self.start_col + self.cells.len()
    }
}

/// Double-buffered screen buffer
///
/// Maintains current and previous frame buffers to compute minimal diffs
/// for efficient terminal rendering.
pub struct DoubleBuffer {
    /// Current frame buffer (flat)
    current: Vec<Cell>,
    /// Previous frame buffer (flat)
    previous: Vec<Cell>,
    /// Number of rows
    rows: usize,
    /// Number of columns
    cols: usize,
    /// Whether the next diff should be a full redraw
    force_full_redraw: bool,
}

impl DoubleBuffer {
    /// Create a new double buffer with the given dimensions
    pub fn new(rows: usize, cols: usize) -> Self {
        let size = rows * cols;
        let current = vec![Cell::empty(); size];
        let previous = vec![Cell::empty(); size];
        Self {
            current,
            previous,
            rows,
            cols,
            force_full_redraw: true, // First frame is always full
        }
    }

    /// Get the number of rows
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Get the number of columns
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Helper to get index from row/col
    #[inline]
    fn idx(&self, row: usize, col: usize) -> usize {
        row * self.cols + col
    }

    /// Get mutable access to the current buffer (logic adapter for existing code)
    /// Note: This reconstructs a Vec<Vec<Cell>> view which is expensive.
    /// Existing code expects &mut Vec<Vec<Cell>>.
    /// To support flattening without breaking massive API changes, we provide specific accessors
    /// and a temporary adapter if absolutely needed, OR update callsites.
    ///
    /// checking `src/layer/mod.rs` - `composite` accesses buffer via `current_mut() -> &mut Vec<Vec<Cell>>`
    /// We MUST update `LayerCompositor::composite` to use `set_cell` or direct slice access.
    /// For now, let's provide a way to get mutable slice.
    pub fn current_slice_mut(&mut self) -> &mut [Cell] {
        &mut self.current
    }

    /// Get read-only access to the current buffer
    pub fn current_slice(&self) -> &[Cell] {
        &self.current
    }

    /// Set a cell in the current buffer
    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) -> bool {
        if row < self.rows && col < self.cols {
            let idx = self.idx(row, col);
            self.current[idx] = cell;
            true
        } else {
            false
        }
    }

    /// Get a cell from the current buffer
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if row < self.rows && col < self.cols {
            let idx = self.idx(row, col);
            Some(&self.current[idx])
        } else {
            None
        }
    }

    /// Copy a 2D buffer into the current frame
    pub fn copy_from(&mut self, source: &[Vec<Cell>]) {
        for (row_idx, row) in source.iter().enumerate() {
            if row_idx >= self.rows {
                break;
            }
            let start = self.idx(row_idx, 0);
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx >= self.cols {
                    break;
                }
                self.current[start + col_idx] = cell.clone();
            }
        }
    }

    /// Resize the buffer to new dimensions
    /// Forces a full redraw on next diff
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        let new_size = new_rows * new_cols;
        let mut new_current = vec![Cell::empty(); new_size];
        let new_previous = vec![Cell::empty(); new_size];

        // Copy existing content where possible
        for r in 0..self.rows.min(new_rows) {
            for c in 0..self.cols.min(new_cols) {
                let old_idx = self.idx(r, c);
                let new_idx = r * new_cols + c;
                new_current[new_idx] = self.current[old_idx].clone();
            }
        }

        self.current = new_current;
        self.previous = new_previous;
        self.rows = new_rows;
        self.cols = new_cols;
        self.force_full_redraw = true;
    }

    /// Force a full redraw on the next diff
    pub fn invalidate(&mut self) {
        self.force_full_redraw = true;
    }

    /// Check if a full redraw is pending
    pub fn needs_full_redraw(&self) -> bool {
        self.force_full_redraw
    }

    /// Check if a specific cell has changed
    pub fn cell_changed(&self, row: usize, col: usize) -> bool {
        if self.force_full_redraw {
            return true;
        }
        if row >= self.rows || col >= self.cols {
            return false;
        }
        let idx = self.idx(row, col);
        self.current[idx] != self.previous[idx]
    }

    /// Iterate over all changed cells
    /// Returns individual cell changes (not batched)
    pub fn iter_changes(&self) -> impl Iterator<Item = CellChange<'_>> {
        let force = self.force_full_redraw;
        let cols = self.cols;
        self.current
            .iter()
            .zip(self.previous.iter())
            .enumerate()
            .filter_map(move |(i, (curr, prev))| {
                if force || curr != prev {
                    let row = i / cols;
                    let col = i % cols;
                    Some(CellChange {
                        row,
                        col,
                        cell: curr,
                    })
                } else {
                    None
                }
            })
    }

    /// Get batched changes for efficient rendering
    /// Groups consecutive changed cells on the same row
    pub fn get_batched_changes(&self) -> (Vec<CellBatch<'_>>, FrameStats) {
        let mut batches = Vec::new();
        let mut stats = FrameStats {
            total_cells: self.rows * self.cols,
            changed_cells: 0,
            full_redraw: self.force_full_redraw,
        };

        for row_idx in 0..self.rows {
            let row_start_idx = self.idx(row_idx, 0);
            let mut batch_start: Option<usize> = None;
            let mut batch_cells: Vec<&Cell> = Vec::new();

            for col_idx in 0..self.cols {
                let idx = row_start_idx + col_idx;
                let curr = &self.current[idx];
                let prev = &self.previous[idx];
                let changed = self.force_full_redraw || curr != prev;

                if changed {
                    stats.changed_cells += 1;
                    if batch_start.is_none() {
                        batch_start = Some(col_idx);
                    }
                    batch_cells.push(curr);
                } else if let Some(start) = batch_start {
                    // End of batch
                    batches.push(CellBatch {
                        row: row_idx,
                        start_col: start,
                        cells: std::mem::take(&mut batch_cells),
                    });
                    batch_start = None;
                }
            }

            // Flush remaining batch
            if let Some(start) = batch_start {
                batches.push(CellBatch {
                    row: row_idx,
                    start_col: start,
                    cells: batch_cells,
                });
            }
        }

        (batches, stats)
    }

    /// Swap buffers after rendering
    /// Copies current to previous and clears force_full_redraw
    pub fn swap(&mut self) {
        self.previous.clone_from(&self.current);
        self.force_full_redraw = false;
    }

    /// Clear the current buffer (fill with empty cells)
    pub fn clear(&mut self) {
        for cell in &mut self.current {
            *cell = Cell::empty();
        }
    }

    /// Get frame statistics for the current diff
    pub fn get_stats(&self) -> FrameStats {
        let mut stats = FrameStats {
            total_cells: self.rows * self.cols,
            changed_cells: 0,
            full_redraw: self.force_full_redraw,
        };

        if self.force_full_redraw {
            stats.changed_cells = stats.total_cells;
        } else {
            for (curr, prev) in self.current.iter().zip(self.previous.iter()) {
                if curr != prev {
                    stats.changed_cells += 1;
                }
            }
        }

        stats
    }

    /// Render the current buffer to the terminal using double buffering
    /// Only cells that changed since the last frame are rendered
    pub fn render_to_terminal<T: crate::term::TerminalBackend>(
        &mut self,
        term: &mut T,
    ) -> Result<FrameStats, String> {
        use crossterm::queue;
        use crossterm::style::ResetColor;

        // Get batched changes
        let (batches, stats) = self.get_batched_changes();

        if batches.is_empty() {
            return Ok(stats);
        }

        // Hide cursor during rendering
        term.hide_cursor()?;

        // Track current colors to minimize escape sequences
        let mut current_fg: Option<Color> = None;
        let mut current_bg: Option<Color> = None;
        let mut last_cursor_pos: Option<(usize, usize)> = None;

        for batch in batches {
            self.flush_cell_batch(
                term,
                &batch,
                &mut current_fg,
                &mut current_bg,
                &mut last_cursor_pos,
            )?;
        }

        // Reset colors at end
        let mut reset_buf = Vec::new();
        queue!(reset_buf, ResetColor).map_err(|e| format!("Failed to reset colors: {e}"))?;
        term.write(&reset_buf)?;

        // Swap buffers: copy current to previous for next frame
        self.swap();

        Ok(stats)
    }

    /// Flush a batch of consecutive changed cells to the terminal
    fn flush_cell_batch<T: crate::term::TerminalBackend>(
        &self,
        term: &mut T,
        batch: &CellBatch,
        current_fg: &mut Option<Color>,
        current_bg: &mut Option<Color>,
        last_cursor_pos: &mut Option<(usize, usize)>,
    ) -> Result<(), String> {
        use crossterm::queue;
        use crossterm::style::{ResetColor, SetBackgroundColor, SetForegroundColor};

        if batch.cells.is_empty() {
            return Ok(());
        }

        // Move cursor if not already at the right position
        let need_move = match last_cursor_pos {
            Some((last_row, last_col)) => *last_row != batch.row || *last_col != batch.start_col,
            None => true,
        };

        if need_move {
            term.move_cursor(batch.row as u16, batch.start_col as u16)?;
        }

        // Use String buffer for formatting Character
        let mut output = String::with_capacity(batch.cells.len() * 4);
        // Commands buffer for color changes
        let mut cmd_buf = Vec::new();

        for (i, cell) in batch.cells.iter().enumerate() {
            // Check if we need to change colors
            if cell.fg != *current_fg || cell.bg != *current_bg {
                // Flush current text output before color change
                if !output.is_empty() {
                    term.write(output.as_bytes())?;
                    output.clear();
                }

                cmd_buf.clear();

                let need_reset = (cell.fg.is_none() && current_fg.is_some())
                    || (cell.bg.is_none() && current_bg.is_some());

                if need_reset {
                    queue!(cmd_buf, ResetColor)
                        .map_err(|e| format!("Failed to reset colors: {e}"))?;
                    *current_fg = None;
                    *current_bg = None;
                }

                if cell.fg != *current_fg {
                    if let Some(fg) = cell.fg {
                        queue!(cmd_buf, SetForegroundColor(fg.to_crossterm()))
                            .map_err(|e| format!("Failed to set fg: {e}"))?;
                        *current_fg = Some(fg);
                    }
                }

                if cell.bg != *current_bg {
                    if let Some(bg) = cell.bg {
                        queue!(cmd_buf, SetBackgroundColor(bg.to_crossterm()))
                            .map_err(|e| format!("Failed to set bg: {e}"))?;
                        *current_bg = Some(bg);
                    }
                }

                term.write(&cmd_buf)?;
            }

            // Render Character to string buffer
            // We use cell.content.render(&mut output)
            cell.content
                .render(&mut output)
                .map_err(|e| format!("Failed to render character: {e}"))?;

            // Update last cursor position
            *last_cursor_pos = Some((batch.row, batch.start_col + i + 1));
        }

        // Flush remaining output
        if !output.is_empty() {
            term.write(output.as_bytes())?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
