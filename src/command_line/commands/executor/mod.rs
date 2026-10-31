use crate::buffer::api::BufferView;
use crate::command_line::commands::types::ParsedCommand;
use crate::command_line::settings::registry::SettingsRegistry;
use crate::document::{definitions::DocumentOptions, Document};
use crate::error::{ErrorType, RiftError};
use crate::state::State;
use crate::state::UserSettings;

/// Result of executing a command
#[derive(PartialEq, Eq)]
pub(crate) enum ExecutionResult {
    /// Command executed successfully
    Success,
    /// Quit command
    Quit {
        bangs: usize,
    },
    /// Write command
    Write {
        path: Option<std::path::PathBuf>,
    },
    /// Write and quit
    WriteAndQuit {
        path: Option<std::path::PathBuf>,
    },
    /// Error occurred during execution
    Failure,
    /// Force a full redraw
    Redraw,
    /// Reload the plugins
    Reload,
    /// Edit command
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
    BufferList,
    /// Jump to the buffer at the given 1-based index
    BufferGoto {
        index: usize,
    },
    /// Open the interactive buffer-list split panel
    OpenBufferList,
    /// Delete a buffer by index, or the current one when absent.
    BufferDelete {
        index: Option<usize>,
        bangs: usize,
    },
    NotificationClear {
        bangs: usize,
    },
    /// Undo command
    Undo {
        count: Option<u64>,
    },
    /// Redo command
    Redo {
        count: Option<u64>,
    },
    /// Undo goto sequence
    UndoGoto {
        seq: u64,
    },
    /// Checkpoint created successfully
    Checkpoint,
    /// Open a terminal buffer
    OpenTerminal {
        cmd: Option<String>,
        bangs: usize,
    },
    /// Run a git subcommand and show its output
    RunGit {
        args: String,
    },
    SplitWindow {
        direction: crate::split::tree::SplitDirection,
        subcommand: crate::command_line::commands::types::SplitSubcommand,
    },
    /// Open a directory as a file-explorer buffer
    OpenDirectory {
        path: std::path::PathBuf,
    },
    /// Open an undo-tree buffer for the active document
    OpenUndoTree,
    /// Open the messages log buffer
    OpenMessages {
        show_all: bool,
    },
    /// Open the clipboard ring index buffer
    OpenClipboard,
    /// The editor checks the plugin host before reporting an error.
    PluginCommand {
        name: String,
        args: Vec<String>,
    },
}

impl std::fmt::Debug for ExecutionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "Success"),
            Self::Quit { bangs } => f.debug_struct("Quit").field("bangs", bangs).finish(),
            Self::Write { path } => f.debug_struct("Write").field("path", path).finish(),
            Self::WriteAndQuit { path } => {
                f.debug_struct("WriteAndQuit").field("path", path).finish()
            }
            Self::Failure => write!(f, "Failure"),
            Self::Redraw => write!(f, "Redraw"),
            Self::Reload => write!(f, "Reload"),
            Self::Edit { path, bangs } => f
                .debug_struct("Edit")
                .field("path", path)
                .field("bangs", bangs)
                .finish(),
            Self::BufferNext { bangs } => {
                f.debug_struct("BufferNext").field("bangs", bangs).finish()
            }
            Self::BufferPrevious { bangs } => f
                .debug_struct("BufferPrevious")
                .field("bangs", bangs)
                .finish(),
            Self::BufferList => write!(f, "BufferList"),
            Self::BufferGoto { index } => write!(f, "BufferGoto({index})"),
            Self::OpenBufferList => write!(f, "OpenBufferList"),
            Self::BufferDelete { index, bangs } => f
                .debug_struct("BufferDelete")
                .field("index", index)
                .field("bangs", bangs)
                .finish(),
            Self::NotificationClear { bangs } => f
                .debug_struct("NotificationClear")
                .field("bangs", bangs)
                .finish(),
            Self::Undo { count } => write!(f, "Undo({count:?})"),
            Self::Redo { count } => write!(f, "Redo({count:?})"),
            Self::UndoGoto { seq } => write!(f, "UndoGoto({seq})"),
            Self::Checkpoint => write!(f, "Checkpoint"),
            Self::OpenTerminal { cmd, bangs } => f
                .debug_struct("OpenTerminal")
                .field("cmd", cmd)
                .field("bangs", bangs)
                .finish(),
            Self::RunGit { args } => f.debug_struct("RunGit").field("args", args).finish(),
            Self::SplitWindow {
                direction,
                subcommand,
            } => f
                .debug_struct("SplitWindow")
                .field("direction", direction)
                .field("subcommand", subcommand)
                .finish(),
            Self::OpenDirectory { path } => {
                f.debug_struct("OpenDirectory").field("path", path).finish()
            }
            Self::OpenUndoTree => write!(f, "OpenUndoTree"),
            Self::OpenMessages { show_all } => write!(f, "OpenMessages({show_all})"),
            Self::OpenClipboard => write!(f, "OpenClipboard"),
            Self::PluginCommand { name, args } => f
                .debug_struct("PluginCommand")
                .field("name", name)
                .field("args", args)
                .finish(),
        }
    }
}

/// Command executor
pub(crate) struct CommandExecutor;

impl CommandExecutor {
    pub(crate) fn execute(
        command: ParsedCommand,
        state: &mut State,
        document: &mut Document,
        settings_registry: &SettingsRegistry<UserSettings>,
        document_settings_registry: &SettingsRegistry<DocumentOptions>,
    ) -> ExecutionResult {
        match command {
            ParsedCommand::Quit { bangs } => ExecutionResult::Quit { bangs },
            ParsedCommand::Terminal { cmd, bangs } => ExecutionResult::OpenTerminal { cmd, bangs },
            ParsedCommand::Git { args, bangs: _ } => ExecutionResult::RunGit { args },
            ParsedCommand::Set {
                option,
                value,
                bangs: _,
            } => {
                let mut errors = Vec::new();
                let mut error_handler = |e: RiftError| errors.push(e);
                let result = settings_registry.execute_setting(
                    &option,
                    value.as_deref(),
                    &mut state.settings,
                    &mut error_handler,
                );
                for err in errors {
                    state.handle_error(err);
                }
                result
            }
            ParsedCommand::SetLocal {
                option: _,
                value: _,
                bangs: _,
            } => local::execute_local_command(command, state, document, document_settings_registry),
            ParsedCommand::Write { path, bangs } => {
                execute_write(state, document, path, bangs, |path| {
                    ExecutionResult::Write { path }
                })
            }
            ParsedCommand::WriteQuit { path, bangs } => {
                execute_write(state, document, path, bangs, |path| {
                    ExecutionResult::WriteAndQuit { path }
                })
            }
            ParsedCommand::Unknown { name, args } => ExecutionResult::PluginCommand { name, args },
            ParsedCommand::Ambiguous { prefix, matches } => {
                report_ambiguous(state, &prefix, &matches)
            }
            ParsedCommand::Redraw { bangs: _ } => ExecutionResult::Redraw,

            ParsedCommand::Reload { bangs: _ } => ExecutionResult::Reload,

            ParsedCommand::Notify {
                kind,
                message,
                bangs,
            } => execute_notify(state, &kind, message, bangs),
            ParsedCommand::Edit { path, bangs } => execute_edit(state, path, bangs),
            ParsedCommand::BufferNext { bangs } => ExecutionResult::BufferNext { bangs },
            ParsedCommand::BufferPrevious { bangs } => ExecutionResult::BufferPrevious { bangs },
            ParsedCommand::NoHighlight { bangs: _ } => clear_highlight(state),
            ParsedCommand::Substitute {
                pattern,
                replacement,
                flags,
                range,
                bangs: _,
            } => execute_substitute(
                state,
                document,
                &pattern,
                &replacement,
                &flags,
                range.as_deref(),
            ),
            ParsedCommand::BufferList => ExecutionResult::BufferList,
            ParsedCommand::BufferGoto { index, bangs: _ } => ExecutionResult::BufferGoto { index },
            ParsedCommand::OpenBufferList { bangs: _ } => ExecutionResult::OpenBufferList,
            ParsedCommand::BufferDelete { index, bangs } => {
                ExecutionResult::BufferDelete { index, bangs }
            }
            ParsedCommand::Undo { count, bangs: _ } => ExecutionResult::Undo { count },
            ParsedCommand::Redo { count, bangs: _ } => ExecutionResult::Redo { count },
            ParsedCommand::UndoGoto { seq, bangs: _ } => ExecutionResult::UndoGoto { seq },
            ParsedCommand::Checkpoint { bangs: _ } => create_checkpoint(state, document),
            ParsedCommand::UndoTree { bangs: _ } => ExecutionResult::OpenUndoTree,
            ParsedCommand::Messages { show_all, bangs: _ } => {
                ExecutionResult::OpenMessages { show_all }
            }
            ParsedCommand::Clipboard { bangs: _ } => ExecutionResult::OpenClipboard,
            ParsedCommand::Split {
                subcommand,
                bangs: _,
            } => ExecutionResult::SplitWindow {
                direction: crate::split::tree::SplitDirection::Horizontal,
                subcommand,
            },
            ParsedCommand::VSplit {
                subcommand,
                bangs: _,
            } => ExecutionResult::SplitWindow {
                direction: crate::split::tree::SplitDirection::Vertical,
                subcommand,
            },
            ParsedCommand::File { path, bangs: _ } => execute_file(state, document, path),
        }
    }
}

fn create_checkpoint(state: &mut State, document: &mut Document) -> ExecutionResult {
    document.checkpoint();
    state.notify(
        crate::notification::NotificationType::Info,
        "Checkpoint created".to_string(),
    );
    ExecutionResult::Checkpoint
}

fn clear_highlight(state: &mut State) -> ExecutionResult {
    state.search_matches.clear();
    state.last_search_query = None;
    ExecutionResult::Redraw
}

fn execute_write(
    state: &mut State,
    document: &Document,
    path: Option<String>,
    bangs: usize,
    result: impl FnOnce(Option<std::path::PathBuf>) -> ExecutionResult,
) -> ExecutionResult {
    let path = path.map(|p| crate::fs_backend::backend().canonicalize(std::path::Path::new(&p)));
    if overwrite_refused(state, document, path.as_deref(), bangs) {
        return ExecutionResult::Failure;
    }
    result(path)
}

fn overwrite_refused(
    state: &mut State,
    document: &Document,
    target: Option<&std::path::Path>,
    bangs: usize,
) -> bool {
    let Some(target) = target else {
        return false;
    };
    if bangs > 0 || document.path() == Some(target) {
        return false;
    }
    if !crate::fs_backend::backend().path_exists(target) {
        return false;
    }

    state.handle_error(RiftError::new(
        ErrorType::Execution,
        crate::constants::errors::FILE_EXISTS,
        crate::constants::errors::MSG_FILE_EXISTS.to_string(),
    ));
    true
}

fn report_ambiguous(state: &mut State, prefix: &str, matches: &[String]) -> ExecutionResult {
    let matches_str = matches.join(", ");
    state.handle_error(RiftError::new(
        ErrorType::Parse,
        "AMBIGUOUS_COMMAND",
        format!("Ambiguous command '{prefix}': matches {matches_str}"),
    ));
    ExecutionResult::Failure
}

fn execute_file(state: &mut State, document: &Document, path: Option<String>) -> ExecutionResult {
    let initial_path = if let Some(p) = path {
        let path_buf = std::path::PathBuf::from(p);
        if path_buf.exists() && path_buf.is_file() {
            state.handle_error(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::NOT_A_DIRECTORY,
                crate::constants::errors::MSG_NOT_A_DIRECTORY.to_string(),
            ));
            return ExecutionResult::Failure;
        }
        path_buf
    } else if let Some(p) = document.path() {
        if p.is_dir() {
            p.to_path_buf()
        } else {
            p.parent().unwrap_or(p).to_path_buf()
        }
    } else {
        std::env::current_dir().unwrap_or_default()
    };

    ExecutionResult::OpenDirectory { path: initial_path }
}

fn execute_notify(state: &mut State, kind: &str, message: String, bangs: usize) -> ExecutionResult {
    use crate::notification::NotificationType;
    if kind.to_lowercase().as_str() == "clear" {
        return ExecutionResult::NotificationClear { bangs };
    }
    let notification_kind = match kind.to_lowercase().as_str() {
        "info" => NotificationType::Info,
        "warning" | "warn" => NotificationType::Warning,
        "error" => NotificationType::Error,
        "success" => NotificationType::Success,
        _ => {
            state.handle_error(RiftError::new(
                ErrorType::Execution,
                "INVALID_NOTIFY_TYPE",
                format!("Unknown notification type: {kind}"),
            ));
            return ExecutionResult::Failure;
        }
    };

    state.notify(notification_kind, message);
    ExecutionResult::Success
}

fn execute_edit(state: &mut State, path: Option<String>, bangs: usize) -> ExecutionResult {
    if let Some(ref p) = path {
        let path_buf = std::path::Path::new(p);
        if path_buf.exists() && path_buf.is_dir() {
            state.handle_error(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::NOT_A_FILE,
                crate::constants::errors::MSG_NOT_A_FILE.to_string(),
            ));
            return ExecutionResult::Failure;
        }
        if !path_buf.exists() && crate::document::manager::parent_dir_missing(path_buf) {
            state.handle_error(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::PARENT_DIR_MISSING,
                crate::constants::errors::MSG_PARENT_DIR_MISSING.to_string(),
            ));
            return ExecutionResult::Failure;
        }
    }
    ExecutionResult::Edit { path, bangs }
}

fn execute_substitute(
    state: &mut State,
    document: &mut Document,
    pattern: &str,
    replacement: &str,
    flags: &str,
    range: Option<&str>,
) -> ExecutionResult {
    match crate::search::find_all(&document.buffer, pattern) {
        Ok((mut matches, _stats)) => {
            let is_global_subst = flags.contains('g');
            let whole_file = range == Some("%");
            let has_selection = !document.selection_set.is_empty();

            if has_selection {
                let region_spans: Vec<(usize, usize)> = document
                    .selection_set
                    .sorted()
                    .iter()
                    .map(|r| r.buffer_span(&document.buffer))
                    .collect();
                matches.retain(|m| {
                    region_spans
                        .iter()
                        .any(|(s, e)| m.range.start >= *s && m.range.end <= *e)
                });
            } else if !whole_file {
                let current_line_idx = document
                    .buffer
                    .line_index
                    .get_line_at(document.buffer.cursor());
                let start_byte = document.buffer.line_start(current_line_idx);
                let end_byte = document
                    .buffer
                    .line_index
                    .get_end(current_line_idx, document.buffer.len())
                    .unwrap_or(document.buffer.len());

                matches.retain(|m| m.range.start >= start_byte && m.range.end <= end_byte);
            }

            if matches.is_empty() {
                state.handle_error(RiftError::new(
                    ErrorType::Execution,
                    "PATTERN_NOT_FOUND",
                    format!("Pattern not found: {pattern}"),
                ));
                return ExecutionResult::Failure;
            }

            let mut valid_matches = Vec::new();
            let mut last_line_idx = None;

            for m in matches {
                let line_idx = document.buffer.line_index.get_line_at(m.range.start);
                if !is_global_subst {
                    if Some(line_idx) == last_line_idx {
                        continue;
                    }
                    last_line_idx = Some(line_idx);
                }
                valid_matches.push(m);
            }

            if valid_matches.is_empty() {
                state.handle_error(RiftError::new(
                    ErrorType::Execution,
                    "PATTERN_NOT_FOUND",
                    format!("Pattern not found: {pattern}"),
                ));
                return ExecutionResult::Failure;
            }

            document.begin_transaction(format!("Substitute: {pattern} -> {replacement}"));

            valid_matches.sort_by_key(|m| std::cmp::Reverse(m.range.start));

            let mut changes_made = 0;
            for m in valid_matches {
                if let Err(e) = document.replace_range(m.range.start, m.range.end, replacement) {
                    state.handle_error(e);
                    continue;
                }

                changes_made += 1;
            }

            document.commit_transaction();
            if has_selection {
                document.selection_set.clear();
            }

            if changes_made > 0 {
                state.last_search_query = Some(pattern.to_string());

                match crate::search::find_all(&document.buffer, pattern) {
                    Ok((new_matches, _)) => state.search_matches = new_matches,
                    Err(_) => state.search_matches.clear(),
                }
            }

            state.notify(
                crate::notification::NotificationType::Info,
                format!("{changes_made} substitutions"),
            );
            ExecutionResult::Redraw
        }
        Err(e) => {
            state.handle_error(e);
            ExecutionResult::Failure
        }
    }
}

mod local;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
