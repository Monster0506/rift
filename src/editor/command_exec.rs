use super::Editor;
use crate::executor::execute_command;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn execute_buffer_command(&mut self, command: crate::command::Command) -> bool {
        let current_mode = self.current_mode;
        let is_mutating = command.is_mutating();

        if current_mode == Mode::Normal
            || current_mode == Mode::Insert
            || current_mode == Mode::Replace
            || current_mode.is_visual()
        {
            let viewport_height = self.render_system.viewport.visible_rows();

            let (doc_id, content_width, cursor) = self.command_content_geometry();

            let mut display_map =
                self.resolve_display_map_cached(doc_id, content_width, cursor, viewport_height);

            let doc = self
                .document_manager
                .get_document_mut(self.active_document_id())
                .unwrap();
            let expand_tabs = doc.options().expand_tabs();
            let tab_width = doc.options().tab_width();

            if execute_command(
                command,
                doc,
                expand_tabs,
                tab_width,
                viewport_height,
                self.state.last_search_query.as_deref(),
                display_map.as_mut().map(std::sync::Arc::make_mut),
            )
            .is_err()
            {
                return false;
            }

            if is_mutating && current_mode.is_visual() {
                self.set_mode(Mode::Normal);
            }
            let records_dot = match self.current_mode {
                Mode::Insert => is_mutating,
                Mode::Replace => {
                    is_mutating && !matches!(command, crate::command::Command::ReplaceBackspace(_))
                }
                _ => false,
            };
            if records_dot && !self.dot_repeat.is_replaying() {
                self.dot_repeat.record_insert_command(command);
            }

            if is_mutating {
                self.refresh_display_map_after_mutation(doc_id, content_width, viewport_height);
            }

            return true;
        }
        false
    }

    fn command_content_geometry(&self) -> (crate::document::DocumentId, usize, usize) {
        let doc = self
            .document_manager
            .get_document(self.active_document_id())
            .unwrap();
        let gutter_width = Self::line_number_gutter_width(doc);
        let content_width = self
            .split_tree
            .focused_window()
            .viewport
            .visible_cols()
            .saturating_sub(gutter_width)
            .max(1);
        (doc.id, content_width, doc.buffer().cursor())
    }

    fn refresh_display_map_after_mutation(
        &mut self,
        doc_id: crate::document::DocumentId,
        content_width: usize,
        viewport_height: usize,
    ) {
        let cursor_after = self
            .document_manager
            .get_document(self.active_document_id())
            .map_or(0, |d| d.buffer().cursor());
        let _ =
            self.resolve_display_map_cached(doc_id, content_width, cursor_after, viewport_height);
    }

    pub(crate) fn deliver_frame_events(&mut self) {
        #[cfg(any(feature = "plugins", feature = "lsp"))]
        self.deliver_document_changes();
        self.drain_syntax_sync();
        #[cfg(feature = "plugins")]
        self.dispatch_frame_events();
    }

    #[cfg(any(feature = "plugins", feature = "lsp"))]
    pub(super) fn deliver_document_changes(&mut self) {
        let drained = self.document_manager.drain_changes();
        if drained.is_empty() {
            return;
        }
        #[cfg(feature = "lsp")]
        for (buf, changes) in &drained {
            if changes.text {
                self.lsp_notify_change(*buf);
            }
        }
        #[cfg(feature = "plugins")]
        for (buf, changes) in drained {
            use crate::plugin::events::EventPayload;

            if changes.text {
                self.raise(EventPayload::TextChangedCoarse { buf });
            }
            if let Some((row, col)) = changes.cursor {
                self.raise(EventPayload::CursorMoved { buf, row, col });
            }
            if changes.selection {
                self.raise(EventPayload::SelectionChanged { buf });
            }
        }
    }
}
