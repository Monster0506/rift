//! Command line tab completion logic
//! Pure functions — no I/O. Filesystem completion is handled by CompletionJob.
//!
//! Command resolution is designed so any correctly registered command works:
//! the leading `:` is stripped from tokens, and `resolve_command_descriptor`
//! matches by exact name/alias, then single name prefix, then single alias prefix.

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

// ─── Context parsing ──────────────────────────────────────────────────────────

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
                // Token doesn't start with prefix — fall through to completion hint
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

// ─── Completion sources ───────────────────────────────────────────────────────

/// Shared name+alias completion: same prefix and same-first-letter alias rule as
/// command and setting parsing. Used by both complete_from_descriptors and
/// complete_setting_name so parsing and completion stay identical.
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

// ─── Completion action resolution ─────────────────────────────────────────────

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

// ─── Helpers ──────────────────────────────────────────────────────────────────

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
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_line::settings::create_settings_registry;
    use crate::document::definitions::{create_document_settings_registry, DocumentOptions};
    use crate::state::UserSettings;

    #[test]
    fn test_complete_command_name_prefix() {
        let result = complete_command_name("q");
        assert!(result.candidates.iter().any(|c| c.text == "quit"));
    }

    #[test]
    fn test_complete_command_name_empty_returns_all() {
        let result = complete_command_name("");
        assert!(!result.candidates.is_empty());
    }

    #[test]
    fn test_complete_command_name_alias() {
        let result = complete_command_name("w");
        assert!(result
            .candidates
            .iter()
            .any(|c| c.text == "write" || c.text == "w"));
    }

    #[test]
    fn test_complete_command_name_f_prefix() {
        let result = complete_command_name("f");
        assert!(result.candidates.iter().any(|c| c.text == "file"));
    }

    #[test]
    fn test_parse_context_colon_prefix_stripped() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context(":f", &reg, &doc_reg);
        assert_eq!(pc.context, CompletionContext::CommandName);
        assert_eq!(pc.prefix, "f");
    }

    #[test]
    fn test_parse_context_f_space_directories_only() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context(":f ", &reg, &doc_reg);
        assert!(matches!(
            pc.context,
            CompletionContext::FilePath {
                filter: PathFilter::DirectoriesOnly,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_context_e_space_shows_both() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context(":e ", &reg, &doc_reg);
        assert!(matches!(
            pc.context,
            CompletionContext::FilePath {
                filter: PathFilter::Both,
                ..
            }
        ));
    }

    #[test]
    fn test_complete_subcommand() {
        let result = complete_subcommand("buffer", "n");
        assert!(result.candidates.iter().any(|c| c.text == "next"));
    }

    #[test]
    fn test_complete_subcommand_empty() {
        let result = complete_subcommand("buffer", "");
        assert!(!result.candidates.is_empty());
    }

    #[test]
    fn test_complete_subcommand_unknown_parent() {
        let result = complete_subcommand("nonexistent", "");
        assert!(result.candidates.is_empty());
    }

    #[test]
    fn test_longest_common_prefix_multiple() {
        assert_eq!(longest_common_prefix_of(&["write", "wq"]), "w");
    }

    #[test]
    fn test_longest_common_prefix_single() {
        assert_eq!(longest_common_prefix_of(&["quit"]), "quit");
    }

    #[test]
    fn test_longest_common_prefix_empty() {
        assert_eq!(longest_common_prefix_of(&[]), "");
    }

    #[test]
    fn test_split_path_prefix_with_dir() {
        let (dir, prefix) = split_path_prefix("src/f");
        assert_eq!(dir, "src");
        assert_eq!(prefix, "f");
    }

    #[test]
    fn test_split_path_prefix_root_only() {
        let (dir, prefix) = split_path_prefix("foo");
        assert_eq!(dir, ".");
        assert_eq!(prefix, "foo");
    }

    #[test]
    fn test_split_path_prefix_trailing_slash() {
        let (dir, prefix) = split_path_prefix("src/");
        assert_eq!(dir, "src");
        assert_eq!(prefix, "");
    }

    #[test]
    fn test_parse_context_command_name() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("q", &reg, &doc_reg);
        assert_eq!(pc.context, CompletionContext::CommandName);
        assert_eq!(pc.prefix, "q");
        assert_eq!(pc.token_start, 0);
    }

    #[test]
    fn test_parse_context_setting_name() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("set nu", &reg, &doc_reg);
        assert_eq!(pc.context, CompletionContext::SettingName);
        assert_eq!(pc.prefix, "nu");
    }

    #[test]
    fn test_parse_context_subcommand() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("buffer n", &reg, &doc_reg);
        assert_eq!(
            pc.context,
            CompletionContext::Subcommand {
                parent: "buffer".into(),
                subcommand_prefix: String::new(),
            }
        );
        assert_eq!(pc.prefix, "n");
    }

    #[test]
    fn test_parse_context_filepath() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("edit src/f", &reg, &doc_reg);
        assert!(matches!(pc.context, CompletionContext::FilePath { .. }));
    }

    #[test]
    fn test_setting_value_boolean() {
        let reg = create_settings_registry();
        let bool_setting = reg
            .descriptors()
            .iter()
            .find(|d| matches!(d.ty, SettingType::Boolean))
            .expect("at least one boolean setting");
        let result = complete_setting_value::<UserSettings>(bool_setting.name, &reg, None);
        assert_eq!(result.candidates.len(), 2);
        assert!(result.candidates.iter().any(|c| c.text == "true"));
        assert!(result.candidates.iter().any(|c| c.text == "false"));
    }

    #[test]
    fn test_setting_name_no_prefix() {
        let reg = create_settings_registry();
        let bool_setting = reg
            .descriptors()
            .iter()
            .find(|d| matches!(d.ty, SettingType::Boolean))
            .expect("at least one boolean setting");
        let result = complete_setting_name(&format!("no{}", bool_setting.name), &reg);
        let no_name = format!("no{}", bool_setting.name);
        assert!(
            result.candidates.iter().any(|c| c.text == no_name),
            "expected candidate {no_name}"
        );
    }

    #[test]
    fn test_parse_context_split_colon_subcommand() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("split :l", &reg, &doc_reg);
        assert_eq!(
            pc.context,
            CompletionContext::Subcommand {
                parent: "split".into(),
                subcommand_prefix: ":".into(),
            }
        );
        assert_eq!(pc.prefix, "l");
    }

    #[test]
    fn test_parse_context_vsplit_colon_subcommand() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("vsplit :fr", &reg, &doc_reg);
        assert_eq!(
            pc.context,
            CompletionContext::Subcommand {
                parent: "vsplit".into(),
                subcommand_prefix: ":".into(),
            }
        );
        assert_eq!(pc.prefix, "fr");
    }

    #[test]
    fn test_parse_context_split_filepath() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("split src/f", &reg, &doc_reg);
        assert!(matches!(pc.context, CompletionContext::FilePath { .. }));
    }

    #[test]
    fn test_parse_context_split_empty_is_filepath() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("split ", &reg, &doc_reg);
        assert!(matches!(pc.context, CompletionContext::FilePath { .. }));
    }

    #[test]
    fn test_complete_split_subcommands() {
        let result = complete_subcommand("split", "");
        assert!(result.candidates.len() >= 7);
        let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(names.contains(&"left"));
        assert!(names.contains(&"right"));
        assert!(names.contains(&"freeze"));
        assert!(names.contains(&"resize"));
    }

    #[test]
    fn test_complete_subcommand_no_same_first_letter_alias() {
        let result = complete_subcommand("split", "");
        let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(
            !names.contains(&"l"),
            "alias 'l' same first letter as 'left'"
        );
        assert!(names.contains(&"left"));
    }

    #[test]
    fn test_complete_buffer_no_ls_alias() {
        let result = complete_subcommand("buffer", "");
        let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(names.contains(&"list"));
        assert!(
            !names.contains(&"ls"),
            "alias 'ls' same first letter as 'list'"
        );
    }

    #[test]
    fn test_complete_split_subcommand_prefix() {
        let result = complete_subcommand("split", "f");
        assert!(result.candidates.iter().any(|c| c.text == "freeze"));
        assert!(!result.candidates.iter().any(|c| c.text == "left"));
    }

    #[test]
    fn test_complete_vsplit_subcommands() {
        let result = complete_subcommand("vsplit", "");
        assert!(result.candidates.len() >= 7);
        let names: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(names.contains(&"down"));
        assert!(names.contains(&"nofreeze"));
    }

    #[test]
    fn test_parse_context_token_start() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("edit foo.txt", &reg, &doc_reg);
        assert_eq!(pc.token_start, 5);

        let pc = parse_context("set ", &reg, &doc_reg);
        assert_eq!(pc.token_start, 4);

        let pc = parse_context("quit", &reg, &doc_reg);
        assert_eq!(pc.token_start, 0);
    }

    #[test]
    fn test_resolve_completion_stale() {
        let result = CompletionResult {
            common_prefix: "quit".into(),
            candidates: vec![CompletionCandidate {
                text: "quit".into(),
                description: String::new(),
                is_directory: false,
            }],
        };
        let action = resolve_completion(result, "qu", 0, "qui", false);
        assert!(matches!(action, CompletionAction::Discard));
    }

    #[test]
    fn test_resolve_completion_single() {
        let result = CompletionResult {
            common_prefix: "quit".into(),
            candidates: vec![CompletionCandidate {
                text: "quit".into(),
                description: String::new(),
                is_directory: false,
            }],
        };
        let action = resolve_completion(result, "qu", 0, "qu", false);
        assert!(matches!(action, CompletionAction::ApplyAndClear { .. }));
    }

    #[test]
    fn test_resolve_completion_dropdown() {
        let result = CompletionResult {
            common_prefix: "w".into(),
            candidates: vec![
                CompletionCandidate {
                    text: "write".into(),
                    description: String::new(),
                    is_directory: false,
                },
                CompletionCandidate {
                    text: "wq".into(),
                    description: String::new(),
                    is_directory: false,
                },
            ],
        };
        let action = resolve_completion(result, "w", 0, "w", false);
        assert!(matches!(action, CompletionAction::ShowDropdown { .. }));
    }

    #[test]
    fn test_from_candidates_sorts_dirs_first() {
        let candidates = vec![
            CompletionCandidate {
                text: "file.rs".into(),
                description: String::new(),
                is_directory: false,
            },
            CompletionCandidate {
                text: "src/".into(),
                description: "directory".into(),
                is_directory: true,
            },
        ];
        let result = CompletionResult::from_candidates(candidates);
        assert_eq!(result.candidates[0].text, "src/");
        assert_eq!(result.candidates[1].text, "file.rs");
    }

    // ── setlocal parse_context tests (parallel to set) ───────────────────────

    #[test]
    fn test_parse_context_local_setting_name_prefix() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("setlocal et", &reg, &doc_reg);
        assert_eq!(pc.context, CompletionContext::LocalSettingName);
        assert_eq!(pc.prefix, "et");
    }

    #[test]
    fn test_parse_context_local_setting_name_empty() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("setlocal ", &reg, &doc_reg);
        assert_eq!(pc.context, CompletionContext::LocalSettingName);
        assert_eq!(pc.prefix, "");
    }

    #[test]
    fn test_parse_context_local_setting_value_exact_name() {
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("setlocal expandtabs ", &reg, &doc_reg);
        assert_eq!(
            pc.context,
            CompletionContext::LocalSettingValue {
                name: "expandtabs".into()
            }
        );
    }

    #[test]
    fn test_parse_context_local_setting_value_alias_resolved() {
        // "et" is an alias for "expandtabs" — must resolve to canonical name
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("setlocal et ", &reg, &doc_reg);
        assert_eq!(
            pc.context,
            CompletionContext::LocalSettingValue {
                name: "expandtabs".into()
            }
        );
    }

    #[test]
    fn test_parse_context_setl_alias_resolves_to_local_setting_name() {
        // "setl" is an alias for "setlocal"
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("setl ", &reg, &doc_reg);
        assert_eq!(pc.context, CompletionContext::LocalSettingName);
        assert_eq!(pc.prefix, "");
    }

    #[test]
    fn test_parse_context_set_setting_value_alias_resolved() {
        // "clborderstyle" is an alias for "command_line.borderstyle"
        let reg = create_settings_registry();
        let doc_reg = create_document_settings_registry();
        let pc = parse_context("set clborderstyle ", &reg, &doc_reg);
        assert_eq!(
            pc.context,
            CompletionContext::SettingValue {
                name: "command_line.borderstyle".into()
            }
        );
    }

    // ── Separation: global vs local setting name completions ─────────────────

    #[test]
    fn test_complete_local_setting_name_returns_doc_settings() {
        let doc_reg = create_document_settings_registry();
        let result = complete_setting_name("", &doc_reg);
        let texts: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"expandtabs"), "expandtabs must appear");
        assert!(texts.contains(&"tabwidth"), "tabwidth must appear");
        assert!(texts.contains(&"line_ending"), "line_ending must appear");
    }

    #[test]
    fn test_complete_local_setting_name_does_not_show_global_settings() {
        let doc_reg = create_document_settings_registry();
        let result = complete_setting_name("", &doc_reg);
        let texts: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(!texts.contains(&"number"), "global 'number' must not appear");
        assert!(
            !texts.contains(&"appearance.background"),
            "global color setting must not appear"
        );
    }

    #[test]
    fn test_complete_global_setting_name_does_not_show_local_settings() {
        let reg = create_settings_registry();
        let result = complete_setting_name("", &reg);
        let texts: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(!texts.contains(&"expandtabs"), "local 'expandtabs' must not appear");
        assert!(!texts.contains(&"tabwidth"), "local 'tabwidth' must not appear");
    }

    // ── complete_setting_value with current value ─────────────────────────────

    #[test]
    fn test_complete_setting_value_integer_shows_current_value() {
        let doc_reg = create_document_settings_registry();
        let opts = DocumentOptions::default(); // tab_width = 4
        let result = complete_setting_value("tabwidth", &doc_reg, Some(&opts));
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].text, "4");
    }

    #[test]
    fn test_complete_setting_value_float_shows_current_value() {
        let reg = create_settings_registry();
        let settings = UserSettings::new();
        // command_line.width_ratio defaults to 0.6
        let result = complete_setting_value("command_line.width_ratio", &reg, Some(&settings));
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].text, "0.6");
    }

    #[test]
    fn test_complete_setting_value_integer_shows_current_value_global() {
        let reg = create_settings_registry();
        let settings = UserSettings::new();
        // command_line.height defaults to 3
        let result = complete_setting_value("command_line.height", &reg, Some(&settings));
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].text, "3");
    }

    #[test]
    fn test_complete_setting_value_boolean_still_shows_both_options() {
        let doc_reg = create_document_settings_registry();
        let opts = DocumentOptions::default();
        let result = complete_setting_value("expandtabs", &doc_reg, Some(&opts));
        assert_eq!(result.candidates.len(), 2);
        assert!(result.candidates.iter().any(|c| c.text == "true"));
        assert!(result.candidates.iter().any(|c| c.text == "false"));
    }

    #[test]
    fn test_complete_setting_value_enum_still_shows_all_variants() {
        let doc_reg = create_document_settings_registry();
        let opts = DocumentOptions::default();
        let result = complete_setting_value("line_ending", &doc_reg, Some(&opts));
        assert!(result.candidates.iter().any(|c| c.text == "lf"));
        assert!(result.candidates.iter().any(|c| c.text == "crlf"));
    }

    #[test]
    fn test_complete_setting_value_integer_no_current_is_empty() {
        let doc_reg = create_document_settings_registry();
        let result = complete_setting_value::<DocumentOptions>("tabwidth", &doc_reg, None);
        assert!(result.candidates.is_empty());
    }

    #[test]
    fn test_complete_setting_value_color_no_current_is_empty() {
        let reg = create_settings_registry();
        let result = complete_setting_value::<UserSettings>("appearance.background", &reg, None);
        assert!(result.candidates.is_empty());
    }

    #[test]
    fn test_complete_setting_value_color_shows_current_value() {
        let reg = create_settings_registry();
        let mut settings = UserSettings::new();
        settings.editor_bg = None;
        let result = complete_setting_value("appearance.background", &reg, Some(&settings));
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].text, "none");
    }
}
