use super::Editor;
use crate::executor::execute_command;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn execute_buffer_command(&mut self, command: crate::command::Command) -> bool {
        let current_mode = self.current_mode;
        if current_mode == Mode::Normal
            || current_mode == Mode::Insert
            || current_mode == Mode::Replace
            || current_mode.is_visual()
        {
            let viewport_height = self.render_system.viewport.visible_rows();

            let (doc_id, content_width) = {
                let doc = self.document_manager.active_document().unwrap();
                let gutter_width = if self.state.settings.show_line_numbers {
                    self.state.gutter_width
                } else {
                    0
                };
                let content_width = self
                    .split_tree
                    .focused_window()
                    .viewport
                    .visible_cols()
                    .saturating_sub(gutter_width)
                    .max(1);
                (doc.id, content_width)
            };

            let display_map = self.resolve_display_map_cached(doc_id, content_width);

            let cursor_before = self
                .document_manager
                .active_document()
                .map(|d| (d.id, d.buffer.cursor()));

            let doc = self.document_manager.active_document_mut().unwrap();
            let expand_tabs = doc.options.expand_tabs;
            let tab_width = doc.options.tab_width;
            let is_mutating = command.is_mutating();

            let _ = execute_command(
                command,
                doc,
                expand_tabs,
                tab_width,
                viewport_height,
                self.state.last_search_query.as_deref(),
                display_map.as_deref(),
            );

            // Record insert-mode mutations for dot-repeat
            if is_mutating && self.current_mode == Mode::Insert && !self.dot_repeat.is_replaying() {
                self.dot_repeat.record_insert_command(command);
            }

            // Synchronous incremental parse for mutating commands
            // Tree-sitter incremental parsing is fast (~1ms for small edits)
            if is_mutating {
                self.do_incremental_syntax_parse();

                // Refresh the display-map cache with the post-mutation revision so
                // subsequent non-mutating commands in the same frame get cache hits.
                let _ = self.resolve_display_map_cached(doc_id, content_width);
            }

            // Collect event info from doc before taking mutable borrows.
            let plugin_events = self.document_manager.active_document().map(|doc| {
                let buf = doc.id;
                let cursor_event = cursor_before.and_then(|(prev_buf, prev_cursor)| {
                    let new_cursor = doc.buffer.cursor();
                    if prev_buf != buf || prev_cursor != new_cursor {
                        let row = doc.buffer.line_index.get_line_at(new_cursor);
                        let col =
                            new_cursor.saturating_sub(doc.buffer.line_index.get_line_start(row));
                        Some((buf, row, col))
                    } else {
                        None
                    }
                });
                (buf, is_mutating, cursor_event)
            });

            if let Some((buf, mutating, cursor_event)) = plugin_events {
                if mutating {
                    self.adjust_plugin_highlights_for_edits();
                    // Defer TextChangedCoarse to the next render cycle so multiple
                    // mutations within a single frame produce only one event.
                    self.pending_text_changed = Some(buf);
                    self.lsp_notify_change();
                }

                // Defer CursorMoved to the next render cycle, same as
                // TextChangedCoarse, so several moves within a frame fire once.
                if let Some(event) = cursor_event {
                    self.pending_cursor_moved = Some(event);
                }
            }

            return true;
        }
        false
    }

    /// Dispatch a pending `TextChangedCoarse` event if one was queued since the last render.
    /// Called once per render cycle so multiple mutations within a frame fire a single event.
    pub(super) fn flush_pending_text_changed(&mut self) {
        if let Some(buf) = self.pending_text_changed.take() {
            self.update_lua_state();
            crate::perf_span!(
                "plugin_dispatch_text_changed",
                crate::perf::PerfFields::default()
            );
            self.plugin_host
                .dispatch(&crate::plugin::EditorEvent::TextChangedCoarse { buf });
            self.apply_plugin_mutations();
        }
    }

    /// Dispatch the latest pending `CursorMoved` event, so several moves within
    /// a frame (e.g. a multi-line motion) fire a single event.
    pub(super) fn flush_pending_cursor_moved(&mut self) {
        if let Some((buf, row, col)) = self.pending_cursor_moved.take() {
            self.plugin_host
                .dispatch(&crate::plugin::EditorEvent::CursorMoved { buf, row, col });
            self.apply_plugin_mutations();
        }
    }

    /// Time-budgeted incremental syntax parse for the active document; past
    /// the budget, falls back to a debounced background `SyntaxParseJob`.
    pub(super) fn do_incremental_syntax_parse(&mut self) {
        use crate::syntax::ParseOutcome;

        // 1.5ms was too tight even for a few-hundred-line file (incremental
        // reparse alone can take 1-2ms), forcing every keystroke into a flicker-visible fallback.
        const SYNC_PARSE_BUDGET: std::time::Duration = std::time::Duration::from_micros(5000);
        use super::SYNC_PARSE_MAX_BYTES;

        let Some(doc) = self.document_manager.active_document_mut() else {
            return;
        };
        if doc.syntax.is_none() {
            return;
        }
        let doc_id = doc.id;
        if doc.buffer.byte_len() > SYNC_PARSE_MAX_BYTES {
            self.debounce_syntax_reparse(doc_id);
            return;
        }
        let source = doc.buffer.to_logical_bytes();
        let outcome = doc
            .syntax
            .as_mut()
            .map(|syntax| syntax.try_incremental_parse(&source, SYNC_PARSE_BUDGET));

        match outcome {
            Some(ParseOutcome::Completed) => self.cancel_pending_syntax_reparse(doc_id),
            Some(ParseOutcome::Aborted) => self.debounce_syntax_reparse(doc_id),
            Some(ParseOutcome::NoLanguage) | None => {}
        }
    }
}
