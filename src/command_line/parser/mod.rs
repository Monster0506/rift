//! Command parser
//! Parses command line input into structured command data

use crate::command_line::registry::{CommandRegistry, MatchResult};

/// Parsed command representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCommand {
    /// Quit command
    Quit,
    /// Set command with option and optional value
    Set {
        option: String,
        value: Option<String>,
    },
    /// Unknown command
    Unknown {
        name: String,
    },
    /// Ambiguous command (multiple matches)
    Ambiguous {
        prefix: String,
        matches: Vec<String>,
    },
}

/// Command parser
pub struct CommandParser {
    registry: CommandRegistry,
}

impl CommandParser {
    /// Create a new parser with the given registry
    pub fn new(registry: CommandRegistry) -> Self {
        CommandParser { registry }
    }

    /// Parse a command line string
    /// 
    /// Input format: `:command [args...]`
    /// The leading colon is optional but typically present in ex-style commands
    pub fn parse(&self, input: &str) -> ParsedCommand {
        let input = input.trim();
        
        // Remove leading colon if present
        let input = if input.starts_with(':') {
            &input[1..]
        } else {
            input
        };
        
        let input = input.trim();
        
        if input.is_empty() {
            return ParsedCommand::Unknown {
                name: String::new(),
            };
        }

        // Split into command and arguments
        let parts: Vec<&str> = input.split_whitespace().collect();
        let command_name = parts[0];
        let args = &parts[1..];

        // Match command name using registry
        match self.registry.match_command(command_name) {
            MatchResult::Exact(name) | MatchResult::Prefix(name) => {
                self.parse_command(&name, args)
            }
            MatchResult::Ambiguous { prefix, matches } => {
                ParsedCommand::Ambiguous {
                    prefix,
                    matches,
                }
            }
            MatchResult::Unknown(_) => ParsedCommand::Unknown {
                name: command_name.to_string(),
            },
        }
    }

    /// Parse a matched command with its arguments
    fn parse_command(&self, command_name: &str, args: &[&str]) -> ParsedCommand {
        match command_name {
            "quit" => ParsedCommand::Quit,
            "set" => self.parse_set_command(args),
            _ => ParsedCommand::Unknown {
                name: command_name.to_string(),
            },
        }
    }

    /// Parse :set command arguments
    /// 
    /// Supports:
    /// - `:set option` (boolean on)
    /// - `:set nooption` (boolean off)
    /// - `:set option=value` (assignment)
    /// - `:set option value` (space-separated)
    fn parse_set_command(&self, args: &[&str]) -> ParsedCommand {
        if args.is_empty() {
            return ParsedCommand::Unknown {
                name: "set".to_string(),
            };
        }

        let option_str = args[0];
        
        // Check for "no" prefix (boolean off)
        if option_str.starts_with("no") && option_str.len() > 2 {
            let option = option_str[2..].to_string();
            return ParsedCommand::Set {
                option,
                value: Some("false".to_string()),
            };
        }

        // Check for assignment syntax: option=value
        if let Some(equals_pos) = option_str.find('=') {
            let option = option_str[..equals_pos].to_string();
            let value = option_str[equals_pos + 1..].to_string();
            return ParsedCommand::Set {
                option,
                value: Some(value),
            };
        }

        // Check for space-separated value
        if args.len() > 1 {
            let option = option_str.to_string();
            let value = args[1].to_string();
            return ParsedCommand::Set {
                option,
                value: Some(value),
            };
        }

        // Boolean on (no value specified)
        ParsedCommand::Set {
            option: option_str.to_string(),
            value: Some("true".to_string()),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;