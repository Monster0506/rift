//! Rendering system
//! Handles drawing the editor UI to the terminal


/// ## render/ Invariants
///
/// - Rendering reads editor state and buffer contents only.
/// - Rendering never mutates editor, buffer, cursor, or viewport state.
/// - Rendering performs no input handling.
/// - Rendering tolerates invalid state but never corrects it.
/// - Displayed cursor position always matches buffer cursor position.
/// - A full redraw is always safe.
/// - Viewport must be updated before calling `render()` (viewport updates happen
///   in the state update phase, not during rendering).
use crate::buffer::GapBuffer;
use crate::mode::Mode;
use crate::key::Key;
use crate::term::{TerminalBackend, ColorTerminal};
use crate::viewport::Viewport;
use crate::state::State;
use crate::status::StatusBar;
use crate::command_line::CommandLine;
use crate::color::{Color, ColorStyle};
use crate::color::buffer::ColorMap;
use crate::color::styled::StyledLine;

/// Helper function to apply editor colors using crossterm
/// Uses crossterm's command system to generate proper escape sequences
fn apply_editor_colors_to_terminal<T: TerminalBackend>(
    term: &mut T,
    bg: Option<Color>,
    fg: Option<Color>,
) -> Result<(), String> {
    use crossterm::style::{SetForegroundColor, SetBackgroundColor};
    use crossterm::queue;
    
    // Use a Vec<u8> as a writer to collect the escape sequences
    let mut buffer = Vec::new();
    
    // Apply foreground color if set
    if let Some(fg_color) = fg {
        queue!(buffer, SetForegroundColor(fg_color.to_crossterm()))
            .map_err(|e| format!("Failed to queue foreground color: {e}"))?;
    }
    
    // Apply background color if set
    if let Some(bg_color) = bg {
        queue!(buffer, SetBackgroundColor(bg_color.to_crossterm()))
            .map_err(|e| format!("Failed to queue background color: {e}"))?;
    }
    
    // Write the collected escape sequences to the terminal
    if !buffer.is_empty() {
        term.write(&buffer)?;
    }
    
    Ok(())
}

/// Render the editor interface
/// Viewport should already be updated before calling this function
pub fn render<T: TerminalBackend>(
    term: &mut T,
    buf: &GapBuffer,
    viewport: &Viewport,
    current_mode: Mode,
    pending_key: Option<Key>,
    state: &State,
    needs_clear: bool,
) -> Result<(), String> {
    // Hide cursor during rendering to reduce flicker
    term.hide_cursor()?;
    
    // Clear screen if viewport scrolled or on first render
    // This reduces flicker when just moving cursor within visible area
    if needs_clear {
        term.clear_screen()?;
    }
    
    // Always render content (it handles positioning efficiently)
    // Pass editor colors so they can be applied
    render_content(term, buf, viewport, None, state.settings.editor_bg, state.settings.editor_fg)?;

    // Render command line floating window if in command mode
    let cmd_window_info = if current_mode == Mode::Command {
        CommandLine::render(term, viewport, &state.command_line, state.settings.default_border_chars.clone(), &state.settings.command_line_window)?
    } else {
        // Always render status bar (it may have changed)
        StatusBar::render(term, viewport, current_mode, pending_key, state)?;
        None
    };

    // Show cursor and position it at the correct location
    term.show_cursor()?;
    
    // Calculate cursor position for rendering
    let cursor_line = buf.get_line();
    
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
        
        let cursor_col = calculate_cursor_column(buf, cursor_line, state.settings.tab_width);
        let display_col = cursor_col.min(viewport.visible_cols().saturating_sub(1));
        
        term.move_cursor(cursor_line_in_viewport as u16, display_col as u16)?;
    }
    
    Ok(())
}

/// Render a styled line to the terminal
/// Efficiently batches color changes to minimize ANSI escape sequences
fn render_styled_line<T: TerminalBackend + ColorTerminal>(
    term: &mut T,
    line: &StyledLine,
    visible_cols: usize,
) -> Result<(), String> {
    match line {
        StyledLine::Plain(text) => {
            // Plain text - just write it
            let display_line: Vec<u8> = text.iter()
                .take(visible_cols)
                .copied()
                .collect();
            
            let mut padded_line = display_line;
            while padded_line.len() < visible_cols {
                padded_line.push(b' ');
            }
            
            term.write(&padded_line)?;
        }
        StyledLine::PerChar(chars) => {
            // Per-character coloring
            let mut current_style = ColorStyle::new();
            let mut output = Vec::new();
            
            for styled_char in chars.iter().take(visible_cols) {
                // Check if we need to change colors
                if styled_char.style != current_style {
                    // Reset colors first
                    term.reset_colors()?;
                    current_style = ColorStyle::new();
                    
                    // Apply new foreground color if set
                    if let Some(fg) = styled_char.style.fg {
                        term.set_foreground_color(fg)?;
                        current_style.fg = Some(fg);
                    }
                    
                    // Apply new background color if set
                    if let Some(bg) = styled_char.style.bg {
                        term.set_background_color(bg)?;
                        current_style.bg = Some(bg);
                    }
                }
                
                output.push(styled_char.ch);
            }
            
            // Write accumulated output
            if output.is_empty() {
                // Empty line - fill with spaces
                term.write(&vec![b' '; visible_cols])?;
            } else {
                // Pad with spaces if needed
                while output.len() < visible_cols {
                    output.push(b' ');
                }
                term.write(&output)?;
            }
            
            // Reset colors at end of line
            term.reset_colors()?;
        }
        StyledLine::PerSpan { text, spans } => {
            // Per-span coloring - more efficient for large uniform spans
            let mut current_style = ColorStyle::new();
            let mut output = Vec::new();
            let mut span_idx = 0;
            
            for (i, &ch) in text.iter().take(visible_cols).enumerate() {
                // Check if we've moved past the current span
                while span_idx < spans.len() && spans[span_idx].end <= i {
                    span_idx += 1;
                }
                
                // Determine style for this character
                let char_style = if span_idx < spans.len() && spans[span_idx].start <= i && i < spans[span_idx].end {
                    spans[span_idx].style
                } else {
                    ColorStyle::new()
                };
                
                // Check if we need to change colors
                if char_style != current_style {
                    // Reset colors first
                    term.reset_colors()?;
                    current_style = ColorStyle::new();
                    
                    // Apply new foreground color if set
                    if let Some(fg) = char_style.fg {
                        term.set_foreground_color(fg)?;
                        current_style.fg = Some(fg);
                    }
                    
                    // Apply new background color if set
                    if let Some(bg) = char_style.bg {
                        term.set_background_color(bg)?;
                        current_style.bg = Some(bg);
                    }
                }
                
                output.push(ch);
            }
            
            // Write accumulated output
            if output.is_empty() {
                // Empty line - fill with spaces
                term.write(&vec![b' '; visible_cols])?;
            } else {
                // Pad with spaces if needed
                while output.len() < visible_cols {
                    output.push(b' ');
                }
                term.write(&output)?;
            }
            
            // Reset colors at end of line
            term.reset_colors()?;
        }
    }
    
    Ok(())
}

/// Render buffer content to terminal
/// Optionally applies colors from `ColorMap` if terminal supports it
fn render_content<T: TerminalBackend>(
    term: &mut T,
    buf: &GapBuffer,
    viewport: &Viewport,
    color_map: Option<&ColorMap>,
    editor_bg: Option<Color>,
    editor_fg: Option<Color>,
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
    
    // Check if terminal supports colors and we have a color map
    let use_colors = color_map.is_some();
    
    // Apply editor colors if set (before rendering first line)
    // Colors will persist for the entire rendering session
    if editor_bg.is_some() || editor_fg.is_some() {
        apply_editor_colors_to_terminal(term, editor_bg, editor_fg)?;
    }
    
    for i in 0..visible_rows {
        // Position cursor at the start of this line
        term.move_cursor(i as u16, 0)?;
        
            let line_num = top_line + i;
        if line_num < lines.len() {
            let line = &lines[line_num];
            
            // Try to render with colors if available
            let rendered_with_colors = if use_colors {
                if let Some(map) = color_map {
                    let spans = map.get_line_spans(line_num);
                    if spans.is_empty() {
                        false
                    } else {
                        // Try to render with colors - this requires ColorTerminal
                        // For now, we'll render plain text and colors can be added
                        // via a separate rendering path that requires ColorTerminal
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            
            if !rendered_with_colors {
                // Plain text rendering
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
            }
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;


