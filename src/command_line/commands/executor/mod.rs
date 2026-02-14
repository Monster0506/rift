use crate::buffer::api::BufferView;
use crate::command_line::commands::ParsedCommand;
use crate::command_line::settings::SettingsRegistry;
use crate::document::{definitions::DocumentOptions, Document};
use crate::error::{ErrorType, RiftError};
use crate::state::State;
use crate::state::UserSettings;

/// Result of executing a command
pub enum ExecutionResult {
    /// Command executed successfully
    Success,
    /// Quit command - editor should exit
    Quit {
        bangs: usize,
    },
    /// Write command - editor should save
    Write,
    /// Write and quit - editor should save then exit
    WriteAndQuit,
    /// Error occurred during execution (already reported to manager)
    Failure,
    /// Force a full redraw
    Redraw,
    /// Edit command - editor should open the specified file
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
    NotificationClear {
        bangs: usize,
    },

    /// Undo command - editor should undo changes
    Undo {
        count: Option<u64>,
    },
    /// Redo command - editor should redo changes
    Redo {
        count: Option<u64>,
    },
    /// Undo goto sequence - editor should jump to specific edit
    UndoGoto {
        seq: u64,
    },
    /// Checkpoint created successfully
    Checkpoint,
    /// Open a generic component (overlay)
    OpenComponent {
        component: Box<dyn crate::component::Component>,
        initial_job: Option<Box<dyn crate::job_manager::Job>>,
        initial_message: Option<crate::message::AppMessage>,
    },
    /// Spawn a background job
    SpawnJob(Box<dyn crate::job_manager::Job>),
    /// Open a terminal buffer
    OpenTerminal {
        cmd: Option<String>,
        bangs: usize,
    },
}

impl PartialEq for ExecutionResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Success, Self::Success) => true,
            (Self::Quit { bangs: b1 }, Self::Quit { bangs: b2 }) => b1 == b2,
            (Self::Write, Self::Write) => true,
            (Self::WriteAndQuit, Self::WriteAndQuit) => true,
            (Self::Failure, Self::Failure) => true,
            (Self::Redraw, Self::Redraw) => true,
            (
                Self::Edit {
                    path: p1,
                    bangs: b1,
                },
                Self::Edit {
                    path: p2,
                    bangs: b2,
                },
            ) => p1 == p2 && b1 == b2,
            (Self::BufferNext { bangs: b1 }, Self::BufferNext { bangs: b2 }) => b1 == b2,
            (Self::BufferPrevious { bangs: b1 }, Self::BufferPrevious { bangs: b2 }) => b1 == b2,
            (Self::BufferList, Self::BufferList) => true,
            (Self::NotificationClear { bangs: b1 }, Self::NotificationClear { bangs: b2 }) => {
                b1 == b2
            }
            (Self::Undo { count: c1 }, Self::Undo { count: c2 }) => c1 == c2,
            (Self::Redo { count: c1 }, Self::Redo { count: c2 }) => c1 == c2,
            (Self::UndoGoto { seq: s1 }, Self::UndoGoto { seq: s2 }) => s1 == s2,
            (Self::Checkpoint, Self::Checkpoint) => true,
            (Self::OpenComponent { .. }, Self::OpenComponent { .. }) => true, // Ignore content for equality check
            (Self::SpawnJob(_), Self::SpawnJob(_)) => true, // Ignore job content for equality
            (
                Self::OpenTerminal { cmd: c1, bangs: b1 },
                Self::OpenTerminal { cmd: c2, bangs: b2 },
            ) => c1 == c2 && b1 == b2,
            _ => false,
        }
    }
}
impl std::fmt::Debug for ExecutionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "Success"),
            Self::Quit { bangs } => f.debug_struct("Quit").field("bangs", bangs).finish(),
            Self::Write => write!(f, "Write"),
            Self::WriteAndQuit => write!(f, "WriteAndQuit"),
            Self::Failure => write!(f, "Failure"),
            Self::Redraw => write!(f, "Redraw"),
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
            Self::NotificationClear { bangs } => f
                .debug_struct("NotificationClear")
                .field("bangs", bangs)
                .finish(),
            Self::Undo { count } => write!(f, "Undo({:?})", count),
            Self::Redo { count } => write!(f, "Redo({:?})", count),
            Self::UndoGoto { seq } => write!(f, "UndoGoto({})", seq),
            Self::Checkpoint => write!(f, "Checkpoint"),
            Self::OpenComponent { .. } => write!(f, "OpenComponent(...)"),
            Self::SpawnJob(_) => write!(f, "SpawnJob(...)"),
            Self::OpenTerminal { cmd, bangs } => f
                .debug_struct("OpenTerminal")
                .field("cmd", cmd)
                .field("bangs", bangs)
                .finish(),
        }
    }
}

impl Eq for ExecutionResult {}

/// Command executor
pub struct CommandExecutor;

impl CommandExecutor {
    /// Execute a parsed command
    ///
    /// Modifies state as needed and returns the execution result
    ///
    /// Note: Write commands do NOT perform file I/O here.
    /// They return Success/WriteAndQuit, and the editor is responsible
    /// for calling Document::save() or Document::save_as().
    pub fn execute(
        command: ParsedCommand,
        state: &mut State,
        document: &mut Document,
        settings_registry: &SettingsRegistry<UserSettings>,
        document_settings_registry: &SettingsRegistry<DocumentOptions>,
    ) -> ExecutionResult {
        match command {
            ParsedCommand::Quit { bangs } => ExecutionResult::Quit { bangs },
            ParsedCommand::Terminal { cmd, bangs } => ExecutionResult::OpenTerminal { cmd, bangs },
            ParsedCommand::Set {
                option,
                value,
                bangs: _,
            } => {
                let mut errors = Vec::new();
                let mut error_handler = |e: RiftError| errors.push(e);
                let result = settings_registry.execute_setting(
                    &option,
                    value,
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
            ParsedCommand::Write { path, bangs: _ } => {
                // Set the path in state if provided (for :w filename)
                if let Some(ref file_path) = path {
                    state.set_file_path(Some(file_path.clone()));
                }
                // Editor will check if path exists and call Document::save()
                ExecutionResult::Write
            }
            ParsedCommand::WriteQuit { path, bangs: _ } => {
                // Set the path in state if provided (for :wq filename)
                if let Some(ref file_path) = path {
                    state.set_file_path(Some(file_path.clone()));
                }
                // Editor will check if path exists, call Document::save(), then quit
                ExecutionResult::WriteAndQuit
            }
            ParsedCommand::Unknown { name } => {
                state.handle_error(RiftError::new(
                    ErrorType::Parse,
                    "UNKNOWN_COMMAND",
                    format!("Unknown command: {name}"),
                ));
                ExecutionResult::Failure
            }
            ParsedCommand::Ambiguous { prefix, matches } => {
                let matches_str = matches.join(", ");
                state.handle_error(RiftError::new(
                    ErrorType::Parse,
                    "AMBIGUOUS_COMMAND",
                    format!("Ambiguous command '{prefix}': matches {matches_str}"),
                ));
                ExecutionResult::Failure
            }
            ParsedCommand::Redraw { bangs: _ } => ExecutionResult::Redraw,

            ParsedCommand::Notify {
                kind,
                message,
                bangs,
            } => {
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
            ParsedCommand::Edit { path, bangs } => ExecutionResult::Edit { path, bangs },
            ParsedCommand::BufferNext { bangs } => ExecutionResult::BufferNext { bangs },
            ParsedCommand::BufferPrevious { bangs } => ExecutionResult::BufferPrevious { bangs },
            ParsedCommand::NoHighlight { bangs: _ } => {
                state.search_matches.clear();
                state.last_search_query = None;
                ExecutionResult::Redraw
            }
            ParsedCommand::Substitute {
                pattern,
                replacement,
                flags,
                range,
                bangs: _,
            } => {
                match crate::search::find_all(&document.buffer, &pattern) {
                    Ok((mut matches, _stats)) => {
                        let is_global_subst = flags.contains('g');
                        let whole_file = range.as_deref() == Some("%");

                        // Filtering matches
                        if !whole_file {
                            // Filter matches that intersect with current line
                            let current_line_idx = document
                                .buffer
                                .line_index
                                .get_line_at(document.buffer.cursor());
                            let start_byte = document.buffer.line_start(current_line_idx);
                            // Use get_end from LineIndex, passing total length
                            let end_byte = document
                                .buffer
                                .line_index
                                .get_end(current_line_idx, document.buffer.len())
                                .unwrap_or(document.buffer.len());

                            matches
                                .retain(|m| m.range.start >= start_byte && m.range.end <= end_byte);
                        }

                        if matches.is_empty() {
                            state.handle_error(RiftError::new(
                                ErrorType::Execution,
                                "PATTERN_NOT_FOUND",
                                format!("Pattern not found: {pattern}"),
                            ));
                            return ExecutionResult::Failure;
                        }

                        // Filter matches based on global flag
                        let mut valid_matches = Vec::new();
                        let mut last_line_idx = None;

                        for m in matches {
                            let line_idx = document.buffer.line_index.get_line_at(m.range.start);
                            if !is_global_subst {
                                if Some(line_idx) == last_line_idx {
                                    continue; // Already processed first match on this line
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

                        // Begin transaction to group all substitutions
                        document.begin_transaction(format!(
                            "Substitute: {} → {}",
                            pattern, replacement
                        ));

                        // Sort reverse for application (to maintain offsets)
                        valid_matches.sort_by_key(|m| std::cmp::Reverse(m.range.start));

                        let mut changes_made = 0;
                        for m in valid_matches {
                            // Delete the matched text using undo-aware method
                            if let Err(e) = document.delete_range(m.range.start, m.range.end) {
                                state.handle_error(e);
                                continue;
                            }

                            // Insert replacement text using undo-aware method
                            if let Err(e) = document.insert_str(&replacement) {
                                state.handle_error(e);
                                continue;
                            }

                            changes_made += 1;
                        }

                        // Commit transaction
                        document.commit_transaction();

                        if changes_made > 0 {
                            state.last_search_query = Some(pattern.clone());

                            // Re-run search to update highlights
                            match crate::search::find_all(&document.buffer, &pattern) {
                                Ok((new_matches, _)) => state.search_matches = new_matches,
                                Err(_) => state.search_matches.clear(),
                            }
                        }

                        state.notify(
                            crate::notification::NotificationType::Info,
                            format!("{} substitutions", changes_made),
                        );
                        ExecutionResult::Redraw
                    }
                    Err(e) => {
                        state.handle_error(e);
                        ExecutionResult::Failure
                    }
                }
            }
            ParsedCommand::BufferList => ExecutionResult::BufferList,
            ParsedCommand::Undo { count, bangs: _ } => ExecutionResult::Undo { count },
            ParsedCommand::Redo { count, bangs: _ } => ExecutionResult::Redo { count },
            ParsedCommand::UndoGoto { seq, bangs: _ } => ExecutionResult::UndoGoto { seq },
            ParsedCommand::Checkpoint { bangs: _ } => {
                // Create checkpoint at current position
                document.checkpoint();
                state.notify(
                    crate::notification::NotificationType::Info,
                    "Checkpoint created".to_string(),
                );
                ExecutionResult::Checkpoint
            }
            ParsedCommand::UndoTree { bangs: _ } => {
                let (component, initial_message) =
                    crate::undotree_view::component::create_undo_tree_component(
                        &document.history,
                        &state.settings,
                    );
                ExecutionResult::OpenComponent {
                    component,
                    initial_job: None,
                    initial_message,
                }
            }
            ParsedCommand::Explore { path, bangs: _ } => {
                let initial_path = if let Some(p) = path {
                    std::path::PathBuf::from(p)
                } else {
                    // Check if document has path
                    if let Some(p) = document.path() {
                        if p.is_dir() {
                            p.to_path_buf()
                        } else {
                            p.parent().unwrap_or(p).to_path_buf()
                        }
                    } else {
                        std::env::current_dir().unwrap_or_default()
                    }
                };

                let mut explorer = crate::file_explorer::FileExplorer::new(initial_path);
                explorer = explorer.with_colors(state.settings.editor_fg, state.settings.editor_bg);

                let job = explorer.create_list_job();

                ExecutionResult::OpenComponent {
                    component: Box::new(explorer),
                    initial_job: Some(job),
                    initial_message: None,
                }
            }
        }
    }
}

mod local;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
