use super::common::*;
use super::*;

#[test]
fn treesitter_object_without_parse_tree_silently_no_ops() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(a, b)");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('f'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo(a, b)");
    assert!(editor.state.error_manager.notifications().is_empty());
}

#[test]
fn surround_delete_strips_enclosing_parens() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "foobarbaz");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn surround_delete_via_bracket_alias() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('b'));

    assert_eq!(editor.active_document().buffer.to_string(), "foobarbaz");
}

#[test]
fn surround_change_swaps_parens_for_quotes() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('c'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo\"bar\"baz");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn surround_change_pads_opening_bracket_char() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('c'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('{'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo{ bar }baz");
}

#[test]
fn surround_add_wraps_inner_word_with_padding() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar baz");
    editor.active_document().buffer.set_cursor(4).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    assert_eq!(editor.pending_surround_add, Some(1));

    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('w'));
    assert_eq!(editor.pending_surround_add, None);

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "foo ( bar ) baz"
    );
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn surround_add_sgg_wraps_current_line() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\nfoo");
    editor.active_document().buffer.set_cursor(2).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"hello world\"\nfoo"
    );
}

#[test]
fn surround_delete_dot_repeat_reresolves_at_new_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(1).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));
    assert_eq!(editor.active_document().buffer.to_string(), "a (b)");

    editor.active_document().buffer.set_cursor(3).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(editor.active_document().buffer.to_string(), "a b");
}

#[test]
fn surround_escape_cancels_pending_grammar() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "foo(bar)baz");
    editor.active_document().buffer.set_cursor(5).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    assert!(editor.pending_grammar.is_some());

    editor.set_mode(Mode::Normal);
    editor.pending_count = 0;
    editor.pending_grammar = None;
    editor.pending_surround_add = None;

    assert!(editor.pending_grammar.is_none());
    let grammar = editor.pending_grammar.take();
    assert!(grammar.is_none());
    assert_eq!(editor.active_document().buffer.to_string(), "foo(bar)baz");
    let _ = Key::Char('j');
}

#[test]
fn surround_delete_count_removes_doubled_delimiter() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "((text))");
    editor.active_document().buffer.set_cursor(3).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "text");
}

#[test]
fn surround_change_count_doubles_both_sides() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "\"\"text\"\"");
    editor.active_document().buffer.set_cursor(3).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('c'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "( ( text ) )");
}

#[test]
fn surround_add_count_doubles_delimiter_for_sgg() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "line");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(editor.active_document().buffer.to_string(), "\"\"line\"\"");
}

#[test]
fn surround_add_outer_and_inner_counts_compose_for_sgg() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;

    let mut editor = create_editor();
    load_text(&mut editor, "line\nline");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"\"line\nline\"\""
    );
}

#[test]
fn surround_interrupted_sg_does_not_corrupt_later_yank() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    assert!(editor.pending_surround_add.is_some());
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.pending_surround_add, None);

    editor.set_mode(Mode::Normal);
    editor.pending_operator = None;

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('w'));

    assert_eq!(editor.active_document().buffer.to_string(), "foo bar");
    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.pending_grammar.is_none());
    assert_eq!(ring_text(&editor, 0), Some("foo".to_string()));
}

#[test]
fn surround_give_line_without_pending_add_cancels_like_unrecognized_key() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar");

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    editor.handle_action(&Action::Editor(EditorAction::SurroundGiveLine));

    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.pending_operator.is_none());
    assert!(editor.pending_grammar.is_none());
}
