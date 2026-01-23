//! Editor core
//! Main editor logic that ties everything together

pub mod actions;

use crate::command::{Command, Dispatcher};
use crate::command_line::commands::{CommandExecutor, CommandParser, ExecutionResult};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use crate::document::{Document, DocumentId};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::executor::execute_command;
use crate::key_handler::{KeyAction, KeyHandler};

use crate::mode::Mode;
use crate::render;
use crate::screen_buffer::FrameStats;
use crate::search::SearchDirection;
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
    #[allow(dead_code)]
    language_loader: Arc<crate::syntax::loader::LanguageLoader>,
    /// Active modal component (overlay)
    pub modal: Option<ActiveModal>,
    /// Background job manager
    pub job_manager: crate::job_manager::JobManager,
    /// Job ID required to finish before quitting
    pending_quit_job_id: Option<usize>,
}

/// Helper struct to track active modal and its layer
pub struct ActiveModal {
    pub component: Box<dyn crate::component::Component>,
    pub layer: crate::layer::LayerPriority,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentAction {
    ExecuteCommand(String),
    ExecuteSearch(String),
    CancelMode,
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

        // Syntax loading moved to post-initialization
        // ...

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
            modal: None,
            job_manager: crate::job_manager::JobManager::new(),
            pending_quit_job_id: None,
        };

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

    /// Get the ID of the active document
    pub fn active_document_id(&self) -> DocumentId {
        self.document_manager
            .active_document_id()
            .expect("No active document")
    }

    /// Get mutable reference to the active document
    pub fn active_document(&mut self) -> &mut Document {
        self.document_manager
            .active_document_mut()
            .expect("No active document")
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
        let doc = self.document_manager.active_document_mut().unwrap();

        // Calculate visible range for syntax highlighting
        let start_line = self.render_system.viewport.top_line();
        let end_line = start_line + self.render_system.viewport.visible_rows();
        let start_char = doc.buffer.line_index.get_start(start_line).unwrap_or(0);
        let end_char = if end_line < doc.buffer.get_total_lines() {
            doc.buffer
                .line_index
                .get_start(end_line)
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
            state: &self.state,
            current_mode: self.current_mode,
            pending_key: self.dispatcher.pending_key(),
            pending_count: self.dispatcher.pending_count(),
            needs_clear: true,
            tab_width: doc.options.tab_width,
            highlights: highlights.as_deref(),
            capture_map: capture_names,
            modal: self.modal.as_mut(),
        };

        self.render_system
            .force_full_redraw(&mut self.term, state)
            .map_err(|e| {
                RiftError::new(
                    ErrorType::Io,
                    crate::constants::errors::RENDER_FAILED,
                    e.to_string(),
                )
            })?;

        Ok(())
    }

    /// Remove a document by ID with strict tab semantics
    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        self.document_manager.remove_document(id)?;
        self.sync_state_with_active_document();
        Ok(())
    }

    /// Open a file in a new document or reload the current one
    ///
    /// If file_path is Some, it opens that file (or creates a new document for
    /// it if not found). If file_path is None, it reloads the current active
    /// document.
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
                // Already open, use manager to switch
                self.document_manager.open_file(Some(path_str), force)?;
            } else {
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

        self.sync_state_with_active_document();
        Ok(())
    }

    /// Perform a search in the document
    fn perform_search(&mut self, query: &str, direction: SearchDirection, skip_current: bool) {
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
            }
            Ok((None, _stats)) => {
                // No match found - don't move cursor, no notification needed
                // The user can see from the cursor position that nothing was found
            }
            Err(e) => {
                // Actual search error (e.g., regex compilation failure)
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("Search error: {}", e),
                );
            }
        }
        self.close_active_modal();
    }

    /// Run the editor main loop
    pub fn run(&mut self) -> Result<(), RiftError> {
        // Initial render
        self.update_and_render()?;

        // Main event loop
        while !self.should_quit {
            // Poll for job messages (throttled)
            let mut processed_jobs = 0;
            const MAX_JOB_MESSAGES: usize = 10;
            while processed_jobs < MAX_JOB_MESSAGES {
                if let Ok(msg) = self.job_manager.receiver().try_recv() {
                    self.handle_job_message(msg)?;
                    processed_jobs += 1;
                } else {
                    break;
                }
            }
            // If we processed jobs, we might need a re-render if they affected state
            if processed_jobs > 0 {
                // For now, we'll just let the next loop iteration handle any render updates
                // triggered by notifications or state changes.
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

                let modal_result = self
                    .modal
                    .as_mut()
                    .map(|modal| modal.component.handle_input(key_press));

                if let Some(result) = modal_result {
                    use crate::component::EventResult;

                    match result {
                        EventResult::Consumed => continue,
                        EventResult::Ignored => continue,
                        EventResult::Action(action) => {
                            if let Err(e) = action.execute(self) {
                                self.state.handle_error(e);
                            }
                            continue;
                        }
                    }
                }

                // Process keypress through key handler
                let current_mode = self.current_mode;
                let action = KeyHandler::process_key(key_press, current_mode);

                // Translate key to command (skip if action indicates special handling)
                let cmd = match action {
                    KeyAction::ExitInsertMode
                    | KeyAction::ExitCommandMode
                    | KeyAction::ToggleDebug
                    | KeyAction::Resize(_, _) => {
                        // Skip command translation for special actions
                        Command::Noop
                    }
                    _ => self.dispatcher.translate_key(key_press),
                };

                // Execute command if it affects the buffer (and not in command
                // mode)
                let should_execute_buffer = current_mode != Mode::Command
                    && current_mode != Mode::Search
                    && !matches!(
                        cmd,
                        Command::EnterInsertMode
                            | Command::EnterCommandMode
                            | Command::EnterSearchMode
                            | Command::AppendToCommandLine(_)
                            | Command::DeleteFromCommandLine
                            | Command::ExecuteCommandLine
                            | Command::ExecuteSearch
                            | Command::Quit
                            | Command::Noop
                            | Command::BufferNext
                            | Command::BufferPrevious
                    );

                if should_execute_buffer {
                    let viewport_height = self.render_system.viewport.visible_rows();

                    // Wrap mutating commands (except Undo/Redo) in a transaction
                    // Skip wrapping in Insert mode - it already has an open
                    // transaction from mode entry
                    let needs_transaction = cmd.is_mutating()
                        && !matches!(cmd, Command::Undo | Command::Redo)
                        && current_mode != Mode::Insert;

                    let res = {
                        let doc = self.document_manager.active_document_mut().unwrap();
                        let expand_tabs = doc.options.expand_tabs;
                        let tab_width = doc.options.tab_width;

                        if needs_transaction {
                            doc.begin_transaction(format!("{:?}", cmd));
                        }

                        let result = execute_command(
                            cmd,
                            doc,
                            expand_tabs,
                            tab_width,
                            viewport_height,
                            self.state.last_search_query.as_deref(),
                        );

                        if needs_transaction {
                            doc.commit_transaction();
                        }

                        result
                    };
                    if let Err(e) = res {
                        self.state.handle_error(e);
                    }
                    if cmd.is_mutating() {
                        // Mark document dirty is handled by Document methods now
                        // Update search highlights if active
                        self.update_search_highlights();
                        // If buffer changed, re-parse syntax
                        if let Some(doc_id) = self.document_manager.active_document_id() {
                            self.spawn_syntax_parse_job(doc_id);
                        }
                    }
                }

                // Handle quit command (special case - exits loop)
                if cmd == Command::Quit {
                    self.should_quit = true;
                    continue;
                }

                self.update_state_and_render(key_press, action, cmd)?;
            } else {
                // Idle processing
                self.update_and_render()?;
            }
        }

        Ok(())
    }

    /// Handle special actions (mutations happen here, not during input handling)
    fn handle_key_actions(&mut self, action: crate::key_handler::KeyAction) {
        match action {
            KeyAction::ExitInsertMode => {
                // Commit insert mode transaction before exiting
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .commit_transaction();
                self.set_mode(Mode::Normal);
            }
            KeyAction::ExitCommandMode => {
                self.state.clear_command_line();
                self.close_active_modal();
                self.set_mode(Mode::Normal);
            }
            KeyAction::ExitSearchMode => {
                self.state.clear_command_line();
                self.close_active_modal();
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

    /// Switch between modes based on command, and handle commandline input
    fn handle_mode_management(&mut self, command: crate::command::Command) {
        match command {
            Command::EnterInsertMode => {
                // Start transaction for grouping insert mode edits
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction(crate::constants::history::INSERT_LABEL);
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAfter => {
                let doc = self.document_manager.active_document_mut().unwrap();
                doc.buffer.move_right();
                doc.begin_transaction(crate::constants::history::INSERT_LABEL);
                self.set_mode(Mode::Insert);
            }
            Command::EnterCommandMode => {
                self.state.clear_command_line();
                self.set_mode(Mode::Command);
            }
            Command::EnterSearchMode => {
                self.state.clear_command_line();
                self.set_mode(Mode::Search);
            }
            Command::ExecuteSearch => {
                let query = self.state.command_line.clone();
                if !query.is_empty() {
                    self.state.last_search_query = Some(query.clone());
                    self.perform_search(&query, SearchDirection::Forward, false);
                }
                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
            }

            Command::ExecuteCommandLine => {
                // Parse and execute the command
                let command_line = self.state.command_line.clone();
                let parsed_command = self.command_parser.parse(&command_line);
                let execution_result = CommandExecutor::execute(
                    parsed_command.clone(),
                    &mut self.state,
                    self.document_manager
                        .active_document_mut()
                        .expect("active document missing"),
                    &self.settings_registry,
                    &self.document_settings_registry,
                );

                self.handle_execution_result(execution_result);
            }
            _ => {}
        }

        // Handle command line editing (mutations happen here)
        match command {
            Command::AppendToCommandLine(ch) => {
                // ch is guaranteed to be valid ASCII (32-126) from
                // translate_command_mode
                self.state.append_to_command_line(ch);
            }
            Command::DeleteFromCommandLine => {
                self.state.remove_from_command_line();
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
                self.state.error_manager.notifications_mut().info(format!(
                    "Moving word left. Cursor before: {}",
                    self.state.command_line_cursor
                ));
                self.state.move_command_line_word_left();
                self.state.error_manager.notifications_mut().info(format!(
                    "Moved word left. Cursor after: {}",
                    self.state.command_line_cursor
                ));
            }
            Command::Move(crate::action::Motion::NextWord, _)
                if self.current_mode == Mode::Command || self.current_mode == Mode::Search =>
            {
                self.state.error_manager.notifications_mut().info(format!(
                    "Moving word right. Cursor before: {}",
                    self.state.command_line_cursor
                ));
                self.state.move_command_line_word_right();
                self.state.error_manager.notifications_mut().info(format!(
                    "Moved word right. Cursor after: {}",
                    self.state.command_line_cursor
                ));
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

    /// Update state and render the editor (for initial render)
    pub fn update_and_render(&mut self) -> Result<(), RiftError> {
        // Update buffer and cursor state only (no input tracking on initial
        // render)
        let tab_width = self.active_document().options.tab_width;
        let cursor_line = self.active_document().buffer.get_line();
        let cursor_col =
            render::calculate_cursor_column(&self.active_document().buffer, cursor_line, tab_width);
        self.state.update_cursor(cursor_line, cursor_col);

        self.sync_state_with_active_document();
        self.state.error_manager.notifications_mut().prune_expired();

        // Update viewport based on cursor position (state mutation happens here)
        let total_lines = self
            .document_manager
            .active_document()
            .unwrap()
            .buffer
            .get_total_lines();
        let gutter_width = if self.state.settings.show_line_numbers {
            self.state.gutter_width
        } else {
            0
        };
        let needs_clear =
            self.render_system
                .viewport
                .update(cursor_line, cursor_col, total_lines, gutter_width);

        self.render(needs_clear)
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
    fn render(&mut self, needs_clear: bool) -> Result<(), RiftError> {
        let Editor {
            document_manager,
            render_system,
            state,
            current_mode,
            dispatcher,
            term,
            ..
        } = self;

        // We need mutable access to call syntax.highlights() which potentially
        // updates parse tree
        let doc = document_manager.active_document_mut().unwrap();

        // Calculate visible range for syntax highlighting
        let start_line = render_system.viewport.top_line();
        let end_line = start_line + render_system.viewport.visible_rows();

        let start_char = doc.buffer.line_index.get_start(start_line).unwrap_or(0);
        let end_char = if end_line < doc.buffer.get_total_lines() {
            doc.buffer
                .line_index
                .get_start(end_line)
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
            pending_key: dispatcher.pending_key(),
            pending_count: dispatcher.pending_count(),
            needs_clear,
            tab_width: doc.options.tab_width,
            highlights: highlights.as_deref(),
            capture_map: capture_names,
            modal: self.modal.as_mut(),
        };

        let _ = render_system.render(term, state)?;

        Ok(())
    }

    /// Helper to close any active modal, clear optional layer, and reset to Normal mode
    fn close_active_modal(&mut self) {
        if let Some(modal) = &self.modal {
            self.render_system.compositor.clear_layer(modal.layer);
        }
        self.modal = None;
        self.set_mode(Mode::Normal);
        if let Err(e) = self.update_and_render() {
            self.state.handle_error(e);
        }
    }

    /// Set editor mode and update dispatcher
    fn set_mode(&mut self, mode: Mode) {
        self.current_mode = mode;
        self.dispatcher.set_mode(mode);

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

    // Handle execution results from command_line commands
    fn handle_execution_result(&mut self, execution_result: ExecutionResult) {
        let mut should_close_modal = true;
        match execution_result {
            ExecutionResult::Quit { bangs } => {
                if self.active_document().is_dirty() && bangs == 0 {
                    self.state.handle_error(RiftError {
                        severity: ErrorSeverity::Warning,
                        kind: ErrorType::Execution,
                        code: "UNSAVED_CHANGES".to_string(),
                        message: "No write since last change (add ! to override)".to_string(),
                    });
                } else {
                    self.should_quit = true;
                }
            }
            ExecutionResult::Write => {
                // Save document ASYNC
                if let Some(doc) = self.document_manager.active_document_mut() {
                    if let Some(path) = doc.path() {
                        let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                            doc.id,
                            doc.buffer.line_index.table.clone(),
                            path.to_path_buf(),
                            doc.options.line_ending,
                            doc.revision(),
                        );
                        let _id = self.job_manager.spawn(job);
                    } else {
                        self.state.handle_error(RiftError::new(
                            ErrorType::Io,
                            "NO_FILENAME",
                            "No file name",
                        ));
                    }
                }
                self.state.clear_command_line();
                self.close_active_modal();
            }
            ExecutionResult::WriteAndQuit => {
                // Save ASYNC then Quit
                let res = {
                    let doc = self.document_manager.active_document().unwrap();
                    if doc.has_path() {
                        Ok((
                            doc.id,
                            doc.path().unwrap().to_path_buf(),
                            doc.buffer.line_index.table.clone(),
                            doc.options.line_ending,
                            doc.revision(),
                        ))
                    } else if let Some(path) = &self.state.file_path {
                        Ok((
                            doc.id,
                            std::path::PathBuf::from(path),
                            doc.buffer.line_index.table.clone(),
                            doc.options.line_ending,
                            doc.revision(),
                        ))
                    } else {
                        Err(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"))
                    }
                };

                match res {
                    Ok((doc_id, path, table, line_ending, revision)) => {
                        let job = crate::job_manager::jobs::file_operations::FileSaveJob::new(
                            doc_id,
                            table,
                            path.clone(),
                            line_ending,
                            revision,
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
                self.close_active_modal();
            }
            ExecutionResult::Failure => {
                // Error already reported by executor to state/notification
                // manager Keep command line visible so user can see it
                should_close_modal = false;
            }
            ExecutionResult::Redraw => {
                // Close command line first before redraw
                self.state.clear_command_line();

                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
            }
            ExecutionResult::Edit { path, bangs } => {
                let force = bangs > 0;
                if let Err(e) = self.open_file(path, force) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                    // Force redraw after opening a file
                    if let Err(e) = self.force_full_redraw() {
                        self.state.handle_error(e);
                    }
                }
            }
            ExecutionResult::BufferNext { bangs: _bangs } => {
                // Use document manager to switch tabs
                self.document_manager.switch_next_tab();

                self.sync_state_with_active_document();
                self.state.clear_command_line();
                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
            }
            ExecutionResult::BufferPrevious { bangs: _bangs } => {
                self.document_manager.switch_prev_tab();

                self.sync_state_with_active_document();
                self.state.clear_command_line();
                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
            }
            ExecutionResult::NotificationClear { bangs } => {
                if bangs > 0 {
                    self.state.error_manager.notifications_mut().clear_all();
                } else {
                    self.state.error_manager.notifications_mut().clear_last();
                }
                self.state.clear_command_line();
                self.close_active_modal();
            }
            ExecutionResult::BufferList => {
                let buffers = self.document_manager.get_buffer_list();
                let mut message = String::new();
                for info in buffers {
                    let dirty = if info.is_dirty { "+" } else { " " };
                    let read_only = if info.is_read_only { "R" } else { " " };
                    let current = if info.is_current { "%" } else { " " };
                    if !message.is_empty() {
                        message.push('\n');
                    }
                    message.push_str(&format!(
                        "[{}] {}: {}{}{}",
                        info.index + 1,
                        info.name,
                        current,
                        dirty,
                        read_only
                    ));
                }

                self.state
                    .notify(crate::notification::NotificationType::Info, message);
                self.state.clear_command_line();
                self.close_active_modal();
            }
            ExecutionResult::Success => {
                self.state.clear_command_line();
                self.close_active_modal();
            }
            ExecutionResult::Undo { count } => {
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
            ExecutionResult::Redo { count } => {
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
            ExecutionResult::UndoGoto { seq } => {
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
            ExecutionResult::Checkpoint => {
                // Already handled in executor
            }
            ExecutionResult::SpawnJob(job) => {
                let id = self.job_manager.spawn(job);
                self.state.notify(
                    crate::notification::NotificationType::Info,
                    format!("Job {} spawned", id),
                );
            }
            ExecutionResult::OpenComponent {
                component,
                initial_job,
            } => {
                // Close command line first (if not already handled by logic ensuring modals are exclusive)
                // Actually close_active_modal() clears mode.
                self.close_active_modal();

                self.modal = Some(ActiveModal {
                    component,
                    layer: crate::layer::LayerPriority::POPUP,
                });
                self.set_mode(Mode::Overlay);
                should_close_modal = false;

                if let Some(job) = initial_job {
                    self.job_manager.spawn(job);
                }
            }
        }
        if should_close_modal {
            self.close_active_modal();
            self.state.clear_command_line();
        }
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
            JobMessage::Started(_, _) => {
                // Silent start
            }
            JobMessage::Progress(_id, _percentage, _msg) => {
                // Silent progress
            }
            JobMessage::Finished(id, silent) => {
                // Propagate to generic component if active
                if let Some(modal) = self.modal.as_mut() {
                    let res = modal
                        .component
                        .handle_job_message(JobMessage::Finished(id, silent));
                    if let crate::component::EventResult::Action(action) = res {
                        if let Err(e) = action.execute(self) {
                            self.state.handle_error(e);
                        }
                    }
                }

                // Cleanup? The manager handles cleanup of joined threads later,
                // but we might want to trigger it eventually.
                // For now, manual cleanup or lazily is fine.
            }
            JobMessage::Error(id, err) => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("Job {} failed: {}", id, err),
                );
            }
            JobMessage::Cancelled(id) => {
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    format!("Job {} cancelled", id),
                );
            }
            JobMessage::Custom(id, payload) => {
                // Check for core types first to avoid moving payload if it's for Editor
                let is_core = {
                    let any = payload.as_any();
                    any.is::<crate::job_manager::jobs::file_operations::FileSaveResult>()
                        || any.is::<crate::job_manager::jobs::file_operations::FileLoadResult>()
                        || any.is::<SyntaxParseResult>()
                        || any.is::<crate::buffer::byte_map::ByteLineMap>()
                };

                // Intercept generic component messages if not core
                if !is_core {
                    if let Some(modal) = self.modal.as_mut() {
                        let res = modal
                            .component
                            .handle_job_message(JobMessage::Custom(id, payload));
                        match res {
                            crate::component::EventResult::Action(action) => {
                                if let Err(e) = action.execute(self) {
                                    self.state.handle_error(e);
                                }
                                return Ok(());
                            }
                            crate::component::EventResult::Consumed => return Ok(()),
                            crate::component::EventResult::Ignored => return Ok(()),
                        }
                    }
                }

                // If we are here, it MUST be a core type (or we have no modal).
                // If we have no modal and it's !is_core, we proceed to try downcast, which will fail nicely.
                let any_payload = payload.into_any();

                // Try FileSaveResult
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::file_operations::FileSaveResult>(
                ) {
                    Ok(res) => {
                        if let Some(doc) = self.document_manager.get_document_mut(res.document_id) {
                            doc.mark_as_saved(res.revision);
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

                // Try SyntaxParseResult
                match any_payload.downcast::<SyntaxParseResult>() {
                    Ok(result) => {
                        let doc_id = result.document_id;
                        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                            if let Some(syntax) = &mut doc.syntax {
                                syntax.update_from_result(*result);
                                // Use render(false) instead of force_full_redraw to reduce flickering.
                                // The RenderSystem will use highlights_hash to detect changes.
                                self.render(false)?;
                            }
                        }
                    }
                    Err(any_payload) => {
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
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.term.deinit();
    }
}

mod component_action_impl;
mod context_impl;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
