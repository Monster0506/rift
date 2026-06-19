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

    // Text objects resolve directly without cursor simulation.
    if let Motion::TextObject(spec) = motion {
        let needs_tree = crate::text_objects::requires_treesitter(spec.kind);
        let ts_bytes: Option<Vec<u8>> =
            if needs_tree && doc.syntax.as_ref().and_then(|s| s.tree.as_ref()).is_some() {
                Some(doc.buffer.to_logical_bytes())
            } else {
                None
            };
        let syntax_ctx = ts_bytes.as_ref().and_then(|bytes| {
            doc.syntax
                .as_ref()
                .and_then(|s| s.tree.as_ref())
                .map(|tree| crate::text_objects::SyntaxContext {
                    tree,
                    source: bytes.as_slice(),
                })
        });
        return crate::text_objects::resolve(spec, &doc.buffer, count, syntax_ctx);
    }

    let is_linewise = matches!(
        motion,
        Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown | Motion::ToLine(_)
    );
    let is_inclusive = matches!(
        motion,
        Motion::FindCharForward(_) | Motion::FindCharBackward(_)
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
    } else if is_inclusive {
        Some(MotionRange::charwise_inclusive(anchor, new_cursor))
    } else {
        Some(MotionRange::charwise(anchor, new_cursor))
    }
}

/// Converts a resolved `MotionRange` into a half-open `(start, end)` char
/// offset pair. Shared by `Delete`, `Change`, and `AddSurround`.
///
/// `consume_trailing_newline` controls the Linewise end boundary: deletion
/// wants to remove the line terminator too (so the line itself disappears),
/// while surround-insertion wants to stop before it (so the delimiter lands
/// inside the line, not on the line that follows).
fn range_to_offsets(
    range: &crate::wrap::MotionRange,
    doc: &Document,
    consume_trailing_newline: bool,
) -> (usize, usize) {
    use crate::wrap::RangeKind;
    match range.kind {
        RangeKind::Linewise => {
            let start_line = doc.buffer.line_index.get_line_at(range.anchor);
            let end_line = doc.buffer.line_index.get_line_at(range.new_cursor);
            let (first_line, last_line) = if start_line <= end_line {
                (start_line, end_line)
            } else {
                (end_line, start_line)
            };
            let start = doc.buffer.line_index.get_start(first_line).unwrap_or(0);
            let len = doc.buffer.len();
            let end = if consume_trailing_newline {
                if last_line + 1 < doc.buffer.get_total_lines() {
                    doc.buffer
                        .line_index
                        .get_start(last_line + 1)
                        .unwrap_or(len)
                } else {
                    len
                }
            } else {
                doc.buffer.line_index.get_end(last_line, len).unwrap_or(len)
            };
            (start, end)
        }
        RangeKind::Charwise | RangeKind::Blockwise => {
            let end_offset = if range.inclusive { 1 } else { 0 };
            if range.new_cursor > range.anchor {
                (
                    range.anchor,
                    (range.new_cursor + end_offset).min(doc.buffer.len()),
                )
            } else {
                (
                    range.new_cursor,
                    (range.anchor + end_offset).min(doc.buffer.len()),
                )
            }
        }
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
            let (delete_start, delete_end) = range_to_offsets(&range, doc, true);
            if delete_end > delete_start {
                doc.begin_transaction("Delete");
                let _ = doc.delete_range(delete_start, delete_end);
                doc.commit_transaction();
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
            let (delete_start, delete_end) = range_to_offsets(&range, doc, true);
            if delete_end > delete_start {
                let _ = doc.delete_range(delete_start, delete_end);
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
        Command::DeleteSurround(ch, count) => {
            if let Some((open_range, close_range)) =
                crate::text_objects::resolve_surround_pair(ch, &doc.buffer, count)
            {
                doc.begin_transaction("DeleteSurround");
                let _ = doc.delete_range(close_range.start, close_range.end);
                let _ = doc.delete_range(open_range.start, open_range.end);
                doc.commit_transaction();
            }
        }
        Command::ChangeSurround(from, to, count) => {
            if let (Some((open_range, close_range)), Some((new_open, new_close))) = (
                crate::text_objects::resolve_surround_pair(from, &doc.buffer, count),
                crate::text_objects::surround_strings(to, count),
            ) {
                doc.begin_transaction("ChangeSurround");
                let _ = doc.delete_range(close_range.start, close_range.end);
                let _ = doc.buffer.set_cursor(close_range.start);
                let _ = doc.insert_str(&new_close);
                let _ = doc.delete_range(open_range.start, open_range.end);
                let _ = doc.buffer.set_cursor(open_range.start);
                let _ = doc.insert_str(&new_open);
                doc.commit_transaction();
            }
        }
        Command::AddSurround(motion, motion_count, ch, delim_count) => {
            let Some((open, close)) = crate::text_objects::surround_strings(ch, delim_count) else {
                return Ok(());
            };
            let Some(range) = compute_motion_range(
                motion,
                motion_count,
                doc,
                viewport_height,
                last_search_query,
                tab_width,
            ) else {
                return Ok(());
            };
            let (start, end) = range_to_offsets(&range, doc, false);
            doc.begin_transaction("AddSurround");
            let _ = doc.buffer.set_cursor(end);
            let _ = doc.insert_str(&close);
            let _ = doc.buffer.set_cursor(start);
            let _ = doc.insert_str(&open);
            doc.commit_transaction();
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
        Command::ReplaceChar(ch, count) => {
            let pos = doc.buffer.cursor();
            let avail = doc.buffer.len().saturating_sub(pos);
            let replace_count = count.min(avail);
            if replace_count > 0 {
                doc.begin_transaction("Replace");
                doc.replace_repeat(pos, replace_count, ch)?;
                doc.commit_transaction();
            }
        }
        Command::EnterReplaceMode => {
            // Mode change handled by editor
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
