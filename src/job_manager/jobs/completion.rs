//! Background job for computing tab completions.
//! Handles filesystem enumeration; in-memory lookups run inline.

use crate::command_line::commands::completion::{
    complete_command_name, complete_setting_name, complete_setting_value, complete_subcommand,
    parse_context, CompletionCandidate, CompletionContext, CompletionResult, PathFilter,
};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use crate::document::definitions::{create_document_settings_registry, DocumentOptions};
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
use crate::state::UserSettings;
use std::any::Any;
use std::sync::{mpsc::Sender, LazyLock};

static SETTINGS_REGISTRY: LazyLock<SettingsRegistry<UserSettings>> =
    LazyLock::new(create_settings_registry);

static DOCUMENT_SETTINGS_REGISTRY: LazyLock<SettingsRegistry<DocumentOptions>> =
    LazyLock::new(create_document_settings_registry);

#[derive(Debug, Clone)]
pub struct CompletionPayload {
    pub result: CompletionResult,
    pub input: String,
    pub token_start: usize,
}

impl JobPayload for CompletionPayload {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[derive(Debug)]
pub struct CompletionJob {
    pub input: String,
    pub current_settings: Option<UserSettings>,
    pub current_doc_options: Option<DocumentOptions>,
    /// Plugin commands: (name, description, arg_type).
    pub plugin_commands: Vec<(String, String, Option<String>)>,
    /// Total number of lines in the active buffer (for `int` / `range` completion).
    pub line_count: usize,
    /// Unique words extracted from the active buffer (for `word` / `string` completion).
    pub buf_words: Vec<String>,
}

impl Job for CompletionJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let parsed = parse_context(&self.input, &SETTINGS_REGISTRY, &DOCUMENT_SETTINGS_REGISTRY);

        let result = match &parsed.context {
            CompletionContext::CommandName => {
                let mut result = complete_command_name(&parsed.prefix);
                let prefix_lower = parsed.prefix.to_lowercase();
                // Plugin commands come after built-ins, sorted among themselves.
                let mut plugin_candidates: Vec<CompletionCandidate> = self
                    .plugin_commands
                    .iter()
                    .filter(|(name, _, _)| name.to_lowercase().starts_with(&prefix_lower))
                    .map(|(name, desc, arg_type)| {
                        let description = match arg_type.as_deref() {
                            Some(t) => format!("{}  <{}>", desc, t),
                            None => desc.clone(),
                        };
                        CompletionCandidate {
                            text: name.clone(),
                            description,
                            is_directory: false,
                        }
                    })
                    .collect();
                plugin_candidates.sort_by(|a, b| a.text.cmp(&b.text));
                result.candidates.extend(plugin_candidates);
                result
            }
            CompletionContext::Subcommand {
                parent,
                subcommand_prefix,
            } => {
                let mut result = complete_subcommand(parent, &parsed.prefix);
                if !subcommand_prefix.is_empty() {
                    for c in &mut result.candidates {
                        c.text = format!("{}{}", subcommand_prefix, c.text);
                    }
                    if !result.common_prefix.is_empty() {
                        result.common_prefix =
                            format!("{}{}", subcommand_prefix, result.common_prefix);
                    }
                }
                result
            }
            CompletionContext::SettingName => {
                complete_setting_name(&parsed.prefix, &SETTINGS_REGISTRY)
            }
            CompletionContext::SettingValue { name } => {
                complete_setting_value(name, &SETTINGS_REGISTRY, self.current_settings.as_ref())
            }
            CompletionContext::LocalSettingName => {
                complete_setting_name(&parsed.prefix, &DOCUMENT_SETTINGS_REGISTRY)
            }
            CompletionContext::LocalSettingValue { name } => complete_setting_value(
                name,
                &DOCUMENT_SETTINGS_REGISTRY,
                self.current_doc_options.as_ref(),
            ),
            CompletionContext::FilePath {
                dir,
                prefix: file_prefix,
                filter,
            } => {
                if signal.is_cancelled() {
                    return;
                }
                complete_filepath(dir, file_prefix, *filter, &signal)
            }
            CompletionContext::None => {
                // parse_context returns None for unknown commands. If the first token
                // is a plugin command with a declared arg_type, provide arg completions.
                complete_plugin_args(
                    &self.input,
                    &self.plugin_commands,
                    self.line_count,
                    &self.buf_words,
                    &signal,
                )
            }
        };

        let payload = CompletionPayload {
            result,
            input: self.input.clone(),
            token_start: parsed.token_start,
        };
        let _ = sender.send(JobMessage::Custom(id, Box::new(payload)));
        let _ = sender.send(JobMessage::Finished(id, true));
    }

    fn is_silent(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "completion"
    }
}

fn complete_plugin_args(
    input: &str,
    plugin_commands: &[(String, String, Option<String>)],
    line_count: usize,
    buf_words: &[String],
    signal: &CancellationSignal,
) -> CompletionResult {
    use crate::command_line::commands::completion::split_path_prefix;

    let has_trailing_space = input.ends_with(' ');
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() || (tokens.len() == 1 && !has_trailing_space) {
        return CompletionResult::empty();
    }

    let cmd_name = tokens[0].strip_prefix(':').unwrap_or(tokens[0]);
    let arg_type = plugin_commands
        .iter()
        .find(|(name, _, _)| name.eq_ignore_ascii_case(cmd_name))
        .and_then(|(_, _, at)| at.as_deref());

    let prefix = if has_trailing_space {
        ""
    } else {
        tokens.last().copied().unwrap_or("")
    };
    let prefix_lower = prefix.to_lowercase();

    match arg_type {
        Some("file") | Some("dir") | Some("path") => {
            let filter = match arg_type {
                Some("file") => PathFilter::FilesOnly,
                Some("dir") | Some("directory") => PathFilter::DirectoriesOnly,
                _ => PathFilter::Both,
            };
            let (dir, file_prefix) = split_path_prefix(prefix);
            complete_filepath(&dir, &file_prefix, filter, signal)
        }

        Some("int") => {
            if line_count == 0 {
                return CompletionResult::empty();
            }
            let candidates: Vec<CompletionCandidate> = (1..=line_count)
                .map(|n| n.to_string())
                .filter(|s| s.starts_with(&prefix_lower))
                .take(200)
                .map(|n| CompletionCandidate {
                    text: n,
                    description: String::new(),
                    is_directory: false,
                })
                .collect();
            CompletionResult::from_candidates(candidates)
        }

        Some("range") => {
            if line_count == 0 {
                return CompletionResult::empty();
            }
            let mid = line_count / 2;
            let presets = [
                (1, line_count, "whole file"),
                (1, mid.max(1), "first half"),
                (mid + 1, line_count, "second half"),
                (1, 10.min(line_count), "first 10 lines"),
            ];
            let candidates: Vec<CompletionCandidate> = presets
                .iter()
                .map(|(s, e, label)| (format!("{},{}", s, e), *label))
                .filter(|(range, _)| range.starts_with(prefix))
                .map(|(range, label)| CompletionCandidate {
                    text: range,
                    description: label.to_string(),
                    is_directory: false,
                })
                .collect();
            CompletionResult::from_candidates(candidates)
        }

        Some("word") | Some("string") => {
            let candidates: Vec<CompletionCandidate> = buf_words
                .iter()
                .filter(|w| w.to_lowercase().starts_with(&prefix_lower))
                .map(|w| CompletionCandidate {
                    text: w.clone(),
                    description: String::new(),
                    is_directory: false,
                })
                .collect();
            CompletionResult::from_candidates(candidates)
        }

        Some("bool") | Some("boolean") => {
            let candidates: Vec<CompletionCandidate> = ["true", "false"]
                .iter()
                .filter(|s| s.starts_with(&prefix_lower))
                .map(|s| CompletionCandidate {
                    text: s.to_string(),
                    description: String::new(),
                    is_directory: false,
                })
                .collect();
            CompletionResult::from_candidates(candidates)
        }

        Some("color") => {
            const COLORS: &[&str] = &[
                "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
                "bright_black", "bright_red", "bright_green", "bright_yellow",
                "bright_blue", "bright_magenta", "bright_cyan", "bright_white",
            ];
            let candidates: Vec<CompletionCandidate> = COLORS
                .iter()
                .filter(|c| c.starts_with(&prefix_lower))
                .map(|c| CompletionCandidate {
                    text: c.to_string(),
                    description: String::new(),
                    is_directory: false,
                })
                .collect();
            CompletionResult::from_candidates(candidates)
        }

        _ => CompletionResult::empty(),
    }
}

fn complete_filepath(
    dir: &str,
    file_prefix: &str,
    filter: PathFilter,
    signal: &CancellationSignal,
) -> CompletionResult {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return CompletionResult::empty(),
    };

    let prefix_lower = file_prefix.to_lowercase();
    let mut candidates: Vec<CompletionCandidate> = Vec::new();

    for entry in read_dir.flatten() {
        if signal.is_cancelled() {
            return CompletionResult::empty();
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }

        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        match filter {
            PathFilter::FilesOnly if is_dir => continue,
            PathFilter::DirectoriesOnly if !is_dir => continue,
            _ => {}
        }

        let text = if dir == "." {
            if is_dir {
                format!("{}/", name_str)
            } else {
                name_str.to_string()
            }
        } else {
            let dir_clean = dir.trim_end_matches('/');
            if is_dir {
                format!("{}/{}/", dir_clean, name_str)
            } else {
                format!("{}/{}", dir_clean, name_str)
            }
        };

        candidates.push(CompletionCandidate {
            text,
            description: if is_dir {
                "directory".into()
            } else {
                String::new()
            },
            is_directory: is_dir,
        });
    }

    CompletionResult::from_candidates(candidates)
}
