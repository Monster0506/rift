//! Soft-wrap display mapping
//!
//! DisplayMap converts between logical buffer lines and visual rows on screen.
//! A logical line longer than the content width is split into multiple visual rows.
//! j/k use visual rows; dj/cj use logical lines.

use crate::buffer::TextBuffer;
use crate::character::Character;

/// One visual row on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRowInfo {
    pub logical_line: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub segment_col_start: usize,
    pub segment_col_end: usize,
    pub is_first: bool,
}

/// Precomputed mapping from visual rows -> buffer positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMap {
    rows: Vec<VisualRowInfo>,
    line_first_visual: Vec<usize>,
    pub wrap_width: usize,
    pub tab_width: usize,
}

/// Wrap chunked chars into visual rows, logging each line's first row index
/// (a trailing newline logs one extra); emit_final_row closes the buffer tail.
#[allow(clippy::too_many_arguments)]
fn wrap_chars<'a>(
    chunks: impl Iterator<Item = &'a [Character]>,
    first_line: usize,
    start_char: usize,
    wrap_width: usize,
    tab_width: usize,
    emit_final_row: bool,
    rows: &mut Vec<VisualRowInfo>,
    line_first_rows: &mut Vec<usize>,
) {
    let mut line_idx = first_line;
    let mut visual_col: usize = 0;
    let mut seg_char_start = start_char;
    let mut seg_col_start: usize = 0;
    let mut is_first = true;
    let mut char_pos = start_char;
    let mut last_word_start_char = start_char;
    let mut last_word_start_col: usize = 0;
    let mut in_word = false;

    line_first_rows.push(rows.len());

    for chunk in chunks {
        for &ch in chunk {
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
                line_first_rows.push(rows.len());
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

            // Printable ASCII is always one column; skip the width tables.
            let w = match ch {
                Character::Unicode(c) if (c as u32).wrapping_sub(0x20) < 0x5f => 1,
                _ => char_visual_width(ch, visual_col, tab_width),
            };

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
    }

    if emit_final_row {
        rows.push(VisualRowInfo {
            logical_line: line_idx,
            char_start: seg_char_start,
            char_end: char_pos,
            segment_col_start: seg_col_start,
            segment_col_end: visual_col,
            is_first,
        });
    }
}

impl DisplayMap {
    pub fn build(buf: &TextBuffer, wrap_width: usize, tab_width: usize) -> Self {
        let total_lines = buf.get_total_lines();
        crate::perf_span!(
            "wrap_build",
            crate::perf::PerfFields {
                lines: Some(total_lines as u32),
                ..Default::default()
            }
        );
        let mut rows: Vec<VisualRowInfo> = Vec::with_capacity(total_lines + 4);
        let mut line_first_visual: Vec<usize> = Vec::with_capacity(total_lines);

        wrap_chars(
            buf.line_index.table.iter_chunks_at(0),
            0,
            0,
            wrap_width,
            tab_width,
            true,
            &mut rows,
            &mut line_first_visual,
        );

        DisplayMap {
            rows,
            line_first_visual,
            wrap_width,
            tab_width,
        }
    }

    /// Patch the map against post-edit `buf` (`del` chars removed at `pos`,
    /// `ins` inserted), rewrapping only the affected lines; false means rebuild.
    pub fn apply_edit(&mut self, buf: &TextBuffer, pos: usize, del: usize, ins: usize) -> bool {
        let old_lines = self.line_first_visual.len();
        let old_len = self.rows.last().map_or(0, |r| r.char_end);
        let new_len = buf.len();
        // The map must describe exactly the pre-edit text.
        if old_len + ins < del || old_len + ins - del != new_len || pos + ins > new_len {
            return false;
        }

        let new_lines = buf.get_total_lines();
        // Chars before pos are untouched, so the first affected line has the
        // same index in the old and new buffer.
        let first_line = buf.line_index.get_line_at(pos);
        let new_last = buf.line_index.get_line_at(pos + ins);
        let lines_delta = new_lines as isize - old_lines as isize;
        let old_last = new_last as isize - lines_delta;
        if old_last < first_line as isize || old_last >= old_lines as isize {
            return false;
        }
        let old_last = old_last as usize;

        let row_start = self.line_first_visual[first_line];
        let row_end = if old_last + 1 < old_lines {
            self.line_first_visual[old_last + 1]
        } else {
            self.rows.len()
        };

        let start_char = buf.line_index.get_line_start(first_line);
        let region_is_tail = new_last + 1 >= new_lines;
        let end_char = if region_is_tail {
            new_len
        } else {
            // Include the last region line's newline so its row is emitted.
            buf.line_index.get_line_start(new_last + 1)
        };

        let mut new_rows: Vec<VisualRowInfo> = Vec::new();
        let mut new_line_rows: Vec<usize> = Vec::new();
        let mut remaining = end_char - start_char;
        let region_chunks =
            buf.line_index
                .table
                .iter_chunks_at(start_char)
                .map_while(move |chunk| {
                    if remaining == 0 {
                        return None;
                    }
                    let take = chunk.len().min(remaining);
                    remaining -= take;
                    Some(&chunk[..take])
                });
        wrap_chars(
            region_chunks,
            first_line,
            start_char,
            self.wrap_width,
            self.tab_width,
            region_is_tail,
            &mut new_rows,
            &mut new_line_rows,
        );
        // The region's trailing newline opens a line beyond it; drop that entry.
        if !region_is_tail {
            new_line_rows.pop();
        }

        let row_delta = new_rows.len() as isize - (row_end - row_start) as isize;
        let char_delta = ins as isize - del as isize;

        for r in &mut self.rows[row_end..] {
            r.char_start = (r.char_start as isize + char_delta) as usize;
            r.char_end = (r.char_end as isize + char_delta) as usize;
            r.logical_line = (r.logical_line as isize + lines_delta) as usize;
        }
        self.rows.splice(row_start..row_end, new_rows);

        for lf in &mut self.line_first_visual[old_last + 1..] {
            *lf = (*lf as isize + row_delta) as usize;
        }
        self.line_first_visual.splice(
            first_line..=old_last,
            new_line_rows.into_iter().map(|r| row_start + r),
        );
        true
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

    /// Like `visual_down` but uses `target_col` instead of the cursor's current column.
    /// Pass `usize::MAX` to always land at end-of-visual-row (the `$` case).
    pub fn visual_down_to_col(
        &self,
        char_offset: usize,
        target_col: usize,
        buf: &TextBuffer,
    ) -> usize {
        let cur_row = self.char_to_visual_row(char_offset);
        let next_row = cur_row + 1;
        if next_row >= self.rows.len() {
            return char_offset;
        }
        self.find_char_at_col(next_row, target_col, buf)
    }

    pub fn visual_up(&self, char_offset: usize, buf: &TextBuffer) -> usize {
        let cur_row = self.char_to_visual_row(char_offset);
        if cur_row == 0 {
            return char_offset;
        }
        let cur_col = self.char_to_visual_col(char_offset, buf);
        self.find_char_at_col(cur_row - 1, cur_col, buf)
    }

    /// Like `visual_up` but uses `target_col` instead of the cursor's current column.
    /// Pass `usize::MAX` to always land at end-of-visual-row (the `$` case).
    pub fn visual_up_to_col(
        &self,
        char_offset: usize,
        target_col: usize,
        buf: &TextBuffer,
    ) -> usize {
        let cur_row = self.char_to_visual_row(char_offset);
        if cur_row == 0 {
            return char_offset;
        }
        self.find_char_at_col(cur_row - 1, target_col, buf)
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
    /// Rectangular column-bounded selection (Ctrl-V Visual Block).
    Blockwise,
}

#[derive(Debug, Clone)]
pub struct MotionRange {
    pub anchor: usize,
    pub new_cursor: usize,
    pub kind: RangeKind,
    /// When true, the endpoint (new_cursor for forward, anchor for backward) is included.
    pub inclusive: bool,
}

impl MotionRange {
    pub fn charwise(anchor: usize, new_cursor: usize) -> Self {
        Self {
            anchor,
            new_cursor,
            kind: RangeKind::Charwise,
            inclusive: false,
        }
    }
    pub fn charwise_inclusive(anchor: usize, new_cursor: usize) -> Self {
        Self {
            anchor,
            new_cursor,
            kind: RangeKind::Charwise,
            inclusive: true,
        }
    }
    pub fn linewise(anchor: usize, new_cursor: usize) -> Self {
        Self {
            anchor,
            new_cursor,
            kind: RangeKind::Linewise,
            inclusive: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf_from(s: &str) -> TextBuffer {
        let mut b = TextBuffer::new(s.len().max(16)).unwrap();
        b.insert_str(s).unwrap();
        b
    }

    fn insert_at(buf: &mut TextBuffer, pos: usize, s: &str) -> (usize, usize, usize) {
        buf.set_cursor(pos).unwrap();
        buf.insert_str(s).unwrap();
        (pos, 0, s.chars().count())
    }

    fn delete_at(buf: &mut TextBuffer, pos: usize, n: usize) -> (usize, usize, usize) {
        assert!(buf.delete_range(pos, n));
        (pos, n, 0)
    }

    fn check_edit(
        text: &str,
        wrap_width: usize,
        edit: impl FnOnce(&mut TextBuffer) -> (usize, usize, usize),
    ) {
        let mut buf = buf_from(text);
        let mut map = DisplayMap::build(&buf, wrap_width, 4);
        let (pos, del, ins) = edit(&mut buf);
        assert!(map.apply_edit(&buf, pos, del, ins), "apply_edit refused");
        let full = DisplayMap::build(&buf, wrap_width, 4);
        assert_eq!(map, full, "incremental map diverged from full rebuild");
    }

    const SAMPLE: &str = "hello world foo bar\nsecond line right here\nthird\n";

    #[test]
    fn apply_edit_insert_char_mid_line() {
        check_edit(SAMPLE, 8, |b| insert_at(b, 7, "XY"));
    }

    #[test]
    fn apply_edit_insert_newline_splits_line() {
        check_edit(SAMPLE, 8, |b| insert_at(b, 5, "\n"));
    }

    #[test]
    fn apply_edit_insert_multiline() {
        check_edit(SAMPLE, 8, |b| insert_at(b, 3, "one\ntwo\nthree"));
    }

    #[test]
    fn apply_edit_delete_char() {
        check_edit(SAMPLE, 8, |b| delete_at(b, 6, 1));
    }

    #[test]
    fn apply_edit_delete_newline_joins_lines() {
        let nl = SAMPLE.find('\n').unwrap();
        check_edit(SAMPLE, 8, |b| delete_at(b, nl, 1));
    }

    #[test]
    fn apply_edit_delete_across_lines() {
        check_edit(SAMPLE, 8, |b| delete_at(b, 15, 12));
    }

    #[test]
    fn apply_edit_at_buffer_start_and_end() {
        check_edit(SAMPLE, 8, |b| insert_at(b, 0, "zz "));
        let len = SAMPLE.chars().count();
        check_edit(SAMPLE, 8, |b| insert_at(b, len, "tail"));
    }

    #[test]
    fn apply_edit_on_last_line_without_trailing_newline() {
        check_edit("first line\nlast has no newline", 8, |b| {
            insert_at(b, 15, "wrap me please")
        });
        check_edit("first line\nlast has no newline", 8, |b| {
            delete_at(b, 12, 6)
        });
    }

    #[test]
    fn apply_edit_delete_everything() {
        let len = SAMPLE.chars().count();
        check_edit(SAMPLE, 8, |b| delete_at(b, 0, len));
    }

    #[test]
    fn apply_edit_insert_into_empty_buffer() {
        check_edit("", 8, |b| insert_at(b, 0, "abc def ghi"));
        check_edit("", 8, |b| insert_at(b, 0, "two\nlines"));
    }

    #[test]
    fn apply_edit_char_wraps_long_word() {
        check_edit("abcdefghijklmnopqrstuvwxyz", 8, |b| {
            insert_at(b, 13, "0123")
        });
    }

    #[test]
    fn apply_edit_with_tabs_and_wide_chars() {
        check_edit("a\tb\tc d e f\n", 8, |b| insert_at(b, 3, "\tx"));
        check_edit("你好世界 hello there\n", 6, |b| insert_at(b, 2, "x"));
        check_edit("你好世界 hello there\n", 6, |b| delete_at(b, 1, 3));
    }

    #[test]
    fn apply_edit_replace_range() {
        check_edit(SAMPLE, 8, |b| {
            let chars: Vec<crate::character::Character> =
                "REPLACED".chars().map(Into::into).collect();
            assert!(b.replace_range(4, 9, &chars));
            (4, 9, chars.len())
        });
    }

    #[test]
    fn apply_edit_rejects_mismatched_buffer() {
        let buf = buf_from(SAMPLE);
        let mut map = DisplayMap::build(&buf, 8, 4);
        let other = buf_from("completely different text of another length");
        assert!(!map.apply_edit(&other, 0, 0, 1));
    }

    #[test]
    fn apply_edit_matches_full_rebuild_random() {
        fn next(seed: &mut u64, m: usize) -> usize {
            *seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((*seed >> 33) as usize) % m.max(1)
        }

        let mut seed: u64 = 0x8765_4321;
        let inserts = ["x", "hello ", "\n", "a\nb", "  ", "wrapping-word", "\t"];
        let mut buf = buf_from(&"the quick brown fox jumps over the lazy dog\n".repeat(8));
        let mut map = DisplayMap::build(&buf, 10, 4);

        for i in 0..300 {
            let len = buf.len();
            let (pos, del, ins) = if next(&mut seed, 2) == 0 || len == 0 {
                let s = inserts[next(&mut seed, inserts.len())];
                insert_at(&mut buf, next(&mut seed, len + 1), s)
            } else {
                let pos = next(&mut seed, len);
                let n = (next(&mut seed, 8) + 1).min(len - pos);
                delete_at(&mut buf, pos, n)
            };
            assert!(
                map.apply_edit(&buf, pos, del, ins),
                "apply_edit refused at step {i}"
            );
            let full = DisplayMap::build(&buf, 10, 4);
            assert_eq!(map, full, "diverged at step {i}");
        }
    }

    #[test]
    fn test_range_kind_blockwise_variant_exists() {
        let _blockwise = RangeKind::Blockwise;
        assert_eq!(_blockwise, RangeKind::Blockwise);
    }

    #[test]
    fn test_motion_range_with_blockwise() {
        let range = MotionRange {
            anchor: 10,
            new_cursor: 20,
            kind: RangeKind::Blockwise,
            inclusive: false,
        };
        assert_eq!(range.kind, RangeKind::Blockwise);
        assert_eq!(range.anchor, 10);
        assert_eq!(range.new_cursor, 20);
        assert!(!range.inclusive);
    }
}
