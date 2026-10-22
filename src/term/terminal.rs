use super::TerminalEvent;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options, Shell};
use std::sync::{mpsc, Arc};

#[derive(Clone)]
pub(crate) struct TerminalListener(mpsc::Sender<TerminalEvent>);

impl EventListener for TerminalListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => {
                let _ = self.0.send(TerminalEvent::Wakeup);
            }
            Event::Title(_) => {
                let _ = self.0.send(TerminalEvent::Title);
            }
            Event::ChildExit(status) => {
                let _ = self
                    .0
                    .send(TerminalEvent::ChildExit(status.code().unwrap_or(-1)));
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

pub(crate) struct Terminal {
    pub(crate) term: Arc<FairMutex<Term<TerminalListener>>>,
    pub(crate) notifier: Notifier,
    pub(crate) size: (u16, u16),
    pub(crate) name: String,
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
    /// Spawn a shell in a PTY of the given size.
    ///
    /// # Errors
    /// Returns `Err` if the PTY cannot be created or the shell cannot be spawned.
    pub(crate) fn new(
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

        let pty =
            tty::new(&options, size, 0).map_err(|e| anyhow::anyhow!("Failed to spawn PTY: {e}"))?;

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
            .map_err(|e| anyhow::anyhow!("Failed to create EventLoop: {e}"))?;

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

    /// Resize the PTY and the terminal grid.
    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        use alacritty_terminal::event::OnResize;
        self.size = (rows, cols);
        let new_dims = TermDims {
            rows: rows as usize,
            cols: cols as usize,
        };
        // Resize the alacritty Term grid synchronously so read_screen() called immediately after
        // sees the new dimensions rather than stale content (the EventLoop resize is async).
        self.term.lock().resize(new_dims);
        // Also notify the EventLoop so the OS PTY gets TIOCSWINSZ / ConPTY
        // resize and the shell receives SIGWINCH and redraws.
        let size = WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: 10,
            cell_height: 20,
        };
        self.notifier.on_resize(size);
    }

    /// Write raw bytes to the PTY.
    pub(crate) fn write(&mut self, data: &[u8]) {
        use alacritty_terminal::event::Notify;
        self.notifier.notify(data.to_vec());
    }

    pub(crate) fn scroll_display(&self, delta: i32) {
        use alacritty_terminal::grid::Scroll;
        self.term
            .lock()
            .grid_mut()
            .scroll_display(Scroll::Delta(delta));
    }

    pub(crate) fn scroll_to_bottom(&self) {
        use alacritty_terminal::grid::Scroll;
        self.term.lock().grid_mut().scroll_display(Scroll::Bottom);
    }

    #[must_use]
    pub(crate) fn read_screen(&self) -> (String, usize, usize, crate::color::CellColorSpans) {
        read_term_screen(&self.term)
    }
}

fn read_term_screen(
    term_arc: &Arc<FairMutex<Term<TerminalListener>>>,
) -> (String, usize, usize, crate::color::CellColorSpans) {
    use alacritty_terminal::grid::Dimensions as _;

    let term = term_arc.lock();
    let grid = term.grid();

    let num_cols = grid.columns();
    let num_lines = grid.screen_lines();
    let display_offset = i32::try_from(grid.display_offset()).unwrap_or(i32::MAX);

    let cursor_point = grid.cursor.point;
    let cursor_abs = cursor_point.line.0;
    let cursor_line_in_view = cursor_abs + display_offset;
    let cursor_line = usize::try_from(cursor_line_in_view)
        .ok()
        .filter(|&line| line < num_lines)
        .unwrap_or(num_lines);
    let cursor_col = cursor_point.column.0.min(num_cols.saturating_sub(1));

    let mut lines: Vec<String> = Vec::with_capacity(num_lines);
    let mut color_spans: crate::color::CellColorSpans = Vec::new();
    let mut byte_offset: usize = 0;
    let mut span_start: usize = 0;
    let mut span_fg: Option<crate::color::Color> = None;
    let mut span_background: Option<crate::color::Color> = None;

    for line_idx in 0..num_lines {
        let line = alacritty_terminal::index::Line(
            i32::try_from(line_idx).unwrap_or(i32::MAX) - display_offset,
        );
        let mut line_chars: Vec<char> = Vec::with_capacity(num_cols);
        let mut line_cell_colors: Vec<(Option<crate::color::Color>, Option<crate::color::Color>)> =
            Vec::with_capacity(num_cols);
        for col_idx in 0..num_cols {
            let col = alacritty_terminal::index::Column(col_idx);
            let cell = &grid[line][col];
            line_chars.push(cell.c);
            line_cell_colors.push((
                alacritty_color_to_rift(cell.fg),
                alacritty_color_to_rift(cell.bg),
            ));
        }

        let last_non_space = line_chars.iter().rposition(|&c| c != ' ');
        let trim_to = if line_idx == cursor_line {
            let cursor_end = (cursor_col + 1).min(num_cols);
            last_non_space.map_or(cursor_end, |idx| (idx + 1).max(cursor_end))
        } else {
            last_non_space.map_or(0, |idx| idx + 1)
        };

        lines.push(line_chars[..trim_to].iter().collect());

        for col_idx in 0..trim_to {
            let (fg, bg) = line_cell_colors[col_idx];
            let ch_bytes = line_chars[col_idx].len_utf8();
            if (fg, bg) != (span_fg, span_background) {
                if (span_fg.is_some() || span_background.is_some()) && byte_offset > span_start {
                    color_spans.push((span_start..byte_offset, (span_fg, span_background)));
                }
                span_start = byte_offset;
                span_fg = fg;
                span_background = bg;
            }
            byte_offset += ch_bytes;
        }

        if (span_fg.is_some() || span_background.is_some()) && byte_offset > span_start {
            color_spans.push((span_start..byte_offset, (span_fg, span_background)));
        }
        span_fg = None;
        span_background = None;

        if line_idx + 1 < num_lines {
            byte_offset += 1;
        }
        span_start = byte_offset;
    }
    drop(term);

    let content = lines.join("\n");
    (content, cursor_line, cursor_col, color_spans)
}

const fn alacritty_color_to_rift(
    c: alacritty_terminal::vte::ansi::Color,
) -> Option<crate::color::Color> {
    use crate::color::Color;
    use alacritty_terminal::vte::ansi::{Color as AColor, NamedColor};

    match c {
        AColor::Named(
            NamedColor::Foreground
            | NamedColor::Background
            | NamedColor::BrightForeground
            | NamedColor::DimForeground
            | NamedColor::Cursor,
        ) => None,
        AColor::Named(NamedColor::Black | NamedColor::DimBlack) => Some(Color::Black),
        AColor::Named(NamedColor::Red | NamedColor::DimRed) => Some(Color::DarkRed),
        AColor::Named(NamedColor::Green | NamedColor::DimGreen) => Some(Color::DarkGreen),
        AColor::Named(NamedColor::Yellow | NamedColor::DimYellow) => Some(Color::DarkYellow),
        AColor::Named(NamedColor::Blue | NamedColor::DimBlue) => Some(Color::DarkBlue),
        AColor::Named(NamedColor::Magenta | NamedColor::DimMagenta) => Some(Color::DarkMagenta),
        AColor::Named(NamedColor::Cyan | NamedColor::DimCyan) => Some(Color::DarkCyan),
        AColor::Named(NamedColor::White | NamedColor::DimWhite) => Some(Color::Grey),
        AColor::Named(NamedColor::BrightBlack) => Some(Color::DarkGrey),
        AColor::Named(NamedColor::BrightRed) => Some(Color::Red),
        AColor::Named(NamedColor::BrightGreen) => Some(Color::Green),
        AColor::Named(NamedColor::BrightYellow) => Some(Color::Yellow),
        AColor::Named(NamedColor::BrightBlue) => Some(Color::Blue),
        AColor::Named(NamedColor::BrightMagenta) => Some(Color::Magenta),
        AColor::Named(NamedColor::BrightCyan) => Some(Color::Cyan),
        AColor::Named(NamedColor::BrightWhite) => Some(Color::White),
        AColor::Indexed(n) => Some(Color::Ansi256(n)),
        AColor::Spec(rgb) => Some(Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }),
    }
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod terminal_tests;
