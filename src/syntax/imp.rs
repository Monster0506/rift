use super::source::Source;
use super::ParseOutcome;
use crate::buffer::TextBuffer;
use crate::job_manager::jobs::syntax::SyntaxParseResult;
use crate::syntax::loader::LoadedLanguage;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Parser, Query, QueryCursor, Tree};

/// Target highlight chunk size; each chunk extends to the next line start.
const CHUNK_BYTES: usize = 4096;

type Highlight = (Range<usize>, u32);

// Highlight chunks

/// Highlights of one line-aligned slice of the document, clipped to it.
struct Chunk {
    range: Range<usize>,
    items: Vec<Highlight>,
}

/// Highlights of one tree, queried a chunk at a time and only on request.
/// Chunk `k` starts at the first line start at or after `k * CHUNK_BYTES`.
#[derive(Default)]
struct ChunkCache {
    chunks: BTreeMap<usize, Chunk>,
}

impl ChunkCache {
    /// Drops the chunk touching `byte` and every later chunk.
    fn invalidate_from(&mut self, byte: usize) -> bool {
        if self
            .chunks
            .last_key_value()
            .is_none_or(|(_, c)| c.range.end < byte)
        {
            return false;
        }

        let start = self
            .chunks
            .range(..byte)
            .next_back()
            .filter(|(_, c)| c.range.end >= byte)
            .map_or(byte, |(&start, _)| start);
        self.chunks.split_off(&start);
        true
    }

    fn clear(&mut self) {
        self.chunks.clear();
    }

    /// Highlights overlapping `range`, querying any chunk not cached yet.
    fn highlights(
        &mut self,
        source: Source,
        tree: &Tree,
        query: &Query,
        range: Range<usize>,
    ) -> Vec<Highlight> {
        let end = range.end.min(source.len());
        let mut out = Vec::new();
        if range.start >= end {
            return out;
        }

        let mut start = source.line_start(range.start);
        while start < end {
            let index = start / CHUNK_BYTES;
            let bounds = source.next_line_start(index * CHUNK_BYTES)
                ..source.next_line_start((index + 1) * CHUNK_BYTES);
            let chunk = self
                .chunks
                .entry(bounds.start)
                .or_insert_with(|| query_chunk(source, tree, query, bounds));

            out.extend(
                chunk
                    .items
                    .iter()
                    .filter(|(r, _)| r.start < end && r.end > range.start)
                    .cloned(),
            );
            start = chunk.range.end;
        }
        out
    }
}

fn query_chunk(source: Source, tree: &Tree, query: &Query, bounds: Range<usize>) -> Chunk {
    crate::perf_span!(
        "syntax_highlight_chunk",
        crate::perf::PerfFields {
            bytes: Some(u32::try_from(bounds.len()).unwrap_or(u32::MAX)),
            ..Default::default()
        }
    );

    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(bounds.clone());
    let mut matches = cursor.matches(query, tree.root_node(), source.node_text());
    let mut found = Vec::new();
    while let Some(m) = matches.next() {
        for capture in m.captures() {
            found.push((capture.node.byte_range(), capture.index, m.pattern_index));
        }
    }

    let items = flatten_highlights(found)
        .into_iter()
        .filter_map(|(r, capture)| {
            let clipped = r.start.max(bounds.start)..r.end.min(bounds.end);
            (!clipped.is_empty()).then_some((clipped, capture))
        })
        .collect();
    Chunk {
        range: bounds,
        items,
    }
}

/// First byte whose highlights may differ between `old` (already edited
/// with `edits`) and its reparse `new`.
fn reparse_start(old: &Tree, new: &Tree, edits: &[InputEdit]) -> usize {
    let root = new.root_node();
    // A token absorbing an edit (e.g. a block comment) can start in an
    // earlier chunk without showing up in `changed_ranges`.
    let edit_start = edits
        .iter()
        .map(|e| {
            root.descendant_for_byte_range(e.start_byte, e.new_end_byte)
                .map_or(e.start_byte, |n| n.start_byte().min(e.start_byte))
        })
        .min();

    old.changed_ranges(new)
        .map(|r| r.start_byte)
        .chain(edit_start)
        .min()
        .unwrap_or(usize::MAX)
}

/// Parses `buffer` straight from its piece table, for callers without a `Syntax`.
pub(crate) fn parse_buffer(
    parser: &mut Parser,
    buffer: &TextBuffer,
    old_tree: Option<&Tree>,
) -> Option<Tree> {
    Source::new(buffer).parse(parser, old_tree, None)
}

// Injection layer

/// A single embedded-language layer (e.g. TypeScript inside a Svelte `<script>`).
struct InjectedLayer {
    language: tree_sitter::Language,
    language_name: String,
    highlights_query: Option<Arc<Query>>,
    tree: Option<Tree>,
    highlights: ChunkCache,
}

impl InjectedLayer {
    fn new(
        language: tree_sitter::Language,
        language_name: String,
        highlights_query: Option<Arc<Query>>,
    ) -> Self {
        Self {
            language,
            language_name,
            highlights_query,
            tree: None,
            highlights: ChunkCache::default(),
        }
    }

    /// Returns whether highlights the renderer may have seen were dropped.
    fn reset(&mut self) -> bool {
        self.highlights.clear();
        self.tree.take().is_some()
    }

    /// Reparses over the host byte `ranges`; returns whether highlights the
    /// renderer may have seen were dropped.
    fn reparse(&mut self, source: Source, ranges: &[Range<usize>], edits: &[InputEdit]) -> bool {
        if ranges.is_empty() {
            return self.reset();
        }

        let ts_ranges: Vec<tree_sitter::Range> = ranges
            .iter()
            .map(|r| tree_sitter::Range {
                start_byte: r.start,
                end_byte: r.end,
                start_point: source.point(r.start),
                end_point: source.point(r.end),
            })
            .collect();

        let mut parser = Parser::new();
        if parser.set_language(&self.language).is_err()
            || parser.set_included_ranges(&ts_ranges).is_err()
        {
            return false;
        }
        let Some(new_tree) = source.parse(&mut parser, self.tree.as_ref(), None) else {
            return false;
        };

        let dropped = if let Some(old) = &self.tree {
            self.highlights
                .invalidate_from(reparse_start(old, &new_tree, edits))
        } else {
            self.highlights.clear();
            true
        };
        self.tree = Some(new_tree);
        dropped
    }

    fn highlights_named(
        &mut self,
        source: Source,
        range: Range<usize>,
    ) -> Vec<(Range<usize>, String)> {
        let Self {
            tree: Some(tree),
            highlights_query: Some(query),
            highlights,
            ..
        } = self
        else {
            return Vec::new();
        };

        let names = query.capture_names();
        highlights
            .highlights(source, tree, query, range)
            .into_iter()
            .filter_map(|(r, capture)| {
                names
                    .get(capture as usize)
                    .map(|name| (r, (*name).to_string()))
            })
            .collect()
    }
}

// Syntax

/// Per-document syntax state: the host grammar plus injection layers
/// (sub-regions re-parsed with a different grammar, e.g. TS inside `<script>`).
pub(crate) struct Syntax {
    pub(crate) language: tree_sitter::Language,
    pub(crate) tree: Option<Tree>,
    pub(crate) highlights_query: Option<Arc<Query>>,
    pub(crate) language_name: String,
    highlights: ChunkCache,
    /// Keeps `language`'s backing dynamic library alive for as long as this
    /// `Syntax` (and any `Tree`/`Parser` derived from it) may exist.
    lib: Option<Arc<libloading::Library>>,

    // --- Static injection support (Svelte, HTML: capture name = language name) ---
    pub(crate) injections_query: Option<Arc<Query>>,
    injection_capture_langs: Vec<(u32, String)>,
    injection_layers: Vec<InjectedLayer>,

    // Dynamic injection support (Markdown: injection.language + injection.content)
    /// Layers created on demand, cached by language name so queries/trees are reused.
    dynamic_injection_layers: BTreeMap<String, InjectedLayer>,

    /// Optional loader used to create dynamic injection layers.
    language_loader: Option<Arc<super::loader::LanguageLoader>>,

    /// Edits since the last completed parse, bounding what it invalidates.
    pending_edits: Vec<InputEdit>,
    /// Bumped whenever host highlights the renderer may have seen are dropped,
    /// never when a chunk is filled - lets callers detect real changes.
    highlights_generation: u64,
    /// Same as `highlights_generation`, for every injection layer.
    injection_generation: u64,
}

impl Syntax {
    /// Create a new Syntax instance (no injections).
    pub(crate) fn new(loaded: LoadedLanguage, highlights_query: Option<Arc<Query>>) -> Self {
        Self {
            language: loaded.language,
            tree: None,
            highlights_query,
            language_name: loaded.name,
            highlights: ChunkCache::default(),
            lib: loaded.lib,
            injections_query: None,
            injection_capture_langs: Vec::new(),
            injection_layers: Vec::new(),
            dynamic_injection_layers: BTreeMap::new(),
            language_loader: None,
            pending_edits: Vec::new(),
            highlights_generation: 0,
            injection_generation: 0,
        }
    }

    /// Monotonic count of host highlight invalidations, for external
    /// staleness gates that must not react to mere queries.
    #[must_use]
    pub(crate) const fn highlights_generation(&self) -> u64 {
        self.highlights_generation
    }

    /// Monotonic count of injection-layer highlight invalidations (static or
    /// dynamic), same reasoning as `highlights_generation`.
    #[must_use]
    pub(crate) const fn injection_generation(&self) -> u64 {
        self.injection_generation
    }

    /// The library keeping this syntax's `language` alive, if it came
    /// from a dynamically loaded grammar. Clone it into anything that outlives `self`.
    #[must_use]
    pub(crate) fn lib(&self) -> Option<Arc<libloading::Library>> {
        self.lib.clone()
    }

    /// Attach an injections query and pre-built injection layers (static protocol).
    fn set_injections(
        &mut self,
        injections_query: Arc<Query>,
        capture_langs: Vec<(u32, String)>,
        layers: Vec<InjectedLayer>,
    ) {
        self.injections_query = Some(injections_query);
        self.injection_capture_langs = capture_langs;
        self.injection_layers = layers;
        self.injection_generation += 1;
    }

    // Update from background job result

    pub(crate) fn update_from_result(&mut self, result: SyntaxParseResult, buffer: &TextBuffer) {
        if result.language_name != self.language_name {
            return;
        }
        if let Some(tree) = result.tree {
            self.apply_tree(tree, Source::new(buffer));
        }
    }

    /// Discard all cached trees and highlights after a non-incremental change (e.g. undo/redo).
    /// The next `incremental_parse()` will do a full re-parse from scratch.
    pub(crate) fn invalidate_trees(&mut self) {
        self.tree = None;
        self.highlights.clear();
        self.highlights_generation += 1;
        self.pending_edits.clear();
        for layer in &mut self.injection_layers {
            layer.reset();
        }
        for layer in self.dynamic_injection_layers.values_mut() {
            layer.reset();
        }
        self.injection_generation += 1;
    }

    /// Notify the syntax tree of an edit, in the caller's own vocabulary
    /// (byte offsets and row/column pairs) rather than tree-sitter's types.
    pub(crate) fn notify_edit(
        &mut self,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
        start_point: (usize, usize),
        old_end_point: (usize, usize),
        new_end_point: (usize, usize),
    ) {
        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: tree_sitter::Point {
                row: start_point.0,
                column: start_point.1,
            },
            old_end_position: tree_sitter::Point {
                row: old_end_point.0,
                column: old_end_point.1,
            },
            new_end_position: tree_sitter::Point {
                row: new_end_point.0,
                column: new_end_point.1,
            },
        };

        if let Some(tree) = &mut self.tree {
            tree.edit(&edit);
        }
        if self.highlights.invalidate_from(start_byte) {
            self.highlights_generation += 1;
        }

        let mut injections_dropped = false;
        for layer in self
            .injection_layers
            .iter_mut()
            .chain(self.dynamic_injection_layers.values_mut())
        {
            if let Some(tree) = &mut layer.tree {
                tree.edit(&edit);
            }
            injections_dropped |= layer.highlights.invalidate_from(start_byte);
        }
        if injections_dropped {
            self.injection_generation += 1;
        }

        self.pending_edits.push(edit);
    }

    // Parsing

    /// Time-budgeted incremental parse, aborting if it exceeds `budget`. On
    /// `Aborted`, the tree is untouched; caller falls back to a background job.
    pub(crate) fn try_incremental_parse(
        &mut self,
        buffer: &TextBuffer,
        budget: std::time::Duration,
    ) -> ParseOutcome {
        use std::ops::ControlFlow;
        use std::time::Instant;
        let source = Source::new(buffer);
        crate::perf_span!(
            "syntax_reparse",
            crate::perf::PerfFields {
                bytes: Some(u32::try_from(source.len()).unwrap_or(u32::MAX)),
                ..Default::default()
            }
        );

        let mut parser = Parser::new();
        if parser.set_language(&self.language).is_err() {
            return ParseOutcome::NoLanguage;
        }

        let deadline = Instant::now() + budget;
        let mut parse_progress = |_state: &tree_sitter::ParseState| {
            if Instant::now() >= deadline {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let options = tree_sitter::ParseOptions::new().progress_callback(&mut parse_progress);
        let Some(tree) = source.parse(&mut parser, self.tree.as_ref(), Some(options)) else {
            return ParseOutcome::Aborted;
        };

        self.apply_tree(tree, source);
        ParseOutcome::Completed
    }

    /// Incremental parse of the host grammar, then injection layers.
    /// Returns `true` if host parsing succeeded.
    pub(crate) fn incremental_parse(&mut self, buffer: &TextBuffer) -> bool {
        let mut parser = Parser::new();
        if parser.set_language(&self.language).is_err() {
            return false;
        }

        let source = Source::new(buffer);
        let Some(tree) = source.parse(&mut parser, self.tree.as_ref(), None) else {
            return false;
        };
        self.apply_tree(tree, source);
        true
    }

    /// Install a freshly parsed host tree, dropping the highlight chunks it
    /// may have changed, then reparse injection layers.
    fn apply_tree(&mut self, tree: Tree, source: Source) {
        let dropped = if let Some(old) = &self.tree {
            self.highlights
                .invalidate_from(reparse_start(old, &tree, &self.pending_edits))
        } else {
            self.highlights.clear();
            true
        };
        if dropped {
            self.highlights_generation += 1;
        }

        self.tree = Some(tree);
        self.parse_injections(source);
        self.pending_edits.clear();
    }

    /// Dispatch to static or dynamic injection parsing depending on the query.
    fn parse_injections(&mut self, source: Source) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let Some(query) = self.injections_query.clone() else {
            return;
        };

        // Detect protocol: dynamic uses injection.language + injection.content captures.
        let cap_names = query.capture_names();
        let lang_idx = cap_names
            .iter()
            .position(|n| *n == "injection.language")
            .and_then(|i| u32::try_from(i).ok());
        let content_idx = cap_names
            .iter()
            .position(|n| *n == "injection.content")
            .and_then(|i| u32::try_from(i).ok());

        let dropped = if let (Some(li), Some(ci)) = (lang_idx, content_idx) {
            self.parse_dynamic_injections(source, &tree, &query, li, ci)
        } else {
            self.parse_static_injections(source, &tree, &query)
        };
        if dropped {
            self.injection_generation += 1;
        }
    }

    /// Static injection protocol: capture name IS the target language name.
    /// Used by Svelte (`@typescript`, `@css`) and HTML (`@javascript`, `@css`).
    fn parse_static_injections(&mut self, source: Source, tree: &Tree, query: &Query) -> bool {
        let mut layer_ranges: Vec<Vec<Range<usize>>> =
            vec![Vec::new(); self.injection_layers.len()];
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source.node_text());
        while let Some(m) = matches.next() {
            for cap in m.captures() {
                let Some((_, lang_name)) = self
                    .injection_capture_langs
                    .iter()
                    .find(|(idx, _)| *idx == cap.index)
                else {
                    continue;
                };
                for (li, layer) in self.injection_layers.iter().enumerate() {
                    if layer.language_name == *lang_name {
                        layer_ranges[li].push(cap.node.byte_range());
                    }
                }
            }
        }

        let mut dropped = false;
        for (layer, mut ranges) in self.injection_layers.iter_mut().zip(layer_ranges) {
            sort_included_ranges(&mut ranges);
            dropped |= layer.reparse(source, &ranges, &self.pending_edits);
        }
        dropped
    }

    /// Dynamic injection protocol used by Markdown: `injection.language`
    /// names the language, `injection.content` gives the byte range to parse.
    fn parse_dynamic_injections(
        &mut self,
        source: Source,
        tree: &Tree,
        query: &Query,
        lang_cap_idx: u32,
        content_cap_idx: u32,
    ) -> bool {
        let Some(loader) = self.language_loader.clone() else {
            return false;
        };

        let mut by_lang: BTreeMap<String, Vec<Range<usize>>> = BTreeMap::new();
        for (range, lang) in collect_injections(query, tree, source, lang_cap_idx, content_cap_idx)
        {
            by_lang.entry(lang).or_default().push(range);
        }

        // Drop cached layers for languages no longer present in this document.
        let layer_count = self.dynamic_injection_layers.len();
        self.dynamic_injection_layers
            .retain(|lang_name, _| by_lang.contains_key(lang_name));
        let mut dropped = self.dynamic_injection_layers.len() != layer_count;

        for (lang_name, mut ranges) in by_lang {
            let layer = match self.dynamic_injection_layers.entry(lang_name) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    let Some(layer) = new_dynamic_layer(&loader, entry.key()) else {
                        continue;
                    };
                    entry.insert(layer)
                }
            };
            sort_included_ranges(&mut ranges);
            dropped |= layer.reparse(source, &ranges, &self.pending_edits);
        }
        dropped
    }

    // Highlight queries

    /// Host grammar highlights overlapping `range`, querying uncached chunks.
    pub(crate) fn highlights(
        &mut self,
        buffer: &TextBuffer,
        range: Range<usize>,
    ) -> Vec<Highlight> {
        let (Some(tree), Some(query)) = (&self.tree, &self.highlights_query) else {
            return Vec::new();
        };
        self.highlights
            .highlights(Source::new(buffer), tree, query, range)
    }

    /// Injection layer highlights (static and dynamic) overlapping `range`
    /// as `(byte_range, capture_name)` pairs.
    pub(crate) fn injection_highlights_named(
        &mut self,
        buffer: &TextBuffer,
        range: Range<usize>,
    ) -> Vec<(Range<usize>, String)> {
        let source = Source::new(buffer);
        let mut result: Vec<(Range<usize>, String)> = self
            .injection_layers
            .iter_mut()
            .chain(self.dynamic_injection_layers.values_mut())
            .flat_map(|layer| layer.highlights_named(source, range.clone()))
            .collect();

        result.sort_by_key(|(r, _)| r.start);
        result
    }

    /// Capture names from the host grammar's query.
    #[must_use]
    pub(crate) fn capture_names(&self) -> &[&str] {
        if let Some(query) = &self.highlights_query {
            query.capture_names()
        } else {
            &[]
        }
    }
}

/// `set_included_ranges` requires sorted, non-overlapping ranges.
fn sort_included_ranges(ranges: &mut Vec<Range<usize>>) {
    ranges.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    ranges.dedup();
}

/// Collects `(range, language)` pairs for every dynamic injection match.
fn collect_injections(
    query: &Query,
    tree: &Tree,
    source: Source,
    lang_cap_idx: u32,
    content_cap_idx: u32,
) -> Vec<(Range<usize>, String)> {
    let mut found: Vec<(Range<usize>, String)> = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source.node_text());

    while let Some(m) = matches.next() {
        let mut lang_name: Option<String> = None;
        let mut content_range: Option<Range<usize>> = None;

        for cap in m.captures() {
            if cap.index == lang_cap_idx {
                let text = source.text(cap.node.byte_range());
                lang_name = Some(normalize_lang_name(
                    std::str::from_utf8(&text).unwrap_or("").trim(),
                ));
            } else if cap.index == content_cap_idx {
                content_range = Some(cap.node.byte_range());
            }
        }

        // Some patterns (e.g. markdown HTML blocks/frontmatter) supply the
        // language via `#set! injection.language "x"` instead of a capture.
        if lang_name.is_none() {
            for prop in query.property_settings(m.pattern_index) {
                if &*prop.key == "injection.language" {
                    if let Some(v) = &prop.value {
                        lang_name = Some(normalize_lang_name(v));
                    }
                }
            }
        }

        if let (Some(lang), Some(range)) = (lang_name, content_range) {
            if !lang.is_empty() {
                found.push((range, lang));
            }
        }
    }
    found
}

/// Creates an unparsed dynamic injection layer for `lang_name`.
fn new_dynamic_layer(
    loader: &super::loader::LanguageLoader,
    lang_name: &str,
) -> Option<InjectedLayer> {
    let lang_loaded = loader.load_language(lang_name).ok()?;
    let highlights_query = loader
        .load_query(lang_name, "highlights")
        .ok()
        .and_then(|src| Query::new(&lang_loaded.language, &src).ok())
        .map(Arc::new);
    Some(InjectedLayer::new(
        lang_loaded.language,
        lang_name.to_owned(),
        highlights_query,
    ))
}

// Public factory

/// Build a `Syntax` with injection layers pre-configured from the loader:
/// pre-created for static grammars (Svelte/HTML), lazy for dynamic ones (Markdown).
pub(crate) fn build_syntax(
    loaded: LoadedLanguage,
    highlights_query: Option<Arc<Query>>,
    language_loader: &Arc<super::loader::LanguageLoader>,
) -> Syntax {
    let mut syntax = Syntax::new(loaded, highlights_query);
    syntax.language_loader = Some(language_loader.clone());

    if let Some(inj_src) = language_loader.load_injections_query(&syntax.language_name) {
        if let Ok(inj_query) = Query::new(&syntax.language, &inj_src) {
            let inj_query = Arc::new(inj_query);
            let cap_names: Vec<String> = inj_query
                .capture_names()
                .iter()
                .map(std::string::ToString::to_string)
                .collect();

            let is_dynamic = cap_names.contains(&"injection.language".to_string());

            if is_dynamic {
                // Dynamic protocol (e.g. Markdown): layers are created at parse time.
                syntax.injections_query = Some(inj_query);
            } else {
                // Static protocol (e.g. Svelte, HTML): pre-create one layer per capture.
                let mut capture_langs: Vec<(u32, String)> = Vec::new();
                let mut layers: Vec<InjectedLayer> = Vec::new();

                for (idx, name) in (0_u32..).zip(cap_names.iter()) {
                    if let Ok(lang_loaded) = language_loader.load_language(name) {
                        let lang_name = lang_loaded.name.clone();
                        let layer_query = language_loader
                            .load_query(&lang_name, "highlights")
                            .ok()
                            .and_then(|src| Query::new(&lang_loaded.language, &src).ok())
                            .map(Arc::new);

                        capture_langs.push((idx, lang_name.clone()));
                        layers.push(InjectedLayer::new(
                            lang_loaded.language,
                            lang_name,
                            layer_query,
                        ));
                    }
                }

                syntax.set_injections(inj_query, capture_langs, layers);
            }
        }
    }

    syntax
}

// Helpers

/// Normalize common language tag aliases used in fenced code blocks.
fn normalize_lang_name(name: &str) -> String {
    match name {
        "rs" => "rust".to_string(),
        "py" | "python3" => "python".to_string(),
        "js" | "mjs" | "cjs" => "javascript".to_string(),
        "ts" => "typescript".to_string(),
        "sh" | "bash" | "zsh" => "bash".to_string(),
        "rb" => "ruby".to_string(),
        "yml" => "yaml".to_string(),
        "md" | "markdown" => "markdown".to_string(),
        other => other.to_lowercase(),
    }
}

fn flatten_highlights(mut found: Vec<(Range<usize>, u32, usize)>) -> Vec<Highlight> {
    found.sort_by(|a, b| {
        a.0.start
            .cmp(&b.0.start)
            .then(b.0.end.cmp(&a.0.end))
            .then(a.2.cmp(&b.2))
    });

    let mut out: Vec<Highlight> = Vec::new();
    let mut open: Vec<(usize, u32)> = Vec::new();
    let mut pos = 0;
    for (range, capture, _) in found {
        emit_open(&mut out, &mut open, &mut pos, range.start);
        pos = pos.max(range.start);
        open.push((range.end, capture));
    }
    emit_open(&mut out, &mut open, &mut pos, usize::MAX);
    out
}

fn emit_open(
    out: &mut Vec<Highlight>,
    open: &mut Vec<(usize, u32)>,
    pos: &mut usize,
    target: usize,
) {
    while let Some(&(end, capture)) = open.last() {
        if end <= *pos {
            open.pop();
            continue;
        }
        if *pos >= target {
            break;
        }
        let seg_end = end.min(target);
        match out.last_mut() {
            Some(last) if last.0.end == *pos && last.1 == capture => last.0.end = seg_end,
            _ => out.push((*pos..seg_end, capture)),
        }
        *pos = seg_end;
    }
}
