#![cfg(feature = "treesitter")]

use super::loader::LanguageLoader;
use super::{build_syntax, Syntax};
use crate::buffer::TextBuffer;
use std::ops::Range;
use std::sync::Arc;
use tree_sitter::Query;

fn buffer(text: &str) -> TextBuffer {
    let mut buffer = TextBuffer::new(text.len() + 64).unwrap();
    crate::document::ops::Edit::type_text(text).apply_to(&mut buffer);
    buffer
}

fn syntax(loader: &Arc<LanguageLoader>, language: &str, buffer: &TextBuffer) -> Syntax {
    let grammar = loader.load_language(language).unwrap();
    let query = loader.load_query(language, "highlights").unwrap();
    let query = Arc::new(Query::new(&grammar.language, &query).unwrap());
    let mut syntax = build_syntax(grammar, Some(query), loader);
    assert!(syntax.incremental_parse(buffer));
    syntax
}

fn point(text: &str, byte: usize) -> (usize, usize) {
    let prefix = &text[..byte];
    let row = prefix.bytes().filter(|&b| b == b'\n').count();
    let column = prefix
        .rfind('\n')
        .map_or(byte, |newline| byte - newline - 1);
    (row, column)
}

fn edit(
    syntax: &mut Syntax,
    buffer: &mut TextBuffer,
    text: &mut String,
    range: Range<usize>,
    replacement: &str,
) {
    let start_point = point(text, range.start);
    let old_end_point = point(text, range.end);
    let start_char = text[..range.start].chars().count();
    let old_chars = text[range.clone()].chars().count();
    if old_chars > 0 {
        assert!(
            crate::document::ops::Edit::delete(start_char..start_char + old_chars).apply_to(buffer)
        );
    }
    assert!(crate::document::ops::standalone::place_cursor(
        buffer, start_char
    ));
    crate::document::ops::Edit::type_text(replacement).apply_to(buffer);
    text.replace_range(range.clone(), replacement);
    let new_end = range.start + replacement.len();
    syntax.notify_edit(
        range.start,
        range.end,
        new_end,
        start_point,
        old_end_point,
        point(text, new_end),
    );
}

fn paint(captures: &[(Range<usize>, String)], viewport: Range<usize>) -> Vec<Option<&str>> {
    let mut painted = vec![None; viewport.len()];
    for (range, name) in captures {
        for byte in range.start.max(viewport.start)..range.end.min(viewport.end) {
            painted[byte - viewport.start].get_or_insert(name.as_str());
        }
    }
    painted
}

fn assert_matches_fresh(
    incremental: &mut Syntax,
    loader: &Arc<LanguageLoader>,
    language: &str,
    buffer: &TextBuffer,
    len: usize,
    viewport: Range<usize>,
) {
    let mut fresh = syntax(loader, language, buffer);
    let all = fresh.injection_highlights_named(buffer, 0..len);
    let actual = incremental.injection_highlights_named(buffer, viewport.clone());
    assert_eq!(
        paint(&actual, viewport.clone()),
        paint(&all, viewport.clone())
    );
    assert_eq!(actual, fresh.injection_highlights_named(buffer, viewport));
}

#[test]
fn html_closing_comment_refreshes_injection_captures_before_edit() {
    let loader = Arc::new(LanguageLoader::new(".".into()));
    let mut text = format!(
        "<script>\n/*\n{}const visible = 7;\n</script>\n",
        "comment payload\n".repeat(900),
    );
    let mut buffer = buffer(&text);
    let mut syntax = syntax(&loader, "html", &buffer);
    let _ = syntax.highlights(&buffer, 0..text.len());
    let _ = syntax.injection_highlights_named(&buffer, 0..text.len());

    let close = text.find("const visible").unwrap();
    edit(&mut syntax, &mut buffer, &mut text, close..close, "*/\n");
    assert!(syntax.incremental_parse(&buffer));

    let deep = 9000..9100;
    let captures = syntax.injection_highlights_named(&buffer, deep.clone());
    assert!(paint(&captures, deep.clone())
        .iter()
        .all(|name| name.is_some_and(|name| name.starts_with("comment"))));
    assert_matches_fresh(&mut syntax, &loader, "html", &buffer, text.len(), deep);
    assert_matches_fresh(
        &mut syntax,
        &loader,
        "html",
        &buffer,
        text.len(),
        0..text.len(),
    );
}

#[test]
fn html_removing_script_drops_cached_injection_and_moves_survivor() {
    let loader = Arc::new(LanguageLoader::new(".".into()));
    let mut text = format!(
        "<p>before</p>\n<script>\n{}</script>\n<p>after</p>\n<script>\nconst survivor = 7;\n</script>\n",
        "const removed = 1;\n".repeat(700),
    );
    let mut buffer = buffer(&text);
    let mut syntax = syntax(&loader, "html", &buffer);
    let _ = syntax.highlights(&buffer, 0..text.len());
    assert_ne!(
        syntax.injection_highlights_named(&buffer, 0..text.len()),
        Vec::new()
    );

    let start = text.find("<script>").unwrap();
    let end = text.find("</script>\n").unwrap() + "</script>\n".len();
    edit(&mut syntax, &mut buffer, &mut text, start..end, "");
    assert!(syntax.incremental_parse(&buffer));

    let captures = syntax.injection_highlights_named(&buffer, 0..text.len());
    let survivor = text.find("7;").unwrap();
    assert!(captures
        .iter()
        .any(|(range, name)| range.contains(&survivor) && name.starts_with("number")));
    let content = text.find("const survivor").unwrap();
    assert!(captures.iter().all(|(range, _)| range.start >= content));
    assert_matches_fresh(
        &mut syntax,
        &loader,
        "html",
        &buffer,
        text.len(),
        0..text.len(),
    );
}

#[test]
fn markdown_deep_fenced_rust_capture_disappears_when_language_changes() {
    let loader = Arc::new(LanguageLoader::new(".".into()));
    let mut text = format!(
        "# Example\n\n```rust\nconst MESSAGE: &str = r#\"\n{}\"#;\n```\n",
        "a long raw string payload\n".repeat(600),
    );
    let mut buffer = buffer(&text);
    let mut syntax = syntax(&loader, "markdown", &buffer);
    let deep = 10000..10100;
    let captures = syntax.injection_highlights_named(&buffer, deep.clone());
    assert!(paint(&captures, deep.clone())
        .iter()
        .all(|name| name.is_some_and(|name| name.starts_with("string"))));
    assert_matches_fresh(&mut syntax, &loader, "markdown", &buffer, text.len(), deep);
    let _ = syntax.highlights(&buffer, 0..text.len());
    let _ = syntax.injection_highlights_named(&buffer, 0..text.len());

    let language = text.find("rust").unwrap();
    edit(
        &mut syntax,
        &mut buffer,
        &mut text,
        language..language + "rust".len(),
        "text",
    );
    assert!(syntax.incremental_parse(&buffer));
    assert_eq!(
        syntax.injection_highlights_named(&buffer, 0..text.len()),
        Vec::new()
    );
    assert_matches_fresh(
        &mut syntax,
        &loader,
        "markdown",
        &buffer,
        text.len(),
        0..text.len(),
    );
}

#[test]
fn nested_capture_wins_over_the_capture_around_it() {
    let loader = Arc::new(LanguageLoader::new(".".into()));
    let text = "fn f() { let s = \"a\\nb\"; }\n";
    let buffer = buffer(text);
    let mut syntax = syntax(&loader, "rust", &buffer);
    let names: Vec<String> = syntax
        .capture_names()
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let spans = syntax.highlights(&buffer, 0..text.len());
    let name_at = |byte: usize| {
        spans
            .iter()
            .find(|(r, _)| r.contains(&byte))
            .map(|(_, c)| names[*c as usize].as_str())
    };

    let escape = text.find("\\n").unwrap();
    assert_eq!(name_at(escape - 1), Some("string"));
    assert_eq!(name_at(escape), Some("escape"));
    assert_eq!(name_at(escape + 2), Some("string"));
    assert!(
        spans.windows(2).all(|w| w[0].0.end <= w[1].0.start),
        "spans must be sorted and non-overlapping: {spans:?}"
    );
}
