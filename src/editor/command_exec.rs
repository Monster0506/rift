use super::resolve_display_map;
use super::Editor;
use crate::executor::execute_command;
use crate::mode::Mode;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn execute_buffer_command(&mut self, command: crate::command::Command) -> bool {
        let current_mode = self.current_mode;
        if current_mode == Mode::Normal || current_mode == Mode::Insert {
            let viewport_height = self.render_system.viewport.visible_rows();

            let display_map = {
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
                resolve_display_map(
                    doc,
                    content_width,
                    self.state.settings.soft_wrap,
                    self.state.settings.wrap_width,
                )
            };

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
                display_map.as_ref(),
            );

            // Record insert-mode mutations for dot-repeat
            if is_mutating && self.current_mode == Mode::Insert && !self.dot_repeat.is_replaying() {
                self.dot_repeat.record_insert_command(command);
            }

            // Synchronous incremental parse for mutating commands
            // Tree-sitter incremental parsing is fast (~1ms for small edits)
            if is_mutating {
                self.do_incremental_syntax_parse();
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
                    self.update_lua_state();
                    self.plugin_host
                        .dispatch(&crate::plugin::EditorEvent::TextChangedCoarse { buf });
                    self.apply_plugin_mutations();
                }

                if let Some((buf, row, col)) = cursor_event {
                    self.plugin_host
                        .dispatch(&crate::plugin::EditorEvent::CursorMoved { buf, row, col });
                    self.apply_plugin_mutations();
                }
            }

            return true;
        }
        false
    }

    /// Perform synchronous incremental syntax parse for the document.
    /// This is fast because tree-sitter reuses unchanged subtrees from the old tree.
    pub(super) fn do_incremental_syntax_parse(&mut self) {
        if let Some(doc) = self.document_manager.active_document_mut() {
            if doc.syntax.is_none() {
                return;
            }

            // Get source bytes for parsing
            let source = doc.buffer.to_logical_bytes();

            if let Some(syntax) = &mut doc.syntax {
                syntax.incremental_parse(&source);
            }
        }
    }
}
