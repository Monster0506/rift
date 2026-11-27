use super::super::Editor;
use crate::buffer::api::BufferView;
use crate::document::ops::{CursorCause, Edit, EditSource};
use crate::error::{ErrorType, RiftError};
use crate::notification::NotificationType;
#[cfg(feature = "plugins")]
use crate::plugin::events::EventPayload;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;
use crate::time::Instant;

#[derive(Clone, Copy)]
pub(in crate::editor) enum SearchRefresh {
    Immediate,
    Debounced,
}

impl<T: TerminalBackend> Editor<T> {
    pub(in crate::editor) fn schedule_search_refresh(&mut self, refresh: SearchRefresh) {
        self.scheduler.search_refresh = self.state.last_search_query.as_ref().map(|_| {
            let now = Instant::now();
            match refresh {
                SearchRefresh::Immediate => now,
                SearchRefresh::Debounced => now + super::super::scheduler::SEARCH_REFRESH_DEBOUNCE,
            }
        });
    }

    pub(in crate::editor) fn refresh_search_highlights(&mut self) {
        let doc_id = self.active_document_id();
        let Some(query) = self.state.last_search_query.as_deref() else {
            self.scheduler.search_refresh = None;
            if self.search_highlights_synced.take().is_some()
                || !self.state.search_matches.is_empty()
            {
                if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                    doc.clear_search_annotations();
                }
                self.state.search_matches.clear();
            }
            return;
        };
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return;
        };
        let revision = doc.buffer().revision();
        if self
            .search_highlights_synced
            .as_ref()
            .is_some_and(|(id, rev, q)| *id == doc_id && *rev == revision && q == query)
        {
            self.scheduler.search_refresh = None;
            return;
        }
        match self.scheduler.search_refresh {
            None => self.schedule_search_refresh(SearchRefresh::Debounced),
            Some(deadline) if Instant::now() >= deadline => {
                self.scheduler.search_refresh = None;
                if let Ok((matches, _)) = doc.find_all_matches(query) {
                    doc.sync_search_annotations(&matches, None);
                    self.state.search_matches = matches;
                } else {
                    doc.clear_search_annotations();
                    self.state.search_matches.clear();
                }
                self.search_highlights_synced = Some((doc_id, revision, query.to_owned()));
            }
            Some(_) => {}
        }
    }

    pub(crate) fn set_search_pattern(&mut self, pattern: Option<&str>) {
        if pattern.is_some() {
            self.state.last_find_char = None;
        }
        if self.state.last_search_query.as_deref() == pattern {
            return;
        }
        self.state.last_search_query = pattern.map(str::to_string);
        #[cfg(feature = "plugins")]
        self.raise(EventPayload::SearchChanged {
            buf: self.active_document_id(),
        });
    }

    pub(crate) fn clear_search(&mut self) -> bool {
        self.set_search_pattern(None);
        self.scheduler.search_refresh = None;
        self.search_highlights_synced = None;
        self.state.search_matches.clear();
        for doc in self.document_manager.iter_documents_mut() {
            doc.clear_search_annotations();
        }
        self.state.clear_command_line();
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

        let mut changes_made = 0;
        let mut errors = Vec::new();
        let _ = document.edit_group(
            format!("Substitute: {pattern} -> {replacement}"),
            |document| {
                for m in matches.into_iter().rev() {
                    match replace_match(document, m.range, replacement) {
                        Ok(true) => changes_made += 1,
                        Ok(false) => {}
                        Err(error) => errors.push(error),
                    }
                }
                if has_selection {
                    document.clear_selection();
                }
            },
        );

        for error in errors {
            self.notify_error(error);
        }
        self.notify(
            NotificationType::Info,
            format!("{changes_made} substitutions"),
        );
        if changes_made > 0 {
            self.set_search_pattern(Some(pattern));
            self.schedule_search_refresh(SearchRefresh::Immediate);
        }
        self.state.clear_command_line();
        true
    }

    pub(crate) fn search(
        &mut self,
        query: &str,
        direction: SearchDirection,
        skip_current: bool,
    ) -> bool {
        self.set_search_pattern(Some(query));
        self.schedule_search_refresh(SearchRefresh::Immediate);

        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
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
    document: &mut crate::document::ops::DocumentWrite<'_>,
    range: std::ops::Range<usize>,
    replacement: &str,
) -> Result<bool, RiftError> {
    let start = range.start;
    let deleted = document.edit(Edit::delete(range), EditSource::User)?;
    let inserted = document.edit(Edit::insert(start, replacement), EditSource::User)?;
    Ok(deleted || inserted)
}
