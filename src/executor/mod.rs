//! Command executor
//! Executes editor commands on the buffer
//!
//! ## executor/ Invariants
//!
//! - The executor mutates buffer and editor state only.
//! - Each command application is atomic.
//! - Mode changes are not handled here unless explicitly documented.
//! - Executor behavior is independent of key bindings.
//! - Executor never inspects raw input or terminal state.
//! - Commands are applied strictly in sequence.

use crate::action::Motion;
use crate::buffer::TextBuffer;
use crate::command::Command;
use crate::error::RiftError;

/// Calculate the current visual column position on the current line
/// Accounts for tab width when calculating visual position
fn calculate_current_column(buf: &TextBuffer, tab_width: usize) -> usize {
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
                        col = ((col / tab_width) + 1) * tab_width;
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
                col = ((col / tab_width) + 1) * tab_width;
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
            col = ((col / tab_width) + 1) * tab_width;
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
                        col = ((col / tab_width) + 1) * tab_width;
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
        for b in after_gap {
            if b == b'\t' {
                col = ((col / tab_width) + 1) * tab_width;
            } else {
                col += 1;
            }
        }
        return col;
    }

    0
}

/// Execute a command on the editor buffer
pub fn execute_command(
    cmd: Command,
    buf: &mut TextBuffer,
    expand_tabs: bool,
    tab_width: usize,
    viewport_height: usize,
) -> Result<(), RiftError> {
    match cmd {
        Command::Move(motion, count) => {
            for _ in 0..count {
                match motion {
                    Motion::Left => {
                        buf.move_left();
                    }
                    Motion::Right => {
                        buf.move_right();
                    }
                    Motion::Up => {
                        buf.move_up();
                    }
                    Motion::Down => {
                        buf.move_down();
                    }
                    Motion::StartOfLine => {
                        buf.move_to_line_start();
                    }
                    Motion::EndOfLine => {
                        buf.move_to_line_end();
                    }
                    Motion::StartOfFile => buf.move_to_start(),
                    Motion::EndOfFile => buf.move_to_end(),
                    Motion::PageUp => {
                        for _ in 0..viewport_height {
                            buf.move_up();
                        }
                    }
                    Motion::PageDown => {
                        for _ in 0..viewport_height {
                            buf.move_down();
                        }
                    }
                    Motion::NextWord => {
                        buf.move_word_right();
                    }
                    Motion::PreviousWord => {
                        buf.move_word_left();
                    }
                    Motion::NextParagraph => {
                        buf.move_paragraph_forward();
                    }
                    Motion::PreviousParagraph => {
                        buf.move_paragraph_backward();
                    }
                    Motion::NextSentence => {
                        buf.move_sentence_forward();
                    }
                    Motion::PreviousSentence => {
                        buf.move_sentence_backward();
                    }
                }
            }
        }
        Command::Delete(motion, count) => {
            let start = buf.cursor();
            // Perform motion to find end point
            for _ in 0..count {
                match motion {
                    Motion::Left => {
                        buf.move_left();
                    }
                    Motion::Right => {
                        buf.move_right();
                    }
                    Motion::Up => {
                        buf.move_up();
                    }
                    Motion::Down => {
                        buf.move_down();
                    }
                    Motion::StartOfLine => {
                        buf.move_to_line_start();
                    }
                    Motion::EndOfLine => {
                        buf.move_to_line_end();
                    }
                    Motion::StartOfFile => buf.move_to_start(),
                    Motion::EndOfFile => buf.move_to_end(),
                    Motion::PageUp => {
                        for _ in 0..viewport_height {
                            buf.move_up();
                        }
                    }
                    Motion::PageDown => {
                        for _ in 0..viewport_height {
                            buf.move_down();
                        }
                    }
                    Motion::NextWord => {
                        buf.move_word_right();
                    }
                    Motion::PreviousWord => {
                        buf.move_word_left();
                    }
                    Motion::NextParagraph => {
                        buf.move_paragraph_forward();
                    }
                    Motion::PreviousParagraph => {
                        buf.move_paragraph_backward();
                    }
                    Motion::NextSentence => {
                        buf.move_sentence_forward();
                    }
                    Motion::PreviousSentence => {
                        buf.move_sentence_backward();
                    }
                }
            }
            let end = buf.cursor();

            if end > start {
                // Forward deletion (e.g. dw)
                // Cursor is at end. We want to delete [start, end).
                // Move back to end, then delete backward until start.
                let len = end - start;
                for _ in 0..len {
                    buf.delete_backward();
                }
            } else if end < start {
                // Backward deletion (e.g. db)
                // Cursor is at end. We want to delete [end, start).
                // We are at end. delete_forward deletes (end).
                let len = start - end;
                for _ in 0..len {
                    buf.delete_forward();
                }
            }
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
        Command::InsertChar(ch) => {
            if ch == '\t' && expand_tabs {
                // Calculate current column position
                let current_col = calculate_current_column(buf, tab_width);
                // Calculate spaces needed to reach next tab stop
                let spaces_needed = tab_width - (current_col % tab_width);
                // Insert that many spaces, stop on error
                for _ in 0..spaces_needed {
                    buf.insert(b' ')?;
                }
            } else {
                buf.insert_char(ch)?;
            }
        }
        Command::EnterInsertMode | Command::EnterInsertModeAfter => {
            // Mode change handled by editor
        }
        Command::EnterCommandMode => {
            // Mode change handled by editor
        }
        Command::AppendToCommandLine(_) => {
            // Command line editing handled by editor
        }
        Command::DeleteFromCommandLine => {
            // Command line editing handled by editor
        }
        Command::ExecuteCommandLine => {
            // Command execution handled by editor
        }
        Command::Quit => {
            // Quit handled by editor
        }
        Command::BufferNext | Command::BufferPrevious => {
            // Buffer navigation handled by editor
        }
        Command::Noop => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
