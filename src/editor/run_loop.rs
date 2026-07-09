use super::{pending_grammar, text_object_input, Editor};
use crate::action::{Action, EditorAction};
use crate::command::Command;
use crate::error::{ErrorType, RiftError};
use crate::key_handler::KeyAction;
use crate::mode::Mode;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

/// Milliseconds an ambiguous non-operator key sequence waits for a follow-up
/// key before the shorter action is flushed, mirroring vim's `timeoutlen`.
const KEY_SEQUENCE_TIMEOUT_MS: u64 = 1000;

impl<T: TerminalBackend> Editor<T> {
    pub fn run(&mut self) -> Result<(), RiftError> {
        // Initial render
        self.update_and_render()?;

        // Main event loop
        while !self.should_quit {
            self.tick()?;
        }

        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::EditorQuit);
        self.apply_plugin_mutations();

        Ok(())
    }

    /// One iteration of the main loop: drains job/LSP/plugin work, then
    /// processes at most one keypress through the same path `run()` uses.
    pub fn tick(&mut self) -> Result<(), RiftError> {
        // Pending-key timer only applies while a sequence is in progress.
        if self.pending_keys.is_empty() {
            self.pending_keys_started_at = None;
        }

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

        self.poll_pending_syntax_reparse();
        self.poll_pending_search_refresh();

        // Poll LSP messages
        let lsp_msgs = self.lsp_manager.poll();
        let had_lsp = !lsp_msgs.is_empty();
        for msg in lsp_msgs {
            self.handle_lsp_message(msg);
        }
        if had_lsp {
            let _ = self.update_and_render();
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
                None => return Ok(()),
            };

            // Update debug info
            self.state.update_keypress(key_press);

            use crate::key::Key;
            use crate::keymap::MatchResult;

            // Handle special keys immediately
            if let Key::Resize(cols, rows) = key_press {
                self.render_system.resize(rows as usize, cols as usize);
                self.update_and_render()?;
                return Ok(());
            }

            // Escape closes an open plugin float before any other key handling.
            if key_press == Key::Escape && self.plugin_host.has_open_float() {
                self.plugin_host.close_float();
                self.update_and_render()?;
                return Ok(());
            }

            // Escape always cancels OperatorPending, regardless of keymap overrides.
            if key_press == Key::Escape && self.current_mode == Mode::OperatorPending {
                self.set_mode(Mode::Normal);
                self.pending_keys.clear();
                self.pending_count = 0;
                self.pending_grammar = None;
                self.pending_surround_add = None;
                self.update_and_render()?;
                return Ok(());
            }

            // Handle terminal input
            let is_terminal_insert = if let Some(doc) = self.document_manager.active_document() {
                doc.is_terminal() && self.current_mode == Mode::Insert
            } else {
                false
            };

            if is_terminal_insert {
                let terminal_match = self
                    .keymap
                    .lookup(crate::keymap::KeyContext::Terminal, &[key_press]);
                if let crate::keymap::MatchResult::Exact(action)
                | crate::keymap::MatchResult::Ambiguous(action) = terminal_match
                {
                    let action = action.clone();
                    self.handle_action(&action);
                    self.update_state_and_render(
                        key_press,
                        crate::key_handler::KeyAction::Continue,
                        crate::command::Command::Noop,
                    )?;
                    return Ok(());
                }

                if let Some(doc) = self.document_manager.active_document_mut() {
                    if let Some(term) = &mut doc.terminal {
                        let bytes = key_press.to_vt100_bytes();
                        if !bytes.is_empty() {
                            term.scroll_to_bottom();
                            if let Err(e) = term.write(&bytes) {
                                self.state.notify(
                                    crate::notification::NotificationType::Error,
                                    format!("Write failed: {}", e),
                                );
                            }
                        }
                    }
                }
                return Ok(());
            }

            if let Some(grammar) = self.pending_grammar.take() {
                self.advance_pending_grammar(grammar, key_press);
                self.update_state_and_render(
                    key_press,
                    crate::key_handler::KeyAction::Continue,
                    crate::command::Command::Noop,
                )?;
                return Ok(());
            }

            // 'i'/'a'/'I'/'A' in OperatorPending start the text-object grammar.
            if self.current_mode == Mode::OperatorPending && self.pending_keys.is_empty() {
                if let Key::Char(ch) = key_press {
                    if let Some(modifier) = crate::text_objects::modifier_for_key(ch) {
                        self.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
                            text_object_input::PendingTextObject::new(modifier),
                        ));
                        self.update_state_and_render(
                            key_press,
                            crate::key_handler::KeyAction::Continue,
                            crate::command::Command::Noop,
                        )?;
                        return Ok(());
                    }
                }
            }

            // Digits accumulate into a count, but only at the start of a
            // sequence and outside Insert mode (where digits are typed text).
            if self.pending_keys.is_empty()
                && self.current_mode != Mode::Insert
                && self.current_mode != Mode::Replace
                && self.current_mode != Mode::Command
                && self.current_mode != Mode::Search
                && self.current_mode != Mode::Rename
            {
                if let Key::Char(ch) = key_press {
                    if ch.is_ascii_digit() && (ch != '0' || self.pending_count > 0) {
                        let digit = ch.to_digit(10).unwrap() as usize;
                        // First digit of the motion's own count (the 3 in
                        // `2d3w`): stash the operator's count, don't concat.
                        if self.current_mode == Mode::OperatorPending
                            && self.pending_operator_count == 0
                        {
                            self.pending_operator_count = self.pending_count.max(1);
                            self.pending_count = digit;
                        } else {
                            self.pending_count =
                                self.pending_count.saturating_mul(10).saturating_add(digit);
                        }
                        // Update UI? (Render pending count)
                        self.update_state_and_render(
                            key_press,
                            crate::key_handler::KeyAction::Continue,
                            crate::command::Command::Noop,
                        )?;
                        return Ok(());
                    }
                }
            }

            // Push key to pending buffer
            self.pending_keys.push(key_press);

            // Input Processing Loop (allows backtracking)
            loop {
                // 1. Resolve Context
                let context = self.resolve_key_context();

                // 2. Lookup Action in KeyMap
                let match_result = self.keymap.lookup(context, &self.pending_keys);

                match match_result {
                    MatchResult::Exact(action) => {
                        let action = action.clone();
                        self.pending_keys.clear();

                        // 3. Dispatch Action
                        let _handled = self.handle_action(&action);

                        // Don't clear count if we just entered OperatorPending mode or
                        // set pending_replace_char (count is consumed on the next keypress).
                        if self.current_mode != Mode::OperatorPending
                            && !matches!(
                                self.pending_grammar,
                                Some(pending_grammar::PendingGrammar::ReplaceChar)
                            )
                        {
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
                        // Valid prefix and executable action (e.g. 'd'). Operators run
                        // immediately so following digits resolve as the motion's count.
                        if let Action::Editor(EditorAction::Operator(_)) = action {
                            let action = action.clone();
                            self.pending_keys.clear();
                            self.pending_keys_started_at = None;
                            self.handle_action(&action);
                            // Don't clear pending_count for operators
                            self.update_state_and_render(
                                key_press,
                                crate::key_handler::KeyAction::Continue,
                                crate::command::Command::Noop,
                            )?;
                            break;
                        }
                        // For non-operators, wait for more keys (subject to timeout).
                        self.pending_keys_started_at
                            .get_or_insert_with(std::time::Instant::now);
                        self.update_state_and_render(
                            key_press,
                            crate::key_handler::KeyAction::Continue,
                            crate::command::Command::Noop,
                        )?;
                        break;
                    }
                    MatchResult::Prefix => {
                        // Wait for more keys (subject to timeout).
                        self.pending_keys_started_at
                            .get_or_insert_with(std::time::Instant::now);
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
                                    let mode_before = self.current_mode;
                                    self.handle_action(&action);
                                    self.pending_count = 0;

                                    self.pending_keys.push(last);
                                    if self.current_mode == mode_before {
                                        // Mode unchanged: normal re-dispatch.
                                        continue;
                                    }
                                    match self.keymap.lookup(context, &[last]) {
                                        MatchResult::Exact(a) | MatchResult::Ambiguous(a) => {
                                            let a = a.clone();
                                            self.pending_keys.clear();
                                            self.handle_action(&a);
                                            self.pending_count = 0;
                                        }
                                        _ => {
                                            continue;
                                        }
                                    }
                                }
                                _ => {
                                    self.pending_keys.clear();
                                    self.pending_keys.push(last);
                                    continue;
                                }
                            }
                        } else if self.current_mode == Mode::Insert {
                            let k = self.pending_keys[0];
                            self.pending_keys.clear();
                            if let Key::Char(ch) = k {
                                self.handle_action(&Action::Editor(EditorAction::InsertChar(ch)));
                            }
                            // Else ignore?
                        } else if self.current_mode == Mode::Replace {
                            let k = self.pending_keys[0];
                            self.pending_keys.clear();
                            if let Key::Char(ch) = k {
                                if let Some(doc) = self.document_manager.active_document_mut() {
                                    let pos = doc.buffer.cursor();
                                    if pos < doc.buffer.len()
                                        && doc.buffer.char_at(pos)
                                            != Some(crate::character::Character::Newline)
                                    {
                                        let _ = doc.replace_chars(
                                            pos,
                                            1,
                                            &[crate::character::Character::from(ch)],
                                        );
                                    } else {
                                        let _ = doc.insert_char(ch);
                                    }
                                }
                            }
                        } else if self.current_mode == Mode::Command
                            || self.current_mode == Mode::Search
                            || self.current_mode == Mode::Rename
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
                                    self.handle_action(&Action::Editor(EditorAction::InsertChar(
                                        ch,
                                    )));
                                }
                                _ => {}
                            }
                        } else {
                            self.pending_keys.clear();
                            // Unrecognized key cancels any pending operator.
                            if self.current_mode == Mode::OperatorPending {
                                self.set_mode(Mode::Normal);
                                self.pending_count = 0;
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
            self.flush_pending_keys_on_timeout()?;

            let poll_dur = std::time::Duration::from_millis(self.state.settings.poll_timeout_ms);
            let notif_tick = self.state.error_manager.notifications().tick(poll_dur);

            if let Some(hold_event) = self.plugin_host.tick_idle() {
                self.update_lua_state();
                self.plugin_host.dispatch(&hold_event);
                self.apply_plugin_mutations();
            }

            if jobs_changed
                || notif_changed
                || notif_tick
                || self.render_system.needs_animation_frame()
            {
                self.update_and_render()?;
                self.state.error_manager.notifications_mut().mark_rendered();
            }
        }

        if notif_changed {
            self.last_notification_generation = current_gen;
        }

        // In remote mode, :q detaches the client instead of exiting.
        if self.should_quit && self.state.is_remote {
            self.term.request_detach();
            self.should_quit = false;
        }

        Ok(())
    }

    /// Resolve the active `KeyContext` for `self.pending_keys` lookups, based
    /// on the current mode and active document kind.
    pub(super) fn resolve_key_context(&self) -> crate::keymap::KeyContext {
        use crate::keymap::KeyContext;

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
            .map(|d| matches!(d.kind, crate::document::BufferKind::ClipboardEntry { .. }))
            .unwrap_or(false);
        let is_terminal = self
            .document_manager
            .active_document()
            .map(|d| d.is_terminal())
            .unwrap_or(false);
        let is_location_list = self
            .document_manager
            .active_document()
            .map(|d| d.is_location_list())
            .unwrap_or(false);
        let is_regions = self
            .document_manager
            .active_document()
            .map(|d| d.is_regions())
            .unwrap_or(false);
        match self.current_mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::Visual
            | Mode::VisualLine
            | Mode::VisualBlock => {
                if self.current_mode.is_visual() {
                    KeyContext::Visual
                } else if is_directory {
                    KeyContext::FileExplorer
                } else if is_undotree {
                    KeyContext::UndoTree
                } else if is_clipboard {
                    KeyContext::Clipboard
                } else if is_clipboard_entry {
                    KeyContext::ClipboardEntry
                } else if is_terminal {
                    KeyContext::TerminalNormal
                } else if is_location_list {
                    KeyContext::LocationList
                } else if is_regions {
                    KeyContext::Regions
                } else if self.current_mode == Mode::OperatorPending {
                    KeyContext::OperatorPending
                } else {
                    KeyContext::Normal
                }
            }
            Mode::Insert | Mode::Replace => KeyContext::Insert,
            Mode::Command => KeyContext::Command,
            Mode::Search => KeyContext::Search,
            Mode::Rename => KeyContext::Command,
        }
    }

    /// Past timeout, flush a pending non-operator sequence: run the shorter
    /// exact action if one exists, otherwise just clear the pending state.
    pub(super) fn flush_pending_keys_on_timeout(&mut self) -> Result<(), RiftError> {
        use crate::keymap::MatchResult;

        let Some(started_at) = self.pending_keys_started_at else {
            return Ok(());
        };
        if started_at.elapsed() < std::time::Duration::from_millis(KEY_SEQUENCE_TIMEOUT_MS) {
            return Ok(());
        }

        let context = self.resolve_key_context();
        let match_result = self.keymap.lookup(context, &self.pending_keys);
        self.pending_keys.clear();
        self.pending_keys_started_at = None;

        if let MatchResult::Exact(action) | MatchResult::Ambiguous(action) = match_result {
            let action = action.clone();
            self.handle_action(&action);
            self.pending_count = 0;
            self.update_and_render()?;
        }
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
