//! Search functionality for Rift
//!
//! Implements efficient regex search over the buffer using a streaming `Haystack` implementation.
//! This avoids allocating a contiguous string for the entire buffer.
//!
//! Supports:
//! - Streaming search (zero-copy from buffer)
//! - Rift-style pattern/flags parsing (via monster-regex)
//! - Forward and Backward search

use crate::buffer::api::BufferView;
use crate::error::{ErrorType, RiftError};
use haystack::{BufferHaystack, BufferHaystackContext};
use monster_regex::{parse_rift_format, Regex};
use std::ops::Range;

mod haystack;

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

#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub compilation_time: std::time::Duration,
    pub index_time: std::time::Duration,
    pub search_time: std::time::Duration,
}

/// Find all occurrences of the pattern in the buffer.
pub fn find_all(
    buffer: &impl BufferView,
    query: &str,
) -> Result<(Vec<SearchMatch>, SearchStats), RiftError> {
    let t0 = std::time::Instant::now();
    let (re, _) = compile_regex(query)?;
    let t1 = std::time::Instant::now();

    let context = BufferHaystackContext::new(buffer);
    let haystack = context.make_haystack();
    let t2 = std::time::Instant::now();

    let mut matches = Vec::new();
    for m in re.find_all_from(haystack) {
        matches.push(convert_match(&haystack, m));
    }
    let t3 = std::time::Instant::now();

    Ok((
        matches,
        SearchStats {
            compilation_time: t1 - t0,
            index_time: t2 - t1,
            search_time: t3 - t2,
        },
    ))
}

/// Find the next occurrence of the pattern in the buffer.
pub fn find_next(
    buffer: &impl BufferView,
    start_pos: usize,
    query: &str,
    direction: SearchDirection,
) -> Result<(Option<SearchMatch>, SearchStats), RiftError> {
    let t0 = std::time::Instant::now();
    let (re, _) = compile_regex(query)?;
    let t1 = std::time::Instant::now();

    let context = BufferHaystackContext::new(buffer);
    let haystack = context.make_haystack();
    let t2 = std::time::Instant::now();

    let result = match direction {
        SearchDirection::Forward => {
            // We need to map `start_pos` (code-point offset) to a byte offset for the regex engine.
            let start_byte = char_to_byte_offset(buffer, start_pos);

            // Search from start_byte
            if let Some(m) = re.find_from_at(haystack, start_byte) {
                Some(convert_match(&haystack, m))
            } else {
                // Wrap around: Search from 0
                if let Some(m) = re.find_from(haystack) {
                    Some(convert_match(&haystack, m))
                } else {
                    None
                }
            }
        }
        SearchDirection::Backward => {
            // Backward search by iterating all matches (monster-regex doesn't support native reverse search yet)
            let mut last_valid_match = None;
            let start_byte = char_to_byte_offset(buffer, start_pos);

            for m in re.find_all_from(haystack) {
                if m.start < start_byte {
                    last_valid_match = Some(m);
                } else {
                    break;
                }
            }

            if let Some(m) = last_valid_match {
                Some(convert_match(&haystack, m))
            } else {
                // Wrap around: Find the very last match in the file
                let mut very_last = None;
                for m in re.find_all_from(haystack) {
                    very_last = Some(m);
                }
                if let Some(m) = very_last {
                    // Check if it's actually after start_pos (it effectively wraps to the end)
                    if m.start >= start_byte {
                        Some(convert_match(&haystack, m))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    };

    let t3 = std::time::Instant::now();

    Ok((
        result,
        SearchStats {
            compilation_time: t1 - t0,
            index_time: t2 - t1,
            search_time: t3 - t2,
        },
    ))
}

fn convert_match<B: BufferView + ?Sized>(
    haystack: &BufferHaystack<B>,
    m: monster_regex::Match,
) -> SearchMatch {
    let char_start = haystack.byte_offset_to_char_abs(m.start);
    let char_end = haystack.byte_offset_to_char_abs(m.end);
    SearchMatch {
        range: char_start..char_end,
    }
}

/// Compile query into Regex
fn compile_regex(query: &str) -> Result<(Regex, String), RiftError> {
    let is_rift_format = query.contains('/');

    if is_rift_format {
        let query_for_parser = if query.starts_with('/') {
            std::borrow::Cow::Borrowed(query)
        } else {
            std::borrow::Cow::Owned(format!("/{}", query))
        };

        let (pattern, mut flags) = parse_rift_format(&query_for_parser).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{:?}", e))
        })?;

        // Force multiline mode to match legacy line-by-line behavior where ^/$ matched line boundaries
        flags.multiline = true;

        let re = Regex::new(&pattern, flags).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "REGEX_COMPILE_ERROR",
                format!("{:?}", e),
            )
        })?;
        Ok((re, pattern))
    } else {
        let mut flags = monster_regex::Flags::default();
        // Force multiline mode
        flags.multiline = true;

        let re = Regex::new(query, flags).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "REGEX_COMPILE_ERROR",
                format!("{:?}", e),
            )
        })?;
        Ok((re, query.to_string()))
    }
}

/// Helper to convert a code-point offset to a byte offset using the buffer's structure.
/// This matches the logic of `BufferHaystack`'s virtual buffer (byte counting).
fn char_to_byte_offset(buffer: &impl BufferView, char_pos: usize) -> usize {
    let line_idx = find_line_index(buffer, char_pos);
    let line_start = buffer.line_start(line_idx);
    let char_offset_in_line = char_pos - line_start;

    let mut current_byte = 0;

    // 1. Sum previous lines
    for i in 0..line_idx {
        let start = buffer.line_start(i);
        let end = if i + 1 < buffer.line_count() {
            buffer.line_start(i + 1)
        } else {
            buffer.len()
        };
        for c in buffer.chars(start..end) {
            current_byte += c.len_utf8();
        }
    }

    // 2. Add bytes in current line up to char_offset_in_line
    let start = buffer.line_start(line_idx);
    let end = if line_idx + 1 < buffer.line_count() {
        buffer.line_start(line_idx + 1)
    } else {
        buffer.len()
    };

    let mut chars_counted = 0;
    for c in buffer.chars(start..end) {
        if chars_counted == char_offset_in_line {
            break;
        }
        current_byte += c.len_utf8();
        chars_counted += 1;
    }

    current_byte
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
                buffer.len()
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
