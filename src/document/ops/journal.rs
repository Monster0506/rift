use super::super::Document;

#[derive(Debug, Default)]
pub(crate) struct ChangeJournal {
    recorded: Changes,
    cursor: bool,
    reported_cursor: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Changes {
    pub(crate) text: bool,
    pub(crate) cursor: Option<(usize, usize)>,
    pub(crate) selection: bool,
}

impl Document {
    pub(in crate::document) const fn record_text_change(&mut self) {
        self.journal.recorded.text = true;
        self.journal.cursor = true;
    }

    pub(in crate::document) const fn record_cursor_change(&mut self) {
        self.journal.cursor = true;
    }

    pub(in crate::document) const fn record_selection_change(&mut self) {
        self.journal.recorded.selection = true;
    }

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
}
