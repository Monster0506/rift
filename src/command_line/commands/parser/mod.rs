//! Command parser
//! Parses command line input into structured commands using declarative definitions

use crate::command_line::commands::{
    CommandDef, CommandDescriptor, CommandRegistry, MatchResult, COMMANDS,
};
use crate::command_line::settings::SettingsRegistry;
use crate::state::UserSettings;

// Re-export ParsedCommand for convenience and backward compatibility
pub use crate::command_line::commands::ParsedCommand;

/// Command parser
pub struct CommandParser {
    registry: CommandRegistry,
    settings_registry: SettingsRegistry<UserSettings>,
}

impl CommandParser {
    /// Create a new command parser
    pub fn new(settings_registry: SettingsRegistry<UserSettings>) -> Self {
        let registry = Self::build_registry();
        CommandParser {
            registry,
            settings_registry,
        }
    }

    /// Build the command registry from declarative definitions
    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        for desc in COMMANDS {
            registry = registry.register(Self::build_command_def(desc));
        }
        registry
    }

    /// Helper to convert a descriptor to a CommandDef
    fn build_command_def(desc: &CommandDescriptor) -> CommandDef {
        let mut def = CommandDef::new(desc.name)
            .with_aliases(
                desc.aliases
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
            )
            .with_description(desc.description);

        for sub in desc.subcommands {
            def = def.with_subcommand(Self::build_command_def(sub));
        }
        def
    }

    /// Parse a command string
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

        // Find the descriptor and execute factory
        self.execute_command(&command_chain, args, bangs)
    }

    fn execute_command(
        &self,
        command_chain: &[String],
        args: &[&str],
        bangs: usize,
    ) -> ParsedCommand {
        let mut current_list = COMMANDS;
        let mut current_desc: Option<&CommandDescriptor> = None;

        for name in command_chain {
            if let Some(desc) = current_list.iter().find(|d| d.name == *name) {
                current_desc = Some(desc);
                current_list = desc.subcommands;
            } else {
                // This should theoretically not happen if registry and COMMANDS are in sync
                return ParsedCommand::Unknown {
                    name: command_chain.join(" "),
                };
            }
        }

        if let Some(desc) = current_desc {
            if let Some(factory) = desc.factory {
                return factory(&self.settings_registry, args, bangs);
            }
        }

        // Command found but no factory (maybe it's a namespace command like "buffer" without args?)
        ParsedCommand::Unknown {
            name: command_chain.join(" "),
        }
    }

    /// Helper to strip trailing '!' from a command name
    /// Returns (name_without_bangs, bang_count)
    fn strip_bangs(input: &str) -> (&str, usize) {
        let trimmed = input.trim_end_matches('!');
        let bangs = input.len() - trimmed.len();
        (trimmed, bangs)
    }

    // Helper for tests
    #[cfg(test)]
    pub fn get_option_registry(&self) -> CommandRegistry {
        self.settings_registry.build_option_registry()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
