//! Software cursor rendering
//!
//! Two rendering strategies, chosen by mode:
//!
//! Normal / OperatorPending  ->  SOFTWARE BLOCK
//!   A compositor cell at the cursor position: same character, fg/bg inverted.
//!   The terminal cursor stays hidden.  No escape-sequence cursor at all.
//!
//! Insert / Command / Search / Rename / …  ->  TERMINAL BAR
//!   The terminal cursor is shown at the cursor position with the DECSCUSR
//!   "steady bar" shape (\e[6 q).  The terminal draws a thin vertical bar ON
//!   TOP of whatever character is in that cell — the character is never
//!   replaced, never hidden.  This is identical to how Neovim does it.

use crate::character::Character;
use crate::color::Color;
use crate::layer::Cell;
use crate::mode::Mode;

pub fn is_software_cursor(mode: Mode) -> bool {
    matches!(mode, Mode::Normal | Mode::OperatorPending)
}

pub struct SoftCursor;

impl SoftCursor {
    pub fn block_cell(
        underlying: Option<&Cell>,
        cursor_color: Option<Color>,
        editor_fg: Option<Color>,
        editor_bg: Option<Color>,
    ) -> Cell {
        let (under_fg, under_bg, content) = match underlying {
            Some(c) => (c.fg, c.bg, c.content),
            None => (None, None, Character::from(' ')),
        };

        let block_bg = cursor_color
            .or(under_fg)
            .or(editor_fg)
            .unwrap_or(Color::White);
        let block_fg = under_bg.or(editor_bg).unwrap_or(Color::Black);

        Cell::new(content).with_fg(block_fg).with_bg(block_bg)
    }
}

/// Animates the software cursor toward its logical target using exponential smoothing.
pub struct CursorAnimator {
    row: f64,
    col: f64,
    target_row: usize,
    target_col: usize,
    initialized: bool,
    animating: bool,
}

impl CursorAnimator {
    pub fn new() -> Self {
        Self {
            row: 0.0,
            col: 0.0,
            target_row: 0,
            target_col: 0,
            initialized: false,
            animating: false,
        }
    }

    pub fn set_target(&mut self, row: usize, col: usize) {
        if !self.initialized {
            self.row = row as f64;
            self.col = col as f64;
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

    pub fn snap_to(&mut self, row: usize, col: usize) {
        self.row = row as f64;
        self.col = col as f64;
        self.target_row = row;
        self.target_col = col;
        self.initialized = true;
        self.animating = false;
    }

    pub fn step(&mut self, factor: f64) {
        if !self.animating {
            return;
        }
        self.row += (self.target_row as f64 - self.row) * factor;
        self.col += (self.target_col as f64 - self.col) * factor;
        if (self.row - self.target_row as f64).abs() < 0.5
            && (self.col - self.target_col as f64).abs() < 0.5
        {
            self.row = self.target_row as f64;
            self.col = self.target_col as f64;
            self.animating = false;
        }
    }

    pub fn display_pos(&self) -> (usize, usize) {
        (self.row.round() as usize, self.col.round() as usize)
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }

    pub fn reset(&mut self) {
        self.initialized = false;
        self.animating = false;
    }
}

impl Default for CursorAnimator {
    fn default() -> Self {
        Self::new()
    }
}
