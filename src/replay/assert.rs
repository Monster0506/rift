//! Evaluates a parsed [`Assertion`] against live editor state, returning an
//! error describing the mismatch when it fails.

use super::backend::ReplayBackend;
use super::ops::Assertion;
use crate::buffer::api::BufferView;
use crate::editor::Editor;
use crate::error::{ErrorType, RiftError};
use std::io::Write;

pub(crate) fn check<W: Write>(
    ed: &mut Editor<ReplayBackend<W>>,
    assertion: &Assertion,
) -> Result<(), RiftError> {
    match assertion {
        Assertion::Cursor { row, col } => {
            let (actual_row, actual_col) = cursor_row_col(ed);
            if (actual_row, actual_col) != (*row, *col) {
                return Err(fail(format!(
                    "cursor: expected {row}:{col}, got {actual_row}:{actual_col}"
                )));
            }
        }
        Assertion::Mode(expected) => {
            let actual = ed.mode().as_str();
            if actual != expected.as_str() {
                return Err(fail(format!("mode: expected {expected}, got {actual}")));
            }
        }
        Assertion::Line { row, text } => {
            let actual = line_text(ed, *row);
            if actual.as_deref() != Some(text.as_str()) {
                return Err(fail(format!(
                    "line {row}: expected {text:?}, got {:?}",
                    actual.unwrap_or_default()
                )));
            }
        }
        Assertion::Buffer(expected) => {
            let actual = buffer_text(ed);
            if &actual != expected {
                return Err(fail(format!(
                    "buffer mismatch:\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
                )));
            }
        }
    }
    Ok(())
}

/// (row, col) of the cursor, found by binary search over line-start offsets.
fn cursor_row_col<W: Write>(ed: &mut Editor<ReplayBackend<W>>) -> (usize, usize) {
    let buf = &ed.active_document().buffer;
    let pos = buf.cursor();
    let mut lo = 0usize;
    let mut hi = buf.line_count().saturating_sub(1);
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if buf.line_start(mid) <= pos {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    (lo, pos - buf.line_start(lo))
}

fn line_text<W: Write>(ed: &mut Editor<ReplayBackend<W>>, row: usize) -> Option<String> {
    let buf = &ed.active_document().buffer;
    if row >= buf.line_count() {
        return None;
    }
    let start = buf.line_start(row);
    let end = if row + 1 < buf.line_count() {
        buf.line_start(row + 1)
    } else {
        buf.len()
    };
    Some(strip_trailing_newline(
        buf.chars(start..end).map(|c| c.to_char_lossy()).collect(),
    ))
}

fn buffer_text<W: Write>(ed: &mut Editor<ReplayBackend<W>>) -> String {
    let buf = &ed.active_document().buffer;
    strip_trailing_newline(buf.chars(0..buf.len()).map(|c| c.to_char_lossy()).collect())
}

fn strip_trailing_newline(mut text: String) -> String {
    if text.ends_with('\n') {
        text.pop();
    }
    text
}

fn fail(message: String) -> RiftError {
    RiftError::new(ErrorType::Execution, "REPLAY_ASSERT", message)
}
