use super::Editor;
use super::{PanelKind, PanelLayout};
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn reload_directory_buffer(
        &mut self,
        doc_id: crate::document::DocumentId,
        new_path: std::path::PathBuf,
    ) {
        let show_hidden = self
            .document_manager
            .get_document(doc_id)
            .and_then(|d| {
                if let crate::document::BufferKind::Directory { show_hidden, .. } = &d.kind {
                    Some(*show_hidden)
                } else {
                    None
                }
            })
            .unwrap_or(false);
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.kind = crate::document::BufferKind::Directory {
                path: new_path.clone(),
                entries: vec![],
                show_hidden,
            };
            let _ = doc.buffer.set_cursor(0);
            let len = doc.buffer.len();
            for _ in 0..len {
                doc.buffer.delete_forward();
            }
            let _ = doc.buffer.insert_str("Loading...");
        }
        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            doc_id as usize,
            new_path,
            show_hidden,
        );
        self.job_manager.spawn(job);
    }

    /// Handle <CR> in a directory (file explorer) buffer.
    pub(super) fn handle_explorer_select(&mut self) {
        use crate::document::BufferKind;

        // If we're in the explorer center pane, delegate to split-aware select.
        let is_explorer_dir = self
            .panel_layout
            .as_ref()
            .map(|l| {
                l.kind == PanelKind::FileExplorer
                    && self.split_tree.focused_window_id() == l.dir_win_id
            })
            .unwrap_or(false);
        if is_explorer_dir {
            self.handle_explorer_split_select();
            return;
        }

        let (doc_id, line_text, dir_path) = {
            let doc = match self.document_manager.active_document() {
                Some(d) if d.is_directory() => d,
                _ => return,
            };
            if doc.is_dirty() {
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    "Unsaved changes — write with :w first".to_string(),
                );
                return;
            }
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
            let dir_path = match &doc.kind {
                BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            };
            (doc.id, line_text, dir_path)
        };

        if line_text == "../" {
            if let Some(parent) = dir_path.parent().map(|p| p.to_path_buf()) {
                self.reload_directory_buffer(doc_id, parent);
            }
            return;
        }

        let entry_name = line_text.trim_end_matches('/').to_string();
        if entry_name.is_empty() {
            return;
        }

        let target_path = dir_path.join(&entry_name);

        if target_path.is_dir() {
            self.reload_directory_buffer(doc_id, target_path);
        } else if let Err(e) = self.open_file(Some(target_path.display().to_string()), false) {
            self.state.handle_error(e);
        } else {
            self.state.clear_command_line();
            if let Err(e) = self.force_full_redraw() {
                self.state.handle_error(e);
            }
        }
    }

    /// Handle `-` in a directory buffer — navigate to parent.
    pub(super) fn handle_explorer_parent(&mut self) {
        use crate::document::BufferKind;

        // If we're in the explorer center pane, delegate to split-aware parent.
        let is_explorer_dir = self
            .panel_layout
            .as_ref()
            .map(|l| {
                l.kind == PanelKind::FileExplorer
                    && self.split_tree.focused_window_id() == l.dir_win_id
            })
            .unwrap_or(false);
        if is_explorer_dir {
            self.handle_explorer_split_parent();
            return;
        }

        let (doc_id, parent) = {
            let doc = match self.document_manager.active_document() {
                Some(d) if d.is_directory() => d,
                _ => return,
            };
            let parent = match &doc.kind {
                BufferKind::Directory { path, .. } => path.parent().map(|p| p.to_path_buf()),
                _ => return,
            };
            (doc.id, parent)
        };
        if let Some(parent_path) = parent {
            self.reload_directory_buffer(doc_id, parent_path);
        }
    }

    /// Open a 3-panel file explorer centred on `center_dir`.
    ///
    /// Layout after call:  [left: parent dir | center: center_dir | right: preview]
    pub fn open_explorer(&mut self, dir: std::path::PathBuf) {
        // If already active, just focus the dir pane.
        if let Some(ref layout) = self.panel_layout.clone() {
            self.split_tree.set_focus(layout.dir_win_id);
            let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
            return;
        }

        let dir_doc_id = self.document_manager.next_id();
        let dir_doc = match crate::document::Document::new_directory(dir_doc_id, dir.clone()) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_private_document(dir_doc);

        let preview_doc_id = self.document_manager.next_id();
        let preview_doc = match crate::document::Document::new_directory(
            preview_doc_id,
            std::path::PathBuf::from("[preview]"),
        ) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_private_document(preview_doc);

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        let dir_win_id = self.split_tree.focused_window_id();
        let original_doc_id = self.split_tree.focused_window().document_id;
        if let Some(w) = self.split_tree.windows.get_mut(&dir_win_id) {
            w.document_id = dir_doc_id;
        }

        let preview_win_id = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            dir_win_id,
            preview_doc_id,
            rows,
            cols,
        );

        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(dir_doc_id);

        self.panel_layout = Some(PanelLayout {
            kind: PanelKind::FileExplorer,
            dir_win_id,
            preview_win_id,
            dir_doc_id,
            preview_doc_id,
            original_doc_id,
        });

        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            dir_doc_id as usize,
            dir,
            false, /* new explorer always starts with hidden off */
        );
        self.job_manager.spawn(job);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Close the active explorer or undotree split session.
    pub fn close_split_panel(&mut self) {
        let layout = match self.panel_layout.take() {
            Some(l) => l,
            None => return,
        };

        match layout.kind {
            PanelKind::FileExplorer => {
                // Close preview window, restore original doc to dir window, remove both private docs.
                self.split_tree.close_window(layout.preview_win_id);
                self.document_manager
                    .remove_private_document(layout.preview_doc_id);
                if let Some(w) = self.split_tree.windows.get_mut(&layout.dir_win_id) {
                    w.document_id = layout.original_doc_id;
                }
                self.document_manager
                    .remove_private_document(layout.dir_doc_id);
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
            PanelKind::UndoTree => {
                // Reassign the preview window to show the original file before removing preview clone.
                if let Some(w) = self.split_tree.windows.get_mut(&layout.preview_win_id) {
                    w.document_id = layout.original_doc_id;
                }
                self.split_tree.close_window(layout.dir_win_id);
                self.document_manager
                    .remove_private_document(layout.dir_doc_id);
                self.document_manager
                    .remove_private_document(layout.preview_doc_id);
                self.split_tree.set_focus(layout.preview_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
            PanelKind::Clipboard => {
                self.split_tree.close_window(layout.preview_win_id);
                self.document_manager
                    .remove_private_document(layout.preview_doc_id);
                if let Some(w) = self.split_tree.windows.get_mut(&layout.dir_win_id) {
                    w.document_id = layout.original_doc_id;
                }
                self.document_manager
                    .remove_private_document(layout.dir_doc_id);
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
        }
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Open the messages log as a standalone buffer in the current window.
    pub fn open_messages(&mut self, show_all: bool) {
        let id = self.document_manager.next_id();
        let mut doc = match crate::document::Document::new_messages(id, show_all) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };

        let log = self
            .state
            .error_manager
            .notifications()
            .message_log()
            .to_vec();
        doc.populate_messages_buffer(&log);
        // On initial open, position at the end so the newest messages are visible
        let len = doc.buffer.len();
        let _ = doc.buffer.set_cursor(len.saturating_sub(1));

        self.document_manager.add_document(doc);
        if let Err(e) = self.document_manager.switch_to_document(id) {
            self.state.handle_error(e);
            return;
        }
        self.split_tree.focused_window_mut().document_id = id;

        self.last_notification_generation = self.state.error_manager.notifications().generation;

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Open the clipboard ring as a two-pane split: left = index, right = preview.
    pub fn open_clipboard(&mut self) {
        // If already open, just focus the index pane.
        if let Some(ref layout) = self.panel_layout.clone() {
            if layout.kind == PanelKind::Clipboard {
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
                return;
            }
        }

        let index_doc_id = self.document_manager.next_id();
        let mut index_doc = match crate::document::Document::new_clipboard(index_doc_id) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        index_doc.populate_clipboard_buffer(self.clipboard_ring.entries());
        self.document_manager.add_private_document(index_doc);

        let preview_doc_id = self.document_manager.next_id();
        let preview_doc = match crate::document::Document::new_clipboard(preview_doc_id) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_private_document(preview_doc);

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        let index_win_id = self.split_tree.focused_window_id();
        let original_doc_id = self.split_tree.focused_window().document_id;
        if let Some(w) = self.split_tree.windows.get_mut(&index_win_id) {
            w.document_id = index_doc_id;
        }

        let preview_win_id = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            index_win_id,
            preview_doc_id,
            rows,
            cols,
        );

        self.split_tree.set_focus(index_win_id);
        let _ = self.document_manager.switch_to_document(index_doc_id);

        self.panel_layout = Some(PanelLayout {
            kind: PanelKind::Clipboard,
            dir_win_id: index_win_id,
            preview_win_id,
            dir_doc_id: index_doc_id,
            preview_doc_id,
            original_doc_id,
        });

        self.update_clipboard_preview();
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Called after every cursor movement in the clipboard index pane: update the preview.
    pub(super) fn update_clipboard_preview(&mut self) {
        let layout = match &self.panel_layout {
            Some(l)
                if l.kind == PanelKind::Clipboard
                    && self.split_tree.focused_window_id() == l.dir_win_id =>
            {
                l.clone()
            }
            _ => return,
        };

        let (entry_text, preview_doc_id) = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes);
            let line_text = line_text.trim();

            // Lines are `[N]` — parse N to look up the entry in the snapshot
            let idx = line_text
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))
                .and_then(|inner| inner.parse::<usize>().ok());

            let text = match idx {
                Some(i) => match &doc.kind {
                    crate::document::BufferKind::Clipboard { entries } => {
                        entries.get(i).cloned().unwrap_or_default()
                    }
                    _ => String::new(),
                },
                None => String::new(),
            };

            (text, layout.preview_doc_id)
        };

        if let Some(preview) = self.document_manager.get_document_mut(preview_doc_id) {
            let old_revision = preview.buffer.revision;
            if let Ok(mut new_buf) = crate::buffer::TextBuffer::new(entry_text.len().max(64)) {
                let _ = new_buf.insert_str(&entry_text);
                let _ = new_buf.set_cursor(0);
                new_buf.revision = old_revision + 1;
                preview.buffer = new_buf;
            }
            preview.custom_highlights.clear();
        }

        let _ = self.force_full_redraw();
    }

    /// Apply the order/deletions from the clipboard index buffer back to the ring.
    pub(super) fn apply_clipboard_diff(&mut self) {
        use crate::document::BufferKind;

        let (entries_snapshot, order) = {
            let doc = match self.document_manager.active_document() {
                Some(d) if d.is_clipboard() => d,
                _ => return,
            };
            let entries = match &doc.kind {
                BufferKind::Clipboard { entries } => entries.clone(),
                _ => return,
            };
            let order = doc.parse_clipboard_order();
            (entries, order)
        };

        // Rebuild the ring from the parsed order
        let mut new_entries: std::collections::VecDeque<String> = order
            .into_iter()
            .filter_map(|i| entries_snapshot.get(i).cloned())
            .collect();

        // Replace ring contents (newest = index 0)
        self.clipboard_ring = {
            let mut ring = crate::clipboard::ClipboardRing::new();
            // Push in reverse so index 0 ends up at front
            for entry in new_entries.drain(..).rev() {
                ring.push(entry);
            }
            ring
        };

        // Repopulate the index buffer to reflect the canonical [N] indices
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.populate_clipboard_buffer(self.clipboard_ring.entries());
        }

        self.state.notify(
            crate::notification::NotificationType::Info,
            format!("{} clipboard entries", self.clipboard_ring.len()),
        );

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Open the undo tree for the active document as a split pane.
    pub fn open_undotree_split(&mut self) {
        if let Some(ref layout) = self.panel_layout.clone() {
            if layout.kind == PanelKind::UndoTree {
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
                return;
            }
        }

        let linked_id = self.active_document_id();

        let ut_id = self.document_manager.next_id();
        let ut_doc = match crate::document::Document::new_undotree(ut_id, linked_id) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        self.document_manager.add_private_document(ut_doc);

        // Create a private read-only preview doc (clone of linked) so goto_seq never
        // touches the original file and show_line_numbers is naturally false.
        let preview_id = self.document_manager.next_id();
        let preview_doc = {
            let linked = match self.document_manager.get_document(linked_id) {
                Some(d) => d,
                None => {
                    return;
                }
            };
            match crate::document::Document::new_undotree_preview(preview_id, linked) {
                Ok(d) => d,
                Err(e) => {
                    self.state.handle_error(e);
                    return;
                }
            }
        };
        self.document_manager.add_private_document(preview_doc);

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        let dir_win_id = self.split_tree.focused_window_id();
        if let Some(w) = self.split_tree.windows.get_mut(&dir_win_id) {
            w.document_id = ut_id;
        }

        let preview_win_id = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            dir_win_id,
            preview_id,
            rows,
            cols,
        );

        if let Some(linked_doc) = self.document_manager.get_document(linked_id) {
            let (text, seqs, lc) = crate::undotree_view::render_tree_to_text(&linked_doc.history);
            if let Some(ut_doc) = self.document_manager.get_document_mut(ut_id) {
                ut_doc.populate_undotree_buffer(text, seqs, lc);
            }
        }

        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(ut_id);

        self.panel_layout = Some(PanelLayout {
            kind: PanelKind::UndoTree,
            dir_win_id,
            preview_win_id,
            dir_doc_id: ut_id,
            preview_doc_id: preview_id,
            original_doc_id: linked_id,
        });

        self.sync_state_with_active_document();
        let _ = self.update_and_render();
    }

    /// Called after every cursor movement: if in the explorer dir pane, spawn a preview job.
    pub(super) fn update_explorer_preview(&mut self) {
        let layout = match &self.panel_layout {
            Some(l)
                if l.kind == PanelKind::FileExplorer
                    && self.split_tree.focused_window_id() == l.dir_win_id =>
            {
                l.clone()
            }
            _ => return,
        };

        let (target_path, preview_doc_id) = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
            let dir_path = match &doc.kind {
                crate::document::BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            };
            let entry_name = line_text.trim_end_matches('/');
            if entry_name.is_empty() || entry_name == ".." {
                return;
            }
            (dir_path.join(entry_name), layout.preview_doc_id)
        };

        let job = crate::job_manager::jobs::explorer_preview::ExplorerPreviewJob::new(
            preview_doc_id,
            target_path,
            false,
        );
        self.job_manager.spawn(job);
    }

    /// Called after every cursor movement in the undotree pane: applies goto_seq on the linked doc.
    pub(super) fn update_undotree_preview(&mut self) {
        let layout = match &self.panel_layout {
            Some(l)
                if l.kind == PanelKind::UndoTree
                    && self.split_tree.focused_window_id() == l.dir_win_id =>
            {
                l.clone()
            }
            _ => return,
        };

        let seq = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            match &doc.kind {
                crate::document::BufferKind::UndoTree { sequences, .. } => {
                    sequences.get(line_num).copied().unwrap_or(u64::MAX)
                }
                _ => return,
            }
        };

        if seq == u64::MAX {
            return;
        }

        if let Some(linked_doc) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        {
            let _ = linked_doc.goto_seq(seq);
            // Reset cursor to top so the preview viewport starts from the beginning
            let _ = linked_doc.buffer.set_cursor(0);
        }
        // Sync the preview window's cursor_position so update_window_viewports uses position 0
        if let Some(w) = self.split_tree.get_window_mut(layout.preview_win_id) {
            w.cursor_position = 0;
        }
        self.spawn_syntax_parse_job(layout.preview_doc_id);
        let _ = self.update_and_render();
    }

    /// Enter a directory or open a file from the explorer dir pane.
    pub(super) fn handle_explorer_split_select(&mut self) {
        let layout = match self.panel_layout.clone() {
            Some(l) => l,
            None => return,
        };

        let (line_text, dir_path) = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            if doc.is_dirty() {
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    "Unsaved changes — write with :w first".to_string(),
                );
                return;
            }
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
            let dir_path = match &doc.kind {
                crate::document::BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            };
            (line_text, dir_path)
        };

        let entry_name = line_text.trim_end_matches('/');
        if entry_name.is_empty() || entry_name == ".." {
            self.handle_explorer_split_parent();
            return;
        }

        let target_path = dir_path.join(entry_name);

        if target_path.is_dir() {
            self.reload_directory_buffer(layout.dir_doc_id, target_path);
            self.update_explorer_preview();
        } else {
            self.close_split_panel();
            if let Err(e) = self.open_file(Some(target_path.display().to_string()), false) {
                self.state.handle_error(e);
            } else {
                self.state.clear_command_line();
                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
            }
        }
    }

    /// Navigate the explorer dir pane to its parent directory.
    pub(super) fn handle_explorer_split_parent(&mut self) {
        let layout = match self.panel_layout.clone() {
            Some(l) => l,
            None => return,
        };

        let parent_path = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            match &doc.kind {
                crate::document::BufferKind::Directory { path, .. } => {
                    match path.parent().map(|p| p.to_path_buf()) {
                        Some(p) => p,
                        None => return,
                    }
                }
                _ => return,
            }
        };

        self.reload_directory_buffer(layout.dir_doc_id, parent_path);
        self.update_explorer_preview();
    }

    /// Handle <CR> in an undo-tree buffer — jump to the node on the cursor line.
    pub(super) fn handle_undotree_select(&mut self) {
        use crate::document::BufferKind;

        let (linked_doc_id, seq) = {
            let doc_id = self.active_document_id();
            let doc = match self.document_manager.get_document(doc_id) {
                Some(d) if d.is_undotree() => d,
                _ => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let (linked_id, seq) = match &doc.kind {
                BufferKind::UndoTree {
                    linked_doc_id,
                    sequences,
                } => {
                    let seq = sequences.get(line_num).copied().unwrap_or(u64::MAX);
                    (*linked_doc_id, seq)
                }
                _ => return,
            };
            (linked_id, seq)
        };

        if seq == u64::MAX {
            return;
        } // connector line

        if let Some(linked_doc) = self.document_manager.get_document_mut(linked_doc_id) {
            if linked_doc.goto_seq(seq).is_err() {
                return;
            }
        }
        self.spawn_syntax_parse_job(linked_doc_id);

        // Close the undo tree pane and focus the linked document
        self.close_split_panel();
    }

    /// Toggle hidden-file visibility for the active file explorer pane.
    pub(super) fn handle_explorer_toggle_hidden(&mut self) {
        use crate::document::BufferKind;

        // Find the directory document for the active explorer (split or standalone).
        let dir_doc_id = if let Some(layout) = self.panel_layout.as_ref() {
            if layout.kind == PanelKind::FileExplorer {
                layout.dir_doc_id
            } else {
                return;
            }
        } else {
            // Standalone directory buffer
            match self.document_manager.active_document() {
                Some(d) if d.is_directory() => d.id,
                _ => return,
            }
        };

        let (path, show_hidden) = match self.document_manager.get_document(dir_doc_id) {
            Some(d) => match &d.kind {
                BufferKind::Directory {
                    path, show_hidden, ..
                } => (path.clone(), *show_hidden),
                _ => return,
            },
            None => return,
        };

        let new_show_hidden = !show_hidden;

        if let Some(doc) = self.document_manager.get_document_mut(dir_doc_id) {
            if let BufferKind::Directory { show_hidden, .. } = &mut doc.kind {
                *show_hidden = new_show_hidden;
            }
        }

        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            dir_doc_id as usize,
            path,
            new_show_hidden,
        );
        self.job_manager.spawn(job);
    }

    /// Re-read the directory listing for the active file explorer pane.
    pub(super) fn handle_explorer_refresh(&mut self) {
        let layout = match self.panel_layout.as_ref() {
            Some(l) if l.kind == PanelKind::FileExplorer => l.clone(),
            _ => return,
        };
        let (path, show_hidden) = match self.document_manager.get_document(layout.dir_doc_id) {
            Some(d) => match &d.kind {
                crate::document::BufferKind::Directory {
                    path, show_hidden, ..
                } => (path.clone(), *show_hidden),
                _ => return,
            },
            None => return,
        };
        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            layout.dir_doc_id as usize,
            path,
            show_hidden,
        );
        self.job_manager.spawn(job);
    }

    /// Re-render the undotree buffer from the linked document's current history.
    pub(super) fn handle_undotree_refresh(&mut self) {
        let layout = match self.panel_layout.as_ref() {
            Some(l) if l.kind == PanelKind::UndoTree => l.clone(),
            _ => return,
        };
        if let Some(linked_doc) = self.document_manager.get_document(layout.original_doc_id) {
            let (text, seqs, lc) = crate::undotree_view::render_tree_to_text(&linked_doc.history);
            if let Some(ut_doc) = self.document_manager.get_document_mut(layout.dir_doc_id) {
                ut_doc.populate_undotree_buffer(text, seqs, lc);
            }
        }
        let _ = self.update_and_render();
    }

    /// Apply the diff from a directory buffer to the filesystem.
    pub(super) fn apply_directory_diff(&mut self) {
        use crate::document::BufferKind;
        use std::fs;

        let (dir_doc_id, dir_path, dir_show_hidden, diff) = {
            let doc = match self.document_manager.active_document() {
                Some(d) if d.is_directory() => d,
                _ => return,
            };
            let (path, show_hidden) = match &doc.kind {
                BufferKind::Directory {
                    path, show_hidden, ..
                } => (path.clone(), *show_hidden),
                _ => return,
            };
            (doc.id, path, show_hidden, doc.parse_directory_diff())
        };

        if diff.renames.is_empty() && diff.deletes.is_empty() && diff.creates.is_empty() {
            return;
        }

        let mut errors: Vec<String> = Vec::new();
        let mut applied = 0usize;

        // Renames — run synchronously so the reload sees the final state
        for (old_path, new_name) in &diff.renames {
            let new_path = old_path.parent().unwrap_or(&dir_path).join(new_name);
            if let Some(parent) = new_path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    errors.push(format!("mkdir {:?}: {}", parent, e));
                    continue;
                }
            }
            let result = fs::rename(old_path, &new_path).or_else(|_| {
                // Cross-device fallback: copy then delete
                crate::job_manager::jobs::fs::FsCopyJob::copy_recursive_pub(old_path, &new_path)
                    .and_then(|_| {
                        if old_path.is_dir() {
                            fs::remove_dir_all(old_path)
                        } else {
                            fs::remove_file(old_path)
                        }
                    })
            });
            match result {
                Ok(_) => applied += 1,
                Err(e) => errors.push(format!(
                    "rename {:?}: {}",
                    old_path.file_name().unwrap_or_default(),
                    e
                )),
            }
        }

        // Deletes
        for path in &diff.deletes {
            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
            match result {
                Ok(_) => applied += 1,
                Err(e) => errors.push(format!(
                    "delete {:?}: {}",
                    path.file_name().unwrap_or_default(),
                    e
                )),
            }
        }

        // Creates
        for name in &diff.creates {
            let is_dir = name.ends_with('/');
            let clean_name = name.trim_end_matches('/');
            let new_path = dir_path.join(clean_name);
            let result = if is_dir {
                fs::create_dir_all(&new_path)
            } else {
                if let Some(parent) = new_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                fs::File::create(&new_path).map(|_| ())
            };
            match result {
                Ok(_) => applied += 1,
                Err(e) => errors.push(format!(
                    "create {:?}: {}",
                    new_path.file_name().unwrap_or_default(),
                    e
                )),
            }
        }

        if applied > 0 {
            self.state.notify(
                crate::notification::NotificationType::Info,
                format!("Applied {} change(s)", applied),
            );
        }
        for err in errors {
            self.state
                .notify(crate::notification::NotificationType::Error, err);
        }

        // Re-read the current directory now that all operations are complete.
        let reload = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            dir_doc_id as usize,
            dir_path,
            dir_show_hidden,
        );
        self.job_manager.spawn(reload);
    }
}
