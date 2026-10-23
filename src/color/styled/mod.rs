//! Styled text representation
//! Efficient representation of colored text for rendering

use super::ColorStyle;

/// A single character with color style
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StyledChar {
    /// The character byte
    ch: u8,
    /// Color style for this character
    style: ColorStyle,
}

impl StyledChar {
    /// Create a new styled character
    #[must_use]
    const fn new(ch: u8, style: ColorStyle) -> Self {
        Self { ch, style }
    }

    /// Create with default (no color) style
    #[must_use]
    const fn plain(ch: u8) -> Self {
        Self {
            ch,
            style: ColorStyle::new(),
        }
    }
}

/// A color span representing a range of text with a color style
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ColorSpan {
    /// Start column (inclusive)
    pub(super) start: usize,
    /// End column (exclusive)
    pub(super) end: usize,
    /// Color style for this span
    pub(super) style: ColorStyle,
}

impl ColorSpan {
    /// Create a new color span
    #[must_use]
    pub(super) const fn new(start: usize, end: usize, style: ColorStyle) -> Self {
        Self { start, end, style }
    }

    /// Check if span is empty (start >= end)
    #[must_use]
    const fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Get the length of the span
    #[must_use]
    const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// A styled line supporting both per-character and per-span coloring
#[derive(Debug, Clone)]
enum StyledLine {
    /// Plain text with no colors
    Plain(Vec<u8>),
    /// Per-character coloring (more flexible but uses more memory)
    PerChar(Vec<StyledChar>),
    /// Per-span coloring (more memory efficient for large uniform spans)
    PerSpan {
        /// The text content
        text: Vec<u8>,
        /// Color spans sorted by start position
        spans: Vec<ColorSpan>,
    },
}

impl StyledLine {
    /// Create a plain unstyled line
    #[must_use]
    const fn plain(text: Vec<u8>) -> Self {
        Self::Plain(text)
    }

    /// Create a line with per-character coloring
    #[must_use]
    const fn per_char(chars: Vec<StyledChar>) -> Self {
        Self::PerChar(chars)
    }

    /// Create a line with per-span coloring
    #[must_use]
    const fn per_span(text: Vec<u8>, spans: Vec<ColorSpan>) -> Self {
        Self::PerSpan { text, spans }
    }

    /// Get the length of the line
    #[must_use]
    const fn len(&self) -> usize {
        match self {
            Self::Plain(text) | Self::PerSpan { text, .. } => text.len(),
            Self::PerChar(chars) => chars.len(),
        }
    }

    /// Get the plain text bytes (without color information)
    #[must_use]
    fn as_bytes(&self) -> Vec<u8> {
        match self {
            Self::Plain(text) | Self::PerSpan { text, .. } => text.clone(),
            Self::PerChar(chars) => chars.iter().map(|sc| sc.ch).collect(),
        }
    }

    /// Convert to per-character representation
    /// Useful when you need uniform access to colors
    #[must_use]
    fn to_per_char(&self) -> Vec<StyledChar> {
        match self {
            Self::Plain(text) => text.iter().map(|&ch| StyledChar::plain(ch)).collect(),
            Self::PerChar(chars) => chars.clone(),
            Self::PerSpan { text, spans } => {
                let mut result = Vec::with_capacity(text.len());
                let mut span_idx = 0;

                for (i, &ch) in text.iter().enumerate() {
                    // Check if we've moved past the current span
                    while span_idx < spans.len() && spans[span_idx].end <= i {
                        span_idx += 1;
                    }

                    let current_style = if span_idx < spans.len()
                        && spans[span_idx].start <= i
                        && i < spans[span_idx].end
                    {
                        spans[span_idx].style
                    } else {
                        ColorStyle::new()
                    };

                    result.push(StyledChar::new(ch, current_style));
                }

                result
            }
        }
    }

    /// Get color style for a specific column
    #[must_use]
    fn get_style_at(&self, column: usize) -> ColorStyle {
        match self {
            Self::Plain(_) => ColorStyle::new(),
            Self::PerChar(chars) => chars.get(column).map(|sc| sc.style).unwrap_or_default(),
            Self::PerSpan { spans, .. } => {
                // Find span containing this column
                for span in spans {
                    if span.start <= column && column < span.end {
                        return span.style;
                    }
                }
                ColorStyle::new()
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
