//! Command executor
//! Executes parsed commands and updates editor state

use crate::command_line::parser::ParsedCommand;
use crate::command_line::settings::SettingsRegistry;
use crate::state::State;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    /// Command executed successfully
    Success,
    /// Quit command - editor should exit
    Quit { bangs: usize },
    /// Write and quit - editor should save then exit
    WriteAndQuit,
    /// Error occurred during execution
    Error(String),
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
        settings_registry: &SettingsRegistry,
    ) -> ExecutionResult {
        match command {
            ParsedCommand::Quit { bangs } => ExecutionResult::Quit { bangs },
            ParsedCommand::Set {
                option,
                value,
                bangs: _,
            } => settings_registry.execute_setting(&option, value, &mut state.settings),
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
                ExecutionResult::Error(format!("Unknown command: {name}"))
            }
            ParsedCommand::Ambiguous { prefix, matches } => {
                let matches_str = matches.join(", ");
                ExecutionResult::Error(format!(
                    "Ambiguous command '{prefix}': matches {matches_str}"
                ))
            }
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
                        return ExecutionResult::Error(format!("Unknown notification type: {kind}"))
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
