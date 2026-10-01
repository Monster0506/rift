use super::common::*;
use super::*;

#[cfg(feature = "treesitter")]
#[test]
fn stale_syntax_job_result_does_not_clobber_newer_state() {
    use crate::syntax::build_syntax;
    use std::sync::Arc;
    use std::time::Duration;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\n");

    let loader = editor.language_loader.clone();
    let loaded = loader.load_language("rust").expect("rust grammar");
    let highlights = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(loaded, highlights, loader.clone()).expect("build_syntax");
    editor.active_document().set_syntax(syntax);
    editor.do_incremental_syntax_parse();

    let doc_id = editor.active_document_id();

    editor
        .spawn_syntax_parse_job(doc_id)
        .expect("job should spawn");

    {
        let doc = editor.active_document();
        doc.buffer.move_to_end();
        doc.buffer.insert_str("\nfn extra() {}\n").unwrap();
    }

    let source = editor.active_document().buffer.to_logical_bytes();
    let syntax = editor.active_document().syntax.as_mut().unwrap();
    syntax.invalidate_trees();
    assert!(syntax.incremental_parse(&source));
    let fresh_highlights = syntax.highlights(None);
    assert!(!fresh_highlights.is_empty());

    let mut drained = false;
    while let Ok(msg) = editor
        .job_manager
        .receiver()
        .recv_timeout(Duration::from_millis(200))
    {
        editor.handle_job_message(msg).unwrap();
        drained = true;
    }
    assert!(drained, "expected the background job's result to arrive");

    let after_highlights = editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .highlights(None);
    assert_eq!(
        after_highlights, fresh_highlights,
        "a stale background parse result must not overwrite newer sync-parsed state"
    );
}

#[cfg(feature = "treesitter")]
#[test]
fn undo_keeps_syntax_tree_for_incremental_reuse() {
    use crate::syntax::build_syntax;
    use std::sync::Arc;

    let mut editor = create_editor();
    load_text(&mut editor, "fn main() {}\n");

    let loader = editor.language_loader.clone();
    let loaded = loader.load_language("rust").expect("rust grammar");
    let highlights = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(loaded, highlights, loader.clone()).expect("build_syntax");
    editor.active_document().set_syntax(syntax);
    editor.do_incremental_syntax_parse();

    editor.do_incremental_syntax_parse();
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
    let loaded = loader.load_language("rust").expect("rust grammar");
    let highlights_query = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");
    editor.active_document().set_syntax(syntax);
    editor.do_incremental_syntax_parse();

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

    let after_set_aware_delete = editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .highlights(None);

    let source = editor.active_document().buffer.to_logical_bytes();
    let syntax = editor.active_document().syntax.as_mut().unwrap();
    syntax.invalidate_trees();
    assert!(syntax.incremental_parse(&source));
    let fresh_highlights = syntax.highlights(None);

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
    let loaded = loader.load_language("rust").expect("rust grammar");
    let highlights_query = loader
        .load_query("rust", "highlights")
        .ok()
        .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
        .map(Arc::new);
    let syntax = build_syntax(loaded, highlights_query, loader.clone()).expect("build_syntax");
    editor.active_document().set_syntax(syntax);
    editor.do_incremental_syntax_parse();

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
    editor.do_incremental_syntax_parse_for(doc_id);

    let painted_highlights = editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .highlights(None);

    let source = editor.active_document().buffer.to_logical_bytes();
    let syntax = editor.active_document().syntax.as_mut().unwrap();
    syntax.invalidate_trees();
    assert!(syntax.incremental_parse(&source));
    let fresh_highlights = syntax.highlights(None);

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
