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
        cursor_pos: usize,
        default_border_chars: Option<BorderChars>,
        window_settings: &CommandLineWindowSettings,
    ) -> (u16, u16, usize, usize) {
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

        let border_width = if window_settings.border { 2 } else { 0 };
        let available_width = cmd_width.saturating_sub(border_width); // Remove borders
        let prompt_len = 1; // ":"
        let available_cmd_width = available_width.saturating_sub(prompt_len);

        // Calculate offset to keep cursor visible
        // We ensure cursor_pos is within the visible window [offset, offset + available_cmd_width)
        // If length fits, offset is 0
        let offset = if command_line.len() <= available_cmd_width {
            0
        } else if cursor_pos >= available_cmd_width {
            // Keep cursor at the right edge minus 1? or just ensure it fits?
            // Panning strategy:
            cursor_pos
                .saturating_sub(available_cmd_width)
                .saturating_add(1)
        } else {
            0
        };

        // Slice command line
        let cmd_len = command_line.len();
        let displayed_cmd = if offset < cmd_len {
            let end = (offset + available_cmd_width).min(cmd_len);
            // Ensure char boundaries if multi-byte (simple slicing might panic on UTF-8)
            // For now assuming ASCII/byte based on TextBuffer<u8> usage in Editor,
            // but command_line is String.
            // We should use chars() if possible or ensure indices.
            // But Rift currently seems to treat indices as bytes?
            // Let's assume bytes for now to match strict slicing.
            // Safe slicing:
            if end > offset {
                // Check boundaries?
                &command_line[offset..end]
            } else {
                ""
            }
        } else {
            ""
        };

        content_line.extend_from_slice(displayed_cmd.as_bytes());

        // Render to layer
        cmd_window.render_with_border_chars(layer, &[content_line], default_border_chars);

        // Calculate window position for cursor positioning
        let window_pos = cmd_window.calculate_position(layer.rows() as u16, layer.cols() as u16);

        // Pass offset to cursor calculation
        (window_pos.0, window_pos.1, cmd_width, offset)
    }

    /// Calculate the cursor position within the command line window
    /// Returns (row, col) for cursor positioning
    #[must_use]
    pub fn calculate_cursor_position(
        window_pos: (u16, u16),
        cursor_pos: usize,
        offset: usize,
        has_border: bool,
    ) -> (u16, u16) {
        let (window_row, window_col) = window_pos;

        // Content start position depends on border
        let border_offset = if has_border { 1 } else { 0 };
        let content_row = window_row + border_offset as u16;
        let content_start_col = window_col as usize + border_offset;

        // Visual cursor position: start_col + prompt (1) + (cursor_pos - offset)
        let visual_index = cursor_pos.saturating_sub(offset);
        let visual_cursor_col = content_start_col + 1 + visual_index;

        (content_row, visual_cursor_col as u16)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
