//! Editor core
//! Main editor logic that ties everything together

use crate::command::{Command, Dispatcher};
use crate::command_line::executor::{CommandExecutor, ExecutionResult};
use crate::command_line::parser::CommandParser;
use crate::command_line::registry::{CommandDef, CommandRegistry};
use crate::command_line::settings::{create_settings_registry, SettingsRegistry};
use crate::document::Document;
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::executor::execute_command;
use crate::key_handler::{KeyAction, KeyHandler};
use crate::layer::LayerCompositor;
use crate::mode::Mode;
use crate::render;
use crate::state::State;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;

/// Main editor struct
pub struct Editor<T: TerminalBackend> {
    terminal: T,
    document: Document,
    compositor: LayerCompositor,
    viewport: Viewport,
    dispatcher: Dispatcher,
    current_mode: Mode,
    should_quit: bool,
    state: State,
    command_parser: CommandParser,
    settings_registry: SettingsRegistry,
}

impl<T: TerminalBackend> Editor<T> {
    /// Create a new editor instance
    pub fn new(terminal: T) -> Result<Self, RiftError> {
        Self::with_file(terminal, None)
    }

    /// Create a new editor instance with an optional file to load
    pub fn with_file(mut terminal: T, file_path: Option<String>) -> Result<Self, RiftError> {
        // Validate file BEFORE initializing terminal or creating buffer
        // This ensures we don't clear the screen or allocate resources if the file is invalid
        if let Some(ref path) = file_path {
            Self::validate_file(path)?;
        }

        // Initialize terminal (clears screen, enters raw mode, etc.)
        terminal.init()?;

        // Get terminal size
        let size = terminal.get_size()?;

        // Create document (either from file or empty)
        let document = if let Some(ref path) = file_path {
            Document::from_file(1, path).map_err(|e| {
                RiftError::new(
                    ErrorType::Io,
                    "LOAD_FAILED",
                    format!("Failed to load file {path}: {e}"),
                )
            })?
        } else {
            Document::new(1)
                .map_err(|e| RiftError::new(ErrorType::Internal, "INTERNAL_ERROR", e.to_string()))?
        };

        // Create viewport
        let viewport = Viewport::new(size.rows as usize, size.cols as usize);

        // Create dispatcher
        let dispatcher = Dispatcher::new(Mode::Normal);

        // Create command registry and settings registry
        let registry = CommandRegistry::new()
            .register(CommandDef::new("quit").with_alias("q"))
            .register(CommandDef::new("set").with_alias("se"))
            .register(CommandDef::new("write").with_alias("w"))
            .register(CommandDef::new("write").with_alias("w"))
            .register(CommandDef::new("wq"))
            .register(CommandDef::new("notify"));
        let settings_registry = create_settings_registry();
        let command_parser = CommandParser::new(registry, settings_registry);

        let mut state = State::new();
        state.set_file_path(file_path.clone());
        // Initialize filename from document
        state.update_filename(document.display_name().to_string());

        // Create layer compositor for layer-based rendering
        let compositor = LayerCompositor::new(size.rows as usize, size.cols as usize);

        Ok(Editor {
            terminal,
            document,
            compositor,
            viewport,
            dispatcher,
            current_mode: Mode::Normal,
            should_quit: false,
            state,
            command_parser,
            settings_registry,
        })
    }

    /// Validate that a file exists and is a valid file (not a directory)
    /// This should be called BEFORE terminal initialization to avoid clearing the screen
    /// if the file is invalid.
    fn validate_file(file_path: &str) -> Result<(), RiftError> {
        use std::path::Path;

        let path = Path::new(file_path);

        // Check if file exists
        if !path.exists() {
            return Err(RiftError::new(
                ErrorType::Io,
                "FILE_NOT_FOUND",
                format!("File not found: {file_path}"),
            ));
        }

        // Check if it's a file (not a directory)
        if !path.is_file() {
            return Err(RiftError::new(
                ErrorType::Io,
                "NOT_A_FILE",
                format!("Path is not a file: {file_path}"),
            ));
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
            let key_press = self.terminal.read_key()?;

            // Process keypress through key handler
            let action = KeyHandler::process_key(key_press, self.current_mode);

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

            // Execute command if it affects the buffer
            let should_execute_buffer = !matches!(
                cmd,
                Command::EnterInsertMode
                    | Command::EnterCommandMode
                    | Command::AppendToCommandLine(_)
                    | Command::DeleteFromCommandLine
                    | Command::ExecuteCommandLine
                    | Command::Quit
                    | Command::Noop
            );

            if should_execute_buffer {
                if let Err(e) = execute_command(
                    cmd,
                    &mut self.document.buffer,
                    self.state.settings.expand_tabs,
                    self.state.settings.tab_width,
                ) {
                    self.state.handle_error(e);
                }
                // Mark document dirty after any buffer mutation (even if part of command failed)
                self.document.mark_dirty();
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
                self.document.buffer.move_right();
                self.set_mode(Mode::Insert);
            }
            Command::EnterCommandMode => {
                self.set_mode(Mode::Command);
            }
            Command::ExecuteCommandLine => {
                // Parse and execute the command
                let command_line = self.state.command_line.clone();
                let parsed_command = self.command_parser.parse(&command_line);
                let execution_result = CommandExecutor::execute(
                    parsed_command,
                    &mut self.state,
                    &self.settings_registry,
                );

                // Handle execution result
                match execution_result {
                    ExecutionResult::Quit { bangs } => {
                        if self.document.is_dirty() && bangs == 0 {
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
                        if self.state.file_path.is_some() && self.document.is_dirty() {
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
                }
            }
            _ => {}
        }

        // Handle command line editing (mutations happen here)
        match command {
            Command::AppendToCommandLine(ch) => {
                // ch is guaranteed to be valid ASCII (32-126) from translate_command_mode
                self.state.append_to_command_line(ch as char);
            }
            Command::DeleteFromCommandLine => {
                self.state.remove_from_command_line();
            }
            _ => {}
        }

        // Update input tracking (happens during state update, not input handling)
        self.state.update_keypress(keypress);
        self.state.update_command(command);

        // Update buffer and cursor state
        let cursor_line = self.document.buffer.get_line();
        let cursor_col = render::calculate_cursor_column(
            &self.document.buffer,
            cursor_line,
            self.state.settings.tab_width,
        );
        self.state.update_cursor(cursor_line, cursor_col);

        let total_lines = self.document.buffer.get_total_lines();
        let buffer_size = self.document.buffer.get_before_gap().len()
            + self.document.buffer.get_after_gap().len();
        self.state.update_buffer_stats(total_lines, buffer_size);
        self.state.error_manager.notifications_mut().prune_expired();

        // Update viewport based on cursor position (state mutation happens here)
        let needs_clear = self.viewport.update(cursor_line, total_lines);

        // Render (pure read - no mutations)
        self.render(needs_clear)
    }

    /// Update state and render the editor (for initial render)
    fn update_and_render(&mut self) -> Result<(), RiftError> {
        // Update buffer and cursor state only (no input tracking on initial render)
        let cursor_line = self.document.buffer.get_line();
        let cursor_col = render::calculate_cursor_column(
            &self.document.buffer,
            cursor_line,
            self.state.settings.tab_width,
        );
        self.state.update_cursor(cursor_line, cursor_col);

        let total_lines = self.document.buffer.get_total_lines();
        let buffer_size = self.document.buffer.get_before_gap().len()
            + self.document.buffer.get_after_gap().len();
        self.state.update_buffer_stats(total_lines, buffer_size);

        // Update viewport based on cursor position (state mutation happens here)
        let needs_clear = self.viewport.update(cursor_line, total_lines);

        self.render(needs_clear)
    }

    /// Render the editor interface (pure read - no mutations)
    /// Uses the layer compositor for composited rendering
    fn render(&mut self, needs_clear: bool) -> Result<(), RiftError> {
        render::render(
            &mut self.terminal,
            &mut self.compositor,
            render::RenderContext {
                buf: &self.document.buffer,
                viewport: &self.viewport,
                current_mode: self.current_mode,
                pending_key: self.dispatcher.pending_key(),
                state: &self.state,
                needs_clear,
            },
        )?;
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
            .as_ref()
            .ok_or_else(|| RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"))?;

        // Update document path if it changed in state
        if self.document.path() != Some(std::path::Path::new(file_path)) {
            self.document.set_path(PathBuf::from(file_path));
        }

        // Save document
        if self.document.has_path() {
            self.document.save().map_err(|e| {
                RiftError::new(
                    ErrorType::Io,
                    "SAVE_FAILED",
                    format!("Failed to write {file_path}: {e}"),
                )
            })?;

            // Update cached filename after save (handles save_as case)
            self.state
                .update_filename(self.document.display_name().to_string());
        } else {
            return Err(RiftError::new(ErrorType::Io, "NO_FILENAME", "No file name"));
        }

        Ok(())
    }
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.terminal.deinit();
    }
}
