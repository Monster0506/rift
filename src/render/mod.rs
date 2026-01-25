//! Rendering system
//! Handles drawing the editor UI to the terminal using layers

use crate::buffer::api::BufferView;
/// ## render/ Invariants
///
/// - Rendering reads editor state and buffer contents only.
/// - Rendering never mutates editor, buffer, cursor, or viewport state.
/// - Rendering performs no input handling.
/// - Rendering tolerates invalid state but never corrects it.
/// - Displayed cursor position always matches buffer cursor position.
/// - A full redraw is always safe.
/// - Viewport must be updated before calling render functions (viewport updates happen
///   in the state update phase, not during rendering).
/// - All rendering is layer-based and composited before output to terminal.
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::color::Color;
use crate::key::Key;
use crate::layer::{Cell, Layer};
use crate::mode::Mode;
use crate::state::State;
use crate::viewport::Viewport;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub mod components;
pub mod ecs;
pub mod pipeline;
pub mod system;
pub use pipeline::*;
pub use system::RenderSystem;

/// Explicitly tracked cursor information for rendering comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorInfo {
    pub row: usize,
    pub col: usize,
}

/// Minimal state required to trigger a re-render of the buffer content
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDrawState {
    pub revision: u64,
    pub top_line: usize,
    pub left_col: usize,
    pub rows: usize,
    pub tab_width: usize,
    /// Whether to show line numbers
    pub show_line_numbers: bool,
    /// Hash/Generation of highlights to trigger redraw on syntax update
    pub highlights_hash: u64,
    /// Current gutter width
    pub gutter_width: usize,
    /// Number of search matches (to trigger redraw on search)
    pub search_matches_count: usize,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    pub theme: Option<String>,
}

/// Minimal state required to trigger a re-render of the status bar
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusDrawState {
    pub mode: Mode,
    pub pending_key: Option<crate::key::Key>,
    pub pending_count: usize,
    pub last_keypress: Option<crate::key::Key>,
    pub file_name: String,
    pub is_dirty: bool,
    pub cursor: CursorInfo,
    pub total_lines: usize,
    pub debug_mode: bool,
    pub cols: usize,
    pub search_query: Option<String>,
    pub search_match_index: Option<usize>,
    pub search_total_matches: usize,
    pub reverse_video: bool,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
}

/// Minimal state required to trigger a re-render of the command line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDrawState {
    pub content: String,
    pub cursor: CursorInfo,
    pub width: usize,
    pub height: usize,
    pub has_border: bool,
    pub reverse_video: bool,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
}

/// Minimal state required to trigger a re-render of notifications
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDrawState {
    pub generation: u64,
    pub count: usize,
}

/// External state passed to RenderSystem::render
/// External state passed to RenderSystem::render
pub struct RenderState<'a> {
    pub buf: &'a TextBuffer,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
    pub highlights: Option<&'a [(std::ops::Range<usize>, u32)]>,
    pub capture_map: Option<&'a [&'a str]>,
    pub modal: Option<&'a mut crate::editor::ActiveModal>,
}

/// Context for rendering passed to helpers
pub struct DrawContext<'a> {
    pub buf: &'a TextBuffer,
    pub viewport: &'a Viewport,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
    pub highlights: Option<&'a [(std::ops::Range<usize>, u32)]>,
    pub capture_map: Option<&'a [&'a str]>,
    pub modal: Option<&'a mut crate::editor::ActiveModal>,
}

/// Cursor position information returned from layer-based rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorPosition {
    /// Absolute terminal position (row, col)
    Absolute(u16, u16),
}

/// Render buffer content to the content layer
pub(crate) fn render_content_to_layer(layer: &mut Layer, ctx: &DrawContext) -> Result<(), String> {
    let buf = ctx.buf;
    let viewport = ctx.viewport;
    let editor_bg = ctx.state.settings.editor_bg;
    let editor_fg = ctx.state.settings.editor_fg;

    let gutter_width = if ctx.state.settings.show_line_numbers {
        ctx.state.gutter_width
    } else {
        0
    };

    let top_line = viewport.top_line();
    let visible_rows = viewport.visible_rows().saturating_sub(1);
    let visible_cols = viewport.visible_cols();

    let search_matches = &ctx.state.search_matches;
    let first_visible_char = buf.line_index.get_start(top_line).unwrap_or(0);

    // State tracked across lines for performance
    let mut search_match_idx =
        search_matches.partition_point(|m| m.range.end <= first_visible_char);
    let mut highlight_idx = 0;

    for i in 0..visible_rows {
        let line_num = top_line + i;

        if gutter_width > 0 {
            render_gutter(
                layer,
                line_num,
                i,
                gutter_width,
                ctx.state.total_lines,
                editor_fg,
                editor_bg,
            );
        }

        render_line(
            layer,
            ctx,
            RenderLineConfig {
                line_num,
                row_idx: i,
                gutter_width,
                visible_cols,
                default_fg: editor_fg,
                default_bg: editor_bg,
            },
            &mut highlight_idx,
            &mut search_match_idx,
        );
    }

    Ok(())
}

fn render_gutter(
    layer: &mut Layer,
    line_num: usize,
    row_idx: usize,
    gutter_width: usize,
    total_lines: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    if line_num < total_lines {
        let line_str = format!("{:width$}", line_num + 1, width = gutter_width - 1);
        for (col, ch) in line_str.chars().enumerate() {
            layer.set_cell(
                row_idx,
                col,
                Cell::new(Character::from(ch)).with_colors(fg, bg),
            );
        }
        layer.set_cell(
            row_idx,
            gutter_width - 1,
            Cell::new(Character::from(' ')).with_colors(fg, bg),
        );
    } else {
        for col in 0..gutter_width {
            layer.set_cell(
                row_idx,
                col,
                Cell::new(Character::from(' ')).with_colors(fg, bg),
            );
        }
    }
}

struct RenderLineConfig {
    line_num: usize,
    row_idx: usize,
    gutter_width: usize,
    visible_cols: usize,
    default_fg: Option<Color>,
    default_bg: Option<Color>,
}

fn render_line(
    layer: &mut Layer,
    ctx: &DrawContext,
    config: RenderLineConfig,
    highlight_idx: &mut usize,
    search_match_idx: &mut usize,
) {
    let buf = ctx.buf;
    if config.line_num >= buf.get_total_lines() {
        // Render empty line (past end of buffer)
        for col in config.gutter_width..config.visible_cols {
            layer.set_cell(
                config.row_idx,
                col,
                Cell::from_char(' ').with_colors(config.default_fg, config.default_bg),
            );
        }
        return;
    }

    let source = LineSource::new(buf, config.line_num);
    let highlights = ctx.highlights.unwrap_or(&[]);
    let search_matches = &ctx.state.search_matches;

    // Decorate
    let syntax = SyntaxDecorator::new(
        source,
        highlights,
        highlight_idx,
        ctx.state.settings.syntax_colors.as_ref(), // UserSettings has crate::color::theme::SyntaxColors
        ctx.capture_map,
    );

    let search = SearchDecorator::new(syntax, search_matches, search_match_idx);

    // Layout
    let layout = TabLayout::new(search, ctx.tab_width);

    let content_cols = config.visible_cols.saturating_sub(config.gutter_width);
    let mut rendered_col = 0;

    // Calculate the absolute column at the left edge of the screen (horizontal scroll)
    let left_col = ctx.viewport.left_col();

    // Internal tracker for absolute visual column (including horizontal scroll)
    let mut current_visual_col = 0;

    for item in layout {
        if rendered_col >= content_cols {
            break;
        }

        if item.char == Character::Newline {
            break;
        }

        let width = item.width;
        let next_visual_col = current_visual_col + width;

        // Check visibility against viewport
        if next_visual_col > left_col {
            // Item is at least partially visible

            // Calculate where to draw in the layer (relative to gutter)
            let display_col = rendered_col + config.gutter_width;

            if display_col < config.visible_cols {
                let fg = item.fg.or(config.default_fg);
                let bg = item.bg.or(config.default_bg);

                layer.set_cell(
                    config.row_idx,
                    display_col,
                    Cell::new(item.char).with_colors(fg, bg),
                );

                if width > 1 {
                    let empty_cell = Cell {
                        content: Character::from(' '),
                        fg,
                        bg,
                    };
                    for k in 1..width {
                        if display_col + k < config.visible_cols {
                            layer.set_cell(config.row_idx, display_col + k, empty_cell.clone());
                        }
                    }
                }
            }
            rendered_col += width;
        }

        current_visual_col = next_visual_col;
    }

    // Fill remaining line with background
    for col in (rendered_col + config.gutter_width)..config.visible_cols {
        layer.set_cell(
            config.row_idx,
            col,
            Cell::from_char(' ').with_colors(config.default_fg, config.default_bg),
        );
    }
}

/// Calculate the cursor column position accounting for tab width and wide characters
pub fn calculate_cursor_column(buf: &TextBuffer, line: usize, tab_width: usize) -> usize {
    if line >= buf.get_total_lines() {
        return 0;
    }

    let line_start = buf.line_index.get_start(line).unwrap_or(0);
    // Cursor is absolute char index
    let cursor_pos = buf.cursor();
    let target_char_idx = cursor_pos.saturating_sub(line_start);

    // We iterate chars from line start up to cursor
    // Bounds check? cursor_pos should be <= len.
    // get_line_start gives us start.
    // We iterate chars.

    let mut col = 0;

    let end = buf.len(); // cap at buffer end

    // Iterate manually over line
    for (current_idx, ch) in BufferView::chars(buf, line_start..end).enumerate() {
        if current_idx >= target_char_idx {
            break;
        }

        if ch == Character::Newline {
            // Cursor on newline?
            break;
        }

        if ch == Character::Tab {
            col += tab_width - (col % tab_width);
        } else {
            col += ch.render_width(col, tab_width);
        }
    }

    col
}

/// Helper to wrap text to a specific width
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::with_capacity(width);
        for word in words {
            if current_line.len() + word.len() + 1 > width && !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::with_capacity(width);
            }
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    lines
}

/// Render notifications to the notification layer
pub(crate) fn render_notifications(
    layer: &mut Layer,
    state: &State,
    viewport_rows: usize,
    viewport_cols: usize,
) {
    let notifications = state.error_manager.notifications();
    if notifications.is_empty() {
        return;
    }

    // Render notifications from bottom up, above status bar
    let mut current_row = viewport_rows.saturating_sub(2); // Start above status bar
    let max_width = (viewport_cols as f32 * 0.8) as usize; // Max 80% width

    for notification in notifications.iter_active().rev() {
        let message = &notification.message;
        let color = match notification.kind {
            crate::notification::NotificationType::Error => Color::Red,
            crate::notification::NotificationType::Warning => Color::Yellow,
            crate::notification::NotificationType::Info => Color::Blue,
            crate::notification::NotificationType::Success => Color::Green,
        };

        // Wrap text if needed
        let lines = wrap_text(message, max_width);

        // Calculate box width based on longest line
        let content_width = lines.iter().map(|l| l.width()).max().unwrap_or(0);
        let box_width = content_width + 4; // +4 for padding
        let start_col = viewport_cols.saturating_sub(box_width);

        // Render lines
        for line in lines.iter().rev() {
            if current_row == 0 {
                break;
            }

            // Draw background box
            for i in 0..box_width {
                layer.set_cell(
                    current_row,
                    start_col + i,
                    Cell::new(Character::from(' ')).with_colors(Some(Color::White), Some(color)),
                );
            }

            // Draw text
            let mut current_col = start_col + 2;
            for ch in line.chars() {
                let ch_width = ch.width().unwrap_or(1);
                layer.set_cell(
                    current_row,
                    current_col,
                    Cell::from_char(ch).with_colors(Some(Color::White), Some(color)),
                );

                // Handle wide characters
                if ch_width > 1 {
                    let empty_cell = Cell {
                        content: Character::from(' '),
                        fg: Some(Color::White),
                        bg: Some(color),
                    };
                    for k in 1..ch_width {
                        layer.set_cell(current_row, current_col + k, empty_cell.clone());
                    }
                }
                current_col += ch_width;
            }

            current_row = current_row.saturating_sub(1);
        }

        // Add spacing between notifications
        current_row = current_row.saturating_sub(1);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
