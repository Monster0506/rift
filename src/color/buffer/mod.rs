//! Buffer colorization
//! Maps buffer positions to colors for efficient rendering

use super::ColorStyle;
use super::styled::ColorSpan;
use std::collections::HashMap;

/// Maps line and column positions to color styles
/// Can be populated manually or by a syntax highlighter
#[derive(Debug, Clone, Default)]
pub struct ColorMap {
    /// Map from line number to color spans for that line
    /// Spans are stored as (start_col, end_col, style) tuples
    line_colors: HashMap<usize, Vec<ColorSpan>>,
}

impl ColorMap {
    /// Create a new empty color map
    pub fn new() -> Self {
        ColorMap {
            line_colors: HashMap::new(),
        }
    }

    /// Set color for a specific character position
    pub fn set_char(&mut self, line: usize, column: usize, style: ColorStyle) {
        // For single character, create a span of length 1
        self.set_span(line, column, column + 1, style);
    }

    /// Set color for a span of text
    pub fn set_span(&mut self, line: usize, start_col: usize, end_col: usize, style: ColorStyle) {
        if start_col >= end_col {
            return; // Empty span
        }

        let spans = self.line_colors.entry(line).or_insert_with(Vec::new);
        
        // Insert span maintaining sorted order by start position
        let insert_pos = spans.binary_search_by_key(&start_col, |s| s.start)
            .unwrap_or_else(|pos| pos);
        
        spans.insert(insert_pos, ColorSpan::new(start_col, end_col, style));
        
        // Merge overlapping or adjacent spans with the same style
        self.merge_spans(line);
    }

    /// Get color style for a specific position
    pub fn get_style(&self, line: usize, column: usize) -> Option<ColorStyle> {
        self.line_colors
            .get(&line)
            .and_then(|spans| {
                spans.iter()
                    .find(|span| span.start <= column && column < span.end)
                    .map(|span| span.style)
            })
    }

    /// Get all color spans for a line
    pub fn get_line_spans(&self, line: usize) -> Vec<ColorSpan> {
        self.line_colors
            .get(&line)
            .cloned()
            .unwrap_or_default()
    }

    /// Clear all colors for a specific line
    pub fn clear_line(&mut self, line: usize) {
        self.line_colors.remove(&line);
    }

    /// Clear all colors
    pub fn clear(&mut self) {
        self.line_colors.clear();
    }

    /// Merge overlapping or adjacent spans with the same style
    fn merge_spans(&mut self, line: usize) {
        let spans = match self.line_colors.get_mut(&line) {
            Some(spans) => spans,
            None => return,
        };

        if spans.len() <= 1 {
            return;
        }

        let mut merged = Vec::new();
        let mut current = spans[0].clone();

        for span in spans.iter().skip(1) {
            // If spans overlap or are adjacent and have the same style, merge them
            if current.end >= span.start && current.style == span.style {
                current.end = current.end.max(span.end);
            } else {
                merged.push(current);
                current = span.clone();
            }
        }
        merged.push(current);

        *spans = merged;
    }

    /// Get the number of lines with colors
    pub fn len(&self) -> usize {
        self.line_colors.len()
    }

    /// Check if color map is empty
    pub fn is_empty(&self) -> bool {
        self.line_colors.is_empty()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

