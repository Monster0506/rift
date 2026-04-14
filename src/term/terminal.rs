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
        let new_dims = TermDims {
            rows: rows as usize,
            cols: cols as usize,
        };
        // Resize the alacritty Term grid synchronously so that read_screen()
        // called immediately after sees content at the new dimensions rather
        // than the stale old-width content (the EventLoop resize is async).
        self.term.lock().resize(new_dims);
        // Also notify the EventLoop so the OS PTY gets TIOCSWINSZ / ConPTY
        // resize and the shell receives SIGWINCH and redraws.
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

    pub fn scroll_display(&self, delta: i32) {
        use alacritty_terminal::grid::Scroll;
        self.term.lock().grid_mut().scroll_display(Scroll::Delta(delta));
    }

    pub fn scroll_to_bottom(&self) {
        use alacritty_terminal::grid::Scroll;
        self.term.lock().grid_mut().scroll_display(Scroll::Bottom);
    }

    pub fn read_screen(&self) -> (String, usize, usize, crate::color::CellColorSpans) {
        use alacritty_terminal::grid::Dimensions as _;

        let term = self.term.lock();
        let grid = term.grid();

        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        let display_offset = grid.display_offset() as i32;

        let cursor_point = grid.cursor.point;
        // Cursor line relative to the currently visible window (may be off-screen when scrolled).
        let cursor_abs = cursor_point.line.0;
        let cursor_line_in_view = cursor_abs + display_offset;
        let cursor_line = if cursor_line_in_view >= 0 && (cursor_line_in_view as usize) < num_lines {
            cursor_line_in_view as usize
        } else {
            num_lines // sentinel: cursor not visible in current scroll position
        };
        let cursor_col = cursor_point.column.0.min(num_cols.saturating_sub(1));

        let mut lines: Vec<String> = Vec::with_capacity(num_lines);
        let mut color_spans: crate::color::CellColorSpans = Vec::new();

        let mut byte_offset: usize = 0;
        let mut span_start: usize = 0;
        let mut span_fg: Option<crate::color::Color> = None;
        let mut span_bg: Option<crate::color::Color> = None;

        for line_idx in 0..num_lines {
            // When scrolled back, visible line 0 is at Line(-display_offset).
            let line = alacritty_terminal::index::Line(line_idx as i32 - display_offset);

            // Collect this line as chars so all trimming is char-indexed, not
            // byte-indexed. Using byte indices on a string containing multi-byte
            // Unicode (box-drawing chars, cargo's ✓/█, vim status-line glyphs)
            // causes truncate() to panic at a non-char-boundary.
            let mut line_chars: Vec<char> = Vec::with_capacity(num_cols);
            let mut line_cell_colors: Vec<(
                Option<crate::color::Color>,
                Option<crate::color::Color>,
            )> = Vec::with_capacity(num_cols);
            for col_idx in 0..num_cols {
                let col = alacritty_terminal::index::Column(col_idx);
                let cell = &grid[line][col];
                line_chars.push(cell.c);
                line_cell_colors.push((
                    alacritty_color_to_rift(cell.fg),
                    alacritty_color_to_rift(cell.bg),
                ));
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

            for col_idx in 0..trim_to {
                let (fg, bg) = line_cell_colors[col_idx];
                let ch_bytes = line_chars[col_idx].len_utf8();

                if (fg, bg) != (span_fg, span_bg) {
                    if (span_fg.is_some() || span_bg.is_some()) && byte_offset > span_start {
                        color_spans.push((span_start..byte_offset, (span_fg, span_bg)));
                    }
                    span_start = byte_offset;
                    span_fg = fg;
                    span_bg = bg;
                }

                byte_offset += ch_bytes;
            }

            // Don't let spans bleed across the '\n' separator.
            if (span_fg.is_some() || span_bg.is_some()) && byte_offset > span_start {
                color_spans.push((span_start..byte_offset, (span_fg, span_bg)));
            }
            span_fg = None;
            span_bg = None;

            // Account for the '\n' joining character (except after the last line).
            if line_idx + 1 < num_lines {
                byte_offset += 1;
            }
            span_start = byte_offset;
        }

        let content = lines.join("\n");
        (content, cursor_line, cursor_col, color_spans)
    }
}

fn alacritty_color_to_rift(c: alacritty_terminal::vte::ansi::Color) -> Option<crate::color::Color> {
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
        AColor::Named(NamedColor::Black) => Some(Color::Black),
        AColor::Named(NamedColor::Red) => Some(Color::DarkRed),
        AColor::Named(NamedColor::Green) => Some(Color::DarkGreen),
        AColor::Named(NamedColor::Yellow) => Some(Color::DarkYellow),
        AColor::Named(NamedColor::Blue) => Some(Color::DarkBlue),
        AColor::Named(NamedColor::Magenta) => Some(Color::DarkMagenta),
        AColor::Named(NamedColor::Cyan) => Some(Color::DarkCyan),
        AColor::Named(NamedColor::White) => Some(Color::Grey),
        AColor::Named(NamedColor::BrightBlack) => Some(Color::DarkGrey),
        AColor::Named(NamedColor::BrightRed) => Some(Color::Red),
        AColor::Named(NamedColor::BrightGreen) => Some(Color::Green),
        AColor::Named(NamedColor::BrightYellow) => Some(Color::Yellow),
        AColor::Named(NamedColor::BrightBlue) => Some(Color::Blue),
        AColor::Named(NamedColor::BrightMagenta) => Some(Color::Magenta),
        AColor::Named(NamedColor::BrightCyan) => Some(Color::Cyan),
        AColor::Named(NamedColor::BrightWhite) => Some(Color::White),
        AColor::Named(NamedColor::DimBlack) => Some(Color::Black),
        AColor::Named(NamedColor::DimRed) => Some(Color::DarkRed),
        AColor::Named(NamedColor::DimGreen) => Some(Color::DarkGreen),
        AColor::Named(NamedColor::DimYellow) => Some(Color::DarkYellow),
        AColor::Named(NamedColor::DimBlue) => Some(Color::DarkBlue),
        AColor::Named(NamedColor::DimMagenta) => Some(Color::DarkMagenta),
        AColor::Named(NamedColor::DimCyan) => Some(Color::DarkCyan),
        AColor::Named(NamedColor::DimWhite) => Some(Color::Grey),
        AColor::Indexed(n) => Some(Color::Ansi256(n)),
        AColor::Spec(rgb) => Some(Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::grid::Dimensions as _;

    fn make_terminal(rows: u16, cols: u16) -> Terminal {
        let (term, _rx) = Terminal::new(rows, cols, None).expect("failed to spawn terminal");
        term
    }

    fn write_and_wait(term: &mut Terminal, data: &[u8]) {
        term.write(data).unwrap();
        // Give the PTY time to process the bytes.
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    #[test]
    fn test_display_offset_changes_after_scroll() {
        let mut term = make_terminal(5, 40);

        // Write enough to create scrollback.
        for i in 0..20u32 {
            write_and_wait(&mut term, format!("echo LINE{i}\r\n").as_bytes());
        }
        std::thread::sleep(std::time::Duration::from_millis(400));

        let offset_before = term.term.lock().grid().display_offset();
        let history_before = term.term.lock().grid().history_size();
        eprintln!("display_offset before scroll: {offset_before}, history_size: {history_before}");

        // Positive delta = scroll UP (toward older content, increases display_offset).
        term.scroll_display(3);

        let offset_after = term.term.lock().grid().display_offset();
        eprintln!("display_offset after scroll(+3): {offset_after}");

        assert!(
            history_before > 0,
            "expected scrollback history to be non-empty, got {history_before}"
        );
        assert_eq!(
            offset_after,
            3.min(history_before),
            "display_offset should be 3 after scroll(+3)"
        );
    }

    #[test]
    fn test_scrollback_changes_visible_content() {
        let mut term = make_terminal(5, 40);

        for i in 0..20u32 {
            write_and_wait(&mut term, format!("echo LINE{i}\r\n").as_bytes());
        }
        std::thread::sleep(std::time::Duration::from_millis(400));

        let history = term.term.lock().grid().history_size();
        assert!(history > 0, "need scrollback history for this test, got 0");

        let (bottom_screen, _, _, _) = term.read_screen();
        eprintln!("bottom_screen:\n{bottom_screen}");

        // Positive delta = scroll UP (toward older content).
        term.scroll_display(3);
        let offset = term.term.lock().grid().display_offset();
        eprintln!("display_offset after scroll(+3): {offset}");

        let (scrolled_screen, _, _, _) = term.read_screen();
        eprintln!("scrolled_screen:\n{scrolled_screen}");

        assert_ne!(
            bottom_screen, scrolled_screen,
            "screen content should differ after scrolling up (display_offset={offset})"
        );

        term.scroll_to_bottom();
        let (restored_screen, _, _, _) = term.read_screen();
        assert_eq!(
            bottom_screen, restored_screen,
            "screen content should match original after scrolling back to bottom"
        );
    }
}
