use super::super::Editor;
use crate::document::ops::CursorCause;
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

    pub(crate) fn finish_substitution(&mut self, pattern: &str) {
        self.set_search_pattern(Some(pattern));
        self.update_search_highlights();
        self.state.clear_command_line();
        if let Err(error) = self.force_full_redraw() {
            self.notify_error(error);
        }
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
