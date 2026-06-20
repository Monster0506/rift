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
                    BufferKind::LocationList { .. } => self.handle_location_list_action(id),
                    BufferKind::Regions { .. } => self.handle_regions_buffer_action(id),
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

        // Accumulate selection-building actions for dot-repeat (S5.9). A
        // plain Normal-mode Move is navigation, not selection-building.
        let is_region_building = matches!(
            editor_action,
            EditorAction::EnterVisualChar
                | EditorAction::EnterVisualLine
                | EditorAction::EnterVisualBlock
                | EditorAction::RegionBankOccurrenceNext
                | EditorAction::RegionBankOccurrencePrev
        ) || self.current_mode.is_visual();
        if is_region_building && !self.dot_repeat.is_replaying() {
            self.region_build_recording.push(action.clone());
        }

        match editor_action {
            EditorAction::Move(motion) => {
                use crate::action::Motion;

                if matches!(
                    motion,
                    Motion::RepeatFindForward | Motion::RepeatFindBackward
                ) {
                    let has_regions = self
                        .document_manager
                        .active_document()
                        .map(|d| !d.selection_set.is_empty())
                        .unwrap_or(false);
                    if has_regions {
                        let forward = matches!(motion, Motion::RepeatFindForward);
                        return self.cycle_to_region(forward);
                    }
                }

                // Interface-mode buffers snap vertical motion between actionable
                // lines, else fall through to ordinary motion (design.md sec 9.4).
                if self.current_mode == Mode::Normal
                    && matches!(motion, Motion::Up | Motion::Down)
                    && self
                        .document_manager
                        .active_document()
                        .map(|d| d.is_interface_mode())
                        .unwrap_or(false)
                    && self.snap_to_actionable_line(matches!(motion, Motion::Down))
                {
                    return true;
                }

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
                if self.try_multi_insert_for_command(crate::command::Command::EnterInsertMode) {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertMode);
                true
            }
            EditorAction::EnterInsertModeAfter => {
                if self.try_multi_insert_for_command(crate::command::Command::EnterInsertModeAfter)
                {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertModeAfter);
                true
            }
            EditorAction::EnterInsertModeAtLineStart => {
                if self.try_multi_insert_for_command(
                    crate::command::Command::EnterInsertModeAtLineStart,
                ) {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineStart);
                true
            }
            EditorAction::EnterInsertModeAtLineEnd => {
                if self
                    .try_multi_insert_for_command(crate::command::Command::EnterInsertModeAtLineEnd)
                {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineEnd);
                true
            }
            EditorAction::OpenLineBelow => {
                if self.try_multi_insert_for_command(crate::command::Command::OpenLineBelow) {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
                self.handle_mode_management(crate::command::Command::OpenLineBelow);
                true
            }
            EditorAction::OpenLineAbove => {
                if self.try_multi_insert_for_command(crate::command::Command::OpenLineAbove) {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
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
                if self.current_mode.is_visual() {
                    if let (Some(anchor), Some(kind)) =
                        (self.visual_anchor, self.current_mode.visual_range_kind())
                    {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            let cursor = doc.buffer.cursor();
                            doc.selection_set
                                .bank(crate::selection::Region::new(anchor, cursor, kind));
                        }
                    }
                    self.visual_anchor = None;
                } else if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.selection_set.clear();
                    self.region_build_recording.clear();
                }
                if self.current_mode == Mode::Insert || self.current_mode == Mode::Replace {
                    // Finalize insert recording for dot-repeat
                    if !self.dot_repeat.is_replaying() {
                        self.dot_repeat.finish_insert_recording();
                    }
                    if !self.pending_multi_insert_anchors.is_empty() {
                        self.replay_multi_insert_at_remaining_anchors();
                    }
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                }
                // Reset history navigation when exiting command/search mode
                self.state.command_history.reset_navigation();
                self.state.search_history.reset_navigation();
                self.rename_context = None;
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
                } else if self.current_mode == Mode::Rename {
                    self.execute_lsp_rename();
                    true
                } else {
                    false
                }
            }
            EditorAction::Delete(motion) => {
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename
                {
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
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename
                {
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
                    doc.buffer.clear_desired_col();
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
                if let Some(layout) = self.panel_layout.clone() {
                    if layout.kind == crate::editor::PanelKind::Regions {
                        let _ = self
                            .document_manager
                            .switch_to_document(layout.preview_doc_id);
                        self.close_split_panel();
                    }
                }
                if self.current_mode.is_visual() {
                    if let (Some(anchor), Some(kind)) =
                        (self.visual_anchor, self.current_mode.visual_range_kind())
                    {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            let cursor = doc.buffer.cursor();
                            doc.selection_set
                                .bank(crate::selection::Region::new(anchor, cursor, kind));
                        }
                    }
                    self.visual_anchor = None;
                    self.set_mode(Mode::Normal);
                }
                if self.try_run_set_aware_operator(*op) {
                    self.finish_region_build(None);
                    return true;
                }
                if self.current_mode == Mode::OperatorPending {
                    if let Some(pending) = self.pending_operator {
                        if pending == *op {
                            return self.execute_operator_linewise(pending);
                        }
                    }
                }
                // A fresh operator key always supersedes any in-progress `ys`.
                self.pending_surround_add = None;
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
                    if self.try_run_set_aware_put(*before, &text) {
                        self.finish_region_build(Some(action.clone()));
                        return true;
                    }
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
                    if self.try_run_set_aware_put(*before, &text) {
                        self.finish_region_build(Some(action.clone()));
                        return true;
                    }
                    self.insert_text_at_cursor(&text, *before)
                } else {
                    false
                }
            }

            EditorAction::ExitTerminalMode => {
                self.set_mode(Mode::Normal);
                true
            }

            EditorAction::TerminalScrollback(delta) => {
                let doc_id = self.active_document_id();
                if let Some(doc) = self.document_manager.get_document(doc_id) {
                    if let Some(term) = &doc.terminal {
                        term.scroll_display(*delta);
                    }
                }
                if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                    doc.sync_terminal_buffer();
                }
                let _ = self.update_and_render();
                true
            }
            EditorAction::FindCharPending { forward, till } => {
                self.pending_grammar = Some(super::pending_grammar::PendingGrammar::FindChar {
                    forward: *forward,
                    till: *till,
                });
                true
            }

            EditorAction::ReplaceCharPending => {
                self.pending_grammar = Some(super::pending_grammar::PendingGrammar::ReplaceChar);
                true
            }

            EditorAction::EnterReplaceMode => {
                self.handle_mode_management(crate::command::Command::EnterReplaceMode);
                true
            }

            EditorAction::LspGotoDefinition => {
                use crate::buffer::api::BufferView;
                if let Some(doc) = self.document_manager.active_document() {
                    if let Some(path) = doc.path().map(|p| p.to_path_buf()) {
                        if self.lsp_manager.is_indexing_path(&path) {
                            self.state.notify(
                                crate::notification::NotificationType::Info,
                                "LSP: still indexing, please wait...".to_string(),
                            );
                        } else {
                            let line = doc.buffer.get_line() as u32;
                            let line_start = doc.buffer.line_start(doc.buffer.get_line());
                            let col = (doc.buffer.cursor().saturating_sub(line_start)) as u32;
                            if self.lsp_manager.goto_definition(&path, line, col).is_none() {
                                self.state.notify(
                                    crate::notification::NotificationType::Warning,
                                    "LSP: no server available for this file".to_string(),
                                );
                            }
                        }
                    }
                }
                true
            }

            EditorAction::LspReferences => {
                use crate::buffer::api::BufferView;
                if let Some(doc) = self.document_manager.active_document() {
                    if let Some(path) = doc.path().map(|p| p.to_path_buf()) {
                        if self.lsp_manager.is_indexing_path(&path) {
                            self.state.notify(
                                crate::notification::NotificationType::Info,
                                "LSP: still indexing, please wait...".to_string(),
                            );
                        } else {
                            let line = doc.buffer.get_line() as u32;
                            let line_start = doc.buffer.line_start(doc.buffer.get_line());
                            let col = (doc.buffer.cursor().saturating_sub(line_start)) as u32;
                            if self.lsp_manager.references(&path, line, col).is_none() {
                                self.state.notify(
                                    crate::notification::NotificationType::Warning,
                                    "LSP: no server available for this file".to_string(),
                                );
                            }
                        }
                    }
                }
                true
            }

            EditorAction::LspHover => {
                use crate::buffer::api::BufferView;
                if let Some(doc) = self.document_manager.active_document() {
                    if let Some(path) = doc.path().map(|p| p.to_path_buf()) {
                        if self.lsp_manager.is_indexing_path(&path) {
                            self.state.notify(
                                crate::notification::NotificationType::Info,
                                "LSP: still indexing, please wait...".to_string(),
                            );
                        } else {
                            let line = doc.buffer.get_line() as u32;
                            let line_start = doc.buffer.line_start(doc.buffer.get_line());
                            let col = (doc.buffer.cursor().saturating_sub(line_start)) as u32;
                            if self.lsp_manager.hover(&path, line, col).is_none() {
                                self.state.notify(
                                    crate::notification::NotificationType::Warning,
                                    "LSP: no server available for this file".to_string(),
                                );
                            }
                        }
                    }
                }
                true
            }

            EditorAction::LspRename => {
                use crate::buffer::api::BufferView;
                let ctx = self.document_manager.active_document().and_then(|doc| {
                    let path = doc.path()?.to_path_buf();
                    let line = doc.buffer.get_line() as u32;
                    let line_start = doc.buffer.line_start(doc.buffer.get_line());
                    let col = (doc.buffer.cursor().saturating_sub(line_start)) as u32;
                    Some((path, line, col))
                });
                if let Some(ctx) = ctx {
                    self.rename_context = Some(ctx);
                    self.state.clear_command_line();
                    self.set_mode(Mode::Rename);
                } else {
                    self.state.notify(
                        crate::notification::NotificationType::Warning,
                        "LSP rename: no file open".to_string(),
                    );
                }
                true
            }

            EditorAction::LspCodeAction => {
                use crate::buffer::api::BufferView;
                if let Some(doc) = self.document_manager.active_document() {
                    if let Some(path) = doc.path().map(|p| p.to_path_buf()) {
                        if self.lsp_manager.is_indexing_path(&path) {
                            self.state.notify(
                                crate::notification::NotificationType::Warning,
                                "LSP: still indexing, please wait...".to_string(),
                            );
                        } else {
                            let line = doc.buffer.get_line() as u32;
                            let line_start = doc.buffer.line_start(doc.buffer.get_line());
                            let col = (doc.buffer.cursor().saturating_sub(line_start)) as u32;
                            let uri = crate::lsp::protocol::path_to_uri(&path);
                            let norm_uri = crate::lsp::protocol::normalize_uri(&uri);
                            let diagnostics: Vec<crate::lsp::protocol::LspDiagnostic> = self
                                .lsp_diagnostics
                                .get(&norm_uri)
                                .map(|diags| {
                                    diags
                                        .iter()
                                        .filter(|d| d.range.start.line == line)
                                        .cloned()
                                        .collect()
                                })
                                .unwrap_or_default();
                            if self
                                .lsp_manager
                                .code_action(&path, line, col, diagnostics)
                                .is_none()
                            {
                                self.state.notify(
                                    crate::notification::NotificationType::Warning,
                                    "LSP: no server for this file".to_string(),
                                );
                            }
                        }
                    }
                }
                true
            }

            EditorAction::LspFormat => {
                let info = self.document_manager.active_document().and_then(|doc| {
                    let path = doc.path()?.to_path_buf();
                    let tab_size = doc.options.tab_width as u32;
                    let insert_spaces = doc.options.expand_tabs;
                    Some((path, tab_size, insert_spaces))
                });
                if let Some((path, tab_size, insert_spaces)) = info {
                    if self.lsp_manager.is_indexing_path(&path) {
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            "LSP: still indexing, please wait...".to_string(),
                        );
                    } else if self
                        .lsp_manager
                        .format(&path, tab_size, insert_spaces)
                        .is_none()
                    {
                        self.state.notify(
                            crate::notification::NotificationType::Warning,
                            "LSP: no server available for this file".to_string(),
                        );
                    }
                }
                true
            }

            EditorAction::LspDiagnosticNext => {
                self.lsp_diagnostic_next();
                true
            }

            EditorAction::LspDiagnosticPrev => {
                self.lsp_diagnostic_prev();
                true
            }

            EditorAction::LspDiagnosticsPanel => {
                self.open_diagnostics_panel();
                true
            }

            EditorAction::ActivateAnnotation => {
                self.activate_annotation_at_cursor();
                true
            }

            EditorAction::ActivateAnnotationVerb(verb) => {
                self.activate_annotation_verb(Some(verb));
                true
            }

            EditorAction::NextInteractiveAnnotation => {
                self.goto_next_interactive_annotation();
                true
            }

            EditorAction::PrevInteractiveAnnotation => {
                self.goto_prev_interactive_annotation();
                true
            }

            EditorAction::SurroundStart => {
                let count = self.pending_count;
                self.pending_count = 0;
                self.pending_grammar =
                    Some(super::pending_grammar::PendingGrammar::SurroundVerb { count });
                true
            }
            EditorAction::SurroundGiveLine => {
                use crate::text_objects::{Direction, Modifier, ObjectKind, TextObjectSpec};
                // Only meaningful mid-`sg`; otherwise mirrors the old
                // unrecognized-key-cancels-pending-operator behavior.
                let Some(delim_count) = self.pending_surround_add.take() else {
                    self.pending_operator = None;
                    self.set_mode(Mode::Normal);
                    self.pending_count = 0;
                    return false;
                };
                let line_span = if self.pending_count > 0 {
                    self.pending_count
                } else {
                    1
                };
                self.pending_count = 0;
                self.pending_operator = None;
                let spec = TextObjectSpec {
                    modifier: Modifier::Inner,
                    direction: Direction::Current,
                    nesting: 1,
                    kind: ObjectKind::Line,
                };
                self.pending_grammar =
                    Some(super::pending_grammar::PendingGrammar::AddSurroundChar {
                        motion: crate::action::Motion::TextObject(spec),
                        count: line_span,
                        delim_count,
                    });
                true
            }

            EditorAction::EnterVisualChar => self.enter_visual_or_resume(Mode::Visual),
            EditorAction::EnterVisualLine => self.enter_visual_or_resume(Mode::VisualLine),
            EditorAction::EnterVisualBlock => self.enter_visual_or_resume(Mode::VisualBlock),
            EditorAction::ExpandRegion => self.expand_active_region(),
            EditorAction::ShrinkRegion => self.shrink_active_region(),
            EditorAction::ToggleRegionsWindow => {
                self.toggle_regions_window();
                true
            }
            EditorAction::RegionsListDrop => self.drop_regions_window_entry(),
            EditorAction::RegionsListDown
            | EditorAction::RegionsListUp
            | EditorAction::RegionsListSelect => {
                let Some(layout) = self.panel_layout.clone() else {
                    return false;
                };
                if layout.kind != crate::editor::PanelKind::Regions {
                    return false;
                }
                // `j`/`k` are bound directly to this arm (not through the
                // generic Move action), so move the list's own cursor first.
                if let Some(doc) = self.document_manager.active_document_mut() {
                    match editor_action {
                        EditorAction::RegionsListDown => {
                            doc.buffer.move_down();
                        }
                        EditorAction::RegionsListUp => {
                            doc.buffer.move_up();
                        }
                        _ => {}
                    }
                }
                let line = self
                    .document_manager
                    .active_document()
                    .map(|d| d.buffer.line_index.get_line_at(d.buffer.cursor()))
                    .unwrap_or(0);
                let region = self
                    .document_manager
                    .get_document(layout.preview_doc_id)
                    .map(|d| d.selection_set.sorted())
                    .and_then(|sorted| sorted.get(line).copied());
                let Some(region) = region else { return false };
                if let Some(source) = self
                    .document_manager
                    .get_document_mut(layout.preview_doc_id)
                {
                    let (start, _) = region.buffer_span(&source.buffer);
                    let _ = source.buffer.set_cursor(start);
                }
                if matches!(editor_action, EditorAction::RegionsListSelect) {
                    self.close_split_panel();
                }
                true
            }
            EditorAction::VisualSwapEnds => {
                let Some(anchor) = self.visual_anchor else {
                    return false;
                };
                let Some(doc) = self.document_manager.active_document_mut() else {
                    return false;
                };
                let cursor = doc.buffer.cursor();
                self.visual_anchor = Some(cursor);
                let _ = doc.buffer.set_cursor(anchor);
                true
            }
            EditorAction::RegionBankOccurrenceNext | EditorAction::RegionBankOccurrencePrev => {
                let forward = matches!(editor_action, EditorAction::RegionBankOccurrenceNext);
                let Some(doc) = self.document_manager.active_document_mut() else {
                    return false;
                };
                if doc.selection_set.regions.is_empty() {
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        "Bank a region first (v + Esc)".to_string(),
                    );
                    return false;
                }
                let buf_snapshot = doc.buffer.clone();
                match doc.selection_set.bank_occurrence(&buf_snapshot, forward) {
                    Some((region, needle)) => {
                        let (start, _) = region.span();
                        let _ = doc.buffer.set_cursor(start);
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            format!("Banked occurrence of \"{}\"", needle),
                        );
                        true
                    }
                    None => {
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            "No further occurrence to bank".to_string(),
                        );
                        false
                    }
                }
            }
            EditorAction::AddSurroundToSet { ch, delim_count } => {
                self.try_run_set_aware_add_surround(*ch, *delim_count)
            }
        }
    }

    /// `v`/`V`/`Ctrl-V`: start a fresh active region at the cursor, or if it sits inside a
    /// banked region of the same kind, pop that back out with its original direction (design.md S3).
    pub(super) fn enter_visual_or_resume(&mut self, mode: Mode) -> bool {
        let Some(kind) = mode.visual_range_kind() else {
            return false;
        };
        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        let cursor = doc.buffer.cursor();
        if let Some(idx) = doc.selection_set.region_containing(cursor) {
            let region = doc.selection_set.regions.remove(idx);
            if region.kind == kind {
                self.visual_anchor = Some(region.anchor);
                let _ = doc.buffer.set_cursor(region.cursor);
                self.expand_history.clear();
                self.set_mode(mode);
                return true;
            }
            doc.selection_set.regions.insert(idx, region);
        }
        self.visual_anchor = Some(cursor);
        self.expand_history.clear();
        self.set_mode(mode);
        true
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
