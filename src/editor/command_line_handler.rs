use super::Editor;
use crate::command_line::commands::executor::CommandExecutor;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// Schedule a debounced `update_search_highlights`. With no active query
    /// the refresh is just a cheap clear, so it runs immediately.
    pub(super) fn schedule_search_refresh(&mut self) {
        if self.state.last_search_query.is_none() {
            self.update_search_highlights();
            return;
        }
        self.scheduler
            .search_refresh
            .arm((), crate::time::Instant::now());
    }

    /// Fire an elapsed search-refresh deadline. Called once per run-loop tick.
    pub(super) fn poll_pending_search_refresh(&mut self) {
        if self
            .scheduler
            .search_refresh
            .is_due((), crate::time::Instant::now())
        {
            self.scheduler.search_refresh.cancel(());
            self.update_search_highlights();
            let _ = self.update_and_render();
        }
    }

    /// Per-frame staleness check: a memo hit is free; an edit, undo, or doc
    /// switch arms the debounce instead of re-searching inline.
    pub(super) fn refresh_search_highlights_if_stale(&mut self) {
        match self.state.last_search_query.as_deref() {
            Some(query) => {
                let Some(doc) = self.document_manager.active_document() else {
                    return;
                };
                let synced = self
                    .search_highlights_synced
                    .as_ref()
                    .is_some_and(|(id, rev, q)| {
                        *id == doc.id && *rev == doc.buffer().revision() && q == query
                    });
                if !synced && !self.scheduler.search_refresh.is_armed(()) {
                    self.schedule_search_refresh();
                }
            }
            None => self.update_search_highlights(),
        }
    }

    pub(super) fn update_search_highlights(&mut self) {
        if let Some(query) = self.state.last_search_query.as_deref() {
            let doc = self.document_manager.active_document_mut().unwrap();
            // Called every frame; skip the full re-search and annotation
            // rebuild when the doc, its content, and the query are unchanged.
            let synced = self
                .search_highlights_synced
                .as_ref()
                .is_some_and(|(id, rev, q)| {
                    *id == doc.id && *rev == doc.buffer().revision() && q == query
                });
            if synced {
                return;
            }
            if let Ok((matches, _)) = doc.find_all_matches(query) {
                // Render highlights through ui.search annotations, not a decorator.
                doc.sync_search_annotations(&matches, None);
                let key = (doc.id, doc.buffer().revision(), query.to_string());
                self.state.search_matches = matches;
                self.search_highlights_synced = Some(key);
            } else {
                doc.clear_search_annotations();
                self.state.search_matches.clear();
                self.search_highlights_synced = None;
            }
        } else if self.search_highlights_synced.take().is_some()
            || !self.state.search_matches.is_empty()
        {
            if let Some(doc) = self.document_manager.active_document_mut() {
                doc.clear_search_annotations();
            }
            self.state.search_matches.clear();
        }
    }

    /// Execute a command line string
    pub(super) fn execute_command_line(&mut self, cmd: &str) {
        let parsed_command = self.command_parser.parse(cmd);
        let active_id = self.active_document_id();
        let result = CommandExecutor::execute(
            parsed_command,
            &mut self.state,
            self.document_manager
                .get_document_mut(active_id)
                .expect("active document missing"),
        );
        if !self.apply_command_result(result, cmd) {
            return;
        }
        if self.current_mode == Mode::Command {
            self.set_mode(Mode::Normal);
        }
    }

    fn apply_ex_option(
        &mut self,
        name: &str,
        value: Option<&str>,
        scope: crate::command_line::settings::resolver::OptionScope,
    ) -> bool {
        if let Err(error) = self.set_option(name, value, scope) {
            self.notify_error(error);
            return false;
        }
        true
    }

    fn apply_command_result(
        &mut self,
        result: crate::command_line::commands::executor::ExecutionResult,
        cmd: &str,
    ) -> bool {
        use crate::command_line::commands::executor::ExecutionResult;
        match result {
            ExecutionResult::Failure => return false, // keep command line visible for editing
            ExecutionResult::OpenTerminal { cmd, .. } => return self.open_terminal_from_ex(cmd),
            ExecutionResult::RunGit { args } => self.run_git_from_command(args),
            ExecutionResult::Success | ExecutionResult::Checkpoint => {}
            ExecutionResult::Quit { bangs } => {
                self.do_quit(bangs > 0);
            }
            ExecutionResult::Write { path } => {
                self.save_buffer(self.active_document_id(), path);
            }
            ExecutionResult::WriteAndQuit { path } => {
                self.save_and_quit(path);
            }
            ExecutionResult::Redraw => self.redraw_from_command(),
            ExecutionResult::SetOption { name, value, scope } => {
                if !self.apply_ex_option(&name, value.as_deref(), scope) {
                    return false;
                }
            }
            ExecutionResult::Reload => self.load_plugins(),
            ExecutionResult::Edit { path, bangs } => self.open_file_from_ex(path, bangs > 0),
            ExecutionResult::BufferNext { .. } => {
                self.do_buffer_next();
            }
            ExecutionResult::BufferPrevious { .. } => {
                self.do_buffer_prev();
            }
            ExecutionResult::BufferList => {
                self.do_show_buffer_list();
            }
            ExecutionResult::BufferGoto { index } => {
                self.do_buffer_goto(index);
            }
            ExecutionResult::OpenBufferList => {
                self.open_buffer_list_panel();
            }
            ExecutionResult::BufferDelete { index, bangs } => {
                self.do_buffer_delete(index, bangs > 0);
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
                if !self.run_plugin_command(&name, &args, cmd) {
                    return false;
                }
            }
        }
        true
    }

    fn open_terminal_from_ex(&mut self, cmd: Option<String>) -> bool {
        if let Err(e) = self.open_terminal(cmd) {
            self.notify_error(e);
        } else {
            self.state.clear_command_line();
            self.set_mode(Mode::Insert);
        }
        false
    }

    fn open_file_from_ex(&mut self, path: Option<String>, force: bool) {
        if let Err(e) = self.open_file(path, force) {
            self.notify_error(e);
        } else {
            self.redraw_from_command();
        }
    }

    fn run_git_from_command(&mut self, args: String) {
        self.run_git_command(args);
        self.state.clear_command_line();
    }

    fn redraw_from_command(&mut self) {
        self.state.clear_command_line();
        if let Err(e) = self.force_full_redraw() {
            self.notify_error(e);
        }
    }

    /// Returns false when the command was unknown and reported as an error.
    fn run_plugin_command(&mut self, name: &str, args: &[String], cmd: &str) -> bool {
        if name == "lua" {
            let s = cmd.trim_start_matches(':').trim();
            let code = s.strip_prefix("lua").map_or("", |r| r.trim_start());
            // First :lua in a pluginless session: snapshot state for it.
            self.plugin_host.mark_lua_used();
            self.call_plugins(|editor| {
                if let Some(err) = editor.plugin_host.lua_exec(code) {
                    editor.notify(crate::notification::NotificationType::Error, err);
                }
            });
        } else if !self.call_plugins(|editor| editor.plugin_host.execute_command(name, args)) {
            self.notify_error(crate::error::RiftError::new(
                crate::error::ErrorType::Parse,
                "UNKNOWN_COMMAND",
                format!("Unknown command: {name}"),
            ));
            return false;
        }
        true
    }

    pub(super) fn handle_command_line_message(&mut self, msg: crate::message::CommandLineMessage) {
        use crate::message::CommandLineMessage;
        match msg {
            CommandLineMessage::RequestCompletion(input) => {
                use crate::job_manager::jobs::completion::CompletionJob;
                let current_settings = Some(self.state.settings.clone());
                let doc = self.active_document();
                let current_doc_options = Some((**doc.options()).clone());
                let line_count = doc.buffer().get_total_lines();
                let buf_text = doc.buffer().to_string();
                let buf_words = {
                    let mut words: Vec<String> = buf_text
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .filter(|w| w.len() >= 2)
                        .map(std::string::ToString::to_string)
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
    }
}
