//! Command parser
//! Parses command line input into structured command data

use crate::command_line::registry::{CommandRegistry, MatchResult};
use crate::command_line::settings::SettingsRegistry;

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
    Unknown { name: String },
    /// Ambiguous command (multiple matches)
    Ambiguous {
        prefix: String,
        matches: Vec<String>,
    },
}

/// Command parser
pub struct CommandParser {
    registry: CommandRegistry,
    settings_registry: SettingsRegistry,
}

impl CommandParser {
    /// Create a new parser with the given command registry and settings registry
    #[must_use]
    pub fn new(registry: CommandRegistry, settings_registry: SettingsRegistry) -> Self {
        CommandParser {
            registry,
            settings_registry,
        }
    }

    /// Get the option registry for :set command options
    fn get_option_registry(&self) -> CommandRegistry {
        self.settings_registry.build_option_registry()
    }

    /// Parse a command line string
    ///
    /// Input format: `:command [args...]`
    /// The leading colon is optional but typically present in ex-style commands
    #[must_use]
    pub fn parse(&self, input: &str) -> ParsedCommand {
        let input = input.trim();

        // Remove leading colon if present
        let input = input.strip_prefix(':').unwrap_or(input);

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
            MatchResult::Exact(name) | MatchResult::Prefix(name) => self.parse_command(&name, args),
            MatchResult::Ambiguous { prefix, matches } => {
                ParsedCommand::Ambiguous { prefix, matches }
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

    /// Parse :set command arguments with prefix matching
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
        let option_registry = self.get_option_registry();

        // Check for "no" prefix (boolean off) - case insensitive
        let option_lower = option_str.to_lowercase();
        if option_lower.starts_with("no") && option_lower.len() > 2 {
            let option_without_no = &option_lower[2..];
            match option_registry.match_command(option_without_no) {
                MatchResult::Exact(name) | MatchResult::Prefix(name) => {
                    return ParsedCommand::Set {
                        option: name,
                        value: Some("false".to_string()),
                    };
                }
                MatchResult::Ambiguous { prefix, matches } => {
                    return ParsedCommand::Ambiguous {
                        prefix: format!("no{prefix}"),
                        matches: matches.iter().map(|m| format!("no{m}")).collect(),
                    };
                }
                MatchResult::Unknown(_) => {
                    // Fall through to try as regular option (might be unknown)
                }
            }
        }

        // Check for assignment syntax: option=value
        if let Some(equals_pos) = option_str.find('=') {
            let option_part = &option_str[..equals_pos];
            let value = option_str[equals_pos + 1..].to_string();

            match option_registry.match_command(option_part) {
                MatchResult::Exact(name) | MatchResult::Prefix(name) => {
                    return ParsedCommand::Set {
                        option: name,
                        value: Some(value),
                    };
                }
                MatchResult::Ambiguous { prefix, matches } => {
                    return ParsedCommand::Ambiguous {
                        prefix: format!("{prefix}="),
                        matches,
                    };
                }
                MatchResult::Unknown(_) => {
                    // Unknown option, but still return Set command
                    // Executor will handle the error
                    return ParsedCommand::Set {
                        option: option_part.to_string(),
                        value: Some(value),
                    };
                }
            }
        }

        // Check for space-separated value
        if args.len() > 1 {
            let value = args[1].to_string();

            match option_registry.match_command(option_str) {
                MatchResult::Exact(name) | MatchResult::Prefix(name) => {
                    return ParsedCommand::Set {
                        option: name,
                        value: Some(value),
                    };
                }
                MatchResult::Ambiguous { prefix, matches } => {
                    return ParsedCommand::Ambiguous {
                        prefix: prefix.clone(),
                        matches,
                    };
                }
                MatchResult::Unknown(_) => {
                    // Unknown option, but still return Set command
                    // Executor will handle the error
                    return ParsedCommand::Set {
                        option: option_str.to_string(),
                        value: Some(value),
                    };
                }
            }
        }

        // Boolean on (no value specified) - use prefix matching
        match option_registry.match_command(option_str) {
            MatchResult::Exact(name) | MatchResult::Prefix(name) => ParsedCommand::Set {
                option: name,
                value: Some("true".to_string()),
            },
            MatchResult::Ambiguous { prefix, matches } => ParsedCommand::Ambiguous {
                prefix: prefix.clone(),
                matches,
            },
            MatchResult::Unknown(_) => {
                // Unknown option, but still return Set command
                // Executor will handle the error
                ParsedCommand::Set {
                    option: option_str.to_string(),
                    value: Some("true".to_string()),
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
