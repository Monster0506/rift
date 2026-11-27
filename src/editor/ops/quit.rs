use super::super::Editor;
use crate::error::{ErrorType, RiftError};
use crate::term::TerminalBackend;

#[derive(Clone, Copy)]
pub(in crate::editor) enum QuitRequest {
    Key,
    Close { force: bool },
    IfAlone,
}

impl<T: TerminalBackend> Editor<T> {
    pub(in crate::editor) fn quit(&mut self, request: QuitRequest) {
        let exit = match request {
            QuitRequest::Key => self.state.settings.editing().quit_on_q || self.close_scope(false),
            QuitRequest::Close { force } => self.close_scope(force),
            QuitRequest::IfAlone => self.can_quit_alone(),
        };
        self.should_quit |= exit;
    }

    pub(in crate::editor) fn detach_if_quitting(&mut self) {
        if self.should_quit && self.state.is_remote {
            self.term.request_detach();
            self.should_quit = false;
        }
    }

    fn can_quit_alone(&mut self) -> bool {
        if self.split_tree.window_count() > 1 || self.panel_layout.is_some() {
            return false;
        }
        if self.document_manager.has_unsaved_changes() {
            self.notify_unsaved_quit();
            return false;
        }
        true
    }

    fn notify_unsaved_quit(&mut self) {
        self.notify_error(RiftError::warning(
            ErrorType::Execution,
            crate::constants::errors::UNSAVED_CHANGES,
            crate::constants::errors::MSG_UNSAVED_CHANGES,
        ));
    }

    fn close_scope(&mut self, force: bool) -> bool {
        let in_clipboard_entry = self
            .document_manager
            .get_document(self.active_document_id())
            .is_some_and(crate::document::Document::is_clipboard_entry);
        if in_clipboard_entry {
            self.handle_clipboard_entry_close();
            return false;
        }

        let in_explorer = self.panel_layout.as_ref().is_some_and(|l| {
            let fid = self.split_tree.focused_window_id();
            fid == l.dir_win_id || fid == l.preview_win_id
        });
        if in_explorer {
            self.close_split_panel();
            return false;
        }
        if self.split_tree.window_count() > 1 {
            let focused_id = self.split_tree.focused_window_id();
            self.close_window(focused_id);
            if let Err(e) = self.force_full_redraw() {
                self.notify_error(e);
            }
        } else if self.document_manager.tab_count() <= 1 {
            if !force {
                let doc_id = self.active_document_id();
                if let Some(doc) = self.document_manager.get_document(doc_id) {
                    let confirm_dirty = doc.kind.descriptor.policies.close
                        == crate::document::runtime::ClosePolicy::ConfirmDirty;
                    if doc.is_dirty() && (confirm_dirty || !doc.is_special()) {
                        self.notify_unsaved_quit();
                        return false;
                    }
                }
            }
            return true;
        } else {
            let doc_id = self.active_document_id();
            let result = if force {
                self.close_buffer(doc_id, crate::document::manager::RemovalIntent::Force)
            } else {
                self.close_buffer(doc_id, crate::document::manager::RemovalIntent::Normal)
            };
            if let Err(e) = result {
                self.notify_error(e);
            } else if let Err(e) = self.force_full_redraw() {
                self.notify_error(e);
            }
        }
        false
    }
}
