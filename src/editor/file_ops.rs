use super::Editor;
use crate::error::{ErrorType, RiftError};
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn do_save(&mut self) {
        use crate::document::BufferKind;
        if let Some(doc) = self.document_manager.active_document() {
            match &doc.kind {
                BufferKind::File => {
                    let save_info = {
                        let doc = self.document_manager.active_document_mut().unwrap();
                        doc.path().map(|p| (doc.id, p.to_path_buf()))
                    };
                    if let Some((buf_id, path)) = save_info {
                        self.plugin_host
                            .dispatch(&crate::plugin::EditorEvent::BufSavePre {
                                buf: buf_id,
                                path: path.clone(),
                            });
                        self.apply_plugin_mutations();
                        let doc = self.document_manager.active_document_mut().unwrap();
                        let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                            doc.id,
                            doc.buffer.line_index.table.clone(),
                            path,
                            doc.options.line_ending,
                            doc.history.current_seq(),
                        );
                        self.job_manager.spawn(job);
                    } else {
                        self.state.handle_error(RiftError::new(
                            ErrorType::Io,
                            "NO_FILENAME",
                            "No file name",
                        ));
                    }
                }
                BufferKind::Directory { .. } => {
                    self.apply_directory_diff();
                }
                BufferKind::Clipboard { .. } => {
                    self.apply_clipboard_diff();
                }
                BufferKind::ClipboardEntry { .. } => {
                    self.apply_clipboard_entry_save();
                }
                BufferKind::UndoTree { .. }
                | BufferKind::Terminal
                | BufferKind::Messages { .. } => {
                    self.state.handle_error(RiftError::new(
                        ErrorType::Io,
                        "CANT_SAVE",
                        format!(
                            "{} buffer cannot be saved",
                            self.document_manager
                                .active_document()
                                .map(|d| d.display_name().into_owned())
                                .unwrap_or_default()
                        ),
                    ));
                }
            }
        }
        self.state.clear_command_line();
    }

    pub(super) fn do_save_and_quit(&mut self) {
        let res = {
            let doc = self.document_manager.active_document().unwrap();
            if doc.has_path() {
                Ok((
                    doc.id,
                    doc.path().unwrap().to_path_buf(),
                    doc.buffer.line_index.table.clone(),
                    doc.options.line_ending,
                    doc.history.current_seq(),
                ))
            } else if let Some(path) = &self.state.file_path {
                Ok((
                    doc.id,
                    std::path::PathBuf::from(path),
                    doc.buffer.line_index.table.clone(),
                    doc.options.line_ending,
                    doc.history.current_seq(),
                ))
            } else {
                Err(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"))
            }
        };
        match res {
            Ok((doc_id, path, table, line_ending, saved_seq)) => {
                let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                    doc_id,
                    table,
                    path.clone(),
                    line_ending,
                    saved_seq,
                );
                let id = self.job_manager.spawn(job);
                self.pending_quit_job_id = Some(id);
                self.state.notify(
                    crate::notification::NotificationType::Info,
                    format!("Saving {} and quitting...", path.display()),
                );
            }
            Err(e) => self.state.handle_error(e),
        }
        self.state.clear_command_line();
    }

    pub(super) fn do_quit(&mut self, force: bool) {
        // If focused on a clipboard entry scratch buffer, return to the index pane
        let in_clipboard_entry = self
            .document_manager
            .active_document()
            .map(|d| matches!(d.kind, crate::document::BufferKind::ClipboardEntry { .. }))
            .unwrap_or(false);
        if in_clipboard_entry {
            self.handle_clipboard_entry_close();
            return;
        }

        let in_explorer = self
            .panel_layout
            .as_ref()
            .map(|l| {
                let fid = self.split_tree.focused_window_id();
                fid == l.dir_win_id || fid == l.preview_win_id
            })
            .unwrap_or(false);
        if in_explorer {
            self.close_split_panel();
            return;
        }
        if self.split_tree.window_count() > 1 {
            let focused_id = self.split_tree.focused_window_id();
            self.split_tree.close_window(focused_id);
            let new_doc_id = self.split_tree.focused_window().document_id;
            let new_cursor = self.split_tree.focused_window().cursor_position;
            let _ = self.document_manager.switch_to_document(new_doc_id);
            if let Some(doc) = self.document_manager.get_document_mut(new_doc_id) {
                let _ = doc.buffer.set_cursor(new_cursor);
            }
            self.sync_state_with_active_document();
            if let Err(e) = self.force_full_redraw() {
                self.state.handle_error(e);
            }
        } else if self.document_manager.tab_count() <= 1 {
            // Last buffer: quit the editor
            if !force {
                let doc_id = self.active_document_id();
                if let Some(doc) = self.document_manager.get_document(doc_id) {
                    if doc.is_dirty() && !doc.is_special() {
                        self.state.handle_error(RiftError::warning(
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
                self.document_manager.remove_document_force(doc_id)
            } else {
                self.document_manager.remove_document(doc_id)
            };
            match result {
                Err(e) => self.state.handle_error(e),
                Ok(()) => {
                    self.update_lua_state();
                    self.plugin_host
                        .dispatch(&crate::plugin::EditorEvent::BufClose { buf: doc_id });
                    if let Some(new_doc_id) = self.document_manager.active_document_id() {
                        self.split_tree.focused_window_mut().document_id = new_doc_id;
                        self.plugin_host
                            .dispatch(&crate::plugin::EditorEvent::BufEnter { buf: new_doc_id });
                    }
                    self.apply_plugin_mutations();
                    self.sync_state_with_active_document();
                    if let Err(e) = self.force_full_redraw() {
                        self.state.handle_error(e);
                    }
                }
            }
        }
    }

    pub(super) fn do_buffer_next(&mut self) {
        let old_buf = self.active_document_id();
        self.save_current_view_state();
        self.document_manager.switch_next_tab();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.restore_view_state();
        self.sync_state_with_active_document();
        self.state.clear_command_line();
        if let Some(new_buf) = self.document_manager.active_document_id() {
            if new_buf != old_buf {
                self.update_lua_state();
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::BufLeave { buf: old_buf });
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::BufEnter { buf: new_buf });
                self.apply_plugin_mutations();
            }
        }
        if let Err(e) = self.force_full_redraw() {
            self.state.handle_error(e);
        }
    }

    pub(super) fn do_buffer_prev(&mut self) {
        let old_buf = self.active_document_id();
        self.save_current_view_state();
        self.document_manager.switch_prev_tab();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.restore_view_state();
        self.sync_state_with_active_document();
        self.state.clear_command_line();
        if let Some(new_buf) = self.document_manager.active_document_id() {
            if new_buf != old_buf {
                self.update_lua_state();
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::BufLeave { buf: old_buf });
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::BufEnter { buf: new_buf });
                self.apply_plugin_mutations();
            }
        }
        if let Err(e) = self.force_full_redraw() {
            self.state.handle_error(e);
        }
    }

    pub(super) fn do_show_buffer_list(&mut self) {
        let buffers = self.document_manager.get_buffer_list();
        let mut message = String::new();
        for info in buffers {
            let dirty = if info.is_dirty { "+" } else { " " };
            let read_only = if info.is_read_only { "R" } else { " " };
            let current = if info.is_current { "%" } else { " " };
            let special = if info.is_special { "~" } else { " " };
            if !message.is_empty() {
                message.push('\n');
            }
            message.push_str(&format!(
                "[{}] {}: {}{}{}{}",
                info.index + 1,
                info.name,
                current,
                dirty,
                read_only,
                special,
            ));
        }
        self.state
            .notify(crate::notification::NotificationType::Info, message);
        self.state.clear_command_line();
    }

    pub(super) fn do_notification_clear(&mut self, all: bool) {
        if all {
            self.state.error_manager.notifications_mut().clear_all();
        } else {
            self.state.error_manager.notifications_mut().clear_last();
        }
        self.state.clear_command_line();
    }

    pub(super) fn do_split_window(
        &mut self,
        direction: crate::split::tree::SplitDirection,
        subcommand: crate::command_line::commands::SplitSubcommand,
    ) {
        use crate::command_line::commands::SplitSubcommand;
        match subcommand {
            SplitSubcommand::Current => {
                let doc_id = self.active_document_id();
                let focused_id = self.split_tree.focused_window_id();
                let size = self.term.get_size().unwrap();
                let new_id = self.split_tree.split(
                    direction,
                    focused_id,
                    doc_id,
                    size.rows as usize,
                    size.cols as usize,
                );
                self.switch_focus(new_id);
            }
            SplitSubcommand::File(path) => {
                let path_buf = std::path::PathBuf::from(&path);
                if !path_buf.exists() {
                    self.state.handle_error(crate::error::RiftError::new(
                        crate::error::ErrorType::Io,
                        "FILE_NOT_FOUND",
                        format!("No such file: {path}"),
                    ));
                    return;
                }
                let doc_id = if let Some(id) =
                    self.document_manager.find_open_document_id(&path_buf)
                {
                    id
                } else {
                    match self.document_manager.create_placeholder(&path) {
                        Ok(id) => {
                            let job = crate::job_manager::jobs::file_operations::FileLoadJob::new(
                                id, path_buf,
                            );
                            self.job_manager.spawn(job);
                            id
                        }
                        Err(e) => {
                            self.state.handle_error(e);
                            return;
                        }
                    }
                };
                let focused_id = self.split_tree.focused_window_id();
                let size = self.term.get_size().unwrap();
                let new_id = self.split_tree.split(
                    direction,
                    focused_id,
                    doc_id,
                    size.rows as usize,
                    size.cols as usize,
                );
                self.switch_focus(new_id);
            }
            SplitSubcommand::Navigate(dir) => {
                let size = self.term.get_size().unwrap();
                let layouts = self
                    .split_tree
                    .compute_layout(size.rows as usize, size.cols as usize);
                if let Some(target_id) = self.split_tree.navigate(dir, &layouts) {
                    self.switch_focus(target_id);
                }
            }
            SplitSubcommand::Resize(delta) => {
                let size = self.term.get_size().unwrap();
                let layouts = self
                    .split_tree
                    .compute_layout(size.rows as usize, size.cols as usize);
                let delta_ratio = (delta as f64) / (size.cols as f64);
                self.split_tree
                    .resize_focused(direction, delta_ratio, &layouts);
                self.render_system.viewport.mark_needs_full_redraw();
            }
        }
        self.state.clear_command_line();
    }
}
