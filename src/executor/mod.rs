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
use crate::document::Document;
use crate::error::RiftError;
use crate::search::{find_next, SearchDirection};

/// Calculate the current visual column position on the current line
/// Accounts for tab width when calculating visual position
fn calculate_current_column(buf: &TextBuffer, tab_width: usize) -> usize {
    use crate::buffer::api::BufferView;
    let cursor = buf.cursor();
    let line_idx = buf.line_index.get_line_at(cursor);
    let line_start = buf.line_index.get_start(line_idx).unwrap_or(0);

    // Iterate chars from line start to cursor
    let mut col = 0;
    for ch in BufferView::chars(buf, line_start..cursor) {
        if ch == crate::character::Character::Tab {
            col = ((col / tab_width) + 1) * tab_width;
        } else {
            col += ch.render_width(col, tab_width);
        }
    }
    col
}

/// Execute a command on the editor buffer
pub fn execute_command(
    cmd: Command,
    doc: &mut Document,
    expand_tabs: bool,
    tab_width: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) -> Result<(), RiftError> {
    match cmd {
        Command::Move(motion, count) => {
            let buf = &mut doc.buffer;
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
                    Motion::NextBigWord => {
                        // Big word support removed - use regular word movement
                        buf.move_word_right();
                    }
                    Motion::PreviousBigWord => {
                        // Big word support removed - use regular word movement
                        buf.move_word_left();
                    }
                    Motion::NextMatch => {
                        if let Some(query) = last_search_query {
                            let start = buf.cursor().saturating_add(1);
                            if let Ok((Some(m), _stats)) =
                                find_next(buf, start, query, SearchDirection::Forward)
                            {
                                buf.set_cursor(m.range.start)?;
                            }
                        }
                    }
                    Motion::PreviousMatch => {
                        if let Some(query) = last_search_query {
                            if let Ok((Some(m), _stats)) =
                                find_next(buf, buf.cursor(), query, SearchDirection::Backward)
                            {
                                buf.set_cursor(m.range.start)?;
                            }
                        }
                    }
                }
            }
        }
        Command::Delete(motion, count) => {
            // Note: We access buffer for navigation to calculate range
            let start = doc.buffer.cursor();
            // Perform motion to find end point
            for _ in 0..count {
                match motion {
                    Motion::Left => {
                        doc.buffer.move_left();
                    }
                    Motion::Right => {
                        doc.buffer.move_right();
                    }
                    Motion::Up => {
                        doc.buffer.move_up();
                    }
                    Motion::Down => {
                        doc.buffer.move_down();
                    }
                    Motion::StartOfLine => {
                        doc.buffer.move_to_line_start();
                    }
                    Motion::EndOfLine => {
                        doc.buffer.move_to_line_end();
                    }
                    Motion::StartOfFile => doc.buffer.move_to_start(),
                    Motion::EndOfFile => doc.buffer.move_to_end(),
                    Motion::PageUp => {
                        for _ in 0..viewport_height {
                            doc.buffer.move_up();
                        }
                    }
                    Motion::PageDown => {
                        for _ in 0..viewport_height {
                            doc.buffer.move_down();
                        }
                    }
                    Motion::NextWord => {
                        doc.buffer.move_word_right();
                    }
                    Motion::PreviousWord => {
                        doc.buffer.move_word_left();
                    }
                    Motion::NextParagraph => {
                        doc.buffer.move_paragraph_forward();
                    }
                    Motion::PreviousParagraph => {
                        doc.buffer.move_paragraph_backward();
                    }
                    Motion::NextSentence => {
                        doc.buffer.move_sentence_forward();
                    }
                    Motion::PreviousSentence => {
                        doc.buffer.move_sentence_backward();
                    }
                    Motion::NextBigWord => {
                        // Big word support removed - use regular word movement
                        doc.buffer.move_word_right();
                    }
                    Motion::PreviousBigWord => {
                        // Big word support removed - use regular word movement
                        doc.buffer.move_word_left();
                    }
                    Motion::NextMatch => {
                        let buf = &mut doc.buffer;
                        if let Some(query) = last_search_query {
                            let start = buf.cursor().saturating_add(1);
                            if let Ok((Some(m), _stats)) =
                                find_next(buf, start, query, SearchDirection::Forward)
                            {
                                buf.set_cursor(m.range.start)?;
                            }
                        }
                    }
                    Motion::PreviousMatch => {
                        let buf = &mut doc.buffer;
                        if let Some(query) = last_search_query {
                            if let Ok((Some(m), _stats)) =
                                find_next(buf, buf.cursor(), query, SearchDirection::Backward)
                            {
                                buf.set_cursor(m.range.start)?;
                            }
                        }
                    }
                }
            }
            let end = doc.buffer.cursor();

            if end > start {
                // Forward deletion (e.g. dw)
                let len = end - start;
                for _ in 0..len {
                    // Use Document's delete_backward to track edits
                    doc.delete_backward();
                }
            } else if end < start {
                // Backward deletion (e.g. db)
                let len = start - end;
                for _ in 0..len {
                    doc.delete_forward();
                }
            }
        }
        Command::DeleteForward => {
            doc.delete_forward();
        }
        Command::DeleteBackward => {
            doc.delete_backward();
        }
        Command::DeleteLine => {
            doc.buffer.move_to_line_start();
            let start = doc.buffer.cursor();
            if doc.buffer.move_down() {
                let end = doc.buffer.cursor();
                doc.buffer.set_cursor(start)?;
                let len = end - start;
                for _ in 0..len {
                    doc.delete_forward();
                }
            } else {
                // Last line
                doc.buffer.move_to_line_end();
                let end = doc.buffer.cursor();
                // Delete content
                if end > start {
                    let len = end - start;
                    for _ in 0..len {
                        doc.delete_backward();
                    }
                }
                // Delete preceding newline if exists
                if start > 0 {
                    doc.delete_backward();
                    doc.buffer.move_to_line_start();
                }
            }
        }
        Command::InsertChar(ch) => {
            if ch == '\t' && expand_tabs {
                // Calculate current column position on the buffer (read-only)
                let current_col = calculate_current_column(&doc.buffer, tab_width);
                // Calculate spaces needed to reach next tab stop
                let spaces_needed = tab_width - (current_col % tab_width);
                // Insert that many spaces, stop on error
                for _ in 0..spaces_needed {
                    doc.insert_char(' ')?;
                }
            } else {
                doc.insert_char(ch)?;
            }
        }
        Command::EnterInsertMode | Command::EnterInsertModeAfter => {
            // Mode change handled by editor
        }
        Command::EnterCommandMode => {
            // Mode change handled by editor
        }
        Command::EnterSearchMode => {
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
        Command::ExecuteSearch => {
            // Search execution handled by editor
        }
        Command::NextMatch | Command::PreviousMatch => {
            // Search navigation handled by editor
        }
        Command::Quit => {
            // Quit handled by editor
        }
        Command::BufferNext | Command::BufferPrevious => {
            // Buffer navigation handled by editor
        }
        Command::Undo => {
            doc.undo();
        }
        Command::Redo => {
            doc.redo();
        }
        Command::Noop => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
