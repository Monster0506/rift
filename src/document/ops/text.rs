use super::super::clipboard::{clipboard_delete_allowed, clipboard_insert_allowed, ClipboardEdit};
use super::super::runtime::{StructuralEditPolicy, CLIPBOARD_STATE_KEY};
use super::super::{AnnotationUndo, AnnotationUndoHint, Document};
use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::error::RiftError;
use crate::history::{EditOperation, EditTransaction, Position, Range, UndoTree};

pub trait IntoText {
    fn into_text(self) -> Vec<Character>;
}

impl IntoText for Vec<Character> {
    fn into_text(self) -> Vec<Character> {
        self
    }
}

impl IntoText for &[Character] {
    fn into_text(self) -> Vec<Character> {
        self.to_vec()
    }
}

impl IntoText for &Vec<Character> {
    fn into_text(self) -> Vec<Character> {
        self.clone()
    }
}

impl<const N: usize> IntoText for [Character; N] {
    fn into_text(self) -> Vec<Character> {
        self.to_vec()
    }
}

impl<const N: usize> IntoText for &[Character; N] {
    fn into_text(self) -> Vec<Character> {
        self.to_vec()
    }
}

impl IntoText for Character {
    fn into_text(self) -> Vec<Character> {
        vec![self]
    }
}

impl IntoText for char {
    fn into_text(self) -> Vec<Character> {
        vec![Character::from(self)]
    }
}

impl IntoText for &str {
    fn into_text(self) -> Vec<Character> {
        self.chars().map(Character::from).collect()
    }
}

impl IntoText for &String {
    fn into_text(self) -> Vec<Character> {
        self.as_str().into_text()
    }
}

impl IntoText for String {
    fn into_text(self) -> Vec<Character> {
        self.as_str().into_text()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    InsertAtCursor(Vec<Character>),
    Insert {
        at: usize,
        text: Vec<Character>,
    },
    Delete(std::ops::Range<usize>),
    DeleteBackward,
    DeleteForward,
    Replace {
        range: std::ops::Range<usize>,
        text: Vec<Character>,
    },
}

impl Edit {
    #[must_use]
    pub fn type_text(text: impl IntoText) -> Self {
        Self::InsertAtCursor(text.into_text())
    }

    #[must_use]
    pub fn insert(at: usize, text: impl IntoText) -> Self {
        Self::Insert {
            at,
            text: text.into_text(),
        }
    }

    #[must_use]
    pub const fn delete(range: std::ops::Range<usize>) -> Self {
        Self::Delete(range)
    }

    #[must_use]
    pub fn replace(range: std::ops::Range<usize>, text: impl IntoText) -> Self {
        Self::Replace {
            range,
            text: text.into_text(),
        }
    }

    fn resolve(self, buffer: &TextBuffer) -> Option<Resolved> {
        let cursor = buffer.cursor();
        let (start, end, text) = match self {
            Self::InsertAtCursor(text) => (cursor, cursor, text),
            Self::Insert { at, text } => (at, at, text),
            Self::Delete(range) => (range.start, range.end, Vec::new()),
            Self::DeleteBackward => {
                if cursor == 0 {
                    return None;
                }
                (cursor - 1, cursor, Vec::new())
            }
            Self::DeleteForward => {
                if cursor >= buffer.len() {
                    return None;
                }
                (cursor, cursor + 1, Vec::new())
            }
            Self::Replace { range, text } => (range.start, range.end, text),
        };
        Some(Resolved { start, end, text })
    }

    pub fn apply_to(self, buffer: &mut TextBuffer) -> bool {
        let Some(Resolved { start, end, text }) = self.resolve(buffer) else {
            return false;
        };
        if start > end || end > buffer.len() || (start == end && text.is_empty()) {
            return false;
        }
        splice(buffer, start, end, &text);
        true
    }
}

fn splice(buffer: &mut TextBuffer, start: usize, end: usize, text: &[Character]) {
    if start == end {
        let _ = buffer.set_cursor(start);
        buffer.insert_chars(text);
    } else if text.is_empty() {
        buffer.delete_range(start, end - start);
    } else {
        buffer.replace_range(start, end - start, text);
    }
    let _ = buffer.set_cursor((start + text.len()).min(buffer.len()));
}

#[derive(Debug)]
pub(crate) struct ReadOnlyEdit;

impl ReadOnlyEdit {
    fn check(read_only: bool) -> Result<(), Self> {
        if read_only {
            Err(Self)
        } else {
            Ok(())
        }
    }
}

impl From<ReadOnlyEdit> for RiftError {
    fn from(_: ReadOnlyEdit) -> Self {
        Self::warning(
            crate::error::ErrorType::Execution,
            "READ_ONLY",
            "Cannot make changes: buffer is read-only",
        )
    }
}

#[cfg(feature = "plugins")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct EditPolicy {
    read_only: bool,
    rules: EditRules,
}

#[cfg(feature = "plugins")]
impl EditPolicy {
    pub(crate) fn for_new_buffer(read_only: bool) -> Self {
        Self {
            read_only,
            ..Self::default()
        }
    }

    pub(super) fn check(self) -> Result<(), ReadOnlyEdit> {
        ReadOnlyEdit::check(self.read_only)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct EditRules {
    preserve_rows: bool,
    clipboard: Option<ClipboardEdit>,
}

impl EditRules {
    fn prepare(self, buffer: &TextBuffer, edit: Edit) -> Option<Resolved> {
        let char_delete = matches!(edit, Edit::DeleteBackward | Edit::DeleteForward);
        let resolved = edit.resolve(buffer)?;
        let Resolved {
            start,
            end,
            ref text,
        } = resolved;
        if self.preserve_rows && char_delete && buffer.char_at(start) == Some(Character::Newline) {
            return None;
        }
        if let Some(clipboard) = self.clipboard {
            if end <= buffer.len()
                && ((start < end && !clipboard_delete_allowed(buffer, start, end))
                    || (!text.is_empty()
                        && !clipboard_insert_allowed(
                            buffer,
                            clipboard,
                            start,
                            text.iter().copied().map(Character::to_char_lossy),
                        )))
            {
                return None;
            }
        }
        Some(resolved)
    }
}

#[cfg(feature = "plugins")]
impl EditPolicy {
    pub(super) fn project(self, buffer: &mut TextBuffer, edit: Edit) -> bool {
        let Some(Resolved { start, end, text }) = self.rules.prepare(buffer, edit) else {
            return false;
        };
        if start > end || end > buffer.len() || (start == end && text.is_empty()) {
            return false;
        }
        splice(buffer, start, end, &text);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditSource {
    User,
    #[cfg(feature = "plugins")]
    Plugin,
    #[cfg(feature = "lsp")]
    Lsp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::document) enum Origin {
    Source(EditSource),
    Replay,
    Generated,
}

pub(in crate::document) struct Resolved {
    pub(in crate::document) start: usize,
    pub(in crate::document) end: usize,
    pub(in crate::document) text: Vec<Character>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentKind {
    Render,
    Load,
}

pub(crate) enum Content {
    Text(Vec<Character>),
    Lines(crate::buffer::line_index::LineIndex),
}

impl<T: IntoText> From<T> for Content {
    fn from(text: T) -> Self {
        Self::Text(text.into_text())
    }
}

impl From<crate::buffer::line_index::LineIndex> for Content {
    fn from(lines: crate::buffer::line_index::LineIndex) -> Self {
        Self::Lines(lines)
    }
}

pub(crate) struct DocumentWrite<'a> {
    pub(in crate::document) doc: &'a mut Document,
}

impl std::ops::Deref for DocumentWrite<'_> {
    type Target = Document;

    fn deref(&self) -> &Document {
        self.doc
    }
}

impl DocumentWrite<'_> {
    pub(crate) fn edit(&mut self, edit: Edit, source: EditSource) -> Result<bool, RiftError> {
        self.doc.apply_edit(edit, source)
    }

    pub(crate) fn move_cursor(&mut self, motion: crate::action::Motion) -> bool {
        self.doc.move_cursor(motion)
    }

    pub(crate) fn set_cursor(
        &mut self,
        pos: usize,
        cause: super::CursorCause,
    ) -> Result<(), RiftError> {
        self.doc.set_cursor(pos, cause)
    }

    pub(crate) fn clear_selection(&mut self) {
        self.doc.clear_selection();
    }

    pub(crate) fn bank_selection(&mut self, kind: crate::wrap::RangeKind) {
        self.doc.bank_selection(kind);
    }

    pub(crate) fn take_selection_batch(&mut self) -> Vec<crate::selection::Region> {
        self.doc.take_selection_batch()
    }

    #[cfg(feature = "lsp")]
    pub(crate) fn invalidate_syntax(&mut self) {
        self.doc.invalidate_syntax();
    }

    pub(crate) fn motion_range(
        &mut self,
        motion: crate::action::Motion,
        count: usize,
        viewport_height: usize,
        query: Option<&str>,
    ) -> Option<crate::wrap::MotionRange> {
        super::cursor::compute_motion_range(motion, count, self.doc, viewport_height, query)
    }

    pub(crate) fn change_range(
        &mut self,
        motion: crate::action::Motion,
        count: usize,
        viewport_height: usize,
        query: Option<&str>,
    ) -> Option<crate::wrap::MotionRange> {
        super::cursor::compute_change_range(motion, count, self.doc, viewport_height, query)
    }

    pub(crate) fn overwrite_char(&mut self, ch: char) -> Result<bool, RiftError> {
        let cursor = self.buffer().cursor();
        let edit = if self.char_to_overwrite().is_some() {
            Edit::replace(cursor..cursor + 1, ch)
        } else {
            Edit::type_text(ch)
        };
        self.edit(edit, EditSource::User)
    }
}

impl Document {
    pub(crate) fn replace_content(&mut self, content: impl Into<Content>, kind: ContentKind) {
        self.commit_pending_ghost();
        let mut buffer = match content.into() {
            Content::Text(chars) => TextBuffer::from_chars(&chars),
            Content::Lines(lines) => TextBuffer::from_line_index(lines),
        };
        buffer.revision = self.buffer.revision.wrapping_add(1);
        self.buffer = buffer;
        self.buffer_replaced |= !self.is_terminal();
        self.content_version = self.content_version.wrapping_add(1);
        #[cfg(feature = "lsp")]
        self.pending_lsp_edits.clear();
        if let Some(syntax) = &mut self.syntax {
            syntax.invalidate_trees();
        }
        if kind == ContentKind::Load {
            self.syntax = None;
            self.history = UndoTree::new();
            self.current_transaction = None;
            self.transaction_depth = 0;
            self.pending_annotation_snapshot = None;
            self.annotation_undo_stack.clear();
            self.annotation_redo_stack.clear();
            self.clear_selection();
            self.set_annotations(crate::annotations::kind::well_known::TEXT_COLOR, []);
        }
        self.record_text_change();
        self.reparse_syntax();
    }

    pub(crate) const fn buffer_replaced(&self) -> bool {
        self.buffer_replaced
    }

    pub(crate) fn take_buffer_replaced(&mut self) -> bool {
        std::mem::take(&mut self.buffer_replaced)
    }

    pub(crate) fn write(&mut self) -> Result<DocumentWrite<'_>, ReadOnlyEdit> {
        ReadOnlyEdit::check(self.is_read_only())?;
        Ok(DocumentWrite { doc: self })
    }

    #[cfg(feature = "plugins")]
    pub(crate) fn edit_policy(&self) -> EditPolicy {
        EditPolicy {
            read_only: self.is_read_only(),
            rules: self.edit_rules(),
        }
    }

    fn edit_rules(&self) -> EditRules {
        EditRules {
            preserve_rows: self.policies().structural_edit == StructuralEditPolicy::PreserveRows,
            clipboard: self.is_clipboard_index().then(|| {
                self.state
                    .try_get(CLIPBOARD_STATE_KEY)
                    .map_or(ClipboardEdit::Number, |s| s.edit)
            }),
        }
    }

    pub(crate) fn edit(&mut self, edit: Edit, source: EditSource) -> Result<bool, RiftError> {
        self.write()?.edit(edit, source)
    }

    pub(in crate::document) fn apply_edit(
        &mut self,
        edit: Edit,
        source: EditSource,
    ) -> Result<bool, RiftError> {
        let policy = self.edit_rules();
        let Some(resolved) = policy.prepare(&self.buffer, edit) else {
            return Ok(false);
        };
        let applied = self.apply_resolved(resolved, Origin::Source(source));
        self.settle_syntax();
        applied
    }

    #[must_use]
    pub(crate) fn char_to_overwrite(&self) -> Option<Character> {
        self.buffer
            .char_at(self.buffer.cursor())
            .filter(|c| *c != Character::Newline)
    }

    pub(in crate::document) fn get_point(&self, byte_offset: usize) -> (usize, usize) {
        let (point, _) = self.get_edit_points(byte_offset);
        point
    }

    pub(in crate::document) fn get_edit_points(
        &self,
        byte_offset: usize,
    ) -> ((usize, usize), Position) {
        let char_offset = self.buffer.byte_to_char(byte_offset);
        let line = self.buffer.line_index.get_line_at(char_offset);
        let line_start_char = self.buffer.line_index.get_start(line).unwrap_or(0);
        let line_start_byte = self.buffer.char_to_byte(line_start_char);
        let byte_col = byte_offset.saturating_sub(line_start_byte);
        let char_col = char_offset.saturating_sub(line_start_char);
        let position = Position::new(
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(char_col).unwrap_or(u32::MAX),
        );
        ((line, byte_col), position)
    }

    pub(in crate::document) fn apply_resolved(
        &mut self,
        resolved: Resolved,
        origin: Origin,
    ) -> Result<bool, RiftError> {
        let Resolved { start, end, text } = resolved;
        if start > end || (start == end && text.is_empty()) {
            return Ok(false);
        }
        if end > self.buffer.len() {
            return Err(RiftError::new(
                crate::error::ErrorType::Internal,
                "INVALID_RANGE",
                format!(
                    "End position {} out of bounds (len: {})",
                    end,
                    self.buffer.len()
                ),
            ));
        }

        self.commit_pending_ghost();
        let cursor_before = self.buffer.cursor();
        let removed: Vec<Character> = self.buffer.chars(start..end).collect();
        let start_line = self.buffer.line_index.get_line_at(start);
        let removed_newlines = removed.iter().filter(|&&c| c == Character::Newline).count();
        let inserted_newlines = text.iter().filter(|&&c| c == Character::Newline).count();
        let deleted_lines = (removed_newlines > 0).then(|| {
            let at_col0 = self.buffer.line_index.get_start(start_line) == Some(start);
            let first_line = if at_col0 { start_line } else { start_line + 1 };
            (first_line, removed_newlines, start_line)
        });

        let start_byte = self.buffer.char_to_byte(start);
        let old_end_byte = self.buffer.char_to_byte(end);
        let (start_position, history_start) = self.get_edit_points(start_byte);
        let (old_end_position, history_end) = self.get_edit_points(old_end_byte);

        splice(&mut self.buffer, start, end, &text);
        let new_end = start + text.len();
        let new_end_byte = self.buffer.char_to_byte(new_end);
        let new_end_position = self.get_point(new_end_byte);

        let line_inserts: Vec<usize> = (0..inserted_newlines).map(|i| start_line + 1 + i).collect();

        let (op, description, hint) = history_entry(
            removed,
            &text,
            Range::new(history_start, history_end),
            start_byte..new_end_byte,
            &line_inserts,
        );
        if origin == Origin::Replay {
            self.content_version = self.content_version.wrapping_add(1);
            #[cfg(feature = "lsp")]
            self.pending_lsp_edits.push(op);
        } else {
            self.record_edit(op, &description, hint, cursor_before);
        }

        if let Some(syntax) = &mut self.syntax {
            syntax.notify_edit(
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            );
        }

        if origin != Origin::Replay {
            self.annotations
                .on_edit(start_byte, old_end_byte, new_end_byte);
            if let Some((first_line, count, merge_line)) = deleted_lines {
                self.annotations
                    .on_lines_deleted(first_line, count, merge_line);
            }
            for at in line_inserts {
                self.annotations.on_line_inserted(at);
            }
        }

        self.mark_syntax_stale();
        self.record_text_change();
        Ok(true)
    }

    fn record_edit(
        &mut self,
        op: EditOperation,
        description: &str,
        annotation_undo: AnnotationUndoHint,
        cursor_before: usize,
    ) {
        self.content_version = self.content_version.wrapping_add(1);
        #[cfg(feature = "lsp")]
        self.pending_lsp_edits.push(op.clone());
        if let Some(tx) = &mut self.current_transaction {
            tx.record(op);
        } else {
            let entry = match annotation_undo {
                AnnotationUndoHint::Insertion {
                    start,
                    new_end,
                    line_inserts,
                } => AnnotationUndo::Insertion {
                    start,
                    new_end,
                    line_inserts,
                },
                AnnotationUndoHint::Snapshot => {
                    AnnotationUndo::Snapshot(self.annotations.snapshot())
                }
            };
            self.annotation_undo_stack.push(entry);
            self.annotation_redo_stack.clear();
            let mut tx = EditTransaction::new(description);
            tx.cursor_before = Some(cursor_before);
            tx.record(op);
            tx.cursor_after = Some(self.buffer.cursor());
            self.history.push(tx, None);
        }
    }
}

fn history_entry(
    removed: Vec<Character>,
    text: &[Character],
    range: Range,
    bytes: std::ops::Range<usize>,
    line_inserts: &[usize],
) -> (EditOperation, String, AnnotationUndoHint) {
    if removed.is_empty() {
        let description = match text {
            [single] => format!("Insert '{}'", describe_char(*single)),
            _ => format!("Insert {} chars", text.len()),
        };
        return (
            EditOperation::Insert {
                position: range.start,
                len: bytes.end - bytes.start,
                text: text.to_vec(),
            },
            description,
            AnnotationUndoHint::Insertion {
                start: bytes.start,
                new_end: bytes.end,
                line_inserts: line_inserts.to_vec(),
            },
        );
    }
    if text.is_empty() {
        let description = match removed.as_slice() {
            [single] => format!("Delete '{}'", describe_char(*single)),
            _ => format!("Delete {} chars", removed.len()),
        };
        return (
            EditOperation::Delete {
                range,
                deleted_text: removed,
            },
            description,
            AnnotationUndoHint::Snapshot,
        );
    }
    (
        EditOperation::Replace {
            range,
            old_text: removed,
            new_text: text.to_vec(),
        },
        "Replace".to_string(),
        AnnotationUndoHint::Snapshot,
    )
}

fn describe_char(c: Character) -> String {
    if c == Character::Newline {
        "\\n".to_string()
    } else {
        c.to_string()
    }
}
