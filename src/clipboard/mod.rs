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
use crate::character::Character;
use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
use crate::layer::{Cell, Layer};
use crate::wrap::{MotionRange, RangeKind};

pub const DEFAULT_RING_CAPACITY: usize = 10;
const TOOLTIP_MAX_WIDTH: usize = 42;

// ─── Ring ────────────────────────────────────────────────────────────────────

/// Stores entries as `Character` sequences, not `String`, so yanked raw
/// bytes/control chars round-trip through paste instead of becoming U+FFFD.
pub struct ClipboardRing {
    entries: VecDeque<Vec<Character>>,
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
    /// Empty entries are ignored. Oldest entry is dropped when at capacity.
    pub fn push(&mut self, text: Vec<Character>) {
        if text.is_empty() {
            return;
        }
        self.entries.push_front(text);
        while self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }

    /// Convenience for callers that only have a `String` (e.g. the system
    /// clipboard, which is plain text and has no byte-faithful representation).
    pub fn push_str(&mut self, text: String) {
        self.push(text.chars().map(Character::from).collect());
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

    pub fn get(&self, index: usize) -> Option<&[Character]> {
        self.entries.get(index).map(Vec::as_slice)
    }

    pub fn most_recent(&self) -> Option<&[Character]> {
        self.get(0)
    }

    pub fn entries(&self) -> &VecDeque<Vec<Character>> {
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

/// Extract the text covered by a `MotionRange` from the buffer, preserving
/// raw bytes/control chars so it round-trips through the clipboard ring.
pub fn capture_text(buf: &TextBuffer, range: &MotionRange) -> Vec<Character> {
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
        RangeKind::Charwise | RangeKind::Blockwise => {
            let end_offset = if range.inclusive { 1 } else { 0 };
            (
                range.anchor.min(range.new_cursor),
                (range.anchor.max(range.new_cursor) + end_offset).min(buf.len()),
            )
        }
    };
    buf.chars(start..end).collect()
}

/// Capture `count` lines (including newlines) starting at the cursor's line.
pub fn capture_current_line(buf: &TextBuffer, count: usize) -> Vec<Character> {
    let cursor = buf.cursor();
    let line = buf.line_index.get_line_at(cursor);
    let last_line = (line + count.max(1).saturating_sub(1)).min(buf.get_total_lines() - 1);
    let start = buf.line_index.get_start(line).unwrap_or(0);
    let end = if last_line + 1 < buf.get_total_lines() {
        buf.line_index.get_start(last_line + 1).unwrap_or(buf.len())
    } else {
        buf.len()
    };
    buf.chars(start..end).collect()
}

const SYSTEM_CLIPBOARD_REFRESH_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);

#[derive(Default)]
pub struct SystemClipboardCache {
    text: Option<String>,
    last_refreshed: Option<std::time::Instant>,
}

impl SystemClipboardCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refresh_if_stale(&mut self) {
        let stale = self
            .last_refreshed
            .is_none_or(|t| t.elapsed() >= SYSTEM_CLIPBOARD_REFRESH_INTERVAL);
        if !stale {
            return;
        }
        self.text = arboard::Clipboard::new()
            .ok()
            .and_then(|mut cb| cb.get_text().ok());
        self.last_refreshed = Some(std::time::Instant::now());
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

// ─── Tooltip ──────────────────────────────────────────────────────────────────

pub struct ClipboardTooltip;

impl ClipboardTooltip {
    /// Render the clipboard ring tooltip near the cursor.
    ///
    /// `selected` is the ring index currently staged for paste. `sys_clip` is
    /// the last [`SystemClipboardCache`] read, not read live here.
    pub fn render(
        ring: &ClipboardRing,
        selected: usize,
        sys_clip: Option<&str>,
        layer: &mut Layer,
        editor_fg: Option<crate::color::Color>,
        editor_bg: Option<crate::color::Color>,
    ) {
        if ring.is_empty() {
            return;
        }

        let content_width = TOOLTIP_MAX_WIDTH;
        let window_width = content_width + 2; // +2 for border

        let ring_rows = ring.len();
        let sys_rows = if sys_clip.is_some() { 2 } else { 0 }; // separator + entry
        let window_height = ring_rows + sys_rows + 2; // +2 for border

        let window =
            FloatingWindow::with_style(WindowPosition::Bottom, window_width, window_height, {
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
            });

        let mut content: Vec<Vec<Cell>> = Vec::new();

        for (i, entry) in ring.entries().iter().enumerate() {
            let is_selected = i == selected;
            let text: String = entry.iter().map(Character::to_char_lossy).collect();
            let row = render_entry(&text, content_width, is_selected, editor_fg, editor_bg);
            content.push(row);
        }

        if let Some(clip_text) = sys_clip {
            // Separator line
            let sep = std::iter::repeat_n(
                Cell::from_char('─').with_colors(editor_fg, editor_bg),
                content_width,
            )
            .collect::<Vec<_>>();
            content.push(sep);

            // System clipboard entry (never highlighted as selected)
            let label = format!(
                "sys: {}",
                clip_text.replace('\n', "\\n").replace('\t', "\\t")
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clipboard_cache_starts_empty() {
        let cache = SystemClipboardCache::new();
        assert!(cache.text().is_none());
        assert!(cache.last_refreshed.is_none());
    }

    #[test]
    fn capture_text_preserves_byte_and_control_chars() {
        let mut buf = TextBuffer::new(16).unwrap();
        let _ = buf.insert_chars(&[
            Character::Unicode('a'),
            Character::Byte(0xFF),
            Character::Control(0x0C),
            Character::Unicode('b'),
        ]);
        let range = MotionRange {
            anchor: 0,
            new_cursor: 4,
            kind: RangeKind::Charwise,
            inclusive: false,
        };
        let captured = capture_text(&buf, &range);
        assert_eq!(
            captured,
            vec![
                Character::Unicode('a'),
                Character::Byte(0xFF),
                Character::Control(0x0C),
                Character::Unicode('b'),
            ]
        );
    }

    #[test]
    fn system_clipboard_cache_skips_rereading_within_interval() {
        let mut cache = SystemClipboardCache::new();
        cache.refresh_if_stale();
        let first = cache.last_refreshed;
        assert!(first.is_some(), "first call must record a refresh time");

        // Calling again immediately must not touch the OS clipboard or the
        // timestamp -- that's the whole point of the cache.
        cache.refresh_if_stale();
        assert_eq!(
            cache.last_refreshed, first,
            "second call within the refresh interval must not re-read"
        );
    }
}
