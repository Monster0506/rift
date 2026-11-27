use super::ops::quit::QuitRequest;
use super::Editor;
use crate::action::{Action, EditorAction};
use crate::command_line::commands::types::ParsedCommand;
use crate::command_line::settings::resolver::OptionScope;
use crate::mode::Mode;
use crate::split::tree::SplitDirection;
use crate::term::TerminalBackend;
use std::path::PathBuf;

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

    pub(super) fn execute_command_line(&mut self, cmd: &str) {
        let command = self.command_parser.parse(cmd);
        if self.apply_ex_command(command, cmd) && self.current_mode == Mode::Command {
            self.set_mode(Mode::Normal);
        }
    }

    fn apply_ex_command(&mut self, command: ParsedCommand, cmd: &str) -> bool {
        match command {
            ParsedCommand::Action(action) => {
                self.handle_action(&Action::Editor(action));
            }
            ParsedCommand::InvalidArgument(message) => return self.reject_ex_argument(message),
            ParsedCommand::Ambiguous { prefix, matches } => {
                return self.reject_ambiguous_ex_command(&prefix, &matches);
            }
            ParsedCommand::Unknown { name, args } => {
                return self.run_plugin_command(&name, &args, cmd)
            }
            ParsedCommand::Terminal { cmd, .. } => {
                self.open_terminal_from_input(cmd);
                return false;
            }
            ParsedCommand::Git { args, .. } => self.run_git_from_command(args),
            ParsedCommand::Checkpoint { .. } => {
                self.create_checkpoint();
            }
            ParsedCommand::Quit { bangs } => self.quit(QuitRequest::Close { force: bangs > 0 }),
            ParsedCommand::Write { path, bangs } => {
                self.save_buffer(
                    self.active_document_id(),
                    path.map(PathBuf::from),
                    bangs > 0,
                );
            }
            ParsedCommand::WriteQuit { path, bangs } => {
                self.save_and_quit(path.map(PathBuf::from), bangs > 0);
            }
            ParsedCommand::Notify {
                kind,
                message,
                bangs,
            } => return self.notify_from_command(&kind, message, bangs > 0),
            ParsedCommand::Redraw { .. } => self.redraw_from_command(),
            ParsedCommand::NoHighlight { .. } => {
                self.handle_action(&Action::Editor(EditorAction::ClearHighlights));
            }
            ParsedCommand::Substitute {
                pattern,
                replacement,
                flags,
                range,
                ..
            } => {
                return self.substitute(
                    &pattern,
                    &replacement,
                    flags.contains('g'),
                    range.as_deref() == Some("%"),
                );
            }
            ParsedCommand::Set { option, value, .. } => {
                return self.apply_ex_option(
                    &option,
                    value.as_deref(),
                    OptionScope::Resolve(self.active_document_id()),
                );
            }
            ParsedCommand::SetLocal { option, value, .. } => {
                return self.apply_ex_option(
                    &option,
                    value.as_deref(),
                    OptionScope::Local(self.active_document_id()),
                );
            }
            ParsedCommand::Reload { .. } => {
                #[cfg(feature = "plugins")]
                self.load_plugins();
            }
            ParsedCommand::Edit { path, bangs } => {
                return self.open_file_from_input(path, bangs > 0)
            }
            ParsedCommand::BufferNext { .. } => self.do_buffer_next(),
            ParsedCommand::BufferPrevious { .. } => self.do_buffer_prev(),
            ParsedCommand::BufferList => self.do_show_buffer_list(),
            ParsedCommand::BufferGoto { index, .. } => self.do_buffer_goto(index),
            ParsedCommand::OpenBufferList { .. } => self.open_buffer_list_panel(),
            ParsedCommand::BufferDelete { index, bangs } => self.do_buffer_delete(index, bangs > 0),
            ParsedCommand::Undo { count, .. } => {
                self.do_undo(count);
            }
            ParsedCommand::Redo { count, .. } => {
                self.do_redo(count);
            }
            ParsedCommand::UndoGoto { seq, .. } => self.do_undo_goto(seq),
            ParsedCommand::Split { subcommand, .. } => {
                self.do_split_window(SplitDirection::Horizontal, subcommand);
            }
            ParsedCommand::VSplit { subcommand, .. } => {
                self.do_split_window(SplitDirection::Vertical, subcommand);
            }
            ParsedCommand::File { path, .. } => return self.open_explorer_from_command(path),
            ParsedCommand::UndoTree { .. } => self.open_undotree_split(),
            ParsedCommand::Messages { show_all, .. } => self.open_messages(show_all),
            ParsedCommand::Clipboard { .. } => self.open_clipboard(),
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
                #[cfg(feature = "plugins")]
                let plugin_commands = self.plugin_host.command_list();
                #[cfg(not(feature = "plugins"))]
                let plugin_commands = Vec::new();
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

#[cfg(test)]
#[path = "command_line_handler_tests.rs"]
mod tests;
