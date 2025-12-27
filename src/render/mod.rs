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
use crate::buffer::GapBuffer;
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

/// Context for rendering
pub struct RenderContext<'a> {
    pub buf: &'a GapBuffer,
    pub viewport: &'a Viewport,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub state: &'a State,
    pub needs_clear: bool,
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
) -> Result<CursorPosition, RiftError> {
    // Resize compositor if needed
    if compositor.rows() != ctx.viewport.visible_rows()
        || compositor.cols() != ctx.viewport.visible_cols()
    {
        compositor.resize(ctx.viewport.visible_rows(), ctx.viewport.visible_cols());
    }

    // Clear all layers before rendering
    compositor.clear_all();

    // 1. Render content to CONTENT layer
    render_content_to_layer(
        compositor.get_layer_mut(LayerPriority::CONTENT),
        ctx.buf,
        ctx.viewport,
        ctx.state.settings.editor_bg,
        ctx.state.settings.editor_fg,
        &ctx,
    );

    // 2. Always render status bar to STATUS_BAR layer (visible in all modes)
    StatusBar::render_to_layer(
        compositor.get_layer_mut(LayerPriority::STATUS_BAR),
        ctx.viewport,
        ctx.current_mode,
        ctx.pending_key,
        ctx.state,
    );

    // 3. Render command window on top if in command mode
    let cursor_info = if ctx.current_mode == Mode::Command {
        // Render command line to FLOATING_WINDOW layer (renders on top of status bar)
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);

        let default_border_chars = ctx.state.settings.default_border_chars.clone();
        let (window_row, window_col, _, offset) = CommandLine::render_to_layer(
            layer,
            ctx.viewport,
            &ctx.state.command_line,
            ctx.state.command_line_cursor,
            default_border_chars,
            &ctx.state.settings.command_line_window,
        );

        // Calculate cursor position in command window
        let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
            (window_row, window_col),
            ctx.state.command_line_cursor,
            offset,
            ctx.state.settings.command_line_window.border,
        );
        CursorPosition::Absolute(cursor_row, cursor_col)
    } else {
        // Calculate normal cursor position
        let cursor_line = ctx.buf.get_line();
        let cursor_line_in_viewport = if cursor_line >= ctx.viewport.top_line()
            && cursor_line < ctx.viewport.top_line() + ctx.viewport.visible_rows().saturating_sub(1)
        {
            cursor_line - ctx.viewport.top_line()
        } else {
            0
        };

        // Calculate gutter width
        let gutter_width = if ctx.state.settings.show_line_numbers {
            calculate_gutter_width(ctx.state.total_lines)
        } else {
            0
        };

        let cursor_col =
            calculate_cursor_column(ctx.buf, cursor_line, ctx.state.settings.tab_width);

        // Add gutter width to cursor column
        // Subtract left_col for horizontal scrolling
        let visual_cursor_col = cursor_col.saturating_sub(ctx.viewport.left_col());

        // Ensure cursor doesn't go onto gutter or past right edge
        let display_col =
            (visual_cursor_col + gutter_width).min(ctx.viewport.visible_cols().saturating_sub(1));

        CursorPosition::Absolute(cursor_line_in_viewport as u16, display_col as u16)
    };

    render_notifications(
        compositor.get_layer_mut(LayerPriority::NOTIFICATION),
        ctx.state,
        ctx.viewport.visible_rows(),
        ctx.viewport.visible_cols(),
    );

    // 4. Render composited output to terminal
    term.hide_cursor()?;
    let _ = compositor.render_to_terminal(term, ctx.needs_clear)?;
    term.show_cursor()?;

    // 5. Position cursor
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
    buf: &GapBuffer,
    viewport: &Viewport,
    editor_bg: Option<Color>,
    editor_fg: Option<Color>,
    ctx: &RenderContext,
) {
    // Calculate gutter width using helper function
    let gutter_width = if ctx.state.settings.show_line_numbers {
        calculate_gutter_width(ctx.state.total_lines)
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
            let line = buf.get_line_bytes(line_num);
            // Write line content
            // We need to skip visual columns based on viewport.left_col
            let content_cols = visible_cols.saturating_sub(gutter_width);
            let mut visual_col = 0;
            let mut rendered_col = 0;
            let left_col = viewport.left_col();

            for &byte in &line {
                if rendered_col >= content_cols {
                    break;
                }

                // Track visual column (handling tabs)
                let char_width = if byte == b'\t' {
                    ctx.state.settings.tab_width - (visual_col % ctx.state.settings.tab_width)
                } else {
                    1
                };

                let next_visual_col = visual_col + char_width;

                // If any part of this character is visible (>= left_col)
                if next_visual_col > left_col {
                    // Check if we need to skip part of the character (e.g. part of a tab)
                    // Currently simplification: just render if it ends after left_col

                    // Only render if we haven't exceeded width
                    if rendered_col < content_cols {
                        let display_col = rendered_col + gutter_width;
                        if display_col < visible_cols {
                            layer.set_cell(
                                i,
                                display_col,
                                Cell::new(byte).with_colors(editor_fg, editor_bg),
                            );
                        }
                        rendered_col += char_width; // Advance by char width (e.g. tab)
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

/// Calculate gutter width for a given number of lines
pub fn calculate_gutter_width(total_lines: usize) -> usize {
    if total_lines == 0 {
        return 0;
    }
    total_lines.to_string().len() + 1
}

/// Calculate the visual column position accounting for tab width
/// If `start_col` is provided, continues from that column position
fn calculate_visual_column(line_bytes: &[u8], start_col: usize, tab_width: usize) -> usize {
    let mut col = start_col;
    for &byte in line_bytes {
        if byte == b'\t' {
            // Move to next tab stop
            col = ((col / tab_width) + 1) * tab_width;
        } else {
            col += 1;
        }
    }
    col
}

/// Calculate the cursor column position in the buffer
pub(crate) fn calculate_cursor_column(buf: &GapBuffer, line: usize, tab_width: usize) -> usize {
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
        return calculate_visual_column(after_gap, col, tab_width);
    }

    0
}

// Re-export format_key for backward compatibility with tests
pub(crate) fn _format_key(key: Key) -> String {
    StatusBar::format_key(key)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Helper to wrap text to a specific width
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.len() + word.len() + 1 > width && !current_line.is_empty() {
            lines.push(current_line);
            current_line = String::new();
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
        let style = crate::floating_window::WindowStyle::default()
            .with_border(true)
            .with_reverse_video(false)
            .with_fg(title_color.unwrap_or(Color::White));

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
        let content_bytes: Vec<Vec<u8>> =
            lines.iter().map(|line| line.as_bytes().to_vec()).collect();

        window.render(layer, &content_bytes);

        // Move up for next notification (preserve existing padding logic)
        current_bottom = start_row.saturating_sub(1);
    }
}
