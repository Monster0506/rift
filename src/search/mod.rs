//! Search over the buffer: a literal fast-path plus regex (via monster-regex),
//! with forward/backward `find_next` and `find_all`.

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
    crate::perf_span!(
        "search_find_all",
        crate::perf::PerfFields {
            bytes: Some(query.len() as u32),
            ..Default::default()
        }
    );
    let t0 = std::time::Instant::now();

    let tier = classify_query(query);

    match tier {
        SearchTier::Literal => {
            let pattern_orig = extract_pattern(query);
            let (pattern_raw, check_anchor) =
                if pattern_orig.starts_with('^') && is_literal(&pattern_orig[1..]) {
                    (pattern_orig[1..].to_string(), true)
                } else {
                    (pattern_orig, false)
                };
            // Unescape backslash sequences (e.g. `\.` -> `.`) so the literal search
            // compares against the actual characters the user intends to match.
            let pattern = unescape_literal(&pattern_raw);

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
        // All non-literal tiers share one O(N) path: materialize once and run the
        // regex engine over a contiguous `&str`.
        SearchTier::LineScoped | SearchTier::Full | SearchTier::Incremental => {
            find_all_materialized(buffer, query, t0)
        }
    }
}

/// Find all matches by materializing the buffer into one contiguous `&str` and
/// running the regex over it. Multiline mode (forced in `compile_regex`) keeps
/// `^`/`$` line-scoped. O(N), unlike the old per-line scan whose per-line
/// `line_start` was O(N) on a single-piece buffer (overall O(N^2)).
fn find_all_materialized(
    buffer: &impl BufferView,
    query: &str,
    t0: std::time::Instant,
) -> Result<(Vec<SearchMatch>, SearchStats), RiftError> {
    if let Some(lit) = required_literal(query) {
        if find_literal(buffer, &lit, 0).is_none() {
            return Ok((Vec::new(), SearchStats::default()));
        }
    }

    let (re, _) = compile_regex(query)?;
    let t1 = std::time::Instant::now();

    let mut text = String::with_capacity(buffer.len());
    for c in buffer.iter_at(0) {
        text.push(c.to_char_lossy());
    }
    let t2 = std::time::Instant::now();

    // Engine byte offsets -> absolute char offsets; matches are ascending and
    // non-overlapping, so one forward cursor keeps the conversion O(N).
    let mut matches = Vec::new();
    let mut base_byte = 0usize;
    let mut base_char = 0usize;
    for m in re.find_all(&text) {
        base_char += text[base_byte..m.start].chars().count();
        base_byte = m.start;
        let end_char = base_char + text[m.start..m.end].chars().count();
        matches.push(SearchMatch {
            range: base_char..end_char,
        });
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

/// Returns true if `pattern` is a plain literal with no unescaped regex metacharacters.
///
/// Backslash-escaped metacharacters (`\.`, `\[`, `\\`) count as literal; real regex
/// constructs (classes, quantifiers, anchors, alternation, groups, `\b`) do not.
fn is_literal(pattern: &str) -> bool {
    // Metacharacters that, when unescaped, make a pattern non-literal.
    const UNESCAPED_SPECIALS: &str = ".^$*+?()[]{}|";

    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                // Assertions / class shorthands -> not literal.
                Some('b') | Some('B') | Some('d') | Some('D') | Some('w') | Some('W')
                | Some('s') | Some('S') | Some('A') | Some('Z') | Some('z') | Some('G') => {
                    return false;
                }
                // Control-character escapes -> let the regex engine handle these.
                Some('n') | Some('t') | Some('r') => {
                    return false;
                }
                // \X (incl. \. \[ \\): the pair is a literal char; keep scanning.
                Some(_) => {}
                // Trailing backslash: invalid, treat as non-literal.
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
                // Push the escaped character literally (e.g. \. -> .)
                out.push(next);
            }
            // Trailing backslash: is_literal already rejects this, but be safe.
        } else {
            out.push(c);
        }
    }
    out
}

/// Longest mandatory ASCII literal run in a pattern: top-level unquantified
/// `Literal` nodes only (excludes groups, alternation, optionals, repetition), so
/// it must appear verbatim in every match. Lowercased for case-insensitive
/// comparison (the most permissive check, so it never rejects a real match).
///
/// Used as a rejection gate: if this literal is absent from the buffer, the
/// pattern cannot match and the engine is skipped. `None` if no run >= 2 chars.
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
pub fn find_next(
    buffer: &impl BufferView,
    start_pos: usize,
    query: &str,
    direction: SearchDirection,
) -> Result<(Option<SearchMatch>, SearchStats), RiftError> {
    crate::perf_span!(
        "search_find_next",
        crate::perf::PerfFields {
            bytes: Some(query.len() as u32),
            ..Default::default()
        }
    );
    // Fast path: plain literals skip the engine entirely (chunk scan).
    if let SearchTier::Literal = classify_query(query) {
        return find_next_literal(buffer, start_pos, query, direction);
    }

    // Rejection gate: if a mandatory literal is absent, the pattern can't match.
    if let Some(lit) = required_literal(query) {
        if find_literal(buffer, &lit, 0).is_none() {
            return Ok((None, SearchStats::default()));
        }
    }

    let t0 = std::time::Instant::now();
    let (re, _) = compile_regex(query)?;
    let t1 = std::time::Instant::now();

    // Run the regex over a contiguous `&str`, far faster than the streaming
    // `BufferHaystack` (whose per-char probes are O(log N) tree descents). Lossy
    // chars match the haystack byte model, so offsets are identical.
    let mut text = String::with_capacity(buffer.len());
    let mut start_byte = None;
    for (i, c) in buffer.iter_at(0).enumerate() {
        if i == start_pos {
            start_byte = Some(text.len());
        }
        text.push(c.to_char_lossy());
    }
    let start_byte = start_byte.unwrap_or(text.len());
    let t2 = std::time::Instant::now();

    let result = match direction {
        SearchDirection::Forward => {
            // First match at/after the cursor; otherwise wrap to the first match overall.
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
            // Last match before the cursor; otherwise wrap to the last match overall.
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

/// Convert a byte range within a materialized string into a `SearchMatch` whose
/// range is expressed in absolute code-point (char) offsets.
fn byte_range_to_char_match(text: &str, (byte_start, byte_end): (usize, usize)) -> SearchMatch {
    let char_start = text[..byte_start].chars().count();
    let char_end = char_start + text[byte_start..byte_end].chars().count();
    SearchMatch {
        range: char_start..char_end,
    }
}

/// Extract the literal pattern (and anchor flag) from a query already classified
/// as `SearchTier::Literal`, mirroring the logic in `find_all`'s literal branch.
fn literal_pattern(query: &str) -> (String, bool) {
    let pattern_orig = extract_pattern(query);
    let (pattern_raw, check_anchor) =
        if pattern_orig.starts_with('^') && is_literal(&pattern_orig[1..]) {
            (pattern_orig[1..].to_string(), true)
        } else {
            (pattern_orig, false)
        };
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
                    .map(|c| c.to_char_lossy() == '\n')
                    .unwrap_or(false);
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
) -> Result<(Option<SearchMatch>, SearchStats), RiftError> {
    let t0 = std::time::Instant::now();
    let (pattern, check_anchor) = literal_pattern(query);
    let t1 = std::time::Instant::now();

    let result = match direction {
        SearchDirection::Forward => {
            // Search forward from the cursor; wrap to the start if nothing found.
            next_literal_match(buffer, &pattern, check_anchor, start_pos).or_else(|| {
                if start_pos > 0 {
                    next_literal_match(buffer, &pattern, check_anchor, 0)
                } else {
                    None
                }
            })
        }
        SearchDirection::Backward => {
            // Walk all matches, tracking the last one before the cursor (and the
            // last one overall, for wrap-around behavior matching the regex path).
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

    let t2 = std::time::Instant::now();
    Ok((
        result,
        SearchStats {
            compilation_time: t1 - t0,
            index_time: std::time::Duration::from_nanos(0),
            search_time: t2 - t1,
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

/// Create an incremental search iterator over a pre-built `BufferHaystackContext`.
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
    // Rift format (`pattern/flags`) only applies when the query is delimiter-led,
    // matching `extract_pattern`'s own definition.
    let is_rift_format = query.starts_with('/');

    let (pattern, mut flags) = if is_rift_format {
        let (pat, flags) = parse_rift_format(query).map_err(|e| {
            RiftError::new(ErrorType::Internal, "REGEX_PARSE_ERROR", format!("{:?}", e))
        })?;
        (pat, flags)
    } else {
        let mut flags = monster_regex::Flags::default();
        if flags.ignore_case.is_none() {
            flags.ignore_case = Some(!query.chars().any(|c| c.is_uppercase()));
        }
        (query.to_string(), flags)
    };

    flags.multiline = true;

    // Anchors go to the backtracking engine; otherwise prefer the linear engine.
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "capability_test.rs"]
mod capability_test;
