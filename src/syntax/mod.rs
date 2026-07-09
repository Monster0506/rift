use crate::error::RiftError;
use crate::job_manager::jobs::syntax::SyntaxParseResult;
use crate::syntax::loader::LoadedLanguage;
use std::collections::HashMap;
use std::sync::Arc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Parser, Query, QueryCursor, Tree};

pub mod interval_tree;
pub mod loader;
use crate::syntax::interval_tree::IntervalTree;

#[derive(Clone, Debug)]
pub enum SyntaxNotification {
    Loaded { language_name: String },
    HighlightsUpdated,
    Error(String),
}

/// Result of a budgeted synchronous parse attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseOutcome {
    /// Parsed and re-highlighted within the budget; state is up to date.
    Completed,
    /// Exceeded the time budget; state is unchanged, caller should background it.
    Aborted,
    /// No language configured for this document.
    NoLanguage,
}

// ---------------------------------------------------------------------------
// Injection layer
// ---------------------------------------------------------------------------

/// A single embedded-language layer (e.g. TypeScript inside a Svelte <script>).
pub struct InjectedLayer {
    pub language: tree_sitter::Language,
    pub language_name: String,
    pub highlights_query: Option<Arc<Query>>,
    pub tree: Option<Tree>,
    pub cached_highlights: IntervalTree<u32>,
    /// Byte ranges in the host document covered by this layer.
    pub byte_ranges: Vec<std::ops::Range<usize>>,
    /// Keeps a dynamically loaded `language`'s backing library alive (see `RawLib`).
    pub lib: Option<Arc<loader::RawLib>>,
}

// ---------------------------------------------------------------------------
// Syntax
// ---------------------------------------------------------------------------

/// Per-document syntax state.
///
/// The *host* language is the language declared in the outer grammar (e.g. Svelte).
/// *Injection layers* are re-parsed sub-regions using a different grammar
/// (e.g. TypeScript inside `<script>`, CSS inside `<style>`).
pub struct Syntax {
    pub language: tree_sitter::Language,
    pub tree: Option<Tree>,
    pub highlights_query: Option<Arc<Query>>,
    pub language_name: String,
    cached_highlights: IntervalTree<u32>,
    /// Keeps `language`'s backing dynamic library alive for as long as this
    /// `Syntax` (and any `Tree`/`Parser` derived from it) may exist.
    lib: Option<Arc<loader::RawLib>>,

    // --- Static injection support (Svelte, HTML: capture name = language name) ---
    pub injections_query: Option<Arc<Query>>,
    pub injection_capture_langs: Vec<(u32, String)>,
    pub injection_layers: Vec<InjectedLayer>,
    /// Content ranges discovered by the last static-injection scan, keyed by
    /// index into `injection_layers`, so a single-edit reparse can reuse them.
    static_injection_ranges: IntervalTree<usize>,

    // --- Dynamic injection support (Markdown: injection.language + injection.content) ---
    /// Layers created on demand at parse time, cached by language name across
    /// parses so queries and trees are reused instead of rebuilt from scratch.
    dynamic_injection_layers: HashMap<String, InjectedLayer>,
    /// Content ranges discovered by the last dynamic-injection scan, keyed by
    /// language, so a single-edit reparse can shift and reuse them.
    dynamic_injection_ranges: IntervalTree<String>,

    /// Optional loader used to create dynamic injection layers.
    language_loader: Option<Arc<loader::LanguageLoader>>,

    /// Edits since `cached_highlights` was last fully replaced, so a
    /// background job can shift the still-valid part instead of requerying.
    pending_edits: Vec<InputEdit>,
}

impl Syntax {
    /// Create a new Syntax instance (no injections).
    pub fn new(
        loaded: LoadedLanguage,
        highlights_query: Option<Arc<Query>>,
    ) -> Result<Self, RiftError> {
        Ok(Self {
            language: loaded.language,
            tree: None,
            highlights_query,
            language_name: loaded.name,
            cached_highlights: IntervalTree::default(),
            lib: loaded.lib,
            injections_query: None,
            injection_capture_langs: Vec::new(),
            injection_layers: Vec::new(),
            static_injection_ranges: IntervalTree::default(),
            dynamic_injection_layers: HashMap::new(),
            dynamic_injection_ranges: IntervalTree::default(),
            language_loader: None,
            pending_edits: Vec::new(),
        })
    }

    /// Snapshot of the current highlights and the edits applied since they
    /// were last fully recomputed, for a background job to reuse.
    pub(crate) fn highlights_snapshot(&self) -> (IntervalTree<u32>, Vec<InputEdit>) {
        (self.cached_highlights.clone(), self.pending_edits.clone())
    }

    /// The single edit since highlights/injections were last recomputed, if
    /// exactly one landed; otherwise the caller should recompute everything.
    pub(crate) fn single_pending_edit(&self) -> Option<InputEdit> {
        match self.pending_edits.as_slice() {
            [edit] => Some(*edit),
            _ => None,
        }
    }

    /// The `Arc<RawLib>` keeping this syntax's `language` alive, if it came
    /// from a dynamically loaded grammar. Clone it into anything that outlives `self`.
    pub fn lib(&self) -> Option<Arc<loader::RawLib>> {
        self.lib.clone()
    }

    /// Attach an injections query and pre-built injection layers (static protocol).
    pub fn set_injections(
        &mut self,
        injections_query: Arc<Query>,
        capture_langs: Vec<(u32, String)>,
        layers: Vec<InjectedLayer>,
    ) {
        self.injections_query = Some(injections_query);
        self.injection_capture_langs = capture_langs;
        self.injection_layers = layers;
    }

    // -----------------------------------------------------------------------
    // Update from background job result
    // -----------------------------------------------------------------------

    pub fn update_from_result(&mut self, result: SyntaxParseResult) {
        if result.language_name != self.language_name {
            return;
        }
        self.tree = result.tree;
        self.cached_highlights = result.highlights;
        self.pending_edits.clear();
    }

    /// Discard all cached trees and highlights after a non-incremental change (e.g. undo/redo).
    /// The next `incremental_parse()` will do a full re-parse from scratch.
    pub fn invalidate_trees(&mut self) {
        self.tree = None;
        self.cached_highlights = IntervalTree::default();
        self.pending_edits.clear();
        for layer in &mut self.injection_layers {
            layer.tree = None;
            layer.cached_highlights = IntervalTree::default();
            layer.byte_ranges.clear();
        }
        self.static_injection_ranges = IntervalTree::default();
        for layer in self.dynamic_injection_layers.values_mut() {
            layer.tree = None;
            layer.cached_highlights = IntervalTree::default();
            layer.byte_ranges.clear();
        }
        self.dynamic_injection_ranges = IntervalTree::default();
    }

    pub fn update_tree(&mut self, edit: &InputEdit) {
        if let Some(tree) = &mut self.tree {
            tree.edit(edit);
        }
        for layer in &mut self.injection_layers {
            if let Some(tree) = &mut layer.tree {
                tree.edit(edit);
            }
        }
        for layer in self.dynamic_injection_layers.values_mut() {
            if let Some(tree) = &mut layer.tree {
                tree.edit(edit);
            }
        }
        self.pending_edits.push(*edit);
    }

    // -----------------------------------------------------------------------
    // Parsing
    // -----------------------------------------------------------------------

    /// Time-budgeted incremental parse: tries to parse and re-highlight the
    /// host grammar synchronously, aborting if it exceeds `budget`.
    ///
    /// On `Aborted`, `self.tree`/`self.cached_highlights` are left untouched
    /// (the caller should fall back to a background `SyntaxParseJob`).
    pub fn try_incremental_parse(
        &mut self,
        source: &[u8],
        budget: std::time::Duration,
    ) -> ParseOutcome {
        crate::perf_span!(
            "syntax_reparse",
            crate::perf::PerfFields {
                bytes: Some(source.len() as u32),
                ..Default::default()
            }
        );
        use std::ops::ControlFlow;
        use std::time::Instant;

        let mut parser = Parser::new();
        if parser.set_language(&self.language).is_err() {
            return ParseOutcome::NoLanguage;
        }

        let old_tree = self.tree.clone();
        let single_edit = self.single_pending_edit();

        let deadline = Instant::now() + budget;
        let mut parse_timed_out = false;
        let mut parse_progress = |_state: &tree_sitter::ParseState| {
            if Instant::now() >= deadline {
                parse_timed_out = true;
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let new_tree = parser.parse_with_options(
            &mut |i, _| source.get(i..).unwrap_or(&[]),
            old_tree.as_ref(),
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut parse_progress)),
        );

        let Some(tree) = new_tree else {
            return ParseOutcome::Aborted;
        };
        if parse_timed_out {
            return ParseOutcome::Aborted;
        }

        if let Some(query) = &self.highlights_query {
            // Scoping to the changed region keeps this inside budget instead
            // of always falling back to the (slower to appear) async job.
            let scoped = match (old_tree.as_ref(), single_edit) {
                (Some(prev), Some(edit)) => Some((prev, edit)),
                _ => None,
            };

            if let Some((prev_tree, edit)) = scoped {
                self.cached_highlights =
                    scoped_query_highlights(query, &tree, source, &self.cached_highlights, Some((prev_tree, edit)));
            } else {
                let root_node = tree.root_node();
                let mut cursor = QueryCursor::new();
                let mut highlights = Vec::new();
                let mut query_timed_out = false;
                let mut query_progress = |_state: &tree_sitter::QueryCursorState| {
                    if Instant::now() >= deadline {
                        query_timed_out = true;
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                };
                let mut matches = cursor.matches_with_options(
                    query,
                    root_node,
                    source,
                    tree_sitter::QueryCursorOptions::new().progress_callback(&mut query_progress),
                );
                while let Some(m) = matches.next() {
                    for capture in m.captures {
                        highlights.push((capture.node.byte_range(), capture.index));
                    }
                }
                if query_timed_out {
                    return ParseOutcome::Aborted;
                }
                self.cached_highlights = IntervalTree::new(highlights);
            }
        }

        self.tree = Some(tree);
        self.pending_edits.clear();
        self.parse_injections(source);
        ParseOutcome::Completed
    }

    /// Incremental parse of the host grammar, then injection layers.
    /// Returns `true` if host parsing succeeded.
    pub fn incremental_parse(&mut self, source: &[u8]) -> bool {
        let mut parser = Parser::new();
        if parser.set_language(&self.language).is_err() {
            return false;
        }

        let new_tree = parser.parse(source, self.tree.as_ref());

        if let Some(tree) = new_tree {
            if let Some(query) = &self.highlights_query {
                let root_node = tree.root_node();
                let mut cursor = QueryCursor::new();
                let mut highlights = Vec::new();
                let mut matches = cursor.matches(query, root_node, source);
                while let Some(m) = matches.next() {
                    for capture in m.captures {
                        let range = capture.node.byte_range();
                        highlights.push((range, capture.index));
                    }
                }
                self.cached_highlights = IntervalTree::new(highlights);
            }
            self.tree = Some(tree);

            // Parse injection regions with their respective languages.
            self.parse_injections(source);

            true
        } else {
            false
        }
    }

    /// Re-run injection parsing after an external tree update. `old_host_tree`/`edit`
    /// scope the requery when this reflects exactly one edit since injections were derived.
    pub fn parse_injections_pub(
        &mut self,
        source: &[u8],
        old_host_tree: Option<&Tree>,
        edit: Option<InputEdit>,
    ) {
        self.parse_injections_dispatch(source, old_host_tree.zip(edit));
    }

    /// Dispatch to static or dynamic injection parsing depending on the query.
    fn parse_injections(&mut self, source: &[u8]) {
        self.parse_injections_dispatch(source, None);
    }

    fn parse_injections_dispatch(&mut self, source: &[u8], scoped: Option<(&Tree, InputEdit)>) {
        let tree = match self.tree.clone() {
            Some(t) => t,
            None => return,
        };
        let query = match self.injections_query.clone() {
            Some(q) => q,
            None => return,
        };

        // Detect protocol: dynamic uses injection.language + injection.content captures.
        let cap_names: Vec<String> = query
            .capture_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let lang_idx = cap_names.iter().position(|n| n == "injection.language");
        let content_idx = cap_names.iter().position(|n| n == "injection.content");

        if let (Some(li), Some(ci)) = (lang_idx, content_idx) {
            self.parse_dynamic_injections(source, &tree, &query, li as u32, ci as u32, scoped);
        } else {
            self.parse_static_injections(source, &tree, &query, scoped);
        }
    }

    /// Static injection protocol: capture name IS the target language name.
    /// Used by Svelte (`@typescript`, `@css`) and HTML (`@javascript`, `@css`).
    fn parse_static_injections(
        &mut self,
        source: &[u8],
        tree: &Tree,
        query: &Query,
        scoped: Option<(&Tree, InputEdit)>,
    ) {
        // (content_range, layer_index) pairs: query just the changed region on
        // a single edit, keeping cached ranges the fresh requery doesn't touch.
        let query_ranges: Vec<std::ops::Range<usize>> = match scoped {
            Some((prev_tree, edit)) => scoped_query_ranges(prev_tree, tree, edit),
            None => vec![0..source.len()],
        };

        let mut fresh: Vec<(std::ops::Range<usize>, usize)> = Vec::new();
        {
            let root = tree.root_node();
            for range in query_ranges {
                let mut cursor = QueryCursor::new();
                cursor.set_byte_range(range);
                let mut matches = cursor.matches(query, root, source);

                while let Some(m) = matches.next() {
                    for cap in m.captures {
                        for (cap_idx, lang_name) in &self.injection_capture_langs {
                            if cap.index == *cap_idx {
                                for (li, layer) in self.injection_layers.iter().enumerate() {
                                    if layer.language_name == *lang_name {
                                        fresh.push((cap.node.byte_range(), li));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut pairs: Vec<(std::ops::Range<usize>, usize)> = match scoped {
            Some((_, edit)) => scoped_kept_items(&self.static_injection_ranges, edit, &fresh),
            None => Vec::new(),
        };
        pairs.extend(fresh);

        // set_included_ranges requires sorted, non-overlapping ranges; a scoped
        // pass can also re-emit a range from an overlapping broad pattern match.
        pairs.sort_by(|a, b| a.0.start.cmp(&b.0.start).then(a.0.end.cmp(&b.0.end)));
        if scoped.is_some() {
            pairs.dedup();
        }
        self.static_injection_ranges = IntervalTree::new(pairs.clone());

        let mut lang_ranges: Vec<Vec<std::ops::Range<usize>>> =
            vec![Vec::new(); self.injection_layers.len()];
        for (range, li) in pairs {
            lang_ranges[li].push(range);
        }

        let newline_index = NewlineIndex::build(source);
        for (li, layer) in self.injection_layers.iter_mut().enumerate() {
            let ranges = &lang_ranges[li];
            if ranges.is_empty() {
                layer.cached_highlights = IntervalTree::default();
                layer.byte_ranges.clear();
                continue;
            }

            layer.byte_ranges = ranges.clone();

            let ts_ranges: Vec<tree_sitter::Range> = ranges
                .iter()
                .map(|r| {
                    let sp = newline_index.point_at(r.start);
                    let ep = newline_index.point_at(r.end);
                    tree_sitter::Range {
                        start_byte: r.start,
                        end_byte: r.end,
                        start_point: sp,
                        end_point: ep,
                    }
                })
                .collect();

            let mut parser = Parser::new();
            if parser.set_language(&layer.language).is_err() {
                continue;
            }
            if parser.set_included_ranges(&ts_ranges).is_err() {
                continue;
            }

            let old_layer_tree = layer.tree.clone();
            let new_tree = match parser.parse(source, old_layer_tree.as_ref()) {
                Some(t) => t,
                None => continue,
            };

            let highlights = if let Some(ref q) = layer.highlights_query {
                let layer_scoped = match (&old_layer_tree, scoped) {
                    (Some(prev), Some((_, edit))) => Some((prev, edit)),
                    _ => None,
                };
                scoped_query_highlights(q, &new_tree, source, &layer.cached_highlights, layer_scoped)
            } else {
                IntervalTree::default()
            };

            layer.cached_highlights = highlights;
            layer.tree = Some(new_tree);
        }
    }

    /// Dynamic injection protocol: `injection.language` capture text names the language;
    /// `injection.content` capture gives the byte range to parse.
    /// Used by Markdown (fenced code blocks with arbitrary language tags).
    fn parse_dynamic_injections(
        &mut self,
        source: &[u8],
        tree: &Tree,
        query: &Query,
        lang_cap_idx: u32,
        content_cap_idx: u32,
        scoped: Option<(&Tree, InputEdit)>,
    ) {
        let loader = match self.language_loader.clone() {
            Some(l) => l,
            None => return,
        };

        // (content_range, language_name) pairs: query just the changed region
        // on a single edit, keeping cached ranges the fresh requery doesn't touch.
        let query_ranges: Vec<std::ops::Range<usize>> = match scoped {
            Some((prev_tree, edit)) => scoped_query_ranges(prev_tree, tree, edit),
            None => vec![0..source.len()],
        };

        let mut fresh: Vec<(std::ops::Range<usize>, String)> = Vec::new();
        {
            let root = tree.root_node();
            for range in query_ranges {
                let mut cursor = QueryCursor::new();
                cursor.set_byte_range(range);
                let mut matches = cursor.matches(query, root, source);

                while let Some(m) = matches.next() {
                    let mut lang_name: Option<String> = None;
                    let mut content_range: Option<std::ops::Range<usize>> = None;

                    for cap in m.captures {
                        if cap.index == lang_cap_idx {
                            let text = std::str::from_utf8(&source[cap.node.byte_range()])
                                .unwrap_or("")
                                .trim();
                            lang_name = Some(normalize_lang_name(text));
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
                            fresh.push((range, lang));
                        }
                    }
                }
            }
        }

        let mut pairs: Vec<(std::ops::Range<usize>, String)> = match scoped {
            Some((_, edit)) => scoped_kept_items(&self.dynamic_injection_ranges, edit, &fresh),
            None => Vec::new(),
        };
        pairs.extend(fresh);

        // set_included_ranges requires sorted, non-overlapping ranges; a scoped
        // pass can also re-emit a range from an overlapping broad pattern match.
        pairs.sort_by(|a, b| a.0.start.cmp(&b.0.start).then(a.0.end.cmp(&b.0.end)));
        if scoped.is_some() {
            pairs.dedup();
        }
        self.dynamic_injection_ranges = IntervalTree::new(pairs.clone());

        // Group ranges by language name.
        let mut by_lang: HashMap<String, Vec<std::ops::Range<usize>>> = HashMap::new();
        for (range, lang) in pairs {
            by_lang.entry(lang).or_default().push(range);
        }

        // Drop cached layers for languages no longer present in this document.
        self.dynamic_injection_layers
            .retain(|lang_name, _| by_lang.contains_key(lang_name));

        let newline_index = NewlineIndex::build(source);
        for (lang_name, ranges) in by_lang {
            let lang_loaded = match loader.load_language(&lang_name) {
                Ok(l) => l,
                Err(_) => continue,
            };

            let ts_ranges: Vec<tree_sitter::Range> = ranges
                .iter()
                .map(|r| {
                    let sp = newline_index.point_at(r.start);
                    let ep = newline_index.point_at(r.end);
                    tree_sitter::Range {
                        start_byte: r.start,
                        end_byte: r.end,
                        start_point: sp,
                        end_point: ep,
                    }
                })
                .collect();

            let mut parser = Parser::new();
            if parser.set_language(&lang_loaded.language).is_err() {
                continue;
            }
            if parser.set_included_ranges(&ts_ranges).is_err() {
                continue;
            }

            // Reuse the cached query/tree/highlights for this language, if present,
            // for an incremental reparse plus a scoped highlights requery.
            let prev_layer = self.dynamic_injection_layers.get(&lang_name);
            let highlights_query =
                prev_layer
                    .and_then(|l| l.highlights_query.clone())
                    .or_else(|| {
                        loader
                            .load_query(&lang_name, "highlights")
                            .ok()
                            .and_then(|src| {
                                tree_sitter::Query::new(&lang_loaded.language, &src).ok()
                            })
                            .map(Arc::new)
                    });
            let old_layer_tree = prev_layer.and_then(|l| l.tree.clone());
            let old_layer_highlights = prev_layer
                .map(|l| l.cached_highlights.clone())
                .unwrap_or_default();

            let new_tree = match parser.parse(source, old_layer_tree.as_ref()) {
                Some(t) => t,
                None => continue,
            };

            let highlights = if let Some(ref q) = highlights_query {
                let layer_scoped = match (&old_layer_tree, scoped) {
                    (Some(prev), Some((_, edit))) => Some((prev, edit)),
                    _ => None,
                };
                scoped_query_highlights(q, &new_tree, source, &old_layer_highlights, layer_scoped)
            } else {
                IntervalTree::default()
            };

            self.dynamic_injection_layers.insert(
                lang_name.clone(),
                InjectedLayer {
                    language: lang_loaded.language,
                    language_name: lang_name,
                    highlights_query,
                    tree: Some(new_tree),
                    cached_highlights: highlights,
                    byte_ranges: ranges,
                    lib: lang_loaded.lib,
                },
            );
        }
    }

    // -----------------------------------------------------------------------
    // Highlight queries
    // -----------------------------------------------------------------------

    /// Query for a specific byte range (used for viewport-bounded parsing).
    pub fn parse_range(
        &self,
        source: &[u8],
        range: std::ops::Range<usize>,
    ) -> Vec<(std::ops::Range<usize>, u32)> {
        let query = match &self.highlights_query {
            Some(q) => q,
            None => return Vec::new(),
        };

        let mut parser = Parser::new();
        if parser.set_language(&self.language).is_err() {
            return Vec::new();
        }

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let root_node = tree.root_node();
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(range);

        let mut highlights = Vec::new();
        let mut matches = cursor.matches(query, root_node, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let r = capture.node.byte_range();
                highlights.push((r, capture.index));
            }
        }
        highlights
    }

    /// Host grammar highlights for a byte range (or the full document).
    pub fn highlights(
        &self,
        range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, u32)> {
        if let Some(r) = range {
            self.cached_highlights.query(r)
        } else {
            self.cached_highlights
                .iter()
                .map(|(r, v)| (r.clone(), *v))
                .collect()
        }
    }

    /// All injection layer highlights (both static and dynamic) as `(byte_range, capture_name)` pairs.
    ///
    /// Includes static layers (Svelte/HTML) and dynamic layers (Markdown code blocks).
    pub fn injection_highlights_named(
        &self,
        range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, String)> {
        let mut result: Vec<(std::ops::Range<usize>, String)> = Vec::new();

        let collect_from_layer = |layer: &InjectedLayer,
                                  range: &Option<std::ops::Range<usize>>|
         -> Vec<(std::ops::Range<usize>, String)> {
            let items: Vec<(std::ops::Range<usize>, u32)> = if let Some(ref r) = range {
                layer.cached_highlights.query(r.clone())
            } else {
                layer
                    .cached_highlights
                    .iter()
                    .map(|(r, v)| (r.clone(), *v))
                    .collect()
            };

            if let Some(ref query) = layer.highlights_query {
                let names = query.capture_names();
                items
                    .into_iter()
                    .filter_map(|(hl_range, capture_idx)| {
                        names
                            .get(capture_idx as usize)
                            .map(|name| (hl_range, name.to_string()))
                    })
                    .collect()
            } else {
                Vec::new()
            }
        };

        for layer in &self.injection_layers {
            result.extend(collect_from_layer(layer, &range));
        }
        for layer in self.dynamic_injection_layers.values() {
            result.extend(collect_from_layer(layer, &range));
        }

        result.sort_by_key(|(r, _)| r.start);
        result
    }

    /// Capture names from the host grammar's query.
    pub fn capture_names(&self) -> &[&str] {
        if let Some(query) = &self.highlights_query {
            query.capture_names()
        } else {
            &[]
        }
    }
}

// ---------------------------------------------------------------------------
// Public factory
// ---------------------------------------------------------------------------

/// Build a `Syntax` instance with injection layers pre-configured from the loader.
///
/// For static injection grammars (Svelte, HTML), pre-creates `InjectedLayer` objects.
/// For dynamic injection grammars (Markdown), stores the loader for lazy layer creation.
pub fn build_syntax(
    loaded: loader::LoadedLanguage,
    highlights_query: Option<Arc<Query>>,
    language_loader: Arc<loader::LanguageLoader>,
) -> Result<Syntax, RiftError> {
    let mut syntax = Syntax::new(loaded, highlights_query)?;
    syntax.language_loader = Some(language_loader.clone());

    if let Some(inj_src) = language_loader.load_injections_query(&syntax.language_name) {
        if let Ok(inj_query) = Query::new(&syntax.language, &inj_src) {
            let inj_query = Arc::new(inj_query);
            let cap_names: Vec<String> = inj_query
                .capture_names()
                .iter()
                .map(|s| s.to_string())
                .collect();

            let is_dynamic = cap_names.contains(&"injection.language".to_string());

            if is_dynamic {
                // Dynamic protocol (e.g. Markdown): layers are created at parse time.
                syntax.injections_query = Some(inj_query);
            } else {
                // Static protocol (e.g. Svelte, HTML): pre-create one layer per capture.
                let mut capture_langs: Vec<(u32, String)> = Vec::new();
                let mut layers: Vec<InjectedLayer> = Vec::new();

                for (idx, name) in cap_names.iter().enumerate() {
                    if let Ok(lang_loaded) = language_loader.load_language(name) {
                        let lang_name = lang_loaded.name.clone();
                        let layer_query = language_loader
                            .load_query(&lang_name, "highlights")
                            .ok()
                            .and_then(|src| Query::new(&lang_loaded.language, &src).ok())
                            .map(Arc::new);

                        capture_langs.push((idx as u32, lang_name.clone()));
                        layers.push(InjectedLayer {
                            language: lang_loaded.language,
                            language_name: lang_name,
                            highlights_query: layer_query,
                            tree: None,
                            cached_highlights: IntervalTree::default(),
                            byte_ranges: Vec::new(),
                            lib: lang_loaded.lib,
                        });
                    }
                }

                syntax.set_injections(inj_query, capture_langs, layers);
            }
        }
    }

    Ok(syntax)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Ranges to (re)query for a single-edit reparse: changed_ranges plus the
/// edit's own span (changed_ranges alone can miss a token absorbing an edit).
pub(crate) fn scoped_query_ranges(
    prev_tree: &Tree,
    new_tree: &Tree,
    edit: InputEdit,
) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = prev_tree
        .changed_ranges(new_tree)
        .map(|r| r.start_byte..r.end_byte)
        .collect();
    ranges.push(edit.start_byte..edit.new_end_byte.max(edit.start_byte + 1));
    ranges.sort_by_key(|r| r.start);

    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

/// Old items surviving a single edit, shifted by its byte delta, with
/// anything overlapping `fresh` dropped (`fresh` is authoritative for whatever it covers).
pub(crate) fn scoped_kept_items<T: Clone>(
    old_items: &IntervalTree<T>,
    edit: InputEdit,
    fresh: &[(std::ops::Range<usize>, T)],
) -> Vec<(std::ops::Range<usize>, T)> {
    old_items
        .shift_for_edit(edit.start_byte, edit.old_end_byte, edit.new_end_byte)
        .into_iter()
        .filter(|(r, _)| !fresh.iter().any(|(fr, _)| r.start < fr.end && fr.start < r.end))
        .collect()
}

/// Recompute a highlights query, scoped to the changed region on a
/// single-edit incremental reparse, or a full scan of `new_tree` otherwise.
pub(crate) fn scoped_query_highlights(
    query: &Query,
    new_tree: &Tree,
    source: &[u8],
    old_highlights: &IntervalTree<u32>,
    scoped: Option<(&Tree, InputEdit)>,
) -> IntervalTree<u32> {
    let root_node = new_tree.root_node();

    let query_ranges = match scoped {
        Some((prev_tree, edit)) => scoped_query_ranges(prev_tree, new_tree, edit),
        None => vec![0..source.len()],
    };

    let mut fresh = Vec::new();
    for range in query_ranges {
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(range);
        let mut matches = cursor.matches(query, root_node, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                fresh.push((capture.node.byte_range(), capture.index));
            }
        }
    }

    let mut highlights = match scoped {
        Some((_, edit)) => scoped_kept_items(old_highlights, edit, &fresh),
        None => Vec::new(),
    };
    highlights.extend(fresh);

    if scoped.is_some() {
        highlights.sort_by(|a, b| {
            a.0.start
                .cmp(&b.0.start)
                .then(a.0.end.cmp(&b.0.end))
                .then(a.1.cmp(&b.1))
        });
        highlights.dedup();
    }

    IntervalTree::new(highlights)
}

/// Byte offsets of every newline in a source buffer, built once per parse so
/// repeated byte-to-point lookups can binary-search instead of rescanning.
struct NewlineIndex {
    newlines: Vec<usize>,
    source_len: usize,
}

impl NewlineIndex {
    fn build(source: &[u8]) -> Self {
        let newlines = source
            .iter()
            .enumerate()
            .filter(|&(_, &b)| b == b'\n')
            .map(|(i, _)| i)
            .collect();
        Self {
            newlines,
            source_len: source.len(),
        }
    }

    /// Convert a byte offset to a tree-sitter `Point` via binary search.
    fn point_at(&self, byte: usize) -> tree_sitter::Point {
        let byte = byte.min(self.source_len);
        let row = self.newlines.partition_point(|&nl| nl < byte);
        let last_nl = if row == 0 {
            0
        } else {
            self.newlines[row - 1] + 1
        };
        tree_sitter::Point {
            row,
            column: byte - last_nl,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
