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
    Quit,
    /// Error occurred during execution
    Error(String),
}

/// Command executor
pub struct CommandExecutor;

impl CommandExecutor {
    /// Execute a parsed command
    ///
    /// Modifies state as needed and returns the execution result
    pub fn execute(
        command: ParsedCommand,
        state: &mut State,
        settings_registry: &SettingsRegistry,
    ) -> ExecutionResult {
        match command {
            ParsedCommand::Quit => ExecutionResult::Quit,
            ParsedCommand::Set { option, value } => {
                settings_registry.execute_setting(&option, value, &mut state.settings)
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
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
