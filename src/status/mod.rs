//! Status bar management
//! Handles rendering and formatting of the editor status bar
//!
//! ## status/ Invariants
//!
//! - Status content is derived entirely from editor state.
//! - Status rendering does not influence editor behavior.
//! - Status display is optional and failure-tolerant.
//! - Status never consumes input or commands.

use crate::character::Character;
use crate::color::Color;
use crate::key::Key;
use crate::layer::{Cell, Layer};
use crate::mode::Mode;

use crate::render::{CursorInfo, StatusDrawState};

/// First `max_chars` characters of `s` (never splits a multi-byte char).
fn char_safe_prefix(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Last `max_chars` characters of `s` (never splits a multi-byte char).
fn char_safe_suffix(s: &str, max_chars: usize) -> &str {
    let char_count = s.chars().count();
    let skip = char_count.saturating_sub(max_chars);
    match s.char_indices().nth(skip) {
        Some((idx, _)) => &s[idx..],
        None => s,
    }
}

/// Status bar renderer
pub struct StatusBar;

impl StatusBar {
    /// Format mode name for display
    #[must_use]
    pub fn format_mode(mode: Mode) -> &'static str {
        match mode {
            Mode::Normal => crate::constants::modes::NORMAL,
            Mode::Insert => crate::constants::modes::INSERT,
            Mode::Command => crate::constants::modes::COMMAND,
            Mode::Search => crate::constants::modes::SEARCH,
            Mode::Rename => crate::constants::modes::RENAME,
            Mode::OperatorPending => crate::constants::modes::OPERATOR_PENDING,
            Mode::Replace => crate::constants::modes::REPLACE,
            Mode::Visual => crate::constants::modes::VISUAL,
            Mode::VisualLine => crate::constants::modes::VISUAL_LINE,
            Mode::VisualBlock => crate::constants::modes::VISUAL_BLOCK,
        }
    }

    /// Format key for display
    #[must_use]
    pub fn format_key(key: Key) -> String {
        match key {
            Key::Char(ch) => {
                if !ch.is_control() {
                    format!("{ch}")
                } else if ch == '\t' {
                    "Tab".to_string()
                } else {
                    format!("\\u{{{:04x}}}", ch as u32)
                }
            }
            Key::Ctrl(ch) => format!("Ctrl+{}", (ch as char).to_uppercase()),
            Key::Alt(ch) => format!("Alt+{}", (ch as char).to_uppercase()),
            Key::ArrowUp => "Up".to_string(),
            Key::ArrowDown => "Down".to_string(),
            Key::ArrowLeft => "Left".to_string(),
            Key::ArrowRight => "Right".to_string(),
            Key::CtrlArrowUp => "Ctrl+Up".to_string(),
            Key::CtrlArrowDown => "Ctrl+Down".to_string(),
            Key::CtrlArrowLeft => "Ctrl+Left".to_string(),
            Key::CtrlArrowRight => "Ctrl+Right".to_string(),
            Key::Backspace => "Backspace".to_string(),
            Key::Delete => "Delete".to_string(),
            Key::Enter => "Enter".to_string(),
            Key::Escape => "Esc".to_string(),
            Key::Tab => "Tab".to_string(),
            Key::ShiftTab => "S-Tab".to_string(),
            Key::ShiftSpace => "S-Space".to_string(),
            Key::Home => "Home".to_string(),
            Key::End => "End".to_string(),
            Key::CtrlHome => "Ctrl+Home".to_string(),
            Key::CtrlEnd => "Ctrl+End".to_string(),
            Key::PageUp => "PageUp".to_string(),
            Key::PageDown => "PageDown".to_string(),
            Key::Resize(cols, rows) => format!("Resize({cols}, {rows})"),
        }
    }

    /// Format debug information string
    fn format_debug_info_from_state(
        file_name: &str,
        cursor: &CursorInfo,
        total_lines: usize,
        last_keypress: Option<Key>,
    ) -> String {
        let mut parts = Vec::new();
        if let Some(key) = last_keypress {
            parts.push(format!("Last: {}", Self::format_key(key)));
        }
        parts.push(format!("Pos: {}:{}", cursor.row + 1, cursor.col + 1));
        parts.push(format!("Lines: {}", total_lines));
        parts.push(format!("File: {}", file_name));
        parts.join(" | ")
    }

    /// Render the status bar to a layer instead of directly to terminal
    /// This allows the status bar to be composited with other layers
    pub fn render_to_layer(layer: &mut Layer, state: &StatusDrawState) {
        let mut frame = crate::paint::PaintFrame::new(layer.rows());
        Self::render_to_paint_frame(&mut frame, layer.rows(), state);
        crate::paint::rasterize(&frame, layer);
    }

    /// Builds the status bar's PaintFrame; render_to_layer rasterizes it.
    fn render_to_paint_frame(
        frame: &mut crate::paint::PaintFrame,
        layer_rows: usize,
        state: &StatusDrawState,
    ) {
        let status_row = layer_rows.saturating_sub(1);
        let visible_cols = state.cols;

        if !state.show_status_line {
            for col in 0..visible_cols {
                frame.set_cell(status_row, col, Cell::new(Character::from(' ')));
            }
            return;
        }

        let (fg, bg) = if state.reverse_video {
            (
                state.editor_bg.or(Some(Color::Black)),
                state.editor_fg.or(Some(Color::White)),
            )
        } else {
            (state.editor_fg, state.editor_bg)
        };

        let mode_str = Self::format_mode(state.mode);

        let mut col = 0;

        frame.write_str_colored(status_row, col, mode_str, fg, bg);
        col += mode_str.len();

        if state.is_remote {
            let remote_str = " [R]";
            frame.write_str_colored(status_row, col, remote_str, fg, bg);
            col += remote_str.len();
        }

        if state.pending_count > 0 {
            let count_str = format!(" {}", state.pending_count);
            frame.write_str_colored(status_row, col, &count_str, fg, bg);
            col += count_str.len();
        }

        if let Some(key) = state.pending_key {
            let pending_str = format!(" [{}]", Self::format_key(key));
            frame.write_str_colored(status_row, col, &pending_str, fg, bg);
            col += pending_str.len();
        }

        if let Some(query) = &state.search_query {
            if !query.is_empty() {
                frame.set_cell(
                    status_row,
                    col,
                    Cell::new(Character::from(' ')).with_colors(fg, bg),
                );
                col += 1;

                frame.write_str_colored(
                    status_row,
                    col,
                    query,
                    Some(Color::Black),
                    Some(Color::Yellow),
                );
                col += query.len();

                if state.search_total_matches > 0 {
                    let stats = if let Some(idx) = state.search_match_index {
                        format!(" {}/{}", idx, state.search_total_matches)
                    } else {
                        format!(" ?/{}", state.search_total_matches)
                    };
                    frame.write_str_colored(status_row, col, &stats, fg, bg);
                    col += stats.len();
                }
            }
        }

        if let Some(lsp) = &state.lsp_status {
            if !lsp.is_empty() {
                frame.set_cell(
                    status_row,
                    col,
                    Cell::new(Character::from(' ')).with_colors(fg, bg),
                );
                col += 1;
                let lsp_color = if lsp.contains('E') {
                    state.lsp_error_color.or(Some(Color::Red))
                } else if lsp.contains('W') {
                    state.lsp_warn_color.or(Some(Color::Yellow))
                } else {
                    state.lsp_ok_color.or(Some(Color::Cyan))
                };
                frame.write_str_colored(status_row, col, lsp, lsp_color, bg);
                col += lsp.len();
            }
        }

        let used_cols = col;
        let available_cols = visible_cols.saturating_sub(used_cols);

        if state.debug_mode {
            let debug_str = Self::format_debug_info_from_state(
                &state.file_name,
                &state.cursor,
                state.total_lines,
                state.last_keypress,
            );
            if !debug_str.is_empty() {
                let padded_cols = available_cols.saturating_sub(1);
                let truncated = if debug_str.len() <= padded_cols {
                    debug_str
                } else if padded_cols > 3 {
                    format!(
                        "{}...",
                        char_safe_prefix(&debug_str, padded_cols.saturating_sub(3))
                    )
                } else {
                    String::new()
                };

                let spacing = padded_cols.saturating_sub(truncated.len());
                for _ in 0..spacing {
                    frame.set_cell(
                        status_row,
                        col,
                        crate::layer::Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                    col += 1;
                }
                frame.write_str_colored(status_row, col, &truncated, fg, bg);
                col += truncated.len();
            }
        } else if state.show_filename {
            let display_name = if state.is_dirty && state.show_dirty_indicator {
                format!("{}*", state.file_name)
            } else {
                state.file_name.clone()
            };

            if display_name.len() <= available_cols {
                let spacing = available_cols.saturating_sub(display_name.len() + 1);
                for _ in 0..spacing {
                    frame.set_cell(
                        status_row,
                        col,
                        crate::layer::Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                    col += 1;
                }
                frame.write_str_colored(status_row, col, &display_name, fg, bg);
                col += display_name.len();
            } else if available_cols > 3 {
                let truncated =
                    format!("...{}", char_safe_suffix(&display_name, available_cols - 3));
                let spacing = available_cols.saturating_sub(truncated.len());
                for _ in 0..spacing {
                    frame.set_cell(
                        status_row,
                        col,
                        crate::layer::Cell::new(Character::from(' ')).with_colors(fg, bg),
                    );
                    col += 1;
                }
                frame.write_str_colored(status_row, col, &truncated, fg, bg);
                col += truncated.len();
            }
        }

        while col < visible_cols {
            frame.set_cell(
                status_row,
                col,
                crate::layer::Cell::new(Character::from(' ')).with_colors(fg, bg),
            );
            col += 1;
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
