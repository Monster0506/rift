//! Command executor: applies editor commands to the buffer atomically and in sequence.
//! Mutates buffer/editor state only; is independent of key bindings and never inspects raw input or terminal state.

use crate::action::Motion;
use crate::buffer::TextBuffer;
use crate::command::Command;
use crate::document::Document;
use crate::error::RiftError;
use crate::wrap::DisplayMap;

fn calculate_current_column(buf: &TextBuffer, tab_width: usize) -> usize {
    use crate::buffer::api::BufferView;
    let cursor = buf.cursor();
    let line_idx = buf.line_index.get_line_at(cursor);
    let line_start = buf.line_index.get_start(line_idx).unwrap_or(0);

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

pub(crate) fn tab_fill_width(buf: &TextBuffer, tab_width: usize) -> usize {
    tab_width - (calculate_current_column(buf, tab_width) % tab_width)
}

pub fn compute_motion_range(
    motion: Motion,
    count: usize,
    doc: &mut Document,
    viewport_height: usize,
    last_search_query: Option<&str>,
) -> Option<crate::wrap::MotionRange> {
    use crate::wrap::{MotionRange, OperatorContext};
    const MAX_MOTION_REPEAT: usize = 1_000_000;

    if let Motion::TextObject(spec) = motion {
        #[cfg(feature = "treesitter")]
        {
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
            return crate::text_objects::resolve(spec, &doc.buffer, count, syntax_ctx.as_ref());
        }
        #[cfg(not(feature = "treesitter"))]
        {
            return crate::text_objects::resolve(spec, &doc.buffer, count, None);
        }
    }

    let is_linewise = matches!(
        motion,
        Motion::Up
            | Motion::Down
            | Motion::PageUp
            | Motion::PageDown
            | Motion::ToLine(_)
            | Motion::StartOfFile
            | Motion::EndOfFile
    );
    let is_inclusive = matches!(
        motion,
        Motion::FindCharForward(_) | Motion::FindCharBackward(_)
    );

    let anchor = doc.buffer.cursor();

    let count = count.min(MAX_MOTION_REPEAT);

    for i in 0..count {
        let iter_start = doc.buffer.cursor();

        motion.apply(
            &mut doc.buffer,
            None,
            OperatorContext::Operator,
            viewport_height,
            last_search_query,
        );

        if i + 1 == count && matches!(motion, Motion::NextWord | Motion::NextBigWord) {
            clamp_to_line_end(&mut doc.buffer, iter_start);
        }
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

fn clamp_to_line_end(buf: &mut crate::buffer::TextBuffer, from: usize) {
    let start_line = buf.line_index.get_line_at(from);
    if buf.line_index.get_line_at(buf.cursor()) <= start_line {
        return;
    }

    let Some(end) = buf.line_index.get_end(start_line, buf.len()) else {
        return;
    };
    if from < end {
        let _ = buf.set_cursor(end);
    }
}

pub(crate) fn range_to_offsets(
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
            let mut start = doc.buffer.line_index.get_start(first_line).unwrap_or(0);
            let len = doc.buffer.len();
            let end = if consume_trailing_newline {
                if last_line + 1 < doc.buffer.get_total_lines() {
                    doc.buffer
                        .line_index
                        .get_start(last_line + 1)
                        .unwrap_or(len)
                } else {
                    start = start.saturating_sub(1);
                    len
                }
            } else {
                doc.buffer.line_index.get_end(last_line, len).unwrap_or(len)
            };
            (start, end)
        }
        RangeKind::Charwise | RangeKind::Blockwise => {
            let end_offset = usize::from(range.inclusive);
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

/// # Errors
pub fn execute_command(
    cmd: Command,
    doc: &mut Document,
    expand_tabs: bool,
    tab_width: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
    display_map: Option<&mut DisplayMap>,
) -> Result<(), RiftError> {
    match cmd {
        Command::Move(motion, count) => {
            move_cmd(
                doc,
                display_map,
                motion,
                count,
                viewport_height,
                last_search_query,
            );
        }
        Command::Delete(motion, count) => {
            delete_motion_cmd(doc, motion, count, viewport_height, last_search_query);
        }
        Command::Change(motion, count) => {
            change_motion_cmd(doc, motion, count, viewport_height, last_search_query);
        }
        Command::DeleteForward => {
            doc.delete_forward();
        }
        Command::DeleteBackward => {
            doc.delete_backward();
        }
        Command::DeleteLine(count) => {
            delete_line_cmd(doc, count);
        }
        Command::ChangeLine(count) => {
            change_line_cmd(doc, count);
        }
        Command::InsertChar(ch) => {
            insert_char_cmd(doc, expand_tabs, tab_width, ch)?;
        }
        Command::ReplaceChar(ch, count) => {
            replace_char_cmd(doc, ch, count)?;
        }
        Command::OverwriteChar(ch) => {
            doc.overwrite_char(ch)?;
        }
        Command::ReplaceBackspace(restore) => {
            replace_backspace_cmd(doc, restore)?;
        }
        Command::DeleteSurround(ch, count) => {
            delete_surround_cmd(doc, ch, count);
        }
        Command::ChangeSurround(from, to, count) => {
            change_surround_cmd(doc, from, to, count);
        }
        Command::AddSurround(motion, motion_count, ch, delim_count) => {
            add_surround_cmd(
                doc,
                motion,
                motion_count,
                ch,
                delim_count,
                viewport_height,
                last_search_query,
            );
        }
        Command::Undo => {
            undo_cmd(doc);
        }
        Command::Redo => {
            redo_cmd(doc);
        }
        Command::EnterInsertMode
        | Command::EnterInsertModeAfter
        | Command::EnterInsertModeAtLineStart
        | Command::EnterInsertModeAtLineEnd
        | Command::OpenLineBelow
        | Command::OpenLineAbove
        | Command::EnterReplaceMode
        | Command::EnterCommandMode
        | Command::EnterSearchMode
        | Command::AppendToCommandLine(_)
        | Command::DeleteFromCommandLine
        | Command::ExecuteCommandLine
        | Command::ExecuteSearch
        | Command::NextMatch
        | Command::PreviousMatch
        | Command::Quit
        | Command::BufferNext
        | Command::BufferPrevious
        | Command::DotRepeat
        | Command::TabComplete
        | Command::TabCompletePrev
        | Command::Noop => {}
    }
    Ok(())
}

/// Cursor motion (`Move`).
fn move_cmd(
    doc: &mut Document,
    mut display_map: Option<&mut DisplayMap>,
    motion: Motion,
    count: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) {
    let buf = &mut doc.buffer;
    for _ in 0..count {
        motion.apply(
            buf,
            display_map.as_deref_mut(),
            crate::wrap::OperatorContext::Move,
            viewport_height,
            last_search_query,
        );
    }
}

/// Delete over a motion (`Delete`).
fn delete_motion_cmd(
    doc: &mut Document,
    motion: Motion,
    count: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) {
    let Some(range) = compute_motion_range(motion, count, doc, viewport_height, last_search_query)
    else {
        return;
    };
    let (delete_start, delete_end) = range_to_offsets(&range, doc, true);
    if delete_end > delete_start {
        doc.begin_transaction("Delete");
        let _ = doc.delete_range(delete_start, delete_end);
        doc.commit_transaction();
    }
}

/// Change over a motion (`Change`).
fn change_motion_cmd(
    doc: &mut Document,
    motion: Motion,
    count: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) {
    let Some(range) = compute_motion_range(motion, count, doc, viewport_height, last_search_query)
    else {
        return;
    };
    let (delete_start, delete_end) = range_to_offsets(&range, doc, false);
    if delete_end > delete_start {
        let _ = doc.delete_range(delete_start, delete_end);
    }
}

/// Delete `count` whole lines (`DeleteLine`).
fn delete_line_cmd(doc: &mut Document, count: usize) {
    doc.begin_transaction("DeleteLine");
    doc.buffer.move_to_line_start();
    let start = doc.buffer.cursor();
    let mut reached_last_line = false;
    for _ in 0..count.max(1) {
        if !doc.buffer.move_down() {
            reached_last_line = true;
            break;
        }
    }
    if reached_last_line {
        doc.buffer.move_to_line_end();
        let end = doc.buffer.cursor();
        if end > start {
            let _ = doc.delete_range(start, end);
        }
        if start > 0 {
            doc.delete_backward();
            doc.buffer.move_to_line_start();
        }
    } else {
        let end = doc.buffer.cursor();
        let _ = doc.delete_range(start, end);
    }
    doc.commit_transaction();
}

/// Change `count` lines up to their end (`ChangeLine`).
fn change_line_cmd(doc: &mut Document, count: usize) {
    doc.buffer.move_to_line_start();
    let start = doc.buffer.cursor();
    for _ in 0..count.max(1).saturating_sub(1) {
        doc.buffer.move_down();
    }
    doc.buffer.move_to_line_end();
    let end = doc.buffer.cursor();
    if end > start {
        let _ = doc.delete_range(start, end);
    }
}

/// Insert a character; `expand_tabs` turns a tab into spaces.
fn insert_char_cmd(
    doc: &mut Document,
    expand_tabs: bool,
    tab_width: usize,
    ch: char,
) -> Result<(), RiftError> {
    if ch == '\t' && expand_tabs {
        for _ in 0..tab_fill_width(&doc.buffer, tab_width) {
            doc.insert_char(' ')?;
        }
        return Ok(());
    }
    doc.insert_char(ch)
}

/// Replace `count` characters under the cursor (`ReplaceChar`).
fn replace_char_cmd(doc: &mut Document, ch: char, count: usize) -> Result<(), RiftError> {
    let pos = doc.buffer.cursor();
    let avail = doc.buffer.len().saturating_sub(pos);
    let replace_count = count.min(avail);
    if replace_count > 0 {
        doc.begin_transaction("Replace");
        doc.replace_repeat(pos, replace_count, ch)?;
        doc.commit_transaction();
    }
    Ok(())
}

fn replace_backspace_cmd(
    doc: &mut Document,
    restore: Option<crate::character::Character>,
) -> Result<(), RiftError> {
    let Some(original) = restore else {
        doc.delete_backward();
        return Ok(());
    };
    let Some(pos) = doc.buffer.cursor().checked_sub(1) else {
        return Ok(());
    };

    doc.replace_chars(pos, 1, &[original])?;
    doc.buffer.set_cursor(pos)
}

fn delete_surround_cmd(doc: &mut Document, ch: char, count: usize) {
    if let Some((open_range, close_range)) =
        crate::text_objects::resolve_surround_pair(ch, &doc.buffer, count)
    {
        doc.begin_transaction("DeleteSurround");
        let _ = doc.delete_range(close_range.start, close_range.end);
        let _ = doc.delete_range(open_range.start, open_range.end);
        doc.commit_transaction();
    }
}

fn change_surround_cmd(doc: &mut Document, from: char, to: char, count: usize) {
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

fn add_surround_cmd(
    doc: &mut Document,
    motion: Motion,
    motion_count: usize,
    ch: char,
    delim_count: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) {
    let Some((open, close)) = crate::text_objects::surround_strings(ch, delim_count) else {
        return;
    };
    let Some(range) = compute_motion_range(
        motion,
        motion_count,
        doc,
        viewport_height,
        last_search_query,
    ) else {
        return;
    };
    let (start, end) = range_to_offsets(&range, doc, false);
    doc.begin_transaction("AddSurround");
    let _ = doc.buffer.set_cursor(end);
    let _ = doc.insert_str(&close);
    let _ = doc.buffer.set_cursor(start);
    let _ = doc.insert_str(&open);
    doc.commit_transaction();
}

fn undo_cmd(doc: &mut Document) {
    crate::perf_span!("history_undo", crate::perf::PerfFields::default());
    doc.undo();
}

fn redo_cmd(doc: &mut Document) {
    crate::perf_span!("history_redo", crate::perf::PerfFields::default());
    doc.redo();
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
