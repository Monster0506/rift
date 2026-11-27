use super::super::Document;

#[cfg(any(feature = "plugins", feature = "lsp"))]
#[derive(Debug, Default)]
pub(crate) struct ChangeJournal {
    #[cfg(any(feature = "plugins", feature = "lsp"))]
    recorded: Changes,
    #[cfg(feature = "plugins")]
    cursor: bool,
    #[cfg(feature = "plugins")]
    reported_cursor: usize,
}

#[cfg(any(feature = "plugins", feature = "lsp"))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Changes {
    pub(crate) text: bool,
    #[cfg(feature = "plugins")]
    pub(crate) cursor: Option<(usize, usize)>,
    #[cfg(feature = "plugins")]
    pub(crate) selection: bool,
}

impl Document {
    pub(in crate::document) const fn record_text_change(&mut self) {
        #[cfg(any(feature = "plugins", feature = "lsp"))]
        {
            self.journal.recorded.text = true;
        }
        #[cfg(feature = "plugins")]
        {
            self.journal.cursor = true;
        }
    }

    pub(in crate::document) const fn record_cursor_change(&mut self) {
        #[cfg(feature = "plugins")]
        {
            self.journal.cursor = true;
        }
    }

    pub(in crate::document) const fn record_selection_change(&mut self) {
        #[cfg(feature = "plugins")]
        {
            self.journal.recorded.selection = true;
        }
    }

    #[cfg(feature = "plugins")]
    pub(crate) fn drain_changes(&mut self) -> Changes {
        let cursor = self.buffer.cursor();
        let moved = self.journal.cursor && cursor != self.journal.reported_cursor;
        let changes = Changes {
            cursor: moved.then(|| {
                let row = self.buffer.line_index.get_line_at(cursor);
                (row, cursor - self.buffer.line_index.get_line_start(row))
            }),
            ..self.journal.recorded
        };
        self.journal = ChangeJournal {
            reported_cursor: cursor,
            ..ChangeJournal::default()
        };
        changes
    }

    #[cfg(all(feature = "lsp", not(feature = "plugins")))]
    pub(crate) fn drain_changes(&mut self) -> Changes {
        std::mem::take(&mut self.journal).recorded
    }
}
