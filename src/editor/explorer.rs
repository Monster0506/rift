use super::Editor;
use super::{PanelKind, PanelLayout};
use crate::term::TerminalBackend;

#[derive(Debug, Clone)]
pub(super) struct PendingExplorerPreview {
    pub(super) path: std::path::PathBuf,
    pub(super) job_id: usize,
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn reload_directory_buffer(
        &mut self,
        doc_id: crate::document::DocumentId,
        new_path: std::path::PathBuf,
    ) {
        let show_hidden = self
            .document_manager
            .get_document(doc_id)
            .and_then(super::super::document::Document::directory_show_hidden)
            .unwrap_or(false);
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.convert_to_directory(new_path.clone());
            doc.set_directory_show_hidden(show_hidden);
            doc.replace_buffer_content("");
        }
        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            doc_id,
            new_path,
            show_hidden,
        );
        self.job_manager.spawn(job);
    }

    pub(super) fn handle_explorer_select(&mut self) {
        let is_explorer_dir = self.panel_layout.as_ref().is_some_and(|l| {
            l.kind == PanelKind::FileExplorer && self.split_tree.focused_window_id() == l.dir_win_id
        });
        if is_explorer_dir {
            self.handle_explorer_split_select();
            return;
        }

        let (doc_id, line_text, dir_path, entry_info) = {
            let Some(doc) = self.document_manager.active_document() else {
                return;
            };
            let diff = doc.parse_directory_diff();
            if !diff.renames.is_empty() || !diff.deletes.is_empty() || !diff.creates.is_empty() {
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
            let Some(dir_path) = doc.directory_path().cloned() else {
                return;
            };
            let entry_info = doc.annotations.directory_entry_info_at_line(line_num);
            (doc.id, line_text, dir_path, entry_info)
        };

        if line_text == "../" {
            if let Some(parent) = dir_path.parent().map(std::path::Path::to_path_buf) {
                self.reload_directory_buffer(doc_id, parent);
            }
            return;
        }

        if entry_info.is_some() && self.activate_annotation_at_cursor() {
            return;
        }

        let entry_name = line_text.trim_end_matches('/').to_string();
        if entry_name.is_empty() {
            return;
        }
        let target_path = dir_path.join(&entry_name);
        if crate::fs_backend::backend().is_dir(&target_path) {
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

    pub(super) fn handle_explorer_parent(&mut self) {
        let is_explorer_dir = self.panel_layout.as_ref().is_some_and(|l| {
            l.kind == PanelKind::FileExplorer && self.split_tree.focused_window_id() == l.dir_win_id
        });
        if is_explorer_dir {
            self.handle_explorer_split_parent();
            return;
        }

        let (doc_id, parent) = {
            let Some(doc) = self.document_manager.active_document() else {
                return;
            };
            let Some(path) = doc.directory_path() else {
                return;
            };
            let parent = path.parent().map(std::path::Path::to_path_buf);
            (doc.id, parent)
        };
        if let Some(parent_path) = parent {
            self.reload_directory_buffer(doc_id, parent_path);
        }
    }

    fn abort_panel_open(
        &mut self,
        win_id: crate::split::window::WindowId,
        original_doc_id: crate::document::DocumentId,
        private_docs: &[crate::document::DocumentId],
    ) {
        self.split_tree.set_window_document(win_id, original_doc_id);
        for id in private_docs {
            let _ = self.remove_private_document(*id);
        }
    }

    pub fn open_explorer(&mut self, dir: std::path::PathBuf) {
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
        self.split_tree.set_window_document(dir_win_id, dir_doc_id);

        let Some(preview_win_id) = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            dir_win_id,
            preview_doc_id,
            rows,
            cols,
        ) else {
            self.abort_panel_open(dir_win_id, original_doc_id, &[dir_doc_id, preview_doc_id]);
            return;
        };

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

        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(dir_doc_id, dir, false);
        self.job_manager.spawn(job);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    pub fn close_split_panel(&mut self) {
        let Some(layout) = self.panel_layout.take() else {
            return;
        };

        match layout.kind {
            PanelKind::FileExplorer | PanelKind::Clipboard => {
                self.split_tree.close_window(layout.preview_win_id);
                let _ = self.remove_private_document(layout.preview_doc_id);
                self.split_tree
                    .set_window_document(layout.dir_win_id, layout.original_doc_id);
                let _ = self.remove_private_document(layout.dir_doc_id);
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
            PanelKind::UndoTree => {
                self.split_tree
                    .set_window_document(layout.preview_win_id, layout.original_doc_id);
                self.split_tree.close_window(layout.dir_win_id);
                let _ = self.remove_private_document(layout.dir_doc_id);
                let _ = self.remove_private_document(layout.preview_doc_id);
                self.split_tree.set_focus(layout.preview_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
            PanelKind::LocationList | PanelKind::Regions => {
                self.split_tree.close_window(layout.dir_win_id);
                let _ = self.remove_private_document(layout.dir_doc_id);
                self.split_tree.set_focus(layout.preview_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
            PanelKind::BufferList => {
                self.split_tree.close_window(layout.preview_win_id);
                self.split_tree
                    .set_window_document(layout.dir_win_id, layout.original_doc_id);
                let _ = self.remove_private_document(layout.dir_doc_id);
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
        }
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    pub fn open_messages(&mut self, show_all: bool) {
        let id = self.document_manager.next_id();
        let mut doc = match crate::document::Document::new_messages(id, show_all) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };

        let log = self.state.error_manager.notifications().message_log();
        doc.populate_messages_buffer(&log);
        let len = doc.buffer.len();
        let _ = doc.buffer.set_cursor(len.saturating_sub(1));

        self.document_manager.add_document(doc);
        if let Err(e) = self.document_manager.switch_to_document(id) {
            self.state.handle_error(e);
            return;
        }
        self.split_tree.set_focused_document(id);

        self.last_notification_generation = self.state.error_manager.notifications().generation;

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    pub fn open_clipboard(&mut self) {
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
        self.split_tree
            .set_window_document(index_win_id, index_doc_id);

        let Some(preview_win_id) = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            index_win_id,
            preview_doc_id,
            rows,
            cols,
        ) else {
            self.abort_panel_open(
                index_win_id,
                original_doc_id,
                &[index_doc_id, preview_doc_id],
            );
            return;
        };

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

    pub fn open_buffer_list_panel(&mut self) {
        if let Some(layout) = self.panel_layout.clone() {
            if layout.kind == PanelKind::BufferList {
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
                self.refresh_buffer_list_panel();
                self.update_buffer_list_preview();
                let _ = self.force_full_redraw();
                return;
            }
            self.close_split_panel();
        }

        let dir_doc_id = self.document_manager.next_id();
        let mut dir_doc = match crate::document::Document::new_buffer_list(dir_doc_id) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        let infos = self.document_manager.get_buffer_list();
        dir_doc.populate_buffer_list_buffer(&infos);
        self.document_manager.add_private_document(dir_doc);

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        let dir_win_id = self.split_tree.focused_window_id();
        let original_doc_id = self.split_tree.focused_window().document_id;
        let preview_target = infos.first().map_or(original_doc_id, |i| i.id);
        self.split_tree.set_window_document(dir_win_id, dir_doc_id);

        let Some(preview_win_id) = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            dir_win_id,
            preview_target,
            rows,
            cols,
        ) else {
            self.abort_panel_open(dir_win_id, original_doc_id, &[dir_doc_id]);
            return;
        };

        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(dir_doc_id);

        self.panel_layout = Some(PanelLayout {
            kind: PanelKind::BufferList,
            dir_win_id,
            preview_win_id,
            dir_doc_id,
            preview_doc_id: original_doc_id,
            original_doc_id,
        });

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    pub(super) fn refresh_buffer_list_panel(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::BufferList) else {
            return;
        };
        let infos = self.document_manager.get_buffer_list();
        let cursor_line = self
            .document_manager
            .get_document(layout.dir_doc_id)
            .map_or(0, |d| d.buffer.line_index.get_line_at(d.buffer.cursor()));
        if let Some(doc) = self.document_manager.get_document_mut(layout.dir_doc_id) {
            doc.populate_buffer_list_buffer(&infos);
            let target_line = cursor_line.min(infos.len().saturating_sub(1));
            if let Some(start) = doc.buffer.line_index.get_start(target_line) {
                let _ = doc.buffer.set_cursor(start);
            }
        }
    }

    pub(super) fn update_buffer_list_preview(&mut self) {
        let layout = match &self.panel_layout {
            Some(l)
                if l.kind == PanelKind::BufferList
                    && self.split_tree.focused_window_id() == l.dir_win_id =>
            {
                l.clone()
            }
            _ => return,
        };

        let target_id = {
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            match doc
                .buffer_list_entries()
                .and_then(|entries| entries.get(line_num))
            {
                Some(id) => *id,
                None => return,
            }
        };

        if self.document_manager.get_document(target_id).is_none() {
            return;
        }
        let already_showing = self
            .split_tree
            .get_window(layout.preview_win_id)
            .map(|w| w.document_id)
            == Some(target_id);
        if already_showing {
            return;
        }
        self.split_tree
            .set_window_document(layout.preview_win_id, target_id);
        let _ = self.update_and_render();
    }

    pub(super) fn handle_buffer_list_select(&mut self) {
        let layout = match self.panel_layout.clone() {
            Some(l) if l.kind == PanelKind::BufferList => l,
            _ => return,
        };
        let target_id = {
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            match doc
                .buffer_list_entries()
                .and_then(|entries| entries.get(line_num))
            {
                Some(id) => *id,
                None => return,
            }
        };
        let target_exists = self.document_manager.get_document(target_id).is_some();
        self.close_split_panel();
        if target_exists {
            let _ = self.document_manager.switch_to_document(target_id);
            self.split_tree.set_focused_document(target_id);
            self.sync_state_with_active_document();
            let _ = self.force_full_redraw();
        }
    }

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
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes);
            let line_text = line_text.trim();

            let idx = line_text
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))
                .and_then(|inner| inner.parse::<usize>().ok());

            let text = idx
                .and_then(|index| doc.clipboard_entry(index))
                .map(<[crate::character::Character]>::to_vec)
                .unwrap_or_default();

            (text, layout.preview_doc_id)
        };

        if let Some(preview) = self.document_manager.get_document_mut(preview_doc_id) {
            let old_revision = preview.buffer.revision;
            if let Ok(mut new_buf) = crate::buffer::TextBuffer::new(entry_text.len().max(64)) {
                let _ = new_buf.insert_chars(&entry_text);
                let _ = new_buf.set_cursor(0);
                new_buf.revision = old_revision + 1;
                preview.buffer = new_buf;
            }
            preview.custom_highlights.clear();
        }

        let _ = self.force_full_redraw();
    }

    pub(super) fn apply_clipboard_diff(&mut self) {
        let (entries_snapshot, order) = {
            let Some(doc) = self.document_manager.active_document() else {
                return;
            };
            let Some(entries) = doc
                .clipboard_entries()
                .map(<[Vec<crate::character::Character>]>::to_vec)
            else {
                return;
            };
            let order = doc.parse_clipboard_order();
            (entries, order)
        };

        let new_entries: std::collections::VecDeque<Vec<crate::character::Character>> = order
            .into_iter()
            .filter_map(|i| entries_snapshot.get(i).cloned())
            .collect();

        self.clipboard_ring = {
            let mut ring = crate::clipboard::ClipboardRing::new();
            for entry in new_entries.into_iter().rev() {
                ring.push(entry);
            }
            ring
        };

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

        let preview_id = self.document_manager.next_id();
        let preview_doc = {
            let Some(linked) = self.document_manager.get_document(linked_id) else {
                return;
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
        let original_doc_id = self.split_tree.focused_window().document_id;
        self.split_tree.set_window_document(dir_win_id, ut_id);

        let Some(preview_win_id) = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            dir_win_id,
            preview_id,
            rows,
            cols,
        ) else {
            self.abort_panel_open(dir_win_id, original_doc_id, &[ut_id, preview_id]);
            return;
        };

        if let Some(linked_doc) = self.document_manager.get_document(linked_id) {
            let (text, seqs, lc) = crate::undotree_view::render_tree_to_text(&linked_doc.history);
            if let Some(ut_doc) = self.document_manager.get_document_mut(ut_id) {
                ut_doc.populate_undotree_buffer(&text, seqs, lc);
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

    pub(super) fn panel_layout_of(&self, kind: PanelKind) -> Option<PanelLayout> {
        match &self.panel_layout {
            Some(l) if l.kind == kind => Some(l.clone()),
            _ => None,
        }
    }

    pub(super) fn current_explorer_target_path(&self) -> Option<std::path::PathBuf> {
        let layout = match &self.panel_layout {
            Some(l)
                if l.kind == PanelKind::FileExplorer
                    && self.split_tree.focused_window_id() == l.dir_win_id =>
            {
                l.clone()
            }
            _ => return None,
        };

        let doc = self.document_manager.get_document(layout.dir_doc_id)?;
        let cursor = doc.buffer.cursor();
        let line_num = doc.buffer.line_index.get_line_at(cursor);
        let line_bytes = doc.buffer.get_line_bytes(line_num);
        let line_text = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
        let dir_path = doc.directory_path()?.clone();
        let entry_name = line_text.trim_end_matches('/');
        if entry_name.is_empty() || entry_name == ".." {
            return None;
        }
        Some(dir_path.join(entry_name))
    }

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

        let Some(target_path) = self.current_explorer_target_path() else {
            return;
        };
        let preview_doc_id = layout.preview_doc_id;

        let already_showing = self
            .document_manager
            .get_document(preview_doc_id)
            .is_some_and(|doc| {
                doc.directory_path().map_or_else(
                    || doc.path() == Some(target_path.as_path()),
                    |path| path == &target_path,
                )
            });
        if already_showing {
            return;
        }

        if let Some(pending) = &self.pending_explorer_preview {
            if pending.path == target_path {
                return;
            }
        }

        if let Some(pending) = self.pending_explorer_preview.take() {
            self.job_manager.cancel_job(pending.job_id);
        }

        let job = crate::job_manager::jobs::explorer_preview::ExplorerPreviewJob::new(
            preview_doc_id,
            target_path.clone(),
            false,
        );
        let job_id = self.job_manager.spawn(job);
        self.pending_explorer_preview = Some(PendingExplorerPreview {
            path: target_path,
            job_id,
        });
    }

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
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            match doc
                .undotree_sequences()
                .and_then(|sequences| sequences.get(line_num))
            {
                Some(sequence) => *sequence,
                None => return,
            }
        };

        if let Some(linked_doc) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        {
            let _ = linked_doc.goto_seq(seq);
            let _ = linked_doc.buffer.set_cursor(0);
        }
        if let Some(w) = self.split_tree.get_window_mut(layout.preview_win_id) {
            w.cursor_position = 0;
        }
        self.spawn_syntax_parse_job(layout.preview_doc_id);
        let _ = self.update_and_render();
    }

    pub(super) fn handle_explorer_split_select(&mut self) {
        let Some(layout) = self.panel_layout.clone() else {
            return;
        };

        let (line_text, dir_path, entry_info) = {
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let diff = doc.parse_directory_diff();
            if !diff.renames.is_empty() || !diff.deletes.is_empty() || !diff.creates.is_empty() {
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
            let Some(dir_path) = doc.directory_path().cloned() else {
                return;
            };
            let entry_info = doc.annotations.directory_entry_info_at_line(line_num);
            (line_text, dir_path, entry_info)
        };

        let (entry_name, is_dir) = if let Some((name, is_dir)) = entry_info {
            (name, is_dir)
        } else {
            let name = line_text.trim_end_matches('/').to_string();
            let is_dir = std::fs::symlink_metadata(dir_path.join(&name))
                .map(|m| m.is_dir())
                .unwrap_or(false);
            (name, is_dir)
        };
        if entry_name.is_empty() || entry_name == ".." {
            self.handle_explorer_split_parent();
            return;
        }

        let target_path = dir_path.join(&entry_name);

        if is_dir {
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

    pub(super) fn handle_explorer_split_parent(&mut self) {
        let Some(layout) = self.panel_layout.clone() else {
            return;
        };

        let (parent_path, child_name) = {
            let Some(doc) = self.document_manager.get_document(layout.dir_doc_id) else {
                return;
            };
            let Some(path) = doc.directory_path() else {
                return;
            };
            let child = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string);
            match path.parent().map(std::path::Path::to_path_buf) {
                Some(parent) => (parent, child),
                None => return,
            }
        };

        self.pending_cursor_entry = child_name;

        self.reload_directory_buffer(layout.dir_doc_id, parent_path);
        self.update_explorer_preview();
    }

    pub(super) fn handle_undotree_select(&mut self) {
        let (linked_doc_id, seq) = {
            let doc_id = self.active_document_id();
            let Some(doc) = self.document_manager.get_document(doc_id) else {
                return;
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let Some(linked_id) = doc.undotree_linked_doc_id() else {
                return;
            };
            let Some(sequences) = doc.undotree_sequences() else {
                return;
            };
            let seq = sequences.get(line_num).copied().unwrap_or(u64::MAX);
            (linked_id, seq)
        };

        if seq == u64::MAX {
            return;
        }

        if let Some(linked_doc) = self.document_manager.get_document_mut(linked_doc_id) {
            if linked_doc.goto_seq(seq).is_err() {
                return;
            }
        }
        self.spawn_syntax_parse_job(linked_doc_id);

        self.close_split_panel();
    }

    pub(super) fn handle_undotree_move(&mut self, delta: i64) {
        let doc_id = self.active_document_id();
        let target_line = {
            let Some(doc) = self.document_manager.get_document(doc_id) else {
                return;
            };
            let cursor = doc.buffer.cursor();
            let line_num =
                i64::try_from(doc.buffer.line_index.get_line_at(cursor)).unwrap_or(i64::MAX);
            let total = i64::try_from(doc.buffer.get_total_lines()).unwrap_or(i64::MAX);
            let Some(sequences) = doc.undotree_sequences() else {
                return;
            };
            let mut target = line_num + delta;
            while target >= 0
                && target < total
                && sequences
                    .get(usize::try_from(target).unwrap_or(usize::MAX))
                    .copied()
                    .unwrap_or(u64::MAX)
                    == u64::MAX
            {
                target += delta;
            }
            usize::try_from(target.clamp(0, total - 1)).unwrap_or(0)
        };

        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            let pos = doc.buffer.line_index.get_line_start(target_line);
            let _ = doc.buffer.set_cursor(pos);
        }
        self.update_undotree_preview();
    }

    pub(super) fn handle_explorer_toggle_hidden(&mut self) {
        let dir_doc_id = if let Some(layout) = self.panel_layout.as_ref() {
            if layout.kind == PanelKind::FileExplorer {
                layout.dir_doc_id
            } else {
                return;
            }
        } else {
            match self.document_manager.active_document() {
                Some(d) => d.id,
                None => return,
            }
        };

        let Some(doc) = self.document_manager.get_document(dir_doc_id) else {
            return;
        };
        let Some(path) = doc.directory_path().cloned() else {
            return;
        };
        let Some(show_hidden) = doc.directory_show_hidden() else {
            return;
        };

        if let Some(doc) = self.document_manager.get_document(dir_doc_id) {
            let diff = doc.parse_directory_diff();
            if !diff.renames.is_empty() || !diff.deletes.is_empty() || !diff.creates.is_empty() {
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    "Unsaved changes — write with :w first".to_string(),
                );
                return;
            }
        }

        let new_show_hidden = !show_hidden;

        if let Some(doc) = self.document_manager.get_document_mut(dir_doc_id) {
            doc.set_directory_show_hidden(new_show_hidden);
        }

        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            dir_doc_id,
            path,
            new_show_hidden,
        );
        self.job_manager.spawn(job);
    }

    pub(super) fn handle_explorer_refresh(&mut self) {
        let dir_doc_id = if let Some(l) = self.panel_layout.as_ref() {
            if l.kind == PanelKind::FileExplorer {
                l.dir_doc_id
            } else {
                return;
            }
        } else {
            match self.document_manager.active_document() {
                Some(d) => d.id,
                None => return,
            }
        };

        let Some(doc) = self.document_manager.get_document(dir_doc_id) else {
            return;
        };
        let Some(path) = doc.directory_path().cloned() else {
            return;
        };
        let Some(show_hidden) = doc.directory_show_hidden() else {
            return;
        };

        if let Some(doc) = self.document_manager.get_document(dir_doc_id) {
            let diff = doc.parse_directory_diff();
            if !diff.renames.is_empty() || !diff.deletes.is_empty() || !diff.creates.is_empty() {
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    "Unsaved changes — write with :w first".to_string(),
                );
                return;
            }
        }

        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            dir_doc_id,
            path,
            show_hidden,
        );
        self.job_manager.spawn(job);
    }

    pub(super) fn handle_undotree_refresh(&mut self) {
        let Some(layout) = self.panel_layout_of(PanelKind::UndoTree) else {
            return;
        };
        if let Some(linked_doc) = self.document_manager.get_document(layout.original_doc_id) {
            let job = crate::job_manager::jobs::undotree::UndoTreeRenderJob::new(
                layout.dir_doc_id,
                linked_doc.history.clone(),
            );
            self.job_manager.spawn(job);
        }
    }

    pub(super) fn apply_directory_diff(&mut self) {
        let (dir_doc_id, dir_path, dir_show_hidden, diff) = {
            let Some(doc) = self.document_manager.active_document() else {
                return;
            };
            let Some(path) = doc.directory_path().cloned() else {
                return;
            };
            let Some(show_hidden) = doc.directory_show_hidden() else {
                return;
            };
            (doc.id, path, show_hidden, doc.parse_directory_diff())
        };

        if diff.renames.is_empty() && diff.deletes.is_empty() && diff.creates.is_empty() {
            return;
        }

        let errors = apply_directory_diff_to_fs(&dir_path, &diff);

        for err in errors {
            self.state
                .notify(crate::notification::NotificationType::Error, err);
        }

        let reload = crate::job_manager::jobs::explorer::DirectoryListJob::new(
            dir_doc_id,
            dir_path,
            dir_show_hidden,
        );
        self.job_manager.spawn(reload);
    }
}

const RENAME_TMP_PREFIX: &str = ".rift-rename-";

fn rename_error(old: &std::path::Path, msg: &str) -> String {
    format!(
        "rename {}: {msg}",
        old.file_name().unwrap_or_default().display()
    )
}

fn is_same_entry(
    fs: &dyn crate::fs_backend::FileSystem,
    a: &std::path::Path,
    b: &std::path::Path,
) -> bool {
    fs.canonicalize(a) == fs.canonicalize(b)
}

fn plan_renames(
    fs: &dyn crate::fs_backend::FileSystem,
    dir_path: &std::path::Path,
    renames: &[(std::path::PathBuf, String)],
    errors: &mut Vec<String>,
) -> Vec<(std::path::PathBuf, std::path::PathBuf)> {
    let mut targeted = std::collections::HashSet::new();
    let mut plan = Vec::new();

    for (old_path, new_name) in renames {
        let new_path = old_path.parent().unwrap_or(dir_path).join(new_name);

        if fs.is_dir(old_path) {
            let old_canonical = fs.canonicalize(old_path);
            let new_parent_raw = new_path.parent().unwrap_or(&new_path);
            let new_canonical = fs.canonicalize(new_parent_raw);
            if new_canonical.starts_with(&old_canonical) {
                errors.push(rename_error(old_path, "destination is inside the source"));
                continue;
            }
        }

        if !targeted.insert(new_path.clone()) {
            errors.push(rename_error(old_path, "another rename has the same name"));
            continue;
        }
        plan.push((old_path.clone(), new_path));
    }

    loop {
        let sources: std::collections::HashSet<std::path::PathBuf> =
            plan.iter().map(|(old, _)| old.clone()).collect();
        let before = plan.len();

        plan.retain(|(old, new)| {
            let blocked =
                fs.path_exists(new) && !sources.contains(new) && !is_same_entry(fs, old, new);
            if blocked {
                errors.push(rename_error(old, "destination already exists"));
            }
            !blocked
        });

        if plan.len() == before {
            return plan;
        }
    }
}

fn tmp_path(fs: &dyn crate::fs_backend::FileSystem, near: &std::path::Path) -> std::path::PathBuf {
    let parent = near.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut n = 0u32;
    loop {
        let candidate = parent.join(format!("{RENAME_TMP_PREFIX}{n}"));
        if !fs.path_exists(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn run_renames(
    fs: &dyn crate::fs_backend::FileSystem,
    plan: Vec<(std::path::PathBuf, std::path::PathBuf)>,
    errors: &mut Vec<String>,
) {
    let destinations: std::collections::HashSet<std::path::PathBuf> =
        plan.iter().map(|(_, new)| new.clone()).collect();
    let mut staged = Vec::new();

    for (old, new) in plan {
        if !destinations.contains(&old) {
            staged.push((old.clone(), old, new));
            continue;
        }

        let tmp = tmp_path(fs, &old);
        match fs.rename(&old, &tmp) {
            Ok(()) => staged.push((tmp, old, new)),
            Err(e) => errors.push(rename_error(&old, &e.message)),
        }
    }

    for (src, old, new) in staged {
        let result = if fs.path_exists(&new) && !is_same_entry(fs, &src, &new) {
            Err("destination already exists".to_string())
        } else {
            fs.rename(&src, &new).map_err(|e| e.message)
        };
        let Err(msg) = result else {
            continue;
        };

        errors.push(rename_error(&old, &msg));
        if src == old {
            continue;
        }
        if let Err(e) = fs.rename(&src, &old) {
            errors.push(rename_error(
                &old,
                &format!("could not restore from {}: {}", src.display(), e.message),
            ));
        }
    }
}

fn apply_directory_diff_to_fs(
    dir_path: &std::path::Path,
    diff: &crate::document::DirectoryDiff,
) -> Vec<String> {
    let fs = crate::fs_backend::backend();
    let mut errors: Vec<String> = Vec::new();

    let plan = plan_renames(fs, dir_path, &diff.renames, &mut errors);
    run_renames(fs, plan, &mut errors);

    for path in &diff.deletes {
        if let Err(e) = fs.delete_recursive(path) {
            errors.push(format!(
                "delete {}: {}",
                path.file_name().unwrap_or_default().display(),
                e.message
            ));
        }
    }

    for name in &diff.creates {
        let is_dir = name.ends_with('/');
        let clean_name = name.trim_end_matches('/');
        if std::path::Path::new(clean_name)
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        {
            errors.push(format!("create {clean_name:?}: path traversal not allowed"));
            continue;
        }
        let new_path = dir_path.join(clean_name);
        let result = if is_dir {
            fs.create_dir(&new_path)
        } else {
            fs.create_file(&new_path)
        };
        if let Err(e) = result {
            errors.push(format!(
                "create {}: {}",
                new_path.file_name().unwrap_or_default().display(),
                e.message
            ));
        }
    }

    errors
}

#[cfg(test)]
#[path = "explorer_tests.rs"]
mod explorer_tests;
