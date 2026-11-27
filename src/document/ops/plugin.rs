use super::{CursorCause, Edit, EditPolicy, EditSource};
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::document::Document;

pub(crate) enum BufferChange<'a> {
    AppendLines(&'a [String]),
    Insert(&'a str),
    DeleteNear {
        count: usize,
        backward: bool,
    },
    ReplaceLines {
        start: usize,
        end: usize,
        lines: &'a [String],
    },
    Cursor {
        row: usize,
        col: usize,
    },
    Center {
        row: usize,
    },
}

pub(crate) enum BufferTarget<'a> {
    Document(&'a mut Document),
    Snapshot(&'a mut TextBuffer, EditPolicy),
}

impl BufferTarget<'_> {
    fn buffer(&self) -> &TextBuffer {
        match self {
            Self::Document(doc) => doc.buffer(),
            Self::Snapshot(buffer, _) => buffer,
        }
    }

    fn edit(&mut self, edit: Edit) -> bool {
        match self {
            Self::Document(doc) => matches!(doc.edit(edit, EditSource::Plugin), Ok(true)),
            Self::Snapshot(buffer, policy) => policy.project(buffer, edit),
        }
    }

    fn set_cursor(&mut self, position: usize) {
        match self {
            Self::Document(doc) => {
                let _ = doc.set_cursor(position, CursorCause::Plugin);
            }
            Self::Snapshot(buffer, _) => {
                let _ = buffer.set_cursor(position);
            }
        }
    }
}

impl BufferChange<'_> {
    pub(crate) fn apply(self, mut target: BufferTarget<'_>) -> Option<(usize, usize)> {
        match self {
            Self::AppendLines(lines) => {
                let capacity =
                    lines.iter().map(String::len).sum::<usize>() + lines.len().saturating_sub(1);
                let mut text = Vec::with_capacity(capacity);
                for (index, line) in lines.iter().enumerate() {
                    if index != 0 {
                        text.push(Character::Newline);
                    }
                    text.extend(line.chars().map(Character::from));
                }
                target.edit(Edit::insert(target.buffer().len(), text));
            }
            Self::Insert(text) => {
                target.edit(Edit::type_text(text));
            }
            Self::DeleteNear { count, backward } => {
                for _ in 0..count {
                    let edit = if backward {
                        Edit::DeleteBackward
                    } else {
                        Edit::DeleteForward
                    };
                    if !target.edit(edit) {
                        break;
                    }
                }
            }
            Self::ReplaceLines { start, end, lines } => {
                if let Some(edit) = target.buffer().line_replacement(start, end, lines) {
                    if target.edit(Edit::replace(edit.start..edit.end, edit.text)) {
                        target.set_cursor(edit.cursor.min(target.buffer().len()));
                    }
                }
            }
            Self::Cursor { row, col } => {
                target.set_cursor(target.buffer().line_col_position(row, col));
            }
            Self::Center { row } => {
                let total = target.buffer().get_total_lines();
                let row = row.min(total.saturating_sub(1));
                target.set_cursor(target.buffer().line_col_position(row, 0));
                return Some((row, total));
            }
        }
        None
    }
}
