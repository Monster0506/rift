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

    // --- Static injection support (Svelte, HTML: capture name = language name) ---
    pub injections_query: Option<Arc<Query>>,
    pub injection_capture_langs: Vec<(u32, String)>,
    pub injection_layers: Vec<InjectedLayer>,

    // --- Dynamic injection support (Markdown: injection.language + injection.content) ---
    /// Layers created on demand at parse time; rebuilt on every incremental_parse.
    dynamic_injection_layers: Vec<InjectedLayer>,

    /// Optional loader used to create dynamic injection layers.
    language_loader: Option<Arc<loader::LanguageLoader>>,
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
            injections_query: None,
            injection_capture_langs: Vec::new(),
            injection_layers: Vec::new(),
            dynamic_injection_layers: Vec::new(),
            language_loader: None,
        })
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
    }

    /// Discard all cached trees and highlights after a non-incremental change (e.g. undo/redo).
    /// The next `incremental_parse()` will do a full re-parse from scratch.
    pub fn invalidate_trees(&mut self) {
        self.tree = None;
        for layer in &mut self.injection_layers {
            layer.tree = None;
            layer.cached_highlights = IntervalTree::default();
            layer.byte_ranges.clear();
        }
        self.dynamic_injection_layers.clear();
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
        for layer in &mut self.dynamic_injection_layers {
            if let Some(tree) = &mut layer.tree {
                tree.edit(edit);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Parsing
    // -----------------------------------------------------------------------

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

    /// Public entry point for re-running injection parsing after an external tree update.
    pub fn parse_injections_pub(&mut self, source: &[u8]) {
        self.parse_injections(source);
    }

    /// Dispatch to static or dynamic injection parsing depending on the query.
    fn parse_injections(&mut self, source: &[u8]) {
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
            self.parse_dynamic_injections(source, &tree, &query, li as u32, ci as u32);
        } else {
            self.parse_static_injections(source, &tree, &query);
        }
    }

    /// Static injection protocol: capture name IS the target language name.
    /// Used by Svelte (`@typescript`, `@css`) and HTML (`@javascript`, `@css`).
    fn parse_static_injections(&mut self, source: &[u8], tree: &Tree, query: &Query) {
        let mut lang_ranges: Vec<Vec<std::ops::Range<usize>>> =
            vec![Vec::new(); self.injection_layers.len()];

        let root = tree.root_node();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, root, source);

        while let Some(m) = matches.next() {
            for cap in m.captures {
                for (cap_idx, lang_name) in &self.injection_capture_langs {
                    if cap.index == *cap_idx {
                        for (li, layer) in self.injection_layers.iter().enumerate() {
                            if layer.language_name == *lang_name {
                                lang_ranges[li].push(cap.node.byte_range());
                            }
                        }
                    }
                }
            }
        }

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
                    let sp = byte_to_point(source, r.start);
                    let ep = byte_to_point(source, r.end);
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

            let new_tree = parser.parse(source, layer.tree.as_ref());
            let tree = match new_tree {
                Some(t) => t,
                None => continue,
            };

            let mut highlights = Vec::new();
            if let Some(ref q) = layer.highlights_query {
                let root = tree.root_node();
                let mut qc = QueryCursor::new();
                let mut ms = qc.matches(q, root, source);
                while let Some(m) = ms.next() {
                    for cap in m.captures {
                        highlights.push((cap.node.byte_range(), cap.index));
                    }
                }
            }

            layer.cached_highlights = IntervalTree::new(highlights);
            layer.tree = Some(tree);
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
    ) {
        let loader = match self.language_loader.clone() {
            Some(l) => l,
            None => return,
        };

        // Collect (normalized_language_name, content_range) pairs from each match.
        let mut pairs: Vec<(String, std::ops::Range<usize>)> = Vec::new();
        {
            let root = tree.root_node();
            let mut cursor = QueryCursor::new();
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

                if let (Some(lang), Some(range)) = (lang_name, content_range) {
                    if !lang.is_empty() {
                        pairs.push((lang, range));
                    }
                }
            }
        }

        // Group ranges by language name.
        let mut by_lang: HashMap<String, Vec<std::ops::Range<usize>>> = HashMap::new();
        for (lang, range) in pairs {
            by_lang.entry(lang).or_default().push(range);
        }

        // Rebuild dynamic layers (cleared on every parse).
        self.dynamic_injection_layers.clear();

        for (lang_name, ranges) in by_lang {
            let lang_loaded = match loader.load_language(&lang_name) {
                Ok(l) => l,
                Err(_) => continue,
            };

            let highlights_query = loader
                .load_query(&lang_name, "highlights")
                .ok()
                .and_then(|src| tree_sitter::Query::new(&lang_loaded.language, &src).ok())
                .map(Arc::new);

            let ts_ranges: Vec<tree_sitter::Range> = ranges
                .iter()
                .map(|r| {
                    let sp = byte_to_point(source, r.start);
                    let ep = byte_to_point(source, r.end);
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

            let new_tree = match parser.parse(source, None) {
                Some(t) => t,
                None => continue,
            };

            let mut highlights = Vec::new();
            if let Some(ref q) = highlights_query {
                let root = new_tree.root_node();
                let mut qc = QueryCursor::new();
                let mut ms = qc.matches(q, root, source);
                while let Some(m) = ms.next() {
                    for cap in m.captures {
                        highlights.push((cap.node.byte_range(), cap.index));
                    }
                }
            }

            self.dynamic_injection_layers.push(InjectedLayer {
                language: lang_loaded.language,
                language_name: lang_name,
                highlights_query,
                tree: Some(new_tree),
                cached_highlights: IntervalTree::new(highlights),
                byte_ranges: ranges,
            });
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
        for layer in &self.dynamic_injection_layers {
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

/// Convert a byte offset to a tree-sitter `Point` (row, column).
fn byte_to_point(source: &[u8], byte: usize) -> tree_sitter::Point {
    let byte = byte.min(source.len());
    let row = source[..byte].iter().filter(|&&b| b == b'\n').count();
    let last_nl = source[..byte]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    tree_sitter::Point {
        row,
        column: byte - last_nl,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
