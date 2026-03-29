//! Command definitions
//! Declarative registry of all editor commands

use crate::command_line::commands::{MatchResult, ParsedCommand, SplitSubcommand};
use crate::command_line::settings::SettingsRegistry;
use crate::split::navigation::Direction;
use crate::state::UserSettings;

/// Function pointer type for command factories
pub type CommandFactory = fn(&SettingsRegistry<UserSettings>, &[&str], usize) -> ParsedCommand;

/// What kind of argument completion a command supports
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionHint {
    None,
    /// Both files and directories (e.g. :edit, :write)
    FilePath,
    /// Directories only (e.g. :file)
    Directory,
    /// Global settings (e.g. :set)
    Setting,
    /// Document-local settings (e.g. :setlocal)
    LocalSetting,
}

/// Descriptor for a command
#[derive(Clone, Copy)]
pub struct CommandDescriptor {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub factory: Option<CommandFactory>,
    pub subcommands: &'static [CommandDescriptor],
    pub completion: CompletionHint,
    /// Prefix required before subcommand tokens (e.g. ":" for `:split :left`)
    pub subcommand_prefix: &'static str,
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
            args: vec![],
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
            args: vec![],
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
            args: vec![],
        },
    }
}

fn parse_notify(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if args.len() == 1 && (args[0] == "clear" || args[0] == "clear!") {
        let extra_bangs = if args[0].ends_with('!') { 1 } else { 0 };
        return ParsedCommand::Notify {
            kind: "clear".to_string(),
            message: "".to_string(),
            bangs: bangs + extra_bangs,
        };
    }
    if args.len() < 2 {
        return ParsedCommand::Unknown {
            name: "notify (usage: :notify <type> <message>)".to_string(),
            args: vec![],
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
            args: vec![],
        };
    }
    ParsedCommand::Redraw { bangs }
}

fn parse_reload(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if !args.is_empty() {
        return ParsedCommand::Unknown {
            name: "reload (usage: :reload)".to_string(),
            args: vec![],
        };
    }
    ParsedCommand::Reload { bangs }
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

fn parse_nohighlight(
    _registry: &SettingsRegistry<UserSettings>,
    _args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    ParsedCommand::NoHighlight { bangs }
}

fn parse_undo(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if args.is_empty() {
        // Simple undo
        return ParsedCommand::Undo { count: None, bangs };
    }

    // Try to parse as sequence number (goto)
    if let Ok(seq) = args[0].parse::<u64>() {
        return ParsedCommand::UndoGoto { seq, bangs };
    }

    // Try to parse as count (for multiple undos)
    ParsedCommand::Unknown {
        name: format!("undo (invalid argument: {})", args[0]),
        args: vec![],
    }
}

fn parse_redo(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if args.is_empty() {
        // Simple redo
        return ParsedCommand::Redo { count: None, bangs };
    }

    // Try to parse as count
    if let Ok(count) = args[0].parse::<u64>() {
        return ParsedCommand::Redo {
            count: Some(count),
            bangs,
        };
    }

    ParsedCommand::Unknown {
        name: format!("redo (invalid argument: {})", args[0]),
        args: vec![],
    }
}

fn parse_checkpoint(
    _registry: &SettingsRegistry<UserSettings>,
    _args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    ParsedCommand::Checkpoint { bangs }
}

fn parse_substitute_impl(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
    range: Option<String>,
) -> ParsedCommand {
    let raw_args = args.join(" ");
    let raw_args = raw_args.trim();

    if raw_args.is_empty() {
        return ParsedCommand::Unknown {
            name: "substitute (usage: :s/pattern/replacement/flags)".to_string(),
            args: vec![],
        };
    }

    // 1. Get separator
    let separator = raw_args.chars().next().unwrap();
    let mut chars = raw_args.chars().skip(1); // Skip separator

    // 2. Parse pattern
    let mut pattern = String::new();
    let mut escaped = false;
    for c in chars.by_ref() {
        if escaped {
            pattern.push(c);
            escaped = false;
        } else if c == '\\' {
            pattern.push(c);
            escaped = true;
        } else if c == separator {
            break;
        } else {
            pattern.push(c);
        }
    }
    // 3. Parse replacement
    let mut replacement = String::new();
    escaped = false;
    let mut found_sep = false;
    // chars iterator continues from after first separator
    for c in chars.by_ref() {
        if escaped {
            replacement.push(c);
            escaped = false;
        } else if c == '\\' {
            replacement.push(c);
            escaped = true;
        } else if c == separator {
            found_sep = true;
            break;
        } else {
            replacement.push(c);
        }
    }

    // 4. Parse flags
    let mut flags_str = String::new();
    if found_sep {
        // Rest determines flags
        flags_str = chars.collect();
    }

    // 5. Separate flags
    let mut subst_flags = String::new();
    let mut regex_flags = String::new();

    for c in flags_str.chars() {
        if c == 'g' {
            subst_flags.push(c);
        } else {
            regex_flags.push(c);
        }
    }

    // append regex flags to pattern if present
    if !regex_flags.is_empty() {
        pattern.push_str(" /");
        pattern.push('/');
        pattern.push_str(&regex_flags);
    }

    ParsedCommand::Substitute {
        pattern,
        replacement,
        flags: subst_flags,
        range,
        bangs,
    }
}

fn parse_substitute(
    registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    parse_substitute_impl(registry, args, bangs, None)
}

fn parse_substitute_range(
    registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    parse_substitute_impl(registry, args, bangs, Some("%".to_string()))
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
            args: vec![],
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

fn parse_file(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    let path = if !args.is_empty() {
        Some(args[0].to_string())
    } else {
        None
    };
    ParsedCommand::File { path, bangs }
}

fn parse_undotree(
    _registry: &SettingsRegistry<UserSettings>,
    _args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    ParsedCommand::UndoTree { bangs }
}

fn parse_messages(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    let show_all = args.first().map(|a| *a == "all").unwrap_or(false);
    ParsedCommand::Messages { show_all, bangs }
}

fn parse_terminal(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    let cmd = if !args.is_empty() {
        Some(args.join(" "))
    } else {
        None
    };
    ParsedCommand::Terminal { cmd, bangs }
}

fn parse_split_base(args: &[&str], bangs: usize) -> (SplitSubcommand, usize) {
    let sub = match args.first() {
        None | Some(&".") => SplitSubcommand::Current,
        Some(path) => SplitSubcommand::File(path.to_string()),
    };
    (sub, bangs)
}

fn parse_split(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if let Some(s) = args.first() {
        if s.starts_with(':') {
            return ParsedCommand::Unknown {
                name: format!("unknown split subcommand '{s}'"),
                args: vec![],
            };
        }
    }
    let (subcommand, bangs) = parse_split_base(args, bangs);
    ParsedCommand::Split { subcommand, bangs }
}

fn parse_vsplit(
    _registry: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    if let Some(s) = args.first() {
        if s.starts_with(':') {
            return ParsedCommand::Unknown {
                name: format!("unknown vsplit subcommand '{s}'"),
                args: vec![],
            };
        }
    }
    let (subcommand, bangs) = parse_split_base(args, bangs);
    ParsedCommand::VSplit { subcommand, bangs }
}

fn split_cmd(sub: SplitSubcommand, bangs: usize) -> ParsedCommand {
    ParsedCommand::Split {
        subcommand: sub,
        bangs,
    }
}

fn vsplit_cmd(sub: SplitSubcommand, bangs: usize) -> ParsedCommand {
    ParsedCommand::VSplit {
        subcommand: sub,
        bangs,
    }
}

fn parse_split_left(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    split_cmd(SplitSubcommand::Navigate(Direction::Left), b)
}
fn parse_split_right(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    split_cmd(SplitSubcommand::Navigate(Direction::Right), b)
}
fn parse_split_up(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    split_cmd(SplitSubcommand::Navigate(Direction::Up), b)
}
fn parse_split_down(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    split_cmd(SplitSubcommand::Navigate(Direction::Down), b)
}
fn parse_split_resize(
    _: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    split_cmd(
        SplitSubcommand::Resize(args.first().and_then(|s| s.parse().ok()).unwrap_or(1)),
        bangs,
    )
}

fn parse_vsplit_left(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    vsplit_cmd(SplitSubcommand::Navigate(Direction::Left), b)
}
fn parse_vsplit_right(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    vsplit_cmd(SplitSubcommand::Navigate(Direction::Right), b)
}
fn parse_vsplit_up(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    vsplit_cmd(SplitSubcommand::Navigate(Direction::Up), b)
}
fn parse_vsplit_down(_: &SettingsRegistry<UserSettings>, _: &[&str], b: usize) -> ParsedCommand {
    vsplit_cmd(SplitSubcommand::Navigate(Direction::Down), b)
}
fn parse_vsplit_resize(
    _: &SettingsRegistry<UserSettings>,
    args: &[&str],
    bangs: usize,
) -> ParsedCommand {
    vsplit_cmd(
        SplitSubcommand::Resize(args.first().and_then(|s| s.parse().ok()).unwrap_or(1)),
        bangs,
    )
}

const SPLIT_SUB_DESC: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "left",
        aliases: &["l"],
        description: "Navigate to left pane",
        factory: Some(parse_split_left),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "right",
        aliases: &["r"],
        description: "Navigate to right pane",
        factory: Some(parse_split_right),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "up",
        aliases: &["u"],
        description: "Navigate to pane above",
        factory: Some(parse_split_up),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "down",
        aliases: &["d"],
        description: "Navigate to pane below",
        factory: Some(parse_split_down),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "resize",
        aliases: &[],
        description: "Resize pane",
        factory: Some(parse_split_resize),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
];

const VSPLIT_SUB_DESC: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "left",
        aliases: &["l"],
        description: "Navigate to left pane",
        factory: Some(parse_vsplit_left),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "right",
        aliases: &["r"],
        description: "Navigate to right pane",
        factory: Some(parse_vsplit_right),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "up",
        aliases: &["u"],
        description: "Navigate to pane above",
        factory: Some(parse_vsplit_up),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "down",
        aliases: &["d"],
        description: "Navigate to pane below",
        factory: Some(parse_vsplit_down),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "resize",
        aliases: &[],
        description: "Resize pane",
        factory: Some(parse_vsplit_resize),
        subcommands: &[],
        completion: CompletionHint::None,
        subcommand_prefix: "",
    },
];

const N: CompletionHint = CompletionHint::None;
const F: CompletionHint = CompletionHint::FilePath;
const D: CompletionHint = CompletionHint::Directory;
const S: CompletionHint = CompletionHint::Setting;
const L: CompletionHint = CompletionHint::LocalSetting;

const UNDO_SUBS: &[CommandDescriptor] = &[CommandDescriptor {
    name: "checkpoint",
    aliases: &[],
    description: "Create undo checkpoint",
    factory: Some(parse_checkpoint),
    subcommands: &[],
    completion: N,
    subcommand_prefix: "",
}];

const BUFFER_SUBS: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "next",
        aliases: &["n"],
        description: "Next buffer",
        factory: Some(parse_bnext),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "previous",
        aliases: &["prev", "p"],
        description: "Previous buffer",
        factory: Some(parse_bprev),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "list",
        aliases: &["ls", "l"],
        description: "List buffers",
        factory: Some(parse_blist),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
];

pub const COMMANDS: &[CommandDescriptor] = &[
    // Core
    CommandDescriptor {
        name: "quit",
        aliases: &["q"],
        description: "Close the current buffer",
        factory: Some(parse_quit),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "write",
        aliases: &["w"],
        description: "Save the current file",
        factory: Some(parse_write),
        subcommands: &[],
        completion: F,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "wq",
        aliases: &[],
        description: "Save the current file and quit",
        factory: Some(parse_write_quit),
        subcommands: &[],
        completion: F,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "edit",
        aliases: &["e"],
        description: "Edit a file",
        factory: Some(parse_edit),
        subcommands: &[],
        completion: F,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "set",
        aliases: &["se"],
        description: "Set an option",
        factory: Some(parse_set),
        subcommands: &[],
        completion: S,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "setlocal",
        aliases: &["setl"],
        description: "Set a local option",
        factory: Some(parse_setlocal),
        subcommands: &[],
        completion: L,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "notify",
        aliases: &[],
        description: "Show a notification",
        factory: Some(parse_notify),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "redraw",
        aliases: &[],
        description: "Redraw the screen",
        factory: Some(parse_redraw),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "reload",
        aliases: &[],
        description: "Reload the plugins",
        factory: Some(parse_reload),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    // Buffer management
    CommandDescriptor {
        name: "buffer",
        aliases: &["b"],
        description: "Buffer management",
        factory: None,
        subcommands: BUFFER_SUBS,
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "bnext",
        aliases: &["bn"],
        description: "Next buffer",
        factory: Some(parse_bnext),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "bprev",
        aliases: &["bp"],
        description: "Previous buffer",
        factory: Some(parse_bprev),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "ls",
        aliases: &[],
        description: "List buffers",
        factory: Some(parse_blist),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    // Search/replace
    CommandDescriptor {
        name: "nohighlight",
        aliases: &["noh"],
        description: "Clear search highlights",
        factory: Some(parse_nohighlight),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "substitute",
        aliases: &["s"],
        description: "Search and replace text",
        factory: Some(parse_substitute),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "substitute_range",
        aliases: &["s%"],
        description: "Search and replace text in whole file",
        factory: Some(parse_substitute_range),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    // Undo/redo
    CommandDescriptor {
        name: "undo",
        aliases: &["u"],
        description: "Undo changes",
        factory: Some(parse_undo),
        subcommands: UNDO_SUBS,
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "redo",
        aliases: &["red"],
        description: "Redo changes",
        factory: Some(parse_redo),
        subcommands: UNDO_SUBS,
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "undotree",
        aliases: &["ut"],
        description: "Open undo tree visualization",
        factory: Some(parse_undotree),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "messages",
        aliases: &["mes"],
        description: "Open messages log buffer",
        factory: Some(parse_messages),
        subcommands: &[],
        completion: N,
        subcommand_prefix: "",
    },
    // File/window management
    CommandDescriptor {
        name: "file",
        aliases: &["f"],
        description: "Open file explorer",
        factory: Some(parse_file),
        subcommands: &[],
        completion: D,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "terminal",
        aliases: &["term"],
        description: "Open terminal buffer",
        factory: Some(parse_terminal),
        subcommands: &[],
        completion: F,
        subcommand_prefix: "",
    },
    CommandDescriptor {
        name: "split",
        aliases: &["sp"],
        description: "Horizontal split",
        factory: Some(parse_split),
        subcommands: SPLIT_SUB_DESC,
        completion: F,
        subcommand_prefix: ":",
    },
    CommandDescriptor {
        name: "vsplit",
        aliases: &["vs"],
        description: "Vertical split",
        factory: Some(parse_vsplit),
        subcommands: VSPLIT_SUB_DESC,
        completion: F,
        subcommand_prefix: ":",
    },
];
