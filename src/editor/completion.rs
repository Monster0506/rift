#[allow(unused_imports)]
use crate::term::TerminalBackend;
use super::Editor;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn handle_completion_result(
        &mut self,
        payload: crate::job_manager::jobs::completion::CompletionPayload,
    ) {
        use crate::command_line::commands::completion::{resolve_completion, CompletionAction};

        let was_dropdown_open = self
            .state
            .completion_session
            .as_ref()
            .is_some_and(|s| s.dropdown_open);

        let token_start = payload.token_start;
        let action = resolve_completion(
            payload.result,
            &payload.input,
            token_start,
            &self.state.command_line,
            was_dropdown_open,
        );

        match action {
            CompletionAction::Discard => return,
            CompletionAction::Clear => {
                self.state.completion_session = None;
            }
            CompletionAction::ApplyAndClear { text, token_start } => {
                self.apply_completion_text(&text, token_start);
                self.state.completion_session = None;
            }
            CompletionAction::UpdateDropdown { candidates } => {
                let mut session = crate::state::CompletionSession::new(
                    self.state.command_line.clone(),
                    candidates,
                    token_start,
                );
                session.dropdown_open = true;
                session.selected = Some(0);
                self.state.completion_session = Some(session);
            }
            CompletionAction::ExpandPrefix {
                text,
                token_start,
                candidates,
            } => {
                self.apply_completion_text(&text, token_start);
                self.state.completion_session = Some(crate::state::CompletionSession::new(
                    self.state.command_line.clone(),
                    candidates,
                    token_start,
                ));
            }
            CompletionAction::ShowDropdown { candidates } => {
                let mut session = crate::state::CompletionSession::new(
                    self.state.command_line.clone(),
                    candidates,
                    token_start,
                );
                session.dropdown_open = true;
                session.selected = Some(0);
                self.state.completion_session = Some(session);
            }
        }

        let _ = self.update_and_render();
    }

    pub(super) fn apply_completion_text(&mut self, text: &str, token_start: usize) {
        let mut new_content = self.state.command_line[..token_start].to_string();
        new_content.push_str(text);
        self.state.command_line_cursor = new_content.len();
        self.state.command_line = new_content;
    }
}
