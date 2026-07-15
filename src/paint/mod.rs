//! Intermediate paint representation between editor state and the render
//! backend; see render_abstraction.md. `rasterize` turns a frame into `Layer` cells.

use crate::character::Character;
use crate::color::Color;
use crate::layer::{Cell, CellAttrs, Layer};

/// A run of characters sharing one style, in visual order starting at `col`.
/// Holds `Character`, not `String`, so raw non-UTF8 bytes survive losslessly.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub col: usize,
    pub chars: Vec<Character>,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub attrs: CellAttrs,
}

/// One visual row's worth of styled runs, in the order they were painted.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaintRow {
    pub runs: Vec<TextRun>,
}

/// Terminal cursor shape. Mirrors `crate::term::CursorShape`'s variants
/// without depending on the `term` module, per the paint/ invariant above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    SteadyBlock,
    SteadyBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPaint {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPaint {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
}

/// A screen's worth of paint output, in row/col units (not yet pixels).
#[derive(Debug, Clone, Default)]
pub struct PaintFrame {
    pub rows: Vec<PaintRow>,
    pub cursor: Option<CursorPaint>,
    pub selections: Vec<SelectionPaint>,
    /// Pool of emptied `TextRun.chars` buffers reused across `reset()` calls
    /// instead of allocating fresh; excluded from `PartialEq` as an internal cache.
    char_buf_pool: Vec<Vec<Character>>,
}

impl PartialEq for PaintFrame {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.cursor == other.cursor
            && self.selections == other.selections
    }
}

impl PaintFrame {
    /// Create an empty frame with the given number of rows.
    pub fn new(rows: usize) -> Self {
        Self {
            rows: vec![PaintRow::default(); rows],
            cursor: None,
            selections: Vec::new(),
            char_buf_pool: Vec::new(),
        }
    }

    /// Clear this frame for reuse at `rows` rows, keeping row/run allocations
    /// and pooling emptied `chars` buffers instead of dropping them.
    pub fn reset(&mut self, rows: usize) {
        if self.rows.len() < rows {
            self.rows.resize_with(rows, PaintRow::default);
        } else {
            self.rows.truncate(rows);
        }
        for row in &mut self.rows {
            for mut run in row.runs.drain(..) {
                run.chars.clear();
                self.char_buf_pool.push(run.chars);
            }
        }
        self.cursor = None;
        self.selections.clear();
    }

    /// Clear a single row for reuse, pooling its runs' `chars` buffers -
    /// unlike `reset()`, leaves every other row untouched.
    pub fn reset_row(&mut self, row: usize) {
        let Some(paint_row) = self.rows.get_mut(row) else {
            return;
        };
        for mut run in paint_row.runs.drain(..) {
            run.chars.clear();
            self.char_buf_pool.push(run.chars);
        }
    }

    /// Paint a single cell at `(row, col)`; out-of-bounds rows are ignored.
    /// Adjacent same-style writes coalesce; anything else opens a new run, in call order.
    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) {
        let Some(paint_row) = self.rows.get_mut(row) else {
            return;
        };
        if let Some(last) = paint_row.runs.last_mut() {
            if last.col + last.chars.len() == col
                && last.fg == cell.fg
                && last.bg == cell.bg
                && last.attrs == cell.attrs
            {
                last.chars.push(cell.content);
                return;
            }
        }
        let mut chars = self.char_buf_pool.pop().unwrap_or_default();
        chars.push(cell.content);
        paint_row.runs.push(TextRun {
            col,
            chars,
            fg: cell.fg,
            bg: cell.bg,
            attrs: cell.attrs,
        });
    }

    /// Paint `text` starting at `(row, start_col)`, one column per char.
    /// Mirrors `Layer::write_str_colored`.
    pub fn write_str_colored(
        &mut self,
        row: usize,
        start_col: usize,
        text: &str,
        fg: Option<Color>,
        bg: Option<Color>,
    ) {
        for (i, ch) in text.chars().enumerate() {
            self.set_cell(row, start_col + i, Cell::from_char(ch).with_colors(fg, bg));
        }
    }
}

/// The terminal renderer's rasterization step: apply a `PaintFrame`'s runs
/// onto a `Layer`, in row/run/char order, via `Layer::set_cell`.
pub fn rasterize(frame: &PaintFrame, layer: &mut Layer) {
    rasterize_offset(frame, layer, 0, 0);
}

/// Like `rasterize`, but composites `frame` (local row/col coordinates)
/// into `layer` shifted by `row_offset`/`col_offset`.
pub fn rasterize_offset(
    frame: &PaintFrame,
    layer: &mut Layer,
    row_offset: usize,
    col_offset: usize,
) {
    for (row, paint_row) in frame.rows.iter().enumerate() {
        for run in &paint_row.runs {
            for (i, &content) in run.chars.iter().enumerate() {
                layer.set_cell(
                    row + row_offset,
                    run.col + i + col_offset,
                    Cell {
                        content,
                        fg: run.fg,
                        bg: run.bg,
                        attrs: run.attrs,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
