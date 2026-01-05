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
    // Determine if the query is in Rift format or a plain pattern
    // Rift format patterns contain '/' (like "/pattern/flags" or "pattern/flags")
    // Plain patterns have no '/' at all
    let is_rift_format = query.contains('/');

    let (re, pattern) = if is_rift_format {
        // Rift format: parse using monster-regex's parser
        // If it doesn't start with '/', prepend one for parse_rift_format
        let query_for_parser = if query.starts_with('/') {
            std::borrow::Cow::Borrowed(query)
        } else {
            std::borrow::Cow::Owned(format!("/{}", query))
        };

        let (pattern, flags) = parse_rift_format(&query_for_parser).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{:?}", e))
        })?;
        let re = Regex::new(&pattern, flags).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "REGEX_COMPILE_ERROR",
                format!("{:?}", e),
            )
        })?;
        (re, pattern)
    } else {
        // Plain pattern: use directly with default flags
        let re = Regex::new(query, monster_regex::Flags::default()).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "REGEX_COMPILE_ERROR",
                format!("{:?}", e),
            )
        })?;
        (re, query.to_string())
    };

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
    // Determine if the query is in Rift format or a plain pattern
    // Rift format patterns contain '/' (like "/pattern/flags" or "pattern/flags")
    // Plain patterns have no '/' at all
    let is_rift_format = query.contains('/');

    let (re, pattern) = if is_rift_format {
        // Rift format: parse using monster-regex's parser
        // If it doesn't start with '/', prepend one for parse_rift_format
        let query_for_parser = if query.starts_with('/') {
            std::borrow::Cow::Borrowed(query)
        } else {
            std::borrow::Cow::Owned(format!("/{}", query))
        };

        let (pattern, flags) = parse_rift_format(&query_for_parser).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{:?}", e))
        })?;
        let re = Regex::new(&pattern, flags).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "REGEX_COMPILE_ERROR",
                format!("{:?}", e),
            )
        })?;
        (re, pattern)
    } else {
        // Plain pattern: use directly with default flags
        let re = Regex::new(query, monster_regex::Flags::default()).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "REGEX_COMPILE_ERROR",
                format!("{:?}", e),
            )
        })?;
        (re, query.to_string())
    };

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

/// Helper to convert a range of characters to a string and a mapping from byte offsets to char offsets.
fn chars_to_string_with_mapping(
    buffer: &impl BufferView,
    range: Range<usize>,
) -> (String, Vec<usize>) {
    let mut s = String::new();
    // Mapping: byte_index -> char_index relative to range.start
    // We need one entry per character + one for the end.
    // However, for O(1) lookup of "byte offset -> char index", a Vec where index is char index and value is byte offset is distinct.
    // We want: given byte offset (from regex), find char index.
    // Since byte offsets are monotonic, we can binary search `char_byte_starts`.
    // `char_byte_starts[i]` = byte offset where i-th char starts.

    let cnt = range.end.saturating_sub(range.start);
    let mut char_byte_starts = Vec::with_capacity(cnt + 1);

    for ch in buffer.chars(range) {
        char_byte_starts.push(s.len());
        s.push(ch.to_char_lossy());
    }
    char_byte_starts.push(s.len()); // Sentinel for end

    (s, char_byte_starts)
}

/// Convert byte offset in `s` to char offset using `char_byte_starts`.
fn byte_to_char_idx(byte_offset: usize, char_byte_starts: &[usize]) -> usize {
    // Binary search to find the index `i` such that `char_byte_starts[i] == byte_offset`.
    // Or if not exact, the largest `i` such that `char_byte_starts[i] <= byte_offset`?
    // Regex matches should align with char boundaries if we constructed string from chars.
    // So exact match should exist.
    match char_byte_starts.binary_search(&byte_offset) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1), // Should not happen for valid match boundaries
    }
}

/// Search strategy: Iterate over lines individually.
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
            if let Some(m) = search_single_line(buffer, start_line_idx, re, Some(start_pos)) {
                return Ok(Some(m));
            }

            for i in (start_line_idx + 1)..line_count {
                if let Some(m) = search_single_line(buffer, i, re, None) {
                    return Ok(Some(m));
                }
            }

            for i in 0..=start_line_idx {
                if let Some(m) = search_single_line(buffer, i, re, None) {
                    if i == start_line_idx && m.range.start >= start_pos {
                        continue;
                    }
                    return Ok(Some(m));
                }
            }
        }
        SearchDirection::Backward => {
            if let Some(m) = search_single_line_backward(buffer, start_line_idx, re, start_pos) {
                return Ok(Some(m));
            }

            for i in (0..start_line_idx).rev() {
                if let Some(m) = search_single_line_backward(buffer, i, re, usize::MAX) {
                    return Ok(Some(m));
                }
            }

            for i in ((start_line_idx + 1)..line_count).rev() {
                if let Some(m) = search_single_line_backward(buffer, i, re, usize::MAX) {
                    return Ok(Some(m));
                }
            }

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
        let line_start_val = buffer.line_start(i);
        // Determine end of line. It's safe to take next line start or buffer len.
        let line_end_val = if i + 1 < line_count {
            buffer.line_start(i + 1)
        } else {
            buffer.len()
        };

        let (line_str, mapping) =
            chars_to_string_with_mapping(buffer, line_start_val..line_end_val);

        for m in re.find_all(&line_str) {
            let char_start = byte_to_char_idx(m.start, &mapping);
            let char_end = byte_to_char_idx(m.end, &mapping);

            let abs_start = line_start_val + char_start;
            let abs_end = line_start_val + char_end;

            matches.push(SearchMatch {
                range: abs_start..abs_end,
            });
        }
    }

    Ok(matches)
}

fn search_single_line(
    buffer: &impl BufferView,
    line_idx: usize,
    re: &Regex,
    min_start_pos: Option<usize>,
) -> Option<SearchMatch> {
    let line_start_val = buffer.line_start(line_idx);
    let line_end_val = if line_idx + 1 < buffer.line_count() {
        buffer.line_start(line_idx + 1)
    } else {
        buffer.len()
    };

    let (line_str, mapping) = chars_to_string_with_mapping(buffer, line_start_val..line_end_val);

    for m in re.find_all(&line_str) {
        let char_start = byte_to_char_idx(m.start, &mapping);
        let char_end = byte_to_char_idx(m.end, &mapping);

        let abs_start = line_start_val + char_start;
        let abs_end = line_start_val + char_end;

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

fn search_single_line_backward(
    buffer: &impl BufferView,
    line_idx: usize,
    re: &Regex,
    max_start_pos: usize,
) -> Option<SearchMatch> {
    let line_start_val = buffer.line_start(line_idx);
    let line_end_val = if line_idx + 1 < buffer.line_count() {
        buffer.line_start(line_idx + 1)
    } else {
        buffer.len()
    };

    let (line_str, mapping) = chars_to_string_with_mapping(buffer, line_start_val..line_end_val);

    let mut last_valid_match = None;

    for m in re.find_all(&line_str) {
        let char_start = byte_to_char_idx(m.start, &mapping);
        let char_end = byte_to_char_idx(m.end, &mapping);

        let abs_start = line_start_val + char_start;
        let abs_end = line_start_val + char_end;

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

fn find_multiline(
    buffer: &impl BufferView,
    start_pos: usize,
    re: &Regex,
    direction: SearchDirection,
) -> Result<Option<SearchMatch>, RiftError> {
    let (full_text, mapping) = chars_to_string_with_mapping(buffer, 0..buffer.len());

    match direction {
        SearchDirection::Forward => {
            for m in re.find_all(&full_text) {
                let char_start = byte_to_char_idx(m.start, &mapping);
                if char_start >= start_pos {
                    return Ok(Some(convert_match_with_mapping(&mapping, m)));
                }
            }

            // Wrap around
            if let Some(m) = re.find(&full_text) {
                return Ok(Some(convert_match_with_mapping(&mapping, m)));
            }
        }
        SearchDirection::Backward => {
            let mut last_match = None;
            for m in re.find_all(&full_text) {
                let char_start = byte_to_char_idx(m.start, &mapping);
                if char_start < start_pos {
                    last_match = Some(m);
                } else {
                    break;
                }
            }

            if let Some(m) = last_match {
                return Ok(Some(convert_match_with_mapping(&mapping, m)));
            }

            // Wrap around
            let mut very_last_match = None;
            for m in re.find_all(&full_text) {
                very_last_match = Some(m);
            }

            if let Some(m) = very_last_match {
                let char_start = byte_to_char_idx(m.start, &mapping);
                if char_start >= start_pos {
                    return Ok(Some(convert_match_with_mapping(&mapping, m)));
                }
            }
        }
    }

    Ok(None)
}

fn find_all_multiline(buffer: &impl BufferView, re: &Regex) -> Result<Vec<SearchMatch>, RiftError> {
    let (full_text, mapping) = chars_to_string_with_mapping(buffer, 0..buffer.len());

    let mut matches = Vec::new();
    for m in re.find_all(&full_text) {
        matches.push(convert_match_with_mapping(&mapping, m));
    }
    Ok(matches)
}

fn convert_match_with_mapping(mapping: &[usize], m: monster_regex::Match) -> SearchMatch {
    let char_start = byte_to_char_idx(m.start, mapping);
    let char_end = byte_to_char_idx(m.end, mapping);
    SearchMatch {
        range: char_start..char_end,
    }
}

// Replaced simple convert_match
// fn convert_match... removed

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
                buffer.len() // Use len as end sentinel
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
