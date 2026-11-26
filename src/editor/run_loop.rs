use super::{pending_grammar, text_object_input, Editor};
use crate::action::{Action, EditorAction};
use crate::command::Command;
use crate::error::{ErrorType, RiftError};
use crate::key::{InputEvent, Key};
use crate::mode::Mode;
use crate::term::TerminalBackend;

/// Milliseconds an ambiguous non-operator key sequence waits for a follow-up
/// key before the shorter action is flushed, mirroring vim's `timeoutlen`.
const KEY_SEQUENCE_TIMEOUT_MS: u64 = 1000;

pub(super) const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_nanos(16_666_667);

impl<T: TerminalBackend> Editor<T> {
    /// # Errors
    ///
    /// Returns an error if the initial render or a tick fails.
    pub fn run(&mut self) -> Result<(), RiftError> {
        // Initial render
        self.update_and_render()?;

        // Main event loop
        while !self.should_quit {
            self.tick()?;
        }

        self.call_plugins(|editor| {
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::EditorQuit);
        });

        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if a tick's input, job, or render work fails.
    pub(crate) fn tick(&mut self) -> Result<(), RiftError> {
        let result = self.tick_inner();
        #[cfg(debug_assertions)]
        {
            let violations = self.assert_invariants(crate::invariants::InvariantTier::Fast);
            crate::invariants::log_violations(&violations);
        }
        result
    }

    fn tick_inner(&mut self) -> Result<(), RiftError> {
        // Pending-key timer only applies while a sequence is in progress.
        if self.pending_keys.is_empty() {
            self.pending_keys_started_at = None;
        }

        self.drain_syntax_sync();
        let mut jobs_changed = self.drain_job_messages();
        self.replay_deferred_keys()?;
        self.drain_syntax_sync();
        self.poll_pending_syntax_reparse();
        self.poll_pending_search_refresh();
        self.poll_git_gutter_diff();

        // Poll LSP messages, after pushing every unsent edit so the server
        // answers against the buffer the user actually sees.
        #[cfg(feature = "lsp")]
        {
            self.poll_lsp();
        }

        let current_gen = self.state.notifications.generation;
        let notif_changed = current_gen != self.last_notification_generation;
        if notif_changed {
            self.refresh_messages_buffer_if_open();
        }

        // Poll for input
        let now = crate::time::Instant::now();
        let mut timeout = std::time::Duration::from_millis(self.state.settings.poll_timeout_ms);
        if let Some(deadline) = self.next_deadline(now) {
            timeout = timeout.min(deadline.saturating_duration_since(now));
        }
        let input_ready = self.term.poll(timeout).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::POLL_FAILED,
                e.to_string(),
            )
        })?;

        // Output can arrive while polling; apply it before either kind of frame.
        jobs_changed |= self.drain_job_messages();
        let timers_fired = self.fire_due_plugin_timers();
        if input_ready {
            self.handle_input_event()?;
        } else {
            self.flush_pending_keys_on_timeout()?;

            let notif_tick = self.state.notifications.tick(timeout);

            if jobs_changed
                || notif_changed
                || notif_tick
                || timers_fired
                || self.render_system.needs_animation_frame()
            {
                self.update_and_render()?;
                self.state.notifications.mark_rendered();
            }
        }

        if self.terminal_frame_delay(crate::time::Instant::now()) == Some(std::time::Duration::ZERO)
        {
            self.update_and_render()?;
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

    fn next_deadline(&self, now: crate::time::Instant) -> Option<crate::time::Instant> {
        let key_sequence_timeout = std::time::Duration::from_millis(KEY_SEQUENCE_TIMEOUT_MS);
        [
            self.terminal_frame_delay(now).map(|delay| now + delay),
            self.plugin_host.next_timer_deadline(),
            self.scheduler.next_deadline(self.active_document_id()),
            self.pending_keys_started_at
                .map(|started_at| started_at + key_sequence_timeout),
            self.state.notifications.next_expiry(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn terminal_frame_delay(&self, now: crate::time::Instant) -> Option<std::time::Duration> {
        if !self.terminal_frame_pending {
            return None;
        }
        Some(
            self.last_frame_at
                .map_or(std::time::Duration::ZERO, |last| {
                    FRAME_INTERVAL.saturating_sub(now.saturating_duration_since(last))
                }),
        )
    }

    fn fire_due_plugin_timers(&mut self) -> bool {
        let now = crate::time::Instant::now();
        if self
            .plugin_host
            .next_timer_deadline()
            .is_none_or(|deadline| deadline > now)
        {
            return false;
        }
        self.call_plugins(|editor| editor.plugin_host.fire_due_timers(now))
    }

    fn drain_job_messages(&mut self) -> bool {
        const MAX_NONTERMINAL_MESSAGES: usize = 10;
        const JOB_DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_millis(2);
        const BUDGET_CHECK_INTERVAL: usize = 256;
        for doc in self.document_manager.documents_iter() {
            self.job_manager
                .register_document_handle(doc.handle(), doc.buffer_kind_id(), None);
        }
        let started = crate::time::Instant::now();
        let mut jobs_changed = false;
        let mut processed_jobs = 0;
        let mut nonterminal_jobs = 0;
        while let Ok(msg) = self.job_manager.receiver().try_recv() {
            let terminal_output =
                matches!(&msg, crate::job_manager::JobMessage::TerminalOutput(..));
            self.handle_job_message(msg);
            processed_jobs += 1;
            if !terminal_output {
                jobs_changed = true;
                nonterminal_jobs += 1;
            }
            // Wakeups are cheap and coalesce into one snapshot. Bound sustained
            // producers without imposing the old ten-message limit on output.
            if nonterminal_jobs >= MAX_NONTERMINAL_MESSAGES
                || (processed_jobs % BUDGET_CHECK_INTERVAL == 0
                    && started.elapsed() >= JOB_DRAIN_BUDGET)
            {
                break;
            }
        }
        jobs_changed
    }

    #[cfg(feature = "lsp")]
    fn poll_lsp(&mut self) {
        self.lsp_flush_pending_changes();
        let lsp_msgs = self.lsp_manager.poll();
        let had_lsp = !lsp_msgs.is_empty();
        for msg in lsp_msgs {
            self.handle_lsp_message(msg);
        }
        if had_lsp {
            let _ = self.update_and_render();
        }
    }

    /// Returns `true` when the input was fully handled by an early exit.
    fn handle_input_event(&mut self) -> Result<bool, RiftError> {
        let Some(event) = self.term.read_event()? else {
            // A filtered event (e.g. Windows key-release) never
            // reaches the coalescing check; flush any owed render.
            self.flush_coalesced_render_if_idle()?;
            return Ok(true);
        };

        let key_press = match event {
            InputEvent::Paste(text) => {
                self.handle_paste(&text)?;
                return Ok(true);
            }
            InputEvent::Resize(cols, rows) => {
                self.handle_terminal_resize(rows, cols)?;
                return Ok(true);
            }
            InputEvent::Key(key) => key,
        };
        if self.is_git_status_refreshing() || !self.deferred_keys.is_empty() {
            self.deferred_keys.push_back(key_press);
            return Ok(true);
        }

        self.handle_key_press(key_press)
    }

    fn handle_terminal_resize(&mut self, rows: u16, cols: u16) -> Result<(), RiftError> {
        self.render_system.resize(rows as usize, cols as usize);
        self.call_plugins(|editor| {
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::WindowResized { rows, cols });
        });
        self.update_and_render()
    }

    fn replay_deferred_keys(&mut self) -> Result<(), RiftError> {
        while !self.is_git_status_refreshing() {
            let Some(key) = self.deferred_keys.pop_front() else {
                return Ok(());
            };
            self.handle_key_press(key)?;
        }

        Ok(())
    }

    fn handle_key_press(&mut self, key_press: Key) -> Result<bool, RiftError> {
        // Update debug info
        self.state.update_keypress(key_press);

        let consumed = self.capture_plugin_float_key(key_press)?
            || self.cancel_operator_on_escape(key_press)?
            || self.handle_terminal_insert_key(key_press)?
            || self.feed_pending_grammar(key_press)?
            || self.start_text_object_grammar(key_press)?
            || self.feed_surround_line(key_press)?
            || self.accumulate_count_digit(key_press)?;
        if consumed {
            return Ok(true);
        }

        self.dispatch_pending_keys(key_press)?;
        Ok(false)
    }

    fn capture_plugin_float_key(&mut self, key_press: Key) -> Result<bool, RiftError> {
        // An open plugin float captures Escape (close) and scroll keys.
        if !self.plugin_host.has_open_float() {
            return Ok(false);
        }
        if key_press.is(Key::ESC) {
            self.plugin_host.close_float();
            self.update_and_render()?;
            return Ok(true);
        }
        let scroll = if key_press.is(Key::char('j')) || key_press.is(Key::DOWN) {
            Some(1)
        } else if key_press.is(Key::char('k')) || key_press.is(Key::UP) {
            Some(-1)
        } else if key_press.is(Key::PAGE_DOWN) {
            Some(10)
        } else if key_press.is(Key::PAGE_UP) {
            Some(-10)
        } else {
            None
        };
        let Some(delta) = scroll else {
            return Ok(false);
        };
        self.plugin_host.scroll_float(delta);
        self.update_and_render()?;
        Ok(true)
    }

    fn cancel_operator_on_escape(&mut self, key_press: Key) -> Result<bool, RiftError> {
        // Escape always cancels OperatorPending, regardless of keymap overrides.
        if key_press.is(Key::ESC) && self.current_mode == Mode::OperatorPending {
            self.set_mode(Mode::Normal);
            self.pending_keys.clear();
            self.pending_count = 0;
            self.pending_grammar = None;
            self.update_and_render()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn handle_terminal_insert_key(&mut self, key_press: Key) -> Result<bool, RiftError> {
        // Handle terminal input
        let is_terminal_insert = if let Some(doc) = self.document_manager.active_document() {
            doc.is_terminal() && self.current_mode == Mode::Insert
        } else {
            false
        };
        if !is_terminal_insert {
            return Ok(false);
        }

        let terminal_match = self.lookup_key_sequence(
            crate::keymap::KeyContext::Terminal,
            std::slice::from_ref(&key_press),
        );
        if let crate::keymap::trie::MatchResult::Exact(action)
        | crate::keymap::trie::MatchResult::Ambiguous(action) = terminal_match
        {
            let action = action.clone();
            self.handle_action(&action);
            self.update_state_and_render(key_press, crate::command::Command::Noop)?;
            return Ok(true);
        }

        let bytes = key_press.to_vt100_bytes();
        if let Some(doc) = self.document_manager.active_document_mut() {
            if let Some(term) = doc.terminal_mut() {
                if !bytes.is_empty() {
                    term.scroll_to_bottom();
                    term.write(&bytes);
                }
            }
        }
        Ok(true)
    }

    fn feed_pending_grammar(&mut self, key_press: Key) -> Result<bool, RiftError> {
        let Some(grammar) = self.pending_grammar.take() else {
            return Ok(false);
        };
        self.advance_pending_grammar(grammar, key_press);
        self.clamp_normal_cursor();
        self.update_state_and_render(key_press, crate::command::Command::Noop)?;
        Ok(true)
    }

    fn start_text_object_grammar(&mut self, key_press: Key) -> Result<bool, RiftError> {
        // 'i'/'a'/'I'/'A' in OperatorPending start the text-object grammar.
        if self.current_mode != Mode::OperatorPending || !self.pending_keys.is_empty() {
            return Ok(false);
        }
        let Some(ch) = key_press.text() else {
            return Ok(false);
        };
        let Some(modifier) = crate::text_objects::modifier_for_key(ch) else {
            return Ok(false);
        };
        self.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
            text_object_input::PendingTextObject::new(modifier),
        ));
        self.update_state_and_render(key_press, crate::command::Command::Noop)?;
        Ok(true)
    }

    fn feed_surround_line(&mut self, key_press: Key) -> Result<bool, RiftError> {
        let is_surround_add = matches!(
            self.pending_operator,
            Some(super::PendingOperator::SurroundAdd(_))
        );
        let is_second_g = self.current_mode == Mode::OperatorPending
            && self.pending_keys.is_empty()
            && key_press.is(Key::char('g'));
        if !is_surround_add || !is_second_g {
            return Ok(false);
        }

        self.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));
        self.update_state_and_render(key_press, crate::command::Command::Noop)?;
        Ok(true)
    }

    fn accumulate_count_digit(&mut self, key_press: Key) -> Result<bool, RiftError> {
        let Some(ch) = key_press.text() else {
            return Ok(false);
        };
        if !ch.is_ascii_digit() || (ch == '0' && self.pending_count == 0) {
            return Ok(false);
        }
        if !self.pending_keys.is_empty()
            || matches!(
                self.current_mode,
                Mode::Insert | Mode::Replace | Mode::Command | Mode::Search | Mode::Rename
            )
        {
            return Ok(false);
        }
        let digit = ch.to_digit(10).unwrap() as usize;
        if self.current_mode == Mode::OperatorPending && self.pending_operator_count == 0 {
            self.pending_operator_count = self.pending_count.max(1);
            self.pending_count = digit;
        } else {
            self.pending_count = self.pending_count.saturating_mul(10).saturating_add(digit);
        }
        // Update UI? (Render pending count)
        self.update_state_and_render(key_press, crate::command::Command::Noop)?;
        Ok(true)
    }

    fn dispatch_pending_keys(&mut self, key_press: Key) -> Result<(), RiftError> {
        use crate::keymap::trie::MatchResult;

        // Push key to pending buffer
        self.pending_keys.push(key_press);

        // Input Processing Loop (allows backtracking)
        loop {
            let context = self.resolve_key_context();

            let match_result = self.lookup_key_sequence(context, &self.pending_keys);

            match match_result {
                MatchResult::Exact(action) => {
                    let action = action.clone();
                    self.pending_keys.clear();

                    let _handled = self.handle_action(&action);

                    if self.current_mode != Mode::OperatorPending
                        && !matches!(
                            self.pending_grammar,
                            Some(pending_grammar::PendingGrammar::ReplaceChar)
                        )
                    {
                        self.pending_count = 0;
                    }

                    self.update_state_and_render(key_press, crate::command::Command::Noop)?;
                    break;
                }
                MatchResult::Ambiguous(action) => {
                    if let Action::Editor(EditorAction::Operator(_)) = action {
                        let action = action.clone();
                        self.pending_keys.clear();
                        self.pending_keys_started_at = None;
                        self.handle_action(&action);
                        // Don't clear pending_count for operators
                        self.update_state_and_render(key_press, crate::command::Command::Noop)?;
                        break;
                    }
                    self.pending_keys_started_at
                        .get_or_insert_with(crate::time::Instant::now);
                    self.update_state_and_render(key_press, crate::command::Command::Noop)?;
                    break;
                }
                MatchResult::Prefix => {
                    self.pending_keys_started_at
                        .get_or_insert_with(crate::time::Instant::now);
                    self.update_state_and_render(key_press, crate::command::Command::Noop)?;
                    break;
                }
                MatchResult::None => {
                    if self.pending_keys.len() > 1 {
                        if self.retry_without_last_key(context) {
                            continue;
                        }
                    } else {
                        self.handle_single_unmatched_key();
                    }

                    self.update_state_and_render(key_press, crate::command::Command::Noop)?;
                    break;
                }
            }
        }
        Ok(())
    }

    /// Returns `true` when the loop should re-dispatch without the last key.
    fn retry_without_last_key(&mut self, context: crate::keymap::KeyContext) -> bool {
        use crate::keymap::trie::MatchResult;

        let last = self.pending_keys.pop().unwrap();
        match self.lookup_key_sequence(context, &self.pending_keys) {
            MatchResult::Ambiguous(action) | MatchResult::Exact(action) => {
                let action = action.clone();
                self.pending_keys.clear();
                let mode_before = self.current_mode;
                self.handle_action(&action);
                self.pending_count = 0;

                self.pending_keys.push(last);
                if self.current_mode == mode_before {
                    // Mode unchanged: normal re-dispatch.
                    return true;
                }
                match self.lookup_key_sequence(context, &[last]) {
                    MatchResult::Exact(a) | MatchResult::Ambiguous(a) => {
                        let a = a.clone();
                        self.pending_keys.clear();
                        self.handle_action(&a);
                        self.pending_count = 0;
                        false
                    }
                    _ => true,
                }
            }
            _ => {
                self.pending_keys.clear();
                self.pending_keys.push(last);
                true
            }
        }
    }

    fn handle_single_unmatched_key(&mut self) {
        if matches!(self.current_mode, Mode::Insert | Mode::Replace) {
            let k = self.pending_keys[0];
            self.pending_keys.clear();
            if let Some(ch) = k.text() {
                self.handle_action(&Action::Editor(EditorAction::InsertChar(ch)));
            }
            // Else ignore?
        } else if self.current_mode == Mode::Command
            || self.current_mode == Mode::Search
            || self.current_mode == Mode::Rename
        {
            let k = self.pending_keys[0];
            self.pending_keys.clear();
            if k == Key::SHIFT_TAB {
                self.handle_mode_management(Command::TabCompletePrev);
            } else if k.is(Key::TAB) {
                self.handle_mode_management(Command::TabComplete);
            } else if let Some(ch) = k.text() {
                self.handle_action(&Action::Editor(EditorAction::InsertChar(ch)));
            }
        } else {
            self.pending_keys.clear();
            // Unrecognized key cancels any pending operator.
            if self.current_mode == Mode::OperatorPending {
                self.set_mode(Mode::Normal);
                self.pending_count = 0;
            }
        }
    }

    fn resolve_key_context(&self) -> crate::keymap::KeyContext {
        use crate::keymap::KeyContext;

        match self.current_mode {
            Mode::Normal => {
                if self.active_doc_is(super::super::document::Document::is_terminal) {
                    KeyContext::TerminalNormal
                } else if let Some(doc) = self.document_manager.active_document() {
                    KeyContext::Buffer(doc.buffer_kind_id())
                } else {
                    KeyContext::Normal
                }
            }
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock => KeyContext::Visual,
            Mode::OperatorPending => KeyContext::OperatorPending,
            Mode::Insert | Mode::Replace => KeyContext::Insert,
            Mode::Command | Mode::Rename => KeyContext::Command,
            Mode::Search => KeyContext::Search,
        }
    }

    fn resolve_parent_key_context(
        &self,
        context: crate::keymap::KeyContext,
    ) -> Option<crate::keymap::KeyContext> {
        match context {
            crate::keymap::KeyContext::Buffer(id) => {
                if let Some(doc) = self.document_manager.active_document() {
                    if doc.buffer_kind_id() == id {
                        return match doc.key_fallback() {
                            crate::document::runtime::KeyFallback::Normal => {
                                Some(crate::keymap::KeyContext::Normal)
                            }
                            crate::document::runtime::KeyFallback::Global => {
                                Some(crate::keymap::KeyContext::Global)
                            }
                            crate::document::runtime::KeyFallback::None => None,
                        };
                    }
                }
                if let Some(desc) = self.buffer_kinds.get_by_id(id) {
                    return match desc.policies.key_fallback {
                        crate::document::runtime::KeyFallback::Normal => {
                            Some(crate::keymap::KeyContext::Normal)
                        }
                        crate::document::runtime::KeyFallback::Global => {
                            Some(crate::keymap::KeyContext::Global)
                        }
                        crate::document::runtime::KeyFallback::None => None,
                    };
                }
                if id == crate::document::runtime::BufferKindId::TERMINAL {
                    None
                } else {
                    Some(crate::keymap::KeyContext::Normal)
                }
            }
            other => crate::keymap::KeyMap::parent_context(other),
        }
    }

    /// Look up a key sequence in the keymap using the active editor parent resolver.
    fn lookup_key_sequence<'a>(
        &'a self,
        context: crate::keymap::KeyContext,
        keys: &[crate::key::Key],
    ) -> crate::keymap::trie::MatchResult<'a> {
        self.keymap
            .lookup_with_parent(context, keys, |ctx| self.resolve_parent_key_context(ctx))
    }

    pub(super) fn handle_paste(&mut self, text: &str) -> Result<(), RiftError> {
        if text.is_empty() {
            return Ok(());
        }

        let is_terminal_insert = self.current_mode == Mode::Insert
            && self.active_doc_is(super::super::document::Document::is_terminal);
        if is_terminal_insert {
            if let Some(doc) = self.document_manager.active_document_mut() {
                if let Some(term) = doc.terminal_mut() {
                    term.scroll_to_bottom();
                    term.write(text.as_bytes());
                }
            }
            return self.update_and_render();
        }

        let text = crate::clipboard::normalize_newlines(text);
        let text = text.as_ref();

        match self.current_mode {
            Mode::Insert => {
                let clip = crate::clipboard::Clip::from_plain(
                    text.chars()
                        .map(crate::character::Character::from)
                        .collect(),
                );
                self.put_into_active(&clip, crate::document::ops::PutAt::Paste);
            }
            Mode::Replace => {
                self.replace_paste(text);
            }
            Mode::Command | Mode::Search | Mode::Rename => {
                for ch in text.chars() {
                    self.handle_mode_management(Command::AppendToCommandLine(ch));
                }
            }
            _ => {
                let clip = crate::clipboard::Clip::from_plain(
                    text.chars()
                        .map(crate::character::Character::from)
                        .collect(),
                );
                self.put_into_active(&clip, crate::document::ops::PutAt::Cursor { before: false });
                self.clamp_normal_cursor();
            }
        }

        self.update_and_render()
    }

    pub(super) fn flush_pending_keys_on_timeout(&mut self) -> Result<(), RiftError> {
        use crate::keymap::trie::MatchResult;

        let Some(started_at) = self.pending_keys_started_at else {
            return Ok(());
        };
        if started_at.elapsed() < std::time::Duration::from_millis(KEY_SEQUENCE_TIMEOUT_MS) {
            return Ok(());
        }

        let context = self.resolve_key_context();
        let action = match self.lookup_key_sequence(context, &self.pending_keys) {
            MatchResult::Exact(action) | MatchResult::Ambiguous(action) => Some(action.clone()),
            _ => None,
        };
        self.pending_keys.clear();
        self.pending_keys_started_at = None;

        if let Some(action) = action {
            self.handle_action(&action);
            self.pending_count = 0;
            self.update_and_render()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "terminal_frame_tests.rs"]
mod terminal_frame_tests;
