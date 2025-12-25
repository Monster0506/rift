//! Styled text representation
//! Efficient representation of colored text for rendering

use super::ColorStyle;

/// A single character with color style
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyledChar {
    /// The character byte
    pub ch: u8,
    /// Color style for this character
    pub style: ColorStyle,
}

impl StyledChar {
    /// Create a new styled character
    pub fn new(ch: u8, style: ColorStyle) -> Self {
        StyledChar { ch, style }
    }

    /// Create with default (no color) style
    pub fn plain(ch: u8) -> Self {
        StyledChar {
            ch,
            style: ColorStyle::new(),
        }
    }
}

/// A color span representing a range of text with a color style
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorSpan {
    /// Start column (inclusive)
    pub start: usize,
    /// End column (exclusive)
    pub end: usize,
    /// Color style for this span
    pub style: ColorStyle,
}

impl ColorSpan {
    /// Create a new color span
    pub fn new(start: usize, end: usize, style: ColorStyle) -> Self {
        ColorSpan { start, end, style }
    }

    /// Check if span is empty (start >= end)
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Get the length of the span
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// A styled line supporting both per-character and per-span coloring
#[derive(Debug, Clone)]
pub enum StyledLine {
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
    pub fn plain(text: Vec<u8>) -> Self {
        StyledLine::Plain(text)
    }

    /// Create a line with per-character coloring
    pub fn per_char(chars: Vec<StyledChar>) -> Self {
        StyledLine::PerChar(chars)
    }

    /// Create a line with per-span coloring
    pub fn per_span(text: Vec<u8>, spans: Vec<ColorSpan>) -> Self {
        StyledLine::PerSpan { text, spans }
    }

    /// Get the length of the line
    pub fn len(&self) -> usize {
        match self {
            StyledLine::Plain(text) => text.len(),
            StyledLine::PerChar(chars) => chars.len(),
            StyledLine::PerSpan { text, .. } => text.len(),
        }
    }

    /// Check if line is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the plain text bytes (without color information)
    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            StyledLine::Plain(text) => text.clone(),
            StyledLine::PerChar(chars) => chars.iter().map(|sc| sc.ch).collect(),
            StyledLine::PerSpan { text, .. } => text.clone(),
        }
    }

    /// Convert to per-character representation
    /// Useful when you need uniform access to colors
    pub fn to_per_char(&self) -> Vec<StyledChar> {
        match self {
            StyledLine::Plain(text) => {
                text.iter().map(|&ch| StyledChar::plain(ch)).collect()
            }
            StyledLine::PerChar(chars) => chars.clone(),
            StyledLine::PerSpan { text, spans } => {
                let mut result = Vec::with_capacity(text.len());
                let mut span_idx = 0;
                let mut current_style = ColorStyle::new();

                for (i, &ch) in text.iter().enumerate() {
                    // Check if we've moved past the current span
                    while span_idx < spans.len() && spans[span_idx].end <= i {
                        span_idx += 1;
                    }

                    // Update current style if we're in a new span
                    if span_idx < spans.len() && spans[span_idx].start <= i && i < spans[span_idx].end {
                        current_style = spans[span_idx].style;
                    } else if span_idx >= spans.len() || i >= spans[span_idx].start {
                        // Outside any span, use default
                        current_style = ColorStyle::new();
                    }

                    result.push(StyledChar::new(ch, current_style));
                }

                result
            }
        }
    }

    /// Get color style for a specific column
    pub fn get_style_at(&self, column: usize) -> ColorStyle {
        match self {
            StyledLine::Plain(_) => ColorStyle::new(),
            StyledLine::PerChar(chars) => {
                chars.get(column)
                    .map(|sc| sc.style)
                    .unwrap_or_default()
            }
            StyledLine::PerSpan { spans, .. } => {
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
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn test_styled_char() {
        let sc = StyledChar::new(b'a', ColorStyle::fg(Color::Red));
        assert_eq!(sc.ch, b'a');
        assert_eq!(sc.style.fg, Some(Color::Red));
    }

    #[test]
    fn test_color_span() {
        let span = ColorSpan::new(0, 5, ColorStyle::fg(Color::Blue));
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 5);
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());

        let empty = ColorSpan::new(5, 5, ColorStyle::new());
        assert!(empty.is_empty());
    }

    #[test]
    fn test_styled_line_plain() {
        let line = StyledLine::plain(b"hello".to_vec());
        assert_eq!(line.len(), 5);
        assert_eq!(line.as_bytes(), b"hello".to_vec());
    }

    #[test]
    fn test_styled_line_per_char() {
        let chars = vec![
            StyledChar::new(b'h', ColorStyle::fg(Color::Red)),
            StyledChar::new(b'e', ColorStyle::fg(Color::Blue)),
        ];
        let line = StyledLine::per_char(chars.clone());
        assert_eq!(line.len(), 2);
        assert_eq!(line.as_bytes(), b"he".to_vec());
    }

    #[test]
    fn test_styled_line_per_span() {
        let text = b"hello world".to_vec();
        let spans = vec![
            ColorSpan::new(0, 5, ColorStyle::fg(Color::Red)),
            ColorSpan::new(6, 11, ColorStyle::fg(Color::Blue)),
        ];
        let line = StyledLine::per_span(text.clone(), spans);
        assert_eq!(line.len(), 11);
        assert_eq!(line.get_style_at(0).fg, Some(Color::Red));
        assert_eq!(line.get_style_at(6).fg, Some(Color::Blue));
        assert_eq!(line.get_style_at(5).fg, None); // Space has no color
    }

    #[test]
    fn test_styled_line_conversion() {
        let text = b"hello".to_vec();
        let spans = vec![
            ColorSpan::new(0, 5, ColorStyle::fg(Color::Red)),
        ];
        let line = StyledLine::per_span(text, spans);
        let per_char = line.to_per_char();
        assert_eq!(per_char.len(), 5);
        assert_eq!(per_char[0].style.fg, Some(Color::Red));
    }
}

