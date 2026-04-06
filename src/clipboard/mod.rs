//! Clipboard ring buffer and tooltip
//!
//! ## clipboard/ Invariants
//!
//! - `ClipboardRing` is pure storage with no UI coupling.
//! - `ClipboardTooltip` is a pure renderer; it owns no state.
//! - The ring always stores the most recent entry at index 0.
//! - Default capacity is 10; adjustable via `clipboard.size` setting.

use std::collections::VecDeque;

use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
use crate::layer::{Cell, Layer};
use crate::wrap::{MotionRange, RangeKind};

pub const DEFAULT_RING_CAPACITY: usize = 10;
const TOOLTIP_MAX_WIDTH: usize = 42;

// ─── Ring ────────────────────────────────────────────────────────────────────

pub struct ClipboardRing {
    entries: VecDeque<String>,
    capacity: usize,
}

impl Default for ClipboardRing {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardRing {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(DEFAULT_RING_CAPACITY),
            capacity: DEFAULT_RING_CAPACITY,
        }
    }

    /// Push a new entry to the front (index 0 = most recent).
    /// Empty strings are ignored. Oldest entry is dropped when at capacity.
    pub fn push(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.entries.push_front(text);
        while self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }

    /// Update the ring capacity, trimming oldest entries if needed.
    pub fn set_capacity(&mut self, new_capacity: usize) {
        let cap = new_capacity.max(1);
        self.capacity = cap;
        while self.entries.len() > cap {
            self.entries.pop_back();
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(String::as_str)
    }

    pub fn most_recent(&self) -> Option<&str> {
        self.get(0)
    }

    pub fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ─── Text capture ─────────────────────────────────────────────────────────────

/// Extract the text covered by a `MotionRange` from the buffer as a `String`.
pub fn capture_text(buf: &TextBuffer, range: &MotionRange) -> String {
    let (start, end) = match range.kind {
        RangeKind::Linewise => {
            let first = range.anchor.min(range.new_cursor);
            let last = range.anchor.max(range.new_cursor);
            let first_line = buf.line_index.get_line_at(first);
            let last_line = buf.line_index.get_line_at(last);
            let s = buf.line_index.get_start(first_line).unwrap_or(0);
            let e = if last_line + 1 < buf.get_total_lines() {
                buf.line_index.get_start(last_line + 1).unwrap_or(buf.len())
            } else {
                buf.len()
            };
            (s, e)
        }
        RangeKind::Charwise => (
            range.anchor.min(range.new_cursor),
            range.anchor.max(range.new_cursor),
        ),
    };
    buf.chars(start..end)
        .map(|c| c.to_char_lossy())
        .collect()
}

/// Capture the full current line (including newline) from the buffer.
pub fn capture_current_line(buf: &TextBuffer) -> String {
    let cursor = buf.cursor();
    let line = buf.line_index.get_line_at(cursor);
    let start = buf.line_index.get_start(line).unwrap_or(0);
    let end = if line + 1 < buf.get_total_lines() {
        buf.line_index.get_start(line + 1).unwrap_or(buf.len())
    } else {
        buf.len()
    };
    buf.chars(start..end).map(|c| c.to_char_lossy()).collect()
}

// ─── Tooltip ──────────────────────────────────────────────────────────────────

pub struct ClipboardTooltip;

impl ClipboardTooltip {
    /// Render the clipboard ring tooltip near the cursor.
    ///
    /// `selected` is the ring index currently staged for paste.
    /// Pass `cursor_row` / `cursor_col` as terminal screen coordinates (0-indexed).
    pub fn render(
        ring: &ClipboardRing,
        selected: usize,
        layer: &mut Layer,
        editor_fg: Option<crate::color::Color>,
        editor_bg: Option<crate::color::Color>,
    ) {
        if ring.is_empty() {
            return;
        }

        // Live read of system clipboard — failure is silently ignored.
        let sys_clip = arboard::Clipboard::new()
            .ok()
            .and_then(|mut cb| cb.get_text().ok());

        let content_width = TOOLTIP_MAX_WIDTH;
        let window_width = content_width + 2; // +2 for border

        let ring_rows = ring.len();
        let sys_rows = if sys_clip.is_some() { 2 } else { 0 }; // separator + entry
        let window_height = ring_rows + sys_rows + 2; // +2 for border

        let window = FloatingWindow::with_style(
            WindowPosition::Bottom,
            window_width,
            window_height,
            {
                let mut style = WindowStyle::new()
                    .with_border(true)
                    .with_reverse_video(false);
                if let Some(fg) = editor_fg {
                    style = style.with_fg(fg);
                }
                if let Some(bg) = editor_bg {
                    style = style.with_bg(bg);
                }
                style
            },
        );

        let mut content: Vec<Vec<Cell>> = Vec::new();

        for (i, entry) in ring.entries().iter().enumerate() {
            let is_selected = i == selected;
            let row = render_entry(entry, content_width, is_selected, editor_fg, editor_bg);
            content.push(row);
        }

        if let Some(clip_text) = sys_clip {
            // Separator line
            let sep = std::iter::repeat(
                Cell::from_char('─').with_colors(editor_fg, editor_bg),
            )
            .take(content_width)
            .collect::<Vec<_>>();
            content.push(sep);

            // System clipboard entry (never highlighted as selected)
            let label = format!("sys: {}", clip_text.replace('\n', "\\n").replace('\t', "\\t"));
            let row = render_entry(&label, content_width, false, editor_fg, editor_bg);
            content.push(row);
        }

        window.render_cells(layer, &content);
    }
}

fn render_entry(
    text: &str,
    max_width: usize,
    selected: bool,
    editor_fg: Option<crate::color::Color>,
    editor_bg: Option<crate::color::Color>,
) -> Vec<Cell> {
    let display = truncate(text, max_width);

    // Selected entry: swap fg/bg for highlight. Normal: use theme colors.
    let (fg, bg) = if selected {
        (editor_bg, editor_fg)
    } else {
        (editor_fg, editor_bg)
    };

    let mut cells: Vec<Cell> = display
        .chars()
        .map(|c| Cell::from_char(c).with_colors(fg, bg))
        .collect();

    // Pad to full width so the highlight fills the row
    while cells.len() < max_width {
        cells.push(Cell::from_char(' ').with_colors(fg, bg));
    }

    cells
}

fn truncate(s: &str, max_width: usize) -> String {
    let s = s.replace('\n', "\\n").replace('\t', "\\t");
    if s.chars().count() <= max_width {
        s
    } else {
        let truncated: String = s.chars().take(max_width.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}
