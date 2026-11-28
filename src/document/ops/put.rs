use super::super::Document;
use super::{Edit, EditSource};
use crate::character::Character;
use crate::clipboard::Clip;
use crate::selection::{Target, TargetSet};
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

    pub(crate) fn put_over_targets(
        &mut self,
        clip: &Clip,
        targets: &TargetSet,
        count: usize,
    ) -> bool {
        let Some(first) = targets.targets().last() else {
            return false;
        };
        let _ = self.set_cursor(first.start, super::CursorCause::Programmatic);
        self.edit_group("Put", |doc| {
            let mut changed = false;
            let mut cursor = first.start;
            for target in targets.targets() {
                let put = SelectionPut::new(clip, target, count);
                if doc
                    .edit(put.edit, EditSource::User)
                    .is_ok_and(|applied| applied)
                {
                    changed = true;
                    cursor = put.cursor;
                }
            }
            if changed {
                let _ = doc.set_cursor(
                    cursor.min(doc.buffer().len()),
                    super::CursorCause::Programmatic,
                );
                doc.doc.buffer.clear_desired_col();
            }
            changed
        })
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
}

struct SelectionPut {
    edit: Edit,
    cursor: usize,
}

impl SelectionPut {
    fn new(clip: &Clip, target: &Target, count: usize) -> Self {
        let target_linewise = target.kind == RangeKind::Linewise;
        let source_linewise = clip.kind() == RangeKind::Linewise;
        let has_following_line = !target.last_line;
        let text = clip.text();
        let tail = if target_linewise && source_linewise && !has_following_line {
            text.strip_suffix(&[Character::Newline]).unwrap_or(text)
        } else {
            text
        };
        let leading = source_linewise && !target_linewise;
        let trailing = target_linewise && !source_linewise && has_following_line;
        let inserted_start = target.start + usize::from(leading);
        let mut replacement = Vec::with_capacity(
            text.len() * (count - 1) + tail.len() + usize::from(leading) + usize::from(trailing),
        );
        replacement.extend(leading.then_some(Character::Newline));
        for _ in 1..count {
            replacement.extend_from_slice(text);
        }
        replacement.extend_from_slice(tail);
        replacement.extend(trailing.then_some(Character::Newline));
        let cursor = if target_linewise || source_linewise {
            let first_line_end = tail
                .iter()
                .position(|&ch| ch == Character::Newline)
                .unwrap_or(tail.len());
            let first_nonblank = tail[..first_line_end]
                .iter()
                .position(|&ch| !matches!(ch, Character::Unicode(' ') | Character::Tab))
                .unwrap_or_else(|| first_line_end.saturating_sub(1));
            inserted_start + first_nonblank
        } else if text.contains(&Character::Newline) {
            inserted_start
        } else {
            (target.start + replacement.len()).saturating_sub(1)
        };
        Self {
            edit: Edit::replace(target.start..target.end, replacement),
            cursor,
        }
    }
}
