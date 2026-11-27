//! Command executor: applies editor commands to the buffer atomically and in sequence.
//! Mutates buffer/editor state only; is independent of key bindings and never inspects raw input or terminal state.

use crate::action::Motion;
use crate::buffer::TextBuffer;
use crate::command::Command;
use crate::document::ops::{DocumentWrite, Edit, EditSource};
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

pub(crate) fn range_to_offsets(
    range: &crate::wrap::MotionRange,
    doc: &Document,
    consume_trailing_newline: bool,
) -> (usize, usize) {
    use crate::wrap::RangeKind;
    match range.kind {
        RangeKind::Linewise => {
            let start_line = doc.buffer().line_index.get_line_at(range.anchor);
            let end_line = doc.buffer().line_index.get_line_at(range.new_cursor);
            let (first_line, last_line) = if start_line <= end_line {
                (start_line, end_line)
            } else {
                (end_line, start_line)
            };
            let mut start = doc.buffer().line_index.get_start(first_line).unwrap_or(0);
            let len = doc.buffer().len();
            let end = if consume_trailing_newline {
                if last_line + 1 < doc.buffer().get_total_lines() {
                    doc.buffer()
                        .line_index
                        .get_start(last_line + 1)
                        .unwrap_or(len)
                } else {
                    start = start.saturating_sub(1);
                    len
                }
            } else {
                doc.buffer()
                    .line_index
                    .get_end(last_line, len)
                    .unwrap_or(len)
            };
            (start, end)
        }
        RangeKind::Charwise | RangeKind::Blockwise => {
            let end_offset = usize::from(range.inclusive);
            if range.new_cursor > range.anchor {
                (
                    range.anchor,
                    (range.new_cursor + end_offset).min(doc.buffer().len()),
                )
            } else {
                (
                    range.new_cursor,
                    (range.anchor + end_offset).min(doc.buffer().len()),
                )
            }
        }
    }
}

/// # Errors
pub(crate) fn execute_command(
    cmd: Command,
    doc: &mut Document,
    expand_tabs: bool,
    tab_width: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
    display_map: Option<&mut DisplayMap>,
) -> Result<(), RiftError> {
    if let Command::MoveBy(motion, count) = cmd {
        move_cmd(
            doc,
            display_map,
            motion,
            count,
            viewport_height,
            last_search_query,
        );
    } else if cmd.is_mutating() {
        execute_write_command(
            cmd,
            &mut doc.write()?,
            expand_tabs,
            tab_width,
            viewport_height,
            last_search_query,
        )?;
    }
    Ok(())
}

pub(crate) fn execute_write_command(
    cmd: Command,
    doc: &mut DocumentWrite<'_>,
    expand_tabs: bool,
    tab_width: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) -> Result<(), RiftError> {
    doc.clear_selection();
    match cmd {
        Command::DeleteMotion(motion, count) => {
            delete_motion_cmd(doc, motion, count, viewport_height, last_search_query);
        }
        Command::Change(motion, count) => {
            change_motion_cmd(doc, motion, count, viewport_height, last_search_query);
        }
        Command::DeleteRows(count) => {
            delete_line_cmd(doc, count);
        }
        Command::ChangeLine(count) => {
            change_line_cmd(doc, count);
        }
        Command::TypeChar(ch) => {
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
        Command::MoveBy(..)
        | Command::BeginInsert(_)
        | Command::BeginReplace
        | Command::ReadExLine
        | Command::ReadSearchPattern
        | Command::AppendToCommandLine(_)
        | Command::DeleteFromCommandLine
        | Command::ExecuteCommandLine
        | Command::ExecuteSearch
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
    let count = count.min(doc.buffer().len().max(1));

    for _ in 0..count {
        if !doc.apply_motion(
            motion,
            display_map.as_deref_mut(),
            crate::wrap::OperatorContext::Move,
            viewport_height,
            last_search_query,
        ) {
            break;
        }
    }
}

/// Delete over a motion (`Delete`).
fn delete_motion_cmd(
    doc: &mut DocumentWrite<'_>,
    motion: Motion,
    count: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) {
    let Some(range) = doc.motion_range(motion, count, viewport_height, last_search_query) else {
        return;
    };
    let (delete_start, delete_end) = range_to_offsets(&range, doc, true);
    if delete_end > delete_start {
        doc.edit_group("Delete", |doc| {
            let _ = doc.edit(Edit::delete(delete_start..delete_end), EditSource::User);
        });
    }
}

/// Change over a motion (`Change`).
fn change_motion_cmd(
    doc: &mut DocumentWrite<'_>,
    motion: Motion,
    count: usize,
    viewport_height: usize,
    last_search_query: Option<&str>,
) {
    let Some(range) = doc.change_range(motion, count, viewport_height, last_search_query) else {
        return;
    };
    let (delete_start, delete_end) = range_to_offsets(&range, doc, false);
    if delete_end > delete_start {
        let _ = doc.edit(Edit::delete(delete_start..delete_end), EditSource::User);
    }
}

/// Delete `count` whole lines (`DeleteLine`).
fn delete_line_cmd(doc: &mut DocumentWrite<'_>, count: usize) {
    if doc.is_clipboard_index() {
        doc.delete_clipboard_rows(count);
        return;
    }
    doc.edit_group("DeleteLine", |doc| {
        doc.move_cursor(crate::action::Motion::StartOfLine);
        let start = doc.buffer().cursor();
        let mut reached_last_line = false;
        for _ in 0..count.max(1) {
            if !doc.move_cursor(crate::action::Motion::Down) {
                reached_last_line = doc.buffer().get_line() <= doc.buffer().last_real_line();
                break;
            }
        }
        if reached_last_line {
            doc.move_cursor(crate::action::Motion::EndOfLine);
            let end = doc.buffer().cursor();
            if end > start {
                let _ = doc.edit(Edit::delete(start..end), EditSource::User);
            }
            if start > 0 {
                let _ = doc.edit(Edit::DeleteBackward, EditSource::User);
                doc.move_cursor(crate::action::Motion::StartOfLine);
            }
        } else {
            let end = doc.buffer().cursor();
            let _ = doc.edit(Edit::delete(start..end), EditSource::User);
        }
    });
}

/// Change `count` lines up to their end (`ChangeLine`).
fn change_line_cmd(doc: &mut DocumentWrite<'_>, count: usize) {
    doc.move_cursor(crate::action::Motion::StartOfLine);
    let start = doc.buffer().cursor();
    for _ in 0..count.max(1).saturating_sub(1) {
        if doc.buffer().get_line() >= doc.buffer().last_real_line() {
            break;
        }
        doc.move_cursor(crate::action::Motion::Down);
    }
    doc.move_cursor(crate::action::Motion::EndOfLine);
    let end = doc.buffer().cursor();
    if end > start {
        let _ = doc.edit(Edit::delete(start..end), EditSource::User);
    }
}

/// Insert a character; `expand_tabs` turns a tab into spaces.
fn insert_char_cmd(
    doc: &mut DocumentWrite<'_>,
    expand_tabs: bool,
    tab_width: usize,
    ch: char,
) -> Result<(), RiftError> {
    if ch == '\t' && expand_tabs {
        for _ in 0..tab_fill_width(doc.buffer(), tab_width) {
            doc.edit(Edit::type_text(' '), EditSource::User)?;
        }
        return Ok(());
    }
    doc.edit(Edit::type_text(ch), EditSource::User).map(|_| ())
}

/// Replace `count` characters under the cursor (`ReplaceChar`).
fn replace_char_cmd(doc: &mut DocumentWrite<'_>, ch: char, count: usize) -> Result<(), RiftError> {
    let pos = doc.buffer().cursor();
    let avail = doc.buffer().len().saturating_sub(pos);
    let replace_count = count.min(avail);
    if replace_count > 0 {
        doc.edit_group("Replace", |doc| {
            doc.edit(
                Edit::replace(
                    pos..pos + replace_count,
                    vec![crate::character::Character::from(ch); replace_count],
                ),
                EditSource::User,
            )
        })?;
    }
    Ok(())
}

fn replace_backspace_cmd(
    doc: &mut DocumentWrite<'_>,
    restore: Option<crate::character::Character>,
) -> Result<(), RiftError> {
    let Some(original) = restore else {
        let _ = doc.edit(Edit::DeleteBackward, EditSource::User);
        return Ok(());
    };
    let Some(pos) = doc.buffer().cursor().checked_sub(1) else {
        return Ok(());
    };

    doc.edit(Edit::replace(pos..pos + 1, original), EditSource::User)?;
    doc.set_cursor(pos, crate::document::ops::CursorCause::Programmatic)
}

fn delete_surround_cmd(doc: &mut DocumentWrite<'_>, ch: char, count: usize) {
    if let Some((open_range, close_range)) =
        crate::text_objects::resolve_surround_pair(ch, doc.buffer(), count)
    {
        doc.edit_group("DeleteSurround", |doc| {
            let _ = doc.edit(Edit::delete(close_range), EditSource::User);
            let _ = doc.edit(Edit::delete(open_range), EditSource::User);
        });
    }
}

fn change_surround_cmd(doc: &mut DocumentWrite<'_>, from: char, to: char, count: usize) {
    if let (Some((open_range, close_range)), Some((new_open, new_close))) = (
        crate::text_objects::resolve_surround_pair(from, doc.buffer(), count),
        crate::text_objects::surround_strings(to, count),
    ) {
        doc.edit_group("ChangeSurround", |doc| {
            let _ = doc.edit(Edit::delete(close_range.clone()), EditSource::User);
            let _ = doc.edit(
                Edit::insert(close_range.start, &new_close),
                EditSource::User,
            );
            let _ = doc.edit(Edit::delete(open_range.clone()), EditSource::User);
            let _ = doc.edit(Edit::insert(open_range.start, &new_open), EditSource::User);
        });
    }
}

fn add_surround_cmd(
    doc: &mut DocumentWrite<'_>,
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
    let Some(range) = doc.motion_range(motion, motion_count, viewport_height, last_search_query)
    else {
        return;
    };
    let (start, end) = range_to_offsets(&range, doc, false);
    doc.edit_group("AddSurround", |doc| {
        let _ = doc.edit(Edit::insert(end, &close), EditSource::User);
        let _ = doc.edit(Edit::insert(start, &open), EditSource::User);
    });
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
