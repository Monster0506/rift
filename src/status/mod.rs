//! Status bar management
//! Handles rendering and formatting of the editor status bar
//!
//! ## status/ Invariants
//!
//! - Status content is derived entirely from editor state.
//! - Status rendering does not influence editor behavior.
//! - Status display is optional and failure-tolerant.
//! - Status never consumes input or commands.

use crate::command::Command;
use crate::key::Key;
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

        // Invert colors for status bar (reverse video)
        term.write(b"\x1b[7m")?;

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
            let used_cols = mode_len + pending_len;
            let available_cols = viewport.visible_cols().saturating_sub(used_cols);

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
        }

        // Reset colors
        term.write(b"\x1b[0m")?;

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
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
