use super::Editor;
use crate::executor::execute_command;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn execute_buffer_command(&mut self, command: crate::command::Command) -> bool {
        if command.is_mutating()
            && self
                .document_manager
                .active_document()
                .is_some_and(super::super::document::Document::is_read_only)
        {
            return false;
        }
        let current_mode = self.current_mode;
        let is_mutating = command.is_mutating();

        if is_mutating && current_mode.is_visual() {
            self.set_mode(Mode::Normal);
        }

        if current_mode == Mode::Normal
            || current_mode == Mode::Insert
            || current_mode == Mode::Replace
            || current_mode.is_visual()
        {
            let viewport_height = self.render_system.viewport.visible_rows();

            let (doc_id, content_width, cursor) = self.command_content_geometry();

            let mut display_map =
                self.resolve_display_map_cached(doc_id, content_width, cursor, viewport_height);

            let doc = self.document_manager.active_document_mut().unwrap();
            let expand_tabs = doc.options.expand_tabs;
            let tab_width = doc.options.tab_width;

            if is_mutating {
                doc.clear_selection();
            }

            let _ = execute_command(
                command,
                doc,
                expand_tabs,
                tab_width,
                viewport_height,
                self.state.last_search_query.as_deref(),
                display_map.as_mut().map(std::sync::Arc::make_mut),
            );

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
        let doc = self.document_manager.active_document().unwrap();
        let gutter_width = self.state.gutter_cols(doc.options.show_line_numbers);
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
            .active_document()
            .map_or(0, |d| d.buffer().cursor());
        let _ =
            self.resolve_display_map_cached(doc_id, content_width, cursor_after, viewport_height);
    }

    pub(super) fn deliver_document_changes(&mut self) {
        use crate::plugin::events::EditorEvent;

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
        self.call_plugins(|editor| {
            crate::perf_span!(
                "plugin_dispatch_buffer_changes",
                crate::perf::PerfFields::default()
            );
            for &(buf, changes) in &drained {
                if changes.opened {
                    if let Some(event) = editor.open_event(buf) {
                        editor.plugin_host.dispatch(&event);
                    }
                }
                if changes.text {
                    editor
                        .plugin_host
                        .dispatch(&EditorEvent::TextChangedCoarse { buf });
                }
                if let Some((row, col)) = changes.cursor {
                    editor
                        .plugin_host
                        .dispatch(&EditorEvent::CursorMoved { buf, row, col });
                }
                if changes.selection {
                    editor
                        .plugin_host
                        .dispatch(&EditorEvent::SelectionChanged { buf });
                }
            }
        });
    }

    pub(super) fn flush_pending_syntax_updated(&mut self) {
        if self.pending_syntax_updated.is_empty() {
            return;
        }
        self.call_plugins(|editor| {
            for buf in std::mem::take(&mut editor.pending_syntax_updated) {
                let lang = editor
                    .document_manager
                    .get_document(buf)
                    .and_then(|doc| doc.syntax.as_ref())
                    .map(|syntax| syntax.language_name.clone());
                editor
                    .plugin_host
                    .dispatch(&crate::plugin::events::EditorEvent::SyntaxUpdated { buf, lang });
            }
        });
    }

    pub(super) fn drain_syntax_sync(&mut self) {
        for (doc_id, notice) in self.document_manager.take_syntax_notices() {
            if notice.completed {
                self.cancel_pending_syntax_reparse(doc_id);
                self.pending_syntax_updated.insert(doc_id);
            }
            if notice.deferred {
                self.debounce_syntax_reparse(doc_id);
            }
        }
    }

    #[cfg(feature = "treesitter")]
    pub(super) fn reparse_syntax(&mut self, doc_id: crate::document::DocumentId) {
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.reparse_syntax();
        }
        self.drain_syntax_sync();
    }
}
