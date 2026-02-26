//! Command line tab completion logic
//! Pure functions — no I/O. Filesystem completion is handled by CompletionJob.

use crate::command_line::commands::definitions::COMMANDS;
use crate::command_line::settings::{SettingType, SettingsRegistry};
use crate::state::UserSettings;

/// What kind of thing is being completed at the cursor
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    /// Completing the top-level command name (or its alias)
    CommandName,
    /// Completing a subcommand of the given parent command name
    Subcommand { parent: String },
    /// Completing a file path (for edit/write/explore/split/vsplit/terminal)
    FilePath { dir: String, prefix: String },
    /// Completing a setting name (for :set / :setlocal)
    SettingName,
    /// Completing the value of a named setting
    SettingValue { name: String },
    /// Nothing to complete
    None,
}

/// A single completion candidate
#[derive(Debug, Clone)]
pub struct CompletionCandidate {
    /// The full replacement text for the completing token
    pub text: String,
    /// Short description shown on the right (command description, type hint, etc.)
    pub description: String,
}

/// Result of a completion query
#[derive(Debug, Clone)]
pub struct CompletionResult {
    /// The longest prefix shared by all candidates (may equal the input token)
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
}

// ─── Context parsing ──────────────────────────────────────────────────────────

/// Determine what should be completed given the current command line content.
///
/// Returns `(context, completing_token)` where `completing_token` is the
/// prefix of the token currently being typed.
pub fn parse_context(
    input: &str,
    settings_registry: &SettingsRegistry<UserSettings>,
) -> (CompletionContext, String) {
    // Split on spaces, keeping track of whether there's a trailing space
    let has_trailing_space = input.ends_with(' ');
    let tokens: Vec<&str> = input.split_whitespace().collect();

    match tokens.as_slice() {
        // Empty input or first token not yet finished → complete command name
        [] => (CompletionContext::CommandName, String::new()),

        [only] if !has_trailing_space => (CompletionContext::CommandName, only.to_string()),

        [cmd, rest @ ..] => {
            let resolved = resolve_command_name(cmd);

            // The token currently being typed (empty string if trailing space)
            let current_token = if has_trailing_space {
                String::new()
            } else {
                rest.last().copied().unwrap_or("").to_string()
            };

            match resolved.as_deref() {
                Some("edit") | Some("write") | Some("wq") | Some("explore") | Some("terminal") => {
                    // The path prefix is the last token (or empty if trailing space)
                    let path_prefix = if has_trailing_space {
                        String::new()
                    } else {
                        current_token.clone()
                    };
                    let (dir, prefix) = split_path_prefix(&path_prefix);
                    (CompletionContext::FilePath { dir, prefix }, String::new())
                }

                Some("set") | Some("setlocal") => {
                    if has_trailing_space && !rest.is_empty() {
                        // "set number " → complete value of "number"
                        let setting_token = rest[0];
                        let canonical = resolve_setting_name(setting_token, settings_registry)
                            .unwrap_or_else(|| setting_token.to_string());
                        (
                            CompletionContext::SettingValue { name: canonical },
                            String::new(),
                        )
                    } else {
                        // "set nu" → complete setting name
                        (CompletionContext::SettingName, current_token)
                    }
                }

                Some("buffer") | Some("undo") | Some("redo") => {
                    let resolved_name = resolved.unwrap();
                    (
                        CompletionContext::Subcommand {
                            parent: resolved_name,
                        },
                        current_token,
                    )
                }

                Some("split") | Some("vsplit") => {
                    if let Some(stripped) = current_token.strip_prefix(':') {
                        let resolved_name = resolved.unwrap();
                        (
                            CompletionContext::Subcommand {
                                parent: resolved_name,
                            },
                            stripped.to_string(),
                        )
                    } else {
                        let path_prefix = if has_trailing_space {
                            String::new()
                        } else {
                            current_token.clone()
                        };
                        let (dir, prefix) = split_path_prefix(&path_prefix);
                        (CompletionContext::FilePath { dir, prefix }, String::new())
                    }
                }

                _ => (CompletionContext::None, String::new()),
            }
        }
    }
}

// ─── In-memory completion sources ─────────────────────────────────────────────

/// Complete a command name from the given prefix.
///
/// Aliases only appear as candidates when the prefix exactly matches an alias;
/// otherwise only canonical command names are shown.
pub fn complete_command_name(prefix: &str) -> CompletionResult {
    let prefix_lower = prefix.to_lowercase();
    let mut candidates: Vec<CompletionCandidate> = Vec::new();

    for cmd in COMMANDS {
        if cmd.name.to_lowercase().starts_with(&prefix_lower) {
            let alias_hint = if cmd.aliases.is_empty() {
                String::new()
            } else {
                format!("[{}]", cmd.aliases.join(", "))
            };
            candidates.push(CompletionCandidate {
                text: cmd.name.to_string(),
                description: format!("{} {}", cmd.description, alias_hint)
                    .trim()
                    .to_string(),
            });
        }
        for alias in cmd.aliases {
            if alias.to_lowercase() == prefix_lower {
                candidates.push(CompletionCandidate {
                    text: alias.to_string(),
                    description: format!("{} (alias for {})", cmd.description, cmd.name),
                });
            }
        }
    }

    candidates.sort_by(|a, b| a.text.cmp(&b.text));
    let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    let common = longest_common_prefix_of(&texts);

    CompletionResult {
        common_prefix: common,
        candidates,
    }
}

/// Complete a subcommand name for a given parent command.
///
/// Aliases only appear as candidates when the prefix exactly matches an alias.
pub fn complete_subcommand(parent: &str, prefix: &str) -> CompletionResult {
    let parent_lower = parent.to_lowercase();
    let prefix_lower = prefix.to_lowercase();

    let parent_cmd = COMMANDS
        .iter()
        .find(|c| c.name.to_lowercase() == parent_lower);
    let subcommands = match parent_cmd {
        Some(cmd) => cmd.subcommands,
        None => return CompletionResult::empty(),
    };

    let mut candidates: Vec<CompletionCandidate> = Vec::new();
    for sub in subcommands {
        if sub.name.to_lowercase().starts_with(&prefix_lower) {
            let alias_hint = if sub.aliases.is_empty() {
                String::new()
            } else {
                format!("[{}]", sub.aliases.join(", "))
            };
            candidates.push(CompletionCandidate {
                text: sub.name.to_string(),
                description: format!("{} {}", sub.description, alias_hint)
                    .trim()
                    .to_string(),
            });
        }
        for alias in sub.aliases {
            if alias.to_lowercase() == prefix_lower {
                candidates.push(CompletionCandidate {
                    text: alias.to_string(),
                    description: format!("{} (alias for {})", sub.description, sub.name),
                });
            }
        }
    }

    candidates.sort_by(|a, b| a.text.cmp(&b.text));
    let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    let common = longest_common_prefix_of(&texts);

    CompletionResult {
        common_prefix: common,
        candidates,
    }
}

/// Complete a setting name for :set / :setlocal.
/// Includes `no<name>` variants for Boolean settings.
pub fn complete_setting_name(
    prefix: &str,
    settings_registry: &SettingsRegistry<UserSettings>,
) -> CompletionResult {
    let prefix_lower = prefix.to_lowercase();
    // Detect if we're completing a "no..." prefix
    let no_inner = if prefix_lower.starts_with("no") && prefix_lower.len() > 2 {
        Some(prefix_lower[2..].to_string())
    } else {
        None
    };

    let mut candidates: Vec<CompletionCandidate> = Vec::new();

    for desc in settings_registry.descriptors() {
        let type_hint = type_hint_for(&desc.ty);

        // Direct name match
        if desc.name.to_lowercase().starts_with(&prefix_lower) {
            candidates.push(CompletionCandidate {
                text: desc.name.to_string(),
                description: format!("{} ({})", desc.description, type_hint),
            });
        }
        for alias in desc.aliases {
            if alias.to_lowercase().starts_with(&prefix_lower) {
                candidates.push(CompletionCandidate {
                    text: alias.to_string(),
                    description: format!(
                        "{} ({}) [alias for {}]",
                        desc.description, type_hint, desc.name
                    ),
                });
            }
        }

        // no<name> variants for Boolean settings only
        if matches!(desc.ty, SettingType::Boolean) {
            let no_name = format!("no{}", desc.name);
            let should_include = no_name.to_lowercase().starts_with(&prefix_lower)
                || no_inner
                    .as_deref()
                    .map(|inner| desc.name.to_lowercase().starts_with(inner))
                    .unwrap_or(false);

            if should_include && !prefix_lower.starts_with(desc.name.to_lowercase().as_str()) {
                candidates.push(CompletionCandidate {
                    text: no_name,
                    description: format!("{} (boolean off)", desc.description),
                });
            }
        }
    }

    candidates.sort_by(|a, b| a.text.cmp(&b.text));
    candidates.dedup_by(|a, b| a.text == b.text);
    let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    let common = longest_common_prefix_of(&texts);

    CompletionResult {
        common_prefix: common,
        candidates,
    }
}

/// Complete the value for a named setting.
pub fn complete_setting_value(
    name: &str,
    settings_registry: &SettingsRegistry<UserSettings>,
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
            },
            CompletionCandidate {
                text: "false".into(),
                description: "disable".into(),
            },
        ],
        SettingType::Enum { variants } => variants
            .iter()
            .map(|v| CompletionCandidate {
                text: v.to_string(),
                description: String::new(),
            })
            .collect(),
        SettingType::Integer { min, max } => {
            let range = match (min, max) {
                (Some(lo), Some(hi)) => format!("<integer {lo}–{hi}>"),
                (Some(lo), None) => format!("<integer ≥{lo}>"),
                (None, Some(hi)) => format!("<integer ≤{hi}>"),
                (None, None) => "<integer>".into(),
            };
            vec![CompletionCandidate {
                text: range,
                description: "type a number".into(),
            }]
        }
        SettingType::Float { min, max } => {
            let range = match (min, max) {
                (Some(lo), Some(hi)) => format!("<float {lo}–{hi}>"),
                (Some(lo), None) => format!("<float ≥{lo}>"),
                (None, Some(hi)) => format!("<float ≤{hi}>"),
                (None, None) => "<float>".into(),
            };
            vec![CompletionCandidate {
                text: range,
                description: "type a decimal".into(),
            }]
        }
        SettingType::Color => vec![CompletionCandidate {
            text: "<color>".into(),
            description: "e.g. red, #ff0000, rgb(255,0,0)".into(),
        }],
    };

    let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    let common = longest_common_prefix_of(&texts);

    CompletionResult {
        common_prefix: common,
        candidates,
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Split a path-like prefix into `(dir, file_prefix)`.
/// `"src/f"` → `("src", "f")`, `"foo"` → `(".", "foo")`, `"src/"` → `("src", "")`.
pub fn split_path_prefix(prefix: &str) -> (String, String) {
    // Handle trailing slash explicitly: "src/" means dir="src", prefix=""
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

/// Compute the longest common prefix of a list of strings.
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

fn resolve_command_name(token: &str) -> Option<String> {
    let token_lower = token.to_lowercase();
    // Exact name or alias match first
    for cmd in COMMANDS {
        if cmd.name.to_lowercase() == token_lower
            || cmd.aliases.iter().any(|a| a.to_lowercase() == token_lower)
        {
            return Some(cmd.name.to_string());
        }
    }
    // Unique prefix match
    let matches: Vec<_> = COMMANDS
        .iter()
        .filter(|c| c.name.to_lowercase().starts_with(&token_lower))
        .collect();
    if matches.len() == 1 {
        Some(matches[0].name.to_string())
    } else {
        None
    }
}

fn resolve_setting_name(
    token: &str,
    settings_registry: &SettingsRegistry<UserSettings>,
) -> Option<String> {
    let token_lower = token.to_lowercase();
    for desc in settings_registry.descriptors() {
        if desc.name.to_lowercase() == token_lower
            || desc.aliases.iter().any(|a| a.to_lowercase() == token_lower)
        {
            return Some(desc.name.to_string());
        }
    }
    None
}

fn type_hint_for(ty: &SettingType) -> String {
    match ty {
        SettingType::Boolean => "boolean".into(),
        SettingType::Integer {
            min: Some(lo),
            max: Some(hi),
        } => format!("integer {lo}–{hi}"),
        SettingType::Integer {
            min: Some(lo),
            max: None,
        } => format!("integer ≥{lo}"),
        SettingType::Integer {
            min: None,
            max: Some(hi),
        } => format!("integer ≤{hi}"),
        SettingType::Integer { .. } => "integer".into(),
        SettingType::Float {
            min: Some(lo),
            max: Some(hi),
        } => format!("float {lo}–{hi}"),
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
        let (ctx, token) = parse_context("q", &reg);
        assert_eq!(ctx, CompletionContext::CommandName);
        assert_eq!(token, "q");
    }

    #[test]
    fn test_parse_context_setting_name() {
        let reg = create_settings_registry();
        let (ctx, token) = parse_context("set nu", &reg);
        assert_eq!(ctx, CompletionContext::SettingName);
        assert_eq!(token, "nu");
    }

    #[test]
    fn test_parse_context_subcommand() {
        let reg = create_settings_registry();
        let (ctx, token) = parse_context("buffer n", &reg);
        assert_eq!(
            ctx,
            CompletionContext::Subcommand {
                parent: "buffer".into()
            }
        );
        assert_eq!(token, "n");
    }

    #[test]
    fn test_parse_context_filepath() {
        let reg = create_settings_registry();
        let (ctx, _) = parse_context("edit src/f", &reg);
        assert!(matches!(ctx, CompletionContext::FilePath { .. }));
    }

    #[test]
    fn test_setting_value_boolean() {
        let reg = create_settings_registry();
        // Find a boolean setting name
        let bool_setting = reg
            .descriptors()
            .iter()
            .find(|d| matches!(d.ty, SettingType::Boolean))
            .expect("at least one boolean setting");
        let result = complete_setting_value(bool_setting.name, &reg);
        assert_eq!(result.candidates.len(), 2);
        assert!(result.candidates.iter().any(|c| c.text == "true"));
        assert!(result.candidates.iter().any(|c| c.text == "false"));
    }

    #[test]
    fn test_setting_name_no_prefix() {
        let reg = create_settings_registry();
        // Find a boolean setting and confirm "no<name>" appears
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
        let (ctx, token) = parse_context("split :l", &reg);
        assert_eq!(
            ctx,
            CompletionContext::Subcommand {
                parent: "split".into()
            }
        );
        assert_eq!(token, "l");
    }

    #[test]
    fn test_parse_context_vsplit_colon_subcommand() {
        let reg = create_settings_registry();
        let (ctx, token) = parse_context("vsplit :fr", &reg);
        assert_eq!(
            ctx,
            CompletionContext::Subcommand {
                parent: "vsplit".into()
            }
        );
        assert_eq!(token, "fr");
    }

    #[test]
    fn test_parse_context_split_filepath() {
        let reg = create_settings_registry();
        let (ctx, _) = parse_context("split src/f", &reg);
        assert!(matches!(ctx, CompletionContext::FilePath { .. }));
    }

    #[test]
    fn test_parse_context_split_empty_is_filepath() {
        let reg = create_settings_registry();
        let (ctx, _) = parse_context("split ", &reg);
        assert!(matches!(ctx, CompletionContext::FilePath { .. }));
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
}
