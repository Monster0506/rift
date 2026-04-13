use super::resolve_display_map;
use super::Editor;
use crate::command::Command;
use crate::executor::execute_command;
use crate::mode::Mode;
use crate::search::SearchDirection;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_mode_management(&mut self, command: crate::command::Command) {
        match command {
            Command::EnterInsertMode => {
                // Start transaction for grouping insert mode edits
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAfter => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.buffer.move_right();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAtLineStart => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.buffer.move_to_line_start();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAtLineEnd => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.buffer.move_to_line_end();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::OpenLineBelow => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                doc.buffer.move_to_line_end();
                let _ = doc.insert_char('\n');
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::OpenLineAbove => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                doc.buffer.move_to_line_start();
                let _ = doc.insert_char('\n');
                doc.buffer.move_up();
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::Change(_, _) | Command::ChangeLine => {
                let (expand_tabs, tab_width) = {
                    let doc = self.document_manager.active_document().unwrap();
                    (doc.options.expand_tabs, doc.options.tab_width)
                };
                let viewport_height = self.render_system.viewport.visible_rows();
                let display_map = {
                    let doc = self.document_manager.active_document().unwrap();
                    let gutter_width = if self.state.settings.show_line_numbers {
                        self.state.gutter_width
                    } else {
                        0
                    };
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
                        self.state.settings.soft_wrap,
                        self.state.settings.wrap_width,
                    )
                };
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                let _ = execute_command(
                    command,
                    self.document_manager.active_document_mut().unwrap(),
                    expand_tabs,
                    tab_width,
                    viewport_height,
                    self.state.last_search_query.as_deref(),
                    display_map.as_ref(),
                );
                self.set_mode(Mode::Insert);
            }
            Command::EnterCommandMode => {
                self.state.completion_session = None;
                self.state.clear_command_line();
                self.state.command_history.reset_navigation();
                self.set_mode(Mode::Command);
            }
            Command::EnterSearchMode => {
                self.state.clear_command_line();
                self.state.search_history.reset_navigation();
                self.set_mode(Mode::Search);
            }
            Command::ExecuteSearch => {
                let query = self.state.command_line.clone();
                if !query.is_empty() {
                    // Add to search history before executing
                    self.state.search_history.add(query.clone());
                    self.state.last_search_query = Some(query.clone());
                    self.state.last_find_char = None;
                    if self.perform_search(&query, SearchDirection::Forward, false) {
                        self.state.clear_command_line();
                        self.set_mode(Mode::Normal);
                    }
                } else {
                    self.state.search_history.reset_navigation();
                    self.state.clear_command_line();
                    self.set_mode(Mode::Normal);
                }
            }

            Command::TabComplete => {
                if let Some(session) = &mut self.state.completion_session {
                    if !session.dropdown_open {
                        session.dropdown_open = true;
                        session.selected = Some(0);
                    } else {
                        session.select_next();
                    }
                    let picked = session.selected_text().map(|s| s.to_string());
                    let ts = session.token_start;
                    if let Some(text) = picked {
                        self.apply_completion_text(&text, ts);
                    }
                } else {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }

            Command::TabCompletePrev => {
                if let Some(session) = &mut self.state.completion_session {
                    if !session.dropdown_open {
                        session.dropdown_open = true;
                    }
                    session.select_prev();
                    let picked = session.selected_text().map(|s| s.to_string());
                    let ts = session.token_start;
                    if let Some(text) = picked {
                        self.apply_completion_text(&text, ts);
                    }
                } else {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }

            Command::ExecuteCommandLine => {
                if let Some(session) = &self.state.completion_session {
                    if session.dropdown_open && session.selected.is_some() {
                        let text = session.selected_text().map(|s| s.to_string());
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
                self.execute_command_line(command_line);
            }
            _ => {}
        }

        // Handle command line editing (mutations happen here)
        match command {
            Command::AppendToCommandLine(ch) => {
                let was_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                self.state.completion_session = None;
                self.state.append_to_command_line(ch);
                if was_open {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }
            Command::DeleteFromCommandLine => {
                let was_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                self.state.completion_session = None;
                self.state.remove_from_command_line();
                if was_open {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }
            Command::Move(crate::action::Motion::Left, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_left();
            }
            Command::Move(crate::action::Motion::Right, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_right();
            }
            Command::Move(crate::action::Motion::StartOfLine, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_home();
            }
            Command::Move(crate::action::Motion::EndOfLine, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_end();
            }
            Command::Move(crate::action::Motion::PreviousWord, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_word_left();
            }
            Command::Move(crate::action::Motion::NextWord, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_word_right();
            }
            Command::DeleteForward
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.delete_forward_command_line();
            }
            _ => {}
        }
    }
    pub(super) fn set_mode(&mut self, mode: Mode) {
        let old_mode = self.current_mode;
        self.current_mode = mode;

        if old_mode != mode {
            self.update_lua_state();
            self.plugin_host
                .dispatch(&crate::plugin::EditorEvent::ModeChanged {
                    from: old_mode,
                    to: mode,
                });
            self.apply_plugin_mutations();
        }
        if (old_mode == Mode::Command || old_mode == Mode::Search) && mode != old_mode {
            self.state.completion_session = None;
            self.render_system
                .compositor
                .clear_layer(crate::layer::LayerPriority::FLOATING_WINDOW);
        }

        // Clear operator if leaving OperatorPending (and not entering it)
        if mode != Mode::OperatorPending {
            self.pending_operator = None;
        }

        match mode {
            Mode::Command => {
                // Command line handled via RenderSystem state
            }
            Mode::Search => {
                // Search line handled via RenderSystem state
            }
            _ => {}
        }
    }
}
