//! Editor core
//! Main editor logic that ties everything together

use crate::buffer::GapBuffer;
use crate::command::{Command, Dispatcher, execute_command};
use crate::mode::Mode;
use crate::key::Key;
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use crate::render;
use crate::state::State;

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
            state: State::new(),
        })
    }

    /// Run the editor main loop
    pub fn run(&mut self) -> Result<(), String> {
        // Initial render
        self.update_state();
        render::render(
            &mut self.terminal,
            &self.buf,
            &mut self.viewport,
            self.current_mode,
            self.dispatcher.pending_key(),
            &self.state,
        )?;

        // Main event loop
        while !self.should_quit {
            // Read key
            let key_press = self.terminal.read_key()?;
            
            // Update state with last keypress
            self.state.update_keypress(key_press);

            // Handle debug mode toggle (?) in normal mode
            if self.current_mode == Mode::Normal {
                match key_press {
                    Key::Char(b'?') => {
                        self.state.toggle_debug();
                        self.update_state();
                        render::render(
                            &mut self.terminal,
                            &self.buf,
                            &mut self.viewport,
                            self.current_mode,
                            self.dispatcher.pending_key(),
                            &self.state,
                        )?;
                        continue;
                    }
                    _ => {}
                }
            }

            // Handle insert mode exit first (before command translation)
            if self.current_mode == Mode::Insert {
                match key_press {
                    Key::Escape => {
                        self.current_mode = Mode::Normal;
                        self.dispatcher.set_mode(Mode::Normal);
                        self.update_state();
                        render::render(
                            &mut self.terminal,
                            &self.buf,
                            &mut self.viewport,
                            self.current_mode,
                            self.dispatcher.pending_key(),
                            &self.state,
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
                        self.update_state();
                        render::render(
                            &mut self.terminal,
                            &self.buf,
                            &mut self.viewport,
                            self.current_mode,
                            self.dispatcher.pending_key(),
                            &self.state,
                        )?;
                        continue;
                    }
                    Key::Ctrl(ch) => {
                        if ch == b']' {
                            // Clear pending key
                            self.update_state();
                            render::render(
                                &mut self.terminal,
                                &self.buf,
                                &mut self.viewport,
                                self.current_mode,
                                self.dispatcher.pending_key(),
                                &self.state,
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

            // Update state before rendering
            self.update_state();

            // Render
            render::render(
                &mut self.terminal,
                &self.buf,
                &mut self.viewport,
                self.current_mode,
                self.dispatcher.pending_key(),
                &self.state,
            )?;
        }

        Ok(())
    }

    /// Update editor state with current buffer and cursor information
    fn update_state(&mut self) {
        let cursor_line = self.buf.get_line();
        let cursor_col = self.calculate_cursor_column(cursor_line);
        self.state.update_cursor(cursor_line, cursor_col);
        
        let total_lines = self.buf.get_total_lines();
        let buffer_size = self.buf.get_before_gap().len() + self.buf.get_after_gap().len();
        self.state.update_buffer_stats(total_lines, buffer_size);
    }

    /// Calculate cursor column for a given line
    fn calculate_cursor_column(&self, line: usize) -> usize {
        let before_gap = self.buf.get_before_gap();
        let mut current_line = 0;
        let mut col = 0;
        
        for &byte in before_gap {
            if byte == b'\n' {
                if current_line == line {
                    return col;
                }
                current_line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        
        // If we're at the gap position on the target line
        if current_line == line {
            return col;
        }
        
        // Check after_gap
        let after_gap = self.buf.get_after_gap();
        for &byte in after_gap {
            if byte == b'\n' {
                if current_line == line {
                    return col;
                }
                current_line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        
        col
    }
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.terminal.deinit();
    }
}

