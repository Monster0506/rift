//! Background job for computing tab completions.
//! Handles filesystem enumeration; in-memory lookups run inline.

use crate::command_line::commands::completion::{
    complete_command_name, complete_setting_name, complete_setting_value, complete_subcommand,
    longest_common_prefix_of, parse_context, CompletionCandidate, CompletionContext,
    CompletionResult,
};
use crate::command_line::settings::create_settings_registry;
use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload};
use std::any::Any;
use std::sync::mpsc::Sender;

/// Sent back to the editor via `JobMessage::Custom`
#[derive(Debug, Clone)]
pub struct CompletionPayload {
    pub result: CompletionResult,
    /// The command line content that was being completed (for staleness checks)
    pub input: String,
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

/// Job that computes completion candidates for the current command line input.
#[derive(Debug)]
pub struct CompletionJob {
    /// Full command line content (without the `:` prompt)
    pub input: String,
}

impl Job for CompletionJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }

        let settings_registry = create_settings_registry();
        let (context, prefix) = parse_context(&self.input, &settings_registry);

        let result = match &context {
            CompletionContext::CommandName => complete_command_name(&prefix),
            CompletionContext::Subcommand { parent } => {
                let mut result = complete_subcommand(parent, &prefix);
                if parent == "split" || parent == "vsplit" {
                    for c in &mut result.candidates {
                        c.text = format!(":{}", c.text);
                    }
                    if !result.common_prefix.is_empty() {
                        result.common_prefix = format!(":{}", result.common_prefix);
                    }
                }
                result
            }
            CompletionContext::SettingName => complete_setting_name(&prefix, &settings_registry),
            CompletionContext::SettingValue { name } => {
                complete_setting_value(name, &settings_registry)
            }
            CompletionContext::FilePath {
                dir,
                prefix: file_prefix,
            } => {
                if signal.is_cancelled() {
                    return;
                }
                complete_filepath(dir, file_prefix, &signal)
            }
            CompletionContext::None => CompletionResult::empty(),
        };

        let payload = CompletionPayload {
            result,
            input: self.input.clone(),
        };
        let _ = sender.send(JobMessage::Custom(id, Box::new(payload)));
    }
}

/// Enumerate files/dirs in `dir` matching `file_prefix`. Dirs first, then alphabetical.
fn complete_filepath(
    dir: &str,
    file_prefix: &str,
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

        // Build path relative to original prefix, with trailing `/` for directories
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
        });
    }

    // Dirs first, then alphabetical within each group
    candidates.sort_by(|a, b| {
        let a_dir = a.description == "directory";
        let b_dir = b.description == "directory";
        b_dir.cmp(&a_dir).then(a.text.cmp(&b.text))
    });

    let strs: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    let common = longest_common_prefix_of(&strs);

    CompletionResult {
        common_prefix: common,
        candidates,
    }
}
