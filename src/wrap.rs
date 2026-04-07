//! Soft-wrap display mapping
//!
//! DisplayMap converts between logical buffer lines and visual rows on screen.
//! A logical line longer than the content width is split into multiple visual rows.
//! j/k use visual rows; dj/cj use logical lines.

use crate::buffer::TextBuffer;
use crate::character::Character;

/// One visual row on screen.
#[derive(Debug, Clone)]
pub struct VisualRowInfo {
    pub logical_line: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub segment_col_start: usize,
    pub segment_col_end: usize,
    pub is_first: bool,
}

/// Precomputed mapping from visual rows → buffer positions.
pub struct DisplayMap {
    rows: Vec<VisualRowInfo>,
    line_first_visual: Vec<usize>,
    pub wrap_width: usize,
    pub tab_width: usize,
}

impl DisplayMap {
    pub fn build(buf: &TextBuffer, wrap_width: usize, tab_width: usize) -> Self {
        let total_lines = buf.get_total_lines();
        let mut rows: Vec<VisualRowInfo> = Vec::with_capacity(total_lines + 4);
        let mut line_first_visual: Vec<usize> = Vec::with_capacity(total_lines);

        if buf.len() == 0 {
            line_first_visual.push(0);
            rows.push(VisualRowInfo {
                logical_line: 0,
                char_start: 0,
                char_end: 0,
                segment_col_start: 0,
                segment_col_end: 0,
                is_first: true,
            });
            return DisplayMap {
                rows,
                line_first_visual,
                wrap_width,
                tab_width,
            };
        }

        let mut line_idx: usize = 0;
        let mut visual_col: usize = 0;
        let mut seg_char_start: usize = 0;
        let mut seg_col_start: usize = 0;
        let mut is_first = true;
        let mut char_pos: usize = 0;
        let mut last_word_start_char: usize = 0;
        let mut last_word_start_col: usize = 0;
        let mut in_word = false;

        line_first_visual.push(0);

        for ch in buf.iter_at(0) {
            if ch == Character::Newline {
                rows.push(VisualRowInfo {
                    logical_line: line_idx,
                    char_start: seg_char_start,
                    char_end: char_pos,
                    segment_col_start: seg_col_start,
                    segment_col_end: visual_col,
                    is_first,
                });
                line_idx += 1;
                char_pos += 1;
                if line_idx < total_lines {
                    line_first_visual.push(rows.len());
                }
                visual_col = 0;
                seg_char_start = char_pos;
                seg_col_start = 0;
                is_first = true;
                in_word = false;
                continue;
            }

            let is_word_char = match ch {
                Character::Unicode(c) => !c.is_whitespace(),
                Character::Tab => false,
                _ => true,
            };
            if is_word_char && !in_word {
                in_word = true;
                last_word_start_char = char_pos;
                last_word_start_col = visual_col;
            } else if !is_word_char {
                in_word = false;
            }

            let w = char_visual_width(ch, visual_col, tab_width);

            if visual_col > seg_col_start && visual_col + w > seg_col_start + wrap_width {
                if last_word_start_char > seg_char_start {
                    rows.push(VisualRowInfo {
                        logical_line: line_idx,
                        char_start: seg_char_start,
                        char_end: last_word_start_char,
                        segment_col_start: seg_col_start,
                        segment_col_end: last_word_start_col,
                        is_first,
                    });
                    is_first = false;
                    seg_col_start = last_word_start_col;
                    seg_char_start = last_word_start_char;
                } else {
                    rows.push(VisualRowInfo {
                        logical_line: line_idx,
                        char_start: seg_char_start,
                        char_end: char_pos,
                        segment_col_start: seg_col_start,
                        segment_col_end: visual_col,
                        is_first,
                    });
                    is_first = false;
                    seg_col_start = visual_col;
                    seg_char_start = char_pos;
                    last_word_start_char = char_pos;
                    last_word_start_col = visual_col;
                }
            }

            visual_col += w;
            char_pos += 1;
        }

        rows.push(VisualRowInfo {
            logical_line: line_idx,
            char_start: seg_char_start,
            char_end: char_pos,
            segment_col_start: seg_col_start,
            segment_col_end: visual_col,
            is_first,
        });

        DisplayMap {
            rows,
            line_first_visual,
            wrap_width,
            tab_width,
        }
    }

    pub fn total_visual_rows(&self) -> usize {
        self.rows.len()
    }

    pub fn get_visual_row(&self, visual_row: usize) -> Option<&VisualRowInfo> {
        self.rows.get(visual_row)
    }

    pub fn logical_to_first_visual(&self, logical_line: usize) -> usize {
        self.line_first_visual
            .get(logical_line)
            .copied()
            .unwrap_or(0)
    }

    pub fn logical_to_last_visual(&self, logical_line: usize) -> usize {
        if logical_line + 1 < self.line_first_visual.len() {
            self.line_first_visual[logical_line + 1].saturating_sub(1)
        } else {
            self.rows.len().saturating_sub(1)
        }
    }

    pub fn char_to_visual_row(&self, char_offset: usize) -> usize {
        let idx = self.rows.partition_point(|r| r.char_start <= char_offset);
        if idx == 0 {
            0
        } else {
            idx - 1
        }
    }

    pub fn char_to_visual_col(&self, char_offset: usize, buf: &TextBuffer) -> usize {
        let row_idx = self.char_to_visual_row(char_offset);
        let row = &self.rows[row_idx];
        let mut col: usize = 0;
        let mut pos = row.char_start;
        while pos < char_offset {
            if let Some(ch) = buf.char_at(pos) {
                col += char_visual_width(ch, row.segment_col_start + col, self.tab_width);
            }
            pos += 1;
        }
        col
    }

    pub fn visual_down(&self, char_offset: usize, buf: &TextBuffer) -> usize {
        let cur_row = self.char_to_visual_row(char_offset);
        let cur_col = self.char_to_visual_col(char_offset, buf);
        let next_row = cur_row + 1;
        if next_row >= self.rows.len() {
            return char_offset;
        }
        self.find_char_at_col(next_row, cur_col, buf)
    }

    pub fn visual_up(&self, char_offset: usize, buf: &TextBuffer) -> usize {
        let cur_row = self.char_to_visual_row(char_offset);
        if cur_row == 0 {
            return char_offset;
        }
        let cur_col = self.char_to_visual_col(char_offset, buf);
        self.find_char_at_col(cur_row - 1, cur_col, buf)
    }

    fn find_char_at_col(&self, visual_row: usize, target_col: usize, buf: &TextBuffer) -> usize {
        let row = &self.rows[visual_row];
        let mut col: usize = 0;
        let mut pos = row.char_start;
        while pos < row.char_end {
            let Some(ch) = buf.char_at(pos) else { break };
            if ch == Character::Newline {
                break;
            }
            let w = char_visual_width(ch, row.segment_col_start + col, self.tab_width);
            if col + w > target_col {
                break;
            }
            col += w;
            pos += 1;
        }
        pos
    }
}

#[inline]
pub fn char_visual_width(ch: Character, abs_col: usize, tab_width: usize) -> usize {
    ch.render_width(abs_col, tab_width)
}

pub struct MotionContext<'a> {
    pub buf: &'a TextBuffer,
    pub tab_width: usize,
    pub wrap_width: usize,
    pub display_map: &'a DisplayMap,
    pub last_search_query: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorContext {
    Move,
    Operator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeKind {
    Charwise,
    Linewise,
}

#[derive(Debug, Clone)]
pub struct MotionRange {
    pub anchor: usize,
    pub new_cursor: usize,
    pub kind: RangeKind,
}

impl MotionRange {
    pub fn charwise(anchor: usize, new_cursor: usize) -> Self {
        Self {
            anchor,
            new_cursor,
            kind: RangeKind::Charwise,
        }
    }
    pub fn linewise(anchor: usize, new_cursor: usize) -> Self {
        Self {
            anchor,
            new_cursor,
            kind: RangeKind::Linewise,
        }
    }
}
