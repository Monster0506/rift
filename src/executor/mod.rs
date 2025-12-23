//! Command executor
//! Executes editor commands on the buffer
//! Invariants:
//! - Executor mutates buffer only
//! - Mode changes are handled by editor
//! - All commands are editor-level, not key-level

use crate::command::Command;
use crate::buffer::GapBuffer;

// Tab width (hardcoded for now, should be a setting later)
const TAB_WIDTH: usize = 8;

/// Calculate the current visual column position on the current line
/// Accounts for tab width when calculating visual position
fn calculate_current_column(buf: &GapBuffer) -> usize {
    let line = buf.get_line();
    let before_gap = buf.get_before_gap();
    let mut current_line = 0;
    let mut line_start = 0;
    let mut col = 0;
    
    // Find the start of the current line
    for (i, &byte) in before_gap.iter().enumerate() {
        if byte == b'\n' {
            if current_line == line {
                // Found the line, calculate visual column up to gap position
                let line_bytes = &before_gap[line_start..i];
                for &b in line_bytes {
                    if b == b'\t' {
                        col = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
                    } else {
                        col += 1;
                    }
                }
                return col;
            }
            current_line += 1;
            line_start = i + 1;
            col = 0;
        }
    }
    
    // If we're at the gap position on the current line
    if current_line == line {
        let line_bytes = &before_gap[line_start..];
        for &b in line_bytes {
            if b == b'\t' {
                col = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
            } else {
                col += 1;
            }
        }
        return col;
    }
    
    // Check after_gap - need to include before_gap bytes from line_start
    let after_gap = buf.get_after_gap();
    // First, calculate column for before_gap portion of this line
    let before_line_bytes = &before_gap[line_start..];
    for &b in before_line_bytes {
        if b == b'\t' {
            col = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
        } else {
            col += 1;
        }
    }
    
    // Now process after_gap bytes
    for (i, &byte) in after_gap.iter().enumerate() {
        if byte == b'\n' {
            if current_line == line {
                // Found the line in after_gap, include bytes up to this newline
                let after_line_bytes = &after_gap[..i];
                for &b in after_line_bytes {
                    if b == b'\t' {
                        col = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
                    } else {
                        col += 1;
                    }
                }
                return col;
            }
            current_line += 1;
            col = 0;
        }
    }
    
    // If we're at the end of the current line (after gap, no newline found)
    if current_line == line {
        // Include all remaining after_gap bytes
        for &b in after_gap {
            if b == b'\t' {
                col = ((col / TAB_WIDTH) + 1) * TAB_WIDTH;
            } else {
                col += 1;
            }
        }
        return col;
    }
    
    0
}

/// Execute a command on the editor buffer
pub fn execute_command(cmd: Command, buf: &mut GapBuffer, expand_tabs: bool) {
    match cmd {
        Command::MoveLeft => {
            buf.move_left();
        }
        Command::MoveRight => {
            buf.move_right();
        }
        Command::MoveUp => {
            buf.move_up();
        }
        Command::MoveDown => {
            buf.move_down();
        }
        Command::MoveToLineStart => {
            buf.move_to_line_start();
        }
        Command::MoveToLineEnd => {
            buf.move_to_line_end();
        }
        Command::MoveToBufferStart => {
            // Move to start of buffer
            while buf.move_left() {}
        }
        Command::MoveToBufferEnd => {
            // Move to end of buffer
            while buf.move_right() {}
        }
        Command::DeleteForward => {
            buf.delete_forward();
        }
        Command::DeleteBackward => {
            buf.delete_backward();
        }
        Command::DeleteLine => {
            // TODO: Implement delete_line
        }
        Command::InsertByte(b) => {
            if b == b'\t' && expand_tabs {
                // Calculate current column position
                let current_col = calculate_current_column(buf);
                // Calculate spaces needed to reach next tab stop
                let spaces_needed = TAB_WIDTH - (current_col % TAB_WIDTH);
                // Insert that many spaces
                for _ in 0..spaces_needed {
                    let _ = buf.insert(b' ');
                }
            } else {
                let _ = buf.insert(b);
            }
        }
        Command::EnterInsertMode | Command::EnterInsertModeAfter => {
            // Mode change handled by editor
        }
        Command::Quit => {
            // Quit handled by editor
        }
        Command::Noop => {}
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

