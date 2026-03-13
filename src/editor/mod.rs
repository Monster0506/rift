//! Editor core
//! Main editor logic that ties everything together

pub mod actions;

#[cfg(test)]
mod terminal_tests;

use crate::action::{Action, EditorAction, Motion};
use crate::command::{Command, Dispatcher};
use crate::command_line::commands::{CommandExecutor, CommandParser};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use crate::document::{Document, DocumentId};
use crate::dot_repeat::{DotRegister, DotRepeat};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::executor::execute_command;
use crate::key_handler::KeyAction;
use crate::keymap::KeyMap;

use crate::mode::Mode;
use crate::render;
use crate::screen_buffer::FrameStats;
use crate::search::SearchDirection;
use crate::split::tree::SplitTree;
use crate::state::{State, UserSettings};
use crate::term::TerminalBackend;
use std::sync::Arc;

/// Main editor struct
pub struct Editor<T: TerminalBackend> {
    /// Terminal backend
    pub term: T,
    pub document_manager: crate::document::DocumentManager,
    pub render_system: crate::render::RenderSystem,
    dispatcher: Dispatcher,
    current_mode: Mode,
    should_quit: bool,
    state: State,
    command_parser: CommandParser,
    settings_registry: SettingsRegistry<UserSettings>,
    document_settings_registry: SettingsRegistry<crate::document::definitions::DocumentOptions>,
    language_loader: Arc<crate::syntax::loader::LanguageLoader>,
    /// Background job manager
    pub job_manager: crate::job_manager::JobManager,
    /// Job ID required to finish before quitting
    pending_quit_job_id: Option<usize>,
    pub keymap: KeyMap,
    pub split_tree: SplitTree,
    // Input state
    pending_keys: Vec<crate::key::Key>,
    pending_count: usize,
    pending_operator: Option<crate::action::OperatorType>,
    dot_repeat: DotRepeat,
    pub panel_layout: Option<PanelLayout>,
    /// Last seen notification generation; used to detect when to refresh open messages buffers.
    last_notification_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    FileExplorer,
    UndoTree,
}

/// Tracks the two windows and documents that make up a live explorer session.
#[derive(Debug, Clone)]
pub struct PanelLayout {
    pub kind: PanelKind,
    pub dir_win_id: crate::split::window::WindowId,
    pub preview_win_id: crate::split::window::WindowId,
    pub dir_doc_id: DocumentId,
    pub preview_doc_id: DocumentId,
    /// For FileExplorer: the document that was showing in `dir_win_id` before the explorer opened.
    pub original_doc_id: DocumentId,
}

impl<T: TerminalBackend> Editor<T> {
    /// Create a new editor instance
    pub fn new(terminal: T) -> Result<Self, RiftError> {
        Self::with_file(terminal, None)
    }

    /// Create a new editor instance with an optional file to load
    pub fn with_file(mut terminal: T, file_path: Option<String>) -> Result<Self, RiftError> {
        // Init language loader
        let grammar_dir = std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|p| p.join(crate::constants::paths::GRAMMARS_DIR))
            })
            .unwrap_or_else(|| std::path::PathBuf::from(crate::constants::paths::GRAMMARS_DIR));

        let language_loader = Arc::new(crate::syntax::loader::LanguageLoader::new(grammar_dir));

        // Create document (either from file or empty)
        // Create document manager
        let mut document_manager = crate::document::DocumentManager::new();

        if let Some(ref path) = file_path {
            document_manager.open_file(Some(path.clone()), false)?;
        } else {
            // Create empty document
            let new_doc = Document::new(1).map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    e.to_string(),
                )
            })?;
            document_manager.add_document(new_doc);
        }

        // Initialize terminal (clears screen, enters raw mode, etc.)
        // We do this AFTER loading the document so we don't mess up the terminal
        // if loading fails
        terminal.init()?;

        // Get terminal size
        let size = terminal.get_size()?;

        // Create render system
        let render_system =
            crate::render::RenderSystem::new(size.rows as usize, size.cols as usize);

        // Create dispatcher
        let dispatcher = Dispatcher::new(Mode::Normal);

        // Create command registry and settings registry
        let settings_registry = create_settings_registry();
        let command_parser = CommandParser::new(settings_registry.clone());

        let mut state = State::new();
        state.set_file_path(file_path.clone());
        if let Some(doc) = document_manager.active_document() {
            state.update_filename(doc.display_name().to_string());
        }

        let initial_doc_id = document_manager
            .active_document_id()
            .expect("No active document after initialization");
        let split_tree = SplitTree::new(initial_doc_id, size.rows as usize, size.cols as usize);

        let mut editor = Self {
            term: terminal,
            render_system,
            document_manager,
            dispatcher,
            current_mode: Mode::Normal,
            should_quit: false,
            state,
            command_parser,
            settings_registry,
            document_settings_registry:
                crate::document::definitions::create_document_settings_registry(),
            language_loader,
            job_manager: crate::job_manager::JobManager::new(),
            pending_quit_job_id: None,
            keymap: KeyMap::new(),
            split_tree,
            pending_keys: Vec::new(),
            pending_count: 0,
            pending_operator: None,
            dot_repeat: DotRepeat::new(),
            panel_layout: None,
            last_notification_generation: 0,
        };

        // Register default keymaps
        crate::keymap::defaults::register_defaults(&mut editor.keymap);

        // Trigger background search cache warming for initial document
        if let Some(doc) = editor.document_manager.active_document() {
            let table = doc.buffer.line_index.table.clone();
            let revision = doc.buffer.revision;
            let job =
                crate::job_manager::jobs::cache_warming::CacheWarmingJob::new(table, revision);

            editor.job_manager.spawn(job);
        }

        // Trigger initial syntax parse
        if let Some(doc) = editor.document_manager.active_document_mut() {
            if let Some(path) = doc.path() {
                let path = path.to_path_buf();
                // Load language
                if let Ok(loaded) = editor.language_loader.load_language_for_file(&path) {
                    // Load and compile query
                    // Load and compile query
                    let highlights_query = editor
                        .language_loader
                        .load_query(&loaded.name, "highlights")
                        .ok()
                        .and_then(|source| tree_sitter::Query::new(&loaded.language, &source).ok())
                        .map(Arc::new);

                    if let Ok(syntax) = crate::syntax::Syntax::new(loaded, highlights_query) {
                        doc.set_syntax(syntax);
                        let doc_id = doc.id;
                        editor.spawn_syntax_parse_job(doc_id);
                    }
                }
            }
        }

        // Trigger initial syntax parse
        if let Some(doc) = editor.document_manager.active_document_mut() {
            if let Some(path) = doc.path() {
                let path = path.to_path_buf();
                // Load language
                if let Ok(loaded) = editor.language_loader.load_language_for_file(&path) {
                    let highlights = editor
                        .language_loader
                        .load_query(&loaded.name, "highlights")
                        .ok()
                        .and_then(|source| tree_sitter::Query::new(&loaded.language, &source).ok())
                        .map(Arc::new);
                    if let Ok(syntax) = crate::syntax::Syntax::new(loaded, highlights) {
                        doc.set_syntax(syntax);
                        let doc_id = doc.id;
                        editor.spawn_syntax_parse_job(doc_id);
                    }
                }
            }
        }

        Ok(editor)
    }

    pub fn active_document_id(&self) -> DocumentId {
        self.split_tree.focused_window().document_id
    }

    pub fn active_document(&mut self) -> &mut Document {
        let doc_id = self.split_tree.focused_window().document_id;
        self.document_manager
            .get_document_mut(doc_id)
            .expect("No active document")
    }

    fn switch_focus(&mut self, target_id: crate::split::window::WindowId) {
        let old_doc_id = self.split_tree.focused_window().document_id;
        if let Some(doc) = self.document_manager.get_document(old_doc_id) {
            let cursor = doc.buffer.cursor();
            self.split_tree.focused_window_mut().cursor_position = cursor;
        }

        self.split_tree.set_focus(target_id);

        let new_doc_id = self.split_tree.focused_window().document_id;
        let new_cursor = self.split_tree.focused_window().cursor_position;
        let _ = self.document_manager.switch_to_document(new_doc_id);
        if let Some(doc) = self.document_manager.get_document_mut(new_doc_id) {
            let _ = doc.buffer.set_cursor(new_cursor);
        }

        self.sync_state_with_active_document();
    }

    fn save_current_view_state(&mut self) {
        let (top_line, left_col) = self.render_system.viewport.get_scroll();
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.save_view_state(top_line, left_col);
        }
    }

    /// Restore view state from the active document after switching
    fn restore_view_state(&mut self) {
        if let Some(doc) = self.document_manager.active_document() {
            let view_state = doc.get_view_state();
            self.render_system
                .viewport
                .set_scroll(view_state.top_line, view_state.left_col);
        }
    }

    /// Sync editor state with the active document
    fn sync_state_with_active_document(&mut self) {
        let (display_name, file_path, is_dirty, line_ending) = {
            let doc = self.active_document();
            (
                doc.display_name().to_string(),
                doc.path().map(|p| p.to_string_lossy().to_string()),
                doc.is_dirty(),
                doc.options.line_ending,
            )
        };

        self.state.update_filename(display_name);
        self.state.set_file_path(file_path);
        self.state.update_dirty(is_dirty);

        let total_lines = self.active_document().buffer.get_total_lines();
        let buffer_size = self.active_document().buffer.len();
        self.state
            .update_buffer_stats(total_lines, buffer_size, line_ending);

        // Update gutter width
        if self.state.settings.show_line_numbers {
            let digits = if total_lines > 0 {
                (total_lines as f64).log10().floor() as usize + 1
            } else {
                1
            };
            // 1 space padding on each side
            self.state.gutter_width = digits + 2;
        } else {
            self.state.gutter_width = 0;
        }
    }

    /// Force a full redraw of the editor
    fn force_full_redraw(&mut self) -> Result<(), RiftError> {
        self.render_system.viewport.mark_needs_full_redraw();
        self.update_and_render().map_err(|e| {
            RiftError::new(ErrorType::Io, crate::constants::errors::RENDER_FAILED, e.to_string())
        })
    }

    /// Remove a document by ID with strict tab semantics
    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        self.document_manager.remove_document(id)?;
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.sync_state_with_active_document();
        Ok(())
    }

    /// Open a file in a new document or reload the current one
    ///
    /// If file_path is Some, it opens that file (or creates a new document for
    /// it if not found). If file_path is None, it reloads the current active
    /// document.
    pub fn open_file(&mut self, file_path: Option<String>, force: bool) -> Result<(), RiftError> {
        // Logic split: if path provided, check if open. If not open, async load.
        // If path provided and open, switch to it (via manager).
        // If no path, reload active (async).

        if let Some(path_str) = file_path {
            let path = std::path::PathBuf::from(&path_str);
            if self
                .document_manager
                .find_open_document_index(&path)
                .is_some()
            {
                // Save current document's view state before switching
                self.save_current_view_state();
                // Already open, use manager to switch
                self.document_manager.open_file(Some(path_str), force)?;
                // Restore the switched-to document's view state
                self.restore_view_state();
            } else {
                // Save current document's view state before switching
                self.save_current_view_state();
                // Not open, create placeholder and async load
                let doc_id = self.document_manager.create_placeholder(&path_str)?;
                let job = crate::job_manager::jobs::file_operations::FileLoadJob::new(
                    doc_id,
                    path.clone(),
                );
                self.job_manager.spawn(job);
            }
        } else {
            // Reload current
            if let Some(doc) = self.document_manager.active_document() {
                if let Some(path) = doc.path() {
                    if !force && doc.is_dirty() {
                        return Err(RiftError {
                            severity: ErrorSeverity::Warning,
                            kind: ErrorType::Execution,
                            code: crate::constants::errors::UNSAVED_CHANGES.to_string(),
                            message: crate::constants::errors::MSG_UNSAVED_CHANGES.to_string(),
                        });
                    }
                    let job = crate::job_manager::jobs::file_operations::FileLoadJob::new(
                        doc.id,
                        path.to_path_buf(),
                    );
                    self.job_manager.spawn(job);
                } else {
                    return Err(RiftError::new(
                        ErrorType::Execution,
                        crate::constants::errors::NO_PATH,
                        "No file name",
                    ));
                }
            } else {
                return Err(RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    "No active document",
                ));
            }
        }

        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.sync_state_with_active_document();
        Ok(())
    }

    /// Open a new terminal buffer
    pub fn open_terminal(&mut self, shell_cmd: Option<String>) -> Result<(), RiftError> {
        let size = self
            .term
            .get_size()
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERM_SIZE", e))?;

        let id = self.document_manager.next_id();
        let terminal_rows = size.rows.saturating_sub(1).max(1); // exclude status bar row
        let (doc, rx) =
            crate::document::Document::new_terminal(id, terminal_rows, size.cols, shell_cmd)?;

        self.document_manager.add_document(doc);

        self.document_manager.switch_to_document(id)?;
        self.split_tree.focused_window_mut().document_id = id;

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();

        let job = crate::job_manager::jobs::terminal_job::TerminalInputJob {
            document_id: id,
            rx,
        };
        self.job_manager.spawn(job);

        Ok(())
    }

    /// Perform a search in the document
    fn perform_search(
        &mut self,
        query: &str,
        direction: SearchDirection,
        skip_current: bool,
    ) -> bool {
        // Find all matches first to populate state for highlighting
        self.update_search_highlights();
        let _ = self.force_full_redraw();

        let doc = self
            .document_manager
            .active_document_mut()
            .expect("No active document");
        match doc.perform_search(query, direction, skip_current) {
            Ok((Some(m), _stats)) => {
                // Move cursor to start of match
                let _ = doc.buffer.set_cursor(m.range.start);
                true
            }
            Ok((None, _stats)) => {
                // No match found - don't move cursor, no notification needed
                // The user can see from the cursor position that nothing was found
                false
            }
            Err(e) => {
                // Actual search error (e.g., regex compilation failure)
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("Search error: {}", e),
                );
                false
            }
        }
    }

    /// Run the editor main loop
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
                        let is_directory = self.document_manager.active_document()
                            .map(|d| d.is_directory())
                            .unwrap_or(false);
                        let is_undotree = self.document_manager.active_document()
                            .map(|d| d.is_undotree())
                            .unwrap_or(false);
                        match self.current_mode {
                            Mode::Normal | Mode::OperatorPending => {
                                if is_directory {
                                    KeyContext::FileExplorer
                                } else if is_undotree {
                                    KeyContext::UndoTree
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
                if jobs_changed || notif_changed || notif_tick {
                    self.update_and_render()?;
                    self.state.error_manager.notifications_mut().mark_rendered();
                }
            }

            if notif_changed {
                self.last_notification_generation = current_gen;
            }
        }

        Ok(())
    }

    /// Handle special actions (mutations happen here, not during input handling)
    fn handle_key_actions(&mut self, action: crate::key_handler::KeyAction) {
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
            }
            KeyAction::SkipAndRender | KeyAction::Continue => {
                // No special action needed
            }
        }
    }

    /// Handle global actions (from KeyMap)
    fn handle_action(&mut self, action: &crate::action::Action) -> bool {
        use crate::action::{Action, EditorAction};

        let editor_action = match action {
            Action::Editor(act) => act,
            Action::Buffer(id) => {
                // messages:open works globally regardless of active buffer kind
                if id == "messages:open" {
                    self.open_messages(false);
                    return true;
                }
                use crate::document::BufferKind;
                let kind = self.active_document().kind.clone();
                match kind {
                    BufferKind::Directory { .. } => self.handle_directory_buffer_action(id),
                    BufferKind::UndoTree { .. } => self.handle_undotree_buffer_action(id),
                    BufferKind::Messages { .. } => self.handle_messages_buffer_action(id),
                    _ => {}
                }
                return true;
            }
            Action::Noop => return false,
        };

        match editor_action {
            EditorAction::Move(motion) => {
                if self.current_mode == Mode::OperatorPending {
                    if let Some(op) = self.pending_operator {
                        return self.execute_operator(op, *motion);
                    }
                }
                let count = if self.pending_count > 0 {
                    self.pending_count
                } else {
                    1
                };
                let command = crate::command::Command::Move(*motion, count);
                // Execute immediately
                self.handle_mode_management(command);
                let consumed = self.execute_buffer_command(command);
                self.update_explorer_preview();
                self.update_undotree_preview();
                consumed
            }
            EditorAction::EnterInsertMode => {
                self.handle_mode_management(crate::command::Command::EnterInsertMode);
                true
            }
            EditorAction::EnterInsertModeAfter => {
                self.handle_mode_management(crate::command::Command::EnterInsertModeAfter);
                true
            }
            EditorAction::EnterInsertModeAtLineStart => {
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineStart);
                true
            }
            EditorAction::EnterInsertModeAtLineEnd => {
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineEnd);
                true
            }
            EditorAction::EnterCommandMode => {
                self.handle_mode_management(crate::command::Command::EnterCommandMode);
                true
            }
            EditorAction::EnterSearchMode => {
                self.handle_mode_management(crate::command::Command::EnterSearchMode);
                true
            }
            EditorAction::EnterNormalMode => {
                if self.current_mode == Mode::Insert {
                    // Finalize insert recording for dot-repeat
                    if !self.dot_repeat.is_replaying() {
                        self.dot_repeat.finish_insert_recording();
                    }
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                }
                // Reset history navigation when exiting command/search mode
                self.state.command_history.reset_navigation();
                self.state.search_history.reset_navigation();
                self.set_mode(Mode::Normal);
                self.state.clear_command_line();
                self.state.search_matches.clear();
                self.pending_operator = None;
                self.pending_keys.clear();
                self.pending_count = 0;
                true
            }
            EditorAction::Undo => self.execute_buffer_command(crate::command::Command::Undo),
            EditorAction::Redo => self.execute_buffer_command(crate::command::Command::Redo),
            EditorAction::Quit => {
                self.do_quit(false);
                true
            }
            EditorAction::Submit => {
                if self.current_mode == Mode::Command {
                    self.handle_mode_management(crate::command::Command::ExecuteCommandLine);
                    true
                } else if self.current_mode == Mode::Search {
                    self.handle_mode_management(crate::command::Command::ExecuteSearch);
                    true
                } else {
                    false
                }
            }
            EditorAction::Delete(motion) => {
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search {
                    // Assuming left motion is backspace
                    if *motion == crate::action::Motion::Left {
                        self.handle_mode_management(crate::command::Command::DeleteFromCommandLine);
                        return true;
                    }
                    if *motion == crate::action::Motion::PreviousWord {
                        self.state.delete_word_back_command_line();
                        return true;
                    }
                }
                let command = crate::command::Command::Delete(*motion, 1);
                let result = self.execute_buffer_command(command);
                if result && self.current_mode == Mode::Normal && !self.dot_repeat.is_replaying() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            EditorAction::DeleteLine => {
                let command = crate::command::Command::DeleteLine;
                let result = self.execute_buffer_command(command);
                if result && self.current_mode == Mode::Normal && !self.dot_repeat.is_replaying() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            EditorAction::InsertChar(c) => {
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search {
                    self.handle_mode_management(crate::command::Command::AppendToCommandLine(*c));
                    return true;
                }
                let command = crate::command::Command::InsertChar(*c);
                self.execute_buffer_command(command)
            }
            EditorAction::BufferNext => {
                self.do_buffer_next();
                true
            }
            EditorAction::BufferPrevious => {
                self.do_buffer_prev();
                true
            }
            EditorAction::ToggleDebug => {
                self.state.toggle_debug();
                true
            }
            EditorAction::Redraw => {
                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
                true
            }

            EditorAction::Save => {
                self.do_save();
                true
            }
            EditorAction::SaveAndQuit => {
                self.do_save_and_quit();
                true
            }
            EditorAction::OpenExplorer => {
                let path = self
                    .document_manager
                    .active_document()
                    .and_then(|d| {
                        if let crate::document::BufferKind::Directory { path, .. } = &d.kind {
                            return Some(path.clone());
                        }
                        d.path().map(|p| {
                            if p.is_dir() {
                                p.to_path_buf()
                            } else {
                                p.parent().unwrap_or(p).to_path_buf()
                            }
                        })
                    })
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                self.open_explorer(path);
                true
            }
            EditorAction::OpenUndoTree => {
                self.open_undotree_split();
                true
            }
            EditorAction::OpenMessages => {
                self.open_messages(false);
                true
            }
            EditorAction::ShowBufferList => {
                self.do_show_buffer_list();
                true
            }
            EditorAction::ClearHighlights => {
                self.state.search_matches.clear();
                self.force_full_redraw().ok();
                true
            }
            EditorAction::ClearNotifications => {
                self.state.error_manager.notifications_mut().clear_all();
                self.state.clear_command_line();
                self.force_full_redraw().ok();
                true
            }
            EditorAction::ClearLastNotification => {
                self.state.error_manager.notifications_mut().clear_last();
                self.state.clear_command_line();
                self.force_full_redraw().ok();
                true
            }
            EditorAction::Checkpoint => {
                if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.checkpoint();
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        "Checkpoint created".to_string(),
                    );
                }
                true
            }
            EditorAction::RunCommand(cmd_str) => {
                let cmd_str = cmd_str.clone();
                self.execute_command_line(cmd_str);
                true
            }
            EditorAction::Operator(op) => {
                if self.current_mode == Mode::OperatorPending {
                    if let Some(pending) = self.pending_operator {
                        if pending == *op {
                            return self.execute_operator_linewise(pending);
                        }
                    }
                }
                self.pending_operator = Some(*op);
                self.set_mode(Mode::OperatorPending);
                true
            }
            EditorAction::Command(cmd) => {
                let command = *cmd.clone();
                self.handle_mode_management(command);
                self.execute_buffer_command(command)
            }
            EditorAction::HistoryUp => {
                let dropdown_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                if dropdown_open {
                    self.handle_mode_management(Command::TabCompletePrev);
                } else {
                    self.navigate_history_up();
                }
                true
            }
            EditorAction::HistoryDown => {
                let dropdown_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                if dropdown_open {
                    self.handle_mode_management(Command::TabComplete);
                } else {
                    self.navigate_history_down();
                }
                true
            }
            EditorAction::DotRepeat => self.execute_dot_repeat(),
            EditorAction::QuitForce => {
                self.should_quit = true;
                true
            }
            EditorAction::OpenFile { path, force } => {
                let path = path.clone();
                let force = *force;
                if let Err(e) = self.open_file(path, force) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    if let Err(e) = self.force_full_redraw() {
                        self.state.handle_error(e);
                    }
                }
                true
            }
            EditorAction::OpenDirectory(path) => {
                let path = path.clone();
                self.open_explorer(path);
                true
            }
            EditorAction::OpenTerminal(cmd) => {
                let cmd = cmd.clone();
                if let Err(e) = self.open_terminal(cmd) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    self.set_mode(Mode::Insert);
                }
                true
            }
            EditorAction::SplitWindow { direction, subcommand } => {
                let direction = *direction;
                let subcommand = subcommand.clone();
                self.do_split_window(direction, subcommand);
                true
            }
            EditorAction::UndoCount(count) => {
                self.do_undo(*count);
                true
            }
            EditorAction::RedoCount(count) => {
                self.do_redo(*count);
                true
            }
            EditorAction::UndoGoto(seq) => {
                self.do_undo_goto(*seq);
                true
            }
            EditorAction::NotificationClearAll => {
                self.state.error_manager.notifications_mut().clear_all();
                self.state.clear_command_line();
                true
            }
        }
    }

    fn handle_directory_buffer_action(&mut self, id: &str) {
        match id {
            "explorer:select"  => self.handle_explorer_select(),
            "explorer:parent"  => self.handle_explorer_parent(),
            "explorer:close"   => self.close_split_panel(),
            "explorer:refresh" => self.handle_explorer_refresh(),
            _ => {}
        }
    }

    fn handle_undotree_buffer_action(&mut self, id: &str) {
        match id {
            "undotree:select"  => self.handle_undotree_select(),
            "undotree:close"   => self.close_split_panel(),
            "undotree:refresh" => self.handle_undotree_refresh(),
            _ => {}
        }
    }

    fn handle_messages_buffer_action(&mut self, id: &str) {
        match id {
            "messages:refresh" => self.refresh_messages_buffer_if_open(),
            _ => {}
        }
    }

    /// Repopulate any open messages buffer with the current notification log.
    /// Preserves cursor position for background refreshes.
    fn refresh_messages_buffer_if_open(&mut self) {
        let doc_id = match self.document_manager.find_messages_doc_id() {
            Some(id) => id,
            None => return,
        };

        let log = self
            .state
            .error_manager
            .notifications()
            .message_log()
            .to_vec();

        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            let cursor = doc.buffer.cursor();
            doc.populate_messages_buffer(&log);
            // Preserve cursor position on background refresh
            let len = doc.buffer.len();
            let _ = doc.buffer.set_cursor(cursor.min(len.saturating_sub(1)));
        }

        // Only re-render if the messages buffer is currently visible
        if self.active_document_id() == doc_id {
            let _ = self.update_and_render();
        }
    }

    fn do_save(&mut self) {
        use crate::document::BufferKind;
        if let Some(doc) = self.document_manager.active_document() {
            match &doc.kind {
                BufferKind::File => {
                    let doc = self.document_manager.active_document_mut().unwrap();
                    if let Some(path) = doc.path() {
                        let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                            doc.id,
                            doc.buffer.line_index.table.clone(),
                            path.to_path_buf(),
                            doc.options.line_ending,
                            doc.history.current_seq(),
                        );
                        self.job_manager.spawn(job);
                    } else {
                        self.state.handle_error(RiftError::new(
                            ErrorType::Io,
                            "NO_FILENAME",
                            "No file name",
                        ));
                    }
                }
                BufferKind::Directory { .. } => {
                    self.apply_directory_diff();
                }
                BufferKind::UndoTree { .. } | BufferKind::Terminal | BufferKind::Messages { .. } => {
                    self.state.handle_error(RiftError::new(
                        ErrorType::Io,
                        "CANT_SAVE",
                        format!(
                            "{} buffer cannot be saved",
                            self.document_manager
                                .active_document()
                                .map(|d| d.display_name().into_owned())
                                .unwrap_or_default()
                        ),
                    ));
                }
            }
        }
        self.state.clear_command_line();
    }

    fn do_save_and_quit(&mut self) {
        let res = {
            let doc = self.document_manager.active_document().unwrap();
            if doc.has_path() {
                Ok((
                    doc.id,
                    doc.path().unwrap().to_path_buf(),
                    doc.buffer.line_index.table.clone(),
                    doc.options.line_ending,
                    doc.history.current_seq(),
                ))
            } else if let Some(path) = &self.state.file_path {
                Ok((
                    doc.id,
                    std::path::PathBuf::from(path),
                    doc.buffer.line_index.table.clone(),
                    doc.options.line_ending,
                    doc.history.current_seq(),
                ))
            } else {
                Err(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"))
            }
        };
        match res {
            Ok((doc_id, path, table, line_ending, saved_seq)) => {
                let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                    doc_id,
                    table,
                    path.clone(),
                    line_ending,
                    saved_seq,
                );
                let id = self.job_manager.spawn(job);
                self.pending_quit_job_id = Some(id);
                self.state.notify(
                    crate::notification::NotificationType::Info,
                    format!("Saving {} and quitting...", path.display()),
                );
            }
            Err(e) => self.state.handle_error(e),
        }
        self.state.clear_command_line();
    }

    fn do_quit(&mut self, force: bool) {
        let in_explorer = self.panel_layout.as_ref().map(|l| {
            let fid = self.split_tree.focused_window_id();
            fid == l.dir_win_id || fid == l.preview_win_id
        }).unwrap_or(false);
        if in_explorer {
            self.close_split_panel();
            return;
        }
        if self.split_tree.window_count() > 1 {
            let focused_id = self.split_tree.focused_window_id();
            self.split_tree.close_window(focused_id);
            let new_doc_id = self.split_tree.focused_window().document_id;
            let new_cursor = self.split_tree.focused_window().cursor_position;
            let _ = self.document_manager.switch_to_document(new_doc_id);
            if let Some(doc) = self.document_manager.get_document_mut(new_doc_id) {
                let _ = doc.buffer.set_cursor(new_cursor);
            }
            self.sync_state_with_active_document();
            if let Err(e) = self.force_full_redraw() {
                self.state.handle_error(e);
            }
        } else if self.document_manager.tab_count() <= 1 {
            // Last buffer: quit the editor
            if !force {
                let doc_id = self.active_document_id();
                if let Some(doc) = self.document_manager.get_document(doc_id) {
                    if doc.is_dirty() && !doc.is_special() {
                        self.state.handle_error(RiftError::warning(
                            ErrorType::Execution,
                            crate::constants::errors::UNSAVED_CHANGES,
                            crate::constants::errors::MSG_UNSAVED_CHANGES,
                        ));
                        return;
                    }
                }
            }
            self.should_quit = true;
        } else {
            let doc_id = self.active_document_id();
            let result = if force {
                self.document_manager.remove_document_force(doc_id)
            } else {
                self.document_manager.remove_document(doc_id)
            };
            match result {
                Err(e) => self.state.handle_error(e),
                Ok(()) => {
                    if let Some(new_doc_id) = self.document_manager.active_document_id() {
                        self.split_tree.focused_window_mut().document_id = new_doc_id;
                    }
                    self.sync_state_with_active_document();
                    if let Err(e) = self.force_full_redraw() {
                        self.state.handle_error(e);
                    }
                }
            }
        }
    }

    fn do_buffer_next(&mut self) {
        self.save_current_view_state();
        self.document_manager.switch_next_tab();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.restore_view_state();
        self.sync_state_with_active_document();
        self.state.clear_command_line();
        if let Err(e) = self.force_full_redraw() {
            self.state.handle_error(e);
        }
    }

    fn do_buffer_prev(&mut self) {
        self.save_current_view_state();
        self.document_manager.switch_prev_tab();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.split_tree.focused_window_mut().document_id = doc_id;
        }
        self.restore_view_state();
        self.sync_state_with_active_document();
        self.state.clear_command_line();
        if let Err(e) = self.force_full_redraw() {
            self.state.handle_error(e);
        }
    }

    fn do_show_buffer_list(&mut self) {
        let buffers = self.document_manager.get_buffer_list();
        let mut message = String::new();
        for info in buffers {
            let dirty = if info.is_dirty { "+" } else { " " };
            let read_only = if info.is_read_only { "R" } else { " " };
            let current = if info.is_current { "%" } else { " " };
            let special = if info.is_special { "~" } else { " " };
            if !message.is_empty() {
                message.push('\n');
            }
            message.push_str(&format!(
                "[{}] {}: {}{}{}{}",
                info.index + 1,
                info.name,
                current,
                dirty,
                read_only,
                special,
            ));
        }
        self.state.notify(crate::notification::NotificationType::Info, message);
        self.state.clear_command_line();
    }

    fn do_notification_clear(&mut self, all: bool) {
        if all {
            self.state.error_manager.notifications_mut().clear_all();
        } else {
            self.state.error_manager.notifications_mut().clear_last();
        }
        self.state.clear_command_line();
    }

    fn do_split_window(
        &mut self,
        direction: crate::split::tree::SplitDirection,
        subcommand: crate::command_line::commands::SplitSubcommand,
    ) {
        use crate::command_line::commands::SplitSubcommand;
        match subcommand {
            SplitSubcommand::Current => {
                let doc_id = self.active_document_id();
                let focused_id = self.split_tree.focused_window_id();
                let size = self.term.get_size().unwrap();
                let new_id = self.split_tree.split(
                    direction,
                    focused_id,
                    doc_id,
                    size.rows as usize,
                    size.cols as usize,
                );
                self.switch_focus(new_id);
            }
            SplitSubcommand::File(path) => {
                let path_buf = std::path::PathBuf::from(&path);
                if !path_buf.exists() {
                    self.state.handle_error(crate::error::RiftError::new(
                        crate::error::ErrorType::Io,
                        "FILE_NOT_FOUND",
                        format!("No such file: {path}"),
                    ));
                    return;
                }
                let doc_id = if let Some(id) =
                    self.document_manager.find_open_document_id(&path_buf)
                {
                    id
                } else {
                    match self.document_manager.create_placeholder(&path) {
                        Ok(id) => {
                            let job =
                                crate::job_manager::jobs::file_operations::FileLoadJob::new(
                                    id, path_buf,
                                );
                            self.job_manager.spawn(job);
                            id
                        }
                        Err(e) => {
                            self.state.handle_error(e);
                            return;
                        }
                    }
                };
                let focused_id = self.split_tree.focused_window_id();
                let size = self.term.get_size().unwrap();
                let new_id = self.split_tree.split(
                    direction,
                    focused_id,
                    doc_id,
                    size.rows as usize,
                    size.cols as usize,
                );
                self.switch_focus(new_id);
            }
            SplitSubcommand::Navigate(dir) => {
                let size = self.term.get_size().unwrap();
                let layouts = self
                    .split_tree
                    .compute_layout(size.rows as usize, size.cols as usize);
                if let Some(target_id) = self.split_tree.navigate(dir, &layouts) {
                    self.switch_focus(target_id);
                }
            }
            SplitSubcommand::Resize(delta) => {
                let size = self.term.get_size().unwrap();
                let layouts = self
                    .split_tree
                    .compute_layout(size.rows as usize, size.cols as usize);
                let delta_ratio = (delta as f64) / (size.cols as f64);
                self.split_tree.resize_focused(direction, delta_ratio, &layouts);
            }
        }
        self.state.clear_command_line();
    }

    fn do_undo(&mut self, count: Option<u64>) {
        let doc = self.document_manager.active_document_mut().unwrap();
        let count = count.unwrap_or(1) as usize;
        let mut undone = 0;
        for _ in 0..count {
            if doc.undo() {
                undone += 1;
            } else {
                break;
            }
        }
        if undone == 0 {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "Already at oldest change".to_string(),
            );
        }
        self.state.clear_command_line();
        self.update_search_highlights();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.spawn_syntax_parse_job(doc_id);
        }
    }

    fn do_redo(&mut self, count: Option<u64>) {
        let doc = self.document_manager.active_document_mut().unwrap();
        let count = count.unwrap_or(1) as usize;
        let mut redone = 0;
        for _ in 0..count {
            if doc.redo() {
                redone += 1;
            } else {
                break;
            }
        }
        if redone == 0 {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "Already at newest change".to_string(),
            );
        }
        self.state.clear_command_line();
        self.update_search_highlights();
        if let Some(doc_id) = self.document_manager.active_document_id() {
            self.spawn_syntax_parse_job(doc_id);
        }
    }

    fn do_undo_goto(&mut self, seq: u64) {
        let doc = self.document_manager.active_document_mut().unwrap();
        match doc.goto_seq(seq) {
            Ok(()) => {
                self.state.notify(
                    crate::notification::NotificationType::Info,
                    format!("Jumped to edit #{}", seq),
                );
                if let Some(doc_id) = self.document_manager.active_document_id() {
                    self.spawn_syntax_parse_job(doc_id);
                }
            }
            Err(e) => {
                self.state.handle_error(RiftError::new(
                    ErrorType::Execution,
                    "UNDO_ERROR",
                    e.to_string(),
                ));
            }
        }
        self.state.clear_command_line();
        self.update_search_highlights();
    }

    /// Navigate to previous (older) history entry
    fn navigate_history_up(&mut self) {
        let current_line = self.state.command_line.clone();
        let history = if self.current_mode == Mode::Command {
            &mut self.state.command_history
        } else if self.current_mode == Mode::Search {
            &mut self.state.search_history
        } else {
            return;
        };

        history.start_navigation(current_line);
        if let Some(entry) = history.prev_match() {
            let entry = entry.to_string();
            self.state.command_line = entry.clone();
            // Clamp cursor to new line length
            self.state.command_line_cursor = self.state.command_line_cursor.min(entry.len());
        }
    }

    /// Navigate to next (newer) history entry
    fn navigate_history_down(&mut self) {
        let history = if self.current_mode == Mode::Command {
            &mut self.state.command_history
        } else if self.current_mode == Mode::Search {
            &mut self.state.search_history
        } else {
            return;
        };

        if let Some(entry) = history.next_match() {
            let entry = entry.to_string();
            self.state.command_line = entry.clone();
            // Clamp cursor to new line length
            self.state.command_line_cursor = self.state.command_line_cursor.min(entry.len());
        }
    }

    /// Helper to execute buffer commands
    fn execute_buffer_command(&mut self, command: crate::command::Command) -> bool {
        crate::perf_span!("execute_buffer_command", crate::perf::PerfFields::default());
        let current_mode = self.current_mode;
        // Simplified check for now
        if current_mode == Mode::Normal || current_mode == Mode::Insert {
            let viewport_height = self.render_system.viewport.visible_rows();

            let display_map = if self.state.settings.soft_wrap {
                let doc = self.document_manager.active_document().unwrap();
                let gutter_width = if self.state.settings.show_line_numbers { self.state.gutter_width } else { 0 };
                let content_width = self.render_system.viewport.visible_cols().saturating_sub(gutter_width).max(1);
                let wrap_width = self.state.settings.wrap_width.unwrap_or(content_width);
                Some(crate::wrap::DisplayMap::build(&doc.buffer, wrap_width, doc.options.tab_width))
            } else {
                None
            };

            let doc = self.document_manager.active_document_mut().unwrap();
            let expand_tabs = doc.options.expand_tabs;
            let tab_width = doc.options.tab_width;
            let is_mutating = command.is_mutating();

            let _ = execute_command(
                command,
                doc,
                expand_tabs,
                tab_width,
                viewport_height,
                self.state.last_search_query.as_deref(),
                display_map.as_ref(),
            );

            // Record insert-mode mutations for dot-repeat
            if is_mutating && self.current_mode == Mode::Insert && !self.dot_repeat.is_replaying() {
                self.dot_repeat.record_insert_command(command);
            }

            // Synchronous incremental parse for mutating commands
            // Tree-sitter incremental parsing is fast (~1ms for small edits)
            if is_mutating {
                self.do_incremental_syntax_parse();
            }

            return true;
        }
        false
    }

    /// Perform synchronous incremental syntax parse for the document.
    /// This is fast because tree-sitter reuses unchanged subtrees from the old tree.
    fn do_incremental_syntax_parse(&mut self) {
        if let Some(doc) = self.document_manager.active_document_mut() {
            if doc.syntax.is_none() {
                return;
            }

            // Get source bytes for parsing
            let source = doc.buffer.to_logical_bytes();

            if let Some(syntax) = &mut doc.syntax {
                syntax.incremental_parse(&source);
            }
        }
    }

    /// Switch between modes based on command, and handle commandline input
    fn handle_mode_management(&mut self, command: crate::command::Command) {
        match command {
            Command::EnterInsertMode => {
                // Start transaction for grouping insert mode edits
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAfter => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.buffer.move_right();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAtLineStart => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.buffer.move_to_line_start();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAtLineEnd => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.buffer.move_to_line_end();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
            }
            Command::Change(_, _) | Command::ChangeLine => {
                let (expand_tabs, tab_width) = {
                    let doc = self.document_manager.active_document().unwrap();
                    (doc.options.expand_tabs, doc.options.tab_width)
                };
                let viewport_height = self.render_system.viewport.visible_rows();
                let display_map = if self.state.settings.soft_wrap {
                    let doc = self.document_manager.active_document().unwrap();
                    let gutter_width = if self.state.settings.show_line_numbers { self.state.gutter_width } else { 0 };
                    let content_width = self.render_system.viewport.visible_cols().saturating_sub(gutter_width).max(1);
                    let wrap_width = self.state.settings.wrap_width.unwrap_or(content_width);
                    Some(crate::wrap::DisplayMap::build(&doc.buffer, wrap_width, tab_width))
                } else {
                    None
                };
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                let _ = execute_command(
                    command,
                    self.document_manager.active_document_mut().unwrap(),
                    expand_tabs,
                    tab_width,
                    viewport_height,
                    self.state.last_search_query.as_deref(),
                    display_map.as_ref(),
                );
                self.set_mode(Mode::Insert);
            }
            Command::EnterCommandMode => {
                self.state.completion_session = None;
                self.state.clear_command_line();
                self.state.command_history.reset_navigation();
                self.set_mode(Mode::Command);
            }
            Command::EnterSearchMode => {
                self.state.clear_command_line();
                self.state.search_history.reset_navigation();
                self.set_mode(Mode::Search);
            }
            Command::ExecuteSearch => {
                let query = self.state.command_line.clone();
                if !query.is_empty() {
                    // Add to search history before executing
                    self.state.search_history.add(query.clone());
                    self.state.last_search_query = Some(query.clone());
                    if self.perform_search(&query, SearchDirection::Forward, false) {
                        self.state.clear_command_line();
                        self.set_mode(Mode::Normal);
                    }
                } else {
                    self.state.search_history.reset_navigation();
                    self.state.clear_command_line();
                    self.set_mode(Mode::Normal);
                }
            }

            Command::TabComplete => {
                if let Some(session) = &mut self.state.completion_session {
                    if !session.dropdown_open {
                        session.dropdown_open = true;
                        session.selected = Some(0);
                    } else {
                        session.select_next();
                    }
                    let picked = session.selected_text().map(|s| s.to_string());
                    let ts = session.token_start;
                    if let Some(text) = picked {
                        self.apply_completion_text(&text, ts);
                    }
                } else {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }

            Command::TabCompletePrev => {
                if let Some(session) = &mut self.state.completion_session {
                    if !session.dropdown_open {
                        session.dropdown_open = true;
                    }
                    session.select_prev();
                    let picked = session.selected_text().map(|s| s.to_string());
                    let ts = session.token_start;
                    if let Some(text) = picked {
                        self.apply_completion_text(&text, ts);
                    }
                } else {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }

            Command::ExecuteCommandLine => {
                if let Some(session) = &self.state.completion_session {
                    if session.dropdown_open && session.selected.is_some() {
                        let text = session.selected_text().map(|s| s.to_string());
                        let ts = session.token_start;
                        if let Some(text) = text {
                            self.apply_completion_text(&text, ts);
                        }
                        self.state.completion_session = None;
                        return;
                    }
                }
                self.state.completion_session = None;
                let command_line = self.state.command_line.clone();
                self.state.command_history.add(command_line.clone());
                self.execute_command_line(command_line);
            }
            _ => {}
        }

        // Handle command line editing (mutations happen here)
        match command {
            Command::AppendToCommandLine(ch) => {
                let was_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                self.state.completion_session = None;
                self.state.append_to_command_line(ch);
                if was_open {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }
            Command::DeleteFromCommandLine => {
                let was_open = self
                    .state
                    .completion_session
                    .as_ref()
                    .map(|s| s.dropdown_open)
                    .unwrap_or(false);
                self.state.completion_session = None;
                self.state.remove_from_command_line();
                if was_open {
                    let input = self.state.command_line.clone();
                    use crate::message::CommandLineMessage;
                    let _ = self
                        .handle_command_line_message(CommandLineMessage::RequestCompletion(input));
                }
            }
            Command::Move(crate::action::Motion::Left, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_left();
            }
            Command::Move(crate::action::Motion::Right, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_right();
            }
            Command::Move(crate::action::Motion::StartOfLine, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_home();
            }
            Command::Move(crate::action::Motion::EndOfLine, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_end();
            }
            Command::Move(crate::action::Motion::PreviousWord, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_word_left();
            }
            Command::Move(crate::action::Motion::NextWord, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.move_command_line_word_right();
            }
            Command::DeleteForward
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.delete_forward_command_line();
            }
            _ => {}
        }
    }

    /// Update editor state and render
    /// This is where ALL state mutations happen - input handling phase is pure
    fn update_state_and_render(
        &mut self,
        keypress: crate::key::Key,
        action: crate::key_handler::KeyAction,
        command: crate::command::Command,
    ) -> Result<(), RiftError> {
        self.handle_key_actions(action);
        self.handle_mode_management(command);

        // Update input tracking (happens during state update, not input handling)
        self.state.update_keypress(keypress);
        self.state.update_command(command);

        self.update_and_render()
    }

    pub fn update_and_render(&mut self) -> Result<(), RiftError> {
        crate::perf_span!("update_and_render", crate::perf::PerfFields::default());
        // Sync buffer cursor to focused window
        let doc_id = self.split_tree.focused_window().document_id;
        if let Some(doc) = self.document_manager.get_document(doc_id) {
            self.split_tree.focused_window_mut().cursor_position = doc.buffer.cursor();
        }

        let (cursor_line, cursor_col, total_lines, is_terminal) =
            if let Some(doc) = self.document_manager.get_document(doc_id) {
                let tw = doc.options.tab_width;
                let line = doc.buffer.get_line();
                let col = render::calculate_cursor_column(&doc.buffer, line, tw);
                let total = doc.buffer.get_total_lines();
                (line, col, total, doc.is_terminal())
            } else {
                return Ok(());
            };
        self.state.update_cursor(cursor_line, cursor_col);

        self.sync_state_with_active_document();
        self.state.error_manager.notifications_mut().prune_expired();
        let gutter_width = if self.state.settings.show_line_numbers {
            self.state.gutter_width
        } else {
            0
        };

        let display_map = if !is_terminal && self.state.settings.soft_wrap {
            let doc = self.document_manager.get_document(doc_id).unwrap();
            let content_width = self.render_system.viewport.visible_cols().saturating_sub(gutter_width).max(1);
            let wrap_width = self.state.settings.wrap_width.unwrap_or(content_width);
            Some(crate::wrap::DisplayMap::build(&doc.buffer, wrap_width, doc.options.tab_width))
        } else {
            None
        };

        let needs_clear = if let Some(ref dm) = display_map {
            let doc = self.document_manager.get_document(doc_id).unwrap();
            let visual_row = dm.char_to_visual_row(doc.buffer.cursor());
            let total_visual = dm.total_visual_rows();
            self.render_system.viewport.update_visual(visual_row, 0, total_visual, gutter_width)
        } else {
            let viewport_col = if is_terminal { 0 } else { cursor_col };
            self.render_system.viewport.update(cursor_line, viewport_col, total_lines, gutter_width)
        };

        if self.split_tree.window_count() > 1 {
            self.update_window_viewports();
            self.render_multi_window(needs_clear)
        } else {
            self.render(needs_clear, display_map.as_ref())
        }
    }

    /// Render the editor interface (pure read - no mutations)
    /// Uses the layer compositor for composited rendering
    pub fn render_to_terminal(&mut self, needs_clear: bool) -> Result<FrameStats, RiftError> {
        self.term.hide_cursor()?;
        let stats = self
            .render_system
            .compositor
            .render_to_terminal(&mut self.term, needs_clear)
            .map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::RENDER_FAILED,
                    e,
                )
            })?;
        self.term.show_cursor()?;
        Ok(stats)
    }

    /// Render the editor interface (pure read - no mutations)
    /// Uses the layer compositor for composited rendering
    fn render(&mut self, needs_clear: bool, display_map: Option<&crate::wrap::DisplayMap>) -> Result<(), RiftError> {
        crate::perf_span!("render", crate::perf::PerfFields::default());
        let Editor {
            document_manager,
            render_system,
            state,
            current_mode,
            dispatcher: _, // Ignore dispatcher
            term,
            pending_keys,
            pending_count,
            ..
        } = self;

        // We need mutable access to call syntax.highlights() which potentially
        // updates parse tree
        let doc = match document_manager.active_document_mut() {
            Some(d) => d,
            None => return Ok(()),
        };

        let (start_logical, end_logical) = if let Some(dm) = display_map {
            let top_vr = render_system.viewport.top_line();
            let bottom_vr = top_vr + render_system.viewport.visible_rows();
            let start_l = dm
                .get_visual_row(top_vr)
                .map(|r| r.logical_line)
                .unwrap_or(0);
            let end_l = dm
                .get_visual_row(bottom_vr.saturating_sub(1).min(dm.total_visual_rows().saturating_sub(1)))
                .map(|r| r.logical_line + 1)
                .unwrap_or(doc.buffer.get_total_lines());
            (start_l, end_l)
        } else {
            let start = render_system.viewport.top_line();
            let end = start + render_system.viewport.visible_rows();
            (start, end)
        };

        let start_char = doc.buffer.line_index.get_start(start_logical).unwrap_or(0);
        let end_char = if end_logical < doc.buffer.get_total_lines() {
            doc.buffer
                .line_index
                .get_start(end_logical)
                .unwrap_or(doc.buffer.len())
        } else {
            doc.buffer.len()
        };

        // Convert to byte offsets for tree-sitter
        let start_byte = doc.buffer.char_to_byte(start_char);
        let end_byte = doc.buffer.char_to_byte(end_char);

        let highlights = doc
            .syntax
            .as_mut()
            .map(|syntax| syntax.highlights(Some(start_byte..end_byte)));

        let capture_names = doc.syntax.as_ref().map(|s| s.capture_names());

        let state = render::RenderState {
            buf: &doc.buffer,
            state,
            current_mode: *current_mode,
            pending_key: pending_keys.last().copied(),
            pending_count: *pending_count,
            needs_clear,
            tab_width: doc.options.tab_width,
            highlights: highlights.as_deref(),
            capture_map: capture_names,
            skip_content: false,
            cursor_row_offset: 0,
            cursor_col_offset: 0,
            cursor_viewport: None,
            terminal_cursor: doc.terminal_cursor,
            custom_highlights: if doc.custom_highlights.is_empty() { None } else { Some(&doc.custom_highlights) },
            show_line_numbers: doc.options.show_line_numbers,
            display_map,
        };

        let _ = render_system.render(term, state)?;

        Ok(())
    }

    fn update_window_viewports(&mut self) {
        let global_show_line_numbers = self.state.settings.show_line_numbers;

        let size = match self.term.get_size() {
            Ok(s) => s,
            Err(_) => return,
        };
        let content_rows = (size.rows as usize).saturating_sub(1);
        let layouts = self
            .split_tree
            .compute_layout(content_rows, size.cols as usize);

        for layout in &layouts {
            let window = match self.split_tree.get_window(layout.window_id) {
                Some(w) => w,
                None => continue,
            };
            let cursor_pos = window.cursor_position;
            let doc_id = window.document_id;
            let doc = match self.document_manager.get_document(doc_id) {
                Some(d) => d,
                None => continue,
            };

            let tab_width = doc.options.tab_width;
            let cursor_line = doc.buffer.line_index.get_line_at(cursor_pos);
            let cursor_col =
                render::calculate_cursor_column_at(&doc.buffer, cursor_line, tab_width, cursor_pos);
            let total_lines = doc.buffer.get_total_lines();
            let viewport_col = if doc.is_terminal() { 0 } else { cursor_col };
            let doc_show_line_numbers = doc.options.show_line_numbers && global_show_line_numbers;
            let gutter_width = if doc_show_line_numbers {
                let digits = if total_lines > 0 {
                    (total_lines as f64).log10().floor() as usize + 1
                } else {
                    1
                };
                digits + 2
            } else {
                0
            };

            let window = match self.split_tree.get_window_mut(layout.window_id) {
                Some(w) => w,
                None => continue,
            };
            // +1 because render_content_to_layer_offset does saturating_sub(1)
            // for the global status bar; multi-window layouts don't need that.
            window.viewport.set_size(layout.rows + 1, layout.cols);
            window
                .viewport
                .update(cursor_line, viewport_col, total_lines, gutter_width);
        }
    }

    fn render_multi_window(&mut self, needs_clear: bool) -> Result<(), RiftError> {
        use crate::layer::LayerPriority;

        let Editor {
            document_manager,
            render_system,
            state,
            current_mode,
            term,
            pending_keys,
            pending_count,
            split_tree,
            ..
        } = self;

        let size = term
            .get_size()
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERM_SIZE", e))?;
        let total_rows = size.rows as usize;
        let total_cols = size.cols as usize;
        let content_rows = total_rows.saturating_sub(1);
        let layouts = split_tree.compute_layout(content_rows, total_cols);

        if render_system.compositor.rows() != total_rows
            || render_system.compositor.cols() != total_cols
        {
            render_system.compositor.resize(total_rows, total_cols);
        }

        let content_layer = render_system
            .compositor
            .get_layer_mut(LayerPriority::CONTENT);
        content_layer.clear();

        let focused_id = split_tree.focused_window_id();

        for layout in &layouts {
            let window = match split_tree.get_window(layout.window_id) {
                Some(w) => w,
                None => continue,
            };
            let doc = match document_manager.get_document_mut(window.document_id) {
                Some(d) => d,
                None => continue,
            };

            let tab_width = doc.options.tab_width;

            let doc_show_line_numbers = doc.options.show_line_numbers && state.settings.show_line_numbers;
            let gutter_width = if doc_show_line_numbers {
                state.gutter_width
            } else {
                0
            };
            let window_cols = layout.cols;
            let display_map = if !doc.is_terminal() {
                let content_width = window_cols.saturating_sub(gutter_width).max(1);
                Some(crate::wrap::DisplayMap::build(&doc.buffer, content_width, tab_width))
            } else {
                None
            };

            let start_line = window.viewport.top_line();
            let end_line = start_line + window.viewport.visible_rows();
            let start_char = doc.buffer.line_index.get_start(start_line).unwrap_or(0);
            let end_char = if end_line < doc.buffer.get_total_lines() {
                doc.buffer
                    .line_index
                    .get_start(end_line)
                    .unwrap_or(doc.buffer.len())
            } else {
                doc.buffer.len()
            };
            let start_byte = doc.buffer.char_to_byte(start_char);
            let end_byte = doc.buffer.char_to_byte(end_char);

            let highlights = doc
                .syntax
                .as_mut()
                .map(|syntax| syntax.highlights(Some(start_byte..end_byte)));
            let capture_names = doc.syntax.as_ref().map(|s| s.capture_names());

            let ctx = render::DrawContext {
                buf: &doc.buffer,
                viewport: &window.viewport,
                current_mode: *current_mode,
                pending_key: pending_keys.last().copied(),
                pending_count: *pending_count,
                state,
                needs_clear,
                tab_width,
                highlights: highlights.as_deref(),
                capture_map: capture_names,
                custom_highlights: if doc.custom_highlights.is_empty() { None } else { Some(&doc.custom_highlights) },
                show_line_numbers: doc.options.show_line_numbers,
                display_map: display_map.as_ref(),
            };

            let content_layer = render_system
                .compositor
                .get_layer_mut(LayerPriority::CONTENT);
            render::render_content_to_layer_offset(content_layer, &ctx, layout.row, layout.col)
                .map_err(|e| RiftError::new(ErrorType::Renderer, "RENDER_FAILED", e))?;
        }

        let divider_fg = state
            .settings
            .syntax_colors
            .as_ref()
            .and_then(|sc| sc.get_color("comment"))
            .or(state.settings.editor_fg);
        let content_layer = render_system
            .compositor
            .get_layer_mut(LayerPriority::CONTENT);
        render::render_dividers(
            content_layer,
            split_tree,
            content_rows,
            total_cols,
            divider_fg,
            state.settings.editor_bg,
        );

        let focused_layout = layouts.iter().find(|l| l.window_id == focused_id).cloned();

        let focused_window = split_tree.focused_window();
        let focused_doc = match document_manager.get_document_mut(focused_window.document_id) {
            Some(d) => d,
            None => return Ok(()),
        };

        let highlights = focused_doc
            .syntax
            .as_mut()
            .map(|syntax| syntax.highlights(None));
        let capture_names = focused_doc.syntax.as_ref().map(|s| s.capture_names());

        let (row_off, col_off, focused_cols) = focused_layout
            .as_ref()
            .map(|l| (l.row, l.col, l.cols))
            .unwrap_or((0, 0, total_cols));

        let focused_tab_width = focused_doc.options.tab_width;
        let focused_doc_show_line_numbers = focused_doc.options.show_line_numbers && state.settings.show_line_numbers;
        let focused_gutter_width = if focused_doc_show_line_numbers {
            state.gutter_width
        } else {
            0
        };
        let focused_display_map = if !focused_doc.is_terminal() {
            let content_width = focused_cols.saturating_sub(focused_gutter_width).max(1);
            Some(crate::wrap::DisplayMap::build(&focused_doc.buffer, content_width, focused_tab_width))
        } else {
            None
        };

        let focused_vp = &split_tree.focused_window().viewport;
        let render_state = render::RenderState {
            buf: &focused_doc.buffer,
            state,
            current_mode: *current_mode,
            pending_key: pending_keys.last().copied(),
            pending_count: *pending_count,
            needs_clear,
            tab_width: focused_tab_width,
            highlights: highlights.as_deref(),
            capture_map: capture_names,
            skip_content: true,
            cursor_row_offset: row_off,
            cursor_col_offset: col_off,
            cursor_viewport: Some(focused_vp),
            terminal_cursor: focused_doc.terminal_cursor,
            custom_highlights: if focused_doc.custom_highlights.is_empty() { None } else { Some(&focused_doc.custom_highlights) },
            show_line_numbers: focused_doc.options.show_line_numbers,
            display_map: focused_display_map.as_ref(),
        };

        let _ = render_system.render(term, render_state)?;

        Ok(())
    }

    fn handle_completion_result(
        &mut self,
        payload: crate::job_manager::jobs::completion::CompletionPayload,
    ) {
        use crate::command_line::commands::completion::{resolve_completion, CompletionAction};

        let was_dropdown_open = self
            .state
            .completion_session
            .as_ref()
            .is_some_and(|s| s.dropdown_open);

        let token_start = payload.token_start;
        let action = resolve_completion(
            payload.result,
            &payload.input,
            token_start,
            &self.state.command_line,
            was_dropdown_open,
        );

        match action {
            CompletionAction::Discard => return,
            CompletionAction::Clear => {
                self.state.completion_session = None;
            }
            CompletionAction::ApplyAndClear { text, token_start } => {
                self.apply_completion_text(&text, token_start);
                self.state.completion_session = None;
            }
            CompletionAction::UpdateDropdown { candidates } => {
                let mut session = crate::state::CompletionSession::new(
                    self.state.command_line.clone(),
                    candidates,
                    token_start,
                );
                session.dropdown_open = true;
                session.selected = Some(0);
                self.state.completion_session = Some(session);
            }
            CompletionAction::ExpandPrefix {
                text,
                token_start,
                candidates,
            } => {
                self.apply_completion_text(&text, token_start);
                self.state.completion_session = Some(crate::state::CompletionSession::new(
                    self.state.command_line.clone(),
                    candidates,
                    token_start,
                ));
            }
            CompletionAction::ShowDropdown { candidates } => {
                let mut session = crate::state::CompletionSession::new(
                    self.state.command_line.clone(),
                    candidates,
                    token_start,
                );
                session.dropdown_open = true;
                session.selected = Some(0);
                self.state.completion_session = Some(session);
            }
        }

        let _ = self.update_and_render();
    }

    fn apply_completion_text(&mut self, text: &str, token_start: usize) {
        let mut new_content = self.state.command_line[..token_start].to_string();
        new_content.push_str(text);
        self.state.command_line_cursor = new_content.len();
        self.state.command_line = new_content;
    }

    /// Reload a directory buffer with a new path.
    fn reload_directory_buffer(&mut self, doc_id: crate::document::DocumentId, new_path: std::path::PathBuf) {
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.kind = crate::document::BufferKind::Directory {
                path: new_path.clone(),
                entries: vec![],
            };
            let _ = doc.buffer.set_cursor(0);
            let len = doc.buffer.len();
            for _ in 0..len { doc.buffer.delete_forward(); }
            let _ = doc.buffer.insert_str("Loading...");
        }
        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(doc_id as usize, new_path, false);
        self.job_manager.spawn(job);
    }

    /// Handle <CR> in a directory (file explorer) buffer.
    fn handle_explorer_select(&mut self) {
        use crate::document::BufferKind;

        // If we're in the explorer center pane, delegate to split-aware select.
        let is_explorer_dir = self.panel_layout.as_ref().map(|l| {
            l.kind == PanelKind::FileExplorer && self.split_tree.focused_window_id() == l.dir_win_id
        }).unwrap_or(false);
        if is_explorer_dir {
            self.handle_explorer_split_select();
            return;
        }

        let (doc_id, line_text, dir_path) = {
            let doc = match self.document_manager.active_document() {
                Some(d) if d.is_directory() => d,
                _ => return,
            };
            if doc.is_dirty() {
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    "Unsaved changes — write with :w first".to_string(),
                );
                return;
            }
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
            let dir_path = match &doc.kind {
                BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            };
            (doc.id, line_text, dir_path)
        };

        if line_text == "../" {
            if let Some(parent) = dir_path.parent().map(|p| p.to_path_buf()) {
                self.reload_directory_buffer(doc_id, parent);
            }
            return;
        }

        let entry_name = line_text.trim_end_matches('/').to_string();
        if entry_name.is_empty() { return; }

        let target_path = dir_path.join(&entry_name);

        if target_path.is_dir() {
            self.reload_directory_buffer(doc_id, target_path);
        } else {
            if let Err(e) = self.open_file(Some(target_path.display().to_string()), false) {
                self.state.handle_error(e);
            } else {
                self.state.clear_command_line();
                if let Err(e) = self.force_full_redraw() { self.state.handle_error(e); }
            }
        }
    }

    /// Handle `-` in a directory buffer — navigate to parent.
    fn handle_explorer_parent(&mut self) {
        use crate::document::BufferKind;

        // If we're in the explorer center pane, delegate to split-aware parent.
        let is_explorer_dir = self.panel_layout.as_ref().map(|l| {
            l.kind == PanelKind::FileExplorer && self.split_tree.focused_window_id() == l.dir_win_id
        }).unwrap_or(false);
        if is_explorer_dir {
            self.handle_explorer_split_parent();
            return;
        }

        let (doc_id, parent) = {
            let doc = match self.document_manager.active_document() {
                Some(d) if d.is_directory() => d,
                _ => return,
            };
            let parent = match &doc.kind {
                BufferKind::Directory { path, .. } => path.parent().map(|p| p.to_path_buf()),
                _ => return,
            };
            (doc.id, parent)
        };
        if let Some(parent_path) = parent {
            self.reload_directory_buffer(doc_id, parent_path);
        }
    }

    /// Open a 3-panel file explorer centred on `center_dir`.
    ///
    /// Layout after call:  [left: parent dir | center: center_dir | right: preview]
    pub fn open_explorer(&mut self, dir: std::path::PathBuf) {
        // If already active, just focus the dir pane.
        if let Some(ref layout) = self.panel_layout.clone() {
            self.split_tree.set_focus(layout.dir_win_id);
            let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
            return;
        }

        let dir_doc_id = self.document_manager.next_id();
        let dir_doc = match crate::document::Document::new_directory(dir_doc_id, dir.clone()) {
            Ok(d) => d,
            Err(e) => { self.state.handle_error(e); return; }
        };
        self.document_manager.add_private_document(dir_doc);

        let preview_doc_id = self.document_manager.next_id();
        let preview_doc = match crate::document::Document::new_directory(
            preview_doc_id,
            std::path::PathBuf::from("[preview]"),
        ) {
            Ok(d) => d,
            Err(e) => { self.state.handle_error(e); return; }
        };
        self.document_manager.add_private_document(preview_doc);

        let size = self.term.get_size().unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        let dir_win_id = self.split_tree.focused_window_id();
        let original_doc_id = self.split_tree.focused_window().document_id;
        if let Some(w) = self.split_tree.windows.get_mut(&dir_win_id) {
            w.document_id = dir_doc_id;
        }

        let preview_win_id = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            dir_win_id,
            preview_doc_id,
            rows,
            cols,
        );

        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(dir_doc_id);

        self.panel_layout = Some(PanelLayout { kind: PanelKind::FileExplorer, dir_win_id, preview_win_id, dir_doc_id, preview_doc_id, original_doc_id });

        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(dir_doc_id as usize, dir, false);
        self.job_manager.spawn(job);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Close the active explorer or undotree split session.
    pub fn close_split_panel(&mut self) {
        let layout = match self.panel_layout.take() {
            Some(l) => l,
            None => return,
        };

        match layout.kind {
            PanelKind::FileExplorer => {
                // Close preview window, restore original doc to dir window, remove both private docs.
                self.split_tree.close_window(layout.preview_win_id);
                self.document_manager.remove_private_document(layout.preview_doc_id);
                if let Some(w) = self.split_tree.windows.get_mut(&layout.dir_win_id) {
                    w.document_id = layout.original_doc_id;
                }
                self.document_manager.remove_private_document(layout.dir_doc_id);
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self.document_manager.switch_to_document(layout.original_doc_id);
            }
            PanelKind::UndoTree => {
                // Reassign the preview window to show the original file before removing preview clone.
                if let Some(w) = self.split_tree.windows.get_mut(&layout.preview_win_id) {
                    w.document_id = layout.original_doc_id;
                }
                self.split_tree.close_window(layout.dir_win_id);
                self.document_manager.remove_private_document(layout.dir_doc_id);
                self.document_manager.remove_private_document(layout.preview_doc_id);
                self.split_tree.set_focus(layout.preview_win_id);
                let _ = self.document_manager.switch_to_document(layout.original_doc_id);
            }
        }
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Open the messages log as a standalone buffer in the current window.
    pub fn open_messages(&mut self, show_all: bool) {
        let id = self.document_manager.next_id();
        let mut doc = match crate::document::Document::new_messages(id, show_all) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };

        let log = self
            .state
            .error_manager
            .notifications()
            .message_log()
            .to_vec();
        doc.populate_messages_buffer(&log);
        // On initial open, position at the end so the newest messages are visible
        let len = doc.buffer.len();
        let _ = doc.buffer.set_cursor(len.saturating_sub(1));

        self.document_manager.add_document(doc);
        if let Err(e) = self.document_manager.switch_to_document(id) {
            self.state.handle_error(e);
            return;
        }
        self.split_tree.focused_window_mut().document_id = id;

        self.last_notification_generation = self
            .state
            .error_manager
            .notifications()
            .generation;

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// Open the undo tree for the active document as a split pane.
    pub fn open_undotree_split(&mut self) {
        if let Some(ref layout) = self.panel_layout.clone() {
            if layout.kind == PanelKind::UndoTree {
                self.split_tree.set_focus(layout.dir_win_id);
                let _ = self.document_manager.switch_to_document(layout.dir_doc_id);
                return;
            }
        }

        let linked_id = self.active_document_id();

        let ut_id = self.document_manager.next_id();
        let ut_doc = match crate::document::Document::new_undotree(ut_id, linked_id) {
            Ok(d) => d,
            Err(e) => { self.state.handle_error(e); return; }
        };
        self.document_manager.add_private_document(ut_doc);

        // Create a private read-only preview doc (clone of linked) so goto_seq never
        // touches the original file and show_line_numbers is naturally false.
        let preview_id = self.document_manager.next_id();
        let preview_doc = {
            let linked = match self.document_manager.get_document(linked_id) {
                Some(d) => d,
                None => { return; }
            };
            match crate::document::Document::new_undotree_preview(preview_id, linked) {
                Ok(d) => d,
                Err(e) => { self.state.handle_error(e); return; }
            }
        };
        self.document_manager.add_private_document(preview_doc);

        let size = self.term.get_size().unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        let dir_win_id = self.split_tree.focused_window_id();
        if let Some(w) = self.split_tree.windows.get_mut(&dir_win_id) {
            w.document_id = ut_id;
        }

        let preview_win_id = self.split_tree.split(
            crate::split::tree::SplitDirection::Vertical,
            dir_win_id,
            preview_id,
            rows,
            cols,
        );

        if let Some(linked_doc) = self.document_manager.get_document(linked_id) {
            let (text, seqs, lc) = crate::undotree_view::render_tree_to_text(&linked_doc.history);
            if let Some(ut_doc) = self.document_manager.get_document_mut(ut_id) {
                ut_doc.populate_undotree_buffer(text, seqs, lc);
            }
        }

        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(ut_id);

        self.panel_layout = Some(PanelLayout {
            kind: PanelKind::UndoTree,
            dir_win_id,
            preview_win_id,
            dir_doc_id: ut_id,
            preview_doc_id: preview_id,
            original_doc_id: linked_id,
        });

        self.sync_state_with_active_document();
        let _ = self.update_and_render();
    }

    /// Called after every cursor movement: if in the explorer dir pane, spawn a preview job.
    fn update_explorer_preview(&mut self) {
        let layout = match &self.panel_layout {
            Some(l) if l.kind == PanelKind::FileExplorer && self.split_tree.focused_window_id() == l.dir_win_id => l.clone(),
            _ => return,
        };

        let (target_path, preview_doc_id) = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
            let dir_path = match &doc.kind {
                crate::document::BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            };
            let entry_name = line_text.trim_end_matches('/');
            if entry_name.is_empty() || entry_name == ".." {
                return;
            }
            (dir_path.join(entry_name), layout.preview_doc_id)
        };

        let job = crate::job_manager::jobs::explorer_preview::ExplorerPreviewJob::new(
            preview_doc_id,
            target_path,
            false,
        );
        self.job_manager.spawn(job);
    }

    /// Called after every cursor movement in the undotree pane: applies goto_seq on the linked doc.
    fn update_undotree_preview(&mut self) {
        let layout = match &self.panel_layout {
            Some(l) if l.kind == PanelKind::UndoTree
                && self.split_tree.focused_window_id() == l.dir_win_id => l.clone(),
            _ => return,
        };

        let seq = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            match &doc.kind {
                crate::document::BufferKind::UndoTree { sequences, .. } => {
                    sequences.get(line_num).copied().unwrap_or(u64::MAX)
                }
                _ => return,
            }
        };

        if seq == u64::MAX { return; }

        if let Some(linked_doc) = self.document_manager.get_document_mut(layout.preview_doc_id) {
            let _ = linked_doc.goto_seq(seq);
            // Reset cursor to top so the preview viewport starts from the beginning
            let _ = linked_doc.buffer.set_cursor(0);
        }
        // Sync the preview window's cursor_position so update_window_viewports uses position 0
        if let Some(w) = self.split_tree.get_window_mut(layout.preview_win_id) {
            w.cursor_position = 0;
        }
        self.spawn_syntax_parse_job(layout.preview_doc_id);
        let _ = self.update_and_render();
    }

    /// Enter a directory or open a file from the explorer dir pane.
    fn handle_explorer_split_select(&mut self) {
        let layout = match self.panel_layout.clone() {
            Some(l) => l,
            None => return,
        };

        let (line_text, dir_path) = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            if doc.is_dirty() {
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    "Unsaved changes — write with :w first".to_string(),
                );
                return;
            }
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let line_bytes = doc.buffer.get_line_bytes(line_num);
            let line_text = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
            let dir_path = match &doc.kind {
                crate::document::BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            };
            (line_text, dir_path)
        };

        let entry_name = line_text.trim_end_matches('/');
        if entry_name.is_empty() || entry_name == ".." {
            self.handle_explorer_split_parent();
            return;
        }

        let target_path = dir_path.join(entry_name);

        if target_path.is_dir() {
            self.reload_directory_buffer(layout.dir_doc_id, target_path);
            self.update_explorer_preview();
        } else {
            self.close_split_panel();
            if let Err(e) = self.open_file(Some(target_path.display().to_string()), false) {
                self.state.handle_error(e);
            } else {
                self.state.clear_command_line();
                if let Err(e) = self.force_full_redraw() { self.state.handle_error(e); }
            }
        }
    }

    /// Navigate the explorer dir pane to its parent directory.
    fn handle_explorer_split_parent(&mut self) {
        let layout = match self.panel_layout.clone() {
            Some(l) => l,
            None => return,
        };

        let parent_path = {
            let doc = match self.document_manager.get_document(layout.dir_doc_id) {
                Some(d) => d,
                None => return,
            };
            match &doc.kind {
                crate::document::BufferKind::Directory { path, .. } => {
                    match path.parent().map(|p| p.to_path_buf()) {
                        Some(p) => p,
                        None => return,
                    }
                }
                _ => return,
            }
        };

        self.reload_directory_buffer(layout.dir_doc_id, parent_path);
        self.update_explorer_preview();
    }

    /// Handle <CR> in an undo-tree buffer — jump to the node on the cursor line.
    fn handle_undotree_select(&mut self) {
        use crate::document::BufferKind;

        let (linked_doc_id, seq) = {
            let doc_id = self.active_document_id();
            let doc = match self.document_manager.get_document(doc_id) {
                Some(d) if d.is_undotree() => d,
                _ => return,
            };
            let cursor = doc.buffer.cursor();
            let line_num = doc.buffer.line_index.get_line_at(cursor);
            let (linked_id, seq) = match &doc.kind {
                BufferKind::UndoTree { linked_doc_id, sequences } => {
                    let seq = sequences.get(line_num).copied().unwrap_or(u64::MAX);
                    (*linked_doc_id, seq)
                }
                _ => return,
            };
            (linked_id, seq)
        };

        if seq == u64::MAX { return; } // connector line

        if let Some(linked_doc) = self.document_manager.get_document_mut(linked_doc_id) {
            if linked_doc.goto_seq(seq).is_err() { return; }
        }
        self.spawn_syntax_parse_job(linked_doc_id);

        // Close the undo tree pane and focus the linked document
        self.close_split_panel();
    }

    /// Re-read the directory listing for the active file explorer pane.
    fn handle_explorer_refresh(&mut self) {
        let layout = match self.panel_layout.as_ref() {
            Some(l) if l.kind == PanelKind::FileExplorer => l.clone(),
            _ => return,
        };
        let path = match self.document_manager.get_document(layout.dir_doc_id) {
            Some(d) => match &d.kind {
                crate::document::BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            },
            None => return,
        };
        let job = crate::job_manager::jobs::explorer::DirectoryListJob::new(layout.dir_doc_id as usize, path, false);
        self.job_manager.spawn(job);
    }

    /// Re-render the undotree buffer from the linked document's current history.
    fn handle_undotree_refresh(&mut self) {
        let layout = match self.panel_layout.as_ref() {
            Some(l) if l.kind == PanelKind::UndoTree => l.clone(),
            _ => return,
        };
        if let Some(linked_doc) = self.document_manager.get_document(layout.original_doc_id) {
            let (text, seqs, lc) = crate::undotree_view::render_tree_to_text(&linked_doc.history);
            if let Some(ut_doc) = self.document_manager.get_document_mut(layout.dir_doc_id) {
                ut_doc.populate_undotree_buffer(text, seqs, lc);
            }
        }
        let _ = self.update_and_render();
    }

    /// Apply the diff from a directory buffer to the filesystem.
    fn apply_directory_diff(&mut self) {
        use crate::document::BufferKind;
        use std::fs;

        let (dir_doc_id, dir_path, diff) = {
            let doc = match self.document_manager.active_document() {
                Some(d) if d.is_directory() => d,
                _ => return,
            };
            let path = match &doc.kind {
                BufferKind::Directory { path, .. } => path.clone(),
                _ => return,
            };
            (doc.id, path, doc.parse_directory_diff())
        };

        if diff.renames.is_empty() && diff.deletes.is_empty() && diff.creates.is_empty() {
            return;
        }

        let mut errors: Vec<String> = Vec::new();
        let mut applied = 0usize;

        // Renames — run synchronously so the reload sees the final state
        for (old_path, new_name) in &diff.renames {
            let new_path = old_path.parent().unwrap_or(&dir_path).join(new_name);
            if let Some(parent) = new_path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    errors.push(format!("mkdir {:?}: {}", parent, e));
                    continue;
                }
            }
            let result = fs::rename(&old_path, &new_path).or_else(|_| {
                // Cross-device fallback: copy then delete
                crate::job_manager::jobs::fs::FsCopyJob::copy_recursive_pub(old_path, &new_path)
                    .and_then(|_| if old_path.is_dir() { fs::remove_dir_all(old_path) } else { fs::remove_file(old_path) })
            });
            match result {
                Ok(_) => applied += 1,
                Err(e) => errors.push(format!("rename {:?}: {}", old_path.file_name().unwrap_or_default(), e)),
            }
        }

        // Deletes
        for path in &diff.deletes {
            let result = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
            match result {
                Ok(_) => applied += 1,
                Err(e) => errors.push(format!("delete {:?}: {}", path.file_name().unwrap_or_default(), e)),
            }
        }

        // Creates
        for name in &diff.creates {
            let is_dir = name.ends_with('/');
            let clean_name = name.trim_end_matches('/');
            let new_path = dir_path.join(clean_name);
            let result = if is_dir {
                fs::create_dir_all(&new_path)
            } else {
                if let Some(parent) = new_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                fs::File::create(&new_path).map(|_| ())
            };
            match result {
                Ok(_) => applied += 1,
                Err(e) => errors.push(format!("create {:?}: {}", new_path.file_name().unwrap_or_default(), e)),
            }
        }

        if applied > 0 {
            self.state.notify(
                crate::notification::NotificationType::Info,
                format!("Applied {} change(s)", applied),
            );
        }
        for err in errors {
            self.state.notify(crate::notification::NotificationType::Error, err);
        }

        // Re-read the current directory now that all operations are complete.
        let reload = crate::job_manager::jobs::explorer::DirectoryListJob::new(dir_doc_id as usize, dir_path, false);
        self.job_manager.spawn(reload);
    }

    /// Set editor mode and update dispatcher
    fn set_mode(&mut self, mode: Mode) {
        let old_mode = self.current_mode;
        self.current_mode = mode;
        if (old_mode == Mode::Command || old_mode == Mode::Search) && mode != old_mode {
            self.state.completion_session = None;
            self.render_system
                .compositor
                .clear_layer(crate::layer::LayerPriority::FLOATING_WINDOW);
        }

        // Clear operator if leaving OperatorPending (and not entering it)
        if mode != Mode::OperatorPending {
            self.pending_operator = None;
        }

        match mode {
            Mode::Command => {
                // Command line handled via RenderSystem state
            }
            Mode::Search => {
                // Search line handled via RenderSystem state
            }
            _ => {}
        }
    }

    pub fn term_mut(&mut self) -> &mut T {
        &mut self.term
    }

    fn execute_operator(&mut self, op: crate::action::OperatorType, motion: Motion) -> bool {
        let count = if self.pending_count > 0 {
            self.pending_count
        } else {
            1
        };
        self.pending_operator = None;
        self.pending_count = 0;

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
            _ => {
                self.set_mode(Mode::Normal);
                false
            }
        }
    }

    fn execute_operator_linewise(&mut self, op: crate::action::OperatorType) -> bool {
        self.pending_operator = None;

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
            _ => {
                self.set_mode(Mode::Normal);
                false
            }
        }
    }

    /// Replay the last repeatable action (dot-repeat)
    fn execute_dot_repeat(&mut self) -> bool {
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

    fn spawn_syntax_parse_job(&mut self, doc_id: crate::document::DocumentId) {
        use crate::job_manager::jobs::syntax::SyntaxParseJob;
        use tree_sitter::Parser;

        if let Some(doc) = self.document_manager.get_document(doc_id) {
            if let Some(syntax) = &doc.syntax {
                // Create parser
                let mut parser = Parser::new();
                if parser.set_language(&syntax.language).is_err() {
                    return;
                }

                let job = SyntaxParseJob::new(
                    doc.buffer.clone(),
                    parser,
                    doc.syntax.as_ref().and_then(|s| s.tree.clone()),
                    doc.syntax.as_ref().and_then(|s| s.highlights_query.clone()),
                    doc.syntax
                        .as_ref()
                        .map(|s| s.language_name.clone())
                        .unwrap_or_default(),
                    doc_id,
                );

                self.job_manager.spawn(job);
            }
        }
    }

    /// Handle a message from a background job
    fn handle_job_message(&mut self, msg: crate::job_manager::JobMessage) -> Result<(), RiftError> {
        use crate::job_manager::jobs::syntax::SyntaxParseResult;
        use crate::job_manager::JobMessage;
        // Parser import not needed here

        // Update manager state
        self.job_manager.update_job_state(&msg);

        match msg {
            JobMessage::Started(id, silent) => {
                let name = self.job_manager.job_name(id);
                self.state
                    .error_manager
                    .notifications_mut()
                    .log_job_event(id, crate::notification::JobEventKind::Started, silent, format!("{}: started", name));
            }
            JobMessage::Progress(id, percentage, msg) => {
                let silent = self.job_manager.is_job_silent(id);
                let name = self.job_manager.job_name(id);
                self.state
                    .error_manager
                    .notifications_mut()
                    .log_job_event(id, crate::notification::JobEventKind::Progress(percentage), silent, format!("{}: {}", name, msg));
            }
            JobMessage::Finished(id, silent) => {
                let name = self.job_manager.job_name(id);
                self.state
                    .error_manager
                    .notifications_mut()
                    .log_job_event(id, crate::notification::JobEventKind::Finished, silent, format!("{}: finished", name));
            }
            JobMessage::Error(id, err) => {
                let silent = self.job_manager.is_job_silent(id);
                let name = self.job_manager.job_name(id);
                self.state
                    .error_manager
                    .notifications_mut()
                    .log_job_event(id, crate::notification::JobEventKind::Error, silent, format!("{}: {}", name, err));
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("{} failed: {}", name, err),
                );
            }
            JobMessage::Cancelled(id) => {
                let silent = self.job_manager.is_job_silent(id);
                let name = self.job_manager.job_name(id);
                self.state
                    .error_manager
                    .notifications_mut()
                    .log_job_event(id, crate::notification::JobEventKind::Cancelled, silent, format!("{}: cancelled", name));
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    format!("{} cancelled", name),
                );
            }
            JobMessage::Custom(id, payload) => {
                let any_payload = payload.into_any();

                // Try DirectoryListing — route to the document by id
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::explorer::DirectoryListing>()
                {
                    Ok(listing) => {
                        let doc_id = listing.doc_id as crate::document::DocumentId;
                        let entries: Vec<crate::document::DirEntry> = listing
                            .entries
                            .iter()
                            .map(|e| crate::document::DirEntry {
                                path: e.path.clone(),
                                is_dir: e.is_dir,
                            })
                            .collect();
                        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                            // Discard stale results if the doc has navigated to a different path
                            let path_matches = matches!(&doc.kind,
                                crate::document::BufferKind::Directory { path, .. }
                                if *path == listing.path);
                            if path_matches {
                                doc.populate_directory_buffer(entries);
                                self.sync_state_with_active_document();
                                let _ = self.force_full_redraw();
                                self.update_explorer_preview();
                            }
                        }
                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try UndoTreeRenderResult — populate the matching undotree buffer
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::undotree::UndoTreeRenderResult>()
                {
                    Ok(res) => {
                        if let Some(ut_doc) =
                            self.document_manager.get_document_mut(res.ut_doc_id)
                        {
                            ut_doc.populate_undotree_buffer(
                                res.text,
                                res.sequences,
                                res.highlights,
                            );
                        }
                        self.sync_state_with_active_document();
                        let _ = self.force_full_redraw();
                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try ExplorerPreviewResult — populate the preview pane
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::explorer_preview::ExplorerPreviewResult>()
                {
                    Ok(res) => {
                        let preview_doc_id = res.right_doc_id;
                        let preview_path = res.path.clone();
                        let is_file_preview = res.dir_entries.is_none();
                        if let Some(doc) = self.document_manager.get_document_mut(preview_doc_id) {
                            let _ = doc.buffer.set_cursor(0);
                            let len = doc.buffer.len();
                            for _ in 0..len {
                                doc.buffer.delete_forward();
                            }
                            doc.syntax = None;
                            doc.custom_highlights.clear();

                            if let Some(entries) = res.dir_entries {
                                doc.kind = crate::document::BufferKind::Directory {
                                    path: res.path.clone(),
                                    entries: entries.clone(),
                                };
                                doc.populate_directory_buffer(entries);
                            } else if let Some(text) = res.file_text {
                                doc.kind = crate::document::BufferKind::File;
                                doc.set_path(&preview_path);
                                let _ = doc.buffer.insert_str(&text);
                                let _ = doc.buffer.set_cursor(0);
                            }
                        }

                        if is_file_preview {
                            if let Ok(loaded) = self.language_loader.load_language_for_file(&preview_path) {
                                let highlights = self
                                    .language_loader
                                    .load_query(&loaded.name, "highlights")
                                    .ok()
                                    .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
                                    .map(Arc::new);
                                if let Ok(syntax) = crate::syntax::Syntax::new(loaded, highlights) {
                                    if let Some(doc) = self.document_manager.get_document_mut(preview_doc_id) {
                                        doc.set_syntax(syntax);
                                        // Synchronously parse so highlights are ready for the
                                        // immediately following render (no async timing gap).
                                        let source = doc.buffer.to_logical_bytes();
                                        if let Some(s) = &mut doc.syntax {
                                            s.incremental_parse(&source);
                                        }
                                    }
                                    self.spawn_syntax_parse_job(preview_doc_id);
                                }
                            }
                        }

                        self.sync_state_with_active_document();
                        let _ = self.update_and_render();
                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try FileSaveResult
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::file_operations::FileSaveResult>(
                ) {
                    Ok(res) => {
                        if let Some(doc) = self.document_manager.get_document_mut(res.document_id) {
                            doc.mark_as_saved(res.saved_seq);
                            doc.set_path(res.path.clone());

                            // Update cached filename in state
                            let display_name = doc.display_name().to_string();
                            self.state.update_filename(display_name);
                        }

                        // Show success notification
                        self.state.notify(
                            crate::notification::NotificationType::Success,
                            format!("Written to {}", res.path.display()),
                        );

                        if self.pending_quit_job_id == Some(id) {
                            self.should_quit = true;
                        }

                        // Sync state and redraw to update dirty indicator
                        self.sync_state_with_active_document();
                        let _ = self.force_full_redraw();

                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try FileLoadResult
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::file_operations::FileLoadResult>(
                ) {
                    Ok(res) => {
                        // Scope for doc mutation
                        let warming_data = if let Some(doc) =
                            self.document_manager.get_document_mut(res.document_id)
                        {
                            doc.apply_loaded_content(res.line_index, res.line_ending);
                            // Extract data for cache warming
                            let table = doc.buffer.line_index.table.clone();
                            let revision = doc.buffer.revision;
                            let path = doc.path().map(|p| p.to_path_buf());
                            Some((table, revision, path))
                        } else {
                            None
                        };

                        // Re-initialize syntax
                        if let Some((_, _, Some(path))) = &warming_data {
                            if let Ok(loaded) = self.language_loader.load_language_for_file(path) {
                                let highlights = self
                                    .language_loader
                                    .load_query(&loaded.name, "highlights")
                                    .ok()
                                    .and_then(|source| {
                                        tree_sitter::Query::new(&loaded.language, &source).ok()
                                    })
                                    .map(Arc::new);

                                if let Ok(syntax) = crate::syntax::Syntax::new(loaded, highlights) {
                                    if let Some(doc) =
                                        self.document_manager.get_document_mut(res.document_id)
                                    {
                                        doc.set_syntax(syntax);
                                    }
                                }
                            }
                        }

                        // Spawn syntax parse (requires self)
                        self.spawn_syntax_parse_job(res.document_id);

                        // Spawn cache warming if data extracted
                        if let Some((table, revision, _)) = warming_data {
                            let job = crate::job_manager::jobs::cache_warming::CacheWarmingJob::new(
                                table, revision,
                            );
                            self.job_manager.spawn(job);
                        }

                        self.sync_state_with_active_document();
                        let _ = self.force_full_redraw();

                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                match any_payload.downcast::<SyntaxParseResult>() {
                    Ok(result) => {
                        let doc_id = result.document_id;
                        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                            if let Some(syntax) = &mut doc.syntax {
                                syntax.update_from_result(*result);
                            }
                        }
                        // In multi-window mode we must re-render all windows so that
                        // secondary panes (e.g. explorer preview) get their highlights updated.
                        if self.split_tree.window_count() > 1 {
                            self.update_and_render()?;
                        } else {
                            self.render(false, None)?;
                        }
                    }
                    Err(any_payload) => {
                        // Try CompletionPayload
                        let any_payload = match any_payload
                            .downcast::<crate::job_manager::jobs::completion::CompletionPayload>()
                        {
                            Ok(payload) => {
                                self.handle_completion_result(*payload);
                                return Ok(());
                            }
                            Err(p) => p,
                        };

                        // Try ByteLineMap (CacheWarmingJob)
                        if let Ok(map) =
                            any_payload.downcast::<crate::buffer::byte_map::ByteLineMap>()
                        {
                            if let Some(doc) = self.document_manager.active_document_mut() {
                                if doc.buffer.revision == map.revision {
                                    *doc.buffer.byte_map_cache.borrow_mut() = Some(*map);
                                    if self.state.debug_mode {
                                        self.state.notify(
                                            crate::notification::NotificationType::Info,
                                            "Search cache warmed".to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            JobMessage::TerminalOutput(doc_id, data) => {
                if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                    doc.handle_terminal_data(&data);
                    // Trigger redraw if this is the active document
                    if self.active_document_id() == doc_id {
                        let _ = self.update_and_render();
                    }
                }
            }
            JobMessage::TerminalExit(doc_id) => {
                // Switch back to Normal mode if this is the active terminal
                if self.active_document_id() == doc_id {
                    self.set_mode(Mode::Normal);
                }
                // Collect split windows showing this terminal before removing the doc
                let affected_windows = self.split_tree.windows_for_document(doc_id);

                // Force-remove the terminal buffer (skips dirty check)
                match self.document_manager.remove_document_force(doc_id) {
                    Err(e) => {
                        self.state.notify(
                            crate::notification::NotificationType::Error,
                            format!("Failed to close terminal: {}", e),
                        );
                    }
                    Ok(()) => {
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            "Terminal closed".to_string(),
                        );
                        // Close each split showing this terminal; reassign if it's the last window.
                        let new_doc_id = self.document_manager.active_document_id().unwrap_or(1);
                        for window_id in affected_windows {
                            if !self.split_tree.close_window(window_id) {
                                if let Some(w) = self.split_tree.get_window_mut(window_id) {
                                    w.document_id = new_doc_id;
                                }
                            }
                        }
                    }
                }
                self.sync_state_with_active_document();
                let _ = self.update_and_render();
            }
        }

        // Periodic cleanup of finished jobs
        self.job_manager.cleanup_finished_jobs();
        Ok(())
    }

    /// Update search highlights based on current buffer state
    fn update_search_highlights(&mut self) {
        if let Some(query) = self.state.last_search_query.clone() {
            let doc = self.document_manager.active_document_mut().unwrap();
            match doc.find_all_matches(&query) {
                Ok((matches, _)) => {
                    self.state.search_matches = matches;
                }
                Err(_) => {
                    self.state.search_matches.clear();
                }
            }
        } else {
            self.state.search_matches.clear();
        }
    }

    /// Execute a command line string
    fn execute_command_line(&mut self, cmd: String) {
        use crate::command_line::commands::executor::ExecutionResult;
        let parsed_command = self.command_parser.parse(&cmd);
        let active_id = self.active_document_id();
        let result = CommandExecutor::execute(
            parsed_command,
            &mut self.state,
            self.document_manager
                .get_document_mut(active_id)
                .expect("active document missing"),
            &self.settings_registry,
            &self.document_settings_registry,
        );
        match result {
            ExecutionResult::Failure => return, // keep command line visible for editing
            ExecutionResult::OpenTerminal { cmd, .. } => {
                if let Err(e) = self.open_terminal(cmd) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    self.set_mode(Mode::Insert);
                }
                return;
            }
            ExecutionResult::Success => {}
            ExecutionResult::Quit { bangs } => { self.do_quit(bangs > 0); }
            ExecutionResult::Write => { self.do_save(); }
            ExecutionResult::WriteAndQuit => { self.do_save_and_quit(); }
            ExecutionResult::Redraw => {
                self.state.clear_command_line();
                if let Err(e) = self.force_full_redraw() { self.state.handle_error(e); }
            }
            ExecutionResult::Edit { path, bangs } => {
                if let Err(e) = self.open_file(path, bangs > 0) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    if let Err(e) = self.force_full_redraw() { self.state.handle_error(e); }
                }
            }
            ExecutionResult::BufferNext { .. } => { self.do_buffer_next(); }
            ExecutionResult::BufferPrevious { .. } => { self.do_buffer_prev(); }
            ExecutionResult::BufferList => { self.do_show_buffer_list(); }
            ExecutionResult::NotificationClear { bangs } => { self.do_notification_clear(bangs > 0); }
            ExecutionResult::Undo { count } => { self.do_undo(count); }
            ExecutionResult::Redo { count } => { self.do_redo(count); }
            ExecutionResult::UndoGoto { seq } => { self.do_undo_goto(seq); }
            ExecutionResult::Checkpoint => {}
            ExecutionResult::SplitWindow { direction, subcommand } => {
                self.do_split_window(direction, subcommand);
            }
            ExecutionResult::OpenDirectory { path } => { self.open_explorer(path); }
            ExecutionResult::OpenUndoTree => { self.open_undotree_split(); }
            ExecutionResult::OpenMessages { show_all } => { self.open_messages(show_all); }
        }
        if self.current_mode == Mode::Command {
            self.set_mode(Mode::Normal);
        }
    }

    fn handle_command_line_message(
        &mut self,
        msg: crate::message::CommandLineMessage,
    ) -> Result<(), RiftError> {
        use crate::message::CommandLineMessage;
        match msg {
            CommandLineMessage::ExecuteCommand(cmd) => {
                self.execute_command_line(cmd);
                self.state.clear_command_line();
            }
            CommandLineMessage::ExecuteSearch(query) => {
                if !query.is_empty() {
                    if self.perform_search(&query, SearchDirection::Forward, false) {
                        self.state.clear_command_line();
                    }
                } else {
                    self.state.clear_command_line();
                }
            }
            CommandLineMessage::CancelMode => {
                self.state.completion_session = None;
                self.state.clear_command_line();
            }
            CommandLineMessage::RequestCompletion(input) => {
                use crate::job_manager::jobs::completion::CompletionJob;
                let current_settings = Some(self.state.settings.clone());
                let current_doc_options = Some(self.active_document().options.clone());
                self.job_manager.spawn(CompletionJob {
                    input,
                    current_settings,
                    current_doc_options,
                });
            }
        }
        Ok(())
    }

}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.term.deinit();
    }
}

mod context_impl;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
