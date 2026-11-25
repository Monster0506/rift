use super::*;
use crate::syntax::loader::{LanguageLoader, LoadedLanguage};
use crate::syntax::Syntax;
use std::path::PathBuf;
use std::sync::mpsc;
use tree_sitter::Query;

type Highlights = Vec<(std::ops::Range<usize>, u32)>;

fn make_signal() -> CancellationSignal {
    CancellationSignal::new(false)
}

fn rust_syntax() -> Syntax {
    let loader = LanguageLoader::new(PathBuf::from("."));
    let grammar = loader.load_language("rust").expect("rust grammar");
    let query_src = loader
        .load_query("rust", "highlights")
        .expect("rust highlights query");
    let query = Query::new(&grammar.language, &query_src).expect("compile highlights query");
    Syntax::new(
        LoadedLanguage::bundled(grammar.language, "rust"),
        Some(Arc::new(query)),
    )
}

fn rust_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("set rust language");
    parser
}

fn make_buffer(src: &str) -> TextBuffer {
    let mut buffer = TextBuffer::new(src.len().max(16)).unwrap();
    crate::document::ops::Edit::type_text(src).apply_to(&mut buffer);
    buffer
}

fn run_job(job: SyntaxParseJob) -> SyntaxParseResult {
    let (tx, rx) = mpsc::channel();
    Box::new(job).run(1, tx, make_signal());

    *crate::job_manager::jobs::test_support::recv_custom_payload::<SyntaxParseResult>(&rx)
        .expect("SyntaxParseJob did not produce a SyntaxParseResult")
}

/// Reparses `buffer` on the background job path and applies the result.
fn job_reparse(syntax: &mut Syntax, buffer: &TextBuffer) {
    let job = SyntaxParseJob::new(
        buffer.clone(),
        rust_parser(),
        syntax.tree.clone(),
        "rust".to_string(),
        crate::document::DocumentId::new(1),
        buffer.revision(),
    );
    let result = run_job(job);
    assert!(result.tree.is_some());
    syntax.update_from_result(result, buffer);
}

fn all_highlights(syntax: &mut Syntax, buffer: &TextBuffer) -> Highlights {
    syntax.highlights(buffer, 0..buffer.byte_len())
}

fn fresh_highlights(buffer: &TextBuffer) -> Highlights {
    let mut syntax = rust_syntax();
    assert!(syntax.incremental_parse(buffer));
    all_highlights(&mut syntax, buffer)
}

fn point_at(source: &[u8], byte: usize) -> (usize, usize) {
    let byte = byte.min(source.len());
    let row = source[..byte].split(|&b| b == b'\n').count() - 1;
    let last_nl = source[..byte]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    (row, byte - last_nl)
}

/// Inserts `text` at char `pos` and tells `syntax`, as `document::edit` does.
fn insert(syntax: &mut Syntax, buffer: &mut TextBuffer, pos: usize, text: &str) {
    let old_source = buffer.to_logical_bytes();
    let start_byte = buffer.char_to_byte(pos);
    assert!(crate::document::ops::standalone::place_cursor(buffer, pos));
    crate::document::ops::Edit::type_text(text).apply_to(buffer);
    let new_end_byte = start_byte + text.len();
    let new_source = buffer.to_logical_bytes();

    let start_point = point_at(&old_source, start_byte);
    syntax.notify_edit(
        start_byte,
        start_byte,
        new_end_byte,
        start_point,
        start_point,
        point_at(&new_source, new_end_byte),
    );
}

fn captures_at(highlights: &Highlights, range: &std::ops::Range<usize>) -> Vec<u32> {
    highlights
        .iter()
        .filter(|(r, _)| r == range)
        .map(|(_, c)| *c)
        .collect()
}

/// A job reparse after one edit must match a from-scratch parse of the
/// same edited content.
#[test]
fn test_job_highlights_match_fresh_parse_after_single_edit() {
    let initial_src = "fn foo() { let a = 1; let b = 2; }";
    let mut buffer = make_buffer(initial_src);
    let mut syntax = rust_syntax();
    job_reparse(&mut syntax, &buffer);
    all_highlights(&mut syntax, &buffer);

    // Insert one character ("1" -> "11") inside the first literal.
    let insert_pos = initial_src.find("1;").unwrap() + 1;
    insert(&mut syntax, &mut buffer, insert_pos, "1");
    job_reparse(&mut syntax, &buffer);

    assert_eq!(
        all_highlights(&mut syntax, &buffer),
        fresh_highlights(&buffer)
    );
}

/// Editing inside a string literal must not change the capture (color)
/// of an unrelated `Ok(...)` constructor call later on the same line.
#[test]
fn test_job_highlights_do_not_recolor_unrelated_token_on_same_line() {
    let initial_src = "fn f(s: &str) -> Result<Action, ()> {\n    match s {\n        \"lsp:diagnostics_panel\" => Ok(Action::Editor(EditorAction::LspDiagnosticsPanel)),\n        _ => Ok(Action::Noop),\n    }\n}\n";
    let mut buffer = make_buffer(initial_src);
    let mut syntax = rust_syntax();
    job_reparse(&mut syntax, &buffer);

    let ok_start = initial_src.find("Ok(Action").unwrap();
    let ok_range = ok_start..ok_start + 2;

    // Insert one character right after "lsp" inside the string literal.
    let insert_pos = initial_src.find("lsp:diagnostics_panel").unwrap() + 3;
    insert(&mut syntax, &mut buffer, insert_pos, "x");
    job_reparse(&mut syntax, &buffer);

    let after = all_highlights(&mut syntax, &buffer);
    let expected = fresh_highlights(&buffer);
    let shifted_ok_range = (ok_range.start + 1)..(ok_range.end + 1);

    assert_eq!(
        captures_at(&after, &shifted_ok_range),
        captures_at(&expected, &shifted_ok_range),
        "capture order for \"Ok\" diverged after an edit elsewhere on the line"
    );
    assert_eq!(
        after, expected,
        "highlights diverged from a fresh parse after editing inside the string"
    );
}

#[test]
fn test_job_highlights_real_file_diagnostics_panel_line() {
    let initial_src = include_str!("../../action/mod.rs");
    let mut buffer = make_buffer(initial_src);
    let mut syntax = rust_syntax();
    job_reparse(&mut syntax, &buffer);
    all_highlights(&mut syntax, &buffer);

    let insert_pos = initial_src.find("lsp:diagnostics_panel").unwrap() + 3;
    insert(&mut syntax, &mut buffer, insert_pos, "x");
    job_reparse(&mut syntax, &buffer);

    assert_eq!(
        all_highlights(&mut syntax, &buffer),
        fresh_highlights(&buffer),
        "highlights diverged from a fresh parse on the real file"
    );
}

/// Simulates fast typing (each keystroke's job chains onto the last).
/// A subtle invalidation bug can compound into flickering without ever failing a single-edit test.
#[test]
fn test_job_highlights_stay_correct_across_many_sequential_edits() {
    let mut buffer = make_buffer("fn foo() {\n}\n");
    let mut syntax = rust_syntax();
    job_reparse(&mut syntax, &buffer);

    // Type this, one character at a time, right before the closing brace.
    let to_type = "    let x = 12345;\n    let y = x + 1;\n";
    let start = buffer.len() - "}\n".len();

    for (insert_pos, ch) in (start..).zip(to_type.chars()) {
        all_highlights(&mut syntax, &buffer);
        insert(&mut syntax, &mut buffer, insert_pos, &ch.to_string());
        job_reparse(&mut syntax, &buffer);

        assert_eq!(
            all_highlights(&mut syntax, &buffer),
            fresh_highlights(&buffer),
            "highlights diverged from a fresh parse after typing {ch:?}"
        );
    }
}
