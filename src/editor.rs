//! Editor core
//! Main editor logic that ties everything together

use crate::buffer::GapBuffer;
use crate::command::{Command, Dispatcher, execute_command};
use crate::mode::Mode;
use crate::key::Key;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use crate::render;

/// Main editor struct
pub struct Editor<T: TerminalBackend> {
    terminal: T,
    buf: GapBuffer,
    viewport: Viewport,
    dispatcher: Dispatcher,
    current_mode: Mode,
    should_quit: bool,
}

impl<T: TerminalBackend> Editor<T> {
    /// Create a new editor instance
    pub fn new(mut terminal: T) -> Result<Self, String> {
        // Initialize terminal
        terminal.init()?;
        
        // Get terminal size
        let size = terminal.get_size()?;
        
        // Create buffer
        let buf = GapBuffer::new(1024)
            .map_err(|e| format!("Failed to create buffer: {}", e))?;
        
        // Create viewport
        let viewport = Viewport::new(size.rows as usize, size.cols as usize);
        
        // Create dispatcher
        let dispatcher = Dispatcher::new(Mode::Normal);
        
        Ok(Editor {
            terminal,
            buf,
            viewport,
            dispatcher,
            current_mode: Mode::Normal,
            should_quit: false,
        })
    }

    /// Run the editor main loop
    pub fn run(&mut self) -> Result<(), String> {
        // Initial render
        render::render(
            &mut self.terminal,
            &self.buf,
            &mut self.viewport,
            self.current_mode,
            self.dispatcher.pending_key(),
        )?;

        // Main event loop
        while !self.should_quit {
            // Read key
            let key_press = self.terminal.read_key()?;

            // Handle debug mode toggle (?)
            match key_press {
                Key::Char(b'?') => {
                    // Toggle debug mode - for now just continue
                    // TODO: Add debug mode support
                    continue;
                }
                _ => {}
            }

            // Handle insert mode exit first (before command translation)
            if self.current_mode == Mode::Insert {
                match key_press {
                    Key::Escape => {
                        self.current_mode = Mode::Normal;
                        self.dispatcher.set_mode(Mode::Normal);
                        render::render(
                            &mut self.terminal,
                            &self.buf,
                            &mut self.viewport,
                            self.current_mode,
                            self.dispatcher.pending_key(),
                        )?;
                        continue;
                    }
                    _ => {}
                }
            }

            // Also handle escape in normal mode to clear pending keys
            if self.current_mode == Mode::Normal {
                match key_press {
                    Key::Escape => {
                        // Clear pending key if any
                        // Note: Dispatcher doesn't expose clear_pending, so we'll handle it in translate
                        render::render(
                            &mut self.terminal,
                            &self.buf,
                            &mut self.viewport,
                            self.current_mode,
                            self.dispatcher.pending_key(),
                        )?;
                        continue;
                    }
                    Key::Ctrl(ch) => {
                        if ch == b']' {
                            // Clear pending key
                            render::render(
                                &mut self.terminal,
                                &self.buf,
                                &mut self.viewport,
                                self.current_mode,
                                self.dispatcher.pending_key(),
                            )?;
                            continue;
                        }
                    }
                    _ => {}
                }
            }

            // Translate key to command
            let cmd = self.dispatcher.translate_key(key_press);

            // Handle mode transitions
            match cmd {
                Command::EnterInsertMode => {
                    self.current_mode = Mode::Insert;
                    self.dispatcher.set_mode(Mode::Insert);
                }
                Command::EnterInsertModeAfter => {
                    self.current_mode = Mode::Insert;
                    self.dispatcher.set_mode(Mode::Insert);
                    execute_command(cmd, &mut self.buf, Some(key_press));
                }
                Command::Quit => {
                    self.should_quit = true;
                    continue;
                }
                _ => {}
            }

            // Execute command
            if cmd != Command::EnterInsertMode && cmd != Command::EnterInsertModeAfter {
                execute_command(cmd, &mut self.buf, Some(key_press));
            }

            // Render
            render::render(
                &mut self.terminal,
                &self.buf,
                &mut self.viewport,
                self.current_mode,
                self.dispatcher.pending_key(),
            )?;
        }

        Ok(())
    }
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.terminal.deinit();
    }
}

