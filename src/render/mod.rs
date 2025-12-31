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
use crate::color::Color;
use crate::command_line::CommandLine;
use crate::error::RiftError;
use crate::floating_window::{FloatingWindow, WindowPosition};
use crate::key::Key;
use crate::layer::{Cell, Layer, LayerCompositor, LayerPriority};
use crate::mode::Mode;
use crate::state::State;
use crate::status::StatusBar;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use unicode_width::UnicodeWidthChar;

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
}

impl RenderCache {
    pub fn invalidate_all(&mut self) {
        self.content = None;
        self.status = None;
        self.command_line = None;
        self.notifications = None;
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
    ctx: RenderContext,
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
pub struct RenderContext<'a> {
    pub buf: &'a TextBuffer,
    pub viewport: &'a Viewport,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
}

/// Cursor position information returned from layer-based rendering
#[derive(Debug, Clone, Copy)]
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
    ctx: RenderContext,
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
    };

    if cache.content.as_ref() != Some(&current_content_state) {
        compositor.mark_dirty(LayerPriority::CONTENT);
        render_content_to_layer(
            compositor.get_layer_mut(LayerPriority::CONTENT),
            ctx.buf,
            ctx.viewport,
            ctx.state.settings.editor_bg,
            ctx.state.settings.editor_fg,
            &ctx,
        );
        cache.content = Some(current_content_state);
    }

    // 2. Render status bar to STATUS_BAR layer (visible in all modes)
    let current_status_state = StatusDrawState {
        mode: ctx.current_mode,
        pending_key: ctx.pending_key,
        pending_count: ctx.pending_count,
        file_name: ctx.state.file_name.clone(),
        is_dirty: ctx.state.is_dirty,
        cursor: CursorInfo {
            row: ctx.buf.get_line(),
            col: calculate_cursor_column(ctx.buf, ctx.buf.get_line(), ctx.tab_width),
        },
        total_lines: ctx.state.total_lines,
        debug_mode: ctx.state.debug_mode,
        cols: ctx.viewport.visible_cols(),
    };

    if cache.status.as_ref() != Some(&current_status_state) {
        compositor.clear_layer(LayerPriority::STATUS_BAR);
        StatusBar::render_to_layer(
            compositor.get_layer_mut(LayerPriority::STATUS_BAR),
            ctx.viewport,
            ctx.current_mode,
            ctx.pending_key,
            ctx.pending_count,
            ctx.state,
        );
        cache.status = Some(current_status_state);
    }

    // 3. Render command window on top if in command mode or search mode
    let mut command_cursor_info = None;
    let current_command_state = if ctx.current_mode == Mode::Command
        || ctx.current_mode == Mode::Search
    {
        Some(CommandDrawState {
            content: ctx.state.command_line.clone(),
            cursor: CursorInfo {
                row: 0, // Command line is single line for now
                col: ctx.state.command_line_cursor,
            },
            width: ((ctx.viewport.visible_cols() as f64
                * ctx.state.settings.command_line_window.width_ratio) as usize)
                .max(ctx.state.settings.command_line_window.min_width)
                .min(ctx.viewport.visible_cols()),
            height: ctx.state.settings.command_line_window.height,
            has_border: ctx.state.settings.command_line_window.border,
            reverse_video: ctx.state.settings.command_line_window.reverse_video,
        })
    } else {
        None
    };

    if cache.command_line != current_command_state {
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
        } else {
            // Transitioned from Command/Search mode to something else
            compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
        }
        cache.command_line = current_command_state;
    } else if let Some(ref state) = current_command_state {
        // State is same, but we still need cursor pos info if we were already in command mode
        // We could cache the cursor_info too, or just re-calculate it since it's cheap.
        // For simplicity, let's just re-calculate it here (re-using identical logic as above)
        let cmd_width = state.width;
        let cmd_height = state.height;
        let has_border = state.has_border;

        // Note: We need window_row/window_col which are calculated during render.
        // This is a bit tricky if we skip render.
        // Let's store the cursor location in the cache or just re-calculate window pos.
        let floating_window = crate::floating_window::FloatingWindow::new(
            WindowPosition::Center,
            cmd_width,
            cmd_height,
        )
        .with_border(has_border);
        let (window_row, window_col) = floating_window.calculate_position(
            ctx.viewport.visible_rows() as u16,
            ctx.viewport.visible_cols() as u16,
        );

        // We also need the 'offset' which CommandLine::render_to_layer calculates.
        // Let's add 'offset' to CommandDrawState if we want to avoid re-rendering but still have cursor info.
        // Actually, let's just re-render if we need simplicity, or add it to the DrawState.
        // If any component depends on something computed during render, include it.
        // Let's just re-calculate the offset logic here (it's O(1)).
        let border_offset = if has_border { 2 } else { 0 };
        let available_cmd_width = cmd_width.saturating_sub(border_offset).saturating_sub(1);
        let offset = if state.content.len() <= available_cmd_width {
            0
        } else if state.cursor.col >= available_cmd_width {
            state
                .cursor
                .col
                .saturating_sub(available_cmd_width)
                .saturating_add(1)
        } else {
            0
        };

        let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
            (window_row, window_col),
            state.cursor.col,
            offset,
            has_border,
        );
        command_cursor_info = Some(CursorPosition::Absolute(cursor_row, cursor_col));
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
    term.hide_cursor()?;
    let _ = compositor.render_to_terminal(term, ctx.needs_clear)?;
    term.show_cursor()?;

    // 6. Position cursor
    match cursor_info {
        CursorPosition::Absolute(row, col) => {
            term.move_cursor(row, col)?;
        }
    }

    Ok(cursor_info)
}

/// Render buffer content to a layer
fn render_content_to_layer(
    layer: &mut Layer,
    buf: &TextBuffer,
    viewport: &Viewport,
    editor_bg: Option<Color>,
    editor_fg: Option<Color>,
    ctx: &RenderContext,
) {
    // Use cached gutter width from state
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
            let line_bytes = buf.get_line_bytes(line_num);
            let line_str = String::from_utf8_lossy(&line_bytes);

            // Write line content
            // We need to skip visual columns based on viewport.left_col
            let content_cols = visible_cols.saturating_sub(gutter_width);
            let mut visual_col = 0;
            let mut rendered_col = 0;
            let left_col = viewport.left_col();

            for ch in line_str.chars() {
                if rendered_col >= content_cols {
                    break;
                }

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
                            layer.set_cell(
                                i,
                                display_col,
                                Cell::from_char(ch).with_colors(editor_fg, editor_bg),
                            );

                            // For wide characters, fill the remaining columns with empty content
                            // so we don't overwrite the wide character with spaces from the background
                            if char_width > 1 {
                                let empty_cell = Cell {
                                    content: Vec::new(),
                                    fg: editor_fg,
                                    bg: editor_bg,
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
}

/// Calculate the visual column position accounting for tab width and wide characters
fn calculate_visual_column(line_bytes: &[u8], start_col: usize, tab_width: usize) -> usize {
    let mut col = start_col;
    let line_str = String::from_utf8_lossy(line_bytes);
    for ch in line_str.chars() {
        if ch == '\t' {
            // Move to next tab stop
            col = ((col / tab_width) + 1) * tab_width;
        } else {
            col += UnicodeWidthChar::width(ch).unwrap_or(1);
        }
    }
    col
}

/// Calculate the cursor column position in the buffer
pub(crate) fn calculate_cursor_column(buf: &TextBuffer, line: usize, tab_width: usize) -> usize {
    let before_gap = buf.get_before_gap();
    let mut current_line = 0;
    let mut line_start = 0;

    // Find the start of the target line
    for (i, &byte) in before_gap.iter().enumerate() {
        if byte == b'\n' {
            if current_line == line {
                // Found the line, calculate visual column up to gap position
                let line_bytes = &before_gap[line_start..i];
                return calculate_visual_column(line_bytes, 0, tab_width);
            }
            current_line += 1;
            line_start = i + 1;
        }
    }

    // If we're at the gap position on the target line
    if current_line == line {
        let line_bytes = &before_gap[line_start..];
        return calculate_visual_column(line_bytes, 0, tab_width);
    }

    // Check after_gap - need to include before_gap bytes from line_start
    let after_gap = buf.get_after_gap();
    // First, calculate column for before_gap portion of this line
    let before_line_bytes = &before_gap[line_start..];
    let mut col = calculate_visual_column(before_line_bytes, 0, tab_width);

    // Now process after_gap bytes
    for (i, &byte) in after_gap.iter().enumerate() {
        if byte == b'\n' {
            if current_line == line {
                // Found the line in after_gap, include bytes up to this newline
                let after_line_bytes = &after_gap[..i];
                return calculate_visual_column(after_line_bytes, col, tab_width);
            }
            current_line += 1;
            col = 0;
        }
    }

    // If we're at the end of the target line (after gap, no newline found)
    if current_line == line {
        // Include all remaining after_gap bytes, continuing from col
        return calculate_visual_column(&after_gap, col, tab_width);
    }

    0
}

// Re-export format_key for backward compatibility with tests
pub(crate) fn _format_key(key: Key) -> String {
    StatusBar::format_key(key)
}

/// Helper to wrap text to a specific width
fn wrap_text(text: &str, width: usize) -> Vec<String> {
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
        // Handle case where text is empty
        if lines.is_empty() {
            lines.push(String::new());
        }
    }
    lines
}

/// Render active notifications
fn render_notifications(layer: &mut Layer, state: &State, term_rows: usize, term_cols: usize) {
    use crate::notification::NotificationType;
    let now = std::time::Instant::now();
    let notifications: Vec<_> = state
        .error_manager
        .notifications()
        .iter_active()
        .filter(|n| !n.is_expired(now))
        .collect();

    let mut current_bottom = term_rows.saturating_sub(2); // Start above bottom status line

    for notification in notifications.iter().rev() {
        // Simple styling
        let (_border_color, title_color) = match notification.kind {
            NotificationType::Info => (Some(Color::Blue), Some(Color::Cyan)),
            NotificationType::Warning => (Some(Color::Yellow), Some(Color::Yellow)),
            NotificationType::Error => (Some(Color::Red), Some(Color::Red)),
            NotificationType::Success => (Some(Color::Green), Some(Color::Green)),
        };

        // Format content
        let prefix = match notification.kind {
            NotificationType::Info => " [I] ",
            NotificationType::Warning => " [W] ",
            NotificationType::Error => " [E] ",
            NotificationType::Success => " [S] ",
        };
        let full_text = format!("{}{}", prefix, notification.message);

        // Calculate dimensions
        // Fixed width for now, but wrapped
        let width = 40;
        let content_width = width - 2; // Subtract borders

        let lines = wrap_text(&full_text, content_width - 2); // Subtract padding
        let height = lines.len() + 2; // Content lines + top border + bottom border

        // Skip if out of space
        if current_bottom < height {
            break;
        }

        let start_row = current_bottom.saturating_sub(height);

        // Create window with style
        let mut style = crate::floating_window::WindowStyle::default()
            .with_border(true)
            .with_reverse_video(false)
            .with_fg(title_color.unwrap_or(Color::White));

        if let Some(bg) = state.settings.editor_bg {
            style = style.with_bg(bg);
        }

        let window = FloatingWindow::with_style(
            WindowPosition::Absolute {
                row: start_row as u16,
                col: term_cols.saturating_sub(width + 2) as u16,
            },
            width,
            height,
            style,
        );

        // Render content lines
        let content_bytes: Vec<Vec<u8>> = lines.into_iter().map(|line| line.into_bytes()).collect();

        window.render(layer, &content_bytes);

        // Move up for next notification (preserve existing padding logic)
        current_bottom = start_row.saturating_sub(1);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
