use super::{FloatingWindow, WindowPosition, WindowStyle};
use crate::layer::Cell;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub(crate) struct PopupContent {
    title: String,
    lines: Vec<String>,
    anchor_row: Option<usize>,
    scroll: usize,
    visible_lines: usize,
}

impl PopupContent {
    pub(crate) fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
            anchor_row: None,
            scroll: 0,
            visible_lines: 0,
        }
    }

    #[must_use]
    #[cfg(any(feature = "lsp", test))]
    pub(crate) const fn with_anchor_row(mut self, row: usize) -> Self {
        self.anchor_row = Some(row);
        self
    }

    #[cfg(all(test, feature = "plugins"))]
    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    #[cfg(all(test, feature = "plugins"))]
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
}

#[derive(Debug, Default)]
pub(crate) struct Popup {
    open: Option<PopupContent>,
    just_closed: bool,
}

impl Popup {
    pub(crate) fn open(&mut self, content: PopupContent) {
        self.open = Some(content);
        self.just_closed = false;
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(crate) fn close(&mut self) {
        if self.open.take().is_some() {
            self.just_closed = true;
        }
    }

    pub(crate) const fn take_closed(&mut self) -> bool {
        let closed = self.just_closed;
        self.just_closed = false;
        closed
    }

    pub(crate) fn scroll(&mut self, delta: isize) {
        if let Some(content) = &mut self.open {
            let max = content
                .lines
                .len()
                .saturating_sub(content.visible_lines.max(1));
            content.scroll = content.scroll.saturating_add_signed(delta).min(max);
        }
    }

    pub(crate) fn render_into_layer(
        &mut self,
        layer: &mut crate::layer::Layer,
        fg: Option<crate::color::Color>,
        bg: Option<crate::color::Color>,
    ) {
        let Some(float) = &mut self.open else {
            return;
        };

        let rows = layer.rows();
        let cols = layer.cols();
        let total = float.lines.len();

        // Anchored: below the anchor row if it fits, else above; never on it.
        // The last screen row (status line) is never used. Otherwise centered.
        let (position, content_h, left_aligned) = float.anchor_row.map_or_else(
            || {
                (
                    WindowPosition::Center,
                    total.min(rows.saturating_sub(4)),
                    false,
                )
            },
            |r| {
                let below = rows.saturating_sub(r + 2);
                let above = r;
                let wanted = total + 2;
                let (top, height) = if wanted <= below {
                    (r + 1, wanted)
                } else if wanted <= above {
                    (r - wanted, wanted)
                } else if below >= above {
                    (r + 1, below)
                } else {
                    (0, above)
                };
                let position = WindowPosition::Absolute {
                    row: u16::try_from(top).unwrap_or(u16::MAX),
                    col: 0,
                };
                (position, height.saturating_sub(2), true)
            },
        );
        float.visible_lines = content_h;
        float.scroll = float.scroll.min(total.saturating_sub(content_h));
        let overflow = total > content_h;
        let title = if overflow {
            format!(
                " {} [{}/{}] ",
                float.title,
                (float.scroll + content_h).min(total),
                total
            )
        } else {
            format!(" {} ", float.title)
        };

        // Size to fit content using unicode display width, not `.len()` (byte length),
        // which would give wrong widths for CJK or other multi-byte characters.
        let content_w = float
            .lines
            .iter()
            .map(|l| UnicodeWidthStr::width(l.as_str()))
            .max()
            .unwrap_or(20)
            .max(UnicodeWidthStr::width(title.as_str()))
            .min(cols.saturating_sub(if left_aligned { 2 } else { 4 }));
        let width = (content_w + 2).min(cols);
        let height = (content_h + 2).min(rows);

        let mut style = WindowStyle::default().with_reverse_video(false);
        if let Some(f) = fg {
            style = style.with_fg(f);
        }
        if let Some(b) = bg {
            style = style.with_bg(b);
        }

        let window = FloatingWindow::with_style(position, width, height, style);

        let char_lines: Vec<Vec<char>> = float
            .lines
            .iter()
            .skip(float.scroll)
            .take(content_h)
            .map(|l| l.chars().collect())
            .collect();

        window.render(layer, &char_lines);

        // Title on the top border, clipped to the inner width.
        let (top, left) = window.calculate_position(
            u16::try_from(rows).unwrap_or(u16::MAX),
            u16::try_from(cols).unwrap_or(u16::MAX),
        );
        let (top, left) = (top as usize, left as usize + 1);
        let mut col = 0;
        for ch in title.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w == 0 || col + w > width.saturating_sub(2) {
                continue;
            }
            layer.set_cell(top, left + col, Cell::from_char(ch).with_colors(fg, bg));
            for k in 1..w {
                layer.set_cell(
                    top,
                    left + col + k,
                    Cell::from_char(' ').with_colors(fg, bg),
                );
            }
            col += w;
        }
    }
}

#[cfg(test)]
#[path = "popup_tests.rs"]
mod tests;
