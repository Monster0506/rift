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
    pub fn run(&mut self) -> Result<(), RiftError> {
        // Initial render
        self.update_and_render()?;

        // Main event loop
        while !self.should_quit {
            let mut jobs_changed = false;
            const MAX_JOB_MESSAGES: usize = 10;
            let mut processed_jobs = 0;
            while processed_jobs < MAX_JOB_MESSAGES {
                if let Ok(msg) = self.job_manager.receiver().try_recv() {
                    self.handle_job_message(msg)?;
                    processed_jobs += 1;
                    jobs_changed = true;
                } else {
                    break;
                }
            }

            let current_gen = self.state.error_manager.notifications().generation;
            let notif_changed = current_gen != self.last_notification_generation;
            if notif_changed {
                self.refresh_messages_buffer_if_open();
            }

            // Poll for input
            let timeout = self.state.settings.poll_timeout_ms;
            if self
                .term
                .poll(std::time::Duration::from_millis(timeout))
                .map_err(|e| {
                    RiftError::new(
                        ErrorType::Internal,
                        crate::constants::errors::POLL_FAILED,
                        e,
                    )
                })?
            {
                // Read key
                let key_press = match self.term.read_key()? {
                    Some(key) => key,
                    None => continue,
                };

                // Update debug info
                self.state.update_keypress(key_press);

                use crate::key::Key;
                use crate::keymap::{KeyContext, MatchResult};

                // Handle special keys immediately
                if let Key::Resize(cols, rows) = key_press {
                    self.render_system.resize(rows as usize, cols as usize);
                    self.update_and_render()?;
                    continue;
                }

                // Escape closes an open plugin float before any other key handling.
                if key_press == Key::Escape && self.plugin_host.has_open_float() {
                    self.plugin_host.close_float();
                    self.update_and_render()?;
                    continue;
                }

                // Handle terminal input
                let is_terminal_insert = if let Some(doc) = self.document_manager.active_document()
                {
                    doc.is_terminal() && self.current_mode == Mode::Insert
                } else {
                    false
                };

                if is_terminal_insert {
                    // Ctrl+\ exits terminal insert mode
                    let is_exit = matches!(key_press, Key::Ctrl(92));

                    if is_exit {
                        self.set_mode(Mode::Normal);
                        self.update_state_and_render(
                            key_press,
                            crate::key_handler::KeyAction::Continue,
                            crate::command::Command::Noop,
                        )?;
                        continue;
                    }

                    if let Some(doc) = self.document_manager.active_document_mut() {
                        if let Some(term) = &mut doc.terminal {
                            let bytes = key_press.to_vt100_bytes();
                            if !bytes.is_empty() {
                                if let Err(e) = term.write(&bytes) {
                                    self.state.notify(
                                        crate::notification::NotificationType::Error,
                                        format!("Write failed: {}", e),
                                    );
                                }
                            }
                        }
                    }
                    continue;
                }

                // Handle digits for count
                // Only if pending_keys is empty (start of sequence)
                // AND we are not in Insert mode (typing numbers)
                if self.pending_keys.is_empty()
                    && self.current_mode != Mode::Insert
                    && self.current_mode != Mode::Command
                    && self.current_mode != Mode::Search
                {
                    if let Key::Char(ch) = key_press {
                        if ch.is_ascii_digit() && (ch != '0' || self.pending_count > 0) {
                            let digit = ch.to_digit(10).unwrap() as usize;
                            self.pending_count =
                                self.pending_count.saturating_mul(10).saturating_add(digit);
                            // Update UI? (Render pending count)
                            self.update_state_and_render(
                                key_press,
                                crate::key_handler::KeyAction::Continue,
                                crate::command::Command::Noop,
                            )?;
                            continue;
                        }
                    }
                }

                // Push key to pending buffer
                self.pending_keys.push(key_press);

                // Input Processing Loop (allows backtracking)
                loop {
                    // 1. Resolve Context
                    let context = {
                        let is_directory = self
                            .document_manager
                            .active_document()
                            .map(|d| d.is_directory())
                            .unwrap_or(false);
                        let is_undotree = self
                            .document_manager
                            .active_document()
                            .map(|d| d.is_undotree())
                            .unwrap_or(false);
                        let is_clipboard = self
                            .document_manager
                            .active_document()
                            .map(|d| d.is_clipboard())
                            .unwrap_or(false);
                        let is_clipboard_entry = self
                            .document_manager
                            .active_document()
                            .map(|d| {
                                matches!(d.kind, crate::document::BufferKind::ClipboardEntry { .. })
                            })
                            .unwrap_or(false);
                        match self.current_mode {
                            Mode::Normal | Mode::OperatorPending => {
                                if is_directory {
                                    KeyContext::FileExplorer
                                } else if is_undotree {
                                    KeyContext::UndoTree
                                } else if is_clipboard {
                                    KeyContext::Clipboard
                                } else if is_clipboard_entry {
                                    KeyContext::ClipboardEntry
                                } else {
                                    KeyContext::Normal
                                }
                            }
                            Mode::Insert => KeyContext::Insert,
                            Mode::Command => KeyContext::Command,
                            Mode::Search => KeyContext::Search,
                        }
                    };

                    // 2. Lookup Action in KeyMap
                    let match_result = self.keymap.lookup(context, &self.pending_keys);

                    match match_result {
                        MatchResult::Exact(action) => {
                            let action = action.clone();
                            self.pending_keys.clear();

                            // 3. Dispatch Action
                            let _handled = self.handle_action(&action);

                            // Don't clear count if we just entered OperatorPending mode
                            // (count may be used with the subsequent motion)
                            if self.current_mode != Mode::OperatorPending {
                                self.pending_count = 0;
                            }

                            self.update_state_and_render(
                                key_press,
                                crate::key_handler::KeyAction::Continue,
                                crate::command::Command::Noop,
                            )?;
                            break;
                        }
                        MatchResult::Ambiguous(action) => {
                            // Valid prefix AND executable action (e.g. 'd').
                            // For operators, execute immediately to enter OperatorPending mode
                            // so that subsequent digits are handled as counts.
                            if let Action::Editor(EditorAction::Operator(_)) = action {
                                let action = action.clone();
                                self.pending_keys.clear();
                                self.handle_action(&action);
                                // Don't clear pending_count for operators
                                self.update_state_and_render(
                                    key_press,
                                    crate::key_handler::KeyAction::Continue,
                                    crate::command::Command::Noop,
                                )?;
                                break;
                            }
                            // For non-operators, wait for more keys
                            self.update_state_and_render(
                                key_press,
                                crate::key_handler::KeyAction::Continue,
                                crate::command::Command::Noop,
                            )?;
                            break;
                        }
                        MatchResult::Prefix => {
                            // Wait for more keys.
                            self.update_state_and_render(
                                key_press,
                                crate::key_handler::KeyAction::Continue,
                                crate::command::Command::Noop,
                            )?;
                            break;
                        }
                        MatchResult::None => {
                            // Invalid sequence. Backtrack?
                            if self.pending_keys.len() > 1 {
                                let last = self.pending_keys.pop().unwrap();
                                // Check if prefix was ambiguous (valid action)
                                match self.keymap.lookup(context, &self.pending_keys) {
                                    MatchResult::Ambiguous(action) | MatchResult::Exact(action) => {
                                        let action = action.clone();
                                        self.pending_keys.clear();
                                        // Execute prefix action
                                        self.handle_action(&action);
                                        self.pending_count = 0;

                                        // Re-process last key
                                        self.pending_keys.push(last);
                                        continue;
                                    }
                                    _ => {
                                        // Prefix wasn't executable. Drop everything?
                                        self.pending_keys.clear();
                                    }
                                }
                            } else {
                                // Single key mismatch.
                                // Fallback for Insert Mode typing
                                if self.current_mode == Mode::Insert {
                                    let k = self.pending_keys[0];
                                    self.pending_keys.clear();
                                    if let Key::Char(ch) = k {
                                        self.handle_action(&Action::Editor(
                                            EditorAction::InsertChar(ch),
                                        ));
                                    }
                                    // Else ignore?
                                } else if self.current_mode == Mode::Command
                                    || self.current_mode == Mode::Search
                                {
                                    // Command Line typing handling (fallback)
                                    // Since KeyMap might not have all chars registered
                                    let k = self.pending_keys[0];
                                    self.pending_keys.clear();
                                    match k {
                                        Key::Tab => {
                                            self.handle_mode_management(Command::TabComplete);
                                        }
                                        Key::ShiftTab => {
                                            self.handle_mode_management(Command::TabCompletePrev);
                                        }
                                        Key::Char(ch) => {
                                            self.handle_action(&Action::Editor(
                                                EditorAction::InsertChar(ch),
                                            ));
                                        }
                                        _ => {}
                                    }
                                } else {
                                    self.pending_keys.clear();
                                }
                            }

                            self.update_state_and_render(
                                key_press,
                                crate::key_handler::KeyAction::Continue,
                                crate::command::Command::Noop,
                            )?;
                            break;
                        }
                    }
                }
            } else {
                let poll_dur =
                    std::time::Duration::from_millis(self.state.settings.poll_timeout_ms);
                let notif_tick = self.state.error_manager.notifications().tick(poll_dur);

                if let Some(hold_event) = self.plugin_host.tick_idle() {
                    self.update_lua_state();
                    self.plugin_host.dispatch(&hold_event);
                    self.apply_plugin_mutations();
                }

                if jobs_changed || notif_changed || notif_tick {
                    self.update_and_render()?;
                    self.state.error_manager.notifications_mut().mark_rendered();
                }
            }

            if notif_changed {
                self.last_notification_generation = current_gen;
            }
        }

        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::EditorQuit);
        self.apply_plugin_mutations();

        Ok(())
    }

    /// Handle special actions (mutations happen here, not during input handling)
    pub(super) fn handle_key_actions(&mut self, action: crate::key_handler::KeyAction) {
        match action {
            KeyAction::ExitInsertMode => {
                // Finalize insert recording for dot-repeat
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.finish_insert_recording();
                }
                // Commit insert mode transaction before exiting
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .commit_transaction();
                self.set_mode(Mode::Normal);
            }
            KeyAction::ExitCommandMode => {
                // Close completion dropdown without exiting command mode
                if self.current_mode == Mode::Command
                    && self
                        .state
                        .completion_session
                        .as_ref()
                        .is_some_and(|s| s.dropdown_open)
                {
                    if let Some(session) = &mut self.state.completion_session {
                        session.dropdown_open = false;
                        session.selected = None;
                    }
                    return;
                }

                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
            }
            KeyAction::ExitSearchMode => {
                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
            }
            KeyAction::ToggleDebug => {
                self.state.toggle_debug();
            }
            KeyAction::Resize(cols, rows) => {
                self.render_system.resize(rows as usize, cols as usize);
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::WindowResized { rows, cols });
                self.apply_plugin_mutations();
            }
            KeyAction::SkipAndRender | KeyAction::Continue => {
                // No special action needed
            }
        }
    }
}
