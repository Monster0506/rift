use super::Editor;
use super::PanelKind;
#[allow(unused_imports)]
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
            _ => {}
        }
    }

    pub(super) fn handle_messages_buffer_action(&mut self, id: &str) {
        if id == "messages:refresh" {
            self.refresh_messages_buffer_if_open()
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
            self.handle_clipboard_entry_close()
        }
    }

    pub(super) fn handle_clipboard_entry_close(&mut self) {
        let layout = match &self.panel_layout {
            Some(l) if l.kind == PanelKind::Clipboard => l.clone(),
            _ => return,
        };
        self.split_tree.set_focus(layout.dir_win_id);
        let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    pub fn refresh_clipboard_buffer_if_open(&mut self) {
        let layout = match &self.panel_layout {
            Some(l) if l.kind == PanelKind::Clipboard => l.clone(),
            _ => return,
        };
        let entries: std::collections::VecDeque<String> =
            self.clipboard_ring.entries().iter().cloned().collect();
        if let Some(doc) = self.document_manager.get_document_mut(layout.dir_doc_id) {
            if doc.is_clipboard() {
                let cursor = doc.buffer.cursor();
                doc.populate_clipboard_buffer(&entries);
                let len = doc.buffer.len();
                let _ = doc.buffer.set_cursor(cursor.min(len.saturating_sub(1)));
            }
        }
        if self.active_document_id() == layout.dir_doc_id {
            let _ = self.update_and_render();
        }
    }

    pub(super) fn handle_clipboard_select(&mut self) {
        use crate::document::BufferKind;

        let layout = match &self.panel_layout {
            Some(l) if l.kind == PanelKind::Clipboard => l.clone(),
            _ => return,
        };

        let (entry_text, entry_index) = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes);
            let idx = line_text
                .trim()
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))
                .and_then(|inner| inner.parse::<usize>().ok());
            match idx {
                Some(i) => match &doc.kind {
                    BufferKind::Clipboard { entries } => match entries.get(i) {
                        Some(text) => (text.clone(), i),
                        None => return,
                    },
                    _ => return,
                },
                None => return,
            }
        };

        // Populate the preview pane with the full entry text as an editable scratch buffer
        if let Some(preview) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        {
            let old_revision = preview.buffer.revision;
            if let Ok(mut new_buf) = crate::buffer::TextBuffer::new(entry_text.len().max(64)) {
                let _ = new_buf.insert_str(&entry_text);
                let _ = new_buf.set_cursor(0);
                new_buf.revision = old_revision + 1;
                preview.buffer = new_buf;
            }
            preview.custom_highlights.clear();
            preview.kind = BufferKind::ClipboardEntry {
                entry_index: Some(entry_index),
            };
            preview.history.mark_saved();
        }

        // Focus the preview pane so the user can edit
        self.split_tree.set_focus(layout.preview_win_id);
        let _ = self
            .document_manager
            .switch_to_document(layout.preview_doc_id);
        if let Some(w) = self.split_tree.windows.get_mut(&layout.preview_win_id) {
            w.document_id = layout.preview_doc_id;
        }

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    pub(super) fn handle_clipboard_new(&mut self) {
        use crate::document::BufferKind;

        let layout = match &self.panel_layout {
            Some(l) if l.kind == PanelKind::Clipboard => l.clone(),
            _ => return,
        };

        if let Some(preview) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        {
            let old_revision = preview.buffer.revision;
            if let Ok(mut new_buf) = crate::buffer::TextBuffer::new(64) {
                new_buf.revision = old_revision + 1;
                preview.buffer = new_buf;
            }
            preview.custom_highlights.clear();
            preview.kind = BufferKind::ClipboardEntry { entry_index: None };
            preview.history.mark_saved();
        }

        self.split_tree.set_focus(layout.preview_win_id);
        let _ = self
            .document_manager
            .switch_to_document(layout.preview_doc_id);
        if let Some(w) = self.split_tree.windows.get_mut(&layout.preview_win_id) {
            w.document_id = layout.preview_doc_id;
        }

        self.set_mode(crate::mode::Mode::Insert);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Save an edited clipboard entry back to the ring.
    pub(super) fn apply_clipboard_entry_save(&mut self) {
        use crate::document::BufferKind;

        let (entry_index, new_text) = {
            let doc = match self.document_manager.active_document() {
                Some(d) => d,
                None => return,
            };
            match &doc.kind {
                BufferKind::ClipboardEntry { entry_index } => {
                    (*entry_index, doc.buffer.to_string())
                }
                _ => return,
            }
        };

        match entry_index {
            None => {
                // New entry — push to front of ring
                if !new_text.is_empty() {
                    self.clipboard_ring.push(new_text);
                }
            }
            Some(idx) => {
                // Replace the entry in the ring at the given index
                let entries: Vec<String> = self.clipboard_ring.entries().iter().cloned().collect();
                self.clipboard_ring = {
                    let mut ring = crate::clipboard::ClipboardRing::new();
                    for (i, entry) in entries.iter().enumerate().rev() {
                        if i == idx {
                            ring.push(new_text.clone());
                        } else {
                            ring.push(entry.clone());
                        }
                    }
                    ring
                };
            }
        }

        // Repopulate the index buffer so it reflects the edit
        if let Some(index_doc) = self.document_manager.get_document_mut(
            self.panel_layout
                .as_ref()
                .map(|l| l.dir_doc_id)
                .unwrap_or(u64::MAX),
        ) {
            if index_doc.is_clipboard() {
                index_doc.populate_clipboard_buffer(self.clipboard_ring.entries());
            }
        }

        self.state.notify(
            crate::notification::NotificationType::Info,
            "Clipboard entry saved".to_string(),
        );
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Repopulate any open messages buffer with the current notification log.
    /// Preserves cursor position for background refreshes.
    pub(super) fn refresh_messages_buffer_if_open(&mut self) {
        let doc_id = match self.document_manager.find_messages_doc_id() {
            Some(id) => id,
            None => return,
        };

        let log = self
            .state
            .error_manager
            .notifications()
            .message_log()
            .to_vec();

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
}
