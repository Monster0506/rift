#[path = "../../document/manager.rs"]
pub(crate) mod manager;

use super::super::Editor;
use crate::document::DocumentId;
use crate::error::{ErrorType, RiftError};
use crate::term::TerminalBackend;
use manager::{CreationReservation, NewDocument, Placement};

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn select_buffer(&mut self, id: DocumentId) -> Result<(), RiftError> {
        self.document_manager.switch_to_document(id)
    }

    pub(crate) fn set_recent_ghost(&mut self, id: Option<DocumentId>) {
        self.document_manager.set_most_recent_ghost_doc(id);
    }

    pub(crate) fn claim_buffer_creation(&mut self, id: DocumentId) -> Option<CreationReservation> {
        self.document_manager.claim_plugin_creation(id)
    }

    #[cfg(test)]
    pub(crate) fn reserve_buffer_creation(&mut self) -> CreationReservation {
        self.document_manager.reserve_creation()
    }

    pub(crate) fn refill_plugin_creation_ids(&self, ids: Vec<u64>) -> Vec<u64> {
        self.document_manager.refill_plugin_ids(ids)
    }

    pub(crate) fn release_plugin_creation_ids(&self, ids: impl IntoIterator<Item = u64>) {
        self.document_manager.release_plugin_ids(ids);
    }

    pub(crate) fn clear_plugin_creation_ids(&self) {
        self.document_manager.clear_plugin_ids();
    }

    pub(crate) fn open_buffer(
        &mut self,
        new: impl Into<NewDocument>,
        placement: Placement,
    ) -> DocumentId {
        let handle = self.document_manager.insert(new, placement);
        if let Some(kind) = self
            .document_manager
            .get_document(handle.doc_id())
            .map(crate::document::Document::buffer_kind_id)
        {
            self.buffer_kinds.increment_open_count(kind);
        }
        handle.doc_id()
    }

    pub(crate) fn open_placeholder(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<DocumentId, crate::error::RiftError> {
        let mut doc = crate::document::Document::new(self.document_manager.next_id())?;
        doc.set_path(path.as_ref());
        Ok(self.open_buffer(doc, Placement::Background))
    }

    pub(crate) fn with_document_target<R>(
        &mut self,
        buf: DocumentId,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let win = self.split_tree.focused_window_id();
        let original = self.active_document_id();
        let original_tab = self.document_manager.active_document_id();
        let scroll = self.render_system.viewport.get_scroll();
        self.show_in_window(win, buf);
        let _ = self.select_buffer(buf);
        self.sync_state_with_active_document();
        let result = f(self);
        if original != buf && self.document_manager.get_document(original).is_some() {
            if self
                .split_tree
                .get_window(win)
                .is_some_and(|w| w.document_id == buf)
            {
                self.show_in_window(win, original);
                if self.split_tree.focused_window_id() == win {
                    self.render_system.viewport.set_scroll(scroll.0, scroll.1);
                }
            }
            if self.document_manager.active_document_id() == Some(buf) {
                if let Some(tab) = original_tab {
                    let _ = self.select_buffer(tab);
                }
            }
            self.sync_state_with_active_document();
        }
        result
    }

    pub(crate) fn focus_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        let previous = self.split_tree.focused_window().document_id;
        if previous != id {
            let (top_line, left_col) = self.render_system.viewport.get_scroll();
            if let Some(doc) = self.document_manager.get_document_mut(previous) {
                doc.save_view_state(top_line, left_col);
            }
        }
        self.select_buffer(id)?;
        self.show_in_window(self.split_tree.focused_window_id(), id);
        if previous != id {
            self.restore_view_state();
        }
        self.sync_state_with_active_document();
        Ok(())
    }

    pub(crate) fn close_buffer(
        &mut self,
        id: DocumentId,
        intent: crate::document::manager::RemovalIntent,
    ) -> Result<(), RiftError> {
        let plan = self.document_manager.prepare_removal(id, intent)?;
        let (handle, kind_id, descriptor) = {
            let document = self.document_manager.get_document(id).ok_or_else(|| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    format!("Document {id} not found"),
                )
            })?;
            (
                document.handle(),
                document.buffer_kind_id(),
                std::sync::Arc::clone(&document.kind.descriptor),
            )
        };

        self.job_manager.cancel_jobs_for_handle(handle);
        match descriptor.on_close {
            crate::document::runtime::CloseHandler::Native(close) => {
                close(handle);
                self.apply_plugin_mutations();
            }
            crate::document::runtime::CloseHandler::Lua => {
                self.call_plugins(|editor| {
                    editor
                        .plugin_host
                        .invoke_buffer_close(id, descriptor.name())
                });
            }
        }

        #[cfg(feature = "lsp")]
        self.lsp_notify_close(id);

        self.file_load_jobs.retain(|_, target_id| *target_id != id);
        self.clear_git_gutter_state(id);
        self.scheduler.forget_document(id);
        self.git_status_refresh_jobs.remove(&id);
        self.display_map_cache.retain(|entry| entry.doc_id != id);
        if self
            .search_highlights_synced
            .as_ref()
            .is_some_and(|(d, ..)| *d == id)
        {
            self.search_highlights_synced = None;
        }

        if plan.needs_replacement() {
            let replacement = crate::document::Document::new(self.document_manager.next_id())?;
            self.open_buffer(replacement, crate::document::manager::Placement::Focused);
        }
        self.document_manager.commit_removal(plan);
        self.buffer_kinds.decrement_open_count(kind_id);
        if let Some(doc_id) = self.document_manager.active_document_id() {
            for win_id in self.split_tree.windows_for_document(id) {
                self.show_in_window(win_id, doc_id);
            }
        }
        self.sync_state_with_active_document();

        self.call_plugins(|editor| {
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::BufClose { buf: id });
        });

        Ok(())
    }
}
