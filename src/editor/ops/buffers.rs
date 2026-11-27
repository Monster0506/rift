#[path = "../../document/manager.rs"]
pub(crate) mod manager;

use super::super::Editor;
use crate::document::DocumentId;
use crate::error::{ErrorType, RiftError};
use crate::term::TerminalBackend;
use manager::{CreationReservation, NewDocument, Placement};

impl<T: TerminalBackend> Editor<T> {
    fn select_buffer(&mut self, id: DocumentId) -> Result<(), RiftError> {
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
        let id = self.insert_buffer(new, placement);
        self.announce_buffer(id);
        id
    }

    pub(crate) fn insert_buffer(
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

    pub(crate) fn announce_buffer(&mut self, id: DocumentId) {
        let Some(kind) = self
            .document_manager
            .get_document(id)
            .map(crate::document::Document::buffer_kind_id)
        else {
            return;
        };
        if kind == crate::document::runtime::BufferKindId::FILE {
            #[cfg(feature = "treesitter")]
            self.attach_syntax_for_document(id);
            if self
                .document_manager
                .get_document(id)
                .is_some_and(|d| d.syntax.is_some())
            {
                self.spawn_syntax_parse_job(id);
            }
            #[cfg(feature = "lsp")]
            self.lsp_notify_open(id);
        }
        let Some(doc) = self.document_manager.get_document(id) else {
            return;
        };
        let path = doc.path().map(std::path::Path::to_path_buf);
        let filetype = doc
            .syntax
            .as_ref()
            .map(|s| s.language_name.clone())
            .or_else(|| {
                path.as_deref()
                    .and_then(|p| self.language_loader.language_name_for_file(p))
            });
        self.raise(crate::plugin::events::EventPayload::BufOpen {
            buf: id,
            path,
            filetype,
        });
    }

    pub(crate) fn open_placeholder(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<DocumentId, crate::error::RiftError> {
        let mut doc = crate::document::Document::new(self.document_manager.next_id())?;
        doc.set_path(path.as_ref());
        Ok(self.insert_buffer(doc, Placement::Background))
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
        let window = self.split_tree.focused_window_id();
        let previous = self.active_document_id();
        self.focus_document_with(id, |editor| {
            editor.show_focused_buffer(window, previous, id);
        })
    }

    fn show_focused_buffer(
        &mut self,
        window: crate::split::window::WindowId,
        previous: DocumentId,
        id: DocumentId,
    ) {
        if previous != id {
            let (top_line, left_col) = self.render_system.viewport.get_scroll();
            if let Some(doc) = self.document_manager.get_document_mut(previous) {
                doc.save_view_state(top_line, left_col);
            }
            self.show_in_window(window, id);
            self.restore_view_state();
        }
    }

    pub(super) fn focus_document_with(
        &mut self,
        id: DocumentId,
        update_view: impl FnOnce(&mut Self),
    ) -> Result<(), RiftError> {
        if self.document_manager.get_document(id).is_some() {
            self.leave_entered_document(id);
        }
        self.select_buffer(id)?;
        update_view(self);
        self.sync_state_with_active_document();
        if self.active_document_id() == id {
            self.enter_document(id);
        }
        Ok(())
    }

    fn leave_entered_document(&mut self, next: DocumentId) {
        let left = self.entered_document;
        if left == next || self.document_manager.get_document(left).is_none() {
            return;
        }
        self.raise(crate::plugin::events::EventPayload::BufLeave { buf: left });
    }

    fn enter_document(&mut self, id: DocumentId) {
        if self.entered_document == id {
            return;
        }
        self.entered_document = id;
        self.raise(crate::plugin::events::EventPayload::BufEnter { buf: id });
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

        self.deliver_open_before_close(id);
        if self
            .document_manager
            .get_document_by_handle(handle)
            .is_none()
        {
            return Ok(());
        }
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

        if self
            .document_manager
            .get_document_by_handle(handle)
            .is_none()
        {
            return Ok(());
        }
        if self.active_document_id() == id {
            let replacement = if let Some(next) = plan.replacement {
                next
            } else {
                let doc = crate::document::Document::new(self.document_manager.next_id())?;
                self.open_buffer(doc, Placement::Background)
            };
            let window = self.split_tree.focused_window_id();
            self.focus_document_with(replacement, |editor| {
                editor.show_focused_buffer(window, id, replacement);
                editor.remove_buffer(plan, kind_id, id, replacement);
            })
        } else {
            self.remove_buffer(plan, kind_id, id, self.active_document_id());
            Ok(())
        }
    }

    fn remove_buffer(
        &mut self,
        plan: manager::PreparedRemoval,
        kind_id: crate::document::runtime::BufferKindId,
        id: DocumentId,
        replacement: DocumentId,
    ) {
        for window in self.split_tree.windows_for_document(id) {
            self.show_in_window(window, replacement);
        }
        if self.document_manager.commit_removal(plan).is_none() {
            return;
        }
        self.buffer_kinds.decrement_open_count(kind_id);
        self.sync_state_with_active_document();

        self.raise(crate::plugin::events::EventPayload::BufClose { buf: id });
    }
}
