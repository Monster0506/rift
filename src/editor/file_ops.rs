use super::Editor;
use crate::error::{ErrorType, RiftError};
use crate::term::TerminalBackend;

pub(super) type NativeSaveHandler<T> = fn(&mut Editor<T>) -> crate::document::runtime::SaveResult;

fn save_directory<T: TerminalBackend>(
    editor: &mut Editor<T>,
) -> crate::document::runtime::SaveResult {
    editor.apply_directory_diff();
    crate::document::runtime::SaveResult::Saved
}

fn save_commit_message<T: TerminalBackend>(
    editor: &mut Editor<T>,
) -> crate::document::runtime::SaveResult {
    editor.apply_git_commit_message();
    crate::document::runtime::SaveResult::Saved
}

fn save_clipboard<T: TerminalBackend>(
    editor: &mut Editor<T>,
) -> crate::document::runtime::SaveResult {
    editor.apply_clipboard_diff()
}

fn save_clipboard_entry<T: TerminalBackend>(
    editor: &mut Editor<T>,
) -> crate::document::runtime::SaveResult {
    editor.apply_clipboard_entry_save()
}

fn save_rebase<T: TerminalBackend>(editor: &mut Editor<T>) -> crate::document::runtime::SaveResult {
    editor.apply_git_rebase_todo();
    crate::document::runtime::SaveResult::Saved
}

pub(super) fn native_save_handlers<T: TerminalBackend>(
) -> std::collections::HashMap<crate::document::runtime::BufferKindId, NativeSaveHandler<T>> {
    use crate::document::runtime::BufferKindId;

    let mut handlers = std::collections::HashMap::new();
    handlers.insert(
        BufferKindId::DIRECTORY,
        save_directory::<T> as NativeSaveHandler<T>,
    );
    handlers.insert(
        BufferKindId::GIT_COMMIT_MESSAGE,
        save_commit_message::<T> as NativeSaveHandler<T>,
    );
    handlers.insert(
        BufferKindId::CLIPBOARD,
        save_clipboard::<T> as NativeSaveHandler<T>,
    );
    handlers.insert(
        BufferKindId::CLIPBOARD_ENTRY,
        save_clipboard_entry::<T> as NativeSaveHandler<T>,
    );
    handlers.insert(
        BufferKindId::GIT_REBASE_TODO,
        save_rebase::<T> as NativeSaveHandler<T>,
    );
    handlers
}

// One table holds both shapes; only the undo-file and image views can fail.
pub(super) enum NativeReloadHandler<T: TerminalBackend> {
    Infallible(fn(&mut Editor<T>)),
    Fallible(fn(&mut Editor<T>) -> Result<(), RiftError>),
}

// Manual impls: derive would require `T: Copy`, which the backend does not satisfy.
impl<T: TerminalBackend> Clone for NativeReloadHandler<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: TerminalBackend> Copy for NativeReloadHandler<T> {}

fn reload_undo_file_view_handler<T: TerminalBackend>(
    editor: &mut Editor<T>,
) -> Result<(), RiftError> {
    let doc_id = editor.active_document_id();
    let Some(doc) = editor.document_manager.get_document_mut(doc_id) else {
        return Ok(());
    };
    doc.reload_undo_file_view()
}

/// `:e` on an image buffer reopens the same file as plain text (raw bytes).
fn reopen_image_as_text<T: TerminalBackend>(editor: &mut Editor<T>) -> Result<(), RiftError> {
    let doc_id = editor.active_document_id();
    let Some(doc) = editor.document_manager.get_document_mut(doc_id) else {
        return Ok(());
    };
    let path = doc
        .path()
        .ok_or_else(|| {
            RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::NO_PATH,
                crate::constants::errors::MSG_NO_FILE_NAME,
            )
        })?
        .to_path_buf();
    doc.convert_to_file();
    doc.replace_content("", crate::document::ops::ContentKind::Load);
    // The load's async token records the kind at spawn; it must be FILE now.
    let (handle, kind) = (doc.handle(), doc.buffer_kind_id());
    editor
        .job_manager
        .register_document_handle(handle, kind, None);
    let job = crate::job_manager::jobs::file_operations::FileLoadJob::new_text(doc_id, path);
    let job_id = editor.job_manager.spawn(job);
    editor.file_load_jobs.insert(job_id, doc_id);
    Ok(())
}

fn reload_directory<T: TerminalBackend>(editor: &mut Editor<T>) {
    editor.handle_explorer_refresh();
}

fn reload_undotree<T: TerminalBackend>(editor: &mut Editor<T>) {
    editor.handle_undotree_refresh();
}

fn reload_messages<T: TerminalBackend>(editor: &mut Editor<T>) {
    editor.refresh_messages_buffer_if_open();
}

fn reload_clipboard<T: TerminalBackend>(editor: &mut Editor<T>) {
    editor.refresh_clipboard_buffer_if_open();
}

fn reload_git_status<T: TerminalBackend>(editor: &mut Editor<T>) {
    let doc_id = editor.active_document_id();
    let Some(repo_root) = editor
        .document_manager
        .get_document(doc_id)
        .and_then(|d| d.git_repo_root())
        .map(std::path::Path::to_path_buf)
    else {
        return;
    };
    editor.refresh_git_status_buffers_for(&repo_root);
}

fn reload_git_log<T: TerminalBackend>(editor: &mut Editor<T>) {
    let doc_id = editor.active_document_id();
    let Some((repo_root, path)) = editor.document_manager.get_document(doc_id).and_then(|d| {
        let repo_root = d.git_repo_root()?.to_path_buf();
        let path = d.git_log_path().flatten().map(std::path::Path::to_path_buf);
        Some((repo_root, path))
    }) else {
        return;
    };
    let job = crate::job_manager::jobs::git::GitLogJob::new(doc_id, repo_root, path);
    editor.job_manager.spawn(job);
}

fn reload_git_blame<T: TerminalBackend>(editor: &mut Editor<T>) {
    let doc_id = editor.active_document_id();
    let Some((repo_root, path, at_commit)) =
        editor.document_manager.get_document(doc_id).and_then(|d| {
            let repo_root = d.git_repo_root()?.to_path_buf();
            let path = d.git_blame_path()?.to_path_buf();
            let at_commit = d.git_blame_at_commit().map(str::to_string);
            Some((repo_root, path, at_commit))
        })
    else {
        return;
    };
    let job = crate::job_manager::jobs::git::GitBlameJob::new(doc_id, repo_root, path, at_commit);
    editor.job_manager.spawn(job);
}

fn reload_buffer_list<T: TerminalBackend>(editor: &mut Editor<T>) {
    editor.refresh_buffer_list_panel();
}

pub(super) fn native_reload_handlers<T: TerminalBackend>(
) -> std::collections::HashMap<crate::document::runtime::BufferKindId, NativeReloadHandler<T>> {
    use crate::document::runtime::BufferKindId;

    let mut handlers = std::collections::HashMap::new();
    handlers.insert(
        BufferKindId::UNDO_FILE_VIEW,
        NativeReloadHandler::Fallible(reload_undo_file_view_handler::<T>),
    );
    handlers.insert(
        BufferKindId::IMAGE_VIEW,
        NativeReloadHandler::Fallible(reopen_image_as_text::<T>),
    );
    handlers.insert(
        BufferKindId::DIRECTORY,
        NativeReloadHandler::Infallible(reload_directory::<T>),
    );
    handlers.insert(
        BufferKindId::UNDO_TREE,
        NativeReloadHandler::Infallible(reload_undotree::<T>),
    );
    handlers.insert(
        BufferKindId::MESSAGES,
        NativeReloadHandler::Infallible(reload_messages::<T>),
    );
    handlers.insert(
        BufferKindId::CLIPBOARD,
        NativeReloadHandler::Infallible(reload_clipboard::<T>),
    );
    handlers.insert(
        BufferKindId::GIT_STATUS,
        NativeReloadHandler::Infallible(reload_git_status::<T>),
    );
    handlers.insert(
        BufferKindId::GIT_LOG,
        NativeReloadHandler::Infallible(reload_git_log::<T>),
    );
    handlers.insert(
        BufferKindId::GIT_BLAME,
        NativeReloadHandler::Infallible(reload_git_blame::<T>),
    );
    handlers.insert(
        BufferKindId::BUFFER_LIST,
        NativeReloadHandler::Infallible(reload_buffer_list::<T>),
    );
    handlers
}

impl<T: TerminalBackend> Editor<T> {
    fn dispatch_save_sync(
        &mut self,
        buf: crate::document::DocumentId,
    ) -> crate::document::runtime::SaveResult {
        let Some((doc_id, kind_id, kind_name, dispatch)) =
            self.document_manager.get_document(buf).map(|document| {
                (
                    document.id,
                    document.buffer_kind_id(),
                    document.descriptor().name().to_string(),
                    document.descriptor().save_dispatch,
                )
            })
        else {
            self.state.clear_command_line();
            return crate::document::runtime::SaveResult::Rejected;
        };

        match dispatch {
            crate::document::runtime::SaveDispatch::Lua => {
                let saved = self.call_plugins(|editor| {
                    editor.plugin_host.invoke_buffer_save(doc_id, &kind_name)
                });
                self.state.clear_command_line();
                if saved {
                    crate::document::runtime::SaveResult::Saved
                } else {
                    crate::document::runtime::SaveResult::Failed
                }
            }
            crate::document::runtime::SaveDispatch::Disabled => {
                self.state.clear_command_line();
                crate::document::runtime::SaveResult::Rejected
            }
            crate::document::runtime::SaveDispatch::Reject => {
                self.notify_error(RiftError::new(
                    ErrorType::Io,
                    "CANT_SAVE",
                    format!(
                        "{} buffer cannot be saved",
                        self.document_manager
                            .get_document(buf)
                            .map_or_default(|document| document.display_name().into_owned())
                    ),
                ));
                self.state.clear_command_line();
                crate::document::runtime::SaveResult::Rejected
            }
            crate::document::runtime::SaveDispatch::Native(handler) => {
                let result = if let Some(handler) = self.native_save_handlers.get(&kind_id).copied()
                {
                    self.with_document_target(buf, handler)
                } else if let Some(handle) = self
                    .document_manager
                    .get_document(buf)
                    .map(super::super::document::Document::handle)
                {
                    handler(handle)
                } else {
                    crate::document::runtime::SaveResult::Rejected
                };
                self.state.clear_command_line();
                result
            }
        }
    }

    pub(super) fn do_save(&mut self) {
        self.save_to(None);
    }

    pub(super) fn save_to(&mut self, target: Option<std::path::PathBuf>) {
        if let Some(buf) = self.document_manager.active_document_id() {
            self.save_document_to(buf, target);
        }
    }

    pub(super) fn save_document_to(
        &mut self,
        buf: crate::document::DocumentId,
        target: Option<std::path::PathBuf>,
    ) {
        let is_plain_file = self.document_manager.get_document(buf).is_some_and(|doc| {
            doc.buffer_kind_id() == crate::document::runtime::BufferKindId::FILE
        });
        if is_plain_file {
            let save_info = {
                let document = self.document_manager.get_document_mut(buf).unwrap();
                document.commit_pending_ghost();
                target
                    .or_else(|| document.path().map(std::path::Path::to_path_buf))
                    .map(|path| (document.id, path))
            };
            if let Some((buf_id, path)) = save_info {
                self.call_plugins(|editor| {
                    editor
                        .plugin_host
                        .dispatch(&crate::plugin::events::EditorEvent::BufSavePre {
                            buf: buf_id,
                            path: path.clone(),
                        });
                });
                let Some(document) = self.document_manager.get_document_mut(buf_id) else {
                    self.notify_error(RiftError::new(
                        ErrorType::Io,
                        "SAVE_TARGET_CLOSED",
                        "Buffer was closed by a plugin hook before it could be saved",
                    ));
                    self.state.clear_command_line();
                    return;
                };
                let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                    document.id,
                    document.buffer().line_index.table.clone(),
                    path,
                    document.options.line_ending,
                    document.options.has_bom,
                    document.history.current_seq(),
                );
                self.job_manager.spawn(job);
            } else {
                self.notify_error(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"));
            }
            self.state.clear_command_line();
        } else {
            if let Some(path) = target {
                if let Some(doc) = self.document_manager.get_document_mut(buf) {
                    doc.set_path(path);
                }
            }
            self.dispatch_save_sync(buf);
        }
    }

    fn set_active_path(&mut self, target: Option<std::path::PathBuf>) {
        let Some(path) = target else {
            return;
        };
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.set_path(path);
        }
    }

    pub(super) fn do_save_and_quit(&mut self) {
        self.save_quit_to(None);
    }

    pub(super) fn save_quit_to(&mut self, target: Option<std::path::PathBuf>) {
        let is_plain_file = self.document_manager.active_document().is_some_and(|doc| {
            doc.buffer_kind_id() == crate::document::runtime::BufferKindId::FILE
        });
        if is_plain_file {
            if let Some(doc) = self.document_manager.active_document_mut() {
                doc.commit_pending_ghost();
            }
            let res = {
                let doc = self.document_manager.active_document().unwrap();
                let path = target
                    .or_else(|| doc.path().map(std::path::Path::to_path_buf))
                    .or_else(|| self.state.file_path.as_ref().map(std::path::PathBuf::from));
                path.map(|path| {
                    (
                        doc.id,
                        path,
                        doc.buffer().line_index.table.clone(),
                        doc.options.line_ending,
                        doc.options.has_bom,
                        doc.history.current_seq(),
                    )
                })
                .ok_or_else(|| RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"))
            };
            match res {
                Ok((doc_id, path, table, line_ending, has_bom, saved_seq)) => {
                    let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                        doc_id,
                        table,
                        path.clone(),
                        line_ending,
                        has_bom,
                        saved_seq,
                    );
                    let id = self.job_manager.spawn(job);
                    self.pending_quit_job_id = Some(id);
                    self.notify(
                        crate::notification::NotificationType::Info,
                        format!("Saving {} and quitting...", path.display()),
                    );
                }
                Err(e) => self.notify_error(e),
            }
            self.state.clear_command_line();
            return;
        }

        self.set_active_path(target);
        let Some(handle) = self
            .document_manager
            .active_document()
            .map(crate::document::Document::handle)
        else {
            self.state.clear_command_line();
            return;
        };

        let save_result = self.dispatch_save_sync(handle.doc_id());
        if save_result != crate::document::runtime::SaveResult::Saved {
            return;
        }

        if self
            .document_manager
            .get_document_by_handle(handle)
            .is_some()
        {
            self.do_quit(false);
            return;
        }

        self.quit_if_alone();
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

    pub(super) fn quit_after_save(&mut self, doc_id: crate::document::DocumentId) {
        let unchanged = self.active_document_id() == doc_id
            && self
                .document_manager
                .get_document(doc_id)
                .is_some_and(|doc| !doc.is_dirty());
        if !unchanged {
            self.notify(
                crate::notification::NotificationType::Warning,
                "Quit aborted: buffer changed during save".to_string(),
            );
            return;
        }

        self.do_quit(false);
    }

    pub(super) fn do_quit(&mut self, force: bool) {
        let in_clipboard_entry = self
            .document_manager
            .active_document()
            .is_some_and(super::super::document::Document::is_clipboard_entry);
        if in_clipboard_entry {
            self.handle_clipboard_entry_close();
            return;
        }

        let in_explorer = self.panel_layout.as_ref().is_some_and(|l| {
            let fid = self.split_tree.focused_window_id();
            fid == l.dir_win_id || fid == l.preview_win_id
        });
        if in_explorer {
            self.close_split_panel();
            return;
        }
        if self.split_tree.window_count() > 1 {
            let focused_id = self.split_tree.focused_window_id();
            self.close_window(focused_id);
            let new_doc_id = self.active_document_id();
            self.call_plugins(|editor| {
                editor
                    .plugin_host
                    .dispatch(&crate::plugin::events::EditorEvent::BufEnter { buf: new_doc_id });
            });
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
                        self.notify_error(RiftError::warning(
                            ErrorType::Execution,
                            crate::constants::errors::UNSAVED_CHANGES,
                            crate::constants::errors::MSG_UNSAVED_CHANGES,
                        ));
                        return;
                    }
                }
            }
            self.should_quit = true;
        } else {
            let doc_id = self.active_document_id();
            let result = if force {
                self.close_buffer(doc_id, crate::document::manager::RemovalIntent::Force)
            } else {
                self.close_buffer(doc_id, crate::document::manager::RemovalIntent::Normal)
            };
            if let Err(e) = result {
                self.notify_error(e);
            } else {
                if let Some(new_doc_id) = self.document_manager.active_document_id() {
                    let _ = self.focus_document(new_doc_id);
                    self.call_plugins(|editor| {
                        editor.plugin_host.dispatch(
                            &crate::plugin::events::EditorEvent::BufEnter { buf: new_doc_id },
                        );
                    });
                }
                self.sync_state_with_active_document();
                if let Err(e) = self.force_full_redraw() {
                    self.notify_error(e);
                }
            }
        }
    }

    pub(super) fn do_buffer_next(&mut self) {
        self.do_buffer_cycle(true);
    }

    pub(super) fn do_buffer_prev(&mut self) {
        self.do_buffer_cycle(false);
    }

    fn do_buffer_cycle(&mut self, forward: bool) {
        let old_buf = self.active_document_id();
        if let Some(target) = self.document_manager.neighbor_tab(forward) {
            let _ = self.focus_document(target);
        }
        self.state.clear_command_line();
        let new_buf = self.active_document_id();
        if new_buf != old_buf {
            self.call_plugins(|editor| {
                editor
                    .plugin_host
                    .dispatch(&crate::plugin::events::EditorEvent::BufLeave { buf: old_buf });
                editor
                    .plugin_host
                    .dispatch(&crate::plugin::events::EditorEvent::BufEnter { buf: new_buf });
            });
        }
        if let Err(e) = self.force_full_redraw() {
            self.notify_error(e);
        }
    }

    pub(super) fn do_buffer_goto(&mut self, index: usize) {
        let Some(target) = self
            .document_manager
            .get_document_id_at(index.saturating_sub(1))
        else {
            self.notify(
                crate::notification::NotificationType::Warning,
                format!("Buffer {index} does not exist"),
            );
            self.state.clear_command_line();
            return;
        };
        let old_buf = self.active_document_id();
        if target == old_buf {
            self.state.clear_command_line();
            return;
        }
        if let Err(e) = self.focus_document(target) {
            self.notify_error(e);
            return;
        }
        self.state.clear_command_line();
        self.call_plugins(|editor| {
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::BufLeave { buf: old_buf });
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::BufEnter { buf: target });
        });
        if let Err(e) = self.force_full_redraw() {
            self.notify_error(e);
        }
    }

    pub(super) fn do_buffer_delete(&mut self, index: Option<usize>, force: bool) {
        let target = match index {
            Some(i) => {
                if let Some(id) = self
                    .document_manager
                    .get_document_id_at(i.saturating_sub(1))
                {
                    id
                } else {
                    self.notify(
                        crate::notification::NotificationType::Warning,
                        format!("Buffer {i} does not exist"),
                    );
                    self.state.clear_command_line();
                    return;
                }
            }
            None => self.active_document_id(),
        };

        if self.document_manager.is_private(target) {
            self.notify(
                crate::notification::NotificationType::Warning,
                "Cannot delete a panel buffer; close its panel instead".to_string(),
            );
            self.state.clear_command_line();
            return;
        }

        let result = if force {
            self.close_buffer(target, crate::document::manager::RemovalIntent::Force)
        } else {
            self.close_buffer(target, crate::document::manager::RemovalIntent::Normal)
        };

        if let Err(e) = result {
            self.notify_error(e);
        } else {
            self.state.clear_command_line();
            if let Err(e) = self.force_full_redraw() {
                self.notify_error(e);
            }
        }
    }

    pub(super) fn do_show_buffer_list(&mut self) {
        use std::fmt::Write as _;
        let buffers = self.document_manager.get_buffer_list();
        let mut message = String::new();
        for info in buffers {
            let dirty = if info.status.is_dirty { "+" } else { " " };
            let read_only = if info.status.is_read_only { "R" } else { " " };
            let current = if info.is_current { "%" } else { " " };
            let special = if info.status.is_special { "~" } else { " " };
            if !message.is_empty() {
                message.push('\n');
            }
            let _ = write!(
                message,
                "[{}] {}: {}{}{}{}",
                info.index + 1,
                info.name,
                current,
                dirty,
                read_only,
                special,
            );
        }
        self.notify(crate::notification::NotificationType::Info, message);
        self.state.clear_command_line();
    }

    pub(super) fn do_notification_clear(&mut self, all: bool) {
        if all {
            self.state.notifications.clear_all();
        } else {
            self.state.notifications.clear_last();
        }
        self.state.clear_command_line();
    }

    pub(super) fn do_split_window(
        &mut self,
        direction: crate::split::tree::SplitDirection,
        subcommand: crate::command_line::commands::types::SplitSubcommand,
    ) {
        use crate::command_line::commands::types::SplitSubcommand;
        match subcommand {
            SplitSubcommand::Current => {
                let doc_id = self.active_document_id();
                self.split_window(direction, doc_id);
            }
            SplitSubcommand::File(path) => self.split_file_window(direction, &path),
            SplitSubcommand::Navigate(dir) => self.navigate_window(dir),
            SplitSubcommand::Resize(delta) => self.resize_window(direction, delta),
            SplitSubcommand::Move(dir) => {
                self.move_window(dir);
            }
            SplitSubcommand::Equalize => self.equalize_windows(),
            SplitSubcommand::Exchange => {
                self.swap_windows();
            }
            SplitSubcommand::PreviousWindow => self.focus_previous_window(),
        }
        self.state.clear_command_line();
    }

    fn split_file_window(&mut self, direction: crate::split::tree::SplitDirection, path: &str) {
        let path_buf = std::path::PathBuf::from(path);
        if !path_buf.exists() {
            self.notify_error(crate::error::RiftError::new(
                crate::error::ErrorType::Io,
                "FILE_NOT_FOUND",
                format!("No such file: {path}"),
            ));
            return;
        }
        let doc_id = if let Some(id) = self.document_manager.find_open_document_id(&path_buf) {
            id
        } else {
            match self.open_placeholder(path) {
                Ok(id) => {
                    let job =
                        crate::job_manager::jobs::file_operations::FileLoadJob::new(id, path_buf);
                    self.job_manager.spawn(job);
                    id
                }
                Err(e) => {
                    self.notify_error(e);
                    return;
                }
            }
        };
        self.split_window(direction, doc_id);
    }
}
