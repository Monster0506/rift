use super::Editor;
use super::PostPasteState;
#[allow(unused_imports)]
use crate::buffer::api::BufferView;
use crate::command::Command;
use crate::mode::Mode;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_action(&mut self, action: &crate::action::Action) -> bool {
        use crate::action::{Action, EditorAction};

        let editor_action = match action {
            Action::Editor(act) => act,
            Action::Buffer(id) => {
                // messages:open works globally regardless of active buffer kind
                if id == "messages:open" {
                    self.open_messages(false);
                    return true;
                }
                use crate::document::BufferKind;
                let kind = self.active_document().kind.clone();
                match kind {
                    BufferKind::Directory { .. } => self.handle_directory_buffer_action(id),
                    BufferKind::UndoTree { .. } => self.handle_undotree_buffer_action(id),
                    BufferKind::Messages { .. } => self.handle_messages_buffer_action(id),
                    BufferKind::Clipboard { .. } => self.handle_clipboard_buffer_action(id),
                    BufferKind::ClipboardEntry { .. } => self.handle_clipboard_entry_action(id),
                    _ => {}
                }
                return true;
            }
            Action::Noop => return false,
        };

        // Clear post-paste cycling state on any action except CyclePaste itself.
        if !matches!(editor_action, EditorAction::CyclePaste { .. }) {
            self.post_paste_state = None;
        }

        match editor_action {
            EditorAction::Move(motion) => {
                use crate::action::Motion;

                let resolved = match motion {
                    Motion::RepeatFindForward => {
                        if let Some((ch, forward, is_till)) = self.state.last_find_char {
                            match (forward, is_till) {
                                (true, false) => Motion::FindCharForward(ch),
                                (true, true) => Motion::TillCharForward(ch),
                                (false, false) => Motion::FindCharBackward(ch),
                                (false, true) => Motion::TillCharBackward(ch),
                            }
                        } else {
                            Motion::NextMatch
                        }
                    }
                    Motion::RepeatFindBackward => {
                        if let Some((ch, forward, is_till)) = self.state.last_find_char {
                            match (forward, is_till) {
                                (true, false) => Motion::FindCharBackward(ch),
                                (true, true) => Motion::TillCharBackward(ch),
                                (false, false) => Motion::FindCharForward(ch),
                                (false, true) => Motion::TillCharForward(ch),
                            }
                        } else {
                            Motion::PreviousMatch
                        }
                    }
                    other => *other,
                };

                match motion {
                    Motion::FindCharForward(ch) => {
                        self.state.last_find_char = Some((*ch, true, false))
                    }
                    Motion::FindCharBackward(ch) => {
                        self.state.last_find_char = Some((*ch, false, false))
                    }
                    Motion::TillCharForward(ch) => {
                        self.state.last_find_char = Some((*ch, true, true))
                    }
                    Motion::TillCharBackward(ch) => {
                        self.state.last_find_char = Some((*ch, false, true))
                    }
                    _ => {}
                }

                if self.current_mode == Mode::OperatorPending {
                    if let Some(op) = self.pending_operator {
                        return self.execute_operator(op, resolved);
                    }
                }
                let count = if self.pending_count > 0 {
                    self.pending_count
                } else {
                    1
                };
                let command = crate::command::Command::Move(resolved, count);
                // Execute immediately
                self.handle_mode_management(command);
                let consumed = self.execute_buffer_command(command);
                self.update_explorer_preview();
                self.update_undotree_preview();
                self.update_clipboard_preview();
                consumed
            }
            EditorAction::EnterInsertMode => {
                self.handle_mode_management(crate::command::Command::EnterInsertMode);
                true
            }
            EditorAction::EnterInsertModeAfter => {
                self.handle_mode_management(crate::command::Command::EnterInsertModeAfter);
                true
            }
            EditorAction::EnterInsertModeAtLineStart => {
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineStart);
                true
            }
            EditorAction::EnterInsertModeAtLineEnd => {
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineEnd);
                true
            }
            EditorAction::OpenLineBelow => {
                self.handle_mode_management(crate::command::Command::OpenLineBelow);
                true
            }
            EditorAction::OpenLineAbove => {
                self.handle_mode_management(crate::command::Command::OpenLineAbove);
                true
            }
            EditorAction::EnterCommandMode => {
                self.handle_mode_management(crate::command::Command::EnterCommandMode);
                true
            }
            EditorAction::EnterSearchMode => {
                self.handle_mode_management(crate::command::Command::EnterSearchMode);
                true
            }
            EditorAction::EnterNormalMode => {
                if self.current_mode == Mode::Insert {
                    // Finalize insert recording for dot-repeat
                    if !self.dot_repeat.is_replaying() {
                        self.dot_repeat.finish_insert_recording();
                    }
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                }
                // Reset history navigation when exiting command/search mode
                self.state.command_history.reset_navigation();
                self.state.search_history.reset_navigation();
                self.set_mode(Mode::Normal);
                self.state.clear_command_line();
                self.state.search_matches.clear();
                self.pending_operator = None;
                self.pending_keys.clear();
                self.pending_count = 0;
                true
            }
            EditorAction::Undo => self.execute_buffer_command(crate::command::Command::Undo),
            EditorAction::Redo => self.execute_buffer_command(crate::command::Command::Redo),
            EditorAction::Quit => {
                self.do_quit(false);
                true
            }
            EditorAction::Submit => {
                if self.current_mode == Mode::Command {
                    self.handle_mode_management(crate::command::Command::ExecuteCommandLine);
                    true
                } else if self.current_mode == Mode::Search {
                    self.handle_mode_management(crate::command::Command::ExecuteSearch);
                    true
                } else {
                    false
                }
            }
            EditorAction::Delete(motion) => {
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search {
                    // Assuming left motion is backspace
                    if *motion == crate::action::Motion::Left {
                        self.handle_mode_management(crate::command::Command::DeleteFromCommandLine);
                        return true;
                    }
                    if *motion == crate::action::Motion::PreviousWord {
                        self.state.delete_word_back_command_line();
                        return true;
                    }
                }
                // Capture deleted text to ring in Normal mode (x / X).
                if self.current_mode == Mode::Normal {
                    let viewport_height = self.render_system.viewport.visible_rows();
                    let last_search_query = self.state.last_search_query.clone();
                    let captured = self.document_manager.active_document_mut().and_then(|doc| {
                        let tab_width = doc.options.tab_width;
                        crate::executor::compute_motion_range(
                            *motion,
                            1,
                            doc,
                            viewport_height,
                            last_search_query.as_deref(),
                            tab_width,
                        )
                        .map(|range| crate::clipboard::capture_text(&doc.buffer, &range))
                    });
                    let in_clipboard = self
                        .document_manager
                        .active_document()
                        .map(|d| d.is_any_clipboard())
                        .unwrap_or(false);
                    if let Some(text) = captured.filter(|s| !s.is_empty()) {
                        if !in_clipboard {
                            self.clipboard_ring.push(text);
                            self.refresh_clipboard_buffer_if_open();
                        }
                    }
                }
                let command = crate::command::Command::Delete(*motion, 1);
                let result = self.execute_buffer_command(command);
                if result && self.current_mode == Mode::Normal && !self.dot_repeat.is_replaying() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            EditorAction::DeleteLine => {
                let command = crate::command::Command::DeleteLine;
                let result = self.execute_buffer_command(command);
                if result && self.current_mode == Mode::Normal && !self.dot_repeat.is_replaying() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            EditorAction::InsertChar(c) => {
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search {
                    self.handle_mode_management(crate::command::Command::AppendToCommandLine(*c));
                    return true;
                }
                let command = crate::command::Command::InsertChar(*c);
                self.execute_buffer_command(command)
            }
            EditorAction::BufferNext => {
                self.do_buffer_next();
                true
            }
            EditorAction::BufferPrevious => {
                self.do_buffer_prev();
                true
            }
            EditorAction::ToggleDebug => {
                self.state.toggle_debug();
                true
            }
            EditorAction::Redraw => {
                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
                true
            }
            EditorAction::Reload => {
                if let Err(e) = self.load_plugins() {
                    self.state.handle_error(e);
                }
                true
            }
            EditorAction::Save => {
                self.do_save();
                true
            }
            EditorAction::SaveAndQuit => {
                self.do_save_and_quit();
                true
            }
            EditorAction::OpenExplorer => {
                let path = self
                    .document_manager
                    .active_document()
                    .and_then(|d| {
                        if let crate::document::BufferKind::Directory { path, .. } = &d.kind {
                            return Some(path.clone());
                        }
                        d.path().map(|p| {
                            if p.is_dir() {
                                p.to_path_buf()
                            } else {
                                p.parent().unwrap_or(p).to_path_buf()
                            }
                        })
                    })
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                self.open_explorer(path);
                true
            }
            EditorAction::ExplorerToggleHidden => {
                self.handle_explorer_toggle_hidden();
                true
            }
            EditorAction::OpenUndoTree => {
                self.open_undotree_split();
                true
            }
            EditorAction::OpenMessages => {
                self.open_messages(false);
                true
            }
            EditorAction::OpenClipboard => {
                self.open_clipboard();
                true
            }
            EditorAction::ShowBufferList => {
                self.do_show_buffer_list();
                true
            }
            EditorAction::ClearHighlights => {
                self.state.search_matches.clear();
                self.force_full_redraw().ok();
                true
            }
            EditorAction::ClearNotifications => {
                self.state.error_manager.notifications_mut().clear_all();
                self.state.clear_command_line();
                self.force_full_redraw().ok();
                true
            }
            EditorAction::ClearLastNotification => {
                self.state.error_manager.notifications_mut().clear_last();
                self.state.clear_command_line();
                self.force_full_redraw().ok();
                true
            }
            EditorAction::Checkpoint => {
                if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.checkpoint();
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        "Checkpoint created".to_string(),
                    );
                }
                true
            }
            EditorAction::RunCommand(cmd_str) => {
                let cmd_str = cmd_str.clone();
                self.execute_command_line(cmd_str);
                true
            }
            EditorAction::GotoLine(n) => {
                use crate::action::Motion;
                let target_n = if self.pending_count > 0 {
                    self.pending_count
                } else {
                    *n
                };
                if self.current_mode == Mode::OperatorPending {
                    if let Some(op) = self.pending_operator {
                        let target_line = {
                            if let Some(doc) = self.document_manager.active_document() {
                                let total = doc.buffer.line_count();
                                if target_n == 0 || target_n > total {
                                    total.saturating_sub(1)
                                } else {
                                    target_n - 1
                                }
                            } else {
                                return false;
                            }
                        };
                        return self.execute_operator(op, Motion::ToLine(target_line));
                    }
                }
                if let Some(doc) = self.document_manager.active_document_mut() {
                    let total = doc.buffer.line_count();
                    let idx = if target_n == 0 || target_n > total {
                        total.saturating_sub(1)
                    } else {
                        target_n - 1
                    };
                    let offset = doc.buffer.line_start(idx);
                    let _ = doc.buffer.set_cursor(offset);
                }
                true
            }
            EditorAction::Search(pattern) => {
                let pattern = pattern.clone();
                self.perform_search(&pattern, SearchDirection::Forward, false);
                true
            }
            EditorAction::Operator(op) => {
                if self.current_mode == Mode::OperatorPending {
                    if let Some(pending) = self.pending_operator {
                        if pending == *op {
                            return self.execute_operator_linewise(pending);
                        }
                    }
                }
                self.pending_operator = Some(*op);
                self.set_mode(Mode::OperatorPending);
                true
            }
            EditorAction::Command(cmd) => {
                let command = *cmd.clone();
                self.handle_mode_management(command);
                self.execute_buffer_command(command)
            }
            EditorAction::HistoryUp => {
                let dropdown_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                if dropdown_open {
                    self.handle_mode_management(Command::TabCompletePrev);
                } else {
                    self.navigate_history_up();
                }
                true
            }
            EditorAction::HistoryDown => {
                let dropdown_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                if dropdown_open {
                    self.handle_mode_management(Command::TabComplete);
                } else {
                    self.navigate_history_down();
                }
                true
            }
            EditorAction::DotRepeat => self.execute_dot_repeat(),
            EditorAction::QuitForce => {
                self.should_quit = true;
                true
            }
            EditorAction::OpenFile { path, force } => {
                let path = path.clone();
                let force = *force;
                if let Err(e) = self.open_file(path, force) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    if let Err(e) = self.force_full_redraw() {
                        self.state.handle_error(e);
                    }
                }
                true
            }
            EditorAction::OpenDirectory(path) => {
                let path = path.clone();
                self.open_explorer(path);
                true
            }
            EditorAction::OpenTerminal(cmd) => {
                let cmd = cmd.clone();
                if let Err(e) = self.open_terminal(cmd) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    self.set_mode(Mode::Insert);
                }
                true
            }
            EditorAction::SplitWindow {
                direction,
                subcommand,
            } => {
                let direction = *direction;
                let subcommand = subcommand.clone();
                self.do_split_window(direction, subcommand);
                true
            }
            EditorAction::UndoCount(count) => {
                self.do_undo(*count);
                true
            }
            EditorAction::RedoCount(count) => {
                self.do_redo(*count);
                true
            }
            EditorAction::UndoGoto(seq) => {
                self.do_undo_goto(*seq);
                true
            }
            EditorAction::NotificationClearAll => {
                self.state.error_manager.notifications_mut().clear_all();
                self.state.clear_command_line();
                true
            }
            EditorAction::PluginAction(id) => {
                self.plugin_host.execute_action(id);
                self.apply_plugin_mutations();
                true
            }

            EditorAction::Put { before } => {
                if let Some(text) = self.clipboard_ring.most_recent().map(|s| s.to_owned()) {
                    let original_cursor = self
                        .document_manager
                        .active_document()
                        .map(|d| d.buffer.cursor())
                        .unwrap_or(0);
                    let result = self.insert_text_at_cursor(&text, *before);
                    if result {
                        self.post_paste_state = Some(PostPasteState {
                            ring_index: 0,
                            before: *before,
                            original_cursor,
                        });
                    }
                    result
                } else {
                    false
                }
            }

            EditorAction::CyclePaste { forward } => {
                if let Some(state) = self.post_paste_state.take() {
                    let len = self.clipboard_ring.len().max(1);
                    let next = if *forward {
                        (state.ring_index + 1) % len
                    } else {
                        (state.ring_index + len - 1) % len
                    };
                    if let Some(text) = self.clipboard_ring.get(next).map(|s| s.to_owned()) {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            doc.undo();
                            // Restore cursor to where it was before the original paste so
                            // insert_text_at_cursor starts from the same position each cycle.
                            let _ = doc.buffer.set_cursor(state.original_cursor);
                        }
                        let result = self.insert_text_at_cursor(&text, state.before);
                        if result {
                            self.post_paste_state = Some(PostPasteState {
                                ring_index: next,
                                before: state.before,
                                original_cursor: state.original_cursor,
                            });
                        }
                        result
                    } else {
                        false
                    }
                } else {
                    false
                }
            }

            EditorAction::PutSystemClipboard { before } => {
                let text = arboard::Clipboard::new()
                    .ok()
                    .and_then(|mut cb| cb.get_text().ok());
                if let Some(text) = text {
                    self.insert_text_at_cursor(&text, *before)
                } else {
                    false
                }
            }

            EditorAction::ExitTerminalMode => {
                self.set_mode(Mode::Normal);
                true
            }
            EditorAction::FindCharPending { forward, till } => {
                self.pending_find_char_dir = Some((*forward, *till));
                true
            }
        }
    }

    /// Insert `text` at the cursor position.
    ///
    /// Linewise text (ends with `\n`) is handled specially:
    /// - `before = false` (`p`): inserts at the start of the next line (below)
    /// - `before = true`  (`P`): inserts at the start of the current line (above)
    ///
    /// Charwise text: `before = false` advances the cursor one position first.
    pub(super) fn insert_text_at_cursor(&mut self, text: &str, before: bool) -> bool {
        let is_linewise = text.ends_with('\n');

        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };

        // Track whether we need to prepend a newline (last-line edge case).
        let mut needs_leading_newline = false;

        if is_linewise {
            if before {
                // P: insert above → start of current line
                doc.buffer.move_to_line_start();
            } else {
                // p: insert below → start of next line
                let line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
                let total = doc.buffer.get_total_lines();
                if line + 1 < total {
                    let next = doc
                        .buffer
                        .line_index
                        .get_start(line + 1)
                        .unwrap_or(doc.buffer.len());
                    let _ = doc.buffer.set_cursor(next);
                } else {
                    // Last line has no trailing newline — go to end and prepend one.
                    doc.buffer.move_to_end();
                    needs_leading_newline = true;
                }
            }
        } else if !before {
            doc.buffer.move_right();
        }

        // For linewise paste, remember where the pasted line starts so we can
        // land the cursor there after the transaction (not at the end of the insert).
        let linewise_start = if is_linewise {
            if needs_leading_newline {
                // The pasted content starts one char after the prepended \n.
                Some(doc.buffer.cursor() + 1)
            } else {
                Some(doc.buffer.cursor())
            }
        } else {
            None
        };

        doc.begin_transaction("Put");
        if needs_leading_newline {
            // Insert "\n" then the text content without its own trailing newline.
            if doc.insert_char('\n').is_ok() {
                for ch in text.trim_end_matches('\n').chars() {
                    if doc.insert_char(ch).is_err() {
                        break;
                    }
                }
            }
        } else {
            for ch in text.chars() {
                if doc.insert_char(ch).is_err() {
                    break;
                }
            }
        }
        doc.commit_transaction();

        // Place cursor at the start of the pasted line, not at the end of the insert.
        if let Some(start) = linewise_start {
            let _ = doc.buffer.set_cursor(start);
        }

        self.do_incremental_syntax_parse();
        true
    }
}
