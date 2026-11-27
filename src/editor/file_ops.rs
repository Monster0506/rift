use super::Editor;
use crate::document::runtime::ReloadDispatch;
use crate::document::DocumentId;
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::job_manager::jobs::file_operations::FileLoadMode;
use crate::term::TerminalBackend;

fn reload_undo_file_view_handler<T: TerminalBackend>(
    editor: &mut Editor<T>,
    doc_id: DocumentId,
) -> Result<(), RiftError> {
    let Some(doc) = editor.document_manager.get_document_mut(doc_id) else {
        return Ok(());
    };
    doc.reload_undo_file_view()
}

/// `:e` on an image buffer reopens the same file as plain text (raw bytes).
fn reopen_image_as_text<T: TerminalBackend>(
    editor: &mut Editor<T>,
    doc_id: DocumentId,
) -> Result<(), RiftError> {
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
    editor.start_file_load(doc_id, path, FileLoadMode::OpenAsText);
    Ok(())
}

fn reload_git_status<T: TerminalBackend>(editor: &mut Editor<T>, doc_id: DocumentId) {
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

fn reload_git_log<T: TerminalBackend>(editor: &mut Editor<T>, doc_id: DocumentId) {
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

fn reload_git_blame<T: TerminalBackend>(editor: &mut Editor<T>, doc_id: DocumentId) {
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

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn reload_document(
        &mut self,
        buf: DocumentId,
        force: bool,
    ) -> Result<(), RiftError> {
        let Some(doc) = self.document_manager.get_document(buf) else {
            return Ok(());
        };
        match doc.descriptor().reload_dispatch {
            ReloadDispatch::File => {
                let Some(path) = doc.path() else {
                    return Err(RiftError::new(
                        ErrorType::Execution,
                        crate::constants::errors::NO_PATH,
                        "No file name",
                    ));
                };
                if !force && doc.is_dirty() {
                    return Err(RiftError {
                        severity: ErrorSeverity::Warning,
                        kind: ErrorType::Execution,
                        code: crate::constants::errors::UNSAVED_CHANGES.to_string(),
                        message: crate::constants::errors::MSG_UNSAVED_CHANGES.to_string(),
                    });
                }
                self.start_file_load(buf, path.to_path_buf(), FileLoadMode::Reload);
            }
            ReloadDispatch::Directory => self.refresh_directory(buf),
            ReloadDispatch::UndoTree => self.refresh_undotree(buf),
            ReloadDispatch::Messages => self.refresh_messages_buffer(buf),
            ReloadDispatch::Clipboard => self.refresh_clipboard_buffer(buf),
            ReloadDispatch::GitStatus => reload_git_status(self, buf),
            ReloadDispatch::GitBlame => reload_git_blame(self, buf),
            ReloadDispatch::GitLog => reload_git_log(self, buf),
            ReloadDispatch::BufferList => self.refresh_buffer_list(buf),
            ReloadDispatch::UndoFileView => reload_undo_file_view_handler(self, buf)?,
            ReloadDispatch::ImageAsText => reopen_image_as_text(self, buf)?,
            ReloadDispatch::Unsupported => {
                return Err(RiftError::new(
                    ErrorType::Execution,
                    crate::constants::errors::RELOAD_UNSUPPORTED,
                    crate::constants::errors::MSG_RELOAD_UNSUPPORTED,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn do_buffer_next(&mut self) {
        self.do_buffer_cycle(true);
    }

    pub(super) fn do_buffer_prev(&mut self) {
        self.do_buffer_cycle(false);
    }

    fn do_buffer_cycle(&mut self, forward: bool) {
        if let Some(target) = self
            .document_manager
            .neighbor_tab(self.active_document_id(), forward)
        {
            let _ = self.focus_document(target);
        }
        self.state.clear_command_line();
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
        let buffers = self
            .document_manager
            .get_buffer_list(self.active_document_id());
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
                    self.start_file_load(id, path_buf, FileLoadMode::Open);
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
