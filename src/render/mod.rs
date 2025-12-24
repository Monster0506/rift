//! Rendering system
//! Handles drawing the editor UI to the terminal


/// ## render/ Invariants
///
/// - Rendering reads editor state and buffer contents only.
/// - Rendering never mutates editor, buffer, or cursor state.
/// - Rendering performs no input handling.
/// - Rendering tolerates invalid state but never corrects it.
/// - Displayed cursor position always matches buffer cursor position.
/// - A full redraw is always safe.
use crate::buffer::GapBuffer;
use crate::mode::Mode;
use crate::key::Key;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use crate::state::State;
use crate::status::StatusBar;
use crate::command_line::CommandLine;

/// Render the editor interface
pub fn render<T: TerminalBackend>(
    term: &mut T,
    buf: &GapBuffer,
    viewport: &mut Viewport,
    current_mode: Mode,
    pending_key: Option<Key>,
    state: &State,
) -> Result<(), String> {
    // Hide cursor during rendering to reduce flicker
    term.hide_cursor()?;
    
    // Update viewport based on cursor position
    let cursor_line = buf.get_line();
    let total_lines = buf.get_total_lines();
    let needs_clear = viewport.update(cursor_line, total_lines);

    // Clear screen if viewport scrolled or on first render
    // This reduces flicker when just moving cursor within visible area
    if needs_clear {
        term.clear_screen()?;
    }
    
    // Always render content (it handles positioning efficiently)
    render_content(term, buf, viewport)?;

    // Render command line floating window if in command mode
    let cmd_window_info = if current_mode == Mode::Command {
        CommandLine::render(term, viewport, &state.command_line, state.default_border_chars.clone())?
    } else {
        // Always render status bar (it may have changed)
        StatusBar::render(term, viewport, current_mode, pending_key, state)?;
        None
    };

    // Show cursor and position it at the correct location
    term.show_cursor()?;
    
    if let Some((window_row, window_col, cmd_width)) = cmd_window_info {
        // Position cursor in the centered command line window
        let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
            (window_row, window_col),
            cmd_width,
            &state.command_line,
        );
        term.move_cursor(cursor_row, cursor_col)?;
    } else {
        let cursor_line_in_viewport = if cursor_line >= viewport.top_line() 
            && cursor_line < viewport.top_line() + viewport.visible_rows().saturating_sub(1) {
            cursor_line - viewport.top_line()
        } else {
            0
        };
        
        let cursor_col = calculate_cursor_column(buf, cursor_line);
        let display_col = cursor_col.min(viewport.visible_cols().saturating_sub(1));
        
        term.move_cursor(cursor_line_in_viewport as u16, display_col as u16)?;
    }
    
    Ok(())
}

fn render_content<T: TerminalBackend>(
    term: &mut T,
    buf: &GapBuffer,
    viewport: &Viewport,
) -> Result<(), String> {
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
            // Skip carriage return (Windows line endings: \r\n)
            // We only care about \n for line breaks
            current_line.push(byte);
        }
    }
    
    // Process after_gap
    for &byte in after_gap {
        if byte == b'\n' {
            lines.push(current_line);
            current_line = Vec::new();
        } else if byte != b'\r' {
            // Skip carriage return (Windows line endings: \r\n)
            // We only care about \n for line breaks
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
        // Position cursor at the start of this line
        term.move_cursor(i as u16, 0)?;
        
        let line_num = top_line + i;
        if line_num < lines.len() {
            let line = &lines[line_num];
            // Truncate line to visible width
            let display_line: Vec<u8> = line.iter()
                .take(visible_cols)
                .copied()
                .collect();
            
            // Pad with spaces if line is shorter than visible width
            let mut padded_line = display_line;
            while padded_line.len() < visible_cols {
                padded_line.push(b' ');
            }
            
            // Write the line content
            term.write(&padded_line)?;
        } else {
            // Empty line - fill with spaces
            term.write(&vec![b' '; visible_cols])?;
        }
        
        // Clear to end of line to remove any leftover content
        term.clear_to_end_of_line()?;
    }
    
    Ok(())
}

// Re-export format_key for backward compatibility with tests
pub(crate) fn _format_key(key: Key) -> String {
    StatusBar::format_key(key)
}

// Tab width (hardcoded for now, should be a setting later)
const TAB_WIDTH: usize = 8;

/// Calculate the visual column position accounting for tab width
/// If start_col is provided, continues from that column position
fn calculate_visual_column(line_bytes: &[u8], start_col: usize) -> usize {
    let mut col = start_col;
    for &byte in line_bytes {
        if byte == b'\t' {
            // Move to next tab stop
            col = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
        } else {
            col += 1;
        }
    }
    col
}

pub(crate) fn calculate_cursor_column(buf: &GapBuffer, line: usize) -> usize {
    let before_gap = buf.get_before_gap();
    let mut current_line = 0;
    let mut line_start = 0;
    
    // Find the start of the target line
    for (i, &byte) in before_gap.iter().enumerate() {
        if byte == b'\n' {
            if current_line == line {
                // Found the line, calculate visual column up to gap position
                let line_bytes = &before_gap[line_start..i];
                return calculate_visual_column(line_bytes, 0);
            }
            current_line += 1;
            line_start = i + 1;
        }
    }
    
    // If we're at the gap position on the target line
    if current_line == line {
        let line_bytes = &before_gap[line_start..];
        return calculate_visual_column(line_bytes, 0);
    }
    
    // Check after_gap - need to include before_gap bytes from line_start
    let after_gap = buf.get_after_gap();
    // First, calculate column for before_gap portion of this line
    let before_line_bytes = &before_gap[line_start..];
    let mut col = calculate_visual_column(before_line_bytes, 0);
    
    // Now process after_gap bytes
    for (i, &byte) in after_gap.iter().enumerate() {
        if byte == b'\n' {
            if current_line == line {
                // Found the line in after_gap, include bytes up to this newline
                let after_line_bytes = &after_gap[..i];
                return calculate_visual_column(after_line_bytes, col);
            }
            current_line += 1;
            col = 0;
        }
    }
    
    // If we're at the end of the target line (after gap, no newline found)
    if current_line == line {
        // Include all remaining after_gap bytes, continuing from col
        return calculate_visual_column(after_gap, col);
    }
    
    0
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;


