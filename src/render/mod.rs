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
use crate::wrap::DisplayMap;

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
    /// Number of plugin custom highlights (to trigger redraw when highlights change)
    pub plugin_highlights_len: usize,
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

/// State for rendering the completion dropdown menu
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionMenuDrawState {
    /// (display_text, description) pairs
    pub candidates: Vec<(String, String)>,
    /// Currently highlighted index
    pub selected: Option<usize>,
    /// Terminal columns (for matching the command line width/position)
    pub terminal_cols: usize,
    /// Command line width ratio
    pub cmd_width_ratio: f64,
    /// Command line minimum width
    pub cmd_min_width: usize,
    /// Whether the command line has a border
    pub cmd_has_border: bool,
    /// Command line total height in rows (including borders)
    pub cmd_height: usize,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    /// First visible candidate (scroll offset)
    pub scroll_offset: usize,
}

/// Minimal state required to trigger a re-render of notifications
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDrawState {
    pub generation: u64,
    pub count: usize,
}

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
    pub skip_content: bool,
    pub cursor_row_offset: usize,
    pub cursor_col_offset: usize,
    pub cursor_viewport: Option<&'a Viewport>,
    /// Terminal cursor (row, col); bypasses text-editor cursor math when set.
    pub terminal_cursor: Option<(usize, usize)>,
    /// Optional per-byte-range foreground color overrides (used by directory/undotree buffers).
    pub custom_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    /// Plugin highlights: rendered as bg color with contrasting fg.
    pub plugin_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    /// Per-document line number override (AND-ed with global setting).
    pub show_line_numbers: bool,
    pub display_map: Option<&'a DisplayMap>,
}

/// Context for rendering passed to helpers
pub struct DrawContext<'a> {
    pub buf: &'a TextBuffer,
    pub viewport: &'a Viewport,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub custom_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    pub plugin_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
    pub highlights: Option<&'a [(std::ops::Range<usize>, u32)]>,
    pub capture_map: Option<&'a [&'a str]>,
    /// Per-document line number override (AND-ed with global setting).
    pub show_line_numbers: bool,
    pub display_map: Option<&'a DisplayMap>,
    /// Overrides state.gutter_width for content rendering (per-window in multi-pane mode).
    pub gutter_width_override: Option<usize>,
}

/// Cursor position information returned from layer-based rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorPosition {
    /// Absolute terminal position (row, col)
    Absolute(u16, u16),
}

/// Render buffer content to the content layer
pub(crate) fn render_content_to_layer(layer: &mut Layer, ctx: &DrawContext) -> Result<(), String> {
    render_content_to_layer_offset(layer, ctx, 0, 0)
}

/// Render buffer content with row/col offsets applied to all cell positions
pub(crate) fn render_content_to_layer_offset(
    layer: &mut Layer,
    ctx: &DrawContext,
    row_offset: usize,
    col_offset: usize,
) -> Result<(), String> {
    let buf = ctx.buf;
    let viewport = ctx.viewport;
    let editor_bg = ctx.state.settings.editor_bg;
    let editor_fg = ctx.state.settings.editor_fg;

    let buf_total_lines = ctx.buf.get_total_lines();
    let gutter_width = if ctx.show_line_numbers && ctx.state.settings.show_line_numbers {
        ctx.gutter_width_override.unwrap_or(ctx.state.gutter_width)
    } else {
        0
    };

    let visible_rows = viewport.visible_rows().saturating_sub(1);
    let visible_cols = viewport.visible_cols();

    if let Some(dm) = ctx.display_map {
        let top_visual_row = viewport.top_visual_row();
        let search_matches = &ctx.state.search_matches;

        let mut highlight_idx: usize = 0;
        let mut search_match_idx: usize = 0;
        let mut last_logical_line: Option<usize> = None;
        let mut highlight_idx_at_line_start: usize = 0;
        let mut search_match_idx_at_line_start: usize = 0;

        if let Some(row_info) = dm.get_visual_row(top_visual_row) {
            let first_char = row_info.char_start;
            search_match_idx = search_matches.partition_point(|m| m.range.end <= first_char);
            search_match_idx_at_line_start = search_match_idx;
        }

        for i in 0..visible_rows {
            let visual_row = top_visual_row + i;
            let row_info = match dm.get_visual_row(visual_row) {
                Some(r) => r,
                None => {
                    if gutter_width > 0 {
                        render_gutter_blank(
                            layer,
                            i + row_offset,
                            col_offset,
                            gutter_width,
                            editor_fg,
                            editor_bg,
                        );
                    }
                    for col in (col_offset + gutter_width)..(col_offset + visible_cols) {
                        layer.set_cell(
                            i + row_offset,
                            col,
                            Cell::from_char(' ').with_colors(editor_fg, editor_bg),
                        );
                    }
                    continue;
                }
            };

            if Some(row_info.logical_line) != last_logical_line {
                last_logical_line = Some(row_info.logical_line);
                highlight_idx_at_line_start = highlight_idx;
                search_match_idx_at_line_start = search_match_idx;
            } else {
                highlight_idx = highlight_idx_at_line_start;
                search_match_idx = search_match_idx_at_line_start;
            }

            if gutter_width > 0 {
                if row_info.is_first {
                    render_gutter(
                        layer,
                        row_info.logical_line,
                        i + row_offset,
                        col_offset,
                        gutter_width,
                        buf_total_lines,
                        editor_fg,
                        editor_bg,
                    );
                } else {
                    render_gutter_blank(
                        layer,
                        i + row_offset,
                        col_offset,
                        gutter_width,
                        editor_fg,
                        editor_bg,
                    );
                }
            }

            render_line(
                layer,
                ctx,
                RenderLineConfig {
                    line_num: row_info.logical_line,
                    row_idx: i + row_offset,
                    gutter_width: col_offset + gutter_width,
                    visible_cols: col_offset + visible_cols,
                    default_fg: editor_fg,
                    default_bg: editor_bg,
                    segment_left_col: Some(row_info.segment_col_start),
                    segment_content_cols: Some(
                        row_info.segment_col_end - row_info.segment_col_start,
                    ),
                },
                &mut highlight_idx,
                &mut search_match_idx,
            );
        }
    } else {
        let top_line = viewport.top_line();
        let search_matches = &ctx.state.search_matches;
        let first_visible_char = buf.line_index.get_start(top_line).unwrap_or(0);

        let mut search_match_idx =
            search_matches.partition_point(|m| m.range.end <= first_visible_char);
        let mut highlight_idx = 0;

        for i in 0..visible_rows {
            let line_num = top_line + i;

            if gutter_width > 0 {
                render_gutter(
                    layer,
                    line_num,
                    i + row_offset,
                    col_offset,
                    gutter_width,
                    buf_total_lines,
                    editor_fg,
                    editor_bg,
                );
            }

            render_line(
                layer,
                ctx,
                RenderLineConfig {
                    line_num,
                    row_idx: i + row_offset,
                    gutter_width: col_offset + gutter_width,
                    visible_cols: col_offset + visible_cols,
                    default_fg: editor_fg,
                    default_bg: editor_bg,
                    segment_left_col: None,
                    segment_content_cols: None,
                },
                &mut highlight_idx,
                &mut search_match_idx,
            );
        }
    }

    Ok(())
}

fn render_gutter_blank(
    layer: &mut Layer,
    row_idx: usize,
    col_start: usize,
    gutter_width: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    for i in 0..gutter_width {
        layer.set_cell(
            row_idx,
            col_start + i,
            Cell::new(Character::from(' ')).with_colors(fg, bg),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_gutter(
    layer: &mut Layer,
    line_num: usize,
    row_idx: usize,
    col_start: usize,
    gutter_width: usize,
    total_lines: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    if line_num < total_lines {
        let line_str = format!("{:width$}", line_num + 1, width = gutter_width - 1);
        for (i, ch) in line_str.chars().enumerate() {
            layer.set_cell(
                row_idx,
                col_start + i,
                Cell::new(Character::from(ch)).with_colors(fg, bg),
            );
        }
        layer.set_cell(
            row_idx,
            col_start + gutter_width - 1,
            Cell::new(Character::from(' ')).with_colors(fg, bg),
        );
    } else {
        for i in 0..gutter_width {
            layer.set_cell(
                row_idx,
                col_start + i,
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
    segment_left_col: Option<usize>,
    segment_content_cols: Option<usize>,
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
    let custom_highlights = ctx.custom_highlights.unwrap_or(&[]);
    let plugin_highlights = ctx.plugin_highlights.unwrap_or(&[]);
    let search_matches = &ctx.state.search_matches;

    let syntax = SyntaxDecorator::new(
        source,
        highlights,
        highlight_idx,
        ctx.state.settings.syntax_colors.as_ref(),
        ctx.capture_map,
    );
    let colored = pipeline::ColorDecorator::new(syntax, custom_highlights);
    let plugin = pipeline::PluginHighlightDecorator::new(colored, plugin_highlights);

    let search = SearchDecorator::new(plugin, search_matches, search_match_idx);

    // Layout
    let layout = TabLayout::new(search, ctx.tab_width);

    let content_cols = config
        .segment_content_cols
        .unwrap_or_else(|| config.visible_cols.saturating_sub(config.gutter_width));
    let mut rendered_col = 0;

    let left_col = config
        .segment_left_col
        .unwrap_or_else(|| ctx.viewport.left_col());

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
            // Number of columns this item contributes to the visible area.
            // For items that straddle the left edge (current_visual_col < left_col),
            // only count the portion that is actually in view.
            let visible_width = next_visual_col - left_col.max(current_visual_col);

            // Calculate where to draw in the layer (relative to gutter)
            let display_col = rendered_col + config.gutter_width;

            if display_col < config.visible_cols {
                let fg = item.fg.or(config.default_fg);
                let bg = item.bg.or(config.default_bg);

                // Tabs are already expanded by TabLayout — store a space so the
                // terminal never receives a raw \t (which would jump to the
                // terminal's own tab stop rather than the editor's tab_width).
                // Wide chars (width > 1) also fill their remaining columns with spaces.
                let display_char = if item.char == Character::Tab {
                    Character::from(' ')
                } else {
                    item.char
                };

                layer.set_cell(
                    config.row_idx,
                    display_col,
                    Cell::new(display_char).with_colors(fg, bg),
                );

                // Fill the rest of the visible span with spaces.
                if visible_width > 1 {
                    let empty_cell = Cell {
                        content: Character::from(' '),
                        fg,
                        bg,
                    };
                    for k in 1..visible_width {
                        if display_col + k < config.visible_cols {
                            layer.set_cell(config.row_idx, display_col + k, empty_cell.clone());
                        }
                    }
                }
            }
            rendered_col += visible_width;
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
    calculate_cursor_column_at(buf, line, tab_width, buf.cursor())
}

/// Like `calculate_cursor_column` but with an explicit cursor position
pub fn calculate_cursor_column_at(
    buf: &TextBuffer,
    line: usize,
    tab_width: usize,
    cursor_pos: usize,
) -> usize {
    if line >= buf.get_total_lines() {
        return 0;
    }

    let line_start = buf.line_index.get_start(line).unwrap_or(0);
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
    use unicode_width::UnicodeWidthStr;

    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::with_capacity(width);
        for word in words {
            let current_w = UnicodeWidthStr::width(current_line.as_str());
            let word_w = UnicodeWidthStr::width(word);
            if current_w + word_w + 1 > width && !current_line.is_empty() {
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
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    let notifications = state.error_manager.notifications();
    if notifications.is_empty() {
        return;
    }

    let mut current_row = viewport_rows.saturating_sub(2);
    let max_width = (viewport_cols as f32 * 0.8) as usize;

    for notification in notifications.iter_active().rev() {
        let message = &notification.message;
        let color = match notification.kind {
            crate::notification::NotificationType::Error => Color::Red,
            crate::notification::NotificationType::Warning => Color::Yellow,
            crate::notification::NotificationType::Info => Color::Blue,
            crate::notification::NotificationType::Success => Color::Green,
        };

        let lines = wrap_text(message, max_width);
        let content_width = lines.iter().map(|l| l.width()).max().unwrap_or(0);
        let box_width = content_width + 3;
        let start_col = viewport_cols.saturating_sub(box_width);

        for line in lines.iter().rev() {
            if current_row == 0 {
                break;
            }

            for i in 0..box_width {
                layer.set_cell(
                    current_row,
                    start_col + i,
                    Cell::new(Character::from(' ')).with_colors(Some(Color::White), Some(color)),
                );
            }

            let mut current_col = start_col + 2;
            for ch in line.chars() {
                let ch_width = ch.width().unwrap_or(1);
                layer.set_cell(
                    current_row,
                    current_col,
                    Cell::from_char(ch).with_colors(Some(Color::White), Some(color)),
                );

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

        current_row = current_row.saturating_sub(1);
    }
}

/// Render split dividers between windows
pub(crate) fn render_dividers(
    layer: &mut Layer,
    tree: &crate::split::tree::SplitTree,
    total_rows: usize,
    total_cols: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    render_node_dividers(layer, &tree.root, 0, 0, total_rows, total_cols, fg, bg);
}

#[allow(clippy::too_many_arguments)]
fn render_node_dividers(
    layer: &mut Layer,
    node: &crate::split::tree::SplitNode,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    use crate::split::layout::{MIN_WINDOW_COLS, MIN_WINDOW_ROWS};
    use crate::split::tree::{SplitDirection, SplitNode};

    if let SplitNode::Split {
        direction,
        ratio,
        first,
        second,
    } = node
    {
        match direction {
            SplitDirection::Horizontal => {
                let available = rows.saturating_sub(1);
                let first_rows = ((available as f64) * ratio).round() as usize;
                let first_rows = first_rows
                    .max(MIN_WINDOW_ROWS)
                    .min(available.saturating_sub(MIN_WINDOW_ROWS));
                let second_rows = available.saturating_sub(first_rows);
                let divider_row = row + first_rows;

                for c in col..col + cols {
                    layer.set_cell(
                        divider_row,
                        c,
                        Cell::new(Character::from('─')).with_colors(fg, bg),
                    );
                }

                render_node_dividers(layer, first, row, col, first_rows, cols, fg, bg);
                render_node_dividers(
                    layer,
                    second,
                    divider_row + 1,
                    col,
                    second_rows,
                    cols,
                    fg,
                    bg,
                );
            }
            SplitDirection::Vertical => {
                let available = cols.saturating_sub(1);
                let first_cols = ((available as f64) * ratio).round() as usize;
                let first_cols = first_cols
                    .max(MIN_WINDOW_COLS)
                    .min(available.saturating_sub(MIN_WINDOW_COLS));
                let second_cols = available.saturating_sub(first_cols);
                let divider_col = col + first_cols;

                for r in row..row + rows {
                    layer.set_cell(
                        r,
                        divider_col,
                        Cell::new(Character::from('│')).with_colors(fg, bg),
                    );
                }

                render_node_dividers(layer, first, row, col, rows, first_cols, fg, bg);
                render_node_dividers(
                    layer,
                    second,
                    row,
                    divider_col + 1,
                    rows,
                    second_cols,
                    fg,
                    bg,
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
