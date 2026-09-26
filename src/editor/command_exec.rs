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
                .is_some_and(|d| d.is_read_only())
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

            let (doc_id, content_width, cursor) = {
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
                (doc.id, content_width, doc.buffer.cursor())
            };

            let mut display_map =
                self.resolve_display_map_cached(doc_id, content_width, cursor, viewport_height);

            let cursor_before = self
                .document_manager
                .active_document()
                .map(|d| (d.id, d.buffer.cursor()));

            let doc = self.document_manager.active_document_mut().unwrap();
            let expand_tabs = doc.options.expand_tabs;
            let tab_width = doc.options.tab_width;
            let rests_on_last_char = current_mode == Mode::Normal
                && matches!(
                    command,
                    crate::command::Command::Move(crate::action::Motion::EndOfLine, _)
                );

            if is_mutating {
                doc.selection_set.clear();
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

            if rests_on_last_char {
                let cursor = doc.buffer.cursor();
                let on_newline =
                    doc.buffer.char_at(cursor) == Some(crate::character::Character::Newline);
                let line_start = doc.buffer.line_index.get_line_start(doc.buffer.get_line());
                if on_newline && cursor > line_start {
                    let _ = doc.buffer.set_cursor(cursor - 1);
                }
            }

            if is_mutating && self.current_mode == Mode::Insert && !self.dot_repeat.is_replaying() {
                self.dot_repeat.record_insert_command(command);
            }

            if is_mutating {
                self.do_incremental_syntax_parse();

                let cursor_after = self
                    .document_manager
                    .active_document()
                    .map(|d| d.buffer.cursor())
                    .unwrap_or(0);
                let _ = self.resolve_display_map_cached(
                    doc_id,
                    content_width,
                    cursor_after,
                    viewport_height,
                );
            }

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
                    self.pending_text_changed = Some(buf);
                    #[cfg(feature = "lsp")]
                    self.lsp_notify_change(buf);
                }

                if let Some(event) = cursor_event {
                    self.pending_cursor_moved = Some(event);
                }
            }

            return true;
        }
        false
    }

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

    pub(super) fn flush_pending_cursor_moved(&mut self) {
        if let Some((buf, row, col)) = self.pending_cursor_moved.take() {
            self.plugin_host
                .dispatch(&crate::plugin::EditorEvent::CursorMoved { buf, row, col });
            self.apply_plugin_mutations();
        }
    }

    pub(super) fn do_incremental_syntax_parse(&mut self) {
        use crate::syntax::ParseOutcome;

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
