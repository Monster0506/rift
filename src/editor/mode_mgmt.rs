use super::Editor;
use crate::command::{Command, InsertEntry};
use crate::document::ops::EditSession;
use crate::executor::execute_write_command;
use crate::message::CommandLineMessage;
use crate::mode::Mode;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_mode_management(&mut self, command: Command) {
        self.handle_mode_switch(command);
        self.handle_change_command(command);
        self.handle_search_completion(command);
        self.handle_command_line_edit(command);
    }

    fn handle_mode_switch(&mut self, command: Command) {
        match command {
            Command::BeginInsert(entry) => {
                let mut doc = match self.active_document().write() {
                    Ok(doc) => doc,
                    Err(error) => {
                        self.notify_error(error.into());
                        return;
                    }
                };
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
                let mut doc = match self.active_document().write() {
                    Ok(doc) => doc,
                    Err(error) => {
                        self.notify_error(error.into());
                        return;
                    }
                };
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
                let viewport_height = self.focused_viewport().visible_rows();
                let doc_id = self.active_document_id();
                let query = self.state.last_search_query.as_deref();
                let mut doc = match self
                    .document_manager
                    .get_document_mut(doc_id)
                    .unwrap()
                    .write()
                {
                    Ok(doc) => doc,
                    Err(error) => {
                        self.notify_error(error.into());
                        return;
                    }
                };
                let expand_tabs = doc.options().expand_tabs();
                let tab_width = doc.options().tab_width();
                doc.start_edit_session(EditSession::Change);
                let _ = execute_write_command(
                    command,
                    &mut doc,
                    expand_tabs,
                    tab_width,
                    viewport_height,
                    query,
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
            self.input.operator = None;
            self.input.operator_count = 0;
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
