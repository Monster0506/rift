use super::super::Editor;
use crate::buffer::api::BufferView;
use crate::document::ops::{CursorCause, Edit, EditSource};
use crate::document::Document;
use crate::error::{ErrorType, RiftError};
use crate::notification::NotificationType;
use crate::plugin::events::EventPayload;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn set_search_pattern(&mut self, pattern: Option<&str>) {
        if pattern.is_some() {
            self.state.last_find_char = None;
        }
        if self.state.last_search_query.as_deref() == pattern {
            return;
        }
        self.state.last_search_query = pattern.map(str::to_string);
        if let Some(buf) = self.document_manager.active_document_id() {
            self.raise(EventPayload::SearchChanged { buf });
        }
    }

    pub(crate) fn clear_search(&mut self) -> bool {
        self.set_search_pattern(None);
        self.scheduler.search_refresh.cancel(());
        self.search_highlights_synced = None;
        self.state.search_matches.clear();
        for doc in self.document_manager.iter_documents_mut() {
            doc.clear_search_annotations();
        }
        self.state.clear_command_line();
        if let Err(error) = self.force_full_redraw() {
            self.notify_error(error);
        }
        true
    }

    pub(in crate::editor) fn substitute(
        &mut self,
        pattern: &str,
        replacement: &str,
        global: bool,
        whole_file: bool,
    ) -> bool {
        let document = self.active_document();
        let mut matches = match document.find_all_matches(pattern) {
            Ok((matches, _)) => matches,
            Err(error) => {
                self.notify_error(error);
                return false;
            }
        };
        let buffer = document.buffer();
        let selections = document.selections();
        let has_selection = !selections.is_empty();
        if has_selection {
            let spans: Vec<_> = selections
                .regions
                .iter()
                .map(|region| region.buffer_span(buffer))
                .collect();
            matches.retain(|m| {
                spans
                    .iter()
                    .any(|&(start, end)| m.range.start >= start && m.range.end <= end)
            });
        } else if !whole_file {
            let line = buffer.line_index.get_line_at(buffer.cursor());
            let start = buffer.line_start(line);
            let end = buffer
                .line_index
                .get_end(line, buffer.len())
                .unwrap_or(buffer.len());
            matches.retain(|m| m.range.start >= start && m.range.end <= end);
        }
        if matches.is_empty() {
            self.notify_error(RiftError::new(
                ErrorType::Execution,
                "PATTERN_NOT_FOUND",
                format!("Pattern not found: {pattern}"),
            ));
            return false;
        }
        if !global {
            let mut last_line = None;
            matches.retain(|m| {
                let line = Some(buffer.line_index.get_line_at(m.range.start));
                let first_on_line = line != last_line;
                last_line = line;
                first_on_line
            });
        }

        document.begin_transaction(format!("Substitute: {pattern} -> {replacement}"));
        let mut changes_made = 0;
        let mut errors = Vec::new();
        for m in matches.into_iter().rev() {
            match replace_match(document, m.range, replacement) {
                Ok(true) => changes_made += 1,
                Ok(false) => {}
                Err(error) => errors.push(error),
            }
        }
        document.commit_transaction();
        if has_selection {
            document.clear_selection();
        }

        for error in errors {
            self.notify_error(error);
        }
        self.notify(
            NotificationType::Info,
            format!("{changes_made} substitutions"),
        );
        if changes_made > 0 {
            self.set_search_pattern(Some(pattern));
            self.update_search_highlights();
        }
        self.redraw_from_command();
        true
    }

    pub(crate) fn search(
        &mut self,
        query: &str,
        direction: SearchDirection,
        skip_current: bool,
    ) -> bool {
        self.set_search_pattern(Some(query));
        self.update_search_highlights();
        let _ = self.force_full_redraw();

        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        match doc.perform_search(query, direction, skip_current) {
            Ok((Some(m), _)) => {
                let _ = doc.set_cursor(m.range.start, CursorCause::Jump);
                true
            }
            Ok((None, _)) => false,
            Err(e) => {
                self.notify(NotificationType::Error, format!("Search error: {e}"));
                false
            }
        }
    }
}

fn replace_match(
    document: &mut Document,
    range: std::ops::Range<usize>,
    replacement: &str,
) -> Result<bool, RiftError> {
    let start = range.start;
    let deleted = document.edit(Edit::delete(range), EditSource::User)?;
    let inserted = document.edit(Edit::insert(start, replacement), EditSource::User)?;
    Ok(deleted || inserted)
}
