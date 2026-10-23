//! Search over the buffer: a literal fast-path plus regex (via monster-regex),
//! with forward/backward `find_next` and `find_all`.

use crate::buffer::api::BufferView;
use crate::error::{ErrorType, RiftError};
#[cfg(test)]
use haystack::{BufferHaystack, BufferHaystackContext};
#[cfg(test)]
use monster_regex::Haystack;
use monster_regex::{parse_rift_format, Regex};
use std::ops::Range;

#[cfg(test)]
mod haystack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub(crate) range: Range<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchStats;

mod literal_search;
use crate::search::literal_search::find_literal;

/// Find all occurrences of the pattern in the buffer.
///
/// # Errors
/// Returns an error if `query` does not compile as a pattern.
pub fn find_all(
    buffer: &impl BufferView,
    query: &str,
) -> Result<(Vec<SearchMatch>, SearchStats), RiftError> {
    crate::perf_span!(
        "search_find_all",
        crate::perf::PerfFields {
            bytes: Some(u32::try_from(query.len()).unwrap_or(u32::MAX)),
            ..Default::default()
        }
    );

    let tier = classify_query(query);

    match tier {
        SearchTier::Literal => {
            let (pattern, check_anchor) = literal_pattern(query);

            if pattern.is_empty() {
                return Ok((Vec::new(), SearchStats));
            }

            let mut matches = Vec::new();
            let mut start_pos = 0;

            while let Some(m) = find_literal(buffer, &pattern, start_pos) {
                if check_anchor {
                    let is_start = if m.range.start == 0 {
                        true
                    } else if let Some(c) = buffer.iter_at(m.range.start - 1).next() {
                        c.to_char_lossy() == '\n'
                    } else {
                        false
                    };

                    if !is_start {
                        start_pos = m.range.start + 1;
                        continue;
                    }
                }

                matches.push(m.clone());
                start_pos = if m.range.end > start_pos {
                    m.range.end
                } else {
                    start_pos + 1
                };
            }
            Ok((matches, SearchStats))
        }
        SearchTier::LineScoped | SearchTier::Full | SearchTier::Incremental => {
            find_all_materialized(buffer, query)
        }
    }
}

/// Find all matches by materializing the buffer into one contiguous `&str` and
/// running the regex over it in one O(N) pass, vs. the old O(N^2) per-line scan.
fn find_all_materialized(
    buffer: &impl BufferView,
    query: &str,
) -> Result<(Vec<SearchMatch>, SearchStats), RiftError> {
    if let Some(lit) = required_literal(query) {
        if find_literal(buffer, &lit, 0).is_none() {
            return Ok((Vec::new(), SearchStats));
        }
    }

    let (re, _) = compile_regex(query)?;

    let mut text = String::with_capacity(buffer.len());
    for c in buffer.iter_at(0) {
        text.push(c.to_char_lossy());
    }

    let mut matches = Vec::new();
    let mut base_byte = 0usize;
    let mut base_char = 0usize;
    for m in re.find_all(&text) {
        base_char += chars_between(&text, base_byte, m.start);
        base_byte = m.start;
        let end_char = base_char + chars_between(&text, m.start, m.end);
        matches.push(SearchMatch {
            range: base_char..end_char,
        });
    }

    Ok((matches, SearchStats))
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
    if anchored_literal(&pattern).is_some_and(|rest| !rest.is_empty()) {
        return SearchTier::Literal;
    }
    if is_literal(&pattern) {
        SearchTier::Literal
    } else if is_line_scoped(&pattern) {
        SearchTier::LineScoped
    } else if check_complexity(&pattern) {
        SearchTier::Incremental
    } else {
        SearchTier::Full
    }
}

use monster_regex::{AstNode, CharClass, Parser};

fn check_complexity(pattern: &str) -> bool {
    let mut parser = Parser::new(pattern, monster_regex::Flags::default());
    parser
        .parse()
        .is_ok_and(|ast| ast.iter().any(is_node_complex))
}

fn is_node_complex(node: &AstNode) -> bool {
    match node {
        AstNode::ZeroOrMore { node, .. } | AstNode::OneOrMore { node, .. } => {
            is_broad_match(node) || is_node_complex(node)
        }
        AstNode::Range {
            max: None, node, ..
        } => is_broad_match(node) || is_node_complex(node),
        AstNode::Group { nodes, .. } => nodes.iter().any(is_node_complex),
        AstNode::Alternation(nodes_vec) => {
            nodes_vec.iter().any(|alt| alt.iter().any(is_node_complex))
        }
        AstNode::LookAhead { nodes, .. } | AstNode::LookBehind { nodes, .. } => {
            nodes.iter().any(is_node_complex)
        }
        _ => false,
    }
}

const fn is_broad_match(node: &AstNode) -> bool {
    matches!(node, AstNode::CharClass(CharClass::Dot))
}

fn extract_pattern(query: &str) -> String {
    if query.starts_with('/') {
        if let Ok((pat, _)) = parse_rift_format(query) {
            return pat;
        }
    }
    query.to_string()
}

/// Returns true if `pattern` is a plain literal with no unescaped regex metacharacters.
/// Backslash-escaped metacharacters (`\.`, `\[`, `\\`) count as literal; regex constructs don't.
fn is_literal(pattern: &str) -> bool {
    const UNESCAPED_SPECIALS: &str = ".^$*+?()[]{}|";

    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(
                    'b' | 'B' | 'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'A' | 'Z' | 'z' | 'G' | 'n'
                    | 't' | 'r',
                ) => {
                    return false;
                }
                Some(_) => {}
                None => return false,
            }
        } else if UNESCAPED_SPECIALS.contains(c) {
            return false;
        }
    }
    true
}

fn is_line_scoped(pattern: &str) -> bool {
    !pattern.contains("\\n") && !pattern.contains("(?s)")
}

/// Unescape a literal pattern (`\.` -> `.`, `\\` -> `\`) for plain-text comparison.
/// Only valid for patterns that `is_literal` approved.
fn unescape_literal(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Longest mandatory ASCII literal run (top-level unquantified nodes, lowercased) that
/// must appear in every match; used to skip the engine when absent. `None` if run < 2 chars.
fn required_literal(query: &str) -> Option<String> {
    let pattern = extract_pattern(query);
    let ast = Parser::new(&pattern, monster_regex::Flags::default())
        .parse()
        .ok()?;

    let mut best = String::new();
    let mut current = String::new();
    for node in &ast {
        match node {
            AstNode::Literal(c) if c.is_ascii() => current.push(c.to_ascii_lowercase()),
            _ => {
                if current.len() > best.len() {
                    std::mem::swap(&mut best, &mut current);
                }
                current.clear();
            }
        }
    }
    if current.len() > best.len() {
        best = current;
    }

    (best.len() >= 2).then_some(best)
}

/// Find the next occurrence of the pattern in the buffer.
///
/// # Errors
/// Returns an error if `query` does not compile as a pattern.
pub(crate) fn find_next(
    buffer: &impl BufferView,
    start_pos: usize,
    query: &str,
    direction: SearchDirection,
) -> Result<(Option<SearchMatch>, SearchStats), RiftError> {
    crate::perf_span!(
        "search_find_next",
        crate::perf::PerfFields {
            bytes: Some(u32::try_from(query.len()).unwrap_or(u32::MAX)),
            ..Default::default()
        }
    );
    if matches!(classify_query(query), SearchTier::Literal) {
        return Ok(find_next_literal(buffer, start_pos, query, direction));
    }

    if let Some(lit) = required_literal(query) {
        if find_literal(buffer, &lit, 0).is_none() {
            return Ok((None, SearchStats));
        }
    }

    let (re, _) = compile_regex(query)?;

    let mut text = String::with_capacity(buffer.len());
    let mut start_byte = None;
    for (i, c) in buffer.iter_at(0).enumerate() {
        if i == start_pos {
            start_byte = Some(text.len());
        }
        text.push(c.to_char_lossy());
    }
    let start_byte = start_byte.unwrap_or(text.len());

    let result = match direction {
        SearchDirection::Forward => {
            let mut first_overall: Option<(usize, usize)> = None;
            let mut after: Option<(usize, usize)> = None;
            for m in re.find_all(&text) {
                if first_overall.is_none() {
                    first_overall = Some((m.start, m.end));
                }
                if m.start >= start_byte {
                    after = Some((m.start, m.end));
                    break;
                }
            }
            after
                .or(if start_pos > 0 { first_overall } else { None })
                .map(|m| byte_range_to_char_match(&text, m))
        }
        SearchDirection::Backward => {
            let mut last_before: Option<(usize, usize)> = None;
            let mut last_overall: Option<(usize, usize)> = None;
            for m in re.find_all(&text) {
                if m.start < start_byte {
                    last_before = Some((m.start, m.end));
                }
                last_overall = Some((m.start, m.end));
            }
            last_before
                .or(last_overall)
                .map(|m| byte_range_to_char_match(&text, m))
        }
    };

    Ok((result, SearchStats))
}

/// Convert a byte range within a materialized string into a `SearchMatch` whose
/// range is expressed in absolute code-point (char) offsets.
fn byte_range_to_char_match(text: &str, (byte_start, byte_end): (usize, usize)) -> SearchMatch {
    let char_start = chars_between(text, 0, byte_start);
    let char_end = char_start + chars_between(text, byte_start, byte_end);
    SearchMatch {
        range: char_start..char_end,
    }
}

/// Counts chars in `text[from..to]`; regex offsets always sit on char boundaries.
fn chars_between(text: &str, from: usize, to: usize) -> usize {
    text.get(from..to).map_or(0, |s| s.chars().count())
}

/// Returns the literal after a leading `^` anchor, if there is one.
fn anchored_literal(pattern: &str) -> Option<&str> {
    pattern.strip_prefix('^').filter(|rest| is_literal(rest))
}

/// Extract the literal pattern (and anchor flag) from a query already classified
/// as `SearchTier::Literal`, mirroring the logic in `find_all`'s literal branch.
fn literal_pattern(query: &str) -> (String, bool) {
    let pattern_orig = extract_pattern(query);
    let anchored = anchored_literal(&pattern_orig).map(str::to_string);
    let (pattern_raw, check_anchor) = anchored.map_or((pattern_orig, false), |rest| (rest, true));
    (unescape_literal(&pattern_raw), check_anchor)
}

/// Find the first literal match at or after `from`, honoring an optional `^`
/// line-start anchor.
fn next_literal_match(
    buffer: &impl BufferView,
    pattern: &str,
    check_anchor: bool,
    from: usize,
) -> Option<SearchMatch> {
    let mut start_pos = from;
    while let Some(m) = find_literal(buffer, pattern, start_pos) {
        if check_anchor {
            let is_start = m.range.start == 0
                || buffer
                    .iter_at(m.range.start - 1)
                    .next()
                    .is_some_and(|c| c.to_char_lossy() == '\n');
            if !is_start {
                start_pos = m.range.start + 1;
                continue;
            }
        }
        return Some(m);
    }
    None
}

/// Literal fast path for `find_next`. Avoids regex compilation and the streaming
/// haystack entirely, scanning the buffer's chunk iterator directly.
fn find_next_literal(
    buffer: &impl BufferView,
    start_pos: usize,
    query: &str,
    direction: SearchDirection,
) -> (Option<SearchMatch>, SearchStats) {
    let (pattern, check_anchor) = literal_pattern(query);

    let result = match direction {
        SearchDirection::Forward => next_literal_match(buffer, &pattern, check_anchor, start_pos)
            .or_else(|| {
                if start_pos > 0 {
                    next_literal_match(buffer, &pattern, check_anchor, 0)
                } else {
                    None
                }
            }),
        SearchDirection::Backward => {
            let mut last_before = None;
            let mut last_overall = None;
            let mut scan = 0;
            while let Some(m) = next_literal_match(buffer, &pattern, check_anchor, scan) {
                if m.range.start < start_pos {
                    last_before = Some(m.clone());
                }
                scan = m.range.end.max(m.range.start + 1);
                last_overall = Some(m);
            }
            last_before.or(last_overall)
        }
    };

    (result, SearchStats)
}

#[cfg(test)]
fn convert_match<B: BufferView + ?Sized>(
    haystack: &BufferHaystack<B>,
    m: &monster_regex::Match,
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
    #[must_use]
    fn find_all<'a>(
        &'a self,
        text: &'a str,
    ) -> Box<dyn Iterator<Item = monster_regex::Match> + 'a> {
        match self {
            Self::Linear(re) => Box::new(re.as_ref().find_all(text)),
            Self::Backtracking(re) => Box::new(re.as_ref().find_all(text)),
        }
    }

    #[cfg(test)]
    fn find_from_at<H: Haystack + Clone>(
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
            Self::Linear(re) => re.as_ref().find_from_at(haystack, start),
            Self::Backtracking(re) => re.as_ref().find_from_at(haystack, start),
        }
    }
}

#[cfg(test)]
/// Iterator for incremental search.
/// Owns the Regex (via Arc) and the Haystack Context keeps buffer alive.
struct IncrementalSearch<'a, B: BufferView + ?Sized> {
    regex: RiftRegex,
    haystack: BufferHaystack<'a, B>,
    pos: usize,
}

#[cfg(test)]
impl<'a, B: BufferView + ?Sized> IncrementalSearch<'a, B> {
    #[must_use]
    const fn new(regex: RiftRegex, haystack: BufferHaystack<'a, B>) -> Self {
        Self {
            regex,
            haystack,
            pos: 0,
        }
    }
}

#[cfg(test)]
impl<B: BufferView + ?Sized> Iterator for IncrementalSearch<'_, B> {
    type Item = SearchMatch;

    fn next(&mut self) -> Option<Self::Item> {
        let m = self.regex.find_from_at(self.haystack, self.pos)?;

        if m.end == m.start {
            self.pos = m.end + 1;
        } else {
            self.pos = m.end;
        }

        Some(convert_match(&self.haystack, &m))
    }
}

#[cfg(test)]
/// Create an incremental search iterator over a pre-built `BufferHaystackContext`.
///
/// # Errors
/// Returns an error if `query` does not compile as a pattern.
fn find_iter<'c, B: BufferView + ?Sized>(
    context: &'c BufferHaystackContext<'_, B>,
    query: &str,
) -> Result<IncrementalSearch<'c, B>, RiftError> {
    let (re, _) = compile_regex(query)?;
    let haystack = context.make_haystack();
    Ok(IncrementalSearch::new(re, haystack))
}

/// Compile query into Regex
///
/// # Errors
/// Returns an error if `query` is not a valid rift-format or regex pattern.
pub fn compile_regex(query: &str) -> Result<(RiftRegex, String), RiftError> {
    let is_rift_format = query.starts_with('/');

    let (pattern, mut flags) = if is_rift_format {
        let (pat, flags) = parse_rift_format(query).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{e:?}"))
        })?;
        (pat, flags)
    } else {
        let mut flags = monster_regex::Flags::default();
        if flags.ignore_case.is_none() {
            flags.ignore_case = Some(!query.chars().any(char::is_uppercase));
        }
        (query.to_string(), flags)
    };

    flags.multiline = true;

    if pattern.contains('^') || pattern.contains('$') {
        let re = Regex::new(&pattern, flags).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_COMPILE_ERROR", format!("{e:?}"))
        })?;
        return Ok((RiftRegex::Backtracking(Arc::new(re)), pattern));
    }

    if let Ok(re) = Regex::new_linear(&pattern, flags) {
        Ok((RiftRegex::Linear(Arc::new(re)), pattern))
    } else {
        let re = Regex::new(&pattern, flags).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_COMPILE_ERROR", format!("{e:?}"))
        })?;
        Ok((RiftRegex::Backtracking(Arc::new(re)), pattern))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "capability_test.rs"]
mod capability_test;
