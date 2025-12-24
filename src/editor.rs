//! Editor core
//! Main editor logic that ties everything together

use crate::buffer::GapBuffer;
use crate::command::{Command, Dispatcher};
use crate::executor::execute_command;
use crate::mode::Mode;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use crate::render;
use crate::state::State;
use crate::key_handler::{KeyHandler, KeyAction};

/// Main editor struct
pub struct Editor<T: TerminalBackend> {
    terminal: T,
    buf: GapBuffer,
    viewport: Viewport,
    dispatcher: Dispatcher,
    current_mode: Mode,
    should_quit: bool,
    state: State,
}

impl<T: TerminalBackend> Editor<T> {
    /// Create a new editor instance
    pub fn new(terminal: T) -> Result<Self, String> {
        Self::with_file(terminal, None)
    }

    /// Create a new editor instance with an optional file to load
    pub fn with_file(mut terminal: T, file_path: Option<String>) -> Result<Self, String> {
        // Validate file BEFORE initializing terminal or creating buffer
        // This ensures we don't clear the screen or allocate resources if the file is invalid
        if let Some(ref path) = file_path {
            Self::validate_file(path)?;
        }
        
        // Initialize terminal (clears screen, enters raw mode, etc.)
        terminal.init()?;
        
        // Get terminal size
        let size = terminal.get_size()?;
        
        // Create buffer with larger initial capacity for file loading
        let mut buf = GapBuffer::new(4096)
            .map_err(|e| format!("Failed to create buffer: {}", e))?;
        
        // Load file if provided (already validated above)
        if let Some(ref path) = file_path {
            Self::load_file_into_buffer(&mut buf, path)?;
        }
        
        // Create viewport
        let viewport = Viewport::new(size.rows as usize, size.cols as usize);
        
        // Create dispatcher
        let dispatcher = Dispatcher::new(Mode::Normal);
        
        let mut state = State::new();
        state.set_file_path(file_path);
        
        Ok(Editor {
            terminal,
            buf,
            viewport,
            dispatcher,
            current_mode: Mode::Normal,
            should_quit: false,
            state,
        })
    }

    /// Validate that a file exists and is a valid file (not a directory)
    /// This should be called BEFORE terminal initialization to avoid clearing the screen
    /// if the file is invalid.
    fn validate_file(file_path: &str) -> Result<(), String> {
        use std::path::Path;
        
        let path = Path::new(file_path);
        
        // Check if file exists
        if !path.exists() {
            return Err(format!("File not found: {}", file_path));
        }
        
        // Check if it's a file (not a directory)
        if !path.is_file() {
            return Err(format!("Path is not a file: {}", file_path));
        }
        
        Ok(())
    }

    /// Load file contents into the buffer
    /// File should already be validated before calling this function
    fn load_file_into_buffer(buf: &mut GapBuffer, file_path: &str) -> Result<(), String> {
        use std::fs;
        use std::path::Path;
        
        let path = Path::new(file_path);
        
        // Read file contents as bytes (preserves all data, including invalid UTF-8)
        let contents = fs::read(path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

        // Insert contents into buffer using batch insertion
        buf.insert_bytes(&contents)
            .map_err(|e| format!("Failed to load file into buffer: {}", e))?;
        
        // Move cursor to start of buffer
        buf.move_to_start();
        
        Ok(())
    }

    /// Run the editor main loop
    pub fn run(&mut self) -> Result<(), String> {
        // Initial render
        self.update_and_render()?;

        // Main event loop
        while !self.should_quit {
            // Read key
            let key_press = self.terminal.read_key()?;
            
            // Update state with last keypress
            self.state.update_keypress(key_press);

            // Process keypress through key handler
            let action = KeyHandler::process_key(key_press, self.current_mode);

            // Handle special actions that skip command processing
            match action {
                KeyAction::ExitInsertMode => {
                    self.set_mode(Mode::Normal);
                    self.update_and_render()?;
                    continue;
                }
                KeyAction::ExitCommandMode => {
                    self.state.clear_command_line();
                    self.set_mode(Mode::Normal);
                    self.update_and_render()?;
                    continue;
                }
                KeyAction::ToggleDebug => {
                    self.state.toggle_debug();
                    self.update_and_render()?;
                    continue;
                }
                KeyAction::SkipAndRender => {
                    self.update_and_render()?;
                    continue;
                }
                KeyAction::Continue => {
                    // Continue to command processing
                }
            }

            // Translate key to command
            let cmd = self.dispatcher.translate_key(key_press);
            
            // Track command in state for debug display
            self.state.update_command(cmd);

            // Handle mode transitions
            match cmd {
                Command::EnterInsertMode => {
                    self.set_mode(Mode::Insert);
                }
                Command::EnterInsertModeAfter => {
                    self.set_mode(Mode::Insert);
                    execute_command(cmd, &mut self.buf, self.state.expand_tabs);
                }
                Command::EnterCommandMode => {
                    self.set_mode(Mode::Command);
                }
                Command::Quit => {
                    self.should_quit = true;
                    continue;
                }
                _ => {}
            }

            // Handle command line editing commands
            match cmd {
                Command::AppendToCommandLine(ch) => {
                    // ch is guaranteed to be valid ASCII (32-126) from translate_command_mode
                    self.state.append_to_command_line(ch as char);
                }
                Command::DeleteFromCommandLine => {
                    self.state.remove_from_command_line();
                }
                Command::ExecuteCommandLine => {
                    // For now, just exit command mode
                    // TODO: Parse and execute the command
                    self.state.clear_command_line();
                    self.set_mode(Mode::Normal);
                    self.update_and_render()?;
                    continue;
                }
                _ => {}
            }

            // Execute command (skip mode transitions and command line editing)
            let should_execute = match cmd {
                Command::EnterInsertMode 
                | Command::EnterInsertModeAfter 
                | Command::EnterCommandMode
                | Command::AppendToCommandLine(_)
                | Command::DeleteFromCommandLine
                | Command::ExecuteCommandLine => false,
                _ => true,
            };
            
            if should_execute {
                execute_command(cmd, &mut self.buf, self.state.expand_tabs);
            }

            // Update state and render
            self.update_and_render()?;
        }

        Ok(())
    }

    /// Update editor state with current buffer and cursor information
    fn update_state(&mut self) {
        let cursor_line = self.buf.get_line();
        let cursor_col = render::calculate_cursor_column(&self.buf, cursor_line);
        self.state.update_cursor(cursor_line, cursor_col);
        
        let total_lines = self.buf.get_total_lines();
        let buffer_size = self.buf.get_before_gap().len() + self.buf.get_after_gap().len();
        self.state.update_buffer_stats(total_lines, buffer_size);
    }

    /// Update state and render the editor
    fn update_and_render(&mut self) -> Result<(), String> {
        self.update_state();
        self.render()
    }

    /// Render the editor interface
    fn render(&mut self) -> Result<(), String> {
        render::render(
            &mut self.terminal,
            &self.buf,
            &mut self.viewport,
            self.current_mode,
            self.dispatcher.pending_key(),
            &self.state,
        )
    }

    /// Set editor mode and update dispatcher
    fn set_mode(&mut self, mode: Mode) {
        self.current_mode = mode;
        self.dispatcher.set_mode(mode);
    }

}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.terminal.deinit();
    }
}

