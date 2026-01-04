//! Editor core
//! Main editor logic that ties everything together

use crate::command::{Command, Dispatcher};
use crate::command_line::commands::{CommandExecutor, CommandParser, ExecutionResult};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use crate::document::{Document, DocumentId};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::executor::execute_command;
use crate::key_handler::{KeyAction, KeyHandler};
use crate::layer::LayerCompositor;
use crate::mode::Mode;
use crate::render;
use crate::screen_buffer::FrameStats;
use crate::search::{find_next, SearchDirection};
use crate::state::{State, UserSettings};
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use std::collections::HashMap;
use std::sync::Arc;

/// Main editor struct
pub struct Editor<T: TerminalBackend> {
    /// Terminal backend
    pub term: T,
    /// Render cache for selective redrawing
    pub render_cache: crate::render::RenderCache,
    documents: HashMap<DocumentId, Document>,
    tab_order: Vec<DocumentId>,
    current_tab: usize,
    next_document_id: DocumentId,
    compositor: LayerCompositor,
    viewport: Viewport,
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
    pub modal: Option<Box<dyn crate::component::Component>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentAction {
    UndoTreeGoto(u64),
    UndoTreePreview(u64),
    UndoTreeCancel,
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
            .and_then(|p| p.parent().map(|p| p.join("grammars")))
            .unwrap_or_else(|| std::path::PathBuf::from("grammars"));

        let language_loader = Arc::new(crate::syntax::loader::LanguageLoader::new(grammar_dir));

        // Create document (either from file or empty)
        let next_id = 1;
        let mut document = if let Some(ref path) = file_path {
            // Try to load the file directly
            Document::from_file(next_id, path).map_err(|e| {
                RiftError::new(
                    ErrorType::Io,
                    "LOAD_FAILED",
                    format!("Failed to load file {path}: {e}"),
                )
            })?
        } else {
            Document::new(next_id)
                .map_err(|e| RiftError::new(ErrorType::Internal, "INTERNAL_ERROR", e.to_string()))?
        };

        // Try to load syntax for the document
        if let Some(path) = document.path() {
            if let Ok(loaded_lang) = language_loader.load_language_for_file(path) {
                let highlights = language_loader
                    .load_query(&loaded_lang.name, "highlights")
                    .ok();
                if let Ok(syntax) = crate::syntax::Syntax::new(loaded_lang, highlights) {
                    document.set_syntax(syntax);
                }
            }
        }

        // Initialize terminal (clears screen, enters raw mode, etc.)
        // We do this AFTER loading the document so we don't mess up the terminal
        // if loading fails
        terminal.init()?;

        // Get terminal size
        let size = terminal.get_size()?;

        // Create viewport
        let viewport = Viewport::new(size.rows as usize, size.cols as usize);

        // Create dispatcher
        let dispatcher = Dispatcher::new(Mode::Normal);

        // Create command registry and settings registry
        let settings_registry = create_settings_registry();
        let command_parser = CommandParser::new(settings_registry.clone());

        let mut state = State::new();
        state.set_file_path(file_path.clone());
        state.update_filename(document.display_name().to_string());

        let compositor = LayerCompositor::new(size.rows as usize, size.cols as usize);

        Ok(Self {
            term: terminal,
            render_cache: crate::render::RenderCache::default(),
            documents: HashMap::from([(next_id, document)]),
            tab_order: vec![next_id],
            current_tab: 0,
            next_document_id: next_id + 1,
            compositor,
            viewport,
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
        })
    }

    /// Get the ID of the active document
    pub fn active_document_id(&self) -> DocumentId {
        self.tab_order[self.current_tab]
    }

    /// Get mutable reference to the active document
    pub fn active_document(&mut self) -> &mut Document {
        let id = self.active_document_id();
        self.documents
            .get_mut(&id)
            .expect("Active document missing from storage")
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
        let buffer_size = self.active_document().buffer.get_before_gap().len()
            + self.active_document().buffer.get_after_gap().len();
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
        let Editor {
            term,
            compositor,
            render_cache,
            documents,
            tab_order,
            current_tab,
            viewport,
            state,
            current_mode,
            dispatcher,
            ..
        } = self;

        let doc_id = tab_order[*current_tab];
        let doc = documents.get_mut(&doc_id).unwrap();

        // Calculate visible range for syntax highlighting
        let start_line = viewport.top_line();
        let end_line = start_line + viewport.visible_rows();
        let start_byte = doc.buffer.line_index.get_start(start_line).unwrap_or(0);
        let end_byte = if end_line < doc.buffer.get_total_lines() {
            doc.buffer
                .line_index
                .get_start(end_line)
                .unwrap_or(doc.buffer.len())
        } else {
            doc.buffer.len()
        };

        let highlights = if let Some(syntax) = doc.syntax.as_mut() {
            Some(syntax.highlights(&doc.buffer, Some(start_byte..end_byte)))
        } else {
            None
        };

        let ctx = render::RenderContext {
            buf: &doc.buffer,
            viewport,
            state,
            current_mode: *current_mode,
            pending_key: dispatcher.pending_key(),
            pending_count: dispatcher.pending_count(),
            needs_clear: true,
            tab_width: doc.options.tab_width,
            highlights: highlights.as_deref(),
        };

        render::full_redraw(term, compositor, ctx, render_cache)
            .map_err(|e| RiftError::new(ErrorType::Io, "REDRAW_FAILED", e.to_string()))?;

        Ok(())
    }

    /// Remove a document by ID with strict tab semantics
    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        // 1. Check if document exists
        if !self.documents.contains_key(&id) {
            return Ok(());
        }

        // 2. Check dirty state
        if self.documents.get(&id).unwrap().is_dirty() {
            return Err(RiftError::warning(
                ErrorType::Execution,
                "UNSAVED_CHANGES",
                "No write since last change",
            ));
        }

        // 3. Find position in tab_order
        let pos = self
            .tab_order
            .iter()
            .position(|&x| x == id)
            .expect("Document in storage but not in tab_order");

        // 4. Update current_tab if necessary
        if self.tab_order.len() == 1 {
            // Closing last tab: auto-create new empty doc
            let new_id = self.next_document_id;
            self.next_document_id += 1;
            let new_doc = Document::new(new_id).map_err(|e| {
                RiftError::new(ErrorType::Internal, "INTERNAL_ERROR", e.to_string())
            })?;
            self.documents.insert(new_id, new_doc);
            self.tab_order.push(new_id);

            // Now remove the old one (it will be at pos 0)
            self.tab_order.remove(pos);
            self.documents.remove(&id);
            self.current_tab = 0;
        } else {
            // General case
            self.tab_order.remove(pos);
            self.documents.remove(&id);

            // Shift current_tab if we closed the active one OR if it's now
            // out of bounds
            if pos <= self.current_tab && self.current_tab > 0 {
                self.current_tab -= 1;
            }

            // Boundary check
            if self.current_tab >= self.tab_order.len() {
                self.current_tab = self.tab_order.len() - 1;
            }
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
        if let Some(path_str) = file_path {
            // Check if already open
            let path_buf = std::path::PathBuf::from(&path_str);
            let absolute_path = std::fs::canonicalize(&path_buf).unwrap_or(path_buf.clone());

            for (idx, &id) in self.tab_order.iter().enumerate() {
                if let Some(doc) = self.documents.get(&id) {
                    if let Some(doc_path) = doc.path() {
                        let doc_abs =
                            std::fs::canonicalize(doc_path).unwrap_or(doc_path.to_path_buf());
                        if doc_abs == absolute_path {
                            self.current_tab = idx;
                            self.sync_state_with_active_document();
                            return Ok(());
                        }
                    }
                }
            }

            // Not open, load it
            // Try to load existing file first
            let document_result = Document::from_file(self.next_document_id, &path_str);

            let document = match document_result {
                Ok(doc) => doc,
                Err(e) => {
                    // If file doesn't exist, create a new empty buffer (standard
                    // :edit behavior). For other errors (permission denied, is a
                    // directory, etc.), return the error
                    if e.kind == ErrorType::Io
                        && e.message
                            .contains("The system cannot find the file specified")
                    {
                        if std::path::Path::new(&path_str).exists() {
                            // File exists but we couldn't read it (AccessDenied,
                            // IsDir, etc.)
                            return Err(e);
                        } else {
                            // File doesn't exist, so we are creating a new one
                            let mut doc = Document::new(self.next_document_id)?;
                            doc.set_path(&path_str);
                            doc
                        }
                    } else {
                        return Err(e);
                    }
                }
            };

            let id = document.id;
            self.next_document_id += 1;
            self.documents.insert(id, document);
            self.tab_order.push(id);
            self.current_tab = self.tab_order.len() - 1;
            self.sync_state_with_active_document();
        } else {
            // Reload current file
            let (is_dirty, has_path) = {
                let doc = self.active_document();
                (doc.is_dirty(), doc.has_path())
            };

            if !force && is_dirty {
                return Err(RiftError {
                    severity: ErrorSeverity::Warning,
                    kind: ErrorType::Execution,
                    code: "UNSAVED_CHANGES".to_string(),
                    message: "No write since last change (add ! to override)".to_string(),
                });
            }

            if has_path {
                self.active_document().reload_from_disk()?;
                self.sync_state_with_active_document();
            } else {
                return Err(RiftError::new(
                    ErrorType::Execution,
                    "NO_PATH",
                    "No file name",
                ));
            }
        }
        Ok(())
    }

    /// Perform a search in the document
    fn perform_search(&mut self, query: &str, direction: SearchDirection, skip_current: bool) {
        let doc_id = self.tab_order[self.current_tab];

        // Calculate start cursor (scope to drop borrow)
        let cursor = {
            let doc = self.documents.get(&doc_id).unwrap();
            let mut c = doc.buffer.cursor();
            // If searching forward and skipping current, advance cursor to avoid
            // matching at current position
            if skip_current && direction == SearchDirection::Forward {
                c = c.saturating_add(1);
            }
            c
        };

        // Find all matches first to populate state for highlighting
        self.update_search_highlights();

        // Re-acquire mutable borrow for find_next and cursor update
        let doc = self.documents.get_mut(&doc_id).unwrap();

        match find_next(&doc.buffer, cursor, query, direction) {
            Ok(Some(m)) => {
                // Move cursor to start of match
                let _ = doc.buffer.set_cursor(m.range.start);
            }
            Ok(None) => {
                self.state.notify(
                    crate::notification::NotificationType::Info,
                    format!("Pattern not found: {}", query),
                );
            }
            Err(e) => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("Search error: {}", e),
                );
            }
        }
    }

    /// Run the editor main loop
    pub fn run(&mut self) -> Result<(), RiftError> {
        // Initial render
        self.update_and_render()?;

        // Main event loop
        while !self.should_quit {
            // Poll for input
            let timeout = self.state.settings.poll_timeout_ms;
            if self
                .term
                .poll(std::time::Duration::from_millis(timeout))
                .map_err(|e| RiftError::new(ErrorType::Internal, "POLL_FAILED", e))?
            {
                // Read key
                let key_press = match self.term.read_key()? {
                    Some(key) => key,
                    None => continue,
                };

                // Handle generic modal input
                let modal_result = self
                    .modal
                    .as_mut()
                    .map(|modal| modal.handle_input(key_press));

                if let Some(result) = modal_result {
                    use crate::component::EventResult;
                    match result {
                        EventResult::Consumed => continue,
                        EventResult::Ignored => continue,
                        EventResult::Action(any) => {
                            if let Some(action) = any.downcast_ref::<ComponentAction>() {
                                match action {
                                    ComponentAction::UndoTreeGoto(seq) => {
                                        let doc_id = self.tab_order[self.current_tab];
                                        let doc = self.documents.get_mut(&doc_id).unwrap();
                                        if let Err(e) = doc.goto_seq(*seq) {
                                            self.state.handle_error(RiftError::new(
                                                ErrorType::Execution,
                                                "UNDO_FAILED",
                                                format!("Failed to go to sequence {}: {}", seq, e),
                                            ));
                                        }
                                        // Close modal
                                        self.modal = None;
                                        use crate::layer::LayerPriority;
                                        self.compositor.clear_layer(LayerPriority::POPUP);
                                        self.set_mode(Mode::Normal);
                                        self.update_and_render()?;
                                    }
                                    ComponentAction::UndoTreeCancel => {
                                        self.modal = None;
                                        use crate::layer::LayerPriority;
                                        self.compositor.clear_layer(LayerPriority::POPUP);
                                        self.set_mode(Mode::Normal);
                                        self.update_and_render()?;
                                    }
                                    ComponentAction::UndoTreePreview(seq) => {
                                        let doc_id = self.tab_order[self.current_tab];
                                        let doc = self.documents.get(&doc_id).unwrap();
                                        if let Ok(preview_text) = doc.preview_at_seq(*seq) {
                                            use crate::layer::Cell;
                                            let mut content = Vec::new();
                                            for line in preview_text.lines() {
                                                let cells: Vec<Cell> =
                                                    line.chars().map(Cell::from_char).collect();
                                                content.push(cells);
                                            }

                                            if let Some(modal) = self.modal.as_mut() {
                                                if let Some(view) = modal
                                                    .as_any_mut()
                                                    .downcast_mut::<crate::select_view::SelectView>(
                                                ) {
                                                    view.set_right_content(content);
                                                }
                                            }
                                        }
                                        self.update_and_render()?;
                                    }
                                    ComponentAction::ExecuteCommand(cmd) => {
                                        // Sync legacy state for consistency
                                        self.state.command_line = cmd.clone();

                                        // Parse and execute the command
                                        let parsed_command = self.command_parser.parse(cmd);
                                        let doc_id = self.tab_order[self.current_tab];
                                        let execution_result = CommandExecutor::execute(
                                            parsed_command.clone(),
                                            &mut self.state,
                                            self.documents
                                                .get_mut(&doc_id)
                                                .expect("active document missing"),
                                            &self.settings_registry,
                                            &self.document_settings_registry,
                                        );

                                        self.handle_execution_result(execution_result);
                                        self.update_and_render()?;
                                    }
                                    ComponentAction::ExecuteSearch(query) => {
                                        self.modal = None;
                                        if !query.is_empty() {
                                            self.state.last_search_query = Some(query.clone());
                                            self.perform_search(
                                                query,
                                                SearchDirection::Forward,
                                                false,
                                            );
                                        }
                                        self.set_mode(Mode::Normal);
                                        self.update_and_render()?;
                                    }
                                    ComponentAction::CancelMode => {
                                        self.modal = None;
                                        self.set_mode(Mode::Normal);
                                        self.update_and_render()?;
                                    }
                                }
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
                    let viewport_height = self.viewport.visible_rows();
                    let doc_id = self.tab_order[self.current_tab];

                    // Wrap mutating commands (except Undo/Redo) in a transaction
                    // Skip wrapping in Insert mode - it already has an open
                    // transaction from mode entry
                    let needs_transaction = cmd.is_mutating()
                        && !matches!(cmd, Command::Undo | Command::Redo)
                        && current_mode != Mode::Insert;

                    let res = {
                        let doc = self.documents.get_mut(&doc_id).unwrap();
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
                let doc_id = self.tab_order[self.current_tab];
                self.documents
                    .get_mut(&doc_id)
                    .unwrap()
                    .commit_transaction();
                self.set_mode(Mode::Normal);
            }
            KeyAction::ExitCommandMode => {
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
                self.viewport.set_size(rows as usize, cols as usize);
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
                let doc_id = self.tab_order[self.current_tab];
                self.documents
                    .get_mut(&doc_id)
                    .unwrap()
                    .begin_transaction("Insert");
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAfter => {
                let doc_id = self.tab_order[self.current_tab];
                let doc = self.documents.get_mut(&doc_id).unwrap();
                doc.buffer.move_right();
                doc.begin_transaction("Insert");
                self.set_mode(Mode::Insert);
            }
            Command::EnterCommandMode => {
                self.set_mode(Mode::Command);
            }
            Command::EnterSearchMode => {
                self.set_mode(Mode::Search);
                self.state.clear_command_line();
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
                let doc_id = self.tab_order[self.current_tab];
                let execution_result = CommandExecutor::execute(
                    parsed_command.clone(),
                    &mut self.state,
                    self.documents
                        .get_mut(&doc_id)
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
        let doc_id = self.tab_order[self.current_tab];
        let total_lines = self
            .documents
            .get(&doc_id)
            .unwrap()
            .buffer
            .get_total_lines();
        let gutter_width = if self.state.settings.show_line_numbers {
            self.state.gutter_width
        } else {
            0
        };
        let needs_clear = self
            .viewport
            .update(cursor_line, cursor_col, total_lines, gutter_width);

        self.render(needs_clear)
    }

    /// Render the editor interface (pure read - no mutations)
    /// Uses the layer compositor for composited rendering
    pub fn render_to_terminal(&mut self, needs_clear: bool) -> Result<FrameStats, RiftError> {
        self.term.hide_cursor()?;
        let stats = self
            .compositor
            .render_to_terminal(&mut self.term, needs_clear)
            .map_err(|e| RiftError::new(ErrorType::Internal, "RENDER_FAILED", e))?;
        self.term.show_cursor()?;
        Ok(stats)
    }

    /// Render the editor interface (pure read - no mutations)
    /// Uses the layer compositor for composited rendering
    fn render(&mut self, needs_clear: bool) -> Result<(), RiftError> {
        let Editor {
            documents,
            tab_order,
            current_tab,
            viewport,
            state,
            current_mode,
            dispatcher,
            compositor,
            term,
            render_cache,
            ..
        } = self;

        let doc_id = tab_order[*current_tab];
        // We need mutable access to call syntax.highlights() which potentially
        // updates parse tree
        let doc = documents.get_mut(&doc_id).unwrap();

        // Calculate visible range for syntax highlighting optimization
        let start_line = viewport.top_line();
        let end_line = start_line + viewport.visible_rows();
        let start_byte = doc.buffer.line_index.get_start(start_line).unwrap_or(0);
        let end_byte = if end_line < doc.buffer.get_total_lines() {
            doc.buffer
                .line_index
                .get_start(end_line)
                .unwrap_or(doc.buffer.len())
        } else {
            doc.buffer.len()
        };

        let highlights = if let Some(syntax) = doc.syntax.as_mut() {
            Some(syntax.highlights(&doc.buffer, Some(start_byte..end_byte)))
        } else {
            None
        };

        let buf = &doc.buffer;

        let ctx = render::RenderContext {
            buf,
            viewport,
            state,
            current_mode: *current_mode,
            pending_key: dispatcher.pending_key(),
            pending_count: dispatcher.pending_count(),
            needs_clear,
            tab_width: doc.options.tab_width,
            highlights: highlights.as_deref(),
        };

        let _ = render::render(term, compositor, ctx, render_cache)?;

        // Render modal if active
        if let Some(ref mut modal) = self.modal {
            use crate::layer::LayerPriority;
            let layer = compositor.get_layer_mut(LayerPriority::POPUP);
            modal.render(layer);
            // Re-composite and render
            let _ = compositor.render_to_terminal(term, false)?;

            // Explicitly set cursor if modal requests it
            if let Some((row, col)) = modal.cursor_position() {
                term.move_cursor(row, col)?;
            }
        }

        Ok(())
    }

    /// Set editor mode and update dispatcher
    fn set_mode(&mut self, mode: Mode) {
        self.current_mode = mode;
        self.dispatcher.set_mode(mode);

        match mode {
            Mode::Command => {
                let settings = self.state.settings.command_line_window.clone();
                self.modal = Some(Box::new(
                    crate::command_line::component::CommandLineComponent::new(':', settings),
                ));
            }
            Mode::Search => {
                let settings = self.state.settings.command_line_window.clone();
                self.modal = Some(Box::new(
                    crate::command_line::component::CommandLineComponent::new('/', settings),
                ));
            }
            _ => {}
        }
    }

    /// Save document to file
    ///
    /// Returns error message string if save fails
    fn save_document(&mut self) -> Result<(), RiftError> {
        use std::path::PathBuf;

        // Get file path from state (executor may have updated it)
        let file_path = self
            .state
            .file_path
            .clone()
            .ok_or_else(|| RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"))?;

        // Update document path if it changed in state
        {
            let doc = self.active_document();
            if doc.path() != Some(std::path::Path::new(&file_path)) {
                doc.set_path(PathBuf::from(&file_path));
            }
        }

        // Save document
        let res = {
            let doc = self.active_document();
            if doc.has_path() {
                doc.save().map_err(|e| {
                    RiftError::new(
                        ErrorType::Io,
                        "SAVE_FAILED",
                        format!("Failed to write {file_path}: {e}"),
                    )
                })
            } else {
                Err(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"))
            }
        };

        if res.is_ok() {
            // Update cached filename after save (handles save_as case)
            let display_name = self.active_document().display_name().to_string();
            self.state.update_filename(display_name);
        }
        res
    }

    pub fn term_mut(&mut self) -> &mut T {
        &mut self.term
    }

    // Handle execution results from command_line commands
    fn handle_execution_result(&mut self, execution_result: ExecutionResult) {
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
                    self.state.clear_command_line();
                    self.modal = None;
                    self.set_mode(Mode::Normal);
                }
            }
            ExecutionResult::Write => {
                // Handle write command - save if file path exists
                if self.state.file_path.is_some() && self.active_document().is_dirty() {
                    if let Err(e) = self.save_document() {
                        self.state.handle_error(e);
                    } else {
                        let filename = self.state.file_name.clone();
                        self.state.notify(
                            crate::notification::NotificationType::Success,
                            format!("Written to {filename}"),
                        );
                    }
                }
                self.state.clear_command_line();
                self.modal = None;
                use crate::layer::LayerPriority;
                self.compositor.clear_layer(LayerPriority::POPUP);
                self.set_mode(Mode::Normal);
            }
            ExecutionResult::WriteAndQuit => {
                // Save document, then quit if successful
                if let Err(e) = self.save_document() {
                    self.state.handle_error(e);
                } else {
                    let filename = self.state.file_name.clone();
                    self.state.notify(
                        crate::notification::NotificationType::Success,
                        format!("Written to {filename}"),
                    );
                }
                // Save successful, now quit
                self.should_quit = true;
                self.state.clear_command_line();
                self.modal = None;
                use crate::layer::LayerPriority;
                self.compositor.clear_layer(LayerPriority::POPUP);
                self.set_mode(Mode::Normal);
            }
            ExecutionResult::Failure => {
                // Error already reported by executor to state/notification
                // manager Keep command line visible so user can see it
            }
            ExecutionResult::Redraw => {
                // Close command line first before redraw
                self.state.clear_command_line();
                self.modal = None;
                self.set_mode(Mode::Normal);

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
                    self.modal = None;
                    self.set_mode(Mode::Normal);
                    // Force redraw after opening a file
                    if let Err(e) = self.force_full_redraw() {
                        self.state.handle_error(e);
                    }
                }
            }
            ExecutionResult::BufferNext { bangs } => {
                if self.tab_order.len() > 1 {
                    if bangs > 0 {
                        // Go to last buffer
                        self.current_tab = self.tab_order.len() - 1;
                    } else {
                        // Go to next buffer
                        self.current_tab = (self.current_tab + 1) % self.tab_order.len();
                    }
                }
                self.sync_state_with_active_document();
                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
                if let Err(e) = self.force_full_redraw() {
                    self.state.handle_error(e);
                }
            }
            ExecutionResult::BufferPrevious { bangs } => {
                if self.tab_order.len() > 1 {
                    if bangs > 0 {
                        // Go to first buffer
                        self.current_tab = 0;
                    } else {
                        // Go to previous buffer
                        self.current_tab =
                            (self.current_tab + self.tab_order.len() - 1) % self.tab_order.len();
                    }
                }
                self.sync_state_with_active_document();
                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
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
                self.set_mode(Mode::Normal);
            }
            ExecutionResult::BufferList => {
                let mut message = String::new();
                for (i, doc_id) in self.tab_order.iter().enumerate() {
                    let doc = self.documents.get(doc_id).unwrap();
                    let name = doc.display_name();
                    let dirty = if doc.is_dirty() { "+" } else { " " };
                    let read_only = if doc.is_read_only { "R" } else { " " };
                    let current = if i == self.current_tab { "%" } else { " " };
                    if !message.is_empty() {
                        message.push('\n');
                    }
                    message.push_str(&format!(
                        "[{}] {}: {}{}{}",
                        i + 1,
                        name,
                        current,
                        dirty,
                        read_only
                    ));
                }

                self.state
                    .notify(crate::notification::NotificationType::Info, message);
                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
            }
            ExecutionResult::Success => {
                self.state.clear_command_line();
                self.modal = None;
                self.set_mode(Mode::Normal);
            }
            ExecutionResult::Undo { count } => {
                let doc_id = self.tab_order[self.current_tab];
                let doc = self.documents.get_mut(&doc_id).unwrap();
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
                self.set_mode(Mode::Normal);
                self.update_search_highlights();
            }
            ExecutionResult::Redo { count } => {
                let doc_id = self.tab_order[self.current_tab];
                let doc = self.documents.get_mut(&doc_id).unwrap();
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
                self.set_mode(Mode::Normal);
                self.update_search_highlights();
            }
            ExecutionResult::UndoGoto { seq } => {
                let doc_id = self.tab_order[self.current_tab];
                let doc = self.documents.get_mut(&doc_id).unwrap();
                match doc.goto_seq(seq) {
                    Ok(()) => {
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            format!("Jumped to edit #{}", seq),
                        );
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
                self.set_mode(Mode::Normal);
                self.update_search_highlights();
            }
            ExecutionResult::Checkpoint => {
                // Already handled in executor
                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
            }
            ExecutionResult::UndoTree { content } => {
                use crate::component::EventResult;
                use crate::select_view::SelectView;

                let mut view = SelectView::new().with_left_width(content.left_width_percent);
                view.set_left_content(content.left);
                view.set_right_content(content.right);
                view.set_selected_line(Some(content.cursor));
                let view = view.with_selectable(content.selectable.clone());

                let sequences = content.sequences;
                let seqs_select = sequences.clone();
                let seqs_change = sequences.clone();

                let mut view = view
                    .on_select(move |idx| {
                        if let Some(&seq) = seqs_select.get(idx) {
                            if seq != crate::history::EditSeq::MAX {
                                return EventResult::Action(Box::new(
                                    ComponentAction::UndoTreeGoto(seq),
                                ));
                            }
                        }
                        EventResult::Consumed
                    })
                    .on_change(move |idx| {
                        if let Some(&seq) = seqs_change.get(idx) {
                            if seq != crate::history::EditSeq::MAX {
                                return EventResult::Action(Box::new(
                                    ComponentAction::UndoTreePreview(seq),
                                ));
                            }
                        }
                        EventResult::Consumed
                    })
                    .on_cancel(|| EventResult::Action(Box::new(ComponentAction::UndoTreeCancel)));

                // Trigger initial preview
                let doc_id = self.tab_order[self.current_tab];
                let doc = self.documents.get(&doc_id).unwrap();
                if let Some(&seq) = sequences.get(content.cursor) {
                    if seq != crate::history::EditSeq::MAX {
                        if let Ok(preview_text) = doc.preview_at_seq(seq) {
                            use crate::layer::Cell;
                            let mut preview_content = Vec::new();
                            for line in preview_text.lines() {
                                let cells: Vec<Cell> = line.chars().map(Cell::from_char).collect();
                                preview_content.push(cells);
                            }
                            view.set_right_content(preview_content);
                        }
                    }
                }

                self.modal = Some(Box::new(view));
                self.state.clear_command_line();
                self.set_mode(Mode::Overlay);
            }
        }
    }

    /// Update search highlights based on current buffer state
    fn update_search_highlights(&mut self) {
        if let Some(query) = self.state.last_search_query.clone() {
            let doc_id = self.tab_order[self.current_tab];
            let doc = self.documents.get_mut(&doc_id).unwrap();
            match crate::search::find_all(&doc.buffer, &query) {
                Ok(matches) => {
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
