use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::render::Color;
use std::iter::Iterator;
use unicode_width::UnicodeWidthChar;

/// A single item to be rendered (character with style)
#[derive(Debug, Clone, PartialEq)]
pub struct RenderItem {
    pub char: Character,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    /// Original byte offset in the buffer (for highlighting/search)
    pub byte_offset: usize,
    /// Length of the character in bytes
    pub len_bytes: usize,
}

impl RenderItem {
    pub fn new(char: Character, byte_offset: usize, len_bytes: usize) -> Self {
        Self {
            char,
            fg: None,
            bg: None,
            byte_offset,
            len_bytes,
        }
    }
}

/// Source that yields characters from a line in the buffer
pub struct LineSource<'a> {
    chars: Box<dyn Iterator<Item = Character> + 'a>,
    current_byte_offset: usize,
}

impl<'a> LineSource<'a> {
    pub fn new(buf: &'a TextBuffer, line_idx: usize) -> Self {
        let line_start = buf.line_index.get_start(line_idx).unwrap_or(0);
        let line_end = buf
            .line_index
            .get_end(line_idx, buf.len())
            .unwrap_or(buf.len());

        let chars = Box::new(buf.chars(line_start..line_end));
        let current_byte_offset = buf.char_to_byte(line_start);

        Self {
            chars,
            current_byte_offset,
        }
    }
}

impl<'a> Iterator for LineSource<'a> {
    type Item = RenderItem;

    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.chars.next()?;
        let len = ch.len_utf8();
        let item = RenderItem::new(ch, self.current_byte_offset, len);
        self.current_byte_offset += len;
        Some(item)
    }
}

/// A trait for pipeline stages
pub trait Pipe: Iterator<Item = RenderItem> {}
impl<T: Iterator<Item = RenderItem>> Pipe for T {}

/// Decorator that applies syntax highlighting
pub struct SyntaxDecorator<'a, I: Iterator<Item = RenderItem>> {
    input: I,
    highlights: &'a [(std::ops::Range<usize>, u32)],
    idx: &'a mut usize,
    syntax_colors: Option<&'a crate::color::theme::SyntaxColors>,
    capture_map: Option<&'a [&'a str]>,
}

impl<'a, I: Iterator<Item = RenderItem>> SyntaxDecorator<'a, I> {
    pub fn new(
        input: I,
        highlights: &'a [(std::ops::Range<usize>, u32)],
        idx: &'a mut usize,
        syntax_colors: Option<&'a crate::color::theme::SyntaxColors>,
        capture_map: Option<&'a [&'a str]>,
    ) -> Self {
        Self {
            input,
            highlights,
            idx,
            syntax_colors,
            capture_map,
        }
    }
}

impl<'a, I: Iterator<Item = RenderItem>> Iterator for SyntaxDecorator<'a, I> {
    type Item = RenderItem;

    fn next(&mut self) -> Option<Self::Item> {
        let mut item = self.input.next()?;

        // Fast forward highlights
        while *self.idx < self.highlights.len() {
            if self.highlights[*self.idx].0.end <= item.byte_offset {
                *self.idx += 1;
            } else {
                break;
            }
        }

        // Check if current item is covered by any highlight
        for (range, capture_idx) in self.highlights.iter().skip(*self.idx) {
            if range.start > item.byte_offset {
                break;
            }
            if range.end > item.byte_offset {
                // Apply color
                if let Some(colors) = self.syntax_colors {
                    if let Some(map) = self.capture_map {
                        if let Some(name) = map.get(*capture_idx as usize) {
                            if let Some(color) = colors.get_color(name) {
                                item.fg = Some(color);
                            }
                        }
                    }
                }
                break;
            }
        }

        Some(item)
    }
}

/// Decorator that applies custom per-byte-range foreground colors.
/// Used by directory and undo-tree buffers to reproduce the original cell-level colours.
pub struct ColorDecorator<'a, I: Iterator<Item = RenderItem>> {
    input: I,
    highlights: &'a [(std::ops::Range<usize>, Color)],
    idx: usize,
}

impl<'a, I: Iterator<Item = RenderItem>> ColorDecorator<'a, I> {
    pub fn new(input: I, highlights: &'a [(std::ops::Range<usize>, Color)]) -> Self {
        Self {
            input,
            highlights,
            idx: 0,
        }
    }
}

impl<'a, I: Iterator<Item = RenderItem>> Iterator for ColorDecorator<'a, I> {
    type Item = RenderItem;

    fn next(&mut self) -> Option<Self::Item> {
        let mut item = self.input.next()?;

        // Advance past expired ranges
        while self.idx < self.highlights.len()
            && self.highlights[self.idx].0.end <= item.byte_offset
        {
            self.idx += 1;
        }

        if self.idx < self.highlights.len() {
            let (range, color) = &self.highlights[self.idx];
            if range.start <= item.byte_offset {
                item.fg = Some(*color);
            }
        }

        Some(item)
    }
}

/// Pick a contrasting foreground color (black or white) for a given background.
pub fn contrasting_color(bg: Color) -> Color {
    match bg {
        Color::Black
        | Color::DarkGrey
        | Color::Blue
        | Color::DarkBlue
        | Color::Red
        | Color::DarkRed
        | Color::Magenta
        | Color::DarkMagenta
        | Color::DarkGreen
        | Color::DarkCyan
        | Color::DarkYellow => Color::White,
        Color::White | Color::Grey | Color::Yellow | Color::Green | Color::Cyan => Color::Black,
        Color::Rgb { r, g, b } => {
            let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            if lum > 128.0 {
                Color::Black
            } else {
                Color::White
            }
        }
        Color::Ansi256(n) => {
            if n >= 232 {
                if n >= 244 {
                    Color::Black
                } else {
                    Color::White
                }
            } else {
                Color::White
            }
        }
        Color::Reset => Color::Reset,
    }
}

/// Decorator that applies plugin highlights as background color with contrasting foreground.
pub struct PluginHighlightDecorator<'a, I: Iterator<Item = RenderItem>> {
    input: I,
    highlights: &'a [(std::ops::Range<usize>, Color)],
    idx: usize,
}

impl<'a, I: Iterator<Item = RenderItem>> PluginHighlightDecorator<'a, I> {
    pub fn new(input: I, highlights: &'a [(std::ops::Range<usize>, Color)]) -> Self {
        Self {
            input,
            highlights,
            idx: 0,
        }
    }
}

impl<'a, I: Iterator<Item = RenderItem>> Iterator for PluginHighlightDecorator<'a, I> {
    type Item = RenderItem;

    fn next(&mut self) -> Option<Self::Item> {
        let mut item = self.input.next()?;

        // Advance past expired ranges
        while self.idx < self.highlights.len()
            && self.highlights[self.idx].0.end <= item.byte_offset
        {
            self.idx += 1;
        }

        if self.idx < self.highlights.len() {
            let (range, bg) = &self.highlights[self.idx];
            if range.start <= item.byte_offset {
                item.bg = Some(*bg);
                item.fg = Some(contrasting_color(*bg));
            }
        }

        Some(item)
    }
}

/// Decorator that applies search match highlighting
pub struct SearchDecorator<'a, I: Iterator<Item = RenderItem>> {
    input: I,
    matches: &'a [crate::search::SearchMatch],
    idx: &'a mut usize,
}

impl<'a, I: Iterator<Item = RenderItem>> SearchDecorator<'a, I> {
    pub fn new(input: I, matches: &'a [crate::search::SearchMatch], idx: &'a mut usize) -> Self {
        Self {
            input,
            matches,
            idx,
        }
    }
}

impl<'a, I: Iterator<Item = RenderItem>> Iterator for SearchDecorator<'a, I> {
    type Item = RenderItem;

    fn next(&mut self) -> Option<Self::Item> {
        let mut item = self.input.next()?;

        // Fast forward matches
        while *self.idx < self.matches.len() {
            if self.matches[*self.idx].range.end <= item.byte_offset {
                *self.idx += 1;
            } else {
                break;
            }
        }

        if *self.idx < self.matches.len() {
            let m = &self.matches[*self.idx];
            if m.range.start <= item.byte_offset {
                item.fg = Some(Color::Black);
                item.bg = Some(Color::Yellow);
            }
        }

        Some(item)
    }
}

/// Layout stage that handles tab expansion and width calculation
pub struct TabLayout<I: Iterator<Item = RenderItem>> {
    input: I,
    tab_width: usize,
    visual_col: usize,
}

impl<I: Iterator<Item = RenderItem>> TabLayout<I> {
    pub fn new(input: I, tab_width: usize) -> Self {
        Self {
            input,
            tab_width,
            visual_col: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutItem {
    pub char: Character,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub width: usize,
}

impl<I: Iterator<Item = RenderItem>> Iterator for TabLayout<I> {
    type Item = LayoutItem;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.input.next()?;

        // Handle tabs specially for width calculation
        let width = if item.char == Character::Tab {
            self.tab_width - (self.visual_col % self.tab_width)
        } else {
            match item.char {
                Character::Unicode(c) => UnicodeWidthChar::width(c).unwrap_or(0),
                Character::Byte(_) => 4,    // \xNN
                Character::Control(_) => 2, // ^C
                Character::Newline => 0,
                Character::Tab => 0, // Should be handled above
            }
        };

        let effective_width = width;
        self.visual_col += effective_width;

        Some(LayoutItem {
            char: item.char,
            fg: item.fg,
            bg: item.bg,
            width: effective_width,
        })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
