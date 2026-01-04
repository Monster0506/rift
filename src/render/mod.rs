//! Rendering system
//! Handles drawing the editor UI to the terminal using layers

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
use crate::color::theme::SyntaxColors;
use crate::color::Color;
use crate::command_line::CommandLine;
use crate::error::RiftError;
use crate::key::Key;
use crate::layer::{Cell, Layer, LayerCompositor, LayerPriority};
use crate::mode::Mode;
use crate::state::State;
use crate::status::StatusBar;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub mod system;
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

/// Cache for detecting changes in component drawing states
#[derive(Debug, Clone, Default)]
pub struct RenderCache {
    pub content: Option<ContentDrawState>,
    pub status: Option<StatusDrawState>,
    pub command_line: Option<CommandDrawState>,
    pub notifications: Option<NotificationDrawState>,
    /// Last calculated cursor position for command mode
    pub last_command_cursor: Option<CursorPosition>,
    /// Last rendered cursor position
    pub last_cursor_pos: Option<CursorPosition>,
}

impl RenderCache {
    pub fn invalidate_all(&mut self) {
        self.content = None;
        self.status = None;
        self.command_line = None;
        self.notifications = None;
        self.last_command_cursor = None;
        self.last_cursor_pos = None;
    }

    pub fn invalidate_content(&mut self) {
        self.content = None;
    }

    pub fn invalidate_status(&mut self) {
        self.status = None;
    }

    pub fn invalidate_command_line(&mut self) {
        self.command_line = None;
    }

    pub fn invalidate_notifications(&mut self) {
        self.notifications = None;
    }
}

/// Force a full redraw of all components by invalidating cache and clearing layers
pub fn full_redraw<T: TerminalBackend>(
    term: &mut T,
    compositor: &mut LayerCompositor,
    ctx: DrawContext,
    cache: &mut RenderCache,
) -> Result<CursorPosition, RiftError> {
    // 1. Invalidate all cache entries
    cache.invalidate_all();

    // 2. Force clear all compositor layers to be absolutely sure
    compositor.clear_layer(LayerPriority::CONTENT);
    compositor.clear_layer(LayerPriority::STATUS_BAR);
    compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
    compositor.clear_layer(LayerPriority::NOTIFICATION);

    // 3. Ensure we clear the terminal via context
    let mut ctx = ctx;
    ctx.needs_clear = true;

    // 4. Perform a regular render pass
    render(term, compositor, ctx, cache)
}

/// Context for rendering
pub struct DrawContext<'a> {
    pub buf: &'a TextBuffer,
    pub viewport: &'a Viewport,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
    pub highlights: Option<&'a [(std::ops::Range<usize>, String)]>,
}

/// Cursor position information returned from layer-based rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorPosition {
    /// Absolute terminal position (row, col)
    Absolute(u16, u16),
}

/// Render the editor interface using the layer compositor
///
/// This is the main rendering function that composites multiple layers:
/// 1. CONTENT layer - the text buffer
/// 2. STATUS_BAR layer - the status line at bottom
/// 3. FLOATING_WINDOW layer - command line and dialogs (when in command mode)
/// 4. NOTIFICATION layer - notifications (when in command mode)
///
/// Returns the cursor position for the terminal.
pub fn render<T: TerminalBackend>(
    term: &mut T,
    compositor: &mut LayerCompositor,
    ctx: DrawContext,
    cache: &mut RenderCache,
) -> Result<CursorPosition, RiftError> {
    // Resize compositor if needed
    if compositor.rows() != ctx.viewport.visible_rows()
        || compositor.cols() != ctx.viewport.visible_cols()
    {
        compositor.resize(ctx.viewport.visible_rows(), ctx.viewport.visible_cols());
        cache.invalidate_all();
    }

    // 1. Render content to CONTENT layer
    let current_content_state = ContentDrawState {
        revision: ctx.buf.revision,
        top_line: ctx.viewport.top_line(),
        left_col: ctx.viewport.left_col(),
        rows: ctx.viewport.visible_rows(),
        tab_width: ctx.tab_width,
        show_line_numbers: ctx.state.settings.show_line_numbers,
        gutter_width: if ctx.state.settings.show_line_numbers {
            ctx.state.gutter_width
        } else {
            0
        },
        search_matches_count: ctx.state.search_matches.len(),
        editor_bg: ctx.state.settings.editor_bg,
        editor_fg: ctx.state.settings.editor_fg,
        theme: ctx.state.settings.theme.clone(),
    };

    if cache.content.as_ref() != Some(&current_content_state) {
        compositor.clear_layer(LayerPriority::CONTENT);
        render_content_to_layer(compositor.get_layer_mut(LayerPriority::CONTENT), &ctx)?;
        cache.content = Some(current_content_state);
    }
    // Calculate search match info
    let (search_match_index, search_total_matches) = if !ctx.state.search_matches.is_empty() {
        let cursor_offset = ctx.buf.cursor();
        let idx = ctx
            .state
            .search_matches
            .iter()
            .position(|m| {
                // Check if cursor is contained in match range or at start
                m.range.contains(&cursor_offset) || m.range.start == cursor_offset
            })
            .map(|i| i + 1); // 1-based index
        (idx, ctx.state.search_matches.len())
    } else {
        (None, 0)
    };

    // 2. Render status bar to STATUS_BAR layer (always updated if relevant state changes)
    let current_status_state = StatusDrawState {
        mode: ctx.current_mode,
        pending_key: ctx.pending_key,
        pending_count: ctx.pending_count,
        file_name: ctx.state.file_name.clone(),
        is_dirty: ctx.state.is_dirty,
        cursor: CursorInfo {
            row: ctx.state.cursor_pos.0,
            col: ctx.state.cursor_pos.1,
        },
        total_lines: ctx.state.total_lines,
        debug_mode: ctx.state.debug_mode,
        cols: ctx.viewport.visible_cols(),
        search_query: ctx.state.last_search_query.clone(),
        search_match_index,
        search_total_matches,
        reverse_video: ctx.state.settings.status_line.reverse_video,
        editor_bg: ctx.state.settings.editor_bg,
        editor_fg: ctx.state.settings.editor_fg,
    };

    if cache.status.as_ref() != Some(&current_status_state) {
        compositor.clear_layer(LayerPriority::STATUS_BAR);
        StatusBar::render_to_layer(
            compositor.get_layer_mut(LayerPriority::STATUS_BAR),
            &current_status_state,
        );
        cache.status = Some(current_status_state);
    }

    // 3. Command line / Floating window
    let mut command_cursor_info = None;
    let current_command_state =
        if ctx.current_mode == Mode::Command || ctx.current_mode == Mode::Search {
            Some(CommandDrawState {
                content: ctx.state.command_line.clone(),
                cursor: CursorInfo {
                    row: 0, // Command line is single line for now
                    col: ctx.state.command_line_cursor,
                },
                width: ctx.viewport.visible_cols(),
                height: ctx.state.settings.command_line_window.height,
                has_border: ctx.state.settings.command_line_window.border,
                reverse_video: ctx.state.settings.command_line_window.reverse_video,
                editor_bg: ctx.state.settings.editor_bg,
                editor_fg: ctx.state.settings.editor_fg,
            })
        } else {
            None
        };

    if cache.command_line.as_ref() != current_command_state.as_ref() {
        if current_command_state.is_some() {
            compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
            // Render command line
            let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);
            let default_border_chars = ctx.state.settings.default_border_chars.clone();
            let (window_row, window_col, _, offset) = CommandLine::render_to_layer(
                layer,
                ctx.viewport,
                &ctx.state.command_line,
                ctx.state.command_line_cursor,
                crate::command_line::RenderOptions {
                    default_border_chars,
                    window_settings: &ctx.state.settings.command_line_window,
                    fg: ctx.state.settings.editor_fg,
                    bg: ctx.state.settings.editor_bg,
                    prompt: if ctx.current_mode == Mode::Search {
                        '/'
                    } else {
                        ':'
                    },
                },
            );

            // Calculate cursor position in command window
            let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
                (window_row, window_col),
                ctx.state.command_line_cursor,
                offset,
                ctx.state.settings.command_line_window.border,
            );
            command_cursor_info = Some(CursorPosition::Absolute(cursor_row, cursor_col));
            cache.last_command_cursor = command_cursor_info;
        } else {
            // Transitioned from Command/Search mode to something else
            compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
            cache.last_command_cursor = None;
        }
        cache.command_line = current_command_state;
    } else if current_command_state.is_some() {
        // Cache hit, retrieve cursor info
        command_cursor_info = cache.last_command_cursor;
    }

    // 4. Render notifications
    let current_notification_state = NotificationDrawState {
        generation: ctx.state.error_manager.notifications().generation,
        count: ctx
            .state
            .error_manager
            .notifications()
            .iter_active()
            .count(),
    };

    if cache.notifications.as_ref() != Some(&current_notification_state) {
        compositor.clear_layer(LayerPriority::NOTIFICATION);
        render_notifications(
            compositor.get_layer_mut(LayerPriority::NOTIFICATION),
            ctx.state,
            ctx.viewport.visible_rows(),
            ctx.viewport.visible_cols(),
        );
        cache.notifications = Some(current_notification_state);
    }

    // Determine final cursor position
    let cursor_info = if let Some(pos) = command_cursor_info {
        pos
    } else {
        // Calculate normal cursor position
        let cursor_line = ctx.buf.get_line();
        let cursor_line_in_viewport = if cursor_line >= ctx.viewport.top_line()
            && cursor_line < ctx.viewport.top_line() + ctx.viewport.visible_rows()
        {
            cursor_line - ctx.viewport.top_line()
        } else {
            0
        };

        // Gutter width is cached in state
        let gutter_width = if ctx.state.settings.show_line_numbers {
            ctx.state.gutter_width
        } else {
            0
        };

        let cursor_col = calculate_cursor_column(ctx.buf, cursor_line, ctx.tab_width);

        // Add gutter width to cursor column
        // Subtract left_col for horizontal scrolling
        let visual_cursor_col = cursor_col.saturating_sub(ctx.viewport.left_col());

        // Ensure cursor doesn't go onto gutter or past right edge
        let display_col =
            (visual_cursor_col + gutter_width).min(ctx.viewport.visible_cols().saturating_sub(1));

        CursorPosition::Absolute(cursor_line_in_viewport as u16, display_col as u16)
    };

    // 5. Render composited output to terminal
    let stats = compositor
        .render_to_terminal(term, false)
        .map_err(|e| RiftError::new(crate::error::ErrorType::Renderer, "RENDER_FAILED", e))?;

    // 6. Position cursor
    if stats.changed_cells > 0 || cache.last_cursor_pos != Some(cursor_info) {
        match cursor_info {
            CursorPosition::Absolute(row, col) => {
                term.move_cursor(row, col)?;
            }
        }
        cache.last_cursor_pos = Some(cursor_info);
    }
    term.show_cursor()?;

    Ok(cursor_info)
}

/// Map syntax highlight capture names to colors
fn highlight_color(capture_name: &str, theme_colors: Option<&SyntaxColors>) -> Option<Color> {
    // Handle sub-scopes (e.g., function.builtin -> function)
    let base_name = capture_name.split('.').next().unwrap_or(capture_name);

    if let Some(theme) = theme_colors {
        match capture_name {
            crate::constants::captures::CONSTRUCTOR => return Some(theme.constructor),
            "function.builtin" | crate::constants::captures::BUILTIN => return Some(theme.builtin),
            _ => {}
        }

        match base_name {
            crate::constants::captures::KEYWORD => return Some(theme.keyword),
            crate::constants::captures::TYPE => return Some(theme.type_def),
            crate::constants::captures::FUNCTION => return Some(theme.function),
            crate::constants::captures::STRING => return Some(theme.string),
            crate::constants::captures::NUMBER => return Some(theme.number),
            crate::constants::captures::CONSTANT => return Some(theme.constant),
            crate::constants::captures::BOOLEAN => return Some(theme.boolean),
            crate::constants::captures::COMMENT => return Some(theme.comment),
            crate::constants::captures::VARIABLE => return Some(theme.variable),
            "parameter" => return Some(theme.parameter),
            crate::constants::captures::PROPERTY | crate::constants::captures::FIELD => {
                return Some(theme.property)
            }
            crate::constants::captures::ATTRIBUTE | crate::constants::captures::LABEL => {
                return Some(theme.attribute)
            }
            crate::constants::captures::NAMESPACE | crate::constants::captures::MODULE => {
                return Some(theme.namespace)
            }
            crate::constants::captures::OPERATOR => return Some(theme.operator),
            crate::constants::captures::PUNCTUATION => return Some(theme.punctuation),
            crate::constants::captures::CONSTRUCTOR => return Some(theme.constructor),
            crate::constants::captures::BUILTIN => return Some(theme.builtin),
            _ => {}
        }
    }

    // Fallback to hardcoded defaults if no theme colors specified or unknown capture
    match base_name {
        crate::constants::captures::KEYWORD => Some(Color::Magenta),
        crate::constants::captures::TYPE => Some(Color::Yellow),
        crate::constants::captures::FUNCTION | crate::constants::captures::CONSTRUCTOR => {
            Some(Color::Blue)
        }
        crate::constants::captures::STRING => Some(Color::Green),
        crate::constants::captures::NUMBER
        | crate::constants::captures::CONSTANT
        | crate::constants::captures::BOOLEAN => Some(Color::Yellow),
        crate::constants::captures::COMMENT => Some(Color::DarkGrey),
        crate::constants::captures::VARIABLE => Some(Color::Cyan),
        "parameter" => Some(Color::White),
        crate::constants::captures::PROPERTY | crate::constants::captures::FIELD => {
            Some(Color::Blue)
        }
        crate::constants::captures::ATTRIBUTE | crate::constants::captures::LABEL => {
            Some(Color::Yellow)
        }
        crate::constants::captures::NAMESPACE | crate::constants::captures::MODULE => {
            Some(Color::Cyan)
        }
        crate::constants::captures::OPERATOR => Some(Color::White),
        crate::constants::captures::PUNCTUATION => Some(Color::White),
        crate::constants::captures::ESCAPE | crate::constants::captures::EMBEDDED => {
            Some(Color::Grey)
        }
        _ => None,
    }
}

/// Render buffer content to the content layer
fn render_content_to_layer(layer: &mut Layer, ctx: &DrawContext) -> Result<(), String> {
    let buf = ctx.buf;
    let viewport = ctx.viewport;
    let editor_bg = ctx.state.settings.editor_bg;
    let editor_fg = ctx.state.settings.editor_fg;

    // Gutter width is based on total lines
    let gutter_width = if ctx.state.settings.show_line_numbers {
        ctx.state.gutter_width
    } else {
        0
    };

    // Render visible lines
    let top_line = viewport.top_line();
    let visible_rows = viewport.visible_rows().saturating_sub(1); // Reserve one row for status bar
    let visible_cols = viewport.visible_cols();

    for i in 0..visible_rows {
        let line_num = top_line + i;

        // Draw line numbers
        if gutter_width > 0 {
            if line_num < ctx.state.total_lines {
                // Show number for valid lines
                let line_str = format!("{:width$}", line_num + 1, width = gutter_width - 1);
                // Draw line number
                for (col, ch) in line_str.chars().enumerate() {
                    layer.set_cell(
                        i,
                        col,
                        Cell::new(ch as u8).with_colors(editor_fg, editor_bg),
                    );
                }
                // Draw separator
                layer.set_cell(
                    i,
                    gutter_width - 1,
                    Cell::new(b' ').with_colors(editor_fg, editor_bg),
                );
            } else {
                // Empty gutter for non-existent lines
                for col in 0..gutter_width {
                    layer.set_cell(i, col, Cell::new(b' ').with_colors(editor_fg, editor_bg));
                }
            }
        }

        if line_num < buf.get_total_lines() {
            let line_start_byte = buf.line_index.get_start(line_num).unwrap_or(0);
            let line_bytes = buf.get_line_bytes(line_num);
            let line_str = String::from_utf8_lossy(&line_bytes);

            // Write line content
            // We need to skip visual columns based on viewport.left_col
            let content_cols = visible_cols.saturating_sub(gutter_width);
            let mut visual_col = 0;
            let mut rendered_col = 0;
            let left_col = viewport.left_col();
            let mut byte_offset_in_line = 0usize;

            for ch in line_str.chars() {
                if rendered_col >= content_cols {
                    break;
                }

                // Calculate absolute byte offset for this character
                let abs_byte_offset = line_start_byte + byte_offset_in_line;
                let char_len = ch.len_utf8();
                let abs_end = abs_byte_offset + char_len;

                // 1. Check for search match (highest priority)
                let is_match = !ctx.state.search_matches.is_empty()
                    && ctx.state.search_matches.iter().any(|m| {
                        let start = std::cmp::max(m.range.start, abs_byte_offset);
                        let end = std::cmp::min(m.range.end, abs_end);
                        start < end
                    });

                // 2. Check for syntax highlighting
                let syntax_fg = if let Some(highlights) = ctx.highlights {
                    // Find all highlights containing this byte and pick the most specific one (shortest range)
                    highlights
                        .iter()
                        .filter(|(range, _)| range.contains(&abs_byte_offset))
                        .min_by_key(|(range, _)| range.end - range.start)
                        .and_then(|(_, name)| {
                            highlight_color(name, ctx.state.settings.syntax_colors.as_ref())
                        })
                        .or(editor_fg)
                } else {
                    editor_fg
                };

                // Determine final colors
                let (fg, bg) = if is_match {
                    (Some(Color::Black), Some(Color::Yellow))
                } else {
                    (syntax_fg, editor_bg)
                };

                // Track visual column (handling tabs and wide chars)
                let char_width = if ch == '\t' {
                    ctx.tab_width - (visual_col % ctx.tab_width)
                } else {
                    UnicodeWidthChar::width(ch).unwrap_or(1)
                };

                let next_visual_col = visual_col + char_width;

                // If any part of this character is visible (>= left_col)
                if next_visual_col > left_col {
                    // Only render if we haven't exceeded width
                    if rendered_col < content_cols {
                        let display_col = rendered_col + gutter_width;
                        if display_col < visible_cols {
                            layer.set_cell(i, display_col, Cell::from_char(ch).with_colors(fg, bg));

                            // For wide characters, fill the remaining columns with empty content
                            if char_width > 1 {
                                let empty_cell = Cell {
                                    content: Vec::new(),
                                    fg,
                                    bg,
                                };
                                for k in 1..char_width {
                                    if display_col + k < visible_cols {
                                        layer.set_cell(i, display_col + k, empty_cell.clone());
                                    }
                                }
                            }
                        }
                        rendered_col += char_width; // Advance by visual width
                    }
                }
                visual_col = next_visual_col;
                byte_offset_in_line += char_len;
            }

            // Pad with spaces
            for col in (rendered_col + gutter_width)..visible_cols {
                layer.set_cell(i, col, Cell::new(b' ').with_colors(editor_fg, editor_bg));
            }
        } else {
            // Empty line - fill with spaces
            for col in gutter_width..visible_cols {
                layer.set_cell(i, col, Cell::new(b' ').with_colors(editor_fg, editor_bg));
            }
        }
    }

    Ok(())
}

/// Calculate the cursor column position accounting for tab width and wide characters
pub fn calculate_cursor_column(buf: &TextBuffer, line: usize, tab_width: usize) -> usize {
    if line >= buf.get_total_lines() {
        return 0;
    }

    let line_bytes = buf.get_line_bytes(line);
    let cursor_offset = buf.cursor() - buf.line_index.get_start(line).unwrap_or(0);

    // Calculate visual column up to cursor offset
    let mut col = 0;
    let mut current_byte = 0;
    let line_str = String::from_utf8_lossy(&line_bytes);

    for ch in line_str.chars() {
        if current_byte >= cursor_offset {
            break;
        }

        if ch == '\t' {
            col += tab_width - (col % tab_width);
        } else {
            col += UnicodeWidthChar::width(ch).unwrap_or(1);
        }
        current_byte += ch.len_utf8();
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
fn render_notifications(
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
                    Cell::new(b' ').with_colors(Some(Color::White), Some(color)),
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
                        content: Vec::new(),
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
