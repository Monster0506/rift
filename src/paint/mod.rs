//! Intermediate paint representation between editor state and the render
//! backend. Decouples the shared render pipeline from `Layer`'s terminal
//! cell grid; see `render_abstraction.md`. `rasterize` is the terminal
//! renderer's own step, turning a `PaintFrame` into `Layer` cells.
//!
//! ## paint/ Invariants
//!
//! - PaintFrame construction never depends on `crate::term` or `crate::layer::Layer`.
//! - `PaintFrame::set_cell` preserves Layer::set_cell's overwrite-order semantics:
//!   replaying a frame's runs in order reproduces the same final cells as writing
//!   directly to a Layer in the same call order.

use crate::character::Character;
use crate::color::Color;
use crate::layer::{Cell, CellAttrs, Layer};

/// A run of characters sharing one style, in visual order starting at `col`.
/// Holds `Character` rather than `String` so raw non-UTF8 bytes and control
/// characters (see `Character::Byte`/`Character::Control`) survive losslessly.
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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaintFrame {
    pub rows: Vec<PaintRow>,
    pub cursor: Option<CursorPaint>,
    pub selections: Vec<SelectionPaint>,
}

impl PaintFrame {
    /// Create an empty frame with the given number of rows.
    pub fn new(rows: usize) -> Self {
        Self {
            rows: vec![PaintRow::default(); rows],
            cursor: None,
            selections: Vec::new(),
        }
    }

    /// Paint a single cell at `(row, col)`. Out-of-bounds rows are ignored,
    /// matching `Layer::set_cell`'s tolerance for out-of-bounds writes.
    /// Adjacent same-style writes coalesce onto the previous run; anything
    /// else opens a new run, so replay order (and therefore final content
    /// on overwrite) always matches the order `set_cell` was called in.
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
        paint_row.runs.push(TextRun {
            col,
            chars: vec![cell.content],
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
/// onto a `Layer`, in row/run/char order, via the same `Layer::set_cell`
/// path direct callers used before this frame existed.
pub fn rasterize(frame: &PaintFrame, layer: &mut Layer) {
    for (row, paint_row) in frame.rows.iter().enumerate() {
        for run in &paint_row.runs {
            for (i, &content) in run.chars.iter().enumerate() {
                layer.set_cell(
                    row,
                    run.col + i,
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
