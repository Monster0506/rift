use super::Editor;
use crate::command_line::commands::CommandExecutor;
use crate::error::RiftError;
use crate::mode::Mode;
use crate::search::SearchDirection;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn update_search_highlights(&mut self) {
        if let Some(query) = self.state.last_search_query.clone() {
            let doc = self.document_manager.active_document_mut().unwrap();
            match doc.find_all_matches(&query) {
                Ok((matches, _)) => {
                    self.state.search_matches = matches;
                }
                Err(_) => {
                    self.state.search_matches.clear();
                }
            }
        } else {
            self.state.search_matches.clear();
        }
    }

    /// Execute a command line string
    pub(super) fn execute_command_line(&mut self, cmd: String) {
        use crate::command_line::commands::executor::ExecutionResult;
        let parsed_command = self.command_parser.parse(&cmd);
        let active_id = self.active_document_id();
        let result = CommandExecutor::execute(
            parsed_command,
            &mut self.state,
            self.document_manager
                .get_document_mut(active_id)
                .expect("active document missing"),
            &self.settings_registry,
            &self.document_settings_registry,
        );
        match result {
            ExecutionResult::Failure => return, // keep command line visible for editing
            ExecutionResult::OpenTerminal { cmd, .. } => {
                if let Err(e) = self.open_terminal(cmd) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    self.set_mode(Mode::Insert);
                }
                return;
            }
            ExecutionResult::Success => {}
            ExecutionResult::Quit { bangs } => {
                self.do_quit(bangs > 0);
            }
            ExecutionResult::Write => {
                self.do_save();
            }
            ExecutionResult::WriteAndQuit => {
                self.do_save_and_quit();
            }
            ExecutionResult::Redraw => {
                self.state.clear_command_line();
                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
            }
            ExecutionResult::Reload => {
                if let Err(e) = self.load_plugins() {
                    self.state.handle_error(e)
                }
            }
            ExecutionResult::Edit { path, bangs } => {
                if let Err(e) = self.open_file(path, bangs > 0) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    if let Err(e) = self.force_full_redraw() {
                        self.state.handle_error(e);
                    }
                }
            }
            ExecutionResult::BufferNext { .. } => {
                self.do_buffer_next();
            }
            ExecutionResult::BufferPrevious { .. } => {
                self.do_buffer_prev();
            }
            ExecutionResult::BufferList => {
                self.do_show_buffer_list();
            }
            ExecutionResult::NotificationClear { bangs } => {
                self.do_notification_clear(bangs > 0);
            }
            ExecutionResult::Undo { count } => {
                self.do_undo(count);
            }
            ExecutionResult::Redo { count } => {
                self.do_redo(count);
            }
            ExecutionResult::UndoGoto { seq } => {
                self.do_undo_goto(seq);
            }
            ExecutionResult::Checkpoint => {}
            ExecutionResult::SplitWindow {
                direction,
                subcommand,
            } => {
                self.do_split_window(direction, subcommand);
            }
            ExecutionResult::OpenDirectory { path } => {
                self.open_explorer(path);
            }
            ExecutionResult::OpenUndoTree => {
                self.open_undotree_split();
            }
            ExecutionResult::OpenMessages { show_all } => {
                self.open_messages(show_all);
            }
            ExecutionResult::OpenClipboard => {
                self.open_clipboard();
            }
            ExecutionResult::PluginCommand { name, args } => {
                if name == "lua" {
                    let s = cmd.trim_start_matches(':').trim();
                    let code = s.strip_prefix("lua").map(|r| r.trim_start()).unwrap_or("");
                    if let Some(err) = self.plugin_host.lua_exec(code) {
                        self.state
                            .notify(crate::notification::NotificationType::Error, err);
                    }
                    self.apply_plugin_mutations();
                } else if !self.plugin_host.execute_command(&name, &args) {
                    self.state.handle_error(crate::error::RiftError::new(
                        crate::error::ErrorType::Parse,
                        "UNKNOWN_COMMAND",
                        format!("Unknown command: {name}"),
                    ));
                    return;
                } else {
                    self.apply_plugin_mutations();
                }
            }
        }
        if self.current_mode == Mode::Command {
            self.set_mode(Mode::Normal);
        }
        // Sync clipboard ring capacity in case clipboard.size was just changed
        let desired = self.state.settings.clipboard_ring_size;
        if self.clipboard_ring.capacity() != desired {
            self.clipboard_ring.set_capacity(desired);
            self.refresh_clipboard_buffer_if_open();
        }
    }

    pub(super) fn handle_command_line_message(
        &mut self,
        msg: crate::message::CommandLineMessage,
    ) -> Result<(), RiftError> {
        use crate::message::CommandLineMessage;
        match msg {
            CommandLineMessage::ExecuteCommand(cmd) => {
                self.execute_command_line(cmd);
                self.state.clear_command_line();
            }
            CommandLineMessage::ExecuteSearch(query) => {
                if !query.is_empty() {
                    if self.perform_search(&query, SearchDirection::Forward, false) {
                        self.state.clear_command_line();
                    }
                } else {
                    self.state.clear_command_line();
                }
            }
            CommandLineMessage::CancelMode => {
                self.state.completion_session = None;
                self.state.clear_command_line();
            }
            CommandLineMessage::RequestCompletion(input) => {
                use crate::job_manager::jobs::completion::CompletionJob;
                let current_settings = Some(self.state.settings.clone());
                let doc = self.active_document();
                let current_doc_options = Some(doc.options.clone());
                let line_count = doc.buffer.get_total_lines();
                let buf_text = doc.buffer.to_string();
                let buf_words = {
                    let mut words: Vec<String> = buf_text
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .filter(|w| w.len() >= 2)
                        .map(|w| w.to_string())
                        .collect();
                    words.sort_unstable();
                    words.dedup();
                    words
                };
                let plugin_commands = self.plugin_host.command_list();
                self.job_manager.spawn(CompletionJob {
                    input,
                    current_settings,
                    current_doc_options,
                    plugin_commands,
                    line_count,
                    buf_words,
                });
            }
        }
        Ok(())
    }
}
