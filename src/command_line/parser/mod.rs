//! Command parser
//! Parses command line input into structured command data

use crate::command_line::registry::{CommandRegistry, MatchResult};
use crate::command_line::settings::SettingsRegistry;
use crate::state::UserSettings;

/// Parsed command representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCommand {
    /// Quit command
    Quit {
        bangs: usize,
    },
    /// Set command with option and optional value
    Set {
        option: String,
        value: Option<String>,
        bangs: usize,
    },
    /// Set local command
    SetLocal {
        option: String,
        value: Option<String>,
        bangs: usize,
    },
    /// Write command (save file)
    Write {
        path: Option<String>,
        bangs: usize,
    },
    /// Write and quit command
    WriteQuit {
        path: Option<String>,
        bangs: usize,
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
    /// Notify command
    Notify {
        kind: String,
        message: String,
        bangs: usize,
    },
    Redraw {
        bangs: usize,
    },
    /// Edit command (open file)
    Edit {
        path: Option<String>,
        bangs: usize,
    },
    /// Switch to next buffer
    BufferNext {
        bangs: usize,
    },
    /// Switch to previous buffer
    BufferPrevious {
        bangs: usize,
    },
}

/// Command parser
pub struct CommandParser {
    registry: CommandRegistry,
    settings_registry: SettingsRegistry<UserSettings>,
}

impl CommandParser {
    /// Create a new parser with the given command registry and settings registry
    #[must_use]
    pub fn new(
        registry: CommandRegistry,
        settings_registry: SettingsRegistry<UserSettings>,
    ) -> Self {
        CommandParser {
            registry,
            settings_registry,
        }
    }

    /// Get the option registry for :set command options
    fn get_option_registry(&self) -> CommandRegistry {
        self.settings_registry.build_option_registry()
    }

    /// Strip trailing bangs from a command name and count them
    fn strip_bangs(input: &str) -> (&str, usize) {
        let trimmed = input.trim_end_matches('!');
        let bangs = input.len() - trimmed.len();
        (trimmed, bangs)
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

        // Split into tokens
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return ParsedCommand::Unknown {
                name: String::new(),
            };
        }

        let mut command_chain = Vec::new();
        let mut current_registry = &self.registry;
        let mut args_start_index = 0;
        let mut bangs = 0;

        // Traverse command hierarchy
        for (i, part) in parts.iter().enumerate() {
            let (name, part_bangs) = Self::strip_bangs(part);

            if name.is_empty() {
                if command_chain.is_empty() {
                    return ParsedCommand::Unknown {
                        name: part.to_string(),
                    };
                }
                break;
            }

            match current_registry.match_command(name) {
                MatchResult::Exact(canonical_name) | MatchResult::Prefix(canonical_name) => {
                    // Found a match
                    command_chain.push(canonical_name.clone());
                    bangs = part_bangs; // Update bangs from current token
                    args_start_index = i + 1;

                    // Check for subcommands
                    if let Some(cmd_def) = current_registry.get(&canonical_name) {
                        if let Some(ref sub_registry) = cmd_def.subcommands {
                            current_registry = sub_registry;
                            continue; // Continue to next token to see if it matches a subcommand
                        }
                    }
                    // No subcommands or not found, stop traversal
                    break;
                }
                MatchResult::Ambiguous { prefix, matches } => {
                    return ParsedCommand::Ambiguous { prefix, matches };
                }
                MatchResult::Unknown(_) => {
                    // Not a command in current registry.
                    // If we haven't matched anything yet, it's an unknown command.
                    if command_chain.is_empty() {
                        return ParsedCommand::Unknown {
                            name: part.to_string(),
                        };
                    }
                    // If we have matched something previously, this token is the start of arguments.
                    break;
                }
            }
        }

        if command_chain.is_empty() {
            return ParsedCommand::Unknown {
                name: parts[0].to_string(),
            };
        }

        let args = &parts[args_start_index..];
        self.parse_command(&command_chain, args, bangs)
    }

    /// Parse a matched command with its arguments
    fn parse_command(
        &self,
        command_chain: &[String],
        args: &[&str],
        bangs: usize,
    ) -> ParsedCommand {
        if command_chain.len() == 1 {
            match command_chain[0].as_str() {
                "quit" => return ParsedCommand::Quit { bangs },
                "set" => return self.parse_set_command(args, bangs, false),
                "setlocal" => return self.parse_set_command(args, bangs, true),
                "write" => return self.parse_write_command(args, bangs),
                "wq" => return self.parse_write_quit_command(args, bangs),
                "notify" => return self.parse_notify_command(args, bangs),
                "redraw" => return self.parse_redraw_command(args, bangs),
                "edit" => return self.parse_edit_command(args, bangs),
                "bnext" => return ParsedCommand::BufferNext { bangs },
                "bprev" => return ParsedCommand::BufferPrevious { bangs },
                _ => {}
            }
        } else if command_chain.len() == 2 {
            match (command_chain[0].as_str(), command_chain[1].as_str()) {
                ("buffer", "next") => return ParsedCommand::BufferNext { bangs },
                ("buffer", "prev") => return ParsedCommand::BufferPrevious { bangs },
                _ => {}
            }
        }

        ParsedCommand::Unknown {
            name: command_chain.join(" "),
        }
    }

    /// Parse :set command arguments with prefix matching
    ///
    /// Supports:
    /// - `:set option` (boolean on)
    /// - `:set nooption` (boolean off)
    /// - `:set option=value` (assignment)
    /// - `:set option value` (space-separated)
    fn parse_set_command(&self, args: &[&str], bangs: usize, local: bool) -> ParsedCommand {
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
                    return if local {
                        ParsedCommand::SetLocal {
                            option: name,
                            value: Some("false".to_string()),
                            bangs,
                        }
                    } else {
                        ParsedCommand::Set {
                            option: name,
                            value: Some("false".to_string()),
                            bangs,
                        }
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
                    return if local {
                        ParsedCommand::SetLocal {
                            option: name,
                            value: Some(value),
                            bangs,
                        }
                    } else {
                        ParsedCommand::Set {
                            option: name,
                            value: Some(value),
                            bangs,
                        }
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
                    return if local {
                        ParsedCommand::SetLocal {
                            option: option_part.to_string(),
                            value: Some(value),
                            bangs,
                        }
                    } else {
                        ParsedCommand::Set {
                            option: option_part.to_string(),
                            value: Some(value),
                            bangs,
                        }
                    };
                }
            }
        }

        // Check for space-separated value
        if args.len() > 1 {
            let value = args[1].to_string();

            match option_registry.match_command(option_str) {
                MatchResult::Exact(name) | MatchResult::Prefix(name) => {
                    return if local {
                        ParsedCommand::SetLocal {
                            option: name,
                            value: Some(value),
                            bangs,
                        }
                    } else {
                        ParsedCommand::Set {
                            option: name,
                            value: Some(value),
                            bangs,
                        }
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
                    return if local {
                        ParsedCommand::SetLocal {
                            option: option_str.to_string(),
                            value: Some(value),
                            bangs,
                        }
                    } else {
                        ParsedCommand::Set {
                            option: option_str.to_string(),
                            value: Some(value),
                            bangs,
                        }
                    };
                }
            }
        }

        // Boolean on (no value specified) - use prefix matching
        match option_registry.match_command(option_str) {
            MatchResult::Exact(name) | MatchResult::Prefix(name) => {
                if local {
                    ParsedCommand::SetLocal {
                        option: name,
                        value: Some("true".to_string()),
                        bangs,
                    }
                } else {
                    ParsedCommand::Set {
                        option: name,
                        value: Some("true".to_string()),
                        bangs,
                    }
                }
            }
            MatchResult::Ambiguous { prefix, matches } => ParsedCommand::Ambiguous {
                prefix: prefix.clone(),
                matches,
            },
            MatchResult::Unknown(_) => {
                // Unknown option, but still return Set command
                // Executor will handle the error
                if local {
                    ParsedCommand::SetLocal {
                        option: option_str.to_string(),
                        value: Some("true".to_string()),
                        bangs,
                    }
                } else {
                    ParsedCommand::Set {
                        option: option_str.to_string(),
                        value: Some("true".to_string()),
                        bangs,
                    }
                }
            }
        }
    }

    /// Parse :write command arguments
    ///
    /// Supports:
    /// - `:w` (write to current path)
    /// - `:w filename` (write to new file)
    /// - `:write filename` (write to new file)
    ///
    /// Error cases:
    /// - `:w file1 file2` (too many arguments)
    fn parse_write_command(&self, args: &[&str], bangs: usize) -> ParsedCommand {
        match args.len() {
            0 => ParsedCommand::Write { path: None, bangs },
            1 => ParsedCommand::Write {
                path: Some(args[0].to_string()),
                bangs,
            },
            _ => ParsedCommand::Unknown {
                name: "write (too many arguments)".to_string(),
            },
        }
    }

    /// Parse :wq command arguments
    ///
    /// Supports:
    /// - `:wq` (write and quit)
    /// - `:wq filename` (write to new file and quit)
    ///
    /// Error cases:
    /// - `:wq file1 file2` (too many arguments)
    fn parse_write_quit_command(&self, args: &[&str], bangs: usize) -> ParsedCommand {
        match args.len() {
            0 => ParsedCommand::WriteQuit { path: None, bangs },
            1 => ParsedCommand::WriteQuit {
                path: Some(args[0].to_string()),
                bangs,
            },
            _ => ParsedCommand::Unknown {
                name: "wq (too many arguments)".to_string(),
            },
        }
    }

    /// Parse :notify command arguments
    ///
    /// Supports:
    /// - `:notify <type> <message>`
    fn parse_notify_command(&self, args: &[&str], bangs: usize) -> ParsedCommand {
        if args.len() < 2 {
            return ParsedCommand::Unknown {
                name: "notify (usage: :notify <type> <message>)".to_string(),
            };
        }

        let kind = args[0].to_string();
        let message = args[1..].join(" ");

        ParsedCommand::Notify {
            kind,
            message,
            bangs,
        }
    }

    /// Parse :redraw command arguments
    ///
    /// Supports:
    /// - `:redraw`
    fn parse_redraw_command(&self, args: &[&str], bangs: usize) -> ParsedCommand {
        if !args.is_empty() {
            return ParsedCommand::Unknown {
                name: "redraw (usage: :redraw)".to_string(),
            };
        }

        ParsedCommand::Redraw { bangs }
    }

    /// Parse :edit command arguments
    ///
    /// Supports:
    /// - `:e` (reload current file or open empty)
    /// - `:e filename` (open file)
    fn parse_edit_command(&self, args: &[&str], bangs: usize) -> ParsedCommand {
        match args.len() {
            0 => ParsedCommand::Edit { path: None, bangs },
            1 => ParsedCommand::Edit {
                path: Some(args[0].to_string()),
                bangs,
            },
            _ => ParsedCommand::Unknown {
                name: "edit (too many arguments)".to_string(),
            },
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
