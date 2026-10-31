use super::Editor;
use super::PanelKind;
use crate::buffer::api::BufferView;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_directory_buffer_action(&mut self, id: &str) {
        match id {
            "explorer:select" => self.handle_explorer_select(),
            "explorer:parent" => self.handle_explorer_parent(),
            "explorer:close" => self.close_split_panel(),
            "explorer:refresh" => self.handle_explorer_refresh(),
            _ => {}
        }
    }

    pub(super) fn handle_undotree_buffer_action(&mut self, id: &str) {
        match id {
            "undotree:select" => self.handle_undotree_select(),
            "undotree:close" => self.close_split_panel(),
            "undotree:refresh" => self.handle_undotree_refresh(),
            "undotree:next" => self.handle_undotree_move(1),
            "undotree:prev" => self.handle_undotree_move(-1),
            _ => {}
        }
    }

    pub(super) fn handle_messages_buffer_action(&mut self, id: &str) {
        if id == "messages:refresh" {
            self.refresh_messages_buffer_if_open();
        }
    }

    pub(super) fn handle_clipboard_buffer_action(&mut self, id: &str) {
        match id {
            "clipboard:select" => self.handle_clipboard_select(),
            "clipboard:new" => self.handle_clipboard_new(),
            "clipboard:refresh" => self.refresh_clipboard_buffer_if_open(),
            "clipboard:close" => self.close_split_panel(),
            _ => {}
        }
    }

    pub(super) fn handle_clipboard_entry_action(&mut self, id: &str) {
        if id == "clipboard:entry:close" {
            self.handle_clipboard_entry_close();
        }
    }

    pub(super) fn handle_clipboard_entry_close(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::Clipboard) else {
            return;
        };
        self.split_tree.set_focus(layout.dir_win_id);
        let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
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
            self.state.notify(
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
            let cursor = doc.buffer.cursor();
            doc.populate_clipboard_buffer(&entries);
            let len = doc.buffer.len();
            let _ = doc.buffer.set_cursor(cursor.min(len.saturating_sub(1)));
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
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let Some(index) = doc.clipboard_source_at(line_num) else {
                return;
            };
            let Some(text) = doc.clipboard_entry(index) else {
                return;
            };
            (text.to_vec(), index)
        };

        let mut new_buf = match crate::buffer::TextBuffer::new(entry_text.len().max(64)) {
            Ok(buffer) => buffer,
            Err(error) => {
                self.state.handle_error(error);
                return;
            }
        };
        new_buf.insert_chars(&entry_text);
        let _ = new_buf.set_cursor(0);
        let Some(preview) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        else {
            return;
        };
        new_buf.revision = preview.buffer.revision + 1;
        preview.buffer = new_buf;
        preview.custom_highlights.clear();
        preview.selection_set.clear();
        preview.convert_to_clipboard_entry(Some(entry_index));
        preview.set_read_only(false);
        preview.history = crate::history::UndoTree::new();

        // Focus the preview pane so the user can edit
        self.split_tree.set_focus(layout.preview_win_id);
        let _ = self
            .document_manager
            .switch_to_document(layout.preview_doc_id);
        self.split_tree
            .set_window_document(layout.preview_win_id, layout.preview_doc_id);

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
        let mut new_buf = match crate::buffer::TextBuffer::new(64) {
            Ok(buffer) => buffer,
            Err(error) => {
                self.state.handle_error(error);
                return;
            }
        };
        let Some(preview) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        else {
            return;
        };
        new_buf.revision = preview.buffer.revision + 1;
        preview.buffer = new_buf;
        preview.custom_highlights.clear();
        preview.selection_set.clear();
        preview.convert_to_clipboard_entry(None);
        preview.set_read_only(false);
        preview.history = crate::history::UndoTree::new();

        self.split_tree.set_focus(layout.preview_win_id);
        let _ = self
            .document_manager
            .switch_to_document(layout.preview_doc_id);
        self.split_tree
            .set_window_document(layout.preview_win_id, layout.preview_doc_id);

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
            self.state.notify(
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
            let len = doc.buffer.len();
            let text = doc.buffer.chars(0..len).collect::<Vec<_>>();
            (entry_index, crate::clipboard::Clip::from_plain(text))
        };

        let Some(index_doc) = self.document_manager.get_document(layout.dir_doc_id) else {
            return SaveResult::Failed;
        };
        let Some(snapshot) = index_doc.clipboard_entries() else {
            return SaveResult::Failed;
        };
        if !snapshot.iter().eq(self.clipboard_ring.entries().iter()) {
            self.state.notify(
                crate::notification::NotificationType::Error,
                "Clipboard changed since this index was opened; entry was not saved".to_string(),
            );
            return SaveResult::Failed;
        }
        let index_dirty = index_doc.is_dirty();
        if index_dirty && (entry_index.is_none() || new_text.text().is_empty()) {
            self.state.notify(
                crate::notification::NotificationType::Error,
                "Save clipboard index edits before adding or deleting an entry".to_string(),
            );
            return SaveResult::Failed;
        }
        if entry_index.is_some_and(|index| index >= snapshot.len()) {
            self.state.notify(
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
            let cursor = index_doc.buffer.cursor();
            index_doc.populate_clipboard_buffer(ring.entries());
            let len = index_doc.buffer.len();
            let _ = index_doc
                .buffer
                .set_cursor(cursor.min(len.saturating_sub(1)));
        }
        self.clipboard_ring = ring;
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.convert_to_clipboard_entry(saved_index);
            doc.history.mark_saved();
        }

        self.state.notify(
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

        let log = self.state.error_manager.notifications().message_log();

        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            let cursor = doc.buffer.cursor();
            doc.populate_messages_buffer(&log);
            // Preserve cursor position on background refresh
            let len = doc.buffer.len();
            let _ = doc.buffer.set_cursor(cursor.min(len.saturating_sub(1)));
        }

        // Only re-render if the messages buffer is currently visible
        if self.active_document_id() == doc_id {
            let _ = self.update_and_render();
        }
    }

    pub(super) fn handle_location_list_action(&mut self, id: &str) {
        match id {
            #[cfg(feature = "lsp")]
            "location_list:select" => self.handle_location_list_select(),
            #[cfg(feature = "lsp")]
            "location_list:code_action" => self.handle_location_list_code_action(),
            "location_list:close" => self.close_split_panel(),
            _ => {}
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
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            doc.location_list_entry_at(line_num)
        };

        let Some(entry) = entry else { return };

        // Empty URI signals a code action row: apply it.
        if entry.uri.is_empty() {
            let idx = entry.line as usize;
            let action = self.pending_code_actions.get(idx).cloned();
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

    pub(super) fn handle_regions_buffer_action(&mut self, id: &str) {
        if id == "regions:close" {
            self.close_split_panel();
        }
    }

    pub(super) fn handle_buffer_list_action(&mut self, id: &str) {
        match id {
            "buffer_list:select" => self.handle_buffer_list_select(),
            "buffer_list:close" => self.close_split_panel(),
            _ => {}
        }
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
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            doc.location_list_entry_at(line_num)
        };

        let Some(entry) = entry else { return };

        // Space applies a selected code action.
        if entry.uri.is_empty() {
            let idx = entry.line as usize;
            let action = self.pending_code_actions.get(idx).cloned();
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
            self.state.notify(
                crate::notification::NotificationType::Warning,
                "LSP: still indexing, please wait...".to_string(),
            );
            return;
        }

        // Collect the matching diagnostics for richer context.
        let norm_uri = crate::lsp::protocol::normalize_uri(&entry.uri);
        let diagnostics: Vec<crate::lsp::protocol::LspDiagnostic> = self
            .lsp_diagnostics
            .get(&norm_uri)
            .map(|diags| {
                diags
                    .iter()
                    .filter(|d| d.range.start.line == entry.line)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        if self
            .lsp_manager
            .code_action(&path, entry.line, entry.col, diagnostics)
            .is_none()
        {
            self.state.notify(
                crate::notification::NotificationType::Warning,
                "LSP: no server for this file".to_string(),
            );
        }
    }
}
