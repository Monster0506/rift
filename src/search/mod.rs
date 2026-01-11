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
use monster_regex::{parse_rift_format, Haystack, Regex};
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

mod literal_search;
use crate::search::literal_search::find_literal;

/// Find all occurrences of the pattern in the buffer.
pub fn find_all(
    buffer: &impl BufferView,
    query: &str,
) -> Result<(Vec<SearchMatch>, SearchStats), RiftError> {
    let t0 = std::time::Instant::now();

    // Classification
    // Tier 1: Literal Search

    // We need a robust Classifier.
    let tier = classify_query(query);

    match tier {
        SearchTier::Literal => {
            let pattern_orig = extract_pattern(query);
            let (pattern, check_anchor) =
                if pattern_orig.starts_with('^') && is_literal(&pattern_orig[1..]) {
                    (pattern_orig[1..].to_string(), true)
                } else {
                    (pattern_orig, false)
                };

            let t1 = std::time::Instant::now();
            let mut matches = Vec::new();
            let mut start_pos = 0;

            while let Some(m) = find_literal(buffer, &pattern, start_pos) {
                if check_anchor {
                    // Check if match is at line start
                    let is_start = if m.range.start == 0 {
                        true
                    } else {
                        // Check previous char for newline
                        if let Some(c) = buffer.iter_at(m.range.start - 1).next() {
                            c.to_char_lossy() == '\n'
                        } else {
                            false
                        }
                    };

                    if !is_start {
                        start_pos = m.range.start + 1;
                        continue;
                    }
                }

                matches.push(m.clone());
                start_pos = m.range.end;
            }
            let t3 = std::time::Instant::now();
            Ok((
                matches,
                SearchStats {
                    compilation_time: t1 - t0,
                    index_time: std::time::Duration::from_nanos(0), // No indexing for literal
                    search_time: t3 - t1,
                },
            ))
        }
        SearchTier::LineScoped => {
            // ... existing line scoped implementation ...

            let (re, pattern) = compile_regex(query)?;
            let is_anchored = pattern.starts_with('^');

            // Optimization: Inspect filter char for anchored search
            let filter_char = if is_anchored {
                // Skip ^
                let mut chars = pattern.chars().skip(1);
                chars.next().filter(|&c| c.is_alphanumeric())
            } else {
                None
            };
            // Smart case detection (same as Tier 1)
            let case_sensitive = is_anchored && pattern.chars().any(char::is_uppercase);

            let t1 = std::time::Instant::now();
            let t2 = std::time::Instant::now(); // Index start (access cache)

            let mut matches = Vec::new();

            // Try to get cache lock
            if let Some(cache_cell) = buffer.line_cache() {
                let mut cache = cache_cell.borrow_mut();
                let current_rev = buffer.revision();

                'line_loop: for line_idx in 0..buffer.line_count() {
                    let line_start_offset_char = buffer.line_start(line_idx);

                    // Pre-filter for anchored search
                    if let Some(fc) = filter_char {
                        if let Some(c) = buffer.iter_at(line_start_offset_char).next() {
                            let ch = c.to_char_lossy();
                            if case_sensitive {
                                if ch != fc {
                                    continue 'line_loop;
                                }
                            } else {
                                // Simple case-insensitive check
                                if ch.to_lowercase().next() != fc.to_lowercase().next() {
                                    continue 'line_loop;
                                }
                            }
                        }
                    }

                    let line_text = cache.get_or_insert(line_idx, current_rev, || {
                        // Materialize line
                        let start = buffer.line_start(line_idx);
                        let end = if line_idx + 1 < buffer.line_count() {
                            buffer.line_start(line_idx + 1)
                        } else {
                            buffer.len()
                        };
                        // We need string.
                        buffer
                            .chars(start..end)
                            .map(|c| c.to_char_lossy())
                            .collect()
                    });

                    let haystack = line_text;

                    if is_anchored {
                        // Use find_all(...).next() to stop after first match/attempt
                        if let Some(m) = re.find_all(haystack).next() {
                            let match_len_chars = haystack[m.start..m.end].chars().count();
                            let abs_start = line_start_offset_char; // relative start is 0
                            let abs_end = abs_start + match_len_chars;

                            matches.push(SearchMatch {
                                range: abs_start..abs_end,
                            });
                        }
                    } else {
                        // Standard line search
                        for m in re.find_all(haystack) {
                            let relative_char_start = haystack[..m.start].chars().count();
                            let match_len_chars = haystack[m.start..m.end].chars().count();

                            let abs_start = line_start_offset_char + relative_char_start;
                            let abs_end = abs_start + match_len_chars;

                            matches.push(SearchMatch {
                                range: abs_start..abs_end,
                            });
                        }
                    }
                }
            } else {
                return find_all_full_tier(buffer, query);
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
        SearchTier::Incremental => {
            let t0 = std::time::Instant::now();
            let (re, _) = compile_regex(query)?;
            let t1 = std::time::Instant::now();

            let context = BufferHaystackContext::new(buffer);
            let haystack = context.make_haystack();
            let t2 = std::time::Instant::now();

            let mut matches = Vec::new();
            let search_iter = IncrementalSearch::new(re, haystack);

            for m in search_iter {
                matches.push(m);
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

        SearchTier::Full => find_all_full_tier(buffer, query),
    }
}

fn find_all_full_tier(
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

#[derive(Debug)]
enum SearchTier {
    Literal,
    LineScoped,
    Full,
    Incremental,
}

fn classify_query(query: &str) -> SearchTier {
    let pattern = extract_pattern(query);
    // Check anchored literal
    // Allow ^literal (len > 1 to avoid just ^)
    if pattern.starts_with('^') && pattern.len() > 1 && is_literal(&pattern[1..]) {
        return SearchTier::Literal;
    }
    if is_literal(&pattern) {
        SearchTier::Literal
    } else if is_line_scoped(&pattern) {
        SearchTier::LineScoped
    } else {
        // Check for capability/complexity
        if check_complexity(&pattern) {
            SearchTier::Incremental
        } else {
            SearchTier::Full
        }
    }
}

use monster_regex::{AstNode, CharClass, Parser};

fn check_complexity(pattern: &str) -> bool {
    let flags = monster_regex::Flags::default(); // we effectively only care about structure
    let mut parser = Parser::new(pattern, flags);
    if let Ok(ast) = parser.parse() {
        // If parsing fails, we fallback to Full/Backup anyway or it will err later.
        // Check AST for specific features.
        ast.iter().any(is_node_complex)
    } else {
        // Failed to parse as AST. Treat as simple/literal for now; compile_regex will handle errors.
        false
    }
}

fn is_node_complex(node: &AstNode) -> bool {
    match node {
        AstNode::ZeroOrMore { node, .. } | AstNode::OneOrMore { node, .. } => {
            // Unbounded repetition. Check if inner is broad (like Dot or large class)
            is_broad_match(node) || is_node_complex(node)
        }
        AstNode::Range {
            max: None, node, ..
        } => is_broad_match(node) || is_node_complex(node),
        // Recursion for other containers
        AstNode::Group { nodes, .. } => nodes.iter().any(is_node_complex),
        AstNode::Alternation(nodes_vec) => {
            // nodes_vec is Vec<Vec<AstNode>>
            nodes_vec.iter().any(|alt| alt.iter().any(is_node_complex))
        }
        AstNode::LookAhead { nodes, .. } | AstNode::LookBehind { nodes, .. } => {
            // Lookarounds imply complexity.
            nodes.iter().any(is_node_complex)
        }
        _ => false,
    }
}

fn is_broad_match(node: &AstNode) -> bool {
    match node {
        AstNode::CharClass(CharClass::Dot) => true,
        AstNode::CharClass(CharClass::Set { negated: true, .. }) => true, // [^a] matches almost everything
        _ => false,
    }
}

fn extract_pattern(query: &str) -> String {
    if query.starts_with('/') {
        if let Ok((pat, _)) = parse_rift_format(query) {
            return pat;
        }
    }
    query.to_string()
}

fn is_literal(pattern: &str) -> bool {
    !pattern.chars().any(|c| ".^$*+?()[]{}|\\".contains(c))
}

fn is_line_scoped(pattern: &str) -> bool {
    !pattern.contains("\\n") && !pattern.contains("(?s)")
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
                re.find_from(haystack).map(|m| convert_match(&haystack, m))
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

use monster_regex::engine::backtracking::BacktrackingRegexEngine;
use monster_regex::engine::linear::LinearRegexEngine;
use std::sync::Arc;

/// A wrapper around either a Linear or Backtracking regex.
#[derive(Clone)]
pub enum RiftRegex {
    Linear(Arc<Regex<LinearRegexEngine>>),
    Backtracking(Arc<Regex<BacktrackingRegexEngine>>),
}

impl RiftRegex {
    pub fn find_all<'a>(
        &'a self,
        text: &'a str,
    ) -> Box<dyn Iterator<Item = monster_regex::Match> + 'a> {
        match self {
            RiftRegex::Linear(re) => Box::new(re.as_ref().find_all(text)),
            RiftRegex::Backtracking(re) => Box::new(re.as_ref().find_all(text)),
        }
    }

    pub fn find_all_from<'a, H: Haystack + Clone + 'a>(
        &'a self,
        haystack: H,
    ) -> Box<dyn Iterator<Item = monster_regex::Match> + 'a>
    where
        <LinearRegexEngine as monster_regex::engine::RegexEngine>::Regex:
            monster_regex::engine::CompiledRegexHaystack,
        <BacktrackingRegexEngine as monster_regex::engine::RegexEngine>::Regex:
            monster_regex::engine::CompiledRegexHaystack,
    {
        // monster_regex find_all_from returns FindMatchesIterator which implements Iterator.
        match self {
            RiftRegex::Linear(re) => Box::new(re.as_ref().find_all_from(haystack)),
            RiftRegex::Backtracking(re) => Box::new(re.as_ref().find_all_from(haystack)),
        }
    }

    pub fn find_from_at<H: Haystack + Clone>(
        &self,
        haystack: H,
        start: usize,
    ) -> Option<monster_regex::Match>
    where
        <LinearRegexEngine as monster_regex::engine::RegexEngine>::Regex:
            monster_regex::engine::CompiledRegexHaystack,
        <BacktrackingRegexEngine as monster_regex::engine::RegexEngine>::Regex:
            monster_regex::engine::CompiledRegexHaystack,
    {
        match self {
            RiftRegex::Linear(re) => re.as_ref().find_from_at(haystack, start),
            RiftRegex::Backtracking(re) => re.as_ref().find_from_at(haystack, start),
        }
    }

    pub fn find_from<H: Haystack + Clone>(&self, haystack: H) -> Option<monster_regex::Match>
    where
        <LinearRegexEngine as monster_regex::engine::RegexEngine>::Regex:
            monster_regex::engine::CompiledRegexHaystack,
        <BacktrackingRegexEngine as monster_regex::engine::RegexEngine>::Regex:
            monster_regex::engine::CompiledRegexHaystack,
    {
        match self {
            RiftRegex::Linear(re) => re.as_ref().find_from(haystack),
            RiftRegex::Backtracking(re) => re.as_ref().find_from(haystack),
        }
    }

    pub fn find_at(&self, text: &str, start: usize) -> Option<monster_regex::Match> {
        // Use standard slicing for find_at, consistent with backtracking implementation
        if start > text.len() {
            return None;
        }
        match self {
            RiftRegex::Linear(re) => {
                re.as_ref()
                    .find(&text[start..])
                    .map(|m| monster_regex::Match {
                        start: m.start + start,
                        end: m.end + start,
                    })
            }
            RiftRegex::Backtracking(re) => {
                re.as_ref()
                    .find(&text[start..])
                    .map(|m| monster_regex::Match {
                        start: m.start + start,
                        end: m.end + start,
                    })
            }
        }
    }
}

/// Iterator for incremental search.
/// Owns the Regex (via Arc) and the Haystack Context keeps buffer alive.
pub struct IncrementalSearch<'a, B: BufferView + ?Sized> {
    regex: RiftRegex,
    haystack: BufferHaystack<'a, B>,
    pos: usize,
}

impl<'a, B: BufferView + ?Sized> IncrementalSearch<'a, B> {
    pub fn new(regex: RiftRegex, haystack: BufferHaystack<'a, B>) -> Self {
        Self {
            regex,
            haystack,
            pos: 0,
        }
    }
}

impl<'a, B: BufferView + ?Sized> Iterator for IncrementalSearch<'a, B> {
    type Item = SearchMatch;

    fn next(&mut self) -> Option<Self::Item> {
        let m = self.regex.find_from_at(self.haystack, self.pos)?;

        // Update position for next iteration
        if m.end == m.start {
            // Empty match: must advance by 1 to avoid infinite loop
            self.pos = m.end + 1;
        } else {
            self.pos = m.end;
        }

        Some(convert_match(&self.haystack, m))
    }
}

/// Create an incremental search iterator.
///
/// Requires a pre-built `BufferHaystackContext`. This allows the context (which may be expensive to build)
/// to be reused or cached by the caller.
pub fn find_iter<'a, 'c, B: BufferView + ?Sized>(
    context: &'c BufferHaystackContext<'a, B>,
    query: &str,
) -> Result<IncrementalSearch<'c, B>, RiftError> {
    let (re, _) = compile_regex(query)?;
    let haystack = context.make_haystack();
    Ok(IncrementalSearch::new(re, haystack))
}

/// Compile query into Regex
pub fn compile_regex(query: &str) -> Result<(RiftRegex, String), RiftError> {
    let is_rift_format = query.contains('/');

    let (pattern, mut flags) = if is_rift_format {
        let query_for_parser = if query.starts_with('/') {
            std::borrow::Cow::Borrowed(query)
        } else {
            std::borrow::Cow::Owned(format!("/{}", query))
        };

        let (pat, flags) = parse_rift_format(&query_for_parser).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{:?}", e))
        })?;
        (pat, flags)
    } else {
        (query.to_string(), monster_regex::Flags::default())
    };

    // Force multiline mode
    flags.multiline = true;

    // 1. Try Linear Engine (purer, O(n))
    // heuristic: if it has anchors, fallback to backtracking for now (debugging)
    if pattern.contains('^') || pattern.contains('$') {
        let re = Regex::new(&pattern, flags).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                "REGEX_COMPILE_ERROR",
                format!("{:?}", e),
            )
        })?;
        return Ok((RiftRegex::Backtracking(Arc::new(re)), pattern));
    }

    match Regex::new_linear(&pattern, flags) {
        Ok(re) => Ok((RiftRegex::Linear(Arc::new(re)), pattern)),
        Err(_) => {
            // 2. Fallback to Backtracking Engine (supports lookarounds, backrefs, etc.)
            let re = Regex::new(&pattern, flags).map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    "REGEX_COMPILE_ERROR",
                    format!("{:?}", e),
                )
            })?;
            Ok((RiftRegex::Backtracking(Arc::new(re)), pattern))
        }
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

    for (chars_counted, c) in buffer.chars(start..end).enumerate() {
        if chars_counted == char_offset_in_line {
            break;
        }
        current_byte += c.len_utf8();
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

#[cfg(test)]
#[path = "capability_test.rs"]
mod capability_test;
