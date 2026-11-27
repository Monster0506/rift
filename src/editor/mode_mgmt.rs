use super::resolve_display_map;
use super::Editor;
use crate::command::{Command, InsertEntry};
use crate::document::ops::EditSession;
use crate::executor::execute_command;
use crate::message::CommandLineMessage;
use crate::mode::Mode;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

/// True for commands that switch into Insert or Replace mode.
const fn enters_edit_mode(command: &Command) -> bool {
    matches!(
        command,
        Command::BeginInsert(_)
            | Command::Change(_, _)
            | Command::ChangeLine(_)
            | Command::BeginReplace
    )
}
impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_mode_management(&mut self, command: crate::command::Command) {
        if enters_edit_mode(&command) && !self.can_enter_edit_mode() {
            return;
        }
        self.handle_mode_switch(command);
        self.handle_change_command(command);
        self.handle_search_completion(command);
        self.handle_command_line_edit(command);
    }

    pub(super) fn can_enter_edit_mode(&mut self) -> bool {
        if let Err(error) = self.active_document().check_edit_access() {
            self.notify_error(error.into());
            return false;
        }
        true
    }

    fn handle_mode_switch(&mut self, command: Command) {
        match command {
            Command::BeginInsert(entry) => {
                let doc = self
                    .document_manager
                    .get_document_mut(self.active_document_id())
                    .unwrap();
                let motion = match entry {
                    InsertEntry::AtCursor => None,
                    InsertEntry::AfterCursor => Some(crate::action::Motion::Right),
                    InsertEntry::LineStart | InsertEntry::NewLineAbove => {
                        Some(crate::action::Motion::StartOfLine)
                    }
                    InsertEntry::LineEnd | InsertEntry::NewLineBelow => {
                        Some(crate::action::Motion::EndOfLine)
                    }
                };
                doc.start_edit_session(EditSession::Insert);
                if let Some(motion) = motion {
                    doc.move_cursor(motion);
                }
                if matches!(entry, InsertEntry::NewLineAbove | InsertEntry::NewLineBelow) {
                    let _ = doc.edit(
                        crate::document::ops::Edit::type_text('\n'),
                        crate::document::ops::EditSource::User,
                    );
                    if entry == InsertEntry::NewLineAbove {
                        doc.move_cursor(crate::action::Motion::Up);
                    }
                }
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::BeginReplace => {
                let doc = self
                    .document_manager
                    .get_document_mut(self.active_document_id())
                    .unwrap();
                doc.start_edit_session(EditSession::Replace);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Replace);
            }
            Command::ReadExLine => {
                self.state.completion_session = None;
                self.state.clear_command_line();
                self.state.command_history.reset_navigation();
                self.set_mode(Mode::Command);
            }
            Command::ReadSearchPattern => {
                self.state.clear_command_line();
                self.state.search_history.reset_navigation();
                self.set_mode(Mode::Search);
            }
            _ => {}
        }
    }

    fn handle_change_command(&mut self, command: Command) {
        match command {
            Command::Change(_, _) | Command::ChangeLine(_) => {
                let (expand_tabs, tab_width) = {
                    let doc = self
                        .document_manager
                        .get_document(self.active_document_id())
                        .unwrap();
                    (doc.options().expand_tabs(), doc.options().tab_width())
                };
                let viewport_height = self.render_system.viewport.visible_rows();
                let mut display_map = {
                    let doc = self
                        .document_manager
                        .get_document(self.active_document_id())
                        .unwrap();
                    let gutter_width = self.state.gutter_cols(doc.options().show_line_numbers());
                    let content_width = self
                        .split_tree
                        .focused_window()
                        .viewport
                        .visible_cols()
                        .saturating_sub(gutter_width)
                        .max(1);
                    resolve_display_map(
                        doc,
                        content_width,
                        self.state.settings.soft_wrap(),
                        self.state.settings.wrap_width(),
                    )
                };
                self.active_document()
                    .start_edit_session(EditSession::Change);
                let _ = execute_command(
                    command,
                    self.document_manager
                        .get_document_mut(self.active_document_id())
                        .unwrap(),
                    expand_tabs,
                    tab_width,
                    viewport_height,
                    self.state.last_search_query.as_deref(),
                    display_map.as_mut(),
                );
                self.set_mode(Mode::Insert);
            }
            _ => {}
        }
    }

    fn handle_search_completion(&mut self, command: Command) {
        match command {
            Command::ExecuteSearch => {
                let query = self.state.command_line.clone();
                if query.is_empty() {
                    self.state.search_history.reset_navigation();
                    self.state.clear_command_line();
                    self.set_mode(Mode::Normal);
                } else {
                    self.state.search_history.add(query.clone());
                    if self.search(&query, SearchDirection::Forward, false) {
                        self.state.clear_command_line();
                        self.set_mode(Mode::Normal);
                    }
                }
            }

            Command::TabComplete => {
                if let Some(session) = &mut self.state.completion_session {
                    if session.dropdown_open {
                        session.select_next();
                    } else {
                        session.dropdown_open = true;
                        session.selected = Some(0);
                    }
                    let picked = session
                        .selected_text()
                        .map(std::string::ToString::to_string);
                    let ts = session.token_start;
                    if let Some(text) = picked {
                        self.apply_completion_text(&text, ts);
                    }
                } else {
                    let input = self.state.command_line.clone();
                    self.handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }

            Command::TabCompletePrev => {
                if let Some(session) = &mut self.state.completion_session {
                    if !session.dropdown_open {
                        session.dropdown_open = true;
                    }
                    session.select_prev();
                    let picked = session
                        .selected_text()
                        .map(std::string::ToString::to_string);
                    let ts = session.token_start;
                    if let Some(text) = picked {
                        self.apply_completion_text(&text, ts);
                    }
                } else {
                    let input = self.state.command_line.clone();
                    self.handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }

            Command::ExecuteCommandLine => {
                if let Some(session) = &self.state.completion_session {
                    if session.dropdown_open && session.selected.is_some() {
                        let text = session
                            .selected_text()
                            .map(std::string::ToString::to_string);
                        let ts = session.token_start;
                        if let Some(text) = text {
                            self.apply_completion_text(&text, ts);
                        }
                        self.state.completion_session = None;
                        return;
                    }
                }
                self.state.completion_session = None;
                let command_line = self.state.command_line.clone();
                self.state.command_history.add(command_line.clone());
                self.execute_command_line(&command_line);
            }
            _ => {}
        }
    }

    fn handle_command_line_edit(&mut self, command: Command) {
        // Handle command line editing (mutations happen here)
        match command {
            Command::AppendToCommandLine(ch) => {
                let was_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .is_some_and(|s| s.dropdown_open);
                self.state.completion_session = None;
                self.state.append_to_command_line(ch);
                if was_open {
                    let input = self.state.command_line.clone();
                    self.handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }
            Command::DeleteFromCommandLine => {
                let was_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .is_some_and(|s| s.dropdown_open);
                self.state.completion_session = None;
                self.state.remove_from_command_line();
                if was_open {
                    let input = self.state.command_line.clone();
                    self.handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }
            Command::MoveBy(crate::action::Motion::Left, _)
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename =>
            {
                self.state.move_command_line_left();
            }
            Command::MoveBy(crate::action::Motion::Right, _)
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename =>
            {
                self.state.move_command_line_right();
            }
            Command::MoveBy(crate::action::Motion::StartOfLine, _)
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename =>
            {
                self.state.move_command_line_home();
            }
            Command::MoveBy(crate::action::Motion::EndOfLine, _)
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename =>
            {
                self.state.move_command_line_end();
            }
            Command::MoveBy(crate::action::Motion::PreviousWord, _)
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename =>
            {
                self.state.move_command_line_word_left();
            }
            Command::MoveBy(crate::action::Motion::NextWord, _)
                if self.current_mode == Mode::Command
                    || self.current_mode == Mode::Search
                    || self.current_mode == Mode::Rename =>
            {
                self.state.move_command_line_word_right();
            }
            _ => {}
        }
    }
    /// The editor's current mode.
    pub(crate) const fn mode(&self) -> Mode {
        self.current_mode
    }

    pub(super) fn set_mode(&mut self, mode: Mode) {
        let old_mode = self.current_mode;

        // Leaving Visual/VisualLine/VisualBlock for a non-visual mode always
        // banks the pending selection and clears `visual_anchor`, no matter
        // which of the many keys/grammars triggered the transition. Callers
        // used to have to remember this bookkeeping themselves, and some
        // (e.g. the surround-grammar's fallback-to-Normal arms) didn't,
        // leaving `visual_anchor` set once `mode` was no longer visual.
        if old_mode.is_visual() && !mode.is_visual() {
            self.bank_visual_selection();
        }

        self.current_mode = mode;

        if old_mode != mode && (old_mode == Mode::Replace || mode == Mode::Replace) {
            self.replace_originals.clear();
        }

        if (old_mode == Mode::Command || old_mode == Mode::Search || old_mode == Mode::Rename)
            && mode != old_mode
        {
            self.state.completion_session = None;
            self.render_system
                .compositor
                .clear_layer(crate::layer::LayerPriority::FLOATING_WINDOW);
        }

        if mode != Mode::OperatorPending {
            self.pending_operator = None;
            self.pending_operator_count = 0;
        }

        #[cfg(feature = "plugins")]
        if old_mode != mode {
            self.raise(crate::plugin::events::EventPayload::ModeChanged {
                from: old_mode,
                to: mode,
            });
        }
    }
}
