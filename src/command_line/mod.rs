//! Command line management
//! Handles rendering and cursor positioning for the command line input window

/// ## command_line/ Invariants
///
/// - Command line rendering only displays state, never mutates it.
/// - Command line is only rendered when in Command mode.
/// - Cursor positioning is calculated based on window dimensions and content.
/// - Window dimensions are constrained to terminal size.

use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use crate::floating_window::{FloatingWindow, WindowPosition};

/// Command line renderer
pub struct CommandLine;

impl CommandLine {
    /// Render the command line window and return cursor position information
    /// Returns Some((window_pos, cmd_width)) if rendered, None otherwise
    pub fn render<T: TerminalBackend>(
        term: &mut T,
        viewport: &Viewport,
        command_line: &str,
    ) -> Result<Option<(u16, u16, usize)>, String> {
        // Use a reasonable width for the command line (e.g., 60% of terminal width)
        let cmd_width = (viewport.visible_cols() * 3 / 5).max(40).min(viewport.visible_cols());
        // Height 3: top border (1) + content (1) + bottom border (1)
        let cmd_window = FloatingWindow::new(WindowPosition::Center, cmd_width, 3)
            .with_border(true)
            .with_reverse_video(true);
        
        // Calculate window position for cursor positioning
        let size = term.get_size()?;
        let window_pos = cmd_window.calculate_position(size.rows, size.cols);
        
        cmd_window.render_single_line(term, ":", command_line)?;
        
        // Return (row, col, width) for cursor positioning
        Ok(Some((window_pos.0, window_pos.1, cmd_width)))
    }

    /// Calculate the cursor position within the command line window
    /// Returns (row, col) for cursor positioning
    pub fn calculate_cursor_position(
        window_pos: (u16, u16),
        cmd_width: usize,
        command_line: &str,
    ) -> (u16, u16) {
        let (window_row, window_col) = window_pos;
        // With border and height=3:
        // Row 0: top border
        // Row 1: content row (left border | content | right border)
        // Row 2: bottom border
        // Content area: window_col + 1 to window_col + cmd_width - 2 (inclusive)
        // Prompt ":" is at window_col + 1, command line starts at window_col + 2
        // Right border is at window_col + cmd_width - 1
        let content_row = window_row + 1; // Content is on the middle row
        let content_start_col = window_col as usize + 1; // After left border
        let content_end_col = window_col as usize + cmd_width - 2; // Before right border
        let cursor_col = (content_start_col + 1 + command_line.len())
            .min(content_end_col);
        (content_row, cursor_col as u16)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

