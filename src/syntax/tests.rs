use super::interval_tree::IntervalTree;
use crate::buffer::TextBuffer;

#[test]
fn test_text_provider_chunks() {
    let mut buffer = TextBuffer::new(100).unwrap();
    buffer.insert_str("line1\nline2\nline3").unwrap();

    assert_eq!(buffer.to_string(), "line1\nline2\nline3");
}

#[test]
fn test_syntax_new_placeholder() {
    // Basic test to ensure TextBuffer is usable
    let buffer = TextBuffer::new(10).unwrap();
    assert_eq!(buffer.len(), 0);
}

#[test]
fn test_byte_to_point_matches_reference_scan() {
    // Reference implementation: rescans source[..byte] each call.
    fn reference_byte_to_point(source: &[u8], byte: usize) -> tree_sitter::Point {
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

    let source = b"line1\nline2\n\nline4\nlast line no newline";
    let index = super::NewlineIndex::build(source);
    for byte in 0..=source.len() {
        let expected = reference_byte_to_point(source, byte);
        let actual = index.point_at(byte);
        assert_eq!(actual, expected, "byte_to_point mismatch at offset {byte}");
    }
}

// =============================================================================
// IntervalTree Tests
// =============================================================================

#[test]
fn test_interval_tree_basic() {
    let items = vec![(0..10, 1), (5..15, 2), (20..30, 3)];

    let tree = IntervalTree::new(items);

    let res = tree.query(0..5);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].1, 1);

    let res = tree.query(5..10);
    assert_eq!(res.len(), 2);

    let res = tree.query(16..19);
    assert_eq!(res.len(), 0);

    let res = tree.query(25..26);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].1, 3);
}

#[test]
fn test_interval_tree_nested() {
    let items = vec![(0..100, 1), (10..20, 2), (50..60, 3)];

    let tree = IntervalTree::new(items);

    let res = tree.query(15..16);
    assert_eq!(res.len(), 2);

    let res = tree.query(55..56);
    assert_eq!(res.len(), 2);

    let res = tree.query(5..6);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].1, 1);
}

#[test]
fn test_interval_tree_empty() {
    let tree: IntervalTree<i32> = IntervalTree::new(vec![]);
    assert!(tree.query(0..10).is_empty());
}

#[test]
fn test_interval_tree_sorted_query() {
    // Tree structure: Root (5..15), Left (0..10), Right (20..30)
    let items = vec![(0..10, 1), (5..15, 2), (20..30, 3)];

    let tree = IntervalTree::new(items);

    // Query (0..30) should return all, sorted by start
    let res = tree.query(0..30);
    assert_eq!(res.len(), 3);
    assert_eq!(res[0].1, 1); // 0..10 sorted first
    assert_eq!(res[1].1, 2); // 5..15
    assert_eq!(res[2].1, 3); // 20..30
}

// =============================================================================
// Svelte / injection highlighting tests
// =============================================================================

#[cfg(feature = "treesitter")]
mod svelte_tests {
    use super::super::*;
    use crate::syntax::loader::{LanguageLoader, LoadedLanguage};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn make_loader() -> Arc<LanguageLoader> {
        Arc::new(LanguageLoader::new(PathBuf::from(".")))
    }

    fn svelte_syntax(loader: &Arc<LanguageLoader>) -> Syntax {
        let loaded = loader.load_language("svelte").expect("svelte grammar");
        let highlights_query = loader
            .load_query("svelte", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax")
    }

    const SVELTE_SRC: &str = r#"<script lang="ts">
  let count: number = 0;
  function increment() { count++; }
</script>

<style>
  button { color: red; }
</style>

<button on:click={increment}>{count}</button>
"#;

    #[test]
    fn test_try_incremental_parse_completes_within_generous_budget() {
        let loader = make_loader();
        let mut syntax = svelte_syntax(&loader);
        let src = SVELTE_SRC.as_bytes();
        let outcome = syntax.try_incremental_parse(src, std::time::Duration::from_secs(5));
        assert_eq!(outcome, crate::syntax::ParseOutcome::Completed);
        assert!(syntax.tree.is_some());
        assert!(!syntax.highlights(None).is_empty());
    }

    #[test]
    fn test_try_incremental_parse_aborts_on_zero_budget_leaves_tree_untouched() {
        let loader = make_loader();
        let mut syntax = svelte_syntax(&loader);
        let src = SVELTE_SRC.as_bytes();
        let outcome = syntax.try_incremental_parse(src, std::time::Duration::ZERO);
        assert_eq!(outcome, crate::syntax::ParseOutcome::Aborted);
        assert!(
            syntax.tree.is_none(),
            "aborted parse must not commit a tree"
        );
    }

    #[test]
    fn test_svelte_host_parse_succeeds() {
        let loader = make_loader();
        let mut syntax = svelte_syntax(&loader);
        let src = SVELTE_SRC.as_bytes();
        let ok = syntax.incremental_parse(src);
        assert!(ok, "Svelte host grammar parse should succeed");
        assert!(syntax.tree.is_some());
    }

    #[test]
    fn test_svelte_host_highlights_nonempty() {
        let loader = make_loader();
        let mut syntax = svelte_syntax(&loader);
        let src = SVELTE_SRC.as_bytes();
        syntax.incremental_parse(src);
        let hl = syntax.highlights(None);
        assert!(!hl.is_empty(), "Svelte should have syntax highlights");
    }

    #[test]
    fn test_svelte_injection_layers_populated() {
        let loader = make_loader();
        let mut syntax = svelte_syntax(&loader);
        let src = SVELTE_SRC.as_bytes();
        syntax.incremental_parse(src);
        // After parse, injection layers for TypeScript and CSS should be present.
        assert!(
            !syntax.injection_layers.is_empty(),
            "Svelte injection layers should be populated after parse"
        );
    }

    #[test]
    fn test_svelte_injection_highlights_nonempty() {
        let loader = make_loader();
        let mut syntax = svelte_syntax(&loader);
        let src = SVELTE_SRC.as_bytes();
        syntax.incremental_parse(src);
        let inj = syntax.injection_highlights_named(None);
        assert!(
            !inj.is_empty(),
            "Svelte injection highlights (TypeScript/CSS) should be non-empty"
        );
    }

    #[test]
    fn test_svelte_typescript_injection_has_keyword() {
        let loader = make_loader();
        let mut syntax = svelte_syntax(&loader);
        let src = SVELTE_SRC.as_bytes();
        syntax.incremental_parse(src);
        let inj = syntax.injection_highlights_named(None);
        // The TypeScript layer should produce at least one "keyword" or "function" capture.
        let has_keyword = inj.iter().any(|(_, name)| {
            name.starts_with("keyword") || name.starts_with("function") || name.starts_with("type")
        });
        assert!(
            has_keyword,
            "TypeScript injection should highlight keywords/types; got: {:?}",
            inj.iter().take(10).map(|(_, n)| n).collect::<Vec<_>>()
        );
    }
}

// =============================================================================
// Markdown code-block injection tests
// =============================================================================

#[cfg(feature = "treesitter")]
mod markdown_tests {
    use super::super::*;
    use crate::syntax::loader::LanguageLoader;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn make_loader() -> Arc<LanguageLoader> {
        Arc::new(LanguageLoader::new(PathBuf::from(".")))
    }

    const MD_SRC: &str =
        "# Hello\n\n```rust\nlet x: u32 = 42;\n```\n\n```python\ndef foo(): pass\n```\n";

    #[test]
    fn test_markdown_parse_succeeds() {
        let loader = make_loader();
        let loaded = loader.load_language("markdown").expect("markdown grammar");
        let highlights_query = loader
            .load_query("markdown", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let mut syntax =
            build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");
        let ok = syntax.incremental_parse(MD_SRC.as_bytes());
        assert!(ok, "Markdown parse should succeed");
    }

    #[test]
    fn test_markdown_rust_injection_highlights() {
        let loader = make_loader();
        let loaded = loader.load_language("markdown").expect("markdown grammar");
        let highlights_query = loader
            .load_query("markdown", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let mut syntax =
            build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");
        syntax.incremental_parse(MD_SRC.as_bytes());

        let inj = syntax.injection_highlights_named(None);
        // The Rust block should produce at least one highlight.
        assert!(
            !inj.is_empty(),
            "Markdown Rust code block should produce injection highlights"
        );
    }

    #[test]
    fn test_markdown_many_paragraphs_parses_quickly() {
        // Many paragraphs each trigger an injection range; a per-range
        // source rescan here would make this quadratic and blow the budget.
        let n = 2_000;
        let mut src = String::new();
        for i in 0..n {
            src.push_str(&format!("Paragraph number {i} with some text in it.\n\n"));
            src.push_str("```rust\nlet x = 1;\n```\n\n");
        }

        let loader = make_loader();
        let loaded = loader.load_language("markdown").expect("markdown grammar");
        let highlights_query = loader
            .load_query("markdown", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let mut syntax =
            build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");

        let start = std::time::Instant::now();
        let ok = syntax.incremental_parse(src.as_bytes());
        let elapsed = start.elapsed();

        assert!(ok, "Markdown parse should succeed");
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "parsing {n} paragraphs took {elapsed:?}, expected well under 1s"
        );
    }

    #[test]
    fn test_markdown_dynamic_injection_query_reused_across_parses() {
        let loader = make_loader();
        let loaded = loader.load_language("markdown").expect("markdown grammar");
        let highlights_query = loader
            .load_query("markdown", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let mut syntax =
            build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");

        syntax.incremental_parse(MD_SRC.as_bytes());
        let first_query_ptr = syntax
            .dynamic_injection_layers
            .get("rust")
            .and_then(|l| l.highlights_query.as_ref())
            .map(Arc::as_ptr)
            .expect("rust layer with compiled query after first parse");

        syntax.incremental_parse(MD_SRC.as_bytes());
        let second_query_ptr = syntax
            .dynamic_injection_layers
            .get("rust")
            .and_then(|l| l.highlights_query.as_ref())
            .map(Arc::as_ptr)
            .expect("rust layer with compiled query after second parse");

        assert_eq!(
            first_query_ptr, second_query_ptr,
            "the compiled highlights Query for an unchanged embedded language must be reused, not recompiled, across consecutive parses"
        );
    }

    const MD_HTML_SRC: &str =
        "# Hello\n\n<div class=\"foo\">\n  <span>bar</span>\n</div>\n\nplain text after\n";

    #[test]
    fn test_markdown_html_block_injection_highlights() {
        let loader = make_loader();
        let loaded = loader.load_language("markdown").expect("markdown grammar");
        let highlights_query = loader
            .load_query("markdown", "highlights")
            .ok()
            .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
            .map(Arc::new);
        let mut syntax =
            build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");
        syntax.incremental_parse(MD_HTML_SRC.as_bytes());

        // The HTML block's language comes from a `#set! injection.language` predicate,
        // not a captured text node, so it must still produce an injection layer.
        assert!(
            !syntax.dynamic_injection_layers.is_empty(),
            "Markdown HTML block should produce a dynamic injection layer"
        );
        let inj = syntax.injection_highlights_named(None);
        assert!(
            !inj.is_empty(),
            "Markdown HTML block should produce injection highlights"
        );
    }
}

// Dynamic grammar registration dedup

#[cfg(feature = "treesitter")]
mod register_grammar_tests {
    use crate::syntax::loader::LanguageLoader;
    use std::path::PathBuf;

    #[test]
    fn test_register_grammar_dedup_skips_second_library_load() {
        let loader = LanguageLoader::new(PathBuf::from("."));
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();

        let first = loader.register_grammar_for_test("dup_lang", lang.clone());
        assert!(first, "first registration should succeed");
        assert_eq!(loader.loaded_libs_count(), 1);

        let second = loader.register_grammar_for_test("dup_lang", lang);
        assert!(!second, "duplicate registration should be a no-op");
        assert_eq!(
            loader.loaded_libs_count(),
            1,
            "loaded_libs must not grow when the same grammar is registered twice"
        );

        let resolved = loader.load_language("dup_lang").expect("dup_lang resolves");
        assert_eq!(resolved.name, "dup_lang");
    }
}
