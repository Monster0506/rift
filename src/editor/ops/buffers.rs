use super::super::Editor;
use crate::document::manager::{NewDocument, Placement};
use crate::document::DocumentId;
use crate::error::{ErrorType, RiftError};
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
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
        Ok(self.open_buffer(doc, Placement::Focused))
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
        {
            self.lsp_notify_close(id);
            if self.pending_goto_target.is_some_and(|(d, ..)| d == id) {
                self.pending_goto_target = None;
            }
        }

        self.file_load_jobs.retain(|_, target_id| *target_id != id);
        self.clear_git_gutter_state(id);
        self.pending_syntax_reparse.remove(&id);
        self.pending_git_status_expand_all.remove(&id);
        self.git_status_refresh_jobs.remove(&id);
        self.pending_git_log_expand_head.remove(&id);
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
                self.split_tree.set_window_document(win_id, doc_id);
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
