//! Command definitions
//! Declarative registry of all editor commands

use crate::command_line::commands::{MatchResult, ParsedCommand};
use crate::command_line::settings::SettingsRegistry;
use crate::state::UserSettings;

/// Function pointer type for command factories
/// Takes the settings registry, arguments, and bang count, returns a ParsedCommand
pub type CommandFactory = fn(&SettingsRegistry<UserSettings>, &[&str], usize) -> ParsedCommand;

/// Descriptor for a command
#[derive(Clone, Copy)]
pub struct CommandDescriptor {
    /// Canonical name of the command
    pub name: &'static str,
    /// List of aliases
    pub aliases: &'static [&'static str],
    /// Description for help text
    pub description: &'static str,
    /// Factory function to create the ParsedCommand
    pub factory: Option<CommandFactory>,
    /// Subcommands
    pub subcommands: &'static [CommandDescriptor],
}

// Factory functions

fn parse_quit(
    _registry: &SettingsRegistry<UserSettings>,
    _args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    ParsedCommand::Quit { bangs }
}

fn parse_write(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
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

fn parse_write_quit(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
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

fn parse_edit(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
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

fn parse_notify(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
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

fn parse_redraw(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if !args.is_empty() {
        return ParsedCommand::Unknown {
            name: "redraw (usage: :redraw)".to_string(),
        };
    }
    ParsedCommand::Redraw { bangs }
}

fn parse_bnext(
    _registry: &SettingsRegistry<UserSettings>,
    _args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    ParsedCommand::BufferNext { bangs }
}

fn parse_bprev(
    _registry: &SettingsRegistry<UserSettings>,
    _args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    ParsedCommand::BufferPrevious { bangs }
}

fn parse_blist(
    _registry: &SettingsRegistry<UserSettings>,
    _args: &[&str],
    _bangs: usize,
) -> ParsedCommand {
    ParsedCommand::BufferList
}

// Set command logic
fn parse_set_impl(
    registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
    local: bool,
) -> ParsedCommand {
    if args.is_empty() {
        return ParsedCommand::Unknown {
            name: "set".to_string(),
        };
    }

    let option_str = args[0];
    let option_registry = registry.build_option_registry();

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
                // Fall through to try as regular option
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

    // Boolean on (no value specified)
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

fn parse_set(
    registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    parse_set_impl(registry, args, bangs, false)
}

fn parse_setlocal(
    registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    parse_set_impl(registry, args, bangs, true)
}

/// Static registry of all commands
pub const COMMANDS: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "quit",
        aliases: &["q"],
        description: "Quit the editor",
        factory: Some(parse_quit),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "write",
        aliases: &["w"],
        description: "Save the current file",
        factory: Some(parse_write),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "wq",
        aliases: &[],
        description: "Save the current file and quit",
        factory: Some(parse_write_quit),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "edit",
        aliases: &["e"],
        description: "Edit a file",
        factory: Some(parse_edit),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "set",
        aliases: &["se"],
        description: "Set an option",
        factory: Some(parse_set),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "setlocal",
        aliases: &["setl"],
        description: "Set a local option",
        factory: Some(parse_setlocal),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "notify",
        aliases: &[],
        description: "Show a notification",
        factory: Some(parse_notify),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "redraw",
        aliases: &[],
        description: "Redraw the screen",
        factory: Some(parse_redraw),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "buffer",
        aliases: &["b"],
        description: "Buffer management",
        factory: None,
        subcommands: &[
            CommandDescriptor {
                name: "next",
                aliases: &["n"],
                description: "Next buffer",
                factory: Some(parse_bnext),
                subcommands: &[],
            },
            CommandDescriptor {
                name: "previous",
                aliases: &["prev", "p"],
                description: "Previous buffer",
                factory: Some(parse_bprev),
                subcommands: &[],
            },
            CommandDescriptor {
                name: "list",
                aliases: &["ls", "l"],
                description: "List buffers",
                factory: Some(parse_blist),
                subcommands: &[],
            },
        ],
    },
    // =================
    // TOP LEVEL ALIASES
    // =================
    CommandDescriptor {
        name: "bnext",
        aliases: &["bn"],
        description: "Next buffer",
        factory: Some(parse_bnext),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "bprev",
        aliases: &["bp"],
        description: "Previous buffer",
        factory: Some(parse_bprev),
        subcommands: &[],
    },
    CommandDescriptor {
        name: "ls",
        aliases: &[],
        description: "List buffers",
        factory: Some(parse_blist),
        subcommands: &[],
    },
];
