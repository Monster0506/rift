use super::ops::quit::QuitRequest;
use super::Editor;
use super::PostPasteState;
use crate::action::{Action, EditorAction};
use crate::buffer::api::BufferView;
use crate::clipboard::Clip;
use crate::command::Command;
use crate::document::ops::PutAt;
use crate::document::runtime::{ActionDispatch, BufferKindId, NavigationPolicy};
use crate::mode::Mode;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

#[derive(Clone, Copy)]
enum PutRegister {
    Replace,
    Preserve,
}

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_action(&mut self, action: &crate::action::Action) -> bool {
        let handled = self.handle_action_inner(action);
        self.clamp_normal_cursor();
        handled
    }

    pub(super) fn clamp_normal_cursor(&mut self) {
        if self.current_mode != Mode::Normal {
            return;
        }
        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
            return;
        };
        let last = doc.buffer().line_last_char();
        if doc.buffer().cursor() > last {
            let _ = doc.set_cursor(last, crate::document::ops::CursorCause::Programmatic);
        }
    }

    fn handle_action_inner(&mut self, action: &crate::action::Action) -> bool {
        let editor_action = match action {
            Action::Editor(act) => act,
            Action::Buffer(id) => return self.action_buffer_action(id),
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
            EditorAction::Move(motion) => self.action_move(motion),
            _ => self.handle_mode_enter_action(action, editor_action),
        }
    }

    fn handle_mode_enter_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::EnterInsertMode => {
                self.action_begin_insert(action, crate::command::InsertEntry::AtCursor)
            }
            EditorAction::EnterInsertModeAfter => {
                self.action_begin_insert(action, crate::command::InsertEntry::AfterCursor)
            }
            EditorAction::EnterInsertModeAtLineStart => {
                self.action_begin_insert(action, crate::command::InsertEntry::LineStart)
            }
            EditorAction::EnterInsertModeAtLineEnd => {
                self.action_begin_insert(action, crate::command::InsertEntry::LineEnd)
            }
            EditorAction::OpenLineBelow => {
                self.action_begin_insert(action, crate::command::InsertEntry::NewLineBelow)
            }
            EditorAction::OpenLineAbove => {
                self.action_begin_insert(action, crate::command::InsertEntry::NewLineAbove)
            }
            EditorAction::EnterCommandMode => {
                self.handle_mode_management(crate::command::Command::ReadExLine);
                true
            }
            EditorAction::EnterSearchMode => {
                self.handle_mode_management(crate::command::Command::ReadSearchPattern);
                true
            }
            _ => self.handle_mode_exit_action(action, editor_action),
        }
    }

    fn handle_mode_exit_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::EnterNormalMode => self.action_enter_normal_mode(),
            EditorAction::Undo => self.do_undo(None),
            EditorAction::Redo => self.do_redo(None),
            EditorAction::Quit => {
                self.quit(QuitRequest::Close { force: false });
                true
            }
            EditorAction::Submit => self.action_submit(),
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
            EditorAction::Delete(motion) => self.action_delete(motion),
            EditorAction::DeleteLine => self.action_delete_line(),
            EditorAction::InsertChar(c) => self.action_insert_char(*c),
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
            EditorAction::Redraw => self.action_redraw(),
            EditorAction::Reload => {
                #[cfg(feature = "plugins")]
                self.load_plugins();
                true
            }
            EditorAction::Save => {
                self.save_buffer(self.active_document_id(), None, false);
                true
            }
            EditorAction::SaveAndQuit => {
                self.save_and_quit(None, false);
                true
            }
            EditorAction::OpenExplorer => self.action_open_explorer(),
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
                self.open_git(crate::git::query::GitQuery::new(["status"]));
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
            EditorAction::ClearHighlights => self.clear_search(),
            EditorAction::ClearNotifications => self.action_clear_notifications(),
            EditorAction::ClearLastNotification => self.action_clear_last_notification(),
            EditorAction::Checkpoint => self.create_checkpoint(),
            EditorAction::RunCommand(cmd_str) => {
                self.execute_command_line(cmd_str);
                true
            }
            _ => self.handle_operator_action(action, editor_action),
        }
    }

    fn handle_operator_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::GotoLine(n) => self.handle_goto_line(*n),
            EditorAction::Search(pattern) => self.action_search(pattern),
            EditorAction::VisualLineOp(op) => self.action_visual_line_op(*op, action),
            EditorAction::Operator(op) => self.handle_operator(action, *op),
            _ => self.handle_history_action(action, editor_action),
        }
    }

    fn handle_goto_line(&mut self, n: usize) -> bool {
        use crate::action::Motion;
        let target_n = if self.input.count > 0 {
            self.input.count
        } else {
            n
        };
        if self.current_mode == Mode::OperatorPending {
            if let Some(op) = self.input.operator {
                let target_line = {
                    if let Some(doc) = self
                        .document_manager
                        .get_document(self.active_document_id())
                    {
                        let last = doc.buffer().last_real_line();
                        if target_n == 0 || target_n > last + 1 {
                            last
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
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            let last = doc.buffer().last_real_line();
            let idx = if target_n == 0 || target_n > last + 1 {
                last
            } else {
                target_n - 1
            };
            let offset = doc.buffer().line_start(idx);
            let _ = doc.set_cursor(offset, crate::document::ops::CursorCause::Jump);
        }
        true
    }

    fn handle_operator(&mut self, action: &Action, op: crate::action::OperatorType) -> bool {
        if let Some(layout) = self.panel_layout.clone() {
            if layout.kind == crate::editor::PanelKind::Regions {
                self.focus_window_showing(layout.preview_win_id, layout.preview_doc_id);
                self.close_split_panel();
            }
        }
        let visual_yank_start = if op == crate::action::OperatorType::Yank {
            self.current_mode.visual_range_kind().and_then(|kind| {
                let anchor = self.visual_anchor()?;
                let doc = self
                    .document_manager
                    .get_document(self.active_document_id())?;
                Some(
                    crate::selection::Region::new(anchor, doc.buffer().cursor(), kind)
                        .buffer_span(doc.buffer())
                        .0,
                )
            })
        } else {
            None
        };
        let visual_line_delete =
            self.current_mode == Mode::VisualLine && op == crate::action::OperatorType::Delete;
        let revision = self.active_document().buffer().revision();
        if self.current_mode.is_visual() && op == crate::action::OperatorType::Yank {
            self.set_mode(Mode::Normal);
        }
        if self.try_run_set_aware_operator(op) {
            if self.current_mode.is_visual() {
                return true;
            }
            if let Some(start) = visual_yank_start {
                if let Some(doc) = self
                    .document_manager
                    .get_document_mut(self.active_document_id())
                {
                    let start = start.min(doc.buffer().len());
                    let _ = doc.set_cursor(start, crate::document::ops::CursorCause::Programmatic);
                }
            }
            if visual_line_delete && self.active_document().buffer().revision() != revision {
                if let Some(doc) = self
                    .document_manager
                    .get_document_mut(self.active_document_id())
                {
                    let line = doc.buffer().line_index.get_line_at(doc.buffer().cursor());
                    let start = doc.buffer().line_start(line);
                    let column = doc
                        .buffer()
                        .chars(start..doc.buffer().len())
                        .take_while(|&ch| ch != crate::character::Character::Newline)
                        .position(|ch| {
                            !matches!(
                                ch,
                                crate::character::Character::Unicode(' ')
                                    | crate::character::Character::Tab
                            )
                        })
                        .unwrap_or(0);
                    let _ = doc.set_cursor(
                        start + column,
                        crate::document::ops::CursorCause::Programmatic,
                    );
                }
            }
            let follow_up = (op == crate::action::OperatorType::Change).then(|| action.clone());
            self.finish_region_build(follow_up);
            return true;
        }
        if self.current_mode == Mode::OperatorPending {
            if let Some(super::pending_grammar::PendingOperator::Op(pending)) = self.input.operator
            {
                if pending == op {
                    return self.execute_operator_linewise(pending);
                }
            }
        }
        self.input.operator = Some(super::pending_grammar::PendingOperator::Op(op));
        self.set_mode(Mode::OperatorPending);
        true
    }

    fn handle_history_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::ChangeToLineEnd | EditorAction::DeleteToLineEnd => {
                self.action_to_line_end(editor_action)
            }
            EditorAction::HistoryUp => self.action_history_up(),
            EditorAction::HistoryDown => self.action_history_down(),
            EditorAction::DotRepeat => self.execute_dot_repeat(),
            EditorAction::QuitKey => self.action_quit_key(),
            _ => self.handle_file_action(action, editor_action),
        }
    }

    fn handle_file_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::OpenFile { path, force } => {
                self.open_file_from_input(path.clone(), *force);
                true
            }
            EditorAction::OpenDirectory(path) => self.action_open_directory(path),
            EditorAction::OpenTerminal(cmd) => self.open_terminal_from_input(cmd.clone()),
            EditorAction::SplitWindow {
                direction,
                subcommand,
            } => self.action_split_window(*direction, subcommand),
            EditorAction::UndoCount(count) => self.do_undo(*count),
            EditorAction::RedoCount(count) => self.do_redo(*count),
            EditorAction::UndoGoto(seq) => {
                self.do_undo_goto(*seq);
                true
            }
            EditorAction::NotificationClearAll => self.action_notification_clear_all(),
            #[cfg(feature = "plugins")]
            EditorAction::PluginAction(id) => {
                self.call_plugins(|editor| editor.plugin_host.execute_action(id));
                true
            }
            #[cfg(not(feature = "plugins"))]
            EditorAction::PluginAction(_) => true,

            _ => self.handle_paste_action(action, editor_action),
        }
    }

    fn handle_paste_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::Put { before } => self.handle_put(action, *before),
            EditorAction::CyclePaste { forward } => self.action_cycle_paste(*forward),
            _ => self.handle_misc_action(action, editor_action),
        }
    }

    fn handle_put(&mut self, action: &Action, before: bool) -> bool {
        if self.active_doc_is(crate::document::Document::is_clipboard_index) {
            let Some(register) = self.clipboard_panel_register.clone() else {
                return false;
            };
            if let Some(rows) = register.rows {
                use crate::document::clipboard::ClipboardSide;
                let side = if before {
                    ClipboardSide::Before
                } else {
                    ClipboardSide::After
                };
                return self
                    .document_manager
                    .get_document_mut(self.active_document_id())
                    .is_some_and(|doc| doc.put_clipboard_rows(&rows, side));
            }
            return self.put_into_active(&register.clip, PutAt::Cursor { before });
        }

        if self.current_mode.is_visual() {
            let register = if before {
                PutRegister::Preserve
            } else {
                PutRegister::Replace
            };
            return self.replace_visual_selection(register);
        }
        if let Some(changed) = self.put_explorer_register(before) {
            return changed;
        }
        let active = self.active_document_id();
        let ghost_doc = self
            .document_manager
            .most_recent_ghost_doc()
            .and_then(|id| self.document_manager.get_document(id));
        let source = ghost_doc
            .and_then(crate::document::Document::most_recent_ghost)
            .or_else(|| self.clipboard_ring.most_recent());
        let Some(clip) = source.cloned() else {
            return false;
        };
        let restore_here = ghost_doc.is_some_and(|doc| doc.id == active && doc.ghost_at_cursor());

        match self.try_run_set_aware_put(before, &clip) {
            Ok(true) => {
                self.commit_all_pending_ghosts();
                self.finish_region_build(Some(action.clone()));
                return true;
            }
            Ok(false) => {}
            Err(_) => return false,
        }
        let original_cursor = self
            .document_manager
            .get_document(self.active_document_id())
            .map_or(0, |d| d.buffer().cursor());
        let result = if restore_here {
            self.active_document().restore_ghost()
        } else {
            self.put_into_active(&clip, PutAt::Cursor { before })
        };
        if result {
            self.commit_all_pending_ghosts();
            self.post_paste_state = Some(PostPasteState {
                ring_index: 0,
                before,
                original_cursor,
            });
        }
        result
    }

    fn replace_visual_selection(&mut self, register: PutRegister) -> bool {
        let Some(anchor) = self.visual_anchor() else {
            return false;
        };
        let Some(kind) = self.current_mode.visual_range_kind() else {
            return false;
        };
        let clip = self
            .document_manager
            .most_recent_ghost_doc()
            .and_then(|id| self.document_manager.get_document(id))
            .and_then(crate::document::Document::most_recent_ghost)
            .or_else(|| self.clipboard_ring.most_recent())
            .cloned();
        let Some(clip) = clip else {
            return false;
        };
        let count = self.input.count.max(1);
        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
            return false;
        };
        let selected = matches!(register, PutRegister::Replace).then(|| {
            crate::clipboard::capture_text(
                doc.buffer(),
                &crate::wrap::MotionRange {
                    anchor,
                    new_cursor: doc.buffer().cursor(),
                    kind,
                    inclusive: true,
                },
            )
        });
        if !doc.put(
            &clip,
            PutAt::Selection {
                anchor,
                kind,
                count,
            },
        ) {
            return false;
        }

        self.commit_all_pending_ghosts();
        if let Some(selected) = selected {
            self.yank(selected);
            self.refresh_clipboard_buffer_if_open();
        }
        self.post_paste_state = None;
        self.set_visual_anchor(None);
        self.set_mode(Mode::Normal);

        true
    }

    fn handle_misc_action(&mut self, action: &Action, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::PutSystemClipboard { before } => {
                self.action_put_system_clipboard(*before, action)
            }

            EditorAction::ExitTerminalMode => {
                self.set_mode(Mode::Normal);
                true
            }

            EditorAction::TerminalScrollback(delta) => self.scroll_terminal(*delta),
            EditorAction::FindCharPending { forward, till } => {
                self.action_find_char_pending(*forward, *till)
            }

            EditorAction::ReplaceSelection(ch) => self.replace_selection(*ch),

            EditorAction::ReplaceCharPending => self.read_replace_char(),

            EditorAction::EnterReplaceMode => {
                self.handle_mode_management(crate::command::Command::BeginReplace);
                true
            }

            _ => self.handle_lsp_action(editor_action),
        }
    }

    fn handle_lsp_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::LspGotoDefinition => self.request_lsp_goto_definition(),

            EditorAction::LspReferences => self.request_lsp_references(),

            EditorAction::LspHover => self.request_lsp_hover(),

            EditorAction::LspRename => self.request_lsp_rename(),

            EditorAction::LspCodeAction => self.request_lsp_code_action(),

            _ => self.handle_diag_annot_action(editor_action),
        }
    }

    fn handle_diag_annot_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::LspFormat => self.request_lsp_format(),

            EditorAction::LspDiagnosticNext => self.request_lsp_diagnostic_next(),

            EditorAction::LspDiagnosticPrev => self.request_lsp_diagnostic_prev(),

            EditorAction::LspDiagnosticsPanel => self.request_lsp_diagnostics_panel(),

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
            EditorAction::SurroundStart => self.action_surround_start(),
            EditorAction::SurroundGiveLine => self.action_surround_give_line(),

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
            | EditorAction::RegionsListSelect => self.action_select_region(editor_action),
            EditorAction::VisualSwapEnds => self.action_visual_swap_ends(),
            _ => self.handle_bank_action(editor_action),
        }
    }

    fn handle_bank_action(&mut self, editor_action: &EditorAction) -> bool {
        match editor_action {
            EditorAction::RegionBankOccurrenceNext | EditorAction::RegionBankOccurrencePrev => {
                self.action_bank_occurrence(editor_action)
            }
            EditorAction::AddSurroundToSet { ch, delim_count } => self
                .try_run_set_aware_add_surround(*ch, *delim_count)
                .unwrap_or(false),
            _ => {
                self.notify(
                    crate::notification::NotificationType::Error,
                    "internal error: unhandled editor action".to_string(),
                );
                false
            }
        }
    }

    fn action_buffer_action(&mut self, id: &str) -> bool {
        let Some(action_dispatch) = self
            .document_manager
            .get_document(self.active_document_id())
            .filter(|document| !document.descriptor().is_tombstone())
            .map(|document| document.descriptor().action_dispatch)
        else {
            return true;
        };
        match action_dispatch {
            ActionDispatch::Native(actions) => self.run_kind_action(actions, id),
            #[cfg(feature = "plugins")]
            ActionDispatch::Lua => {
                let Some((doc_id, kind_name)) = self
                    .document_manager
                    .get_document(self.active_document_id())
                    .map(|document| (document.id, document.descriptor().name().to_string()))
                else {
                    return true;
                };
                self.call_plugins(|editor| {
                    editor
                        .plugin_host
                        .invoke_buffer_action(doc_id, &kind_name, id)
                });
            }
            ActionDispatch::Disabled => {}
        }
        true
    }

    fn action_move(&mut self, motion: &crate::action::Motion) -> bool {
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
                .get_document(self.active_document_id())
                .is_some_and(|d| !d.selections().is_empty());
            if has_regions {
                let forward = matches!(motion, Motion::RepeatFindForward);
                return self.cycle_to_region(forward);
            }
        }

        if self.current_mode == Mode::Normal
            && matches!(motion, Motion::Up | Motion::Down)
            && self.active_doc_is(|doc| doc.buffer_kind_id() == BufferKindId::GIT_BLAME)
        {
            if self.snap_to_actionable_line(matches!(motion, Motion::Down)) {
                self.sync_git_blame_cursor();
            }
            return true;
        }

        if self.current_mode == Mode::Normal
            && matches!(motion, Motion::Up | Motion::Down)
            && self.active_doc_is(|d| d.policies().navigation == NavigationPolicy::ActionRows)
            && self.snap_to_actionable_line(matches!(motion, Motion::Down))
        {
            self.update_explorer_preview();
            self.update_buffer_list_preview();
            return true;
        }

        let resolved = self.resolve_motion(motion);

        if self.current_mode == Mode::OperatorPending {
            if let Some(op) = self.input.operator {
                return self.execute_operator(op, resolved);
            }
        }
        let count = if self.input.count > 0 {
            self.input.count
        } else {
            1
        };
        let command = crate::command::Command::MoveBy(resolved, count);
        self.handle_mode_management(command);
        let consumed = self.execute_buffer_command(command);
        self.update_explorer_preview();
        self.update_undotree_preview();
        self.update_clipboard_preview();
        consumed
    }

    fn action_begin_insert(&mut self, action: &Action, entry: crate::command::InsertEntry) -> bool {
        let command = Command::BeginInsert(entry);
        match self.try_multi_insert_for_command(command) {
            Ok(true) => self.finish_region_build(Some(action.clone())),
            Ok(false) => self.handle_mode_management(command),
            Err(error) => self.notify_error(error.into()),
        }
        true
    }

    fn action_enter_normal_mode(&mut self) -> bool {
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            doc.commit_pending_ghost();
        }
        if !self.current_mode.is_visual() {
            if let Some(doc) = self
                .document_manager
                .get_document_mut(self.active_document_id())
            {
                doc.clear_selection();
                self.region_build_recording.clear();
            }
        }
        if self.current_mode == Mode::Insert || self.current_mode == Mode::Replace {
            if !self.dot_repeat.is_replaying() {
                self.dot_repeat.finish_insert_recording();
            }
            if !self.input.multi_insert_anchors.is_empty() {
                self.replay_multi_insert_at_remaining_anchors();
            }
            if let Some(doc) = self
                .document_manager
                .get_document_mut(self.active_document_id())
            {
                doc.finish_edit_session();
            }
            if let Some(doc) = self
                .document_manager
                .get_document_mut(self.active_document_id())
            {
                let cursor = doc.buffer().cursor();
                let target = if doc.buffer().get_col() > 0 {
                    cursor - 1
                } else {
                    cursor
                };
                let _ = doc.set_cursor(target, crate::document::ops::CursorCause::Jump);
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
        self.input.operator = None;
        self.input.keys.clear();
        self.input.count = 0;
        true
    }

    fn action_submit(&mut self) -> bool {
        match self.current_mode {
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
        }
    }

    fn action_delete(&mut self, motion: &crate::action::Motion) -> bool {
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
        let count = self.input.count.max(1);
        if self.current_mode == Mode::Normal {
            return self.execute_resolved_operator(
                crate::action::OperatorType::Delete,
                Some(*motion),
                count,
                false,
            );
        }
        self.execute_buffer_command(Command::DeleteMotion(*motion, count))
    }

    fn action_delete_line(&mut self) -> bool {
        let count = if self.input.count > 0 {
            self.input.count
        } else {
            1
        };
        let command = crate::command::Command::DeleteRows(count);
        let result = self.execute_buffer_command(command);
        if result && self.current_mode == Mode::Normal && !self.dot_repeat.is_replaying() {
            self.dot_repeat.record_single(command);
        }
        result
    }

    fn action_insert_char(&mut self, c: char) -> bool {
        if self.current_mode == Mode::Command
            || self.current_mode == Mode::Search
            || self.current_mode == Mode::Rename
        {
            self.handle_mode_management(crate::command::Command::AppendToCommandLine(c));
            return true;
        }
        if self.current_mode == Mode::Replace {
            self.replace_type(c);
            return true;
        }
        let command = crate::command::Command::TypeChar(c);
        self.execute_buffer_command(command)
    }

    fn action_redraw(&mut self) -> bool {
        self.request_full_redraw();
        true
    }

    fn action_open_explorer(&mut self) -> bool {
        let path = self
            .document_manager
            .get_document(self.active_document_id())
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

    fn action_clear_notifications(&mut self) -> bool {
        self.state.notifications.clear_all();
        self.state.clear_command_line();
        true
    }

    fn action_clear_last_notification(&mut self) -> bool {
        self.state.notifications.clear_last();
        self.state.clear_command_line();
        true
    }

    fn action_search(&mut self, pattern: &str) -> bool {
        self.search(pattern, SearchDirection::Forward, false);
        true
    }

    fn action_visual_line_op(&mut self, op: crate::action::OperatorType, action: &Action) -> bool {
        if !self.current_mode.is_visual() {
            return false;
        }
        self.set_mode(Mode::VisualLine);
        self.handle_operator_action(action, &EditorAction::Operator(op))
    }

    fn action_to_line_end(&mut self, editor_action: &EditorAction) -> bool {
        let command = if matches!(editor_action, EditorAction::ChangeToLineEnd) {
            Command::Change(crate::action::Motion::EndOfLine, 1)
        } else {
            Command::DeleteMotion(crate::action::Motion::EndOfLine, 1)
        };
        if matches!(command, Command::Change(..)) {
            self.handle_mode_management(command);
            self.current_mode == Mode::Insert
        } else {
            self.execute_buffer_command(command)
        }
    }

    fn action_history_up(&mut self) -> bool {
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

    fn action_history_down(&mut self) -> bool {
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

    fn action_quit_key(&mut self) -> bool {
        self.quit(QuitRequest::Key);
        true
    }

    fn action_open_directory(&mut self, path: &std::path::Path) -> bool {
        let path = path.to_path_buf();
        self.open_explorer(path);
        true
    }

    fn action_split_window(
        &mut self,
        direction: crate::split::tree::SplitDirection,
        subcommand: &crate::command_line::commands::types::SplitSubcommand,
    ) -> bool {
        let subcommand = subcommand.clone();
        self.do_split_window(direction, subcommand);
        true
    }

    fn action_notification_clear_all(&mut self) -> bool {
        self.state.notifications.clear_all();
        self.state.clear_command_line();
        true
    }

    fn action_cycle_paste(&mut self, forward: bool) -> bool {
        let Some(state) = self.post_paste_state.take() else {
            return false;
        };
        let len = self.clipboard_ring.len().max(1);
        let next = if forward {
            (state.ring_index + 1) % len
        } else {
            (state.ring_index + len - 1) % len
        };
        let Some(clip) = self.clipboard_ring.get(next).cloned() else {
            return false;
        };
        let Ok(mut doc) = self.active_document().write() else {
            self.post_paste_state = Some(state);
            return false;
        };
        doc.undo();
        let _ = doc.set_cursor(
            state.original_cursor,
            crate::document::ops::CursorCause::Programmatic,
        );
        let result = doc.put(
            &clip,
            PutAt::Cursor {
                before: state.before,
            },
        );
        if result {
            self.post_paste_state = Some(PostPasteState {
                ring_index: next,
                before: state.before,
                original_cursor: state.original_cursor,
            });
        }
        result
    }

    fn action_put_system_clipboard(&mut self, before: bool, action: &Action) -> bool {
        let text = crate::clipboard::read_system_clipboard_text();
        if let Some(text) = text {
            let text = crate::clipboard::normalize_newlines(&text);
            let text: Vec<crate::character::Character> = text
                .chars()
                .map(crate::character::Character::from)
                .collect();
            let clip = Clip::from_plain(text);
            match self.try_run_set_aware_put(before, &clip) {
                Ok(true) => {
                    self.finish_region_build(Some(action.clone()));
                    true
                }
                Ok(false) => self.put_into_active(&clip, PutAt::Cursor { before }),
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn action_find_char_pending(&mut self, forward: bool, till: bool) -> bool {
        self.input.grammar =
            Some(super::pending_grammar::PendingGrammar::FindChar { forward, till });
        true
    }

    fn action_surround_start(&mut self) -> bool {
        let count = self.input.count;
        self.input.count = 0;
        self.input.grammar = Some(super::pending_grammar::PendingGrammar::SurroundVerb { count });
        true
    }

    fn action_surround_give_line(&mut self) -> bool {
        use crate::text_objects::{Direction, Modifier, ObjectKind, TextObjectSpec};
        let Some(super::pending_grammar::PendingOperator::SurroundAdd(delim_count)) =
            self.input.operator.take()
        else {
            self.input.operator = None;
            self.set_mode(Mode::Normal);
            self.input.count = 0;
            return false;
        };
        let line_span = if self.input.count > 0 {
            self.input.count
        } else {
            1
        };
        self.input.count = 0;
        self.input.operator = None;
        let spec = TextObjectSpec {
            modifier: Modifier::Inner,
            direction: Direction::Current,
            nesting: 1,
            kind: ObjectKind::Line,
        };
        self.input.grammar = Some(super::pending_grammar::PendingGrammar::AddSurroundChar {
            motion: crate::action::Motion::TextObject(spec),
            count: line_span,
            delim_count,
        });
        true
    }

    fn action_select_region(&mut self, editor_action: &EditorAction) -> bool {
        let Some(layout) = self.panel_layout.clone() else {
            return false;
        };
        if layout.kind != crate::editor::PanelKind::Regions {
            return false;
        }
        if let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        {
            match editor_action {
                EditorAction::RegionsListDown => {
                    doc.move_cursor(crate::action::Motion::Down);
                }
                EditorAction::RegionsListUp => {
                    doc.move_cursor(crate::action::Motion::Up);
                }
                _ => {}
            }
        }
        let line = self
            .document_manager
            .get_document(self.active_document_id())
            .map_or(0, |d| {
                d.buffer().line_index.get_line_at(d.buffer().cursor())
            });
        let region = self
            .document_manager
            .get_document(layout.preview_doc_id)
            .map(|d| d.selections().sorted())
            .and_then(|sorted| sorted.get(line).copied());
        let Some(region) = region else { return false };
        if let Some(source) = self
            .document_manager
            .get_document_mut(layout.preview_doc_id)
        {
            let (start, _) = region.buffer_span(source.buffer());
            let _ = source.set_cursor(start, crate::document::ops::CursorCause::Programmatic);
        }
        if matches!(editor_action, EditorAction::RegionsListSelect) {
            self.close_split_panel();
        }
        true
    }

    fn action_visual_swap_ends(&mut self) -> bool {
        let Some(anchor) = self.visual_anchor() else {
            return false;
        };
        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
            return false;
        };
        let cursor = doc.buffer().cursor();
        doc.set_selection(Some(cursor));
        let _ = doc.set_cursor(anchor, crate::document::ops::CursorCause::User);
        true
    }

    fn action_bank_occurrence(&mut self, editor_action: &EditorAction) -> bool {
        let forward = matches!(editor_action, EditorAction::RegionBankOccurrenceNext);
        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
            return false;
        };
        if doc.selections().regions.is_empty() {
            self.notify(
                crate::notification::NotificationType::Info,
                "Bank a region first (v + Esc)".to_string(),
            );
            return false;
        }
        if let Some((region, needle)) = doc.bank_occurrence(forward) {
            let (start, _) = region.span();
            let _ = doc.set_cursor(start, crate::document::ops::CursorCause::Programmatic);
            self.notify(
                crate::notification::NotificationType::Info,
                format!("Banked occurrence of \"{needle}\""),
            );
            true
        } else {
            self.notify(
                crate::notification::NotificationType::Info,
                "No further occurrence to bank".to_string(),
            );
            false
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
        let Some(doc) = self
            .document_manager
            .get_document_mut(self.active_document_id())
        else {
            return false;
        };
        doc.resume_selection(kind);
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
        self.set_recent_ghost(None);
    }

    pub(super) fn put_into_active(&mut self, clip: &Clip, at: PutAt) -> bool {
        self.document_manager
            .get_document_mut(self.active_document_id())
            .is_some_and(|doc| doc.put(clip, at))
    }
}
