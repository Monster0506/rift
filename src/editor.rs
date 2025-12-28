//! Editor core
//! Main editor logic that ties everything together

use crate::command::{Command, Dispatcher};
use crate::command_line::executor::{CommandExecutor, ExecutionResult};
use crate::command_line::parser::CommandParser;
use crate::command_line::registry::{CommandDef, CommandRegistry};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use crate::document::{Document, DocumentId};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::executor::execute_command;
use crate::key_handler::{KeyAction, KeyHandler};
use crate::layer::LayerCompositor;
use crate::mode::Mode;
use crate::render;
use crate::screen_buffer::FrameStats;
use crate::state::{State, UserSettings};
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use std::collections::HashMap;

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
}

impl<T: TerminalBackend> Editor<T> {
    /// Create a new editor instance
    pub fn new(terminal: T) -> Result<Self, RiftError> {
        Self::with_file(terminal, None)
    }

    /// Create a new editor instance with an optional file to load
    pub fn with_file(mut terminal: T, file_path: Option<String>) -> Result<Self, RiftError> {
        // Validate file BEFORE initializing terminal or creating buffer
        // We attempt to load the file directly in the document creation block below

        // Create document (either from file or empty)
        let next_id = 1;
        let document = if let Some(ref path) = file_path {
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

        // Initialize terminal (clears screen, enters raw mode, etc.)
        // We do this AFTER loading the document so we don't mess up the terminal if loading fails
        terminal.init()?;

        // Get terminal size
        let size = terminal.get_size()?;

        // Create viewport
        let viewport = Viewport::new(size.rows as usize, size.cols as usize);

        // Create dispatcher
        let dispatcher = Dispatcher::new(Mode::Normal);

        // Create command registry and settings registry
        let registry = CommandRegistry::new()
            .register(CommandDef::new("quit").with_alias("q"))
            .register(CommandDef::new("set"))
            .register(CommandDef::new("setlocal"))
            .register(CommandDef::new("write").with_alias("w"))
            .register(CommandDef::new("wq"))
            .register(CommandDef::new("notify"))
            .register(CommandDef::new("redraw"))
            .register(CommandDef::new("edit").with_alias("e"));
        let settings_registry = create_settings_registry();
        let command_parser = CommandParser::new(registry.clone(), settings_registry.clone());

        let mut state = State::new();
        state.set_file_path(file_path.clone());
        // Initialize filename from document
        state.update_filename(document.display_name().to_string());

        // Create layer compositor for layer-based rendering
        let compositor = LayerCompositor::new(size.rows as usize, size.cols as usize);

        // Create document settings registry
        use crate::document::definitions::create_document_settings_registry;
        let document_settings_registry = create_document_settings_registry();

        let mut documents = HashMap::new();
        documents.insert(document.id, document);
        let tab_order = vec![next_id];

        Ok(Editor {
            term: terminal,
            render_cache: crate::render::RenderCache::default(),
            documents,
            tab_order,
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
            document_settings_registry,
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
        let ctx = render::RenderContext {
            buf: &documents.get(&doc_id).unwrap().buffer,
            viewport,
            state,
            current_mode: *current_mode,
            pending_key: dispatcher.pending_key(),
            needs_clear: true,
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

            // Shift current_tab if we closed the active one OR if it's now out of bounds
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
    /// If file_path is Some, it opens that file (or creates a new document for it if not found).
    /// If file_path is None, it reloads the current active document.
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
                    // If file doesn't exist, create a new empty buffer (standard :edit behavior)
                    // For other errors (permission denied, is a directory, etc.), return the error
                    if e.kind == ErrorType::Io
                        && e.message
                            .contains("The system cannot find the file specified")
                    {
                        if std::path::Path::new(&path_str).exists() {
                            // File exists but we couldn't read it (AccessDenied, IsDir, etc.)
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

    /// Run the editor main loop
    pub fn run(&mut self) -> Result<(), RiftError> {
        // Initial render
        self.update_and_render()?;

        // Main event loop
        while !self.should_quit {
            // ============================================================
            // INPUT HANDLING PHASE (Pure - no mutations)
            // ============================================================

            // Read key
            let key_press = self.term.read_key()?;

            // Process keypress through key handler
            let current_mode = self.current_mode;
            let action = KeyHandler::process_key(key_press, current_mode);

            // Translate key to command (skip if action indicates special handling)
            let cmd = match action {
                KeyAction::ExitInsertMode | KeyAction::ExitCommandMode | KeyAction::ToggleDebug => {
                    // Skip command translation for special actions
                    Command::Noop
                }
                _ => self.dispatcher.translate_key(key_press),
            };

            // ============================================================
            // COMMAND EXECUTION PHASE (Buffer mutations only)
            // ============================================================

            // Execute command if it affects the buffer (and not in command mode)
            let should_execute_buffer = current_mode != Mode::Command
                && !matches!(
                    cmd,
                    Command::EnterInsertMode
                        | Command::EnterCommandMode
                        | Command::AppendToCommandLine(_)
                        | Command::DeleteFromCommandLine
                        | Command::ExecuteCommandLine
                        | Command::Quit
                        | Command::Noop
                        | Command::BufferNext
                        | Command::BufferPrevious
                );

            if should_execute_buffer {
                let expand_tabs = self.state.settings.expand_tabs;
                let tab_width = self.state.settings.tab_width;
                let doc_id = self.tab_order[self.current_tab];
                let res = {
                    let doc = self.documents.get_mut(&doc_id).unwrap();
                    execute_command(cmd, &mut doc.buffer, expand_tabs, tab_width)
                };
                if let Err(e) = res {
                    self.state.handle_error(e);
                }
                if cmd.is_mutating() {
                    // Mark document dirty after a mutating command
                    self.documents.get_mut(&doc_id).unwrap().mark_dirty();
                }
            }

            // Handle quit command (special case - exits loop)
            if cmd == Command::Quit {
                self.should_quit = true;
                continue;
            }

            // ============================================================
            // STATE UPDATE PHASE (All state mutations happen here)
            // ============================================================

            self.update_state_and_render(key_press, action, cmd)?;
        }

        Ok(())
    }

    /// Update editor state and render
    /// This is where ALL state mutations happen - input handling phase is pure
    fn update_state_and_render(
        &mut self,
        keypress: crate::key::Key,
        action: crate::key_handler::KeyAction,
        command: crate::command::Command,
    ) -> Result<(), RiftError> {
        // Handle special actions (mutations happen here, not during input handling)
        match action {
            KeyAction::ExitInsertMode => {
                self.set_mode(Mode::Normal);
            }
            KeyAction::ExitCommandMode => {
                self.state.clear_command_line();
                self.set_mode(Mode::Normal);
            }
            KeyAction::ToggleDebug => {
                self.state.toggle_debug();
            }
            KeyAction::SkipAndRender | KeyAction::Continue => {
                // No special action needed
            }
        }

        // Handle mode transitions from commands (mutations happen here)
        match command {
            Command::EnterInsertMode => {
                self.set_mode(Mode::Insert);
            }
            Command::EnterInsertModeAfter => {
                let doc_id = self.tab_order[self.current_tab];
                self.documents.get_mut(&doc_id).unwrap().buffer.move_right();
                self.set_mode(Mode::Insert);
            }
            Command::EnterCommandMode => {
                self.set_mode(Mode::Command);
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

                // Handle execution result
                match execution_result {
                    ExecutionResult::Quit { bangs } => {
                        if self.active_document().is_dirty() && bangs == 0 {
                            self.state.handle_error(RiftError {
                                severity: ErrorSeverity::Warning,
                                kind: ErrorType::Execution,
                                code: "UNSAVED_CHANGES".to_string(),
                                message: "No write since last change (add ! to override)"
                                    .to_string(),
                            });
                        } else {
                            self.should_quit = true;
                            self.state.clear_command_line();
                            self.set_mode(Mode::Normal);
                        }
                    }
                    ExecutionResult::Success => {
                        // Handle write command - save if file path exists
                        if self.state.file_path.is_some() && self.active_document().is_dirty() {
                            if let Err(e) = self.save_document() {
                                self.state.handle_error(e);
                                return Ok(()); // Don't clear command line on error
                            } else {
                                let filename = self.state.file_name.clone();
                                self.state.notify(
                                    crate::notification::NotificationType::Success,
                                    format!("Written to {filename}"),
                                );
                            }
                        }
                        self.state.clear_command_line();
                        self.set_mode(Mode::Normal);
                    }
                    ExecutionResult::WriteAndQuit => {
                        // Save document, then quit if successful
                        if let Err(e) = self.save_document() {
                            self.state.handle_error(e);
                            return Ok(()); // Don't quit on save error
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
                        self.set_mode(Mode::Normal);
                    }
                    ExecutionResult::Failure => {
                        // Error already reported by executor to state/notification manager
                        // Keep command line visible so user can see it
                    }
                    ExecutionResult::Redraw => {
                        // Close command line first before redraw
                        self.state.clear_command_line();
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
                            self.set_mode(Mode::Normal);
                            // Force redraw after opening a file
                            if let Err(e) = self.force_full_redraw() {
                                self.state.handle_error(e);
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Handle command line editing (mutations happen here)
        // Handle command line editing (mutations happen here)
        match command {
            Command::AppendToCommandLine(ch) => {
                // ch is guaranteed to be valid ASCII (32-126) from translate_command_mode
                self.state.append_to_command_line(ch as char);
            }
            Command::DeleteFromCommandLine => {
                self.state.remove_from_command_line();
            }
            Command::MoveLeft if self.current_mode == Mode::Command => {
                self.state.move_command_line_left();
            }
            Command::MoveRight if self.current_mode == Mode::Command => {
                self.state.move_command_line_right();
            }
            Command::MoveToLineStart if self.current_mode == Mode::Command => {
                self.state.move_command_line_home();
            }
            Command::MoveToLineEnd if self.current_mode == Mode::Command => {
                self.state.move_command_line_end();
            }
            Command::DeleteForward if self.current_mode == Mode::Command => {
                self.state.delete_forward_command_line();
            }
            _ => {}
        }

        // Update input tracking (happens during state update, not input handling)
        self.state.update_keypress(keypress);
        self.state.update_command(command);

        // Update buffer and cursor state
        let doc_id = self.tab_order[self.current_tab];
        let cursor_line = self.documents.get(&doc_id).unwrap().buffer.get_line();
        let tab_width = self.state.settings.tab_width;
        let cursor_col = render::calculate_cursor_column(
            &self.documents.get(&doc_id).unwrap().buffer,
            cursor_line,
            tab_width,
        );
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

        // Render (pure read - no mutations)
        self.render(needs_clear)
    }

    /// Update state and render the editor (for initial render)
    pub fn update_and_render(&mut self) -> Result<(), RiftError> {
        // Update buffer and cursor state only (no input tracking on initial render)
        let tab_width = self.state.settings.tab_width;
        let cursor_line = self.active_document().buffer.get_line();
        let cursor_col =
            render::calculate_cursor_column(&self.active_document().buffer, cursor_line, tab_width);
        self.state.update_cursor(cursor_line, cursor_col);

        self.sync_state_with_active_document();

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
        let buf = &documents.get(&doc_id).unwrap().buffer;

        let ctx = render::RenderContext {
            buf,
            viewport,
            state,
            current_mode: *current_mode,
            pending_key: dispatcher.pending_key(),
            needs_clear,
        };

        let _ = render::render(term, compositor, ctx, render_cache)?;
        Ok(())
    }

    /// Set editor mode and update dispatcher
    fn set_mode(&mut self, mode: Mode) {
        self.current_mode = mode;
        self.dispatcher.set_mode(mode);
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
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.term.deinit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockTerminal;

    fn create_editor() -> Editor<MockTerminal> {
        let term = MockTerminal::new(24, 80);
        Editor::new(term).unwrap()
    }

    #[test]
    fn test_editor_initial_state() {
        let editor = create_editor();
        assert_eq!(editor.documents.len(), 1);
        assert_eq!(editor.tab_order.len(), 1);
        assert_eq!(editor.current_tab, 0);
    }

    #[test]
    fn test_editor_remove_last_tab() {
        let mut editor = create_editor();
        let doc_id = editor.tab_order[0];

        // Removing the only tab should create a new empty one
        let result = editor.remove_document(doc_id);
        assert!(result.is_ok());
        assert_eq!(editor.documents.len(), 1);
        assert_eq!(editor.tab_order.len(), 1);
        assert_ne!(editor.tab_order[0], doc_id, "Should have a new doc ID");
    }

    #[test]
    fn test_editor_remove_dirty_tab() {
        let mut editor = create_editor();
        editor.active_document().mark_dirty();
        let doc_id = editor.tab_order[0];

        // Removing a dirty tab should return a warning
        let result = editor.remove_document(doc_id);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.severity, ErrorSeverity::Warning);
    }

    #[test]
    fn test_editor_open_file() {
        let mut editor = create_editor();
        // Open a new "file" (doesn't exist on disk, should create empty buffer)
        editor
            .open_file(Some("new_file.txt".to_string()), false)
            .unwrap();

        assert_eq!(editor.tab_order.len(), 2);
        assert_eq!(editor.current_tab, 1);
        assert_eq!(editor.active_document().display_name(), "new_file.txt");

        // Open same file again, should just switch
        editor
            .open_file(Some("new_file.txt".to_string()), false)
            .unwrap();
        assert_eq!(editor.tab_order.len(), 2);
        assert_eq!(editor.current_tab, 1);
    }
}
