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
use crate::wrap::DisplayMap;

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

/// Compute the byte range that a motion-operator pair would affect, without applying it.
///
/// Temporarily moves the cursor to simulate the motion, records the resulting
/// range, then restores the cursor. The document is otherwise not mutated.
///
/// Returns `None` if the motion produces an empty range (no movement).
pub fn compute_motion_range(
    motion: Motion,
    count: usize,
    doc: &mut Document,
    viewport_height: usize,
    last_search_query: Option<&str>,
    tab_width: usize,
) -> Option<crate::wrap::MotionRange> {
    use crate::wrap::{MotionRange, OperatorContext};

    let is_linewise = matches!(
        motion,
        Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown | Motion::ToLine(_)
    );

    let anchor = doc.buffer.cursor();

    for _ in 0..count {
        motion.apply(
            &mut doc.buffer,
            None,
            OperatorContext::Operator,
            tab_width,
            viewport_height,
            last_search_query,
        );
    }

    let new_cursor = doc.buffer.cursor();
    let _ = doc.buffer.set_cursor(anchor);

    if anchor == new_cursor {
        return None;
    }

    if is_linewise {
        Some(MotionRange::linewise(anchor, new_cursor))
    } else {
        Some(MotionRange::charwise(anchor, new_cursor))
    }
}

/// Execute a command on the editor buffer
pub fn execute_command(
    cmd: Command,
    doc: &mut Document,
    expand_tabs: bool,
    tab_width: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
    display_map: Option<&DisplayMap>,
) -> Result<(), RiftError> {
    match cmd {
        Command::Move(motion, count) => {
            let buf = &mut doc.buffer;
            for _ in 0..count {
                motion.apply(
                    buf,
                    display_map,
                    crate::wrap::OperatorContext::Move,
                    tab_width,
                    viewport_height,
                    last_search_query,
                );
            }
        }
        Command::Delete(motion, count) => {
            let Some(range) = compute_motion_range(
                motion,
                count,
                doc,
                viewport_height,
                last_search_query,
                tab_width,
            ) else {
                return Ok(());
            };

            match range.kind {
                crate::wrap::RangeKind::Linewise => {
                    let start_line = doc.buffer.line_index.get_line_at(range.anchor);
                    let end_line = doc.buffer.line_index.get_line_at(range.new_cursor);
                    let (first_line, last_line) = if start_line <= end_line {
                        (start_line, end_line)
                    } else {
                        (end_line, start_line)
                    };

                    let delete_start = doc.buffer.line_index.get_start(first_line).unwrap_or(0);
                    let delete_end = if last_line + 1 < doc.buffer.get_total_lines() {
                        doc.buffer
                            .line_index
                            .get_start(last_line + 1)
                            .unwrap_or(doc.buffer.len())
                    } else {
                        doc.buffer.len()
                    };

                    if delete_end > delete_start {
                        doc.begin_transaction("Delete");
                        let _ = doc.delete_range(delete_start, delete_end);
                        doc.commit_transaction();
                    }
                }
                crate::wrap::RangeKind::Charwise => {
                    if range.new_cursor > range.anchor {
                        let (del_start, del_end) = (range.anchor, range.new_cursor);
                        doc.begin_transaction("Delete");
                        let _ = doc.delete_range(del_start, del_end);
                        doc.commit_transaction();
                    } else {
                        let (del_start, del_end) = (range.new_cursor, range.anchor);
                        doc.begin_transaction("Delete");
                        let _ = doc.delete_range(del_start, del_end);
                        doc.commit_transaction();
                    }
                }
            }
        }
        Command::Change(motion, count) => {
            let Some(range) = compute_motion_range(
                motion,
                count,
                doc,
                viewport_height,
                last_search_query,
                tab_width,
            ) else {
                return Ok(());
            };

            match range.kind {
                crate::wrap::RangeKind::Linewise => {
                    let start_line = doc.buffer.line_index.get_line_at(range.anchor);
                    let end_line = doc.buffer.line_index.get_line_at(range.new_cursor);
                    let (first_line, last_line) = if start_line <= end_line {
                        (start_line, end_line)
                    } else {
                        (end_line, start_line)
                    };

                    let delete_start = doc.buffer.line_index.get_start(first_line).unwrap_or(0);
                    let delete_end = if last_line + 1 < doc.buffer.get_total_lines() {
                        doc.buffer
                            .line_index
                            .get_start(last_line + 1)
                            .unwrap_or(doc.buffer.len())
                    } else {
                        doc.buffer.len()
                    };

                    if delete_end > delete_start {
                        let _ = doc.delete_range(delete_start, delete_end);
                    }
                }
                crate::wrap::RangeKind::Charwise => {
                    if range.new_cursor > range.anchor {
                        let _ = doc.delete_range(range.anchor, range.new_cursor);
                    } else {
                        let _ = doc.delete_range(range.new_cursor, range.anchor);
                    }
                }
            }
        }
        Command::ChangeLine => {
            doc.buffer.move_to_line_start();
            let start = doc.buffer.cursor();
            doc.buffer.move_to_line_end();
            let end = doc.buffer.cursor();
            if end > start {
                let _ = doc.delete_range(start, end);
            }
        }
        Command::DeleteForward => {
            doc.delete_forward();
        }
        Command::DeleteBackward => {
            doc.delete_backward();
        }
        Command::DeleteLine => {
            doc.begin_transaction("DeleteLine");
            doc.buffer.move_to_line_start();
            let start = doc.buffer.cursor();
            if doc.buffer.move_down() {
                let end = doc.buffer.cursor();
                let _ = doc.delete_range(start, end);
            } else {
                // Last line: delete content then preceding newline
                doc.buffer.move_to_line_end();
                let end = doc.buffer.cursor();
                if end > start {
                    let _ = doc.delete_range(start, end);
                }
                if start > 0 {
                    doc.delete_backward();
                    doc.buffer.move_to_line_start();
                }
            }
            doc.commit_transaction();
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
        Command::EnterInsertMode
        | Command::EnterInsertModeAfter
        | Command::EnterInsertModeAtLineStart
        | Command::EnterInsertModeAtLineEnd
        | Command::OpenLineBelow
        | Command::OpenLineAbove => {
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
        Command::DotRepeat => {
            // Handled at editor level
        }
        Command::TabComplete | Command::TabCompletePrev => {
            // Handled at editor level
        }
        Command::Noop => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
