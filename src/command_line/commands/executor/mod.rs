//! Command executor
//! Executes parsed commands and updates editor state

use crate::buffer::api::BufferView;
use crate::command_line::commands::ParsedCommand;
use crate::command_line::settings::SettingsRegistry;
use crate::document::{definitions::DocumentOptions, Document};
use crate::error::{ErrorType, RiftError};
use crate::state::State;
use crate::state::UserSettings;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Open undo tree visualization
    UndoTree {
        content: crate::state::OverlayContent,
    },
}

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
                    Ok(mut matches) => {
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

                        // Apply replacements
                        matches.sort_by_key(|m| std::cmp::Reverse(m.range.start));

                        let mut changes_made = 0;

                        matches.sort_by_key(|m| m.range.start);

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

                        // Sort reverse for application
                        valid_matches.sort_by_key(|m| std::cmp::Reverse(m.range.start));

                        for m in valid_matches {
                            // Delete
                            let range_len = m.range.end - m.range.start;
                            document.buffer.line_index.delete(m.range.start, range_len);

                            // Reset cursor to 0 because direct delete might have made current cursor OOB
                            document.buffer.move_to_start();

                            // Insert
                            let _ = document.buffer.set_cursor(m.range.start); // Set cursor
                            let _ = document.buffer.insert_str(&replacement); // Insert at cursor
                            changes_made += 1;
                        }

                        if changes_made > 0 {
                            document.buffer.revision += 1;
                            state.last_search_query = Some(pattern.clone());

                            // Re-run search to update highlights
                            match crate::search::find_all(&document.buffer, &pattern) {
                                Ok(new_matches) => state.search_matches = new_matches,
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
                let (lines, _seqs, cursor) = crate::undotree_view::render_tree(&document.history);

                // Create overlay content
                use crate::history::EditSeq;
                let selectable = _seqs.iter().map(|&s| s != EditSeq::MAX).collect();

                let mut preview: Vec<Vec<crate::layer::Cell>> = Vec::new();
                use crate::layer::Cell;
                preview.push(vec![
                    Cell::new(b'P'),
                    Cell::new(b'r'),
                    Cell::new(b'e'),
                    Cell::new(b'v'),
                    Cell::new(b'i'),
                    Cell::new(b'e'),
                    Cell::new(b'w'),
                ]);

                let content = crate::state::OverlayContent {
                    left: lines,
                    right: preview, // Placeholder
                    left_width_percent: 50,
                    cursor,
                    selectable,
                    sequences: _seqs,
                    right_scroll: 0,
                };
                ExecutionResult::UndoTree { content }
            }
        }
    }
}

mod local;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
