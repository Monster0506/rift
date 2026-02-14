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
struct TerminalListener(mpsc::Sender<TerminalEvent>);

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

        let mut content = String::with_capacity(num_lines * (num_cols + 1));

        // Build line-by-line from grid rows
        for line_idx in 0..num_lines {
            let line = alacritty_terminal::index::Line(line_idx as i32);
            for col_idx in 0..num_cols {
                let col = alacritty_terminal::index::Column(col_idx);
                let cell = &grid[line][col];
                content.push(cell.c);
            }
            let trimmed_len = content.rfind(|c: char| c != ' ').map_or(0, |i| i + 1);
            let line_start = content.len() - num_cols;
            if trimmed_len > line_start {
                content.truncate(trimmed_len);
            } else {
                content.truncate(line_start);
            }
            if line_idx + 1 < num_lines {
                content.push('\n');
            }
        }

        let cursor_point = grid.cursor.point;
        let cursor_line = cursor_point.line.0 as usize;
        let cursor_col = cursor_point.column.0;

        (content, cursor_line, cursor_col)
    }
}
