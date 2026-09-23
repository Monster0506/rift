//! Command line tab completion logic (pure functions; filesystem completion is handled by CompletionJob).
//! `resolve_command_descriptor` strips the leading `:` and matches by exact name/alias, then name prefix, then alias prefix.

use crate::command_line::commands::definitions::{CompletionHint, COMMANDS};
use crate::command_line::commands::{CommandDescriptor, MatchResult};
use crate::command_line::settings::{SettingType, SettingsRegistry};
use crate::document::definitions::DocumentOptions;
use crate::state::UserSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFilter {
    Both,
    FilesOnly,
    DirectoriesOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    CommandName,
    Subcommand {
        parent: String,
        subcommand_prefix: String,
    },
    FilePath {
        dir: String,
        prefix: String,
        filter: PathFilter,
    },
    SettingName,
    SettingValue {
        name: String,
    },
    LocalSettingName,
    LocalSettingValue {
        name: String,
    },
    None,
}

#[derive(Debug, Clone)]
pub struct CompletionCandidate {
    pub text: String,
    pub description: String,
    pub is_directory: bool,
}

#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub common_prefix: String,
    pub candidates: Vec<CompletionCandidate>,
}

impl CompletionResult {
    pub fn empty() -> Self {
        Self {
            common_prefix: String::new(),
            candidates: Vec::new(),
        }
    }

    pub fn from_candidates(mut candidates: Vec<CompletionCandidate>) -> Self {
        candidates.sort_by(|a, b| {
            a.is_directory
                .cmp(&b.is_directory)
                .reverse()
                .then(a.text.cmp(&b.text))
        });
        let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        let common = longest_common_prefix_of(&texts);
        Self {
            common_prefix: common,
            candidates,
        }
    }
}

/// Result of parsing the command line to determine completion context.
#[derive(Debug, Clone)]
pub struct ParsedContext {
    pub context: CompletionContext,
    pub prefix: String,
    pub token_start: usize,
}

pub fn parse_context(
    input: &str,
    settings_registry: &SettingsRegistry<UserSettings>,
    document_settings_registry: &SettingsRegistry<DocumentOptions>,
) -> ParsedContext {
    let has_trailing_space = input.ends_with(' ');
    let tokens: Vec<&str> = input.split_whitespace().collect();

    let token_start = if has_trailing_space {
        input.len()
    } else if let Some(pos) = input.rfind(char::is_whitespace) {
        pos + 1
    } else {
        0
    };

    match tokens.as_slice() {
        [] => ParsedContext {
            context: CompletionContext::CommandName,
            prefix: String::new(),
            token_start: 0,
        },

        [only] if !has_trailing_space => {
            let prefix = only.strip_prefix(':').unwrap_or(only).to_string();
            ParsedContext {
                context: CompletionContext::CommandName,
                prefix,
                token_start: 0,
            }
        }

        [cmd, rest @ ..] => {
            let current_token = if has_trailing_space {
                String::new()
            } else {
                rest.last().copied().unwrap_or("").to_string()
            };

            let cmd_stripped = cmd.strip_prefix(':').unwrap_or(cmd);
            let desc = match resolve_command_descriptor(cmd_stripped) {
                Some(d) => d,
                None => {
                    return ParsedContext {
                        context: CompletionContext::None,
                        prefix: String::new(),
                        token_start,
                    }
                }
            };

            // Commands with subcommands: check if current token matches the subcommand prefix
            if !desc.subcommands.is_empty() {
                let pfx = desc.subcommand_prefix;
                if pfx.is_empty() {
                    return ParsedContext {
                        context: CompletionContext::Subcommand {
                            parent: desc.name.to_string(),
                            subcommand_prefix: String::new(),
                        },
                        prefix: current_token,
                        token_start,
                    };
                }
                if let Some(stripped) = current_token.strip_prefix(pfx) {
                    return ParsedContext {
                        context: CompletionContext::Subcommand {
                            parent: desc.name.to_string(),
                            subcommand_prefix: pfx.to_string(),
                        },
                        prefix: stripped.to_string(),
                        token_start,
                    };
                }
            }

            match desc.completion {
                CompletionHint::FilePath => {
                    let path_prefix = if has_trailing_space {
                        String::new()
                    } else {
                        current_token
                    };
                    let (dir, prefix) = split_path_prefix(&path_prefix);
                    ParsedContext {
                        context: CompletionContext::FilePath {
                            dir,
                            prefix,
                            filter: PathFilter::Both,
                        },
                        prefix: String::new(),
                        token_start,
                    }
                }
                CompletionHint::Directory => {
                    let path_prefix = if has_trailing_space {
                        String::new()
                    } else {
                        current_token
                    };
                    let (dir, prefix) = split_path_prefix(&path_prefix);
                    ParsedContext {
                        context: CompletionContext::FilePath {
                            dir,
                            prefix,
                            filter: PathFilter::DirectoriesOnly,
                        },
                        prefix: String::new(),
                        token_start,
                    }
                }
                CompletionHint::Setting => {
                    if has_trailing_space && !rest.is_empty() {
                        let setting_token = rest[0];
                        let canonical = resolve_setting_name(setting_token, settings_registry)
                            .unwrap_or_else(|| setting_token.to_string());
                        ParsedContext {
                            context: CompletionContext::SettingValue { name: canonical },
                            prefix: String::new(),
                            token_start,
                        }
                    } else {
                        ParsedContext {
                            context: CompletionContext::SettingName,
                            prefix: current_token,
                            token_start,
                        }
                    }
                }
                CompletionHint::LocalSetting => {
                    if has_trailing_space && !rest.is_empty() {
                        let setting_token = rest[0];
                        let canonical =
                            resolve_setting_name(setting_token, document_settings_registry)
                                .unwrap_or_else(|| setting_token.to_string());
                        ParsedContext {
                            context: CompletionContext::LocalSettingValue { name: canonical },
                            prefix: String::new(),
                            token_start,
                        }
                    } else {
                        ParsedContext {
                            context: CompletionContext::LocalSettingName,
                            prefix: current_token,
                            token_start,
                        }
                    }
                }
                CompletionHint::None => ParsedContext {
                    context: CompletionContext::None,
                    prefix: String::new(),
                    token_start,
                },
            }
        }
    }
}

/// Shared name+alias completion (same prefix/alias rule as command and setting parsing), used
/// by both complete_from_descriptors and complete_setting_name so parsing and completion stay identical.
fn candidates_from_name_aliases(
    name: &str,
    aliases: &[&str],
    prefix_lower: &str,
    desc_name: &str,
    desc_alias: impl Fn(&str) -> String,
) -> Vec<CompletionCandidate> {
    let mut out = Vec::new();
    let name_lower = name.to_lowercase();
    if name_lower.starts_with(prefix_lower) {
        out.push(CompletionCandidate {
            text: name.to_string(),
            description: desc_name.to_string(),
            is_directory: false,
        });
    }
    for alias in aliases {
        let alias_lower = alias.to_lowercase();
        if alias_lower.chars().next() == name_lower.chars().next() {
            continue;
        }
        if alias_lower == prefix_lower || alias_lower.starts_with(prefix_lower) {
            out.push(CompletionCandidate {
                text: (*alias).to_string(),
                description: desc_alias(alias),
                is_directory: false,
            });
        }
    }
    out
}

fn complete_from_descriptors(descriptors: &[CommandDescriptor], prefix: &str) -> CompletionResult {
    let prefix_lower = prefix.to_lowercase();
    let mut candidates: Vec<CompletionCandidate> = Vec::new();

    for desc in descriptors {
        let alias_hint = if desc.aliases.is_empty() {
            String::new()
        } else {
            format!("[{}]", desc.aliases.join(", "))
        };
        let desc_name = format!("{} {}", desc.description, alias_hint)
            .trim()
            .to_string();
        candidates.extend(candidates_from_name_aliases(
            desc.name,
            desc.aliases,
            &prefix_lower,
            &desc_name,
            |_alias| format!("{} (alias for {})", desc.description, desc.name),
        ));
    }

    CompletionResult::from_candidates(candidates)
}

pub fn complete_command_name(prefix: &str) -> CompletionResult {
    complete_from_descriptors(COMMANDS, prefix)
}

pub fn complete_subcommand(parent: &str, prefix: &str) -> CompletionResult {
    let parent_lower = parent.to_lowercase();
    let parent_cmd = COMMANDS
        .iter()
        .find(|c| c.name.to_lowercase() == parent_lower);
    match parent_cmd {
        Some(cmd) => complete_from_descriptors(cmd.subcommands, prefix),
        None => CompletionResult::empty(),
    }
}

pub fn complete_setting_name<T: 'static>(
    prefix: &str,
    settings_registry: &SettingsRegistry<T>,
) -> CompletionResult {
    let prefix_lower = prefix.to_lowercase();
    let no_inner = if prefix_lower.starts_with("no") && prefix_lower.len() > 2 {
        Some(prefix_lower[2..].to_string())
    } else {
        None
    };

    let mut candidates: Vec<CompletionCandidate> = Vec::new();

    for desc in settings_registry.descriptors() {
        let type_hint = type_hint_for(&desc.ty);
        let desc_name = format!("{} ({})", desc.description, type_hint);
        let desc_alias = |_alias: &str| {
            format!(
                "{} ({}) [alias for {}]",
                desc.description, type_hint, desc.name
            )
        };

        candidates.extend(candidates_from_name_aliases(
            desc.name,
            desc.aliases,
            &prefix_lower,
            &desc_name,
            desc_alias,
        ));

        if matches!(desc.ty, SettingType::Boolean) {
            let no_inner_str = no_inner.as_deref().unwrap_or("");
            let no_candidates = candidates_from_name_aliases(
                desc.name,
                desc.aliases,
                no_inner_str,
                &format!("{} (boolean off)", desc.description),
                |_| format!("{} (boolean off)", desc.description),
            );
            let name_lower = desc.name.to_lowercase();
            for c in no_candidates {
                let no_text = format!("no{}", c.text);
                if no_text.to_lowercase().starts_with(&prefix_lower)
                    && !prefix_lower.starts_with(name_lower.as_str())
                {
                    candidates.push(CompletionCandidate {
                        text: no_text,
                        description: format!("{} (boolean off)", desc.description),
                        is_directory: false,
                    });
                }
            }
        }
    }

    CompletionResult::from_candidates(candidates)
}

pub fn complete_setting_value<T: 'static>(
    name: &str,
    settings_registry: &SettingsRegistry<T>,
    current: Option<&T>,
) -> CompletionResult {
    let desc = settings_registry
        .descriptors()
        .iter()
        .find(|d| d.name == name);
    let desc = match desc {
        Some(d) => d,
        None => return CompletionResult::empty(),
    };

    let candidates: Vec<CompletionCandidate> = match &desc.ty {
        SettingType::Boolean => vec![
            CompletionCandidate {
                text: "true".into(),
                description: "enable".into(),
                is_directory: false,
            },
            CompletionCandidate {
                text: "false".into(),
                description: "disable".into(),
                is_directory: false,
            },
        ],
        SettingType::Enum { variants } => variants
            .iter()
            .map(|v| CompletionCandidate {
                text: v.to_string(),
                description: String::new(),
                is_directory: false,
            })
            .collect(),
        SettingType::IntegerOrKeyword { keywords, .. } => {
            let mut candidates: Vec<CompletionCandidate> = keywords
                .iter()
                .map(|k| CompletionCandidate {
                    text: k.to_string(),
                    description: String::new(),
                    is_directory: false,
                })
                .collect();
            if let (Some(getter), Some(val)) = (desc.get, current) {
                candidates.push(CompletionCandidate {
                    text: getter(val),
                    description: "current value".into(),
                    is_directory: false,
                });
            }
            candidates
        }
        SettingType::Integer { .. } | SettingType::Float { .. } | SettingType::Color => {
            match (desc.get, current) {
                (Some(getter), Some(val)) => vec![CompletionCandidate {
                    text: getter(val),
                    description: "current value".into(),
                    is_directory: false,
                }],
                _ => vec![],
            }
        }
    };

    CompletionResult::from_candidates(candidates)
}

/// Pure description of what the editor should do with a completion result.
#[derive(Debug)]
pub enum CompletionAction {
    Discard,
    Clear,
    ApplyAndClear {
        text: String,
        token_start: usize,
    },
    UpdateDropdown {
        candidates: Vec<CompletionCandidate>,
    },
    ExpandPrefix {
        text: String,
        token_start: usize,
        candidates: Vec<CompletionCandidate>,
    },
    ShowDropdown {
        candidates: Vec<CompletionCandidate>,
    },
}

pub fn resolve_completion(
    result: CompletionResult,
    payload_input: &str,
    token_start: usize,
    command_line: &str,
    was_dropdown_open: bool,
) -> CompletionAction {
    if payload_input != command_line {
        return CompletionAction::Discard;
    }

    let candidates = result.candidates;

    if candidates.is_empty() {
        return CompletionAction::Clear;
    }

    if was_dropdown_open {
        return CompletionAction::UpdateDropdown { candidates };
    }

    if candidates.len() == 1 {
        return CompletionAction::ApplyAndClear {
            text: candidates[0].text.clone(),
            token_start,
        };
    }

    let current_token_len = command_line.len() - token_start;
    if result.common_prefix.len() > current_token_len {
        CompletionAction::ExpandPrefix {
            text: result.common_prefix,
            token_start,
            candidates,
        }
    } else {
        CompletionAction::ShowDropdown { candidates }
    }
}

pub fn split_path_prefix(prefix: &str) -> (String, String) {
    if prefix.ends_with('/') || prefix.ends_with('\\') {
        let dir = prefix.trim_end_matches(['/', '\\']);
        return (
            if dir.is_empty() {
                ".".to_string()
            } else {
                dir.to_string()
            },
            String::new(),
        );
    }
    let path = std::path::Path::new(prefix);
    match path.parent() {
        Some(parent) if parent != std::path::Path::new("") => (
            parent.to_string_lossy().to_string(),
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        ),
        _ => (".".to_string(), prefix.to_string()),
    }
}

pub fn longest_common_prefix_of(strs: &[&str]) -> String {
    if strs.is_empty() {
        return String::new();
    }
    let first = strs[0];
    let mut len = first.len();
    for s in &strs[1..] {
        len = first
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a == b)
            .count()
            .min(len);
    }
    first[..len].to_string()
}

fn resolve_command_descriptor(token: &str) -> Option<&'static CommandDescriptor> {
    let token_lower = token.to_lowercase();
    for cmd in COMMANDS {
        if cmd.name.to_lowercase() == token_lower
            || cmd.aliases.iter().any(|a| a.to_lowercase() == token_lower)
        {
            return Some(cmd);
        }
    }
    let name_matches: Vec<_> = COMMANDS
        .iter()
        .filter(|c| c.name.to_lowercase().starts_with(&token_lower))
        .collect();
    if name_matches.len() == 1 {
        return Some(name_matches[0]);
    }
    let alias_matches: Vec<_> = COMMANDS
        .iter()
        .filter(|c| {
            c.aliases
                .iter()
                .any(|a| a.to_lowercase().starts_with(&token_lower))
        })
        .collect();
    if alias_matches.len() == 1 {
        Some(alias_matches[0])
    } else {
        None
    }
}

/// Resolve a setting token to its canonical name using the same registry and
/// matching order as command parsing (exact name/alias, then single prefix match).
fn resolve_setting_name<T: 'static>(
    token: &str,
    settings_registry: &SettingsRegistry<T>,
) -> Option<String> {
    let registry = settings_registry.build_option_registry();
    match registry.match_command(token) {
        MatchResult::Exact(name) | MatchResult::Prefix(name) => Some(name),
        MatchResult::Ambiguous { .. } | MatchResult::Unknown(_) => None,
    }
}

fn type_hint_for(ty: &SettingType) -> String {
    match ty {
        SettingType::Boolean => "boolean".into(),
        SettingType::Integer {
            min: Some(lo),
            max: Some(hi),
        } => format!("integer {lo}\u{2013}{hi}"),
        SettingType::Integer {
            min: Some(lo),
            max: None,
        } => format!("integer \u{2265}{lo}"),
        SettingType::Integer {
            min: None,
            max: Some(hi),
        } => format!("integer \u{2264}{hi}"),
        SettingType::Integer { .. } => "integer".into(),
        SettingType::Float {
            min: Some(lo),
            max: Some(hi),
        } => format!("float {lo}\u{2013}{hi}"),
        SettingType::Float { .. } => "float".into(),
        SettingType::Enum { variants } => variants.join("|"),
        SettingType::Color => "color".into(),
        SettingType::IntegerOrKeyword { keywords, .. } => format!("integer|{}", keywords.join("|")),
    }
}

#[cfg(test)]
#[path = "completion_tests.rs"]
mod completion_tests;
