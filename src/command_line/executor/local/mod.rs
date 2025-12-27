use crate::command_line::executor::ExecutionResult;
use crate::command_line::parser::ParsedCommand;
use crate::command_line::settings::SettingsRegistry;
use crate::document::settings::DocumentOptions;
use crate::document::Document;
use crate::error::RiftError;
use crate::state::State;

pub fn execute_local_command(
    command: ParsedCommand,
    state: &mut State,
    document: &mut Document,
    document_settings_registry: &SettingsRegistry<DocumentOptions>,
) -> ExecutionResult {
    match command {
        ParsedCommand::SetLocal {
            option,
            value,
            bangs: _,
        } => {
            let mut errors = Vec::new();
            let mut error_handler = |e: RiftError| errors.push(e);
            let result = document_settings_registry.execute_setting(
                &option,
                value,
                &mut document.options,
                &mut error_handler,
            );
            for err in errors {
                state.handle_error(err);
            }
            result
        }
        _ => ExecutionResult::Failure,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
