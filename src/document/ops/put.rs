use super::super::Document;
use super::{Edit, EditSource};
use crate::character::Character;
use crate::clipboard::Clip;
use crate::wrap::RangeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PutAt {
    Cursor {
        before: bool,
    },
    Region {
        start: usize,
        end: usize,
        before: bool,
    },
    Paste,
    Selection {
        anchor: usize,
        kind: RangeKind,
        count: usize,
    },
}

impl Document {
    pub(crate) fn put(&mut self, clip: &Clip, at: PutAt) -> bool {
        self.write().is_ok_and(|mut writer| writer.put(clip, at))
    }
}

impl super::DocumentWrite<'_> {
    pub(crate) fn put(&mut self, clip: &Clip, at: PutAt) -> bool {
        self.doc.put_edit(clip, at)
    }
}

impl Document {
    fn put_edit(&mut self, clip: &Clip, at: PutAt) -> bool {
        match at {
            PutAt::Cursor { before } => {
                let cursor = self.buffer.cursor();
                let char_at = if before {
                    cursor
                } else {
                    (cursor + 1).min(self.buffer.len())
                };
                let start = self.put_beside(clip, char_at, cursor, !before);
                if clip.kind() == RangeKind::Linewise || clip.text().contains(&Character::Newline) {
                    let _ = self.set_cursor(start, crate::document::ops::CursorCause::Programmatic);
                }
            }
            PutAt::Region { start, end, before } => {
                let (char_at, line_of) = if before {
                    (start, start)
                } else {
                    (end, end.saturating_sub(1))
                };
                self.put_beside(clip, char_at, line_of, !before);
            }
            PutAt::Paste => {
                if clip.text().is_empty() {
                    return false;
                }
                self.begin_transaction("Paste");
                let _ = self.apply_edit(Edit::type_text(clip.text()), EditSource::User);
                self.commit_transaction();
            }
            PutAt::Selection {
                anchor,
                kind,
                count,
            } => self.put_over_selection(clip, anchor, kind, count),
        }
        true
    }

    fn put_beside(&mut self, clip: &Clip, char_at: usize, line_of: usize, below: bool) -> usize {
        let text = clip.text();
        let (pos, needs_newline) = if clip.kind() == RangeKind::Linewise {
            let line = self.buffer.line_index.get_line_at(line_of);
            let line_start = |line| self.buffer.line_index.get_start(line).unwrap_or(0);
            if !below {
                (line_start(line), false)
            } else if line + 1 < self.buffer.get_total_lines() {
                (line_start(line + 1), false)
            } else {
                (self.buffer.len(), true)
            }
        } else {
            (char_at, false)
        };
        let _ = self.set_cursor(pos, crate::document::ops::CursorCause::Programmatic);
        self.begin_transaction("Put");
        if needs_newline {
            let trimmed = text.strip_suffix(&[Character::Newline]).unwrap_or(text);
            let _ = self.apply_edit(Edit::insert(pos, Character::Newline), EditSource::User);
            let _ = self.apply_edit(Edit::insert(pos + 1, trimmed), EditSource::User);
        } else {
            let _ = self.apply_edit(Edit::insert(pos, text), EditSource::User);
        }
        self.commit_transaction();
        pos + usize::from(needs_newline)
    }

    fn put_over_selection(&mut self, clip: &Clip, anchor: usize, kind: RangeKind, count: usize) {
        let cursor = self.buffer.cursor();
        let region = crate::selection::Region::new(anchor, cursor, kind);
        let (start, end) = region.buffer_span(&self.buffer);
        let start = start.min(self.buffer.len());
        let end = end.min(self.buffer.len());
        let target_linewise = kind == RangeKind::Linewise;
        let source_linewise = clip.kind() == RangeKind::Linewise;
        let last_selected_line = self.buffer.line_index.get_line_at(anchor.max(cursor));
        let has_following_line = last_selected_line + 1 < self.buffer.get_total_lines();
        let text = if target_linewise && source_linewise && !has_following_line {
            clip.text()
                .strip_suffix(&[Character::Newline])
                .unwrap_or_else(|| clip.text())
        } else {
            clip.text()
        };

        let _ = self.set_cursor(start, crate::document::ops::CursorCause::Programmatic);
        self.begin_transaction("Put");
        let _ = self.apply_edit(Edit::delete(start..end), EditSource::User);
        if source_linewise && !target_linewise {
            let _ = self.apply_edit(Edit::type_text(Character::Newline), EditSource::User);
        }
        let inserted_start = self.buffer.cursor();
        for _ in 1..count {
            let _ = self.apply_edit(Edit::type_text(clip.text()), EditSource::User);
        }
        let _ = self.apply_edit(Edit::type_text(text), EditSource::User);
        if target_linewise && !source_linewise && has_following_line {
            let _ = self.apply_edit(Edit::type_text(Character::Newline), EditSource::User);
        }
        let inserted_cursor = if target_linewise || source_linewise {
            let first_line_end = text
                .iter()
                .position(|&ch| ch == Character::Newline)
                .unwrap_or(text.len());
            let first_nonblank = text[..first_line_end]
                .iter()
                .position(|&ch| !matches!(ch, Character::Unicode(' ') | Character::Tab))
                .unwrap_or_else(|| first_line_end.saturating_sub(1));
            inserted_start + first_nonblank
        } else if text.contains(&Character::Newline) {
            inserted_start
        } else {
            self.buffer.cursor().saturating_sub(1)
        };
        let _ = self.set_cursor(
            inserted_cursor.min(self.buffer.len()),
            crate::document::ops::CursorCause::Programmatic,
        );
        self.buffer.clear_desired_col();
        self.clear_selection();
        self.commit_transaction();
    }
}
