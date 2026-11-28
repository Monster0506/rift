#[cfg(feature = "plugins")]
use super::super::scheduler::HeldSave;
use super::super::scheduler::{FileSaveRequest, QueuedFileSave, SaveQuit};
use super::super::Editor;
use crate::document::runtime::{BufferKindId, DocumentHandle, SaveDispatch, SaveResult};
use crate::document::DocumentId;
use crate::error::{ErrorType, RiftError};
use crate::job_manager::jobs::file_operations::{FileSaveJob, FileSaveResult};
use crate::job_manager::{AsyncOpDomain, AsyncToken};
use crate::notification::NotificationType;
#[cfg(feature = "plugins")]
use crate::plugin::events::{EventPayload, SaveDecision, SaveRequestId};
use crate::term::TerminalBackend;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(crate) enum SaveOutcome {
    Queued,
    Saved,
    NotSaved,
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
            self.finish_save_quit(result, &quit);
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
        force: bool,
        quit: &mut Option<SaveQuit>,
    ) -> SaveOutcome {
        match self.prepare_save(buf, target) {
            Some((handle, _, dispatch, path)) => {
                self.write_save(handle, dispatch, path, force, quit)
            }
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
            SaveDecision::Proceed => self.write_save(handle, dispatch, path, force, quit),
            SaveDecision::Cancel => SaveOutcome::NotSaved,
            SaveDecision::Hold(hold) => {
                if !self.exit_requested() && doc.matches_kind(kind) && hold.activate() {
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
        if self.exit_requested() {
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

        let mut quit = save.quit.filter(|quit| {
            let current = self.active_document_id() == buf
                && self.split_tree.focused_window_id() == quit.window;
            if !current {
                self.notify_save_quit_aborted();
            }
            current
        });
        let result = self.write_save(save.handle, dispatch, save.path, save.force, &mut quit);
        if let Some(quit) = quit {
            self.finish_save_quit(result, &quit);
        }
    }

    fn write_save(
        &mut self,
        handle: DocumentHandle,
        dispatch: SaveDispatch,
        path: Option<PathBuf>,
        force: bool,
        quit: &mut Option<SaveQuit>,
    ) -> SaveOutcome {
        let buf = handle.doc_id();
        let Some(doc) = self.document_manager.get_document_by_handle_mut(handle) else {
            return SaveOutcome::NotSaved;
        };
        let path = match (dispatch, path) {
            (SaveDispatch::File, Some(path)) => {
                doc.checkpoint_edit_session();
                let request = FileSaveRequest {
                    handle,
                    kind: doc.buffer_kind_id(),
                    destination: save_destination(&path),
                    quit: None,
                    detached_history: None,
                };
                let job = FileSaveJob::new(
                    buf,
                    doc.buffer().line_index.table.clone(),
                    path,
                    doc.options().line_ending(),
                    doc.options().has_bom(),
                    doc.history.current_seq(),
                );
                self.enqueue_file_save(job, request, quit.take());
                return SaveOutcome::Queued;
            }
            (_, path) => path,
        };
        if let Some(path) = &path {
            doc.set_path(path);
        }
        if self.save_special_buffer(handle, force) != SaveResult::Saved {
            return SaveOutcome::NotSaved;
        }
        self.deliver_save_post(buf, path, false);
        SaveOutcome::Saved
    }

    fn enqueue_file_save(
        &mut self,
        job: FileSaveJob,
        mut request: FileSaveRequest,
        quit: Option<SaveQuit>,
    ) {
        let mut superseded_quit = false;
        for existing in self.scheduler.running_file_saves.values_mut().chain(
            self.scheduler
                .queued_file_saves
                .iter_mut()
                .map(|save| &mut save.request),
        ) {
            if existing.handle == request.handle {
                superseded_quit |= existing.quit.take().is_some();
            }
        }
        if superseded_quit {
            self.notify_save_quit_aborted();
        }
        if let Some(quit) = quit {
            request.quit = Some(quit.window);
            if let Some(message) = quit.message {
                self.notify(NotificationType::Info, message);
            }
        }
        self.scheduler
            .queued_file_saves
            .push_back(QueuedFileSave { job, request });
        self.start_queued_file_saves();
    }

    fn start_queued_file_saves(&mut self) {
        let mut index = 0;
        while let Some(save) = self.scheduler.queued_file_saves.get(index) {
            let request = &save.request;
            if self
                .scheduler
                .running_file_saves
                .values()
                .chain(
                    self.scheduler
                        .queued_file_saves
                        .iter()
                        .take(index)
                        .map(|save| &save.request),
                )
                .any(|other| request.conflicts_with(other))
            {
                index += 1;
                continue;
            }
            let Some(save) = self.scheduler.queued_file_saves.remove(index) else {
                break;
            };
            let request = save.request;
            if self
                .document_manager
                .get_document_by_handle(request.handle)
                .is_none_or(|doc| !doc.matches_kind(request.kind))
            {
                if request.quit.is_some() {
                    self.notify_save_quit_aborted();
                }
                continue;
            }
            self.job_manager
                .register_document_handle(request.handle, request.kind);
            let id = self.job_manager.spawn(save.job);
            self.scheduler.running_file_saves.insert(id, request);
        }
    }

    pub(in crate::editor) fn settle_file_save(&mut self, job: usize) {
        if let Some(save) = self.scheduler.running_file_saves.remove(&job) {
            if save.quit.is_some() {
                self.notify(
                    NotificationType::Warning,
                    "Quit aborted: save did not complete",
                );
            }
            self.start_queued_file_saves();
        }
    }

    pub(in crate::editor) fn preserve_file_save_history(&mut self, handle: DocumentHandle) {
        let mut saves = self
            .scheduler
            .running_file_saves
            .values_mut()
            .chain(
                self.scheduler
                    .queued_file_saves
                    .iter_mut()
                    .map(|save| &mut save.request),
            )
            .filter(|save| save.handle == handle && save.detached_history.is_none())
            .peekable();
        if saves.peek().is_none() {
            return;
        }
        if let Some(doc) = self.document_manager.get_document_by_handle_mut(handle) {
            let history = Arc::new(std::mem::take(&mut doc.history));
            for save in saves {
                save.detached_history = Some(Arc::clone(&history));
            }
        }
    }

    pub(in crate::editor) fn finish_file_save(
        &mut self,
        job: usize,
        token: Option<AsyncToken>,
        result: FileSaveResult,
    ) {
        let Some(save) = self.scheduler.running_file_saves.get(&job) else {
            return;
        };
        if !self.job_manager.is_job_running(job)
            || token != self.job_manager.job_token(job)
            || !token.is_some_and(|token| {
                token.handle == save.handle
                    && token.expected_kind == save.kind
                    && token.domain == AsyncOpDomain::FileSave
                    && token.doc_id() == result.document_id
            })
        {
            return;
        }
        let handle = save.handle;
        let Some(doc) = self.document_manager.get_document_by_handle_mut(handle) else {
            return;
        };
        let live = save.detached_history.is_none()
            && doc.matches_kind(save.kind)
            && doc.history.nodes.contains_key(&result.saved_seq);
        if live {
            doc.mark_as_saved(result.saved_seq);
            doc.set_path(&result.path);
        }
        if let Some(history) = &save.detached_history {
            let path = crate::fs_backend::backend().canonicalize(&result.path);
            self.persist_undo_after_save(&path, history, result.saved_seq, result.content_hash);
        } else if live {
            let Some(doc) = self.document_manager.get_document_by_handle(handle) else {
                return;
            };
            self.persist_undo_after_save(
                doc.path().unwrap_or(&result.path),
                &doc.history,
                result.saved_seq,
                result.content_hash,
            );
        }
        self.notify(
            NotificationType::Success,
            format!("Written to {}", result.path.display()),
        );
        self.deliver_save_post(result.document_id, Some(result.path), true);
        if let Some(save) = self.scheduler.running_file_saves.get_mut(&job) {
            if let Some(window) = save.quit.take() {
                if live
                    && !self
                        .scheduler
                        .queued_file_saves
                        .iter()
                        .any(|pending| pending.request.destination == save.destination)
                {
                    self.quit_after_completed_save(handle, window, SaveKind::File);
                } else {
                    self.notify_save_quit_aborted();
                }
            }
        }
        self.request_frame();
    }

    fn deliver_save_post(&mut self, buf: DocumentId, path: Option<PathBuf>, file: bool) {
        #[cfg(feature = "plugins")]
        let path = {
            let (event_path, lsp_path) = if cfg!(feature = "lsp") && file {
                (path.clone(), path)
            } else {
                (path, None)
            };
            self.deliver_buffer_open(buf);
            self.raise(EventPayload::BufSavePost {
                buf,
                path: event_path,
            });
            lsp_path
        };
        #[cfg(feature = "lsp")]
        if file {
            if let Some(path) = path {
                self.lsp_manager.did_save(&path, None);
            }
        }
        #[cfg(not(feature = "lsp"))]
        let _ = (path, file);
        #[cfg(not(feature = "plugins"))]
        let _ = buf;
    }

    fn save_special_buffer(&mut self, handle: DocumentHandle, force: bool) -> SaveResult {
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
            SaveDispatch::Directory => self.apply_directory_diff(handle.doc_id(), force),
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
        let force =
            force || (target.is_none() && doc.descriptor().save_dispatch == SaveDispatch::File);
        let message = target
            .as_deref()
            .or_else(|| doc.path())
            .map(|path| format!("Saving {} and quitting...", path.display()));
        let quit = SaveQuit {
            handle,
            window: self.split_tree.focused_window_id(),
            message,
        };
        self.save_buffer_with_quit(handle.doc_id(), target, force, Some(quit));
    }

    fn finish_save_quit(&mut self, outcome: SaveOutcome, quit: &SaveQuit) {
        if matches!(outcome, SaveOutcome::Saved) {
            self.quit_after_completed_save(quit.handle, quit.window, SaveKind::Special);
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
            && (kind == SaveKind::Special
                || doc.is_some_and(|doc| !doc.is_dirty() && !doc.has_pending_edits()))
        {
            self.quit(super::quit::QuitRequest::Close { force: false });
        } else if kind == SaveKind::Special && doc.is_none() {
            self.quit(super::quit::QuitRequest::IfAlone);
        } else {
            self.notify_save_quit_aborted();
        }
    }
}

fn save_destination(path: &Path) -> PathBuf {
    let canonical = path
        .parent()
        .and_then(|parent| std::fs::canonicalize(parent).ok())
        .zip(path.file_name())
        .map(|(parent, name)| parent.join(name));
    let destination = canonical.as_deref().unwrap_or(path);
    if cfg!(windows) {
        PathBuf::from(destination.to_string_lossy().to_lowercase())
    } else {
        canonical.unwrap_or_else(|| path.to_path_buf())
    }
}
