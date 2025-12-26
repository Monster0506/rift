//! Status bar management
//! Handles rendering and formatting of the editor status bar
//!
//! ## status/ Invariants
//!
//! - Status content is derived entirely from editor state.
//! - Status rendering does not influence editor behavior.
//! - Status display is optional and failure-tolerant.
//! - Status never consumes input or commands.

use crate::color::Color;
use crate::command::Command;
use crate::key::Key;
use crate::layer::Layer;
use crate::mode::Mode;
use crate::state::State;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;

/// Status bar renderer
pub struct StatusBar;

impl StatusBar {
    /// Render the status bar to the terminal
    pub fn render<T: TerminalBackend>(
        term: &mut T,
        viewport: &Viewport,
        current_mode: Mode,
        pending_key: Option<Key>,
        state: &State,
    ) -> Result<(), String> {
        let status_row = viewport.visible_rows().saturating_sub(1);
        term.move_cursor(status_row as u16, 0)?;

        // If status line is disabled, just clear the line and return
        if !state.settings.status_line.show_status_line {
            // Clear the entire status line
            for _ in 0..viewport.visible_cols() {
                term.write(b" ")?;
            }
            return Ok(());
        }

        // Invert colors for status bar (reverse video) if enabled
        if state.settings.status_line.reverse_video {
            term.write(b"\x1b[7m")?;
        }

        // In command mode, show colon prompt and fill rest with spaces
        if current_mode == Mode::Command {
            let mode_str = Self::format_mode(current_mode);
            term.write(mode_str.as_bytes())?;

            // Fill rest of line with spaces
            let remaining_cols = viewport.visible_cols().saturating_sub(mode_str.len());
            for _ in 0..remaining_cols {
                term.write(b" ")?;
            }
        } else {
            // Mode indicator
            let mode_str = Self::format_mode(current_mode);
            term.write(mode_str.as_bytes())?;

            // Pending key indicator
            let pending_str = if let Some(key) = pending_key {
                format!(" [{}]", Self::format_key(key))
            } else {
                String::new()
            };
            if !pending_str.is_empty() {
                term.write(pending_str.as_bytes())?;
            }

            // Debug information (if debug mode is enabled)
            let debug_str = if state.debug_mode {
                Self::format_debug_info(state, current_mode)
            } else {
                String::new()
            };

            // Calculate layout
            let mode_len = mode_str.len();
            let pending_len = pending_str.len();
            let file_len = state.file_name.len();
            let used_cols = mode_len + pending_len;
            let available_cols = viewport.visible_cols().saturating_sub(used_cols);

            // In debug mode, show debug info. In normal mode, show filename on right
            if state.debug_mode {
                // Format debug info with proper spacing
                let (debug_display, debug_len) = if debug_str.is_empty() {
                    (String::new(), 0)
                } else {
                    let truncated = if debug_str.len() <= available_cols {
                        debug_str
                    } else {
                        format!("{}...", &debug_str[..available_cols.saturating_sub(3)])
                    };
                    let spacing = available_cols.saturating_sub(truncated.len());
                    let spaced = format!("{}{}", " ".repeat(spacing), truncated);
                    (spaced, truncated.len() + spacing)
                };

                // Write debug info
                if !debug_display.is_empty() {
                    term.write(debug_display.as_bytes())?;
                }

                // Fill rest of line with spaces
                let total_used = mode_len + pending_len + debug_len;
                let remaining_cols = viewport.visible_cols().saturating_sub(total_used);

                for _ in 0..remaining_cols {
                    term.write(b" ")?;
                }
            } else {
                // Normal mode: show filename on the right (if enabled in settings)
                if state.settings.status_line.show_filename {
                    if file_len <= available_cols {
                        // Right-align filename
                        let spacing = available_cols.saturating_sub(file_len);
                        for _ in 0..spacing {
                            term.write(b" ")?;
                        }
                        term.write(state.file_name.as_bytes())?;
                    } else {
                        // Filename too long, truncate it
                        let truncated = if available_cols > 3 {
                            format!(
                                "...{}",
                                &state.file_name
                                    [state.file_name.len().saturating_sub(available_cols - 3)..]
                            )
                        } else {
                            String::new()
                        };
                        let spacing = available_cols.saturating_sub(truncated.len());
                        for _ in 0..spacing {
                            term.write(b" ")?;
                        }
                        term.write(truncated.as_bytes())?;
                    }
                } else {
                    // Filename display disabled, fill with spaces
                    for _ in 0..available_cols {
                        term.write(b" ")?;
                    }
                }
            }
        }

        // Reset colors (if reverse video was enabled)
        if state.settings.status_line.reverse_video {
            term.write(b"\x1b[0m")?;
        }

        Ok(())
    }

    /// Format mode name for display
    #[must_use]
    pub fn format_mode(mode: Mode) -> &'static str {
        match mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Command => ":",
        }
    }

    /// Format key for display
    #[must_use]
    pub fn format_key(key: Key) -> String {
        match key {
            Key::Char(ch) => {
                if (32..127).contains(&ch) {
                    format!("{}", ch as char)
                } else {
                    format!("\\x{ch:02x}")
                }
            }
            Key::Ctrl(ch) => format!("Ctrl+{}", (ch as char).to_uppercase()),
            Key::ArrowUp => "↑".to_string(),
            Key::ArrowDown => "↓".to_string(),
            Key::ArrowLeft => "←".to_string(),
            Key::ArrowRight => "→".to_string(),
            Key::Backspace => "Backspace".to_string(),
            Key::Delete => "Delete".to_string(),
            Key::Enter => "Enter".to_string(),
            Key::Escape => "Esc".to_string(),
            Key::Tab => "Tab".to_string(),
            Key::Home => "Home".to_string(),
            Key::End => "End".to_string(),
            Key::PageUp => "PageUp".to_string(),
            Key::PageDown => "PageDown".to_string(),
        }
    }

    /// Format debug information string
    fn format_debug_info(state: &State, current_mode: Mode) -> String {
        let mut parts = Vec::new();

        // Filepath (in debug mode, show full path)
        if let Some(path) = &state.file_path {
            parts.push(format!("File: {}", path));
        }

        // Last keypress
        if let Some(key) = state.last_keypress {
            parts.push(format!("Last: {}", Self::format_key(key)));
        }

        // In insert mode, show the byte being inserted
        if current_mode == Mode::Insert {
            if let Some(Command::InsertByte(b)) = state.last_command {
                let byte_str = if b == b'\t' {
                    "\\t".to_string()
                } else if b == b'\n' {
                    "\\n".to_string()
                } else if (32..127).contains(&b) {
                    format!("'{}'", b as char)
                } else {
                    format!("\\x{b:02x}")
                };
                parts.push(format!("Insert: {byte_str} (0x{b:02x})"));
            }
        }

        // Cursor position (1-indexed for display)
        parts.push(format!(
            "Pos: {}:{}",
            state.cursor_pos.0 + 1,
            state.cursor_pos.1 + 1
        ));

        // Buffer stats
        parts.push(format!("Lines: {}", state.total_lines));
        parts.push(format!("Size: {}B", state.buffer_size));

        parts.join(" | ")
    }

    /// Render the status bar to a layer instead of directly to terminal
    /// This allows the status bar to be composited with other layers
    pub fn render_to_layer(
        layer: &mut Layer,
        viewport: &Viewport,
        current_mode: Mode,
        pending_key: Option<Key>,
        state: &State,
    ) {
        let status_row = viewport.visible_rows().saturating_sub(1);
        let visible_cols = viewport.visible_cols();

        // If status line is disabled, clear the row and return
        if !state.settings.status_line.show_status_line {
            for col in 0..visible_cols {
                layer.clear_cell(status_row, col);
            }
            return;
        }

        // Determine colors based on reverse video setting
        let (fg, bg) = if state.settings.status_line.reverse_video {
            // Reverse video: swap default fg/bg (use white on black or similar)
            (Some(Color::Black), Some(Color::White))
        } else {
            (None, None)
        };

        // Build the status line content
        let mode_str = Self::format_mode(current_mode);

        // In command mode, just show the mode
        if current_mode == Mode::Command {
            // Write mode string
            layer.write_bytes_colored(status_row, 0, mode_str.as_bytes(), fg, bg);
            // Fill rest with spaces
            for col in mode_str.len()..visible_cols {
                layer.set_cell(
                    status_row,
                    col,
                    crate::layer::Cell::new(b' ').with_colors(fg, bg),
                );
            }
            return;
        }

        // Normal display: mode + pending key + (debug info or filename)
        let mut col = 0;

        // Write mode
        layer.write_bytes_colored(status_row, col, mode_str.as_bytes(), fg, bg);
        col += mode_str.len();

        // Pending key indicator
        if let Some(key) = pending_key {
            let pending_str = format!(" [{}]", Self::format_key(key));
            layer.write_bytes_colored(status_row, col, pending_str.as_bytes(), fg, bg);
            col += pending_str.len();
        }

        // Calculate remaining space
        let used_cols = col;
        let available_cols = visible_cols.saturating_sub(used_cols);

        if state.debug_mode {
            // Debug mode: show debug info
            let debug_str = Self::format_debug_info(state, current_mode);
            if !debug_str.is_empty() {
                let truncated = if debug_str.len() <= available_cols {
                    debug_str
                } else if available_cols > 3 {
                    format!("{}...", &debug_str[..available_cols.saturating_sub(3)])
                } else {
                    String::new()
                };

                // Right-align debug info
                let spacing = available_cols.saturating_sub(truncated.len());
                for _ in 0..spacing {
                    layer.set_cell(
                        status_row,
                        col,
                        crate::layer::Cell::new(b' ').with_colors(fg, bg),
                    );
                    col += 1;
                }
                layer.write_bytes_colored(status_row, col, truncated.as_bytes(), fg, bg);
                col += truncated.len();
            }
        } else if state.settings.status_line.show_filename {
            // Normal mode: show filename on the right
            let file_name = &state.file_name;
            if file_name.len() <= available_cols {
                // Right-align filename
                let spacing = available_cols.saturating_sub(file_name.len());
                for _ in 0..spacing {
                    layer.set_cell(
                        status_row,
                        col,
                        crate::layer::Cell::new(b' ').with_colors(fg, bg),
                    );
                    col += 1;
                }
                layer.write_bytes_colored(status_row, col, file_name.as_bytes(), fg, bg);
                col += file_name.len();
            } else if available_cols > 3 {
                // Truncate filename
                let truncated = format!(
                    "...{}",
                    &file_name[file_name.len().saturating_sub(available_cols - 3)..]
                );
                let spacing = available_cols.saturating_sub(truncated.len());
                for _ in 0..spacing {
                    layer.set_cell(
                        status_row,
                        col,
                        crate::layer::Cell::new(b' ').with_colors(fg, bg),
                    );
                    col += 1;
                }
                layer.write_bytes_colored(status_row, col, truncated.as_bytes(), fg, bg);
                col += truncated.len();
            }
        }

        // Fill remaining space with spaces
        while col < visible_cols {
            layer.set_cell(
                status_row,
                col,
                crate::layer::Cell::new(b' ').with_colors(fg, bg),
            );
            col += 1;
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
