use super::common::*;
#[cfg(feature = "treesitter")]
use super::*;
use crate::document::ops::{Edit, EditSource};
#[cfg(feature = "treesitter")]
use crate::test_utils::MockTerminal;

#[cfg(feature = "treesitter")]
#[test]
fn stale_syntax_job_result_does_not_clobber_newer_state() {
    use crate::syntax::build_syntax;
    use std::sync::Arc;
    use std::time::Duration;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\n");

    let loader = editor.language_loader.clone();
    let grammar = loader.load_language("rust").expect("rust grammar");
    let highlights = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&grammar.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(grammar, highlights, &loader);
    editor.active_document().set_syntax(syntax);
    editor.reparse_syntax(editor.active_document_id());

    let doc_id = editor.active_document_id();

    editor
        .spawn_syntax_parse_job(doc_id)
        .expect("job should spawn");

    {
        let doc = editor.active_document();
        doc.buffer.move_to_end();
        doc.buffer.insert_str("\nfn extra() {}\n").unwrap();
    }

    let doc = editor.active_document();
    let syntax = doc.syntax.as_mut().unwrap();
    syntax.invalidate_trees();
    assert!(syntax.incremental_parse(&doc.buffer));
    let fresh_highlights = syntax.highlights(&doc.buffer, 0..doc.buffer.byte_len());
    assert_ne!(fresh_highlights, Vec::new());

    let mut drained = false;
    while let Ok(msg) = editor
        .job_manager
        .receiver()
        .recv_timeout(Duration::from_millis(200))
    {
        editor.handle_job_message(msg);
        drained = true;
    }
    assert!(drained, "expected the background job's result to arrive");

    let after_highlights = document_highlights(&mut editor);
    assert_eq!(
        after_highlights, fresh_highlights,
        "a stale background parse result must not overwrite newer sync-parsed state"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn undo_keeps_syntax_tree_for_incremental_reuse() {
    use crate::action::{Action, EditorAction};
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\n");

    let loader = editor.language_loader.clone();
    let grammar = loader.load_language("rust").expect("rust grammar");
    let highlights = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&grammar.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(grammar, highlights, &loader);
    editor.active_document().set_syntax(syntax);
    editor.reparse_syntax(editor.active_document_id());

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineEnd));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar(';')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert!(editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .tree
        .is_some());

    assert!(editor.active_document().undo());
    assert!(
        editor
            .active_document()
            .syntax
            .as_ref()
            .unwrap()
            .tree
            .is_some(),
        "undo must not invalidate the syntax tree"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn set_aware_delete_keeps_syntax_highlights_in_sync() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::buffer::api::BufferView;
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\nfn extra() {}\n");

    let loader = editor.language_loader.clone();
    let grammar = loader.load_language("rust").expect("rust grammar");
    let highlights_query = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&grammar.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(grammar, highlights_query, &loader);
    editor.active_document().set_syntax(syntax);
    editor.reparse_syntax(editor.active_document_id());

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualLine));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    let second_line_start = editor.active_document().buffer.line_start(1);
    editor
        .active_document()
        .buffer
        .set_cursor(second_line_start)
        .unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualLine));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert!(!editor.active_document().pending_ghost.is_empty());
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "");

    let after_set_aware_delete = document_highlights(&mut editor);
    let fresh_highlights = fresh_highlights(&mut editor);

    assert_eq!(
        after_set_aware_delete, fresh_highlights,
        "set-aware delete must trigger a sync reparse, not leave the tree stale"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn ghost_paint_keeps_syntax_highlights_in_sync_while_painted() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\nfn extra() {}\n");

    let loader = editor.language_loader.clone();
    let grammar = loader.load_language("rust").expect("rust grammar");
    let highlights_query = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&grammar.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(grammar, highlights_query, &loader);
    editor.active_document().set_syntax(syntax);
    editor.reparse_syntax(editor.active_document_id());

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));
    assert!(!editor.active_document().pending_ghost.is_empty());

    let doc_id = editor.active_document_id();
    editor
        .document_manager
        .get_document_mut(doc_id)
        .unwrap()
        .begin_ghost_paint();

    let painted_highlights = document_highlights(&mut editor);
    let fresh_highlights = fresh_highlights(&mut editor);

    assert_eq!(
        painted_highlights, fresh_highlights,
        "ghost paint must trigger a real reparse, not just shift stale node ranges"
    );

    editor
        .document_manager
        .get_document_mut(doc_id)
        .unwrap()
        .end_ghost_paint();
}

#[cfg(feature = "treesitter")]
type Highlights = Vec<(std::ops::Range<usize>, u32)>;

#[cfg(feature = "treesitter")]
fn rust_syntax(editor: &Editor<MockTerminal>) -> crate::syntax::Syntax {
    use std::sync::Arc;

    let loader = editor.language_loader.clone();
    let grammar = loader.load_language("rust").expect("rust grammar");
    let highlights = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&grammar.language, &src).ok())
        .map(Arc::new);
    crate::syntax::build_syntax(grammar, highlights, &loader)
}

#[cfg(feature = "treesitter")]
fn load_rust(text: &str) -> Editor<MockTerminal> {
    let mut editor = create_editor();
    load_text(&mut editor, text);
    let syntax = rust_syntax(&editor);
    editor.active_document().set_syntax(syntax);
    settle_syntax(&mut editor);
    editor
}

/// Finishes any parse the sync budget deferred to a background job.
#[cfg(feature = "treesitter")]
fn settle_syntax(editor: &mut Editor<MockTerminal>) {
    let doc = editor.active_document();
    assert!(doc.syntax.as_mut().unwrap().incremental_parse(&doc.buffer));
}

#[cfg(feature = "treesitter")]
fn document_highlights(editor: &mut Editor<MockTerminal>) -> Highlights {
    let doc = editor.active_document();
    let len = doc.buffer.byte_len();
    doc.syntax.as_mut().unwrap().highlights(&doc.buffer, 0..len)
}

#[cfg(feature = "treesitter")]
fn fresh_highlights(editor: &mut Editor<MockTerminal>) -> Highlights {
    let mut syntax = rust_syntax(editor);
    let buffer = &editor.active_document().buffer;
    assert!(syntax.incremental_parse(buffer));
    syntax.highlights(buffer, 0..buffer.byte_len())
}

/// One whole-document query, no chunks: the reference for clipped captures.
#[cfg(feature = "treesitter")]
fn unchunked_highlights(editor: &mut Editor<MockTerminal>) -> Highlights {
    use streaming_iterator::StreamingIterator;

    let syntax = rust_syntax(editor);
    let query = syntax.highlights_query.expect("rust highlights query");
    let source = editor.active_document().buffer.to_logical_bytes();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&syntax.language).unwrap();
    let tree = parser.parse(&source, None).unwrap();

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_slice());
    let mut found = Vec::new();
    while let Some(m) = matches.next() {
        for capture in m.captures() {
            found.push((capture.node.byte_range(), capture.index, m.pattern_index));
        }
    }
    found.sort_by(|a, b| (a.0.start, a.0.end, b.2).cmp(&(b.0.start, b.0.end, a.2)));
    found.into_iter().map(|(r, c, _)| (r, c)).collect()
}

/// Capture the renderer paints at each byte: the first range covering it.
#[cfg(feature = "treesitter")]
fn paint(highlights: &Highlights, len: usize) -> Vec<Option<u32>> {
    (0..len)
        .map(|b| {
            highlights
                .iter()
                .find(|(r, _)| r.contains(&b))
                .map(|(_, c)| *c)
        })
        .collect()
}

/// Types `text` at `byte`, reparsing and repainting after every key.
#[cfg(feature = "treesitter")]
fn type_at(editor: &mut Editor<MockTerminal>, byte: usize, text: &str) {
    use crate::action::{Action, EditorAction};

    let pos = editor.active_document().buffer.byte_to_char(byte);
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    for ch in text.chars() {
        editor.handle_action(&Action::Editor(EditorAction::InsertChar(ch)));
        settle_syntax(editor);
        document_highlights(editor);
    }
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
}

/// Enough `let` lines to span several highlight chunks.
#[cfg(feature = "treesitter")]
fn let_lines(count: usize) -> String {
    use std::fmt::Write;

    (0..count).fold(String::new(), |mut out, i| {
        let _ = writeln!(out, "    let value_{i} = {i};");
        out
    })
}

#[cfg(feature = "treesitter")]
fn assert_recolors_before_edit(open: &str, close: &str) {
    let text = format!("fn main() {{\n    {open}\n{}}}\n", let_lines(400));
    let probe = text.find("value_100 ").unwrap();
    let edit = text.find("    let value_350 ").unwrap();
    let mut editor = load_rust(&text);
    let before = document_highlights(&mut editor);

    type_at(&mut editor, edit, close);

    let after = document_highlights(&mut editor);
    assert_eq!(after, fresh_highlights(&mut editor));
    assert_ne!(
        paint(&before, text.len())[probe],
        paint(&after, text.len())[probe],
        "closing {open:?} must recolor a chunk before the edit"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn typing_comment_close_recolors_earlier_chunks() {
    assert_recolors_before_edit("/* open", "*/");
}

#[cfg(feature = "treesitter")]
#[test]
fn typing_closing_quote_recolors_earlier_chunks() {
    assert_recolors_before_edit("let s = \"open", "\";");
}

#[cfg(feature = "treesitter")]
#[test]
fn captures_spanning_chunk_boundaries_match_unchunked_query() {
    use std::fmt::Write;

    let comment = (0..800).fold(String::new(), |mut out, i| {
        let _ = writeln!(out, "   comment line {i}");
        out
    });
    let text = format!("fn main() {{}}\n/*\n{comment}*/\nfn after() {{ let x = 1; }}\n");
    let mut editor = load_rust(&text);

    let inside = text.find("comment line 500").unwrap();
    let window = document_highlights_in(&mut editor, inside..inside + 10);
    assert_eq!(
        paint(&window, inside + 10)[inside..],
        paint(&unchunked_highlights(&mut editor), inside + 10)[inside..],
        "a window deep inside a multi-chunk comment must still paint it"
    );

    let all = document_highlights(&mut editor);
    assert_eq!(
        paint(&all, text.len()),
        paint(&unchunked_highlights(&mut editor), text.len())
    );

    type_at(&mut editor, text.find("comment line 700").unwrap(), "x");
    let len = editor.active_document().buffer.byte_len();
    assert_eq!(
        document_highlights(&mut editor),
        fresh_highlights(&mut editor)
    );
    assert_eq!(
        paint(&document_highlights(&mut editor), len),
        paint(&unchunked_highlights(&mut editor), len)
    );
}

#[cfg(feature = "treesitter")]
fn document_highlights_in(
    editor: &mut Editor<MockTerminal>,
    range: std::ops::Range<usize>,
) -> Highlights {
    let doc = editor.active_document();
    doc.syntax.as_mut().unwrap().highlights(&doc.buffer, range)
}

#[cfg(feature = "treesitter")]
#[test]
fn append_invalidates_rendered_eof() {
    let text = "fn main() {}";
    let mut editor = load_rust(text);
    document_highlights(&mut editor);
    let doc = editor.active_document();
    let generation = doc.syntax.as_ref().unwrap().highlights_generation();
    doc.buffer.move_to_end();
    doc.edit(Edit::type_text(' '), EditSource::User).unwrap();
    assert_ne!(
        generation,
        doc.syntax.as_ref().unwrap().highlights_generation(),
        "appending must invalidate the rendered EOF chunk before a deferred parse"
    );

    // Rendering can happen before the deferred parse completes.
    document_highlights(&mut editor);
    settle_syntax(&mut editor);
    assert_eq!(
        document_highlights(&mut editor),
        fresh_highlights(&mut editor)
    );
}
