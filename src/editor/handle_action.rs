use super::Editor;
use super::PostPasteState;
use crate::action::{Action, EditorAction};
use crate::buffer::api::BufferView;
use crate::command::Command;
use crate::document::runtime::{ActionDispatch, BufferKindId, NavigationPolicy};
use crate::mode::Mode;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

pub(super) type NativeActionHandler<T> = fn(&mut Editor<T>, &str);

pub(super) fn native_action_handlers<T: TerminalBackend>(
) -> std::collections::HashMap<BufferKindId, NativeActionHandler<T>> {
    let mut handlers = std::collections::HashMap::new();
    handlers.insert(
        BufferKindId::DIRECTORY,
        Editor::handle_directory_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::GIT_STATUS,
        Editor::handle_git_status_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::GIT_BLAME,
        Editor::handle_git_blame_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::GIT_LOG,
        Editor::handle_git_log_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::UNDO_TREE,
        Editor::handle_undotree_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::MESSAGES,
        Editor::handle_messages_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::CLIPBOARD,
        Editor::handle_clipboard_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::CLIPBOARD_ENTRY,
        Editor::handle_clipboard_entry_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::LOCATION_LIST,
        Editor::handle_location_list_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::REGIONS,
        Editor::handle_regions_buffer_action as NativeActionHandler<T>,
    );
    handlers.insert(
        BufferKindId::BUFFER_LIST,
        Editor::handle_buffer_list_action as NativeActionHandler<T>,
    );
    handlers
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_action(&mut self, action: &crate::action::Action) -> bool {
        self.handle_action_inner(action)
    }

    fn handle_action_inner(&mut self, action: &crate::action::Action) -> bool {
        let editor_action = match action {
            Action::Editor(act) => act,
            Action::Buffer(id) => {
                if id == "messages:open" {
                    self.open_messages(false);
                    return true;
                }
                let Some((doc_id, kind_id, kind_name, action_dispatch, handle, is_tombstone)) =
                    self.document_manager.active_document().map(|document| {
                        (
                            document.id,
                            document.buffer_kind_id(),
                            document.descriptor().name().to_string(),
                            document.descriptor().action_dispatch,
                            document.handle(),
                            document.descriptor().is_tombstone(),
                        )
                    })
                else {
                    return true;
                };

                if is_tombstone || matches!(action_dispatch, ActionDispatch::Disabled) {
                    return true;
                }

                match action_dispatch {
                    ActionDispatch::Native(handler) => {
                        handler(handle);
                        if let Some(handler) = self.native_action_handlers.get(&kind_id).copied() {
                            handler(self, id);
                        }
                    }
                    ActionDispatch::Lua => {
                        self.plugin_host
                            .invoke_buffer_action(doc_id, &kind_name, id);
                        self.apply_plugin_mutations();
                    }
                    ActionDispatch::Disabled => {}
                }
                return true;
            }
            Action::Noop => return false,
        };

        if !matches!(editor_action, EditorAction::CyclePaste { .. }) {
            self.post_paste_state = None;
        }

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
        self.handle_motion_action(action, editor_action)
    }

    fn handle_motion_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::Move(motion) => {
                use crate::action::Motion;

                if self.current_mode == Mode::Replace {
                    self.replace_originals.clear();
                }

                if matches!(
                    motion,
                    Motion::RepeatFindForward | Motion::RepeatFindBackward
                ) {
                    let has_regions = self
                        .document_manager
                        .active_document()
                        .is_some_and(|d| !d.selection_set.is_empty());
                    if has_regions {
                        let forward = matches!(motion, Motion::RepeatFindForward);
                        return self.cycle_to_region(forward);
                    }
                }

                if self.current_mode == Mode::Normal
                    && matches!(motion, Motion::Up | Motion::Down)
                    && self.active_doc_is(|doc| doc.buffer_kind_id() == BufferKindId::GIT_BLAME)
                {
                    let _ = self.snap_to_actionable_line(matches!(motion, Motion::Down));
                    return true;
                }

                if self.current_mode == Mode::Normal
                    && matches!(motion, Motion::Up | Motion::Down)
                    && self
                        .active_doc_is(|d| d.policies().navigation == NavigationPolicy::ActionRows)
                    && self.snap_to_actionable_line(matches!(motion, Motion::Down))
                {
                    self.update_explorer_preview();
                    self.update_buffer_list_preview();
                    return true;
                }

                let resolved = self.resolve_motion(motion);

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
                self.handle_mode_management(command);
                let consumed = self.execute_buffer_command(command);
                self.update_explorer_preview();
                self.update_undotree_preview();
                self.update_clipboard_preview();
                consumed
            }
            _ => self.handle_mode_enter_action(action, editor_action),
        }
    }

    fn handle_mode_enter_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
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
            _ => self.handle_mode_exit_action(action, editor_action),
        }
    }

    fn handle_mode_exit_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::EnterNormalMode => {
                if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.commit_pending_ghost();
                }
                if !self.current_mode.is_visual() {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.selection_set.clear();
                        self.region_build_recording.clear();
                    }
                }
                if self.current_mode == Mode::Insert || self.current_mode == Mode::Replace {
                    if !self.dot_repeat.is_replaying() {
                        self.dot_repeat.finish_insert_recording();
                    }
                    if !self.pending_multi_insert_anchors.is_empty() {
                        self.replay_multi_insert_at_remaining_anchors();
                    }
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        if doc.buffer.get_col() > 0 {
                            doc.buffer.move_left();
                        }
                        doc.buffer.clear_desired_col();
                    }
                }
                self.state.command_history.reset_navigation();
                self.state.search_history.reset_navigation();
                #[cfg(feature = "lsp")]
                {
                    self.rename_context = None;
                }
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
            EditorAction::Submit => match self.current_mode {
                Mode::Command => {
                    self.handle_mode_management(crate::command::Command::ExecuteCommandLine);
                    true
                }
                Mode::Search => {
                    self.handle_mode_management(crate::command::Command::ExecuteSearch);
                    true
                }
                Mode::Rename => {
                    #[cfg(feature = "lsp")]
                    self.execute_lsp_rename();
                    true
                }
                _ => false,
            },
            _ => self.handle_edit_action(action, editor_action),
        }
    }

    fn handle_edit_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::Delete(crate::action::Motion::Left)
                if self.current_mode == Mode::Replace =>
            {
                self.replace_backspace();
                true
            }
            EditorAction::Delete(crate::action::Motion::PreviousWord)
                if self.current_mode == Mode::Replace =>
            {
                self.replace_delete_word();
                true
            }
            EditorAction::Delete(motion) => {
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename
                {
                    if *motion == crate::action::Motion::Left {
                        self.handle_mode_management(crate::command::Command::DeleteFromCommandLine);
                        return true;
                    }
                    if *motion == crate::action::Motion::PreviousWord {
                        self.state.delete_word_back_command_line();
                        return true;
                    }
                }
                let count = if self.pending_count > 0 {
                    self.pending_count
                } else {
                    1
                };
                if self.current_mode == Mode::Normal {
                    let viewport_height = self.render_system.viewport.visible_rows();
                    let last_search_query = self.state.last_search_query.clone();
                    let captured = self.document_manager.active_document_mut().and_then(|doc| {
                        crate::executor::compute_motion_range(
                            *motion,
                            count,
                            doc,
                            viewport_height,
                            last_search_query.as_deref(),
                        )
                        .map(|range| crate::clipboard::capture_text(&doc.buffer, &range))
                    });
                    let in_clipboard =
                        self.active_doc_is(super::super::document::Document::is_any_clipboard);
                    if let Some(text) = captured.filter(|s| !s.is_empty()) {
                        if !in_clipboard {
                            self.clipboard_ring.push(text);
                            self.refresh_clipboard_buffer_if_open();
                        }
                    }
                }
                let command = crate::command::Command::Delete(*motion, count);
                let result = self.execute_buffer_command(command);
                if result && self.current_mode == Mode::Normal && !self.dot_repeat.is_replaying() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            EditorAction::DeleteLine => {
                let count = if self.pending_count > 0 {
                    self.pending_count
                } else {
                    1
                };
                let command = crate::command::Command::DeleteLine(count);
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
                if self.current_mode == Mode::Replace {
                    self.replace_type(*c);
                    return true;
                }
                let command = crate::command::Command::InsertChar(*c);
                self.execute_buffer_command(command)
            }
            _ => self.handle_app_action(action, editor_action),
        }
    }

    fn handle_app_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
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
                        if let Some(dir) = d.directory_path() {
                            return Some(dir.clone());
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
            _ => self.handle_git_action(action, editor_action),
        }
    }

    fn handle_git_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::GitStatus => {
                self.open_git_status();
                true
            }
            EditorAction::GitCommitNew => {
                self.open_git_commit_new();
                true
            }
            EditorAction::GitCommitAmend => {
                self.open_git_commit_amend();
                true
            }
            EditorAction::GitCommitFixup => {
                self.run_git_commit_fixup();
                true
            }
            EditorAction::GitRebaseFromLogCommit => {
                self.git_rebase_from_log_commit();
                true
            }
            EditorAction::GitRebaseAbort => {
                self.abort_git_rebase();
                true
            }
            EditorAction::GitRebaseMoveUp => {
                self.git_rebase_move(false);
                true
            }
            EditorAction::GitRebaseMoveDown => {
                self.git_rebase_move(true);
                true
            }
            EditorAction::GitRebaseSetPick => {
                self.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Pick);
                true
            }
            EditorAction::GitRebaseSetSquash => {
                self.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Squash);
                true
            }
            EditorAction::GitRebaseSetFixup => {
                self.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Fixup);
                true
            }
            EditorAction::GitRebaseSetEdit => {
                self.git_rebase_set_verb(crate::git::rebase::RebaseVerb::Edit);
                true
            }
            EditorAction::GitRebaseDrop => {
                self.git_rebase_drop();
                true
            }
            EditorAction::GitRebaseToggleFold => {
                self.git_rebase_toggle_fold();
                true
            }
            EditorAction::GitRebaseOpenMessage => {
                self.git_rebase_open_message_editor();
                true
            }
            EditorAction::GitHelp => {
                self.open_git_help();
                true
            }
            _ => self.handle_ui_action(action, editor_action),
        }
    }

    fn handle_ui_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
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
                self.execute_command_line(cmd_str);
                true
            }
            _ => self.handle_operator_action(action, editor_action),
        }
    }

    fn handle_operator_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
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
                    self.set_mode(Mode::Normal);
                }
                if self.try_run_set_aware_operator(*op) {
                    self.finish_region_build(None);
                    return true;
                }
                if self.current_mode == Mode::OperatorPending {
                    if let Some(super::PendingOperator::Op(pending)) = self.pending_operator {
                        if pending == *op {
                            return self.execute_operator_linewise(pending);
                        }
                    }
                }
                self.pending_operator = Some(super::PendingOperator::Op(*op));
                self.set_mode(Mode::OperatorPending);
                true
            }
            _ => self.handle_history_action(action, editor_action),
        }
    }

    fn handle_history_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
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
                    .is_some_and(|s| s.dropdown_open);
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
                    .is_some_and(|s| s.dropdown_open);
                if dropdown_open {
                    self.handle_mode_management(Command::TabComplete);
                } else {
                    self.navigate_history_down();
                }
                true
            }
            EditorAction::DotRepeat => self.execute_dot_repeat(),
            EditorAction::QuitKey => {
                if self.state.settings.editing.quit_on_q {
                    self.should_quit = true;
                } else {
                    self.do_quit(false);
                }
                true
            }
            _ => self.handle_file_action(action, editor_action),
        }
    }

    fn handle_file_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
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

            _ => self.handle_paste_action(action, editor_action),
        }
    }

    fn handle_paste_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::Put { before } => {
                let active_doc_id = self.document_manager.active_document_id();
                let cursor_before_resolve = self
                    .document_manager
                    .active_document()
                    .map(|d| d.buffer.cursor());
                let ghost_move = self
                    .document_manager
                    .most_recent_ghost_doc()
                    .and_then(|doc_id| {
                        let doc = self.document_manager.get_document(doc_id)?;
                        let (text, live_start) = doc.most_recent_ghost()?;
                        Some((doc_id, text.to_vec(), live_start))
                    });
                self.commit_all_pending_ghosts();

                let (text, same_location_origin) = match &ghost_move {
                    Some((doc_id, text, live_start)) => {
                        let same_location = Some(*doc_id) == active_doc_id
                            && cursor_before_resolve == Some(*live_start);
                        (text.clone(), same_location.then_some(*live_start))
                    }
                    None => match self.clipboard_ring.most_recent() {
                        Some(t) => (t.to_owned(), None),
                        None => return false,
                    },
                };

                if self.try_run_set_aware_put(*before, &text) {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
                let original_cursor = self
                    .document_manager
                    .active_document()
                    .map_or(0, |d| d.buffer.cursor());
                let result = if let Some(origin) = same_location_origin {
                    self.insert_ghost_text_verbatim(&text, origin)
                } else {
                    self.insert_text_at_cursor(&text, *before)
                };
                if result {
                    self.post_paste_state = Some(PostPasteState {
                        ring_index: 0,
                        before: *before,
                        original_cursor,
                    });
                }
                result
            }

            EditorAction::CyclePaste { forward } => {
                if let Some(state) = self.post_paste_state.take() {
                    let len = self.clipboard_ring.len().max(1);
                    let next = if *forward {
                        (state.ring_index + 1) % len
                    } else {
                        (state.ring_index + len - 1) % len
                    };
                    if let Some(text) = self
                        .clipboard_ring
                        .get(next)
                        .map(std::borrow::ToOwned::to_owned)
                    {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            doc.undo();
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

            _ => self.handle_misc_action(action, editor_action),
        }
    }

    fn handle_misc_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::PutSystemClipboard { before } => {
                let text = crate::clipboard::read_system_clipboard_text();
                if let Some(text) = text {
                    let text = crate::clipboard::normalize_newlines(&text);
                    let text: Vec<crate::character::Character> = text
                        .chars()
                        .map(crate::character::Character::from)
                        .collect();
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
                    if let Some(term) = doc.terminal() {
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

            _ => self.handle_lsp_action(editor_action),
        }
    }

    fn handle_lsp_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::LspGotoDefinition => {
                #[cfg(feature = "lsp")]
                if let Some((path, line, col)) = self.lsp_request_position() {
                    if self.lsp_manager.goto_definition(&path, line, col).is_none() {
                        self.notify_no_lsp_server();
                    }
                }
                true
            }

            EditorAction::LspReferences => {
                #[cfg(feature = "lsp")]
                if let Some((path, line, col)) = self.lsp_request_position() {
                    if self.lsp_manager.references(&path, line, col).is_none() {
                        self.notify_no_lsp_server();
                    }
                }
                true
            }

            EditorAction::LspHover => {
                #[cfg(feature = "lsp")]
                if let Some((path, line, col)) = self.lsp_request_position() {
                    if self.lsp_manager.hover(&path, line, col).is_none() {
                        self.notify_no_lsp_server();
                    }
                }
                true
            }

            EditorAction::LspRename => {
                #[cfg(feature = "lsp")]
                {
                    if let Some(ctx) = self.cursor_lsp_position() {
                        self.rename_context = Some(ctx);
                        self.state.clear_command_line();
                        self.set_mode(Mode::Rename);
                    } else {
                        self.state.notify(
                            crate::notification::NotificationType::Warning,
                            "LSP rename: no file open".to_string(),
                        );
                    }
                }
                true
            }

            EditorAction::LspCodeAction => {
                #[cfg(feature = "lsp")]
                if let Some((path, line, col)) = self.lsp_request_position() {
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
                        self.notify_no_lsp_server();
                    }
                }
                true
            }

            _ => self.handle_diag_annot_action(editor_action),
        }
    }

    fn handle_diag_annot_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::LspFormat => {
                #[cfg(feature = "lsp")]
                {
                    let info = self.document_manager.active_document().and_then(|doc| {
                        let path = doc.path()?.to_path_buf();
                        let tab_size = u32::try_from(doc.options.tab_width).unwrap_or(u32::MAX);
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
                            self.notify_no_lsp_server();
                        }
                    }
                }
                true
            }

            EditorAction::LspDiagnosticNext => {
                #[cfg(feature = "lsp")]
                self.lsp_diagnostic_next();
                true
            }

            EditorAction::LspDiagnosticPrev => {
                #[cfg(feature = "lsp")]
                self.lsp_diagnostic_prev();
                true
            }

            EditorAction::LspDiagnosticsPanel => {
                #[cfg(feature = "lsp")]
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

            _ => self.handle_visual_action(editor_action),
        }
    }

    fn handle_visual_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::SurroundStart => {
                let count = self.pending_count;
                self.pending_count = 0;
                self.pending_grammar =
                    Some(super::pending_grammar::PendingGrammar::SurroundVerb { count });
                true
            }
            EditorAction::SurroundGiveLine => {
                use crate::text_objects::{Direction, Modifier, ObjectKind, TextObjectSpec};
                let Some(super::PendingOperator::SurroundAdd(delim_count)) =
                    self.pending_operator.take()
                else {
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
            _ => self.handle_regions_list_action(editor_action),
        }
    }

    fn handle_regions_list_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::RegionsListDown
            | EditorAction::RegionsListUp
            | EditorAction::RegionsListSelect => {
                let Some(layout) = self.panel_layout.clone() else {
                    return false;
                };
                if layout.kind != crate::editor::PanelKind::Regions {
                    return false;
                }
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
                    .map_or(0, |d| d.buffer.line_index.get_line_at(d.buffer.cursor()));
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
            _ => self.handle_bank_action(editor_action),
        }
    }

    fn handle_bank_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
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
                if let Some((region, needle)) =
                    doc.selection_set.bank_occurrence(&buf_snapshot, forward)
                {
                    let (start, _) = region.span();
                    let _ = doc.buffer.set_cursor(start);
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        format!("Banked occurrence of \"{needle}\""),
                    );
                    true
                } else {
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        "No further occurrence to bank".to_string(),
                    );
                    false
                }
            }
            EditorAction::AddSurroundToSet { ch, delim_count } => {
                self.try_run_set_aware_add_surround(*ch, *delim_count)
            }
            _ => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    "internal error: unhandled editor action".to_string(),
                );
                false
            }
        }
    }

    const fn resolve_motion(&mut self, motion: &crate::action::Motion) -> crate::action::Motion {
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
                self.state.last_find_char = Some((*ch, true, false));
            }
            Motion::FindCharBackward(ch) => {
                self.state.last_find_char = Some((*ch, false, false));
            }
            Motion::TillCharForward(ch) => {
                self.state.last_find_char = Some((*ch, true, true));
            }
            Motion::TillCharBackward(ch) => {
                self.state.last_find_char = Some((*ch, false, true));
            }
            _ => {}
        }
        resolved
    }

    fn enter_visual_or_resume(&mut self, mode: Mode) -> bool {
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

    fn commit_all_pending_ghosts(&mut self) {
        let doc_ids: Vec<crate::document::DocumentId> = self
            .document_manager
            .iter_documents()
            .map(|d| d.id)
            .collect();
        for id in doc_ids {
            if let Some(doc) = self.document_manager.get_document_mut(id) {
                doc.commit_pending_ghost();
            }
        }
        self.document_manager.set_most_recent_ghost_doc(None);
    }

    fn insert_ghost_text_verbatim(
        &mut self,
        text: &[crate::character::Character],
        pos: usize,
    ) -> bool {
        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        let _ = doc.buffer.set_cursor(pos);
        doc.begin_transaction("Put");
        doc.insert_characters(text);
        doc.commit_transaction();
        let _ = doc.buffer.set_cursor(pos);
        self.do_incremental_syntax_parse();
        true
    }

    pub(super) fn insert_text_at_cursor(
        &mut self,
        text: &[crate::character::Character],
        before: bool,
    ) -> bool {
        let is_linewise = matches!(text.last(), Some(crate::character::Character::Newline));

        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        if doc.is_read_only() {
            return false;
        }

        let mut needs_leading_newline = false;

        if is_linewise {
            if before {
                doc.buffer.move_to_line_start();
            } else {
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
                    doc.buffer.move_to_end();
                    needs_leading_newline = true;
                }
            }
        } else if !before {
            doc.buffer.move_right();
        }

        let linewise_start = if is_linewise {
            if needs_leading_newline {
                Some(doc.buffer.cursor() + 1)
            } else {
                Some(doc.buffer.cursor())
            }
        } else {
            None
        };

        doc.begin_transaction("Put");
        if needs_leading_newline {
            doc.insert_characters(&[crate::character::Character::Newline]);
            let trimmed = text
                .strip_suffix(&[crate::character::Character::Newline])
                .unwrap_or(text);
            doc.insert_characters(trimmed);
        } else {
            doc.insert_characters(text);
        }
        doc.commit_transaction();

        if let Some(start) = linewise_start {
            let _ = doc.buffer.set_cursor(start);
        }

        self.do_incremental_syntax_parse();
        true
    }

    pub(super) fn insert_pasted_text_at_cursor(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        if doc.is_read_only() {
            return false;
        }
        doc.begin_transaction("Paste");
        let _ = doc.insert_str(text);
        doc.commit_transaction();
        self.do_incremental_syntax_parse();
        true
    }
}
