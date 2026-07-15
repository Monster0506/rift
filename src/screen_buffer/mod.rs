//! Double-buffered screen buffer: tracks current/previous frames and produces
//! minimal diffs for terminal rendering; first frame and resizes force a full redraw.

use crate::character::Character;
use crate::color::Color;
use crate::layer::{Cell, Rect};

/// A single text attribute toggled by a `StyleOp::SetAttr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrKind {
    Bold,
    Italic,
    Underline,
    Strike,
    Reverse,
}

/// A terminal-agnostic style change, emitted by the diff serializer below
/// and turned into escape codes by whichever backend consumes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleOp {
    ResetColor,
    SetForeground(Color),
    SetBackground(Color),
    SetAttr(AttrKind, bool),
    ResetAttrs,
}

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
pub struct CellBatch {
    /// Row position (0-indexed)
    pub row: usize,
    /// Starting column position (0-indexed)
    pub start_col: usize,
    /// The cells to render (consecutive), owned rather than `Vec<&Cell>` so
    /// it can be pooled across frames instead of allocated fresh (see `cell_batch_pool`).
    pub cells: Vec<Cell>,
}

impl CellBatch {
    /// Get the ending column (exclusive)
    pub fn end_col(&self) -> usize {
        self.start_col + self.cells.len()
    }
}

/// Maintains current and previous frame buffers to compute minimal diffs for
/// efficient terminal rendering.
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

    /// Scratch buffers reused across `render_to_terminal` calls instead of
    /// allocating fresh per cell-batch - see `flush_cell_batch`.
    scratch_output: String,
    scratch_cmd_buf: Vec<u8>,
    scratch_ops: Vec<StyleOp>,
    /// Pool of emptied `CellBatch.cells` buffers, returned by
    /// `render_to_terminal` and reused by `get_batched_changes`.
    cell_batch_pool: Vec<Vec<Cell>>,
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
            scratch_output: String::new(),
            scratch_cmd_buf: Vec::new(),
            scratch_ops: Vec::new(),
            cell_batch_pool: Vec::new(),
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

    /// Mutable access to the flat current buffer, for callers (e.g.
    /// `LayerCompositor::composite`) that need direct slice access.
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
                self.current[start + col_idx] = crate::perf_clone!(cell.clone());
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
                new_current[new_idx] = crate::perf_clone!(self.current[old_idx].clone());
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
    pub fn get_batched_changes(&mut self) -> (Vec<CellBatch>, FrameStats) {
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

        // Each open batch pulls from `cell_batch_pool` and moves straight into
        // the `CellBatch` when closed - no per-batch clone needed.
        let mut current_cells: Option<Vec<Cell>> = None;

        for row_idx in start_row..=end_row {
            // Ensure bounds
            if row_idx >= self.rows {
                break;
            }

            let row_start_idx = self.idx(row_idx, 0);
            let mut batch_start: Option<usize> = None;

            // Optimization: Only scan potentially dirty columns
            for col_idx in start_col..=end_col {
                if col_idx >= self.cols {
                    break;
                }

                let idx = row_start_idx + col_idx;
                let curr = &self.current[idx];
                let prev = &self.previous[idx];
                let mut changed = self.force_full_redraw || curr != prev;

                // Force-include the tail column of a multi-column char (e.g.
                // padding after ^M) so stale terminal output is overwritten.
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
                        current_cells = Some(self.cell_batch_pool.pop().unwrap_or_default());
                    }
                    current_cells.as_mut().unwrap().push(curr.clone());
                } else if let Some(start) = batch_start {
                    // End of batch
                    batches.push(CellBatch {
                        row: row_idx,
                        start_col: start,
                        cells: current_cells.take().unwrap(),
                    });
                    batch_start = None;
                }
            }

            // Flush remaining batch
            if let Some(start) = batch_start {
                batches.push(CellBatch {
                    row: row_idx,
                    start_col: start,
                    cells: current_cells.take().unwrap(),
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
                    self.previous[dst + c] = crate::perf_clone!(self.previous[src + c].clone());
                }
            }
            for r in bottom - k + 1..=bottom {
                self.previous[r * self.cols..(r + 1) * self.cols].fill(Cell::empty());
            }
        } else {
            for r in (top + k..=bottom).rev() {
                let (dst, src) = (r * self.cols, (r - k) * self.cols);
                for c in 0..self.cols {
                    self.previous[dst + c] = crate::perf_clone!(self.previous[src + c].clone());
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
        // Taken out (not borrowed) so get_batched_changes's &self borrow never
        // overlaps a &mut self.scratch_* access; put back before returning.
        let mut output = std::mem::take(&mut self.scratch_output);
        let mut cmd_buf = std::mem::take(&mut self.scratch_cmd_buf);
        let mut ops = std::mem::take(&mut self.scratch_ops);

        // Get batched changes
        let (batches, stats) = {
            crate::perf_span!("render_diff", crate::perf::PerfFields::default());
            self.get_batched_changes()
        };

        if batches.is_empty() {
            self.scratch_output = output;
            self.scratch_cmd_buf = cmd_buf;
            self.scratch_ops = ops;
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

        let result = (|| -> Result<(), String> {
            for mut batch in batches {
                Self::flush_cell_batch(
                    term,
                    &batch,
                    &mut current_fg,
                    &mut current_bg,
                    &mut current_attrs,
                    &mut last_cursor_pos,
                    &mut output,
                    &mut cmd_buf,
                    &mut ops,
                )?;
                batch.cells.clear();
                self.cell_batch_pool.push(batch.cells);
            }
            Ok(())
        })();

        self.scratch_output = output;
        self.scratch_cmd_buf = cmd_buf;
        self.scratch_ops = ops;
        result?;

        // Reset colors and attributes at end
        let mut reset_ops = vec![StyleOp::ResetColor];
        if !current_attrs.is_empty() {
            reset_ops.push(StyleOp::ResetAttrs);
        }
        let mut reset_buf = Vec::new();
        crate::term::crossterm::encode_style_ops(&reset_ops, &mut reset_buf)?;
        term.write(&reset_buf)?;

        term.flush().map_err(|e| format!("Failed to flush: {e}"))?;

        // Swap buffers: copy current to previous for next frame
        self.swap();

        Ok(stats)
    }

    /// Flush a batch of consecutive changed cells to the terminal; `output`/
    /// `cmd_buf`/`ops` are caller-owned scratch space, cleared not reallocated.
    #[allow(clippy::too_many_arguments)]
    fn flush_cell_batch<T: crate::term::TerminalBackend>(
        term: &mut T,
        batch: &CellBatch,
        current_fg: &mut Option<Color>,
        current_bg: &mut Option<Color>,
        current_attrs: &mut crate::layer::CellAttrs,
        last_cursor_pos: &mut Option<(usize, usize)>,
        output: &mut String,
        cmd_buf: &mut Vec<u8>,
        ops: &mut Vec<StyleOp>,
    ) -> Result<(), String> {
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

        output.clear();
        cmd_buf.clear();
        ops.clear();

        for cell in batch.cells.iter() {
            // Check if we need to change colors or attributes
            if cell.fg != *current_fg || cell.bg != *current_bg || cell.attrs != *current_attrs {
                // Flush current text output before the style change
                if !output.is_empty() {
                    term.write(output.as_bytes())?;
                    output.clear();
                }

                ops.clear();

                let need_reset = (cell.fg.is_none() && current_fg.is_some())
                    || (cell.bg.is_none() && current_bg.is_some());

                if need_reset {
                    ops.push(StyleOp::ResetColor);
                    *current_fg = None;
                    *current_bg = None;
                }

                if cell.fg != *current_fg {
                    if let Some(fg) = cell.fg {
                        ops.push(StyleOp::SetForeground(fg));
                        *current_fg = Some(fg);
                    }
                }

                if cell.bg != *current_bg {
                    if let Some(bg) = cell.bg {
                        ops.push(StyleOp::SetBackground(bg));
                        *current_bg = Some(bg);
                    }
                }

                // Emit attribute deltas with per-attribute on/off so colors are
                // unaffected (avoids a full SGR reset).
                if cell.attrs != *current_attrs {
                    let a = cell.attrs;
                    let c = *current_attrs;
                    let set = |ops: &mut Vec<StyleOp>, on: bool, was: bool, kind: AttrKind| {
                        if on != was {
                            ops.push(StyleOp::SetAttr(kind, on));
                        }
                    };
                    set(ops, a.bold, c.bold, AttrKind::Bold);
                    set(ops, a.italic, c.italic, AttrKind::Italic);
                    set(ops, a.underline, c.underline, AttrKind::Underline);
                    set(ops, a.strike, c.strike, AttrKind::Strike);
                    set(ops, a.reverse, c.reverse, AttrKind::Reverse);
                    *current_attrs = cell.attrs;
                }

                cmd_buf.clear();
                crate::term::crossterm::encode_style_ops(ops, cmd_buf)?;
                term.write(cmd_buf)?;
            }

            // Render Character to string buffer
            cell.content
                .render(output)
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
