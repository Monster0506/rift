//! Command executor
//! Executes parsed commands and updates editor state

use crate::command_line::parser::ParsedCommand;
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
    pub fn execute(command: ParsedCommand, state: &mut State) -> ExecutionResult {
        match command {
            ParsedCommand::Quit => ExecutionResult::Quit,
            ParsedCommand::Set { option, value } => {
                Self::execute_set_command(option, value, state)
            }
            ParsedCommand::Unknown { name } => {
                ExecutionResult::Error(format!("Unknown command: {}", name))
            }
            ParsedCommand::Ambiguous { prefix, matches } => {
                let matches_str = matches.join(", ");
                ExecutionResult::Error(format!(
                    "Ambiguous command '{}': matches {}",
                    prefix, matches_str
                ))
            }
        }
    }

    /// Execute a :set command
    fn execute_set_command(option: String, value: Option<String>, state: &mut State) -> ExecutionResult {
        let option_lower = option.to_lowercase();
        
        match option_lower.as_str() {
            "expandtabs" | "et" => {
                match Self::parse_boolean_value(&value) {
                    Ok(bool_value) => {
                        state.set_expand_tabs(bool_value);
                        ExecutionResult::Success
                    }
                    Err(e) => ExecutionResult::Error(e),
                }
            }
            "tabwidth" | "tw" => {
                match Self::parse_numeric_value(&value) {
                    Ok(num_value) => {
                        if num_value == 0 {
                            return ExecutionResult::Error("tabwidth must be greater than 0".to_string());
                        }
                        state.settings.tab_width = num_value;
                        ExecutionResult::Success
                    }
                    Err(e) => ExecutionResult::Error(e),
                }
            }
            _ => ExecutionResult::Error(format!("Unknown option: {}", option)),
        }
    }

    /// Parse a boolean value from string
    /// 
    /// Supports: "true", "false", "1", "0", "on", "off", "yes", "no"
    fn parse_boolean_value(value: &Option<String>) -> Result<bool, String> {
        let value = value.as_ref().ok_or_else(|| "Missing value".to_string())?;
        let value_lower = value.to_lowercase();
        
        match value_lower.as_str() {
            "true" | "1" | "on" | "yes" => Ok(true),
            "false" | "0" | "off" | "no" => Ok(false),
            _ => Err(format!("Invalid boolean value: {}", value)),
        }
    }

    /// Parse a numeric value from string
    fn parse_numeric_value(value: &Option<String>) -> Result<usize, String> {
        let value = value.as_ref().ok_or_else(|| "Missing value".to_string())?;
        value.parse::<usize>()
            .map_err(|_| format!("Invalid numeric value: {}", value))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
