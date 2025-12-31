//! Search functionality for Rift
//!
//! Implements efficient regex search over the buffer.
//! Supports:
//! - Hybrid search strategy (line-by-line optimization vs full buffer)
//! - Smartcase (case-insensitive if query is all lowercase, unless overridden)
//! - Vim-style flags (\c for case-insensitive, \C for case-sensitive)
//! - Forward and Backward search

use crate::buffer::api::BufferView;
use crate::error::{ErrorType, RiftError};
use regex::bytes::{Regex, RegexBuilder};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Range in code-points (absolute buffer offsets)
    pub range: Range<usize>,
}

/// Configuration for a search operation
struct SearchConfig {
    pattern: String,
    case_sensitive: bool,
}

/// Parse the raw query string into a regex pattern and configuration.
/// Handles Vim-style flags:
/// - \c: Force case insensitive
/// - \C: Force case sensitive
/// - Smartcase: If no flags, case sensitive only if query contains uppercase.
fn parse_query(raw_query: &str) -> SearchConfig {
    let mut pattern = raw_query.to_string();
    let mut case_sensitive = false;
    let mut smart_case = true;

    // Check for flags at the end or anywhere? Vim usually treats them as part of the pattern atom
    // unless using specific syntax, but often users type `/foo\c`.
    // For simplicity in this regex engine wrapper, we'll scan for the sequences.

    if let Some(idx) = pattern.find("\\c") {
        pattern.remove(idx);
        pattern.remove(idx); // remove 'c'
        case_sensitive = false;
        smart_case = false;
    } else if let Some(idx) = pattern.find("\\C") {
        pattern.remove(idx);
        pattern.remove(idx); // remove 'C'
        case_sensitive = true;
        smart_case = false;
    }

    if smart_case {
        // If any character is uppercase, default to case sensitive
        case_sensitive = pattern.chars().any(|c| c.is_uppercase());
    }

    SearchConfig {
        pattern,
        case_sensitive,
    }
}

/// Compile the regex from the configuration
fn compile_regex(config: &SearchConfig) -> Result<Regex, RiftError> {
    RegexBuilder::new(&config.pattern)
        .case_insensitive(!config.case_sensitive)
        .build()
        .map_err(|e| RiftError::new(ErrorType::Internal, "REGEX_ERROR", e.to_string()))
}

/// Find the next occurrence of the pattern in the buffer.
pub fn find_next(
    buffer: &impl BufferView,
    start_pos: usize,
    query: &str,
    direction: SearchDirection,
) -> Result<Option<SearchMatch>, RiftError> {
    let config = parse_query(query);
    let re = compile_regex(&config)?;

    // Optimization: Check if the pattern contains a newline.
    // If it does not, we can search line-by-line which is much more efficient
    // for gap buffers / ropes than reconstructing the whole text.
    // Note: We check the original pattern, not the compiled one, though regex syntax might hide \n.
    // A simple check is usually sufficient for the optimization.
    let is_multiline = config.pattern.contains("\\n") || config.pattern.contains('\n');

    if is_multiline {
        find_multiline(buffer, start_pos, &re, direction)
    } else {
        find_line_by_line(buffer, start_pos, &re, direction)
    }
}

/// Search strategy: Iterate over lines individually.
/// This avoids allocating a massive string for the entire file.
fn find_line_by_line(
    buffer: &impl BufferView,
    start_pos: usize,
    re: &Regex,
    direction: SearchDirection,
) -> Result<Option<SearchMatch>, RiftError> {
    let line_count = buffer.line_count();
    if line_count == 0 {
        return Ok(None);
    }

    // Find the line containing start_pos
    let start_line_idx = find_line_index(buffer, start_pos);

    match direction {
        SearchDirection::Forward => {
            // 1. Search the current line, starting from the cursor offset
            if let Some(m) = search_single_line(buffer, start_line_idx, re, Some(start_pos)) {
                return Ok(Some(m));
            }

            // 2. Search subsequent lines
            for i in (start_line_idx + 1)..line_count {
                if let Some(m) = search_single_line(buffer, i, re, None) {
                    return Ok(Some(m));
                }
            }

            // 3. Wrap around: Search from beginning to start_line_idx
            for i in 0..=start_line_idx {
                // For the start line (again), we only search UP TO start_pos if we want strict wrapping,
                // but usually finding the *next* match implies we already checked after start_pos.
                // If we are wrapping, we treat the start line as a fresh line but stop if we pass start_pos?
                // Standard behavior: just find the first match in the file.

                // However, if i == start_line_idx, we already searched the *tail*.
                // Now we need to search the *head* (before start_pos).

                // If we are at the start line again, we need to be careful not to return the same match
                // if it started exactly at start_pos (which we already checked in step 1).
                // But step 1 checked `after` start_pos.

                if let Some(m) = search_single_line(buffer, i, re, None) {
                    // If we are back at the start line, ensure the match is before the original start_pos
                    if i == start_line_idx && m.range.start >= start_pos {
                        continue;
                    }
                    return Ok(Some(m));
                }
            }
        }
        SearchDirection::Backward => {
            // 1. Search current line, BEFORE start_pos
            // We search the whole line, then pick the last match that ends before (or starts before?) start_pos.
            if let Some(m) = search_single_line_backward(buffer, start_line_idx, re, start_pos) {
                return Ok(Some(m));
            }

            // 2. Search previous lines
            for i in (0..start_line_idx).rev() {
                // For full lines, the "limit" is effectively the end of the line
                if let Some(m) = search_single_line_backward(buffer, i, re, usize::MAX) {
                    return Ok(Some(m));
                }
            }

            // 3. Wrap around: Search from end of file to start_line_idx
            for i in ((start_line_idx + 1)..line_count).rev() {
                if let Some(m) = search_single_line_backward(buffer, i, re, usize::MAX) {
                    return Ok(Some(m));
                }
            }

            // 4. Check the tail of the start line (if we wrapped around)
            // Actually, if we wrap backward, we start from the very last line.
            // If we reach start_line_idx again from the bottom, we want matches *after* start_pos?
            // No, "Previous" means geometrically previous.
            // If we wrap, we go to the end of the file.
            // The logic above covers 0..start_line.
            // The wrap covers start_line+1 .. end.
            // We missed the *tail* of start_line_idx (matches > start_pos).

            if let Some(m) = search_single_line_backward(buffer, start_line_idx, re, usize::MAX) {
                if m.range.start > start_pos {
                    return Ok(Some(m));
                }
            }
        }
    }

    Ok(None)
}

/// Helper to search a single line.
/// `min_start_pos`: If provided, the match must start at or after this absolute code-point offset.
fn search_single_line(
    buffer: &impl BufferView,
    line_idx: usize,
    re: &Regex,
    min_start_pos: Option<usize>,
) -> Option<SearchMatch> {
    let line_start_offset = buffer.line_start(line_idx);

    // Construct contiguous byte slice for the line
    // TODO: Optimization - if line_bytes yields one chunk, use it directly (Cow)
    let mut line_bytes = Vec::new();
    for chunk in buffer.line_bytes(line_idx) {
        line_bytes.extend_from_slice(chunk);
    }

    // Find matches
    for m in re.find_iter(&line_bytes) {
        let byte_start = m.start();
        let byte_end = m.end();

        // Convert byte offsets to code-point offsets
        // We need to count code points in line_bytes[0..byte_start]
        let prefix_str = std::str::from_utf8(&line_bytes[0..byte_start]).ok()?;
        let match_str = std::str::from_utf8(&line_bytes[byte_start..byte_end]).ok()?;

        let cp_start_rel = prefix_str.chars().count();
        let cp_len = match_str.chars().count();

        let abs_start = line_start_offset + cp_start_rel;
        let abs_end = abs_start + cp_len;

        if let Some(min) = min_start_pos {
            if abs_start <= min {
                // If we are strictly looking for "next", usually we want > start_pos
                // or >= start_pos if we want to match character under cursor?
                // Let's assume strict forward progress: > start_pos
                // But standard editors usually match current word if cursor is at start.
                // Let's stick to >= min for now, caller handles +1 if needed.
                if abs_start < min {
                    continue;
                }
            }
        }

        return Some(SearchMatch {
            range: abs_start..abs_end,
        });
    }

    None
}

/// Helper to search a single line backwards.
/// Returns the *last* match that starts before `max_start_pos`.
fn search_single_line_backward(
    buffer: &impl BufferView,
    line_idx: usize,
    re: &Regex,
    max_start_pos: usize,
) -> Option<SearchMatch> {
    let line_start_offset = buffer.line_start(line_idx);

    let mut line_bytes = Vec::new();
    for chunk in buffer.line_bytes(line_idx) {
        line_bytes.extend_from_slice(chunk);
    }

    let mut last_valid_match = None;

    for m in re.find_iter(&line_bytes) {
        let byte_start = m.start();
        let byte_end = m.end();

        let prefix_str = std::str::from_utf8(&line_bytes[0..byte_start]).ok()?;
        let match_str = std::str::from_utf8(&line_bytes[byte_start..byte_end]).ok()?;

        let cp_start_rel = prefix_str.chars().count();
        let cp_len = match_str.chars().count();

        let abs_start = line_start_offset + cp_start_rel;
        let abs_end = abs_start + cp_len;

        if abs_start < max_start_pos {
            last_valid_match = Some(SearchMatch {
                range: abs_start..abs_end,
            });
        } else {
            // Since matches are ordered, once we pass max_start_pos, we can stop?
            // Yes, find_iter returns matches in order.
            break;
        }
    }

    last_valid_match
}

/// Fallback strategy: Construct full buffer string.
/// Necessary for regexes containing `\n`.
fn find_multiline(
    buffer: &impl BufferView,
    start_pos: usize,
    re: &Regex,
    direction: SearchDirection,
) -> Result<Option<SearchMatch>, RiftError> {
    // 1. Materialize the entire buffer into bytes
    // This is expensive for large files, but necessary for multi-line regex
    // without a specialized streaming regex engine.
    let mut full_text = Vec::with_capacity(buffer.len()); // heuristic size
    for i in 0..buffer.line_count() {
        for chunk in buffer.line_bytes(i) {
            full_text.extend_from_slice(chunk);
        }
        // line_bytes usually doesn't include the newline char if it's stored separately or implicit?
        // BufferView docs say: "Contents of `line` without the trailing newline"
        // So we MUST add the newline back for the regex to match against it.
        // We need to know what the newline sequence is.
        // The BufferView trait doesn't explicitly expose line ending per line,
        // but usually we can assume LF or CRLF based on document settings.
        // For now, let's assume LF for search purposes or we might miss matches.
        // TODO: Get actual line ending from buffer/document
        full_text.push(b'\n');
    }

    // 2. We need to map the code-point `start_pos` to a byte offset in `full_text`.
    // This is O(N) scan.
    let start_byte_offset = match get_byte_offset_from_char_offset(&full_text, start_pos) {
        Some(o) => o,
        None => full_text.len(), // End of file
    };

    match direction {
        SearchDirection::Forward => {
            // Search from start_byte_offset
            if let Some(m) = re.find_at(&full_text, start_byte_offset) {
                return Ok(Some(convert_match(&full_text, m)));
            }
            // Wrap around
            if let Some(m) = re.find(&full_text) {
                return Ok(Some(convert_match(&full_text, m)));
            }
        }
        SearchDirection::Backward => {
            // Regex crate doesn't support reverse search natively efficiently.
            // We have to iterate all matches and find the one before start.
            let mut last_match = None;
            for m in re.find_iter(&full_text) {
                if m.start() < start_byte_offset {
                    last_match = Some(m);
                } else {
                    break;
                }
            }

            if let Some(m) = last_match {
                return Ok(Some(convert_match(&full_text, m)));
            }

            // Wrap around (search from end, effectively finding the very last match in file)
            let mut very_last_match = None;
            for m in re.find_iter(&full_text) {
                very_last_match = Some(m);
            }

            // If we wrapped, we want the last match that is > start_pos?
            // No, wrapping backward means we go to the end of the file and search backwards.
            // So we just want the last match in the file.
            if let Some(m) = very_last_match {
                // Only return if it's actually after start_pos (otherwise we just found the same one as above)
                if m.start() >= start_byte_offset {
                    return Ok(Some(convert_match(&full_text, m)));
                }
            }
        }
    }

    Ok(None)
}

fn convert_match(text: &[u8], m: regex::bytes::Match) -> SearchMatch {
    let byte_start = m.start();
    let byte_end = m.end();

    // Convert back to code-points
    // This is expensive (O(N)) but unavoidable with this approach
    let prefix = unsafe { std::str::from_utf8_unchecked(&text[0..byte_start]) };
    let matched = unsafe { std::str::from_utf8_unchecked(&text[byte_start..byte_end]) };

    let start_cp = prefix.chars().count();
    let len_cp = matched.chars().count();

    SearchMatch {
        range: start_cp..(start_cp + len_cp),
    }
}

fn get_byte_offset_from_char_offset(text: &[u8], char_offset: usize) -> Option<usize> {
    let s = std::str::from_utf8(text).ok()?;
    s.char_indices().nth(char_offset).map(|(idx, _)| idx)
}

/// Helper to find which line index a code-point offset belongs to.
/// Uses binary search on line_start().
fn find_line_index(buffer: &impl BufferView, pos: usize) -> usize {
    let line_count = buffer.line_count();
    if line_count == 0 {
        return 0;
    }

    let mut low = 0;
    let mut high = line_count; // Exclusive

    while low < high {
        let mid = low + (high - low) / 2;
        let start = buffer.line_start(mid);

        if start == pos {
            return mid;
        } else if start < pos {
            // Check if pos is within this line
            let next_start = if mid + 1 < line_count {
                buffer.line_start(mid + 1)
            } else {
                usize::MAX
            };

            if pos < next_start {
                return mid;
            }
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    // Fallback
    if low > 0 {
        low - 1
    } else {
        0
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
