//! Command executor
//! Executes parsed commands and updates editor state

use crate::command_line::parser::ParsedCommand;
use crate::command_line::settings::SettingsRegistry;
use crate::document::{settings::DocumentOptions, Document};
use crate::error::{ErrorType, RiftError};
use crate::state::State;
use crate::state::UserSettings;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    /// Command executed successfully
    Success,
    /// Quit command - editor should exit
    Quit { bangs: usize },
    /// Write and quit - editor should save then exit
    WriteAndQuit,
    /// Error occurred during execution (already reported to manager)
    Failure,
    /// Force a full redraw
    Redraw,
}

/// Command executor
pub struct CommandExecutor;

impl CommandExecutor {
    /// Execute a parsed command
    ///
    /// Modifies state as needed and returns the execution result
    ///
    /// Note: Write commands do NOT perform file I/O here.
    /// They return Success/WriteAndQuit, and the editor is responsible
    /// for calling Document::save() or Document::save_as().
    pub fn execute(
        command: ParsedCommand,
        state: &mut State,
        document: &mut Document,
        settings_registry: &SettingsRegistry<UserSettings>,
        document_settings_registry: &SettingsRegistry<DocumentOptions>,
    ) -> ExecutionResult {
        match command {
            ParsedCommand::Quit { bangs } => ExecutionResult::Quit { bangs },
            ParsedCommand::Set {
                option,
                value,
                bangs: _,
            } => {
                let mut errors = Vec::new();
                let mut error_handler = |e: RiftError| errors.push(e);
                let result = settings_registry.execute_setting(
                    &option,
                    value,
                    &mut state.settings,
                    &mut error_handler,
                );
                for err in errors {
                    state.handle_error(err);
                }
                result
            }
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
            ParsedCommand::Write { path, bangs: _ } => {
                // Set the path in state if provided (for :w filename)
                if let Some(ref file_path) = path {
                    state.set_file_path(Some(file_path.clone()));
                }
                // Editor will check if path exists and call Document::save()
                ExecutionResult::Success
            }
            ParsedCommand::WriteQuit { path, bangs: _ } => {
                // Set the path in state if provided (for :wq filename)
                if let Some(ref file_path) = path {
                    state.set_file_path(Some(file_path.clone()));
                }
                // Editor will check if path exists, call Document::save(), then quit
                ExecutionResult::WriteAndQuit
            }
            ParsedCommand::Unknown { name } => {
                state.handle_error(RiftError::new(
                    ErrorType::Parse,
                    "UNKNOWN_COMMAND",
                    format!("Unknown command: {name}"),
                ));
                ExecutionResult::Failure
            }
            ParsedCommand::Ambiguous { prefix, matches } => {
                let matches_str = matches.join(", ");
                state.handle_error(RiftError::new(
                    ErrorType::Parse,
                    "AMBIGUOUS_COMMAND",
                    format!("Ambiguous command '{prefix}': matches {matches_str}"),
                ));
                ExecutionResult::Failure
            }
            ParsedCommand::Redraw { bangs: _ } => ExecutionResult::Redraw,

            ParsedCommand::Notify {
                kind,
                message,
                bangs: _,
            } => {
                use crate::notification::NotificationType;
                let notification_kind = match kind.to_lowercase().as_str() {
                    "info" => NotificationType::Info,
                    "warning" | "warn" => NotificationType::Warning,
                    "error" => NotificationType::Error,
                    "success" => NotificationType::Success,
                    _ => {
                        state.handle_error(RiftError::new(
                            ErrorType::Execution,
                            "INVALID_NOTIFY_TYPE",
                            format!("Unknown notification type: {kind}"),
                        ));
                        return ExecutionResult::Failure;
                    }
                };

                state.notify(notification_kind, message);
                ExecutionResult::Success
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
mod tests_local;
