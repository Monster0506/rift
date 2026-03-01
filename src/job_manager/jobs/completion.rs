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
}

impl Job for CompletionJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let parsed = parse_context(&self.input, &SETTINGS_REGISTRY, &DOCUMENT_SETTINGS_REGISTRY);

        let result = match &parsed.context {
            CompletionContext::CommandName => complete_command_name(&parsed.prefix),
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
            CompletionContext::None => CompletionResult::empty(),
        };

        let payload = CompletionPayload {
            result,
            input: self.input.clone(),
            token_start: parsed.token_start,
        };
        let _ = sender.send(JobMessage::Custom(id, Box::new(payload)));
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
