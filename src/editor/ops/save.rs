use super::super::Editor;
use crate::document::runtime::{BufferKindId, DocumentHandle, SaveDispatch, SaveResult};
use crate::document::DocumentId;
use crate::error::{ErrorType, RiftError};
use crate::job_manager::jobs::file_operations::FileSaveJob;
use crate::notification::NotificationType;
use crate::plugin::events::EventPayload;
use crate::term::TerminalBackend;
use std::path::{Path, PathBuf};

pub(crate) enum SaveOutcome {
    Queued(usize),
    Saved,
    NotSaved,
}

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn save_buffer(
        &mut self,
        buf: DocumentId,
        target: Option<PathBuf>,
        force: bool,
    ) -> SaveOutcome {
        let fs = crate::fs_backend::backend();
        let target = target.map(|path| fs.canonicalize(&path));
        if !force
            && target.as_deref().is_some_and(|path| {
                self.document_manager
                    .get_document(buf)
                    .is_some_and(|doc| doc.path() != Some(path) && fs.path_exists(path))
            })
        {
            self.notify_error(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::FILE_EXISTS,
                crate::constants::errors::MSG_FILE_EXISTS,
            ));
            return SaveOutcome::NotSaved;
        }
        let result = self.start_save(buf, target);
        self.state.clear_command_line();
        result
    }

    fn start_save(&mut self, buf: DocumentId, target: Option<PathBuf>) -> SaveOutcome {
        let Some(doc) = self.document_manager.get_document_mut(buf) else {
            return SaveOutcome::NotSaved;
        };
        let handle = doc.handle();
        let file = doc.buffer_kind_id() == BufferKindId::FILE;
        if !file {
            match doc.descriptor().save_dispatch {
                SaveDispatch::Disabled => return SaveOutcome::NotSaved,
                SaveDispatch::Reject => {
                    let message = format!("{} buffer cannot be saved", doc.display_name());
                    self.notify_error(RiftError::new(ErrorType::Io, "CANT_SAVE", message));
                    return SaveOutcome::NotSaved;
                }
                SaveDispatch::Lua | SaveDispatch::Native(_) => {}
            }
        }
        doc.commit_pending_ghost();
        let path = target.or_else(|| doc.path().map(Path::to_path_buf));
        if file && path.is_none() {
            self.notify_error(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"));
            return SaveOutcome::NotSaved;
        }
        self.raise(EventPayload::BufSavePre {
            buf,
            path: path.clone(),
        });
        let Some(doc) = self.document_manager.get_document_by_handle_mut(handle) else {
            self.notify_error(RiftError::new(
                ErrorType::Io,
                "SAVE_TARGET_CLOSED",
                "Buffer was closed by a plugin hook before it could be saved",
            ));
            return SaveOutcome::NotSaved;
        };
        let path = match (file, path) {
            (true, Some(path)) => {
                let job = FileSaveJob::new(
                    buf,
                    doc.buffer().line_index.table.clone(),
                    path,
                    doc.options().line_ending(),
                    doc.options().has_bom(),
                    doc.history.current_seq(),
                );
                return SaveOutcome::Queued(self.job_manager.spawn(job));
            }
            (_, path) => path,
        };
        if let Some(path) = &path {
            doc.set_path(path);
        }
        if self.save_special_buffer(handle) != SaveResult::Saved {
            return SaveOutcome::NotSaved;
        }
        self.raise(EventPayload::BufSavePost { buf, path });
        SaveOutcome::Saved
    }

    fn save_special_buffer(&mut self, handle: DocumentHandle) -> SaveResult {
        let Some(doc) = self.document_manager.get_document_by_handle(handle) else {
            return SaveResult::Rejected;
        };
        match doc.descriptor().save_dispatch {
            SaveDispatch::Lua => {
                let descriptor = std::sync::Arc::clone(&doc.kind.descriptor);
                let saved = self.call_plugins(|editor| {
                    editor
                        .plugin_host
                        .invoke_buffer_save(handle.doc_id(), descriptor.name())
                });
                if saved {
                    SaveResult::Saved
                } else {
                    SaveResult::Failed
                }
            }
            SaveDispatch::Native(handler) => {
                if let Some(save) = self
                    .native_save_handlers
                    .get(&doc.buffer_kind_id())
                    .copied()
                {
                    self.with_document_target(handle.doc_id(), save)
                } else {
                    handler(handle)
                }
            }
            SaveDispatch::Disabled | SaveDispatch::Reject => SaveResult::Rejected,
        }
    }

    pub(crate) fn save_and_quit(&mut self, target: Option<PathBuf>, force: bool) {
        let Some(doc) = self.document_manager.active_document() else {
            self.state.clear_command_line();
            return;
        };
        let handle = doc.handle();
        let implicit_target = target.is_none();
        let target = if implicit_target
            && doc.buffer_kind_id() == BufferKindId::FILE
            && doc.path().is_none()
        {
            self.state.file_path.as_ref().map(PathBuf::from)
        } else {
            target
        };
        let message = target
            .as_deref()
            .or_else(|| doc.path())
            .map(|path| format!("Saving {} and quitting...", path.display()));
        match self.save_buffer(handle.doc_id(), target, force || implicit_target) {
            SaveOutcome::Queued(job) => {
                self.scheduler.quit_after_job = Some(job);
                if let Some(message) = message {
                    self.notify(NotificationType::Info, message);
                }
            }
            SaveOutcome::Saved if self.active_document_id() == handle.doc_id() => {
                self.do_quit(false);
            }
            SaveOutcome::Saved
                if self
                    .document_manager
                    .get_document_by_handle(handle)
                    .is_none() =>
            {
                self.quit_if_alone();
            }
            SaveOutcome::Saved => {
                self.notify(
                    NotificationType::Warning,
                    "Quit aborted: buffer changed during save",
                );
            }
            SaveOutcome::NotSaved => {}
        }
    }

    fn quit_if_alone(&mut self) {
        if self.split_tree.window_count() > 1 || self.panel_layout.is_some() {
            return;
        }
        if self.document_manager.has_unsaved_changes() {
            self.notify_error(RiftError::warning(
                ErrorType::Execution,
                crate::constants::errors::UNSAVED_CHANGES,
                crate::constants::errors::MSG_UNSAVED_CHANGES,
            ));
            return;
        }
        self.should_quit = true;
    }

    pub(crate) fn quit_after_save(&mut self, doc_id: DocumentId) {
        let unchanged = self.active_document_id() == doc_id
            && self
                .document_manager
                .get_document(doc_id)
                .is_some_and(|doc| !doc.is_dirty());
        if !unchanged {
            self.notify(
                NotificationType::Warning,
                "Quit aborted: buffer changed during save",
            );
            return;
        }
        self.do_quit(false);
    }
}
