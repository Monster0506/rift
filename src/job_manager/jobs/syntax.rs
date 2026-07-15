use crate::buffer::TextBuffer;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use crate::syntax::interval_tree::IntervalTree;
use crate::syntax::loader::RawLib;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Parser, Query, QueryCursor, Tree};

#[derive(Debug)]
pub struct SyntaxParseResult {
    pub tree: Option<Tree>,
    pub highlights: IntervalTree<u32>,
    pub language_name: String,
    pub document_id: u64,
    /// Buffer revision at spawn time; the caller must discard this result if
    /// the live revision has since moved on, or a stale parse can clobber newer state.
    pub revision: u64,
    /// Logical bytes this job parsed from, cached by the caller so a future
    /// job can patch it instead of rebuilding from scratch.
    pub logical_bytes: Vec<u8>,
}

crate::impl_job_payload!(SyntaxParseResult);

// Manual Debug because Parser and TextBuffer might not impl Debug
pub struct SyntaxParseJob {
    buffer: TextBuffer,
    parser: Parser,
    old_tree: Option<Tree>,
    highlights_query: Option<std::sync::Arc<Query>>,
    language_name: String,
    document_id: u64,
    revision: u64,
    /// Prior logical-bytes buffer and the single edit since it was captured,
    /// if any — lets `run()` patch instead of a full `to_logical_bytes()` rebuild.
    cached_logical_bytes: Option<Arc<Vec<u8>>>,
    single_edit: Option<InputEdit>,
    /// Keeps `parser`'s language's backing dynamic library alive for the job's
    /// lifetime; runs on a background thread detached from the `Syntax` that spawned it.
    _lib: Option<Arc<RawLib>>,
    /// Highlights as of `old_tree`, plus the single edit since then (if
    /// exactly one landed); otherwise the highlights query rescans everything.
    old_highlights: IntervalTree<u32>,
}

impl std::fmt::Debug for SyntaxParseJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxParseJob")
            .field("language_name", &self.language_name)
            .field("buffer_len", &self.buffer.len())
            .field("has_old_tree", &self.old_tree.is_some())
            .field("has_query", &self.highlights_query.is_some())
            .field(
                "has_cached_logical_bytes",
                &self.cached_logical_bytes.is_some(),
            )
            .finish()
    }
}

impl SyntaxParseJob {
    pub fn new(
        buffer: TextBuffer,
        parser: Parser,
        old_tree: Option<Tree>,
        highlights_query: Option<std::sync::Arc<Query>>,
        language_name: String,
        document_id: u64,
        revision: u64,
    ) -> Self {
        Self {
            buffer,
            parser,
            old_tree,
            highlights_query,
            language_name,
            document_id,
            revision,
            cached_logical_bytes: None,
            single_edit: None,
            _lib: None,
            old_highlights: IntervalTree::default(),
        }
    }

    /// Attach the backing library handle for `parser`'s language, if it was
    /// loaded dynamically, so it stays mapped for the lifetime of this job.
    pub fn with_lib(mut self, lib: Option<Arc<RawLib>>) -> Self {
        self._lib = lib;
        self
    }

    /// Attach the previous highlights and edits since the last completed parse,
    /// so highlights can be scoped to the changed region (only 1 edit is optimized).
    pub fn with_highlights_context(
        mut self,
        old_highlights: IntervalTree<u32>,
        edits: &[InputEdit],
    ) -> Self {
        self.old_highlights = old_highlights;
        self.single_edit = match edits {
            [edit] => Some(*edit),
            _ => None,
        };
        self
    }
    /// Attach a prior logical-bytes buffer and the edit since it was captured,
    /// so `run()` can patch instead of rebuilding; `None`/`None` forces a full rebuild.
    pub fn with_incremental_bytes(
        mut self,
        cached_logical_bytes: Option<Arc<Vec<u8>>>,
        single_edit: Option<tree_sitter::InputEdit>,
    ) -> Self {
        self.cached_logical_bytes = cached_logical_bytes;
        self.single_edit = single_edit;
        self
    }
}

impl Job for SyntaxParseJob {
    fn name(&self) -> &'static str {
        "syntax-parse"
    }

    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, signal: CancellationSignal) {
        if signal.is_cancelled() {
            return;
        }
        crate::perf_span!(
            "syntax_reparse_job",
            crate::perf::PerfFields {
                bytes: Some(self.buffer.byte_len() as u32),
                ..Default::default()
            }
        );

        // Destructure to avoid partial moves
        let SyntaxParseJob {
            buffer,
            mut parser,
            old_tree,
            highlights_query,
            language_name,
            document_id,
            revision,
            cached_logical_bytes,
            single_edit,
            _lib,
            old_highlights,
        } = *self;

        // Uses logical bytes, not the rendered form, so tree-sitter offsets match
        // the query cursor's coordinate space (control chars differ in byte width).
        let text = buffer;
        let source_bytes = {
            let will_patch = cached_logical_bytes.is_some() && single_edit.is_some();
            crate::perf_span!(
                "syntax_reparse_job_to_bytes",
                crate::perf::PerfFields {
                    bytes: Some(text.byte_len() as u32),
                    tag: Some(if will_patch { "patch_attempt" } else { "full" }),
                    ..Default::default()
                }
            );
            match (&cached_logical_bytes, &single_edit) {
                (Some(cached), Some(edit)) => text
                    .patch_logical_bytes(
                        cached,
                        edit.start_byte,
                        edit.old_end_byte,
                        edit.new_end_byte,
                    )
                    .unwrap_or_else(|| text.to_logical_bytes()),
                _ => text.to_logical_bytes(),
            }
        };

        let tree = {
            crate::perf_span!(
                "syntax_reparse_job_parse",
                crate::perf::PerfFields {
                    bytes: Some(source_bytes.len() as u32),
                    tag: Some(if old_tree.is_some() {
                        "incremental"
                    } else {
                        "full"
                    }),
                    ..Default::default()
                }
            );
            parser.parse(source_bytes.as_slice(), old_tree.as_ref())
        };

        if signal.is_cancelled() {
            return;
        }

        // On a single-edit incremental reparse, scope the query to what
        // tree-sitter says changed and reuse the rest of the old highlights.
        let mut highlights = Vec::new();
        if let (Some(tree), Some(query)) = (&tree, &highlights_query) {
            let scoped = match (old_tree.as_ref(), single_edit) {
                (Some(prev_tree), Some(edit)) => Some((prev_tree, edit)),
                _ => None,
            };
            crate::perf_span!(
                "syntax_reparse_job_highlights",
                crate::perf::PerfFields {
                    bytes: Some(source_bytes.len() as u32),
                    tag: Some(if scoped.is_some() { "scoped" } else { "full" }),
                    ..Default::default()
                }
            );
            let root_node = tree.root_node();

            // Single-range vec matches the Vec<Range<usize>> the scoped arm returns.
            #[allow(clippy::single_range_in_vec_init)]
            let query_ranges: Vec<std::ops::Range<usize>> = match scoped {
                Some((prev_tree, edit)) => {
                    crate::syntax::scoped_query_ranges(prev_tree, tree, edit)
                }
                None => vec![0..source_bytes.len()],
            };

            let mut fresh: Vec<(std::ops::Range<usize>, u32, usize)> = Vec::new();
            for range in query_ranges {
                let mut cursor = QueryCursor::new();
                cursor.set_byte_range(range);
                let mut matches = cursor.matches(query, root_node, source_bytes.as_slice());
                while let Some(m) = matches.next() {
                    if signal.is_cancelled() {
                        return;
                    }
                    let pattern_index = m.pattern_index;
                    for capture in m.captures {
                        fresh.push((capture.node.byte_range(), capture.index, pattern_index));
                    }
                }
            }

            // Keep old entries the fresh requery above doesn't overlap; only
            // it can tell whether a boundary-touching range was affected.
            let fresh_ranges: Vec<(std::ops::Range<usize>, u32)> =
                fresh.iter().map(|(r, c, _)| (r.clone(), *c)).collect();
            if let Some((_, edit)) = scoped {
                let kept = crate::syntax::scoped_kept_items(&old_highlights, edit, &fresh_ranges);
                highlights.extend(kept.into_iter().map(|(r, c)| (r, c, 0)));
            }
            highlights.extend(fresh);
        }

        let result = SyntaxParseResult {
            tree,
            highlights: IntervalTree::new(crate::syntax::finalize_highlights(highlights)),
            language_name,
            document_id,
            revision,
            logical_bytes: source_bytes,
        };

        crate::job_manager::send_job_result(&sender, id, Box::new(result));
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[cfg(feature = "treesitter")]
mod tests {
    use super::*;
    use crate::syntax::loader::{LanguageLoader, LoadedLanguage};
    use crate::syntax::Syntax;
    use std::path::PathBuf;
    use std::sync::mpsc;

    fn make_signal() -> CancellationSignal {
        CancellationSignal::new(false)
    }

    fn rust_language_and_query() -> (tree_sitter::Language, Arc<Query>) {
        let loader = LanguageLoader::new(PathBuf::from("."));
        let loaded = loader.load_language("rust").expect("rust grammar");
        let query_src = loader
            .load_query("rust", "highlights")
            .expect("rust highlights query");
        let query = Query::new(&loaded.language, &query_src).expect("compile highlights query");
        (loaded.language, Arc::new(query))
    }

    fn make_buffer(src: &str) -> TextBuffer {
        let mut buffer = TextBuffer::new(src.len().max(16)).unwrap();
        buffer.insert_str(src).unwrap();
        buffer
    }

    fn run_job(job: SyntaxParseJob) -> SyntaxParseResult {
        let (tx, rx) = mpsc::channel();
        Box::new(job).run(1, tx, make_signal());

        *crate::job_manager::jobs::test_support::recv_custom_payload::<SyntaxParseResult>(&rx)
            .expect("SyntaxParseJob did not produce a SyntaxParseResult")
    }

    fn sorted_highlights(tree: &IntervalTree<u32>) -> Vec<(std::ops::Range<usize>, u32)> {
        let mut items: Vec<_> = tree.iter().map(|(r, v)| (r.clone(), *v)).collect();
        items.sort_by(|a, b| {
            a.0.start
                .cmp(&b.0.start)
                .then(a.0.end.cmp(&b.0.end))
                .then(a.1.cmp(&b.1))
        });
        items
    }

    /// A scoped single-edit reparse must match a full from-scratch parse of
    /// the same edited content.
    #[test]
    fn test_scoped_highlights_match_full_recompute_after_single_edit() {
        let (language, query) = rust_language_and_query();

        let initial_src = "fn foo() { let a = 1; let b = 2; }";
        let buffer1 = make_buffer(initial_src);

        let mut parser1 = Parser::new();
        parser1.set_language(&language).unwrap();
        let tree1 = parser1
            .parse(buffer1.to_logical_bytes().as_slice(), None)
            .unwrap();

        let initial_highlights = {
            let root = tree1.root_node();
            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(0..buffer1.byte_len());
            let source = buffer1.to_logical_bytes();
            let mut matches = cursor.matches(&query, root, source.as_slice());
            let mut items = Vec::new();
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    items.push((capture.node.byte_range(), capture.index));
                }
            }
            IntervalTree::new(items)
        };

        // Insert one character ("1" -> "11") inside the first literal.
        let insert_pos = initial_src.find("1;").unwrap() + 1;
        let mut buffer2 = buffer1.clone();
        buffer2.set_cursor(insert_pos).unwrap();
        buffer2.insert_str("1").unwrap();

        let edit = InputEdit {
            start_byte: insert_pos,
            old_end_byte: insert_pos,
            new_end_byte: insert_pos + 1,
            start_position: tree_sitter::Point {
                row: 0,
                column: insert_pos,
            },
            old_end_position: tree_sitter::Point {
                row: 0,
                column: insert_pos,
            },
            new_end_position: tree_sitter::Point {
                row: 0,
                column: insert_pos + 1,
            },
        };
        let mut old_tree_edited = tree1.clone();
        old_tree_edited.edit(&edit);

        let mut parser2 = Parser::new();
        parser2.set_language(&language).unwrap();
        let job = SyntaxParseJob::new(
            buffer2.clone(),
            parser2,
            Some(old_tree_edited),
            Some(query.clone()),
            "rust".to_string(),
            1,
            buffer2.revision,
        )
        .with_highlights_context(initial_highlights.clone(), std::slice::from_ref(&edit));

        let result = run_job(job);

        // Ground truth: a full from-scratch parse and query of the edited content.
        let mut parser3 = Parser::new();
        parser3.set_language(&language).unwrap();
        let expected_source = buffer2.to_logical_bytes();
        let expected_tree = parser3.parse(expected_source.as_slice(), None).unwrap();
        let expected_highlights = {
            let root = expected_tree.root_node();
            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(0..expected_source.len());
            let mut matches = cursor.matches(&query, root, expected_source.as_slice());
            let mut items = Vec::new();
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    items.push((capture.node.byte_range(), capture.index));
                }
            }
            IntervalTree::new(items)
        };

        assert_eq!(
            sorted_highlights(&result.highlights),
            sorted_highlights(&expected_highlights),
        );
    }

    fn point_at(source: &[u8], byte: usize) -> tree_sitter::Point {
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

    /// Editing inside a string literal must not change the capture (color)
    /// of an unrelated `Ok(...)` constructor call later on the same line.
    #[test]
    fn test_scoped_highlights_do_not_recolor_unrelated_token_on_same_line() {
        let (language, query) = rust_language_and_query();

        let initial_src = "fn f(s: &str) -> Result<Action, ()> {\n    match s {\n        \"lsp:diagnostics_panel\" => Ok(Action::Editor(EditorAction::LspDiagnosticsPanel)),\n        _ => Ok(Action::Noop),\n    }\n}\n";
        let buffer1 = make_buffer(initial_src);
        let source1 = buffer1.to_logical_bytes();

        let mut parser1 = Parser::new();
        parser1.set_language(&language).unwrap();
        let tree1 = parser1.parse(source1.as_slice(), None).unwrap();
        let initial_highlights = full_highlights(&language, &query, source1.as_slice());

        let ok_start = initial_src.find("Ok(Action").unwrap();
        let ok_range = ok_start..ok_start + 2;
        let initial_ok_capture: Vec<u32> = initial_highlights
            .query(ok_range.clone())
            .into_iter()
            .filter(|(r, _)| *r == ok_range)
            .map(|(_, c)| c)
            .collect();

        // Insert one character right after "lsp" inside the string literal.
        let insert_pos = initial_src.find("lsp:diagnostics_panel").unwrap() + 3;
        let mut buffer2 = buffer1.clone();
        buffer2.set_cursor(insert_pos).unwrap();
        buffer2.insert_str("x").unwrap();
        let source2 = buffer2.to_logical_bytes();

        let edit = InputEdit {
            start_byte: insert_pos,
            old_end_byte: insert_pos,
            new_end_byte: insert_pos + 1,
            start_position: point_at(&source1, insert_pos),
            old_end_position: point_at(&source1, insert_pos),
            new_end_position: point_at(&source2, insert_pos + 1),
        };
        let mut old_tree_edited = tree1.clone();
        old_tree_edited.edit(&edit);

        let mut parser2 = Parser::new();
        parser2.set_language(&language).unwrap();
        let job = SyntaxParseJob::new(
            buffer2.clone(),
            parser2,
            Some(old_tree_edited),
            Some(query.clone()),
            "rust".to_string(),
            1,
            buffer2.revision,
        )
        .with_highlights_context(initial_highlights.clone(), std::slice::from_ref(&edit));

        let result = run_job(job);

        let expected_highlights = full_highlights(&language, &query, source2.as_slice());

        let shifted_ok_range = (ok_range.start + 1)..(ok_range.end + 1);
        let scoped_ok: Vec<u32> = result
            .highlights
            .query(shifted_ok_range.clone())
            .into_iter()
            .filter(|(r, _)| *r == shifted_ok_range)
            .map(|(_, c)| c)
            .collect();
        let expected_ok: Vec<u32> = expected_highlights
            .query(shifted_ok_range.clone())
            .into_iter()
            .filter(|(r, _)| *r == shifted_ok_range)
            .map(|(_, c)| c)
            .collect();

        // "Ok" matches both @constructor and @function; the render picks
        // whichever comes first, so order must match a full recompute.
        assert_eq!(
            initial_ok_capture.len(),
            2,
            "test fixture must exercise a dual-capture token"
        );
        assert_eq!(
            scoped_ok, expected_ok,
            "capture order for \"Ok\" diverged after a scoped edit elsewhere on the line"
        );

        assert_eq!(
            sorted_highlights(&result.highlights),
            sorted_highlights(&expected_highlights),
            "scoped highlights diverged from a full recompute after editing inside the string"
        );
    }

    #[test]
    fn test_scoped_highlights_real_file_diagnostics_panel_line() {
        let (language, query) = rust_language_and_query();
        let initial_src = include_str!("../../action/mod.rs");
        let buffer1 = make_buffer(initial_src);
        let source1 = buffer1.to_logical_bytes();

        let mut parser1 = Parser::new();
        parser1.set_language(&language).unwrap();
        let tree1 = parser1.parse(source1.as_slice(), None).unwrap();
        let initial_highlights = full_highlights(&language, &query, source1.as_slice());

        let insert_pos = initial_src.find("lsp:diagnostics_panel").unwrap() + 3;
        let mut buffer2 = buffer1.clone();
        buffer2.set_cursor(insert_pos).unwrap();
        buffer2.insert_str("x").unwrap();
        let source2 = buffer2.to_logical_bytes();

        let edit = InputEdit {
            start_byte: insert_pos,
            old_end_byte: insert_pos,
            new_end_byte: insert_pos + 1,
            start_position: point_at(&source1, insert_pos),
            old_end_position: point_at(&source1, insert_pos),
            new_end_position: point_at(&source2, insert_pos + 1),
        };
        let mut old_tree_edited = tree1.clone();
        old_tree_edited.edit(&edit);

        let mut parser2 = Parser::new();
        parser2.set_language(&language).unwrap();
        let job = SyntaxParseJob::new(
            buffer2.clone(),
            parser2,
            Some(old_tree_edited),
            Some(query.clone()),
            "rust".to_string(),
            1,
            buffer2.revision,
        )
        .with_highlights_context(initial_highlights.clone(), std::slice::from_ref(&edit));

        let result = run_job(job);
        let expected_highlights = full_highlights(&language, &query, source2.as_slice());

        // "Ok" sits after the inserted character on the same line, so its
        // post-edit position is shifted by one byte from its pre-edit offset.
        let ok_start = initial_src
            .find("Ok(Action::Editor(EditorAction::LspDiagnosticsPanel")
            .unwrap()
            + 1;
        let ok_range = ok_start..ok_start + 2;
        assert_eq!(
            result.highlights.query(ok_range.clone()),
            expected_highlights.query(ok_range),
            "capture order for \"Ok\" diverged from a full recompute on the real file"
        );

        assert_eq!(
            sorted_highlights(&result.highlights),
            sorted_highlights(&expected_highlights),
            "scoped highlights diverged from a full recompute on the real file"
        );
    }

    fn full_highlights(
        language: &tree_sitter::Language,
        query: &Query,
        source: &[u8],
    ) -> IntervalTree<u32> {
        let mut parser = Parser::new();
        parser.set_language(language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(0..source.len());
        let mut matches = cursor.matches(query, root, source);
        let mut items = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                items.push((capture.node.byte_range(), capture.index));
            }
        }
        IntervalTree::new(items)
    }

    /// Simulates fast typing (each keystroke's job chains onto the last).
    /// A subtle scoping bug can compound into flickering without ever failing a single-edit test.
    #[test]
    fn test_scoped_highlights_stay_correct_across_many_sequential_edits() {
        let (language, query) = rust_language_and_query();

        let mut buffer = make_buffer("fn foo() {\n}\n");
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let mut tree = parser
            .parse(buffer.to_logical_bytes().as_slice(), None)
            .unwrap();
        let mut highlights =
            full_highlights(&language, &query, buffer.to_logical_bytes().as_slice());

        // Type this, one character at a time, right before the closing brace.
        let to_type = "    let x = 12345;\n    let y = x + 1;\n";
        let mut insert_pos = buffer.to_logical_bytes().len() - "}\n".len();

        for ch in to_type.chars() {
            let mut ch_buf = [0u8; 4];
            let ch_bytes = ch.encode_utf8(&mut ch_buf).as_bytes();

            let old_source = buffer.to_logical_bytes();
            let start_position = point_at(&old_source, insert_pos);

            buffer.set_cursor(insert_pos).unwrap();
            buffer.insert_str(&ch.to_string()).unwrap();
            let new_source = buffer.to_logical_bytes();
            let new_end_position = point_at(&new_source, insert_pos + ch_bytes.len());

            let edit = InputEdit {
                start_byte: insert_pos,
                old_end_byte: insert_pos,
                new_end_byte: insert_pos + ch_bytes.len(),
                start_position,
                old_end_position: start_position,
                new_end_position,
            };
            tree.edit(&edit);

            let mut step_parser = Parser::new();
            step_parser.set_language(&language).unwrap();
            let job = SyntaxParseJob::new(
                buffer.clone(),
                step_parser,
                Some(tree.clone()),
                Some(query.clone()),
                "rust".to_string(),
                1,
                buffer.revision,
            )
            .with_highlights_context(highlights.clone(), std::slice::from_ref(&edit));

            let result = run_job(job);
            tree = result.tree.expect("each step must produce a tree");
            highlights = result.highlights;
            insert_pos += ch_bytes.len();

            // Zero-width ranges are transient error-recovery artifacts of
            // incomplete syntax mid-typing; they can't render, so exclude them.
            let step_expected =
                full_highlights(&language, &query, buffer.to_logical_bytes().as_slice());
            let non_empty = |v: Vec<(std::ops::Range<usize>, u32)>| -> Vec<_> {
                v.into_iter().filter(|(r, _)| !r.is_empty()).collect()
            };
            assert_eq!(
                non_empty(sorted_highlights(&highlights)),
                non_empty(sorted_highlights(&step_expected)),
                "highlights diverged from a full recompute after typing {ch:?}"
            );
        }

        let expected = full_highlights(&language, &query, buffer.to_logical_bytes().as_slice());
        assert_eq!(sorted_highlights(&highlights), sorted_highlights(&expected));
    }

    fn rust_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("set rust language");
        parser
    }

    fn rust_syntax() -> Syntax {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        Syntax::new(LoadedLanguage::bundled(lang, "rust"), None).expect("Syntax::new")
    }

    /// Patch path wired up as `Syntax`/`editor::jobs` would drive it must
    /// match a full `to_logical_bytes()` rebuild of the edited buffer.
    #[test]
    fn patch_path_matches_full_rebuild_end_to_end() {
        let mut buffer = TextBuffer::new(64).unwrap();
        buffer
            .insert_str("fn main() {\n    let x = 1;\n}\n")
            .unwrap();
        let mut syntax = rust_syntax();

        let job1 = SyntaxParseJob::new(
            buffer.clone(),
            rust_parser(),
            syntax.tree.clone(),
            None,
            "rust".to_string(),
            0,
            buffer.revision,
        );
        let result1 = run_job(job1);
        assert_eq!(result1.logical_bytes, buffer.to_logical_bytes());
        assert!(result1.tree.is_some());
        syntax.update_from_result(result1);

        // Insert one character right after 'x', mirroring how `document::edit`
        // derives an edit and calls `Syntax::notify_edit`.
        let insert_pos = 21;
        let start_byte = buffer.char_to_byte(insert_pos);
        buffer.set_cursor(insert_pos).unwrap();
        buffer.insert_str("y").unwrap();
        syntax.notify_edit(
            start_byte,
            start_byte,
            start_byte + 1,
            (0, 0),
            (0, 0),
            (0, 0),
        );

        let (cached_logical_bytes, single_edit) = syntax.incremental_logical_bytes();
        assert!(
            cached_logical_bytes.is_some() && single_edit.is_some(),
            "a single edit after a completed parse should surface a patchable cache"
        );

        let job2 = SyntaxParseJob::new(
            buffer.clone(),
            rust_parser(),
            syntax.tree.clone(),
            None,
            "rust".to_string(),
            0,
            buffer.revision,
        )
        .with_incremental_bytes(cached_logical_bytes, single_edit);

        let result2 = run_job(job2);
        assert_eq!(result2.logical_bytes, buffer.to_logical_bytes());
        assert!(result2.tree.is_some());
    }
}
