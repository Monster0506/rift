use super::runtime::CLIPBOARD_STATE_KEY;
use super::Document;
use crate::annotations::kind::Kind;
use crate::annotations::marker::Marker;
use crate::annotations::value::Value;
use crate::annotations::{Anchor, Annotation, AnnotationOwner};
use crate::character::Character;
use crate::clipboard::Clip;
use std::ops::Range;

pub(super) const ROW_KIND: &str = "clipboard.entry";
pub(super) const EMPTY_LABEL: &str = "(empty)";
const SOURCE_KEY: &str = "source";
const POSITION_KEY: &str = "position";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardSide {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ClipboardEdit {
    #[default]
    Number,
    Rows,
}

impl Document {
    pub(super) fn track_clipboard_row(&mut self, closing: usize, source: usize, position: usize) {
        let mut payload = Value::map();
        payload.set(
            SOURCE_KEY,
            Value::Int(i64::try_from(source).expect("clipboard source fits i64")),
        );
        payload.set(
            POSITION_KEY,
            Value::Int(i64::try_from(position).expect("clipboard position fits i64")),
        );
        self.annotations.add(
            Annotation::new(
                Kind::new(ROW_KIND),
                Anchor::Point(Marker::right(self.buffer.char_to_byte(closing))),
                AnnotationOwner::System,
            )
            .with_payload(payload)
            .with_visible(false),
        );
    }

    fn clipboard_row(&self, line: usize) -> Option<(usize, usize)> {
        let start = self.buffer.line_index.get_start(line)?;
        let span = self.clipboard_number_span(start)?;
        let closing = self.buffer.char_to_byte(span.end);
        let mut rows = self.annotations.query_at(closing).filter(|annotation| {
            annotation.kind.as_str() == ROW_KIND
                && matches!(annotation.anchor, Anchor::Point(marker) if marker.offset == closing)
        });
        let row = rows.next()?;
        if rows.next().is_some() {
            return None;
        }
        let Value::Int(source) = row.payload.get(SOURCE_KEY)? else {
            return None;
        };
        let Value::Int(position) = row.payload.get(POSITION_KEY)? else {
            return None;
        };
        Some((
            usize::try_from(*source).ok()?,
            usize::try_from(*position).ok()?,
        ))
    }

    pub(crate) fn clipboard_source_at(&self, line: usize) -> Option<usize> {
        self.clipboard_row(line).map(|(source, _)| source)
    }

    pub(crate) fn set_clipboard_entry(&mut self, index: usize, clip: Clip) -> bool {
        let Some(entry) = self
            .state
            .try_get_mut(CLIPBOARD_STATE_KEY)
            .and_then(|state| state.entries.get_mut(index))
        else {
            return false;
        };
        *entry = clip;
        true
    }

    pub(crate) fn clipboard_clip(&self, source: usize) -> Option<&Clip> {
        let state = self.state.try_get(CLIPBOARD_STATE_KEY)?;
        if source < state.entries.len() {
            return state.entries.get(source);
        }
        state.inserted.get(source - state.entries.len())
    }

    pub(crate) fn parse_clipboard_entries(&self) -> Option<Vec<Clip>> {
        let state = self.state.try_get(CLIPBOARD_STATE_KEY)?;
        let content = self.buffer.to_string();
        if content == EMPTY_LABEL && state.entries.is_empty() {
            return Some(Vec::new());
        }

        let mut rows = Vec::new();
        for (line, text) in content.lines().enumerate() {
            let number = text.strip_prefix('[')?.strip_suffix(']')?;
            if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let destination = number.parse::<usize>().ok()?;
            let (source, original) = self.clipboard_row(line)?;
            rows.push((line, source, destination, original));
        }

        let mut order: Vec<usize> = (0..rows.len()).collect();
        for &(row, _, destination, original) in &rows {
            if destination == original {
                continue;
            }
            if destination >= order.len() {
                return None;
            }
            let current = order.iter().position(|&index| index == row)?;
            order.remove(current);
            order.insert(destination, row);
        }
        order
            .into_iter()
            .map(|row| self.clipboard_clip(rows[row].1).cloned())
            .collect()
    }

    pub(crate) fn clipboard_number_span(&self, pos: usize) -> Option<Range<usize>> {
        let line = self.buffer.line_index.get_line_at(pos);
        let start = self.buffer.line_index.get_start(line)?;
        let end = self
            .buffer
            .line_index
            .get_start(line + 1)
            .map_or(self.buffer.len(), |next| next - 1);
        if end < start + 2
            || self.buffer.char_at(start) != Some(Character::from('['))
            || self.buffer.char_at(end - 1) != Some(Character::from(']'))
        {
            return None;
        }
        Some(start + 1..end - 1)
    }

    pub(super) fn clipboard_insert_allowed(
        &self,
        pos: usize,
        mut text: impl Iterator<Item = char>,
    ) -> bool {
        if !self.is_clipboard_index()
            || self
                .state
                .try_get(CLIPBOARD_STATE_KEY)
                .is_some_and(|state| state.edit == ClipboardEdit::Rows)
        {
            return true;
        }
        self.clipboard_number_span(pos).is_some_and(|span| {
            span.start <= pos && pos <= span.end && text.all(|character| character.is_ascii_digit())
        })
    }

    fn insert_clipboard_text(&mut self, text: &str) {
        if let Some(state) = self.state.try_get_mut(CLIPBOARD_STATE_KEY) {
            state.edit = ClipboardEdit::Rows;
        }
        let _ = self.insert_str(text);
        if let Some(state) = self.state.try_get_mut(CLIPBOARD_STATE_KEY) {
            state.edit = ClipboardEdit::Number;
        }
    }

    pub(super) fn clipboard_delete_allowed(&self, start: usize, end: usize) -> bool {
        if !self.is_clipboard_index() {
            return true;
        }
        if self
            .clipboard_number_span(start)
            .is_some_and(|span| span.start <= start && start <= end && end <= span.end)
        {
            return true;
        }
        let line = self.buffer.line_index.get_line_at(start);
        let at_start = self.buffer.line_index.get_start(line) == Some(start);
        let at_end = end == self.buffer.len()
            || self
                .buffer
                .line_index
                .get_start(self.buffer.line_index.get_line_at(end))
                == Some(end);
        let before_final_rows =
            self.buffer.char_at(start) == Some(Character::Newline) && end == self.buffer.len();
        (at_start && at_end) || before_final_rows
    }

    pub(crate) fn delete_clipboard_rows(&mut self, count: usize) {
        let line = self.buffer.line_index.get_line_at(self.buffer.cursor());
        let mut start = self.buffer.line_index.get_start(line).unwrap_or(0);
        let end = self
            .buffer
            .line_index
            .get_start(line.saturating_add(count.max(1)))
            .unwrap_or(self.buffer.len());
        if end == self.buffer.len() && line > 0 {
            start -= 1;
        }
        self.begin_transaction("Delete clipboard rows");
        let _ = self.delete_range(start, end);
        self.commit_transaction();
    }

    pub(crate) fn clipboard_row_clips(&self, text: &Clip) -> Option<Vec<Clip>> {
        let line = self.buffer.line_index.get_line_at(self.buffer.cursor());
        let count = text
            .text()
            .iter()
            .filter(|&&c| c == Character::Newline)
            .count();
        (line..line + count)
            .map(|line| {
                self.clipboard_clip(self.clipboard_source_at(line)?)
                    .cloned()
            })
            .collect()
    }

    pub(crate) fn put_clipboard_rows(&mut self, rows: &[Clip], before: ClipboardSide) -> bool {
        if rows.is_empty() {
            return false;
        }
        let line = self.buffer.line_index.get_line_at(self.buffer.cursor());
        let empty = self.buffer.is_empty()
            || self
                .buffer
                .iter_at(0)
                .map(Character::to_char_lossy)
                .eq(EMPTY_LABEL.chars());
        let insert_line = if empty || before == ClipboardSide::Before {
            line
        } else {
            line + 1
        };
        let start = self
            .buffer
            .line_index
            .get_start(insert_line)
            .unwrap_or(self.buffer.len());
        let at_end = !empty && insert_line >= self.buffer.get_total_lines();
        let Some(state) = self.state.try_get_mut(CLIPBOARD_STATE_KEY) else {
            return false;
        };
        let first_source = state.entries.len() + state.inserted.len();
        state.inserted.extend(rows.iter().cloned());
        let mut text = String::new();
        if at_end {
            text.push('\n');
        }
        let mut closings = Vec::with_capacity(rows.len());
        for (offset, _) in rows.iter().enumerate() {
            let position = insert_line + offset;
            let _ = std::fmt::Write::write_fmt(&mut text, format_args!("[{position}]"));
            closings.push((start + text.len() - 1, first_source + offset, position));
            text.push('\n');
        }
        if at_end || empty {
            text.pop();
        }

        self.begin_transaction("Put clipboard rows");
        if empty && !self.buffer.is_empty() {
            let _ = self.delete_range(0, self.buffer.len());
        }
        let _ = self.buffer.set_cursor(start.min(self.buffer.len()));
        self.insert_clipboard_text(&text);
        for (closing, source, position) in closings {
            self.track_clipboard_row(closing, source, position);
        }
        self.commit_transaction();
        let _ = self.buffer.set_cursor(start + usize::from(at_end) + 1);
        true
    }
}
