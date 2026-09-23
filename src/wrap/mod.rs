//! Soft-wrap display mapping: `DisplayMap` splits a logical line wider than
//! the content width into multiple visual rows. j/k use visual rows, dj/cj logical.

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
    /// Sorted lines whose end-of-line needs its own row when the text exactly
    /// fills the last segment (they carry trailing virtual text).
    eol_rows: Vec<usize>,
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
    eol_rows: &[usize],
    rows: &mut Vec<VisualRowInfo>,
    line_first_rows: &mut Vec<usize>,
) {
    let wants_eol_row = |line: usize| eol_rows.binary_search(&line).is_ok();
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
                close_line(
                    rows,
                    line_idx,
                    seg_char_start,
                    char_pos,
                    seg_col_start,
                    visual_col,
                    is_first,
                    wants_eol_row(line_idx).then_some(wrap_width),
                );
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
        close_line(
            rows,
            line_idx,
            seg_char_start,
            char_pos,
            seg_col_start,
            visual_col,
            is_first,
            wants_eol_row(line_idx).then_some(wrap_width),
        );
    }
}

/// Emit the rows ending a logical line. With `eol_row_width` set and the
/// segment exactly that wide, the line end gets an extra row of its own.
#[allow(clippy::too_many_arguments)]
fn close_line(
    rows: &mut Vec<VisualRowInfo>,
    logical_line: usize,
    seg_char_start: usize,
    char_pos: usize,
    seg_col_start: usize,
    visual_col: usize,
    is_first: bool,
    eol_row_width: Option<usize>,
) {
    let full = eol_row_width
        .is_some_and(|w| visual_col > seg_col_start && visual_col - seg_col_start >= w);
    if full {
        rows.push(VisualRowInfo {
            logical_line,
            char_start: seg_char_start,
            char_end: char_pos,
            segment_col_start: seg_col_start,
            segment_col_end: visual_col,
            is_first,
        });
    }
    rows.push(VisualRowInfo {
        logical_line,
        char_start: if full { char_pos } else { seg_char_start },
        char_end: char_pos,
        segment_col_start: if full { visual_col } else { seg_col_start },
        segment_col_end: visual_col,
        is_first: is_first && !full,
    });
}

impl DisplayMap {
    pub fn build(buf: &TextBuffer, wrap_width: usize, tab_width: usize) -> Self {
        Self::build_with(buf, wrap_width, tab_width, Vec::new())
    }

    /// `build`, giving the lines in `eol_rows` (sorted) an extra row for
    /// their line end when their text exactly fills the last segment.
    pub fn build_with(
        buf: &TextBuffer,
        wrap_width: usize,
        tab_width: usize,
        eol_rows: Vec<usize>,
    ) -> Self {
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
            &eol_rows,
            &mut rows,
            &mut line_first_visual,
        );

        DisplayMap {
            rows,
            line_first_visual,
            wrap_width,
            tab_width,
            complete: true,
            eol_rows,
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
            eol_rows: Vec::new(),
        }
    }

    /// Set the EOL-row lines on a map that has not wrapped anything yet.
    pub fn with_eol_rows(mut self, eol_rows: Vec<usize>) -> Self {
        debug_assert!(self.rows.is_empty(), "eol rows must be set before wrapping");
        self.eol_rows = eol_rows;
        self
    }

    /// Lines that get an EOL row when their text exactly fills the wrap width.
    pub fn eol_rows(&self) -> &[usize] {
        &self.eol_rows
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
            &self.eol_rows,
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

    /// Patch only the affected lines after an edit; return false when rebuilding is required.
    /// `eol_rows` must match unchanged lines outside the rewrapped region.
    pub fn apply_edit(
        &mut self,
        buf: &TextBuffer,
        pos: usize,
        del: usize,
        ins: usize,
        eol_rows: Vec<usize>,
    ) -> bool {
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

        // Outside the region every old EOL-row line must survive (shifted);
        // a changed set there means rows we would keep were built wrong.
        let shifted_old = self.eol_rows.iter().filter_map(|&l| {
            if l < first_line {
                Some(l)
            } else if l > old_last {
                Some((l as isize + lines_delta) as usize)
            } else {
                None
            }
        });
        let outside_new = eol_rows
            .iter()
            .copied()
            .filter(|&l| l < first_line || l > new_last);
        if !shifted_old.eq(outside_new) {
            return false;
        }
        self.eol_rows = eol_rows;

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
            &self.eol_rows,
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

    /// A row holding only a line end (exactly-full line's continuation). Vertical
    /// motion steps over it so `j`/`k` never stall on a blank row.
    fn is_eol_only(&self, row: usize) -> bool {
        self.rows
            .get(row)
            .is_some_and(|r| r.char_start == r.char_end && !r.is_first)
    }

    fn row_below(&self, cur_row: usize) -> Option<usize> {
        let mut next = cur_row + 1;
        if self.is_eol_only(next) && next + 1 < self.rows.len() {
            next += 1;
        }
        (next < self.rows.len()).then_some(next)
    }

    fn row_above(&self, cur_row: usize) -> Option<usize> {
        let mut prev = cur_row.checked_sub(1)?;
        if self.is_eol_only(prev) && prev > 0 {
            prev -= 1;
        }
        Some(prev)
    }

    pub fn visual_down(&self, char_offset: usize, buf: &TextBuffer) -> usize {
        let cur_row = self.char_to_visual_row(char_offset);
        let cur_col = self.char_to_visual_col(char_offset, buf);
        match self.row_below(cur_row) {
            Some(row) => self.find_char_at_col(row, cur_col, buf),
            None => char_offset,
        }
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
        match self.row_below(cur_row) {
            Some(row) => self.find_char_at_col(row, target_col, buf),
            None => char_offset,
        }
    }

    pub fn visual_up(&self, char_offset: usize, buf: &TextBuffer) -> usize {
        let cur_row = self.char_to_visual_row(char_offset);
        let cur_col = self.char_to_visual_col(char_offset, buf);
        match self.row_above(cur_row) {
            Some(row) => self.find_char_at_col(row, cur_col, buf),
            None => char_offset,
        }
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
        match self.row_above(cur_row) {
            Some(row) => self.find_char_at_col(row, target_col, buf),
            None => char_offset,
        }
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
        self.extend_to_row(buf, cur_row + 3);
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
        self.extend_to_row(buf, cur_row + 3);
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
#[path = "tests.rs"]
mod tests;
