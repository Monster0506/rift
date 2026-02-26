use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options, Shell};
use std::io;
use std::sync::{mpsc, Arc};

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Title(String),
    Wakeup,
    ChildExit(i32),
}

#[derive(Clone)]
pub struct TerminalListener(mpsc::Sender<TerminalEvent>);

impl EventListener for TerminalListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => {
                let _ = self.0.send(TerminalEvent::Wakeup);
            }
            Event::Title(t) => {
                let _ = self.0.send(TerminalEvent::Title(t));
            }
            Event::ChildExit(code) => {
                let _ = self.0.send(TerminalEvent::ChildExit(code));
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TermDims {
    rows: usize,
    cols: usize,
}

impl alacritty_terminal::grid::Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

pub struct Terminal {
    pub term: Arc<FairMutex<Term<TerminalListener>>>,
    pub notifier: Notifier,
    pub size: (u16, u16),
    pub name: String,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("size", &self.size)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Terminal {
    pub fn new(
        rows: u16,
        cols: u16,
        shell_cmd: Option<String>,
    ) -> anyhow::Result<(Self, mpsc::Receiver<TerminalEvent>)> {
        let shell_name = shell_cmd.unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                std::env::var("SHELL").unwrap_or_else(|_| "powershell.exe".to_string())
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
            }
        });

        let name = std::path::Path::new(&shell_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("terminal")
            .to_string();

        let shell = Shell::new(shell_name, vec![]);
        let options = Options {
            shell: Some(shell),
            working_directory: None,
            drain_on_exit: false,
            env: std::collections::HashMap::new(),
            #[cfg(windows)]
            escape_args: true,
        };

        let size = WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: 10,
            cell_height: 20,
        };

        let pty = tty::new(&options, size, 0)
            .map_err(|e| anyhow::anyhow!("Failed to spawn PTY: {}", e))?;

        let (tx, rx) = mpsc::channel();
        let listener = TerminalListener(tx);
        let config = Config::default();
        let dims = TermDims {
            rows: rows as usize,
            cols: cols as usize,
        };

        let term = Term::new(config, &dims, listener.clone());
        let term = Arc::new(FairMutex::new(term));

        let event_loop = EventLoop::new(term.clone(), listener, pty, false, false)
            .map_err(|e| anyhow::anyhow!("Failed to create EventLoop: {}", e))?;

        let notifier = Notifier(event_loop.channel());
        let _ = event_loop.spawn();

        Ok((
            Self {
                term,
                notifier,
                size: (rows, cols),
                name,
            },
            rx,
        ))
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        self.size = (rows, cols);
        let size = WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: 10,
            cell_height: 20,
        };
        use alacritty_terminal::event::OnResize;
        self.notifier.on_resize(size);
        Ok(())
    }

    pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
        use alacritty_terminal::event::Notify;
        self.notifier.notify(data.to_vec());
        Ok(())
    }

    pub fn read_screen(&self) -> (String, usize, usize) {
        use alacritty_terminal::grid::Dimensions as _;

        let term = self.term.lock();
        let grid = term.grid();

        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();

        let cursor_point = grid.cursor.point;
        // Line indices can be negative in alacritty (scrollback). Clamp to screen.
        let cursor_line = (cursor_point.line.0.max(0) as usize).min(num_lines.saturating_sub(1));
        let cursor_col = cursor_point.column.0.min(num_cols.saturating_sub(1));

        let mut lines: Vec<String> = Vec::with_capacity(num_lines);

        for line_idx in 0..num_lines {
            let line = alacritty_terminal::index::Line(line_idx as i32);

            // Collect this line as chars so all trimming is char-indexed, not
            // byte-indexed. Using byte indices on a string containing multi-byte
            // Unicode (box-drawing chars, cargo's ✓/█, vim status-line glyphs)
            // causes truncate() to panic at a non-char-boundary.
            let mut line_chars: Vec<char> = Vec::with_capacity(num_cols);
            for col_idx in 0..num_cols {
                let col = alacritty_terminal::index::Column(col_idx);
                line_chars.push(grid[line][col].c);
            }

            // Find last non-space char index (char position, not byte).
            let last_non_space = line_chars.iter().rposition(|&c| c != ' ');

            // On the cursor line preserve trailing spaces up to the cursor so
            // cursor positioning in handle_terminal_data() lands correctly.
            let trim_to = if line_idx == cursor_line {
                let cursor_end = (cursor_col + 1).min(num_cols);
                match last_non_space {
                    Some(idx) => (idx + 1).max(cursor_end),
                    None => cursor_end,
                }
            } else {
                match last_non_space {
                    Some(idx) => idx + 1,
                    None => 0,
                }
            };

            lines.push(line_chars[..trim_to].iter().collect());
        }

        let content = lines.join("\n");
        (content, cursor_line, cursor_col)
    }
}
