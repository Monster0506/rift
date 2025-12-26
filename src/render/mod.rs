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
use crate::floating_window::{FloatingWindow, WindowPosition};
use crate::key::Key;
use crate::layer::{Cell, Layer, LayerCompositor, LayerPriority};
use crate::mode::Mode;
use crate::state::State;
use crate::status::StatusBar;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;

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
///
/// Returns the cursor position for the terminal.
pub fn render<T: TerminalBackend>(
    term: &mut T,
    compositor: &mut LayerCompositor,
    buf: &GapBuffer,
    viewport: &Viewport,
    current_mode: Mode,
    pending_key: Option<Key>,
    state: &State,
    needs_clear: bool,
) -> Result<CursorPosition, String> {
    // Resize compositor if needed
    if compositor.rows() != viewport.visible_rows() || compositor.cols() != viewport.visible_cols()
    {
        compositor.resize(viewport.visible_rows(), viewport.visible_cols());
    }

    // Clear all layers before rendering
    compositor.clear_all();

    // 1. Render content to CONTENT layer
    render_content_to_layer(
        compositor.get_layer_mut(LayerPriority::CONTENT),
        buf,
        viewport,
        state.settings.editor_bg,
        state.settings.editor_fg,
    );

    // 2. Always render status bar to STATUS_BAR layer (visible in all modes)
    StatusBar::render_to_layer(
        compositor.get_layer_mut(LayerPriority::STATUS_BAR),
        viewport,
        current_mode,
        pending_key,
        state,
    );

    // 3. Render command window on top if in command mode
    let cursor_info = if current_mode == Mode::Command {
        // Render command line to FLOATING_WINDOW layer (renders on top of status bar)
        let layer = compositor.get_layer_mut(LayerPriority::FLOATING_WINDOW);

        // Calculate command window dimensions
        let cmd_width = ((viewport.visible_cols() as f64
            * state.settings.command_line_window.width_ratio) as usize)
            .max(state.settings.command_line_window.min_width)
            .min(viewport.visible_cols());

        let cmd_window = FloatingWindow::new(
            WindowPosition::Center,
            cmd_width,
            state.settings.command_line_window.height,
        )
        .with_border(state.settings.command_line_window.border)
        .with_reverse_video(state.settings.command_line_window.reverse_video);

        // Prepare content
        let mut content_line = Vec::new();
        content_line.push(b':');
        content_line.extend_from_slice(state.command_line.as_bytes());

        // Render to layer using new API
        cmd_window.render_with_border_chars(
            layer,
            &[content_line],
            state.settings.default_border_chars.clone(),
        );

        // Calculate cursor position in command window
        let window_pos = cmd_window.calculate_position(
            viewport.visible_rows() as u16,
            viewport.visible_cols() as u16,
        );
        let (cursor_row, cursor_col) =
            CommandLine::calculate_cursor_position(window_pos, cmd_width, &state.command_line);
        CursorPosition::Absolute(cursor_row, cursor_col)
    } else {
        // Calculate normal cursor position
        let cursor_line = buf.get_line();
        let cursor_line_in_viewport = if cursor_line >= viewport.top_line()
            && cursor_line < viewport.top_line() + viewport.visible_rows().saturating_sub(1)
        {
            cursor_line - viewport.top_line()
        } else {
            0
        };

        let cursor_col = calculate_cursor_column(buf, cursor_line, state.settings.tab_width);
        let display_col = cursor_col.min(viewport.visible_cols().saturating_sub(1));

        CursorPosition::Absolute(cursor_line_in_viewport as u16, display_col as u16)
    };

    // 4. Render composited output to terminal
    term.hide_cursor()?;
    compositor.render_to_terminal(term, needs_clear)?;
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
) {
    let before_gap = buf.get_before_gap();
    let after_gap = buf.get_after_gap();

    // Combine before and after gap to get full text
    let mut lines: Vec<Vec<u8>> = Vec::new();
    let mut current_line = Vec::new();

    // Process before_gap
    for &byte in before_gap {
        if byte == b'\n' {
            lines.push(current_line);
            current_line = Vec::new();
        } else if byte != b'\r' {
            current_line.push(byte);
        }
    }

    // Process after_gap
    for &byte in after_gap {
        if byte == b'\n' {
            lines.push(current_line);
            current_line = Vec::new();
        } else if byte != b'\r' {
            current_line.push(byte);
        }
    }

    // Add last line if not empty
    if !current_line.is_empty() || lines.is_empty() {
        lines.push(current_line);
    }

    // Render visible lines
    let top_line = viewport.top_line();
    let visible_rows = viewport.visible_rows().saturating_sub(1); // Reserve one row for status bar
    let visible_cols = viewport.visible_cols();

    for i in 0..visible_rows {
        let line_num = top_line + i;
        if line_num < lines.len() {
            let line = &lines[line_num];
            // Write line content
            for (col, &byte) in line.iter().take(visible_cols).enumerate() {
                layer.set_cell(i, col, Cell::new(byte).with_colors(editor_fg, editor_bg));
            }
            // Pad with spaces
            for col in line.len().min(visible_cols)..visible_cols {
                layer.set_cell(i, col, Cell::new(b' ').with_colors(editor_fg, editor_bg));
            }
        } else {
            // Empty line - fill with spaces
            for col in 0..visible_cols {
                layer.set_cell(i, col, Cell::new(b' ').with_colors(editor_fg, editor_bg));
            }
        }
    }
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
