#[allow(unused_imports)]
use crate::term::TerminalBackend;
use super::Editor;
use super::{PostPasteState, PanelKind, PanelLayout};
use super::{resolve_display_map, plugin_dirs};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::mode::Mode;
use crate::command::Command;
use crate::action::{Action, EditorAction, Motion};
use crate::document::{Document, DocumentId};
use crate::dot_repeat::{DotRepeat, DotRegister};
use crate::keymap::KeyMap;
use crate::split::tree::SplitTree;
use crate::state::{State, UserSettings};
use crate::search::SearchDirection;
use crate::executor::execute_command;
use crate::key_handler::KeyAction;
use crate::render;
use crate::screen_buffer::FrameStats;
use crate::command_line::commands::{CommandExecutor, CommandParser};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use std::sync::Arc;

impl<T: TerminalBackend> Editor<T> {
    pub fn term_mut(&mut self) -> &mut T {
        &mut self.term
    }

    pub(super) fn execute_operator(&mut self, op: crate::action::OperatorType, motion: Motion) -> bool {
        let count = if self.pending_count > 0 {
            self.pending_count
        } else {
            1
        };
        self.pending_operator = None;
        self.pending_count = 0;

        // Capture text to ring before any destructive operation, and for yank.
        let viewport_height = self.render_system.viewport.visible_rows();
        let last_search_query = self.state.last_search_query.clone();
        let captured = self.document_manager.active_document_mut().and_then(|doc| {
            let tab_width = doc.options.tab_width;
            crate::executor::compute_motion_range(
                motion,
                count,
                doc,
                viewport_height,
                last_search_query.as_deref(),
                tab_width,
            )
            .map(|range| crate::clipboard::capture_text(&doc.buffer, &range))
        });
        let in_clipboard = self
            .document_manager
            .active_document()
            .map(|d| d.is_any_clipboard())
            .unwrap_or(false);
        if let Some(text) = captured.filter(|s| !s.is_empty()) {
            if !in_clipboard {
                self.clipboard_ring.push(text);
                self.refresh_clipboard_buffer_if_open();
            }
        }

        match op {
            crate::action::OperatorType::Delete => {
                let command = crate::command::Command::Delete(motion, count);
                self.set_mode(Mode::Normal);
                let result = self.execute_buffer_command(command);
                if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            crate::action::OperatorType::Change => {
                let command = crate::command::Command::Change(motion, count);
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                self.set_mode(Mode::Normal);
                self.execute_buffer_command(command);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
                true
            }
            crate::action::OperatorType::Yank => {
                // Text already captured above; just return to Normal.
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

    pub(super) fn execute_operator_linewise(&mut self, op: crate::action::OperatorType) -> bool {
        self.pending_operator = None;

        // Capture current line text for all operators.
        let captured = self
            .document_manager
            .active_document()
            .map(|doc| crate::clipboard::capture_current_line(&doc.buffer));
        let in_clipboard = self
            .document_manager
            .active_document()
            .map(|d| d.is_any_clipboard())
            .unwrap_or(false);
        if let Some(text) = captured.filter(|s| !s.is_empty()) {
            if !in_clipboard {
                self.clipboard_ring.push(text);
                self.refresh_clipboard_buffer_if_open();
            }
        }

        match op {
            crate::action::OperatorType::Delete => {
                let command = crate::command::Command::DeleteLine;
                self.set_mode(Mode::Normal);
                let result = self.execute_buffer_command(command);
                if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            crate::action::OperatorType::Change => {
                let command = crate::command::Command::ChangeLine;
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                self.set_mode(Mode::Normal);
                self.execute_buffer_command(command);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
                true
            }
            crate::action::OperatorType::Yank => {
                // Text already captured above; just return to Normal.
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

    /// Replay the last repeatable action (dot-repeat)
    pub(super) fn execute_dot_repeat(&mut self) -> bool {
        let register = match self.dot_repeat.register() {
            Some(reg) => reg.clone(),
            None => return false,
        };

        let count = if self.pending_count > 0 {
            self.pending_count
        } else {
            1
        };

        self.dot_repeat.set_replaying(true);

        match register {
            DotRegister::Single(cmd) => {
                for _ in 0..count {
                    self.execute_buffer_command(cmd);
                }
            }
            DotRegister::InsertSession { entry, commands } => {
                for _ in 0..count {
                    // Enter insert mode (handles cursor positioning for a/A/I)
                    self.handle_mode_management(entry);

                    // Replay all commands from the session
                    for &cmd in &commands {
                        self.execute_buffer_command(cmd);
                    }

                    // Exit insert mode: commit transaction
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                    self.set_mode(Mode::Normal);
                }
            }
        }

        self.dot_repeat.set_replaying(false);
        true
    }
}
