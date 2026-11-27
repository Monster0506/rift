#[cfg(feature = "plugins")]
use super::super::scheduler::HeldSave;
use super::super::scheduler::SaveQuit;
use super::super::Editor;
use crate::document::runtime::{BufferKindId, DocumentHandle, SaveDispatch, SaveResult};
use crate::document::DocumentId;
use crate::error::{ErrorType, RiftError};
use crate::job_manager::jobs::file_operations::{FileSaveJob, FileSaveResult};
use crate::notification::NotificationType;
#[cfg(feature = "plugins")]
use crate::plugin::events::{EventPayload, SaveDecision, SaveRequestId};
use crate::term::TerminalBackend;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum SaveOutcome {
    Queued(usize),
    Saved,
    NotSaved,
}

pub(in crate::editor) enum CompletedSave {
    File {
        job: usize,
        handle: DocumentHandle,
        result: FileSaveResult,
    },
    Special {
        buf: DocumentId,
        path: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveKind {
    File,
    Special,
}

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn save_buffer(
        &mut self,
        buf: DocumentId,
        target: Option<PathBuf>,
        force: bool,
    ) -> SaveOutcome {
        self.save_buffer_with_quit(buf, target, force, None)
    }

    fn save_buffer_with_quit(
        &mut self,
        buf: DocumentId,
        target: Option<PathBuf>,
        force: bool,
        mut quit: Option<SaveQuit>,
    ) -> SaveOutcome {
        let target = target.map(|path| crate::fs_backend::backend().canonicalize(&path));
        if !self.save_target_allowed(buf, target.as_deref(), force) {
            return SaveOutcome::NotSaved;
        }
        let result = self.start_save(buf, target, force, &mut quit);
        self.state.clear_command_line();
        if let Some(quit) = quit {
            self.finish_save_quit(result, quit);
        }
        result
    }

    fn save_target_allowed(&mut self, buf: DocumentId, path: Option<&Path>, force: bool) -> bool {
        if !force
            && path.is_some_and(|path| {
                self.document_manager.get_document(buf).is_some_and(|doc| {
                    doc.path() != Some(path) && crate::fs_backend::backend().path_exists(path)
                })
            })
        {
            self.notify_error(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::FILE_EXISTS,
                crate::constants::errors::MSG_FILE_EXISTS,
            ));
            return false;
        }
        true
    }

    fn prepare_save(
        &mut self,
        buf: DocumentId,
        target: Option<PathBuf>,
    ) -> Option<(DocumentHandle, BufferKindId, SaveDispatch, Option<PathBuf>)> {
        let doc = self.document_manager.get_document_mut(buf)?;
        let handle = doc.handle();
        let kind = doc.buffer_kind_id();
        let dispatch = doc.descriptor().save_dispatch;
        match dispatch {
            SaveDispatch::Disabled => return None,
            SaveDispatch::Reject => {
                let message = format!("{} buffer cannot be saved", doc.display_name());
                self.notify_error(RiftError::new(ErrorType::Io, "CANT_SAVE", message));
                return None;
            }
            _ => {}
        }
        doc.commit_pending_ghost();
        let path = target.or_else(|| doc.path().map(Path::to_path_buf));
        if dispatch == SaveDispatch::File && path.is_none() {
            self.notify_error(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"));
            return None;
        }
        Some((handle, kind, dispatch, path))
    }

    #[cfg(not(feature = "plugins"))]
    fn start_save(
        &mut self,
        buf: DocumentId,
        target: Option<PathBuf>,
        _force: bool,
        _quit: &mut Option<SaveQuit>,
    ) -> SaveOutcome {
        match self.prepare_save(buf, target) {
            Some((handle, _, dispatch, path)) => self.write_save(handle, dispatch, path),
            None => SaveOutcome::NotSaved,
        }
    }

    #[cfg(feature = "plugins")]
    fn start_save(
        &mut self,
        buf: DocumentId,
        target: Option<PathBuf>,
        force: bool,
        quit: &mut Option<SaveQuit>,
    ) -> SaveOutcome {
        if self.scheduler.save_pre_in_progress.contains(&buf) {
            return SaveOutcome::NotSaved;
        }
        let Some((handle, kind, dispatch, path)) = self.prepare_save(buf, target) else {
            return SaveOutcome::NotSaved;
        };
        self.scheduler.held_saves.remove(&buf);
        self.scheduler.save_pre_in_progress.insert(buf);
        let decision = self.raise_save_pre(buf, path.clone());
        self.scheduler.save_pre_in_progress.remove(&buf);
        let Some(doc) = self.document_manager.get_document_by_handle(handle) else {
            self.notify_error(RiftError::new(
                ErrorType::Io,
                "SAVE_TARGET_CLOSED",
                "Buffer was closed by a plugin hook before it could be saved",
            ));
            return SaveOutcome::NotSaved;
        };
        match decision {
            SaveDecision::Proceed => self.write_save(handle, dispatch, path),
            SaveDecision::Cancel => SaveOutcome::NotSaved,
            SaveDecision::Hold(hold) => {
                if !self.should_quit && doc.matches_kind(kind) && hold.activate() {
                    self.scheduler.held_saves.insert(
                        buf,
                        HeldSave {
                            hold,
                            handle,
                            kind,
                            source_path: doc.path().map(Path::to_path_buf),
                            path,
                            force,
                            quit: quit.take(),
                        },
                    );
                }
                SaveOutcome::NotSaved
            }
        }
    }

    #[cfg(feature = "plugins")]
    pub(crate) fn resume_save(&mut self, request: &SaveRequestId) {
        let buf = request.buf;
        if self
            .scheduler
            .held_saves
            .get(&buf)
            .is_none_or(|save| !save.hold.matches_resume(request))
        {
            return;
        }
        let Some(save) = self.scheduler.held_saves.remove(&buf) else {
            return;
        };
        if self.should_quit {
            return;
        }
        if !self.save_target_allowed(buf, save.path.as_deref(), save.force) {
            return;
        }
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(save.handle)
        else {
            return;
        };
        if !doc.matches_kind(save.kind) || doc.path() != save.source_path.as_deref() {
            return;
        }
        let dispatch = doc.descriptor().save_dispatch;
        doc.commit_pending_ghost();

        let quit = save.quit.filter(|quit| {
            let current = self.active_document_id() == buf
                && self.split_tree.focused_window_id() == quit.window;
            if !current {
                self.notify_save_quit_aborted();
            }
            current
        });
        let result = self.write_save(save.handle, dispatch, save.path);
        if let Some(quit) = quit {
            self.finish_save_quit(result, quit);
        }
    }

    fn write_save(
        &mut self,
        handle: DocumentHandle,
        dispatch: SaveDispatch,
        path: Option<PathBuf>,
    ) -> SaveOutcome {
        let buf = handle.doc_id();
        let Some(doc) = self.document_manager.get_document_by_handle_mut(handle) else {
            return SaveOutcome::NotSaved;
        };
        let path = match (dispatch, path) {
            (SaveDispatch::File, Some(path)) => {
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
        self.finish_save(CompletedSave::Special { buf, path });
        SaveOutcome::Saved
    }

    pub(in crate::editor) fn finish_save(&mut self, completion: CompletedSave) {
        let (buf, path, file) = match completion {
            CompletedSave::Special { buf, path } => (buf, path, None),
            CompletedSave::File {
                job,
                handle,
                result,
            } => {
                let Some(doc) = self.document_manager.get_document_by_handle_mut(handle) else {
                    return;
                };
                doc.mark_as_saved(result.saved_seq);
                doc.set_path(&result.path);
                let display_name = doc.display_name().into_owned();
                self.persist_undo_after_save(result.document_id, result.content_hash);
                self.state.update_filename(display_name);
                self.notify(
                    NotificationType::Success,
                    format!("Written to {}", result.path.display()),
                );
                (result.document_id, Some(result.path), Some((job, handle)))
            }
        };
        #[cfg(feature = "plugins")]
        let path = {
            let (event_path, lsp_path) = if cfg!(feature = "lsp") && file.is_some() {
                (path.clone(), path)
            } else {
                (path, None)
            };
            self.raise(EventPayload::BufSavePost {
                buf,
                path: event_path,
            });
            lsp_path
        };
        #[cfg(feature = "lsp")]
        if let Some(path) = path {
            if file.is_some() {
                self.lsp_manager.did_save(&path, None);
            }
        }
        #[cfg(not(feature = "lsp"))]
        drop(path);
        if let Some((job, handle)) = file {
            if let Some((_, window)) = self.scheduler.quit_after_job.take_if(|(id, _)| *id == job) {
                self.quit_after_completed_save(handle, window, SaveKind::File);
            }
            self.sync_state_with_active_document();
            let _ = self.update_and_render();
        }
        #[cfg(not(feature = "plugins"))]
        let _ = buf;
    }

    pub(in crate::editor) fn cancel_quit_after_save(&mut self, job: usize) {
        if self
            .scheduler
            .quit_after_job
            .take_if(|(id, _)| *id == job)
            .is_some()
        {
            self.notify(
                NotificationType::Warning,
                "Quit aborted: save was cancelled",
            );
        }
    }

    fn save_special_buffer(&mut self, handle: DocumentHandle) -> SaveResult {
        let Some(doc) = self.document_manager.get_document_by_handle(handle) else {
            return SaveResult::Rejected;
        };
        match doc.descriptor().save_dispatch {
            #[cfg(feature = "plugins")]
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
            SaveDispatch::Directory => self.apply_directory_diff(handle.doc_id()),
            SaveDispatch::Clipboard => self.apply_clipboard_diff(handle.doc_id()),
            SaveDispatch::ClipboardEntry => self.apply_clipboard_entry_save(handle.doc_id()),
            SaveDispatch::GitCommitMessage => self.apply_git_commit_message(handle.doc_id()),
            SaveDispatch::GitRebaseTodo => self.apply_git_rebase_todo(handle.doc_id()),
            SaveDispatch::Disabled | SaveDispatch::Reject | SaveDispatch::File => {
                SaveResult::Rejected
            }
        }
    }

    pub(crate) fn save_and_quit(&mut self, target: Option<PathBuf>, force: bool) {
        let Some(doc) = self
            .document_manager
            .get_document(self.active_document_id())
        else {
            self.state.clear_command_line();
            return;
        };
        let handle = doc.handle();
        let implicit_target = target.is_none();
        let target = if implicit_target
            && doc.descriptor().save_dispatch == SaveDispatch::File
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
        let quit = SaveQuit {
            handle,
            window: self.split_tree.focused_window_id(),
            message,
        };
        self.save_buffer_with_quit(
            handle.doc_id(),
            target,
            force || implicit_target,
            Some(quit),
        );
    }

    fn finish_save_quit(&mut self, outcome: SaveOutcome, quit: SaveQuit) {
        match outcome {
            SaveOutcome::Queued(job) => {
                self.scheduler.quit_after_job = Some((job, quit.window));
                if let Some(message) = quit.message {
                    self.notify(NotificationType::Info, message);
                }
            }
            SaveOutcome::Saved => {
                self.quit_after_completed_save(quit.handle, quit.window, SaveKind::Special);
            }
            SaveOutcome::NotSaved => {}
        }
    }

    fn notify_save_quit_aborted(&mut self) {
        self.notify(
            NotificationType::Warning,
            "Quit aborted: buffer changed during save",
        );
    }

    fn quit_after_completed_save(
        &mut self,
        handle: DocumentHandle,
        window: crate::split::window::WindowId,
        kind: SaveKind,
    ) {
        let doc = self.document_manager.get_document_by_handle(handle);
        if self.split_tree.focused_window_id() == window
            && self.active_document_id() == handle.doc_id()
            && (kind == SaveKind::Special || doc.is_some_and(|doc| !doc.is_dirty()))
        {
            self.quit(super::quit::QuitRequest::Close { force: false });
        } else if kind == SaveKind::Special && doc.is_none() {
            self.quit(super::quit::QuitRequest::IfAlone);
        } else {
            self.notify_save_quit_aborted();
        }
    }
}
