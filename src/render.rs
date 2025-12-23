//! Rendering system
//! Handles drawing the editor UI to the terminal

use crate::buffer::GapBuffer;
use crate::mode::Mode;
use crate::key::Key;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use crate::state::State;

/// Render the editor interface
pub fn render<T: TerminalBackend>(
    term: &mut T,
    buf: &GapBuffer,
    viewport: &mut Viewport,
    current_mode: Mode,
    pending_key: Option<Key>,
    state: &State,
) -> Result<(), String> {
    // Clear screen
    term.clear_screen()?;

    // Update viewport based on cursor position
    let cursor_line = buf.get_line();
    let total_lines = buf.get_total_lines();
    viewport.update(cursor_line, total_lines);

    // Render content
    render_content(term, buf, viewport)?;

    // Render status bar
    render_status_bar(term, viewport, current_mode, pending_key, state)?;

    // Position cursor
    let cursor_line_in_viewport = if cursor_line >= viewport.top_line() 
        && cursor_line < viewport.top_line() + viewport.visible_rows() {
        cursor_line - viewport.top_line()
    } else {
        0
    };
    
    let cursor_col = calculate_cursor_column(buf, cursor_line);
    let display_col = cursor_col.min(viewport.visible_cols().saturating_sub(1));
    
    term.move_cursor(cursor_line_in_viewport as u16, display_col as u16)?;
    
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
        } else {
            current_line.push(byte);
        }
    }
    
    // Process after_gap
    for &byte in after_gap {
        if byte == b'\n' {
            lines.push(current_line);
            current_line = Vec::new();
        } else {
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
            
            term.write(&padded_line)?;
            term.write(b"\r\n")?;
        } else {
            // Empty line
            term.write(&vec![b' '; visible_cols])?;
            term.write(b"\r\n")?;
        }
    }
    
    Ok(())
}

fn render_status_bar<T: TerminalBackend>(
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
    
    // Mode indicator
    let mode_str = match current_mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
    };
    
    term.write(mode_str.as_bytes())?;
    
    // Pending key indicator
    if let Some(key) = pending_key {
        term.write(b" [")?;
        let key_str = format_key(key);
        term.write(key_str.as_bytes())?;
        term.write(b"]")?;
    }
    
    // Debug information (if debug mode is enabled)
    if state.debug_mode {
        let mut debug_parts = Vec::new();
        
        // Last keypress
        if let Some(key) = state.last_keypress {
            debug_parts.push(format!("Last: {}", format_key(key)));
        }
        
        // Cursor position
        debug_parts.push(format!("Pos: {}:{}", state.cursor_pos.0 + 1, state.cursor_pos.1 + 1));
        
        // Buffer stats
        debug_parts.push(format!("Lines: {}", state.total_lines));
        debug_parts.push(format!("Size: {}B", state.buffer_size));
        
        // Join debug parts
        let debug_str = debug_parts.join(" | ");
        
        // Calculate available space
        let mode_len = mode_str.len();
        let pending_len = if pending_key.is_some() { 
            format_key(pending_key.unwrap()).len() + 3 // "[key]"
        } else { 
            0 
        };
        let used_cols = mode_len + pending_len;
        let available_cols = viewport.visible_cols().saturating_sub(used_cols);
        
        // Truncate debug string if needed
        let debug_display = if debug_str.len() <= available_cols {
            debug_str
        } else {
            format!("{}...", &debug_str[..available_cols.saturating_sub(3)])
        };
        
        // Add spacing before debug info
        let spacing = available_cols.saturating_sub(debug_display.len());
        for _ in 0..spacing {
            term.write(b" ")?;
        }
        
        term.write(debug_display.as_bytes())?;
    }
    
    // Fill rest of line with spaces
    let mode_len = mode_str.len();
    let pending_len = if pending_key.is_some() { 
        format_key(pending_key.unwrap()).len() + 3 // "[key]"
    } else { 
        0 
    };
    let debug_len = if state.debug_mode {
        // Calculate actual debug length
        let mut debug_parts = Vec::new();
        if let Some(key) = state.last_keypress {
            debug_parts.push(format!("Last: {}", format_key(key)));
        }
        debug_parts.push(format!("Pos: {}:{}", state.cursor_pos.0 + 1, state.cursor_pos.1 + 1));
        debug_parts.push(format!("Lines: {}", state.total_lines));
        debug_parts.push(format!("Size: {}B", state.buffer_size));
        let debug_str = debug_parts.join(" | ");
        let available_cols = viewport.visible_cols().saturating_sub(mode_len + pending_len);
        debug_str.len().min(available_cols)
    } else {
        0
    };
    let used_cols = mode_len + pending_len + debug_len;
    let remaining_cols = viewport.visible_cols().saturating_sub(used_cols);
    
    for _ in 0..remaining_cols {
        term.write(b" ")?;
    }
    
    // Reset colors
    term.write(b"\x1b[0m")?;
    
    Ok(())
}

fn format_key(key: Key) -> String {
    match key {
        Key::Char(ch) => {
            if ch >= 32 && ch < 127 {
                format!("{}", ch as char)
            } else {
                format!("\\x{:02x}", ch)
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

fn calculate_cursor_column(buf: &GapBuffer, line: usize) -> usize {
    let before_gap = buf.get_before_gap();
    let mut current_line = 0;
    let mut col = 0;
    
    for &byte in before_gap {
        if byte == b'\n' {
            if current_line == line {
                return col;
            }
            current_line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    
    // If we're at the gap position on the target line
    if current_line == line {
        return col;
    }
    
    // Check after_gap
    let after_gap = buf.get_after_gap();
    for &byte in after_gap {
        if byte == b'\n' {
            if current_line == line {
                return col;
            }
            current_line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    
    col
}

