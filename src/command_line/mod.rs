//! Command line management
//! Handles rendering, cursor positioning, and command parsing for the command line input window

//! ## `command_line`/ Invariants
//!
//! - Command line rendering only displays state, never mutates it.
//! - Command line is only rendered when in Command mode.
//! - Cursor positioning is calculated based on window dimensions and content.
//! - Window dimensions are constrained to terminal size.

use crate::floating_window::{BorderChars, FloatingWindow, WindowPosition};
use crate::layer::Layer;
use crate::state::CommandLineWindowSettings;
use crate::viewport::Viewport;

pub mod executor;
pub mod parser;
pub mod registry;
pub mod settings;

/// Command line renderer
pub struct CommandLine;

impl CommandLine {
    /// Render the command line window to a layer and return cursor position information
    /// Returns `(window_row, window_col, cmd_width)` for cursor positioning
    ///
    /// This is the layer-based rendering method.
    pub fn render_to_layer(
        layer: &mut Layer,
        viewport: &Viewport,
        command_line: &str,
        default_border_chars: Option<BorderChars>,
        window_settings: &CommandLineWindowSettings,
    ) -> (u16, u16, usize) {
        // Calculate width based on settings: ratio of terminal width, clamped to min/max
        let cmd_width = ((viewport.visible_cols() as f64 * window_settings.width_ratio) as usize)
            .max(window_settings.min_width)
            .min(viewport.visible_cols());

        let cmd_window =
            FloatingWindow::new(WindowPosition::Center, cmd_width, window_settings.height)
                .with_border(window_settings.border)
                .with_reverse_video(window_settings.reverse_video);

        // Prepare content: prompt + command line
        let mut content_line = Vec::new();
        content_line.push(b':');
        content_line.extend_from_slice(command_line.as_bytes());

        // Render to layer
        cmd_window.render_with_border_chars(layer, &[content_line], default_border_chars);

        // Calculate window position for cursor positioning
        let window_pos = cmd_window.calculate_position(layer.rows() as u16, layer.cols() as u16);

        (window_pos.0, window_pos.1, cmd_width)
    }

    /// Calculate the cursor position within the command line window
    /// Returns (row, col) for cursor positioning
    #[must_use]
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
        let cursor_col = (content_start_col + 1 + command_line.len()).min(content_end_col);
        (content_row, cursor_col as u16)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
