use super::super::Editor;
use crate::document::ops::CursorCause;
use crate::notification::NotificationType;
use crate::plugin::events::EventPayload;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn set_search_pattern(&mut self, pattern: Option<&str>) {
        self.state.last_find_char = None;
        if self.state.last_search_query.as_deref() == pattern {
            return;
        }
        self.state.last_search_query = pattern.map(str::to_string);
        if let Some(buf) = self.document_manager.active_document_id() {
            self.raise(EventPayload::SearchChanged { buf });
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
