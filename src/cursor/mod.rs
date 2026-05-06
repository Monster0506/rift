//! Software cursor rendering
//!
//! Two rendering strategies, chosen by mode:
//!
//! Normal / OperatorPending  →  SOFTWARE BLOCK
//!   A compositor cell at the cursor position: same character, fg/bg inverted.
//!   The terminal cursor stays hidden.  No escape-sequence cursor at all.
//!
//! Insert / Command / Search / Rename / …  →  TERMINAL BAR
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
