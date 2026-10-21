use super::Editor;
use crate::action::Motion;
use crate::character::Character;
use crate::command::Command;
use crate::term::TerminalBackend;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DotRecord {
    Recorded,
    Skipped,
}

pub(super) struct ReplaceEntry {
    pos: usize,
    original: Option<Character>,
    dot: DotRecord,
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn replace_type(&mut self, ch: char) {
        if ch == '\n' {
            self.replace_line_break();
            return;
        }

        let (fill, count) = self.replace_expand(ch);
        for _ in 0..count {
            self.replace_one(fill);
        }
    }

    pub(super) fn replace_paste(&mut self, text: &str) {
        let Some(doc) = self.document_manager.active_document_mut() else {
            return;
        };
        if doc.is_read_only() {
            return;
        }

        let mut entries = Vec::new();
        doc.begin_transaction("Paste");
        for ch in text.chars() {
            let pos = doc.buffer.cursor();
            let (original, result) = if ch == '\n' {
                (None, doc.insert_char(ch))
            } else {
                (doc.char_to_overwrite(), doc.overwrite_char(ch))
            };
            if result.is_err() {
                break;
            }
            entries.push(ReplaceEntry {
                pos,
                original,
                dot: DotRecord::Skipped,
            });
        }
        doc.commit_transaction();

        self.replace_originals.extend(entries);
        self.do_incremental_syntax_parse();
    }

    pub(super) fn replace_backspace(&mut self) {
        let Some(cursor) = self
            .document_manager
            .active_document()
            .map(|doc| doc.buffer.cursor())
        else {
            return;
        };
        let Some(entry) = self.replace_pop(cursor) else {
            if let Some(doc) = self.document_manager.active_document_mut() {
                doc.buffer.move_left();
            }
            return;
        };

        if entry.dot == DotRecord::Recorded && !self.dot_repeat.is_replaying() {
            self.dot_repeat.pop_insert_command();
        }
        self.execute_buffer_command(Command::ReplaceBackspace(entry.original));
    }

    pub(super) fn replace_delete_word(&mut self) {
        let Some(start) = self
            .document_manager
            .active_document()
            .map(|doc| doc.buffer.cursor())
        else {
            return;
        };

        self.execute_buffer_command(Command::Move(Motion::PreviousWord, 1));
        let Some(doc) = self.document_manager.active_document_mut() else {
            return;
        };
        let target = doc.buffer.cursor();
        let _ = doc.buffer.set_cursor(start);

        while self.replace_cursor().is_some_and(|c| c > target) {
            let before = self.replace_cursor();
            self.replace_backspace();
            if self.replace_cursor() >= before {
                break;
            }
        }
    }

    fn replace_cursor(&self) -> Option<usize> {
        self.document_manager
            .active_document()
            .map(|doc| doc.buffer.cursor())
    }

    fn replace_one(&mut self, ch: char) {
        let Some(doc) = self.document_manager.active_document() else {
            return;
        };
        let pos = doc.buffer.cursor();
        let original = doc.char_to_overwrite();

        if self.execute_buffer_command(Command::OverwriteChar(ch)) {
            self.replace_push(pos, original);
        }
    }

    fn replace_line_break(&mut self) {
        let Some(pos) = self.replace_cursor() else {
            return;
        };

        if self.execute_buffer_command(Command::InsertChar('\n')) {
            self.replace_push(pos, None);
        }
    }

    fn replace_expand(&self, ch: char) -> (char, usize) {
        let Some(doc) = self.document_manager.active_document() else {
            return (ch, 1);
        };
        if ch != '\t' || !doc.options.expand_tabs {
            return (ch, 1);
        }
        let width = crate::executor::tab_fill_width(&doc.buffer, doc.options.tab_width);
        (' ', width)
    }

    fn replace_push(&mut self, pos: usize, original: Option<Character>) {
        self.replace_originals.push(ReplaceEntry {
            pos,
            original,
            dot: DotRecord::Recorded,
        });
    }

    fn replace_pop(&mut self, cursor: usize) -> Option<ReplaceEntry> {
        let top = self.replace_originals.last()?;
        if top.pos + 1 == cursor {
            return self.replace_originals.pop();
        }

        self.replace_originals.clear();
        None
    }
}
