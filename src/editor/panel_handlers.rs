use super::Editor;
use super::PanelKind;
use crate::action::table::{self, KindActionSpec, KindOp};
use crate::buffer::api::BufferView;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn run_kind_action(&mut self, actions: &[KindActionSpec], id: &str) {
        if let Some(op) = table::kind_op(actions, id) {
            self.run_kind_op(op);
        }
    }

    fn run_kind_op(&mut self, op: KindOp) {
        match op {
            KindOp::ClosePanel => self.close_split_panel(),
            KindOp::ExplorerSelect => self.handle_explorer_select(),
            KindOp::ExplorerParent => self.handle_explorer_parent(),
            KindOp::ExplorerRefresh => self.handle_explorer_refresh(),
            KindOp::UndoTreeSelect => self.handle_undotree_select(),
            KindOp::UndoTreeRefresh => self.handle_undotree_refresh(),
            KindOp::UndoTreeNext => self.handle_undotree_move(1),
            KindOp::UndoTreePrev => self.handle_undotree_move(-1),
            KindOp::MessagesRefresh => self.refresh_messages_buffer_if_open(),
            KindOp::ClipboardSelect => self.handle_clipboard_select(),
            KindOp::ClipboardNew => self.handle_clipboard_new(),
            KindOp::ClipboardRefresh => self.refresh_clipboard_buffer_if_open(),
            KindOp::ClipboardEntryClose => self.handle_clipboard_entry_close(),
            #[cfg(feature = "lsp")]
            KindOp::LocationListSelect => self.handle_location_list_select(),
            #[cfg(feature = "lsp")]
            KindOp::LocationListCodeAction => self.handle_location_list_code_action(),
            #[cfg(not(feature = "lsp"))]
            KindOp::LocationListSelect | KindOp::LocationListCodeAction => {}
            KindOp::BufferListSelect => self.handle_buffer_list_select(),
            KindOp::GitStatusSelect => self.git_status_select(),
            KindOp::GitStatusStage => self.git_status_cursor_action("stage"),
            KindOp::GitStatusUnstage => self.git_status_cursor_action("unstage"),
            KindOp::GitStatusToggleStage => self.git_status_cursor_action("toggle"),
            KindOp::GitStatusDiscard => self.git_status_cursor_action("discard"),
            KindOp::GitStatusToggleExpand => self.git_status_toggle_expand(),
            KindOp::GitStatusNextHunk => self.git_status_navigate_hunk(true),
            KindOp::GitStatusPrevHunk => self.git_status_navigate_hunk(false),
            KindOp::GitStatusBlame => self.git_status_blame_cursor(),
            KindOp::GitStatusRebase => self.git_status_rebase_cursor(),
            KindOp::GitBlameWalkBack => self.git_blame_walk_back(),
            KindOp::GitBlameWalkForward => self.git_blame_walk_forward(),
            KindOp::GitBlameClose => self.close_git_blame(),
            KindOp::GitLogToggleExpand => self.git_log_toggle_expand(),
        }
    }

    pub(super) fn handle_clipboard_entry_close(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::Clipboard) else {
            return;
        };
        self.focus_window_showing(layout.dir_win_id, layout.dir_doc_id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    pub(super) fn refresh_clipboard_buffer_if_open(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::Clipboard) else {
            return;
        };
        if self
            .document_manager
            .get_document(layout.dir_doc_id)
            .is_some_and(crate::document::Document::is_dirty)
        {
            self.notify(
                crate::notification::NotificationType::Error,
                "Clipboard index has unsaved edits; save it before refreshing".to_string(),
            );
            return;
        }
        if !self.clipboard_entry_can_be_replaced(layout.preview_doc_id) {
            return;
        }
        let entries: std::collections::VecDeque<crate::clipboard::Clip> =
            self.clipboard_ring.entries().clone();
        if let Some(doc) = self.document_manager.get_document_mut(layout.dir_doc_id) {
            let cursor = doc.buffer().cursor();
            doc.populate_clipboard_buffer(&entries);
            let len = doc.buffer().len();
            let _ = doc.set_cursor(
                cursor.min(len.saturating_sub(1)),
                crate::document::ops::CursorCause::User,
            );
        }
        if self.active_document_id() == layout.dir_doc_id {
            let _ = self.update_and_render();
        }
    }

    fn handle_clipboard_select(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::Clipboard) else {
            return;
        };
        if !self.clipboard_entry_can_be_replaced(layout.preview_doc_id) {
            return;
        }

        let (entry_text, entry_index) = {
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let cursor = doc.buffer().cursor();
            let line_num = doc.buffer().line_index.get_line_at(cursor);
            let Some(index) = doc.clipboard_source_at(line_num) else {
                return;
            };
            let Some(text) = doc.clipboard_entry(index) else {
                return;
            };
            (text.to_vec(), index)
        };

        let Some(preview) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        else {
            return;
        };
        preview.replace_content(&entry_text, crate::document::ops::ContentKind::Load);
        preview.convert_to_clipboard_entry(Some(entry_index));
        preview.set_read_only(false);

        // Focus the preview pane so the user can edit
        self.focus_window_showing(layout.preview_win_id, layout.preview_doc_id);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    fn handle_clipboard_new(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::Clipboard) else {
            return;
        };

        if !self.clipboard_entry_can_be_replaced(layout.preview_doc_id) {
            return;
        }
        let Some(preview) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        else {
            return;
        };
        preview.replace_content("", crate::document::ops::ContentKind::Load);
        preview.convert_to_clipboard_entry(None);
        preview.set_read_only(false);

        self.focus_window_showing(layout.preview_win_id, layout.preview_doc_id);

        self.set_mode(crate::mode::Mode::Insert);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    fn clipboard_entry_can_be_replaced(&mut self, doc_id: crate::document::DocumentId) -> bool {
        if self
            .document_manager
            .get_document(doc_id)
            .is_some_and(crate::document::Document::is_dirty)
        {
            self.notify(
                crate::notification::NotificationType::Error,
                "Clipboard entry has unsaved edits; save it before replacing it".to_string(),
            );
            return false;
        }
        true
    }

    /// Save an edited clipboard entry back to the ring.
    pub(super) fn apply_clipboard_entry_save(&mut self) -> crate::document::runtime::SaveResult {
        use crate::document::runtime::SaveResult;

        let Some(layout) = self.panel_layout_of(PanelKind::Clipboard) else {
            return SaveResult::Failed;
        };
        let (entry_index, new_text) = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return SaveResult::Failed;
            };
            doc.commit_pending_ghost();
            let Some(entry_index) = doc.clipboard_entry_index() else {
                return SaveResult::Failed;
            };
            let len = doc.buffer().len();
            let text = doc.buffer().chars(0..len).collect::<Vec<_>>();
            (entry_index, crate::clipboard::Clip::from_plain(text))
        };

        let Some(index_doc) = self.document_manager.get_document(layout.dir_doc_id) else {
            return SaveResult::Failed;
        };
        let Some(snapshot) = index_doc.clipboard_entries() else {
            return SaveResult::Failed;
        };
        if !snapshot.iter().eq(self.clipboard_ring.entries().iter()) {
            self.notify(
                crate::notification::NotificationType::Error,
                "Clipboard changed since this index was opened; entry was not saved".to_string(),
            );
            return SaveResult::Failed;
        }
        let index_dirty = index_doc.is_dirty();
        if index_dirty && (entry_index.is_none() || new_text.text().is_empty()) {
            self.notify(
                crate::notification::NotificationType::Error,
                "Save clipboard index edits before adding or deleting an entry".to_string(),
            );
            return SaveResult::Failed;
        }
        if entry_index.is_some_and(|index| index >= snapshot.len()) {
            self.notify(
                crate::notification::NotificationType::Error,
                "Clipboard entry no longer exists; entry was not saved".to_string(),
            );
            return SaveResult::Failed;
        }

        let saved_index = if new_text.text().is_empty() {
            None
        } else {
            Some(entry_index.unwrap_or(0))
        };
        let mut ring = crate::clipboard::ClipboardRing::new();
        ring.set_capacity(self.clipboard_ring.capacity());
        for (index, entry) in self.clipboard_ring.entries().iter().enumerate().rev() {
            if entry_index == Some(index) {
                ring.push(new_text.clone());
            } else {
                ring.push(entry.clone());
            }
        }
        if entry_index.is_none() {
            ring.push(new_text.clone());
        }

        let Some(index_doc) = self.document_manager.get_document_mut(layout.dir_doc_id) else {
            return SaveResult::Failed;
        };
        if index_dirty {
            let Some(index) = entry_index else {
                return SaveResult::Failed;
            };
            if !index_doc.set_clipboard_entry(index, new_text) {
                return SaveResult::Failed;
            }
        } else {
            let cursor = index_doc.buffer().cursor();
            index_doc.populate_clipboard_buffer(ring.entries());
            let len = index_doc.buffer().len();
            let _ = index_doc.set_cursor(
                cursor.min(len.saturating_sub(1)),
                crate::document::ops::CursorCause::User,
            );
        }
        self.clipboard_ring = ring;
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.convert_to_clipboard_entry(saved_index);
            doc.history.mark_saved();
        }

        self.notify(
            crate::notification::NotificationType::Info,
            "Clipboard entry saved".to_string(),
        );
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        SaveResult::Saved
    }

    /// Repopulate any open messages buffer with the current notification log.
    /// Preserves cursor position for background refreshes.
    pub(super) fn refresh_messages_buffer_if_open(&mut self) {
        let Some(doc_id) = self.document_manager.find_messages_doc_id() else {
            return;
        };

        let log = self.state.notifications.message_log();

        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            let cursor = doc.buffer().cursor();
            doc.populate_messages_buffer(&log);
            // Preserve cursor position on background refresh
            let len = doc.buffer().len();
            let _ = doc.set_cursor(
                cursor.min(len.saturating_sub(1)),
                crate::document::ops::CursorCause::User,
            );
        }

        // Only re-render if the messages buffer is currently visible
        if self.active_document_id() == doc_id {
            let _ = self.update_and_render();
        }
    }

    #[cfg(feature = "lsp")]
    fn handle_location_list_select(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::LocationList) else {
            return;
        };

        let entry = {
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let cursor = doc.buffer().cursor();
            let line_num = doc.buffer().line_index.get_line_at(cursor);
            doc.location_list_entry_at(line_num)
        };

        let Some(entry) = entry else { return };

        // Empty URI signals a code action row: apply it.
        if entry.uri.is_empty() {
            let idx = entry.line as usize;
            let action = self.scheduler.code_actions.get(idx).cloned();
            self.close_split_panel();
            if let Some(action) = action {
                self.execute_code_action(action);
            }
            return;
        }

        let Some(path) = crate::lsp::protocol::uri_to_path(&entry.uri) else {
            return;
        };

        self.close_split_panel();
        self.jump_to_location(&path, entry.line as usize, entry.col);
    }

    /// Space on a diagnostic entry: send a code action request scoped to that diagnostic.
    #[cfg(feature = "lsp")]
    fn handle_location_list_code_action(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::LocationList) else {
            return;
        };

        let entry = {
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let cursor = doc.buffer().cursor();
            let line_num = doc.buffer().line_index.get_line_at(cursor);
            doc.location_list_entry_at(line_num)
        };

        let Some(entry) = entry else { return };

        // Space applies a selected code action.
        if entry.uri.is_empty() {
            let idx = entry.line as usize;
            let action = self.scheduler.code_actions.get(idx).cloned();
            self.close_split_panel();
            if let Some(action) = action {
                self.execute_code_action(action);
            }
            return;
        }

        let Some(path) = crate::lsp::protocol::uri_to_path(&entry.uri) else {
            return;
        };

        if self.lsp_manager.is_indexing_path(&path) {
            self.notify(
                crate::notification::NotificationType::Warning,
                "LSP: still indexing, please wait...".to_string(),
            );
            return;
        }

        // Collect the matching diagnostics for richer context.
        let norm_uri = crate::lsp::protocol::normalize_uri(&entry.uri);
        let diagnostics: Vec<crate::lsp::protocol::LspDiagnostic> =
            self.lsp_diagnostics.get(&norm_uri).map_or_default(|diags| {
                diags
                    .iter()
                    .filter(|d| d.range.start.line == entry.line)
                    .cloned()
                    .collect()
            });

        if self
            .lsp_manager
            .code_action(&path, entry.line, entry.col, diagnostics)
            .is_none()
        {
            self.notify(
                crate::notification::NotificationType::Warning,
                "LSP: no server for this file".to_string(),
            );
        }
    }
}
