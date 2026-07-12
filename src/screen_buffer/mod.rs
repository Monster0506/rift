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

use crate::character::Character;
use crate::color::Color;
use crate::layer::{Cell, Rect};

fn cell_visual_width(cell: &Cell) -> usize {
    match &cell.content {
        Character::Control(_) => 2,
        Character::Byte(_) => 4,
        _ => 1,
    }
}

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
    /// Dirty rectangle for the current frame
    frame_dirty_rect: Option<Rect>,
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
            frame_dirty_rect: None,
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

            // Update frame dirty rect
            let rect = Rect::new(row, col, row, col);
            if let Some(existing) = self.frame_dirty_rect {
                self.frame_dirty_rect = Some(existing.union(&rect));
            } else {
                self.frame_dirty_rect = Some(rect);
            }
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
        // Mark full screen dirty on copy
        self.frame_dirty_rect = Some(Rect::new(
            0,
            0,
            self.rows.saturating_sub(1),
            self.cols.saturating_sub(1),
        ));
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
        self.frame_dirty_rect = None; // Reset dirty rect, full redraw takes precedence
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

        // Determine scan range
        let (start_row, end_row, start_col, end_col) = if self.force_full_redraw {
            (
                0,
                self.rows.saturating_sub(1),
                0,
                self.cols.saturating_sub(1),
            )
        } else if let Some(rect) = self.frame_dirty_rect {
            (rect.start_row, rect.end_row, rect.start_col, rect.end_col)
        } else {
            // Nothing dirty
            return (batches, stats);
        };

        for row_idx in start_row..=end_row {
            // Ensure bounds
            if row_idx >= self.rows {
                break;
            }

            let row_start_idx = self.idx(row_idx, 0);
            let mut batch_start: Option<usize> = None;
            let mut batch_cells: Vec<&Cell> = Vec::new();

            // Optimization: Only scan potentially dirty columns
            for col_idx in start_col..=end_col {
                if col_idx >= self.cols {
                    break;
                }

                let idx = row_start_idx + col_idx;
                let curr = &self.current[idx];
                let prev = &self.previous[idx];
                let mut changed = self.force_full_redraw || curr != prev;

                // Force-include the tail column of any multi-column character
                // (e.g. the padding space after ^M) so stale terminal output
                // from the previous render is overwritten.
                if !changed && col_idx > start_col {
                    let prev_idx = row_start_idx + col_idx - 1;
                    if cell_visual_width(&self.previous[prev_idx]) > 1
                        || cell_visual_width(&self.current[prev_idx]) > 1
                    {
                        changed = true;
                    }
                }

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

    /// Scroll rows `top..=bottom` by `delta` (positive = content moved up) via
    /// the terminal's scroll region; false means fall back to the plain diff.
    pub fn apply_scroll<T: crate::term::TerminalBackend>(
        &mut self,
        term: &mut T,
        top: usize,
        bottom: usize,
        delta: isize,
    ) -> Result<bool, String> {
        let k = delta.unsigned_abs();
        if self.force_full_redraw
            || delta == 0
            || bottom >= self.rows
            || top >= bottom
            || k > bottom - top
        {
            return Ok(false);
        }

        // Probe the shifted region's edge rows: the centered cursor (and any
        // cursor-line styling) sits mid-region, so edges verify a real shift.
        let (lo, hi) = if delta > 0 {
            (top, bottom - k)
        } else {
            (top + k, bottom)
        };
        let row_shifted = |r: usize| {
            let s = (r as isize + delta) as usize;
            self.current[r * self.cols..(r + 1) * self.cols]
                == self.previous[s * self.cols..(s + 1) * self.cols]
        };
        if !row_shifted(lo) || !row_shifted(hi) {
            return Ok(false);
        }

        // Reset colors first: cells the terminal blanks in use the current
        // background, which must match the Cell::empty() model below.
        let mut esc = format!("\x1b[0m\x1b[{};{}r", top + 1, bottom + 1);
        if delta > 0 {
            esc.push_str(&format!("\x1b[{}S", k));
        } else {
            esc.push_str(&format!("\x1b[{}T", k));
        }
        esc.push_str("\x1b[r");
        term.write(esc.as_bytes())?;

        // Shift the previous-frame model the same way the terminal moved.
        if delta > 0 {
            for r in top..=bottom - k {
                let (dst, src) = (r * self.cols, (r + k) * self.cols);
                for c in 0..self.cols {
                    self.previous[dst + c] = self.previous[src + c].clone();
                }
            }
            for r in bottom - k + 1..=bottom {
                self.previous[r * self.cols..(r + 1) * self.cols].fill(Cell::empty());
            }
        } else {
            for r in (top + k..=bottom).rev() {
                let (dst, src) = (r * self.cols, (r - k) * self.cols);
                for c in 0..self.cols {
                    self.previous[dst + c] = self.previous[src + c].clone();
                }
            }
            for r in top..top + k {
                self.previous[r * self.cols..(r + 1) * self.cols].fill(Cell::empty());
            }
        }

        // Everything in the region is fair game for the diff scan.
        let region = Rect::new(top, 0, bottom, self.cols.saturating_sub(1));
        self.frame_dirty_rect = Some(match self.frame_dirty_rect {
            Some(existing) => existing.union(&region),
            None => region,
        });
        Ok(true)
    }

    /// Swap buffers after rendering
    /// Copies current to previous and clears force_full_redraw
    pub fn swap(&mut self) {
        self.previous.clone_from(&self.current);
        self.force_full_redraw = false;
        self.frame_dirty_rect = None;
    }

    /// Clear the current buffer (fill with empty cells)
    pub fn clear(&mut self) -> Result<(), String> {
        Err(
            "Use clear_cell or specialized method. clear() ambiguous on buffering strategy"
                .to_string(),
        )
    }

    /// Clear content (fill with empty)
    pub fn clear_content(&mut self) {
        for cell in &mut self.current {
            *cell = Cell::empty();
        }
        self.frame_dirty_rect = Some(Rect::new(
            0,
            0,
            self.rows.saturating_sub(1),
            self.cols.saturating_sub(1),
        ));
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
        let (batches, stats) = {
            crate::perf_span!("render_diff", crate::perf::PerfFields::default());
            self.get_batched_changes()
        };

        if batches.is_empty() {
            return Ok(stats);
        }

        crate::perf_span!("ansi_serialize", crate::perf::PerfFields::default());

        // Hide cursor during rendering
        term.hide_cursor()?;

        // Track current colors/attrs to minimize escape sequences
        let mut current_fg: Option<Color> = None;
        let mut current_bg: Option<Color> = None;
        let mut current_attrs = crate::layer::CellAttrs::default();
        let mut last_cursor_pos: Option<(usize, usize)> = None;

        for batch in batches {
            self.flush_cell_batch(
                term,
                &batch,
                &mut current_fg,
                &mut current_bg,
                &mut current_attrs,
                &mut last_cursor_pos,
            )?;
        }

        // Reset colors and attributes at end
        let mut reset_buf = Vec::new();
        queue!(reset_buf, ResetColor).map_err(|e| format!("Failed to reset colors: {e}"))?;
        if !current_attrs.is_empty() {
            use crossterm::style::{Attribute, SetAttribute};
            queue!(reset_buf, SetAttribute(Attribute::Reset))
                .map_err(|e| format!("Failed to reset attrs: {e}"))?;
        }
        term.write(&reset_buf)?;

        term.flush().map_err(|e| format!("Failed to flush: {e}"))?;

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
        current_attrs: &mut crate::layer::CellAttrs,
        last_cursor_pos: &mut Option<(usize, usize)>,
    ) -> Result<(), String> {
        use crossterm::queue;
        use crossterm::style::{
            Attribute, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
        };

        if batch.cells.is_empty() {
            return Ok(());
        }

        let mut terminal_col = batch.start_col;

        // Move cursor if not already at the right position
        let need_move = match last_cursor_pos {
            Some((last_row, last_col)) => *last_row != batch.row || *last_col != terminal_col,
            None => true,
        };

        if need_move {
            term.move_cursor(batch.row as u16, batch.start_col as u16)?;
        }

        // Use String buffer for formatting Character
        let mut output = String::with_capacity(batch.cells.len() * 4);
        // Commands buffer for color changes
        let mut cmd_buf = Vec::new();

        for cell in batch.cells.iter() {
            // Check if we need to change colors or attributes
            if cell.fg != *current_fg || cell.bg != *current_bg || cell.attrs != *current_attrs {
                // Flush current text output before the style change
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
                        queue!(
                            cmd_buf,
                            SetForegroundColor(crate::term::crossterm::color_to_crossterm(fg))
                        )
                        .map_err(|e| format!("Failed to set fg: {e}"))?;
                        *current_fg = Some(fg);
                    }
                }

                if cell.bg != *current_bg {
                    if let Some(bg) = cell.bg {
                        queue!(
                            cmd_buf,
                            SetBackgroundColor(crate::term::crossterm::color_to_crossterm(bg))
                        )
                        .map_err(|e| format!("Failed to set bg: {e}"))?;
                        *current_bg = Some(bg);
                    }
                }

                // Emit attribute deltas with per-attribute on/off so colors are
                // unaffected (avoids a full SGR reset).
                if cell.attrs != *current_attrs {
                    let a = cell.attrs;
                    let c = *current_attrs;
                    let mut set = |on: bool, was: bool, yes: Attribute, no: Attribute| {
                        if on != was {
                            let _ = queue!(cmd_buf, SetAttribute(if on { yes } else { no }));
                        }
                    };
                    set(a.bold, c.bold, Attribute::Bold, Attribute::NormalIntensity);
                    set(a.italic, c.italic, Attribute::Italic, Attribute::NoItalic);
                    set(
                        a.underline,
                        c.underline,
                        Attribute::Underlined,
                        Attribute::NoUnderline,
                    );
                    set(
                        a.strike,
                        c.strike,
                        Attribute::CrossedOut,
                        Attribute::NotCrossedOut,
                    );
                    set(
                        a.reverse,
                        c.reverse,
                        Attribute::Reverse,
                        Attribute::NoReverse,
                    );
                    *current_attrs = cell.attrs;
                }

                term.write(&cmd_buf)?;
            }

            // Render Character to string buffer
            cell.content
                .render(&mut output)
                .map_err(|e| format!("Failed to render character: {e}"))?;

            terminal_col += cell_visual_width(cell);
            *last_cursor_pos = Some((batch.row, terminal_col));
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
