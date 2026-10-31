use std::collections::VecDeque;

use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
use crate::layer::{Cell, Layer};
use crate::wrap::{MotionRange, RangeKind};

pub(crate) const DEFAULT_RING_CAPACITY: usize = 10;
const TOOLTIP_MAX_WIDTH: usize = 42;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Clip {
    text: Vec<Character>,
    kind: RangeKind,
}

impl Clip {
    #[must_use]
    pub(crate) const fn new(text: Vec<Character>, kind: RangeKind) -> Self {
        Self { text, kind }
    }

    #[must_use]
    pub(crate) fn from_plain(text: Vec<Character>) -> Self {
        let kind = if text.last() == Some(&Character::Newline) {
            RangeKind::Linewise
        } else {
            RangeKind::Charwise
        };
        Self { text, kind }
    }

    #[must_use]
    pub(crate) fn text(&self) -> &[Character] {
        &self.text
    }

    #[must_use]
    pub(crate) const fn kind(&self) -> RangeKind {
        self.kind
    }
}

pub(crate) struct ClipboardRing {
    entries: VecDeque<Clip>,
    capacity: usize,
}

impl Default for ClipboardRing {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardRing {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(DEFAULT_RING_CAPACITY),
            capacity: DEFAULT_RING_CAPACITY,
        }
    }

    pub(crate) fn push(&mut self, clip: Clip) {
        if clip.text.is_empty() {
            return;
        }
        self.entries.push_front(clip);
        while self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }

    #[cfg(test)]
    pub(crate) fn push_str(&mut self, text: &str, kind: RangeKind) {
        self.push(Clip::new(text.chars().map(Character::from).collect(), kind));
    }

    pub(crate) fn set_capacity(&mut self, new_capacity: usize) {
        let cap = new_capacity.max(1);
        self.capacity = cap;
        while self.entries.len() > cap {
            self.entries.pop_back();
        }
    }

    #[must_use]
    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn get(&self, index: usize) -> Option<&Clip> {
        self.entries.get(index)
    }

    #[must_use]
    pub(crate) fn most_recent(&self) -> Option<&Clip> {
        self.get(0)
    }

    #[must_use]
    pub(crate) const fn entries(&self) -> &VecDeque<Clip> {
        &self.entries
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub(crate) fn capture_text(buf: &TextBuffer, range: &MotionRange) -> Clip {
    let first = range.anchor.min(range.new_cursor);
    let last = range.anchor.max(range.new_cursor);
    if range.kind == RangeKind::Linewise {
        let first_line = buf.line_index.get_line_at(first);
        return capture_lines(buf, first_line, buf.line_index.get_line_at(last));
    }

    let end = (last + usize::from(range.inclusive)).min(buf.len());
    Clip::new(buf.chars(first..end).collect(), range.kind)
}

pub(crate) fn capture_current_line(buf: &TextBuffer, count: usize) -> Clip {
    let line = buf.line_index.get_line_at(buf.cursor());
    let last_line = (line + count.max(1).saturating_sub(1)).min(buf.get_total_lines() - 1);
    capture_lines(buf, line, last_line)
}

fn capture_lines(buf: &TextBuffer, first_line: usize, last_line: usize) -> Clip {
    let start = buf.line_index.get_start(first_line).unwrap_or(0);
    let is_final = last_line + 1 >= buf.get_total_lines();
    let end = if is_final {
        buf.len()
    } else {
        buf.line_index.get_start(last_line + 1).unwrap_or(buf.len())
    };
    let text = buf
        .chars(start..end)
        .chain(is_final.then_some(Character::Newline))
        .collect();
    Clip::new(text, RangeKind::Linewise)
}

#[must_use]
pub(crate) fn normalize_newlines(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('\r') {
        return std::borrow::Cow::Borrowed(text);
    }

    std::borrow::Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
}

const SYSTEM_CLIPBOARD_REFRESH_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);

#[cfg(feature = "system_clipboard")]
const CLIPBOARD_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);

#[cfg(feature = "system_clipboard")]
struct ReadRequest(std::sync::mpsc::Sender<Option<String>>);

#[cfg(feature = "system_clipboard")]
struct ClipboardWorker {
    requests: std::sync::mpsc::Sender<ReadRequest>,
}

#[cfg(feature = "system_clipboard")]
impl ClipboardWorker {
    fn new() -> Self {
        let (requests, rx) = std::sync::mpsc::channel::<ReadRequest>();
        std::thread::spawn(move || {
            let mut clipboard = arboard::Clipboard::new().ok();
            for ReadRequest(reply) in rx {
                let text = clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                let _ = reply.send(text);
            }
        });
        Self { requests }
    }

    fn read(&self) -> Option<String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        if self.requests.send(ReadRequest(reply_tx)).is_err() {
            return None;
        }
        reply_rx.recv_timeout(CLIPBOARD_READ_TIMEOUT).ok().flatten()
    }
}

#[cfg(feature = "system_clipboard")]
pub(crate) fn read_system_clipboard_text() -> Option<String> {
    use std::sync::OnceLock;
    static WORKER: OnceLock<ClipboardWorker> = OnceLock::new();
    WORKER.get_or_init(ClipboardWorker::new).read()
}

#[cfg(not(feature = "system_clipboard"))]
#[must_use]
pub(crate) const fn read_system_clipboard_text() -> Option<String> {
    None
}

#[derive(Default)]
pub(crate) struct SystemClipboardCache {
    text: Option<String>,
    last_refreshed: Option<crate::time::Instant>,
    read_count: u32,
}

impl SystemClipboardCache {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn refresh_if_stale(&mut self) {
        let stale = self
            .last_refreshed
            .is_none_or(|t| t.elapsed() >= SYSTEM_CLIPBOARD_REFRESH_INTERVAL);
        if !stale {
            return;
        }
        self.text = read_system_clipboard_text();
        self.read_count += 1;
        self.last_refreshed = Some(crate::time::Instant::now());
    }

    #[must_use]
    pub(crate) fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    #[cfg(test)]
    #[must_use]
    const fn read_count(&self) -> u32 {
        self.read_count
    }
}

pub(crate) struct ClipboardTooltip;

impl ClipboardTooltip {
    pub(crate) fn render(
        ring: &ClipboardRing,
        selected: usize,
        sys_clip: Option<&str>,
        layer: &mut Layer,
        editor_fg: Option<crate::color::Color>,
        editor_background: Option<crate::color::Color>,
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
                if let Some(bg) = editor_background {
                    style = style.with_bg(bg);
                }
                style
            });

        let mut content: Vec<Vec<Cell>> = Vec::new();

        for (i, entry) in ring.entries().iter().enumerate() {
            let is_selected = i == selected;
            let text: String = entry
                .text()
                .iter()
                .copied()
                .map(Character::to_char_lossy)
                .collect();
            let row = render_entry(
                &text,
                content_width,
                is_selected,
                editor_fg,
                editor_background,
            );
            content.push(row);
        }

        if let Some(clip_text) = sys_clip {
            // Separator line
            let sep = std::iter::repeat_n(
                Cell::from_char('─').with_colors(editor_fg, editor_background),
                content_width,
            )
            .collect::<Vec<_>>();
            content.push(sep);

            let label = format!(
                "sys: {}",
                clip_text.replace('\n', "\\n").replace('\t', "\\t")
            );
            let row = render_entry(&label, content_width, false, editor_fg, editor_background);
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
    editor_background: Option<crate::color::Color>,
) -> Vec<Cell> {
    let display = truncate(text, max_width);

    // Selected entry: swap fg/bg for highlight. Normal: use theme colors.
    let (fg, bg) = if selected {
        (editor_background, editor_fg)
    } else {
        (editor_fg, editor_background)
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
        format!("{truncated}…")
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
