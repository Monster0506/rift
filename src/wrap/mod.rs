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

/// Precomputed mapping from visual rows -> buffer positions. May cover only
/// a prefix (`complete` is true once it reaches the end of the document).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMap {
    rows: Vec<VisualRowInfo>,
    line_first_visual: Vec<usize>,
    pub wrap_width: usize,
    pub tab_width: usize,
    complete: bool,
}

/// Lines wrapped per lazy-extension step; bounds how much a single
/// `extend_to_row`/`extend_to_char` call does before rechecking its target.
const EXTEND_BATCH_LINES: usize = 256;

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
            complete: true,
        }
    }

    /// An empty map that covers nothing yet. Callers extend it on demand via
    /// `extend_to_row`/`extend_to_char` before reading past what's built.
    pub fn empty(wrap_width: usize, tab_width: usize) -> Self {
        DisplayMap {
            rows: Vec::new(),
            line_first_visual: Vec::new(),
            wrap_width,
            tab_width,
            complete: false,
        }
    }

    /// Whether the map has been extended through the end of the document.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Wrap the next unwrapped batch of lines and append to `rows`/`line_first_visual`.
    /// A no-op once `complete`.
    fn extend_batch(&mut self, buf: &TextBuffer) {
        if self.complete {
            return;
        }
        let total_lines = buf.get_total_lines();
        let start_line = self.line_first_visual.len();
        if start_line >= total_lines {
            self.complete = true;
            return;
        }
        let end_line = (start_line + EXTEND_BATCH_LINES).min(total_lines);
        let is_tail = end_line >= total_lines;
        let start_char = buf.line_index.get_start(start_line).unwrap_or(0);
        let end_char = if is_tail {
            buf.len()
        } else {
            buf.line_index.get_start(end_line).unwrap_or(buf.len())
        };
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
            start_line,
            start_char,
            self.wrap_width,
            self.tab_width,
            is_tail,
            &mut self.rows,
            &mut self.line_first_visual,
        );
        // A non-tail batch's trailing newline opens the next (unwrapped) line;
        // drop that premature entry, exactly as `apply_edit`'s region reuse does.
        if is_tail {
            self.complete = true;
        } else {
            self.line_first_visual.pop();
        }
    }

    /// Extend until at least `min_rows` rows exist, or the document ends.
    pub fn extend_to_row(&mut self, buf: &TextBuffer, min_rows: usize) {
        while !self.complete && self.rows.len() < min_rows {
            self.extend_batch(buf);
        }
    }

    /// Extend until a row covering `char_offset` exists, or the document ends.
    pub fn extend_to_char(&mut self, buf: &TextBuffer, char_offset: usize) {
        while !self.complete && self.rows.last().is_none_or(|r| r.char_end <= char_offset) {
            self.extend_batch(buf);
        }
    }

    /// Extend fully through the end of the document. Equivalent in cost to
    /// `DisplayMap::build`; only pay this when the whole document is genuinely needed.
    pub fn extend_to_end(&mut self, buf: &TextBuffer) {
        while !self.complete {
            self.extend_batch(buf);
        }
    }

    /// Whether covering `char_offset` plus `extend_margin_rows` rows needs
    /// wrapping work - lets a shared `Arc<DisplayMap>` skip `Arc::make_mut`.
    pub fn needs_extension(&self, char_offset: usize, extend_margin_rows: usize) -> bool {
        if self.complete {
            return false;
        }
        if self.rows.last().is_none_or(|r| r.char_end <= char_offset) {
            return true;
        }
        let row = self.char_to_visual_row(char_offset);
        self.rows.len() < row + extend_margin_rows + 1
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

    /// Rows built so far. Equals the true document total once `is_complete()`;
    /// callers that need the true total unconditionally call `extend_to_end` first.
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

    /// Extending counterpart of `char_to_visual_row`: grows the map to cover
    /// `char_offset` before answering, so the result is always exact.
    pub fn char_to_visual_row_ext(&mut self, char_offset: usize, buf: &TextBuffer) -> usize {
        self.extend_to_char(buf, char_offset);
        self.char_to_visual_row(char_offset)
    }

    /// Extending counterpart of `char_to_visual_col`.
    pub fn char_to_visual_col_ext(&mut self, char_offset: usize, buf: &TextBuffer) -> usize {
        self.extend_to_char(buf, char_offset);
        self.char_to_visual_col(char_offset, buf)
    }

    /// Extending counterpart of `get_visual_row`.
    pub fn get_visual_row_ext(
        &mut self,
        visual_row: usize,
        buf: &TextBuffer,
    ) -> Option<&VisualRowInfo> {
        self.extend_to_row(buf, visual_row + 1);
        self.get_visual_row(visual_row)
    }

    /// Extending counterpart of `visual_down`.
    pub fn visual_down_ext(&mut self, char_offset: usize, buf: &TextBuffer) -> usize {
        self.extend_to_char(buf, char_offset);
        let cur_row = self.char_to_visual_row(char_offset);
        self.extend_to_row(buf, cur_row + 2);
        self.visual_down(char_offset, buf)
    }

    /// Extending counterpart of `visual_down_to_col`.
    pub fn visual_down_to_col_ext(
        &mut self,
        char_offset: usize,
        target_col: usize,
        buf: &TextBuffer,
    ) -> usize {
        self.extend_to_char(buf, char_offset);
        let cur_row = self.char_to_visual_row(char_offset);
        self.extend_to_row(buf, cur_row + 2);
        self.visual_down_to_col(char_offset, target_col, buf)
    }

    /// Extending counterpart of `visual_up` - upward motion never needs new
    /// wrapping once `char_offset`'s own row is built.
    pub fn visual_up_ext(&mut self, char_offset: usize, buf: &TextBuffer) -> usize {
        self.extend_to_char(buf, char_offset);
        self.visual_up(char_offset, buf)
    }

    /// Extending counterpart of `visual_up_to_col`.
    pub fn visual_up_to_col_ext(
        &mut self,
        char_offset: usize,
        target_col: usize,
        buf: &TextBuffer,
    ) -> usize {
        self.extend_to_char(buf, char_offset);
        self.visual_up_to_col(char_offset, target_col, buf)
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

    /// A rightmost-first single-char delete run: collapsing it into one net
    /// `apply_edit` call must match applying it incrementally and a full rebuild.
    #[test]
    fn apply_edit_combined_delete_matches_full_rebuild() {
        let mut buf = buf_from(SAMPLE);
        let mut map = DisplayMap::build(&buf, 8, 4);
        let (pos, del, ins) = insert_at(&mut buf, 6, "ABCDE");
        assert!(map.apply_edit(&buf, pos, del, ins));

        for p in (6..11).rev() {
            assert!(buf.delete_range(p, 1));
        }
        assert!(
            map.apply_edit(&buf, 6, 5, 0),
            "combined delete apply_edit refused"
        );
        let full = DisplayMap::build(&buf, 8, 4);
        assert_eq!(map, full, "combined delete diverged from full rebuild");
    }

    /// A single-char delete run at a fixed position, mirroring repeated
    /// forward-delete (`<Del>`) at an unmoving cursor.
    #[test]
    fn apply_edit_combined_fixed_position_delete_matches_full_rebuild() {
        let mut buf = buf_from(SAMPLE);
        let mut map = DisplayMap::build(&buf, 8, 4);
        let (pos, del, ins) = insert_at(&mut buf, 6, "ABCDE");
        assert!(map.apply_edit(&buf, pos, del, ins));

        for _ in 0..5 {
            assert!(buf.delete_range(6, 1));
        }
        assert!(
            map.apply_edit(&buf, 6, 5, 0),
            "combined fixed-position delete apply_edit refused"
        );
        let full = DisplayMap::build(&buf, 8, 4);
        assert_eq!(map, full, "combined delete diverged from full rebuild");
    }

    /// An ascending single-char insert run, mirroring forward typing or a
    /// redone multi-char insert, one character at a time.
    #[test]
    fn apply_edit_combined_insert_matches_full_rebuild() {
        let mut buf = buf_from(SAMPLE);
        let mut map = DisplayMap::build(&buf, 8, 4);

        let start = 6;
        for (i, ch) in "ABCDE".chars().enumerate() {
            insert_at(&mut buf, start + i, &ch.to_string());
        }
        assert!(
            map.apply_edit(&buf, start, 0, 5),
            "combined insert apply_edit refused"
        );
        let full = DisplayMap::build(&buf, 8, 4);
        assert_eq!(map, full, "combined insert diverged from full rebuild");
    }

    /// Undo of "open a line, then type": N descending char-deletes (typed
    /// text reversed) plus one more delete landing back at the newline's spot.
    #[test]
    fn apply_edit_combined_descending_then_repeat_matches_full_rebuild() {
        let mut buf = buf_from(SAMPLE);
        let mut map = DisplayMap::build(&buf, 8, 4);

        let (pos, del, ins) = insert_at(&mut buf, 6, "\n");
        assert!(map.apply_edit(&buf, pos, del, ins));
        for (i, ch) in "ABCDE".chars().enumerate() {
            let (pos, del, ins) = insert_at(&mut buf, 7 + i, &ch.to_string());
            assert!(map.apply_edit(&buf, pos, del, ins));
        }

        for p in (7..12).rev() {
            assert!(buf.delete_range(p, 1));
        }
        assert!(buf.delete_range(6, 1));
        assert!(
            map.apply_edit(&buf, 6, 6, 0),
            "combined descending-then-repeat apply_edit refused"
        );
        let full = DisplayMap::build(&buf, 8, 4);
        assert_eq!(
            map, full,
            "combined descending-then-repeat diverged from full rebuild"
        );
    }

    /// The `dd` shape (bulk line delete + adjacent newline delete), combined
    /// via the real `combine_char_edits`, not a hand-computed triple.
    #[test]
    fn combine_char_edits_handles_dd_shaped_batch() {
        use crate::buffer::CharEdit;
        use crate::editor::rendering::combine_char_edits;

        let text = format!("{}line to delete\nnext\n", "pad ".repeat(20));
        let mut buf = buf_from(&text);
        let mut map = DisplayMap::build(&buf, 12, 4);

        let line_start = text.find("line to delete").unwrap();
        let line_len = "line to delete\n".len();
        assert!(buf.delete_range(line_start, line_len - 1));
        assert!(buf.delete_range(line_start, 1));
        let edits = [
            CharEdit {
                pos: line_start,
                del: line_len - 1,
                ins: 0,
            },
            CharEdit {
                pos: line_start,
                del: 1,
                ins: 0,
            },
        ];

        let combined = combine_char_edits(&edits).expect("dd shape should combine");
        assert!(map.apply_edit(&buf, combined.pos, combined.del, combined.ins));
        let full = DisplayMap::build(&buf, 12, 4);
        assert_eq!(
            map, full,
            "dd-shaped combined edit diverged from full rebuild"
        );
    }

    /// Randomized batches of 2-6 edits near a moving anchor: every batch
    /// `combine_char_edits` accepts must match a from-scratch rebuild.
    #[test]
    fn combine_char_edits_matches_full_rebuild_across_random_batches() {
        use crate::buffer::CharEdit;
        use crate::editor::rendering::combine_char_edits;

        fn next(seed: &mut u64, m: usize) -> usize {
            *seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((*seed >> 33) as usize) % m.max(1)
        }

        let mut seed: u64 = 0x1357_9bdf_2468_ace0;
        let inserts = ["x", "hi", "\n", "wrap-me", "  ", "a\nb"];

        for trial in 0..300 {
            let mut buf = buf_from(&"the quick brown fox jumps over the lazy dog\n".repeat(6));
            let mut map = DisplayMap::build(&buf, 10, 4);

            let n_edits = 2 + next(&mut seed, 5); // 2..=6
            let mut edits: Vec<CharEdit> = Vec::new();
            let mut anchor = next(&mut seed, buf.len().max(1));

            for _ in 0..n_edits {
                let len = buf.len();
                if len == 0 {
                    break;
                }
                let gap = if next(&mut seed, 4) == 0 {
                    next(&mut seed, 5) + 1
                } else {
                    0
                };
                let pos = (anchor + gap).min(len);
                let (p, d, i) = if next(&mut seed, 2) == 0 {
                    let s = inserts[next(&mut seed, inserts.len())];
                    insert_at(&mut buf, pos, s)
                } else {
                    let n = (next(&mut seed, 5) + 1).min(len - pos);
                    if n == 0 {
                        continue;
                    }
                    delete_at(&mut buf, pos, n)
                };
                edits.push(CharEdit {
                    pos: p,
                    del: d,
                    ins: i,
                });
                anchor = p + i;
            }
            if edits.is_empty() {
                continue;
            }

            if let Some(combined) = combine_char_edits(&edits) {
                assert!(
                    map.apply_edit(&buf, combined.pos, combined.del, combined.ins),
                    "trial {trial}: apply_edit refused a combined edit"
                );
                let full = DisplayMap::build(&buf, 10, 4);
                assert_eq!(
                    map, full,
                    "trial {trial}: combined-edit map diverged from full rebuild (edits={edits:?})"
                );
            }
            // None means a non-contiguous batch was correctly detected - the
            // safe fallback-to-full-rebuild path, nothing to check here.
        }
    }

    fn lcg_next(seed: &mut u64, m: usize) -> usize {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 33) as usize) % m.max(1)
    }

    /// Drives a lazily-extended `DisplayMap` through randomized out-of-order
    /// queries, asserting every answer matches a full eager `DisplayMap::build`.
    fn assert_lazy_matches_full(text: &str, wrap_width: usize, tab_width: usize, seed: u64) {
        let buf = buf_from(text);
        let full = DisplayMap::build(&buf, wrap_width, tab_width);
        let mut lazy = DisplayMap::empty(wrap_width, tab_width);
        let len = buf.len();

        let mut seed = seed;
        let mut offsets: Vec<usize> = Vec::new();
        offsets.push(0);
        if len > 0 {
            offsets.push(len - 1);
            offsets.push(len);
        }
        for _ in 0..60 {
            offsets.push(lcg_next(&mut seed, len + 1));
        }

        for &off in &offsets {
            assert_eq!(
                lazy.char_to_visual_row_ext(off, &buf),
                full.char_to_visual_row(off),
                "char_to_visual_row diverged at offset {off} (text={text:?}, width={wrap_width})"
            );
            assert_eq!(
                lazy.char_to_visual_col_ext(off, &buf),
                full.char_to_visual_col(off, &buf),
                "char_to_visual_col diverged at offset {off} (text={text:?}, width={wrap_width})"
            );
            assert_eq!(
                lazy.visual_down_ext(off, &buf),
                full.visual_down(off, &buf),
                "visual_down diverged at offset {off} (text={text:?}, width={wrap_width})"
            );
            assert_eq!(
                lazy.visual_up_ext(off, &buf),
                full.visual_up(off, &buf),
                "visual_up diverged at offset {off} (text={text:?}, width={wrap_width})"
            );
            assert_eq!(
                lazy.visual_down_to_col_ext(off, 3, &buf),
                full.visual_down_to_col(off, 3, &buf),
                "visual_down_to_col diverged at offset {off} (text={text:?}, width={wrap_width})"
            );
            assert_eq!(
                lazy.visual_up_to_col_ext(off, usize::MAX, &buf),
                full.visual_up_to_col(off, usize::MAX, &buf),
                "visual_up_to_col diverged at offset {off} (text={text:?}, width={wrap_width})"
            );
        }

        let full_rows = full.total_visual_rows();
        for _ in 0..30 {
            let vr = lcg_next(&mut seed, full_rows + 2);
            assert_eq!(
                lazy.get_visual_row_ext(vr, &buf).cloned(),
                full.get_visual_row(vr).cloned(),
                "get_visual_row diverged at row {vr} (text={text:?}, width={wrap_width})"
            );
        }

        lazy.extend_to_end(&buf);
        assert_eq!(
            lazy, full,
            "fully-extended lazy map must equal a fresh full build (text={text:?}, width={wrap_width})"
        );
        assert!(lazy.is_complete());
        assert_eq!(lazy.total_visual_rows(), full.total_visual_rows());
    }

    #[test]
    fn lazy_extension_matches_full_build_basic_sample() {
        assert_lazy_matches_full(SAMPLE, 8, 4, 0x1111_2222);
    }

    #[test]
    fn lazy_extension_matches_full_build_empty_document() {
        assert_lazy_matches_full("", 10, 4, 0x2222_3333);
    }

    #[test]
    fn lazy_extension_matches_full_build_blank_lines_only() {
        assert_lazy_matches_full("\n\n\n\n\n", 10, 4, 0x3333_4444);
    }

    #[test]
    fn lazy_extension_matches_full_build_no_trailing_newline() {
        assert_lazy_matches_full("first line\nlast has no newline", 8, 4, 0x4444_5555);
    }

    #[test]
    fn lazy_extension_matches_full_build_single_huge_word() {
        let text = "x".repeat(500);
        assert_lazy_matches_full(&text, 8, 4, 0x5555_6666);
    }

    #[test]
    fn lazy_extension_matches_full_build_multibyte_utf8() {
        assert_lazy_matches_full(
            "你好世界 hello there\nmore unicode: 日本語のテキストです\n\u{1F600}\u{1F601}\n",
            6,
            4,
            0x6666_7777,
        );
    }

    #[test]
    fn lazy_extension_matches_full_build_tabs_mixed_widths() {
        assert_lazy_matches_full(
            "a\tb\tc\td\te\tf\tg\n\ttabbed start\nend",
            8,
            4,
            0x7777_8888,
        );
    }

    #[test]
    fn lazy_extension_matches_full_build_crosses_extend_batch_boundary() {
        // EXTEND_BATCH_LINES is 256; use enough lines that extension spans
        // multiple internal batches, exercising the extend loop itself.
        let text = "the quick brown fox jumps over the lazy dog\n".repeat(600);
        assert_lazy_matches_full(&text, 10, 4, 0x8888_9999);
    }

    #[test]
    fn lazy_extension_matches_full_build_varied_widths() {
        let text =
            "the quick brown fox jumps over the lazy dog\nsecond line here\n\nfourth\n".repeat(20);
        for width in [1usize, 2, 3, 5, 8, 20, 100] {
            assert_lazy_matches_full(&text, width, 4, 0x9999_aaaa + width as u64);
        }
    }

    #[test]
    fn needs_extension_reflects_whether_extension_would_do_work() {
        let buf = buf_from(&"line of text here\n".repeat(500));
        let mut dm = DisplayMap::empty(10, 4);
        // Nothing built yet: any real request needs extension.
        assert!(dm.needs_extension(0, 5));
        dm.extend_to_char(&buf, 0);
        dm.extend_to_row(&buf, 5);
        assert!(
            !dm.needs_extension(0, 4),
            "already covers offset 0 + margin 4"
        );
        assert!(
            dm.needs_extension(buf.len() - 1, 0),
            "far offset not yet covered"
        );
        dm.extend_to_end(&buf);
        assert!(
            !dm.needs_extension(buf.len().saturating_sub(1), 1_000_000),
            "complete map never needs extension"
        );
    }

    #[test]
    fn lazy_map_starts_empty_and_incomplete() {
        let dm = DisplayMap::empty(10, 4);
        assert!(!dm.is_complete());
        assert_eq!(dm.total_visual_rows(), 0);
    }

    #[test]
    fn extend_to_row_stops_at_document_end_rather_than_looping() {
        let buf = buf_from("only one short line\n");
        let mut dm = DisplayMap::empty(80, 4);
        dm.extend_to_row(&buf, 1_000_000);
        assert!(dm.is_complete());
        let full = DisplayMap::build(&buf, 80, 4);
        assert_eq!(dm, full);
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
