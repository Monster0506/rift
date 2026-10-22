//! Software cursor: Normal/OperatorPending inverts a compositor cell (terminal
//! cursor hidden); Insert/Command/Search/Rename shows a real DECSCUSR bar.

use crate::character::Character;
use crate::color::Color;
use crate::layer::Cell;
use crate::mode::Mode;
use crate::num::{f64_to_usize, usize_to_f64};

#[must_use]
pub(crate) const fn is_software_cursor(mode: Mode) -> bool {
    matches!(mode, Mode::Normal | Mode::OperatorPending)
}

pub(crate) struct SoftCursor;

impl SoftCursor {
    #[must_use]
    pub(crate) fn block_cell(
        underlying: Option<&Cell>,
        cursor_color: Option<Color>,
        editor_fg: Option<Color>,
        editor_background: Option<Color>,
    ) -> Cell {
        let (under_fg, under_background, content) = underlying.map_or_else(
            || (None, None, Character::from(' ')),
            |c| (c.fg, c.bg, c.content),
        );

        let block_bg = cursor_color
            .or(under_fg)
            .or(editor_fg)
            .unwrap_or(Color::White);
        let block_text = under_background
            .or(editor_background)
            .unwrap_or(Color::Black);

        Cell::new(content).with_fg(block_text).with_bg(block_bg)
    }
}

/// Animates the software cursor toward its logical target using exponential smoothing.
pub(crate) struct CursorAnimator {
    row: f64,
    col: f64,
    target_row: usize,
    target_col: usize,
    initialized: bool,
    animating: bool,
}

// Screen coordinates stay far below 2^52, so the f64 conversion is exact.
fn coord(v: usize) -> f64 {
    usize_to_f64(v)
}

impl CursorAnimator {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            row: 0.0,
            col: 0.0,
            target_row: 0,
            target_col: 0,
            initialized: false,
            animating: false,
        }
    }

    pub(crate) fn set_target(&mut self, row: usize, col: usize) {
        if !self.initialized {
            self.row = coord(row);
            self.col = coord(col);
            self.target_row = row;
            self.target_col = col;
            self.initialized = true;
            return;
        }
        if row != self.target_row || col != self.target_col {
            self.target_row = row;
            self.target_col = col;
            self.animating = true;
        }
    }

    pub(crate) fn snap_to(&mut self, row: usize, col: usize) {
        self.row = coord(row);
        self.col = coord(col);
        self.target_row = row;
        self.target_col = col;
        self.initialized = true;
        self.animating = false;
    }

    pub(crate) fn step(&mut self, factor: f64) {
        if !self.animating {
            return;
        }
        self.row += (coord(self.target_row) - self.row) * factor;
        self.col += (coord(self.target_col) - self.col) * factor;
        if (self.row - coord(self.target_row)).abs() < 0.5
            && (self.col - coord(self.target_col)).abs() < 0.5
        {
            self.row = coord(self.target_row);
            self.col = coord(self.target_col);
            self.animating = false;
        }
    }

    #[must_use]
    pub(crate) fn display_pos(&self) -> (usize, usize) {
        (
            f64_to_usize(self.row.round()),
            f64_to_usize(self.col.round()),
        )
    }

    #[must_use]
    pub(crate) const fn is_animating(&self) -> bool {
        self.animating
    }

    pub(crate) const fn reset(&mut self) {
        self.initialized = false;
        self.animating = false;
    }
}

impl Default for CursorAnimator {
    fn default() -> Self {
        Self::new()
    }
}
