//! Search functionality for Rift
//!
//! Implements efficient regex search over the buffer.
//! Supports:
//! - Hybrid search strategy (line-by-line optimization vs full buffer)
//! - Rift-style pattern/flags parsing (via monster-regex)
//! - Forward and Backward search

use crate::buffer::api::BufferView;
use crate::error::{ErrorType, RiftError};
use monster_regex::{parse_rift_format, Regex};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Range in bytes (absolute buffer offsets)
    pub range: Range<usize>,
}

/// Find all occurrences of the pattern in the buffer.
pub fn find_all(buffer: &impl BufferView, query: &str) -> Result<Vec<SearchMatch>, RiftError> {
    // Check if flags are provided (last unescaped slash)
    let has_flags = {
        let bytes = query.as_bytes();
        let mut found = false;
        for (i, &b) in bytes.iter().enumerate().rev() {
            if b == b'/' {
                if i == 0 || bytes[i - 1] != b'\\' {
                    found = true;
                    break;
                }
            }
        }
        found
    };

    let query_cow = if has_flags {
        std::borrow::Cow::Borrowed(query)
    } else {
        std::borrow::Cow::Owned(format!("{}/", query))
    };

    // Parse query
    let (pattern, flags) = parse_rift_format(&query_cow).map_err(|e| {
        RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{:?}", e))
    })?;

    let re = Regex::new(&pattern, flags).map_err(|e| {
        RiftError::new(
            ErrorType::Internal,
            "REGEX_COMPILE_ERROR",
            format!("{:?}", e),
        )
    })?;

    let is_multiline = pattern.contains("\\n") || pattern.contains('\n');

    if is_multiline {
        find_all_multiline(buffer, &re)
    } else {
        find_all_line_by_line(buffer, &re)
    }
}

/// Find the next occurrence of the pattern in the buffer.
pub fn find_next(
    buffer: &impl BufferView,
    start_pos: usize,
    query: &str,
    direction: SearchDirection,
) -> Result<Option<SearchMatch>, RiftError> {
    // Check if flags are provided (last unescaped slash)
    let has_flags = {
        let bytes = query.as_bytes();
        let mut found = false;
        for (i, &b) in bytes.iter().enumerate().rev() {
            if b == b'/' {
                if i == 0 || bytes[i - 1] != b'\\' {
                    found = true;
                    break;
                }
            }
        }
        found
    };

    let query_cow = if has_flags {
        std::borrow::Cow::Borrowed(query)
    } else {
        std::borrow::Cow::Owned(format!("{}/", query))
    };

    // Parse query using monster-regex's Rift format parser
    let (pattern, flags) = parse_rift_format(&query_cow).map_err(|e| {
        RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{:?}", e))
    })?;

    let re = Regex::new(&pattern, flags).map_err(|e| {
        RiftError::new(
            ErrorType::Internal,
            "REGEX_COMPILE_ERROR",
            format!("{:?}", e),
        )
    })?;

    // Optimization: Check if the pattern contains a newline.
    // If it does not, we can search line-by-line which is much more efficient
    // for gap buffers / ropes than reconstructing the whole text.
    let is_multiline = pattern.contains("\\n") || pattern.contains('\n');

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
            if let Some(m) = search_single_line_backward(buffer, start_line_idx, re, start_pos) {
                return Ok(Some(m));
            }

            // 2. Search previous lines
            for i in (0..start_line_idx).rev() {
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
            if let Some(m) = search_single_line_backward(buffer, start_line_idx, re, usize::MAX) {
                if m.range.start > start_pos {
                    return Ok(Some(m));
                }
            }
        }
    }

    Ok(None)
}

fn find_all_line_by_line(
    buffer: &impl BufferView,
    re: &Regex,
) -> Result<Vec<SearchMatch>, RiftError> {
    let mut matches = Vec::new();
    let line_count = buffer.line_count();

    for i in 0..line_count {
        let line_start_offset = buffer.line_start(i);

        let mut line_bytes = Vec::new();
        for chunk in buffer.line_bytes(i) {
            line_bytes.extend_from_slice(chunk);
        }
        if let Ok(line_str) = std::str::from_utf8(&line_bytes) {
            for m in re.find_all(line_str) {
                let byte_start = m.start;
                let byte_end = m.end;
                let abs_start = line_start_offset + byte_start;
                let abs_end = line_start_offset + byte_end;

                matches.push(SearchMatch {
                    range: abs_start..abs_end,
                });
            }
        }
    }

    Ok(matches)
}

/// Helper to search a single line.
/// `min_start_pos`: If provided, the match must start at or after this absolute byte offset.
fn search_single_line(
    buffer: &impl BufferView,
    line_idx: usize,
    re: &Regex,
    min_start_pos: Option<usize>,
) -> Option<SearchMatch> {
    let line_start_offset = buffer.line_start(line_idx);

    // Construct contiguous string for the line
    let mut line_bytes = Vec::new();
    for chunk in buffer.line_bytes(line_idx) {
        line_bytes.extend_from_slice(chunk);
    }
    let line_str = std::str::from_utf8(&line_bytes).ok()?;

    // Find matches
    for m in re.find_all(line_str) {
        let byte_start = m.start;
        let byte_end = m.end;

        let abs_start = line_start_offset + byte_start;
        let abs_end = line_start_offset + byte_end;

        if let Some(min) = min_start_pos {
            if abs_start < min {
                continue;
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
    let line_str = std::str::from_utf8(&line_bytes).ok()?;

    let mut last_valid_match = None;

    for m in re.find_all(line_str) {
        let byte_start = m.start;
        let byte_end = m.end;

        let abs_start = line_start_offset + byte_start;
        let abs_end = line_start_offset + byte_end;

        if abs_start < max_start_pos {
            last_valid_match = Some(SearchMatch {
                range: abs_start..abs_end,
            });
        } else {
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
    // 1. Materialize the entire buffer into string
    let mut full_text_bytes = Vec::with_capacity(buffer.len());
    for i in 0..buffer.line_count() {
        for chunk in buffer.line_bytes(i) {
            full_text_bytes.extend_from_slice(chunk);
        }
        // Add newline back as buffer.line_bytes() strips it
        full_text_bytes.push(b'\n');
    }
    let full_text = String::from_utf8(full_text_bytes)
        .map_err(|e| RiftError::new(ErrorType::Internal, "UTF8_ERROR", e.to_string()))?;

    // 2. Map start_pos for multiline search
    // Since we are using byte offsets now, start_pos is already a byte offset.
    // However, the constructed full_text might differ if we are adding newlines that weren't there?
    // BufferView::line_bytes strips newlines?
    // "buffer.line_bytes() strips it" -> Comment says so.
    // So full_text closely matches buffer structure but is contiguous.

    // Note: buffer.line_start(i) is based on the PieceTable with newlines.
    // The constructed full_text has the same content.
    // So byte offsets should match 1:1.

    match direction {
        SearchDirection::Forward => {
            // Search from start_pos
            for m in re.find_all(&full_text) {
                if m.start >= start_pos {
                    return Ok(Some(convert_match(&full_text, m)));
                }
            }

            // Wrap around
            if let Some(m) = re.find(&full_text) {
                return Ok(Some(convert_match(&full_text, m)));
            }
        }
        SearchDirection::Backward => {
            let mut last_match = None;
            for m in re.find_all(&full_text) {
                if m.start < start_pos {
                    last_match = Some(m);
                } else {
                    break;
                }
            }

            if let Some(m) = last_match {
                return Ok(Some(convert_match(&full_text, m)));
            }

            // Wrap around (search from end)
            let mut very_last_match = None;
            for m in re.find_all(&full_text) {
                very_last_match = Some(m);
            }

            if let Some(m) = very_last_match {
                if m.start >= start_pos {
                    return Ok(Some(convert_match(&full_text, m)));
                }
            }
        }
    }

    Ok(None)
}

fn find_all_multiline(buffer: &impl BufferView, re: &Regex) -> Result<Vec<SearchMatch>, RiftError> {
    let mut full_text_bytes = Vec::with_capacity(buffer.len());
    for i in 0..buffer.line_count() {
        for chunk in buffer.line_bytes(i) {
            full_text_bytes.extend_from_slice(chunk);
        }
        full_text_bytes.push(b'\n');
    }
    let full_text = String::from_utf8(full_text_bytes)
        .map_err(|e| RiftError::new(ErrorType::Internal, "UTF8_ERROR", e.to_string()))?;

    let mut matches = Vec::new();
    for m in re.find_all(&full_text) {
        matches.push(convert_match(&full_text, m));
    }
    Ok(matches)
}

fn convert_match(_text: &str, m: monster_regex::Match) -> SearchMatch {
    SearchMatch {
        range: m.start..m.end,
    }
}

/// Helper to find which line index a code-point offset belongs to.
fn find_line_index(buffer: &impl BufferView, pos: usize) -> usize {
    let line_count = buffer.line_count();
    if line_count == 0 {
        return 0;
    }

    let mut low = 0;
    let mut high = line_count;

    while low < high {
        let mid = low + (high - low) / 2;
        let start = buffer.line_start(mid);

        if start == pos {
            return mid;
        } else if start < pos {
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

    if low > 0 {
        low - 1
    } else {
        0
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
