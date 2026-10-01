use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

#[test]
fn test_g_no_count_goes_to_last_line() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\n");

    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    let last_line = doc.buffer.line_count() - 1;
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(last_line));
}

#[test]
fn test_g_with_count_jumps_to_line() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "alpha\nbeta\ngamma\ndelta\n");

    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(2));
}

#[test]
fn test_g_count_beyond_last_line_clamps() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "one\ntwo\nthree\n");

    editor.pending_count = 999;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    let last_line = doc.buffer.line_count() - 1;
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(last_line));
}

#[test]
fn test_g_count_one_goes_to_first_line() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "first\nsecond\nthird\n");

    editor.pending_count = 0;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));
    editor.pending_count = 1;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(0));
}

#[test]
fn test_goto_line_explicit_n_no_count() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "a\nb\nc\nd\n");

    editor.handle_action(&Action::Editor(EditorAction::GotoLine(2)));

    let doc = editor.active_document();
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(1));
}

#[test]
fn test_f_find_char_forward_moves_to_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('o'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 4);
}

#[test]
fn test_f_find_char_forward_not_found_stays() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('z'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_f_find_char_backward_moves_to_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");
    editor.active_document().buffer.set_cursor(10).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharBackward('h'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_find_char_pending_sets_state() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::FindCharPending {
        forward: true,
        till: false,
    }));

    assert!(matches!(
        editor.pending_grammar,
        Some(super::pending_grammar::PendingGrammar::FindChar {
            forward: true,
            till: false
        })
    ));
}

#[test]
fn test_find_char_pending_backward_sets_state() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();

    editor.handle_action(&Action::Editor(EditorAction::FindCharPending {
        forward: false,
        till: false,
    }));

    assert!(matches!(
        editor.pending_grammar,
        Some(super::pending_grammar::PendingGrammar::FindChar {
            forward: false,
            till: false
        })
    ));
}

#[test]
fn test_f_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('f'));
    assert!(
        action.is_some(),
        "'f' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_shift_f_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('F'));
    assert!(
        action.is_some(),
        "'F' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_t_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('t'));
    assert!(
        action.is_some(),
        "'t' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_shift_t_keybinding_is_registered() {
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('T'));
    assert!(
        action.is_some(),
        "'T' should have a keybinding in Normal mode"
    );
}

#[test]
fn test_till_forward_stops_one_before_target() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('o'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);
}

#[test]
fn test_till_backward_stops_one_after_target() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");
    editor.active_document().buffer.set_cursor(10).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharBackward('o'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 8);
}

#[test]
fn test_till_records_last_find_and_repeats() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(
        &mut editor,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
    );

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('C'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 15);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 22);
}

#[test]
fn test_till_direction_not_flipped_by_repeat() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(
        &mut editor,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
    );

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('C'),
    )));
    let pos_before_clone = editor.active_document().buffer.cursor();
    assert_eq!(pos_before_clone, 15);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    let pos_before_copy = editor.active_document().buffer.cursor();
    assert_eq!(pos_before_copy, 22);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    let pos_after_clone = editor.active_document().buffer.cursor();
    assert_eq!(pos_after_clone, 17);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 17);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_before_copy);
}

#[test]
fn test_dt_on_char_not_found_does_nothing() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('a'),
    )));

    assert_eq!(editor.active_document().buffer.len(), 6);
    assert_eq!(editor.active_document().buffer.cursor(), 0);
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn test_dt_does_not_cross_line_boundary() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "bcdef\naXXa\n");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('a'),
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "bcdef\naXXa\n");
    assert_eq!(editor.active_document().buffer.cursor(), 0);
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn test_dtf_on_abcdef_leaves_f_only() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('f'),
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "f");
    assert_eq!(editor.active_document().pending_ghost.len(), 1);

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "f");
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn test_dta_with_second_a_leaves_only_that_a() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abca");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('a'),
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "a");
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "a");
}

#[test]
fn test_tf_move_still_stops_one_before_target() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::TillCharForward('f'),
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 4);
}

#[test]
fn test_dg_deletes_from_cursor_to_end_of_file() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\n");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    assert_eq!(editor.active_document().buffer.len(), 0);
    assert_eq!(editor.active_document().pending_ghost.len(), 1);

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.len(), 0);
}

#[test]
fn test_dg_with_count_deletes_to_specific_line() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\nline4\n");
    let lines_before = editor.active_document().buffer.get_total_lines();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    assert!(editor.active_document().buffer.get_total_lines() < lines_before);
    assert_eq!(editor.active_document().pending_ghost.len(), 1);

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    let doc = editor.active_document();
    assert!(!doc.buffer.is_empty());
    let remaining = doc.buffer.get_total_lines();
    assert!(remaining <= 3);
}

#[test]
fn test_g_outside_operator_pending_just_moves_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "line1\nline2\nline3\n");

    editor.handle_action(&Action::Editor(EditorAction::GotoLine(0)));

    let doc = editor.active_document();
    let last_line = doc.buffer.line_count() - 1;
    assert_eq!(doc.buffer.cursor(), doc.buffer.line_start(last_line));
    assert_eq!(doc.buffer.len(), 18);
}

#[test]
fn test_n_repeats_last_forward_find_in_same_direction() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "ababa\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);
}

#[test]
fn test_n_repeats_last_backward_find_in_same_direction() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "ababa\n");
    editor.active_document().buffer.set_cursor(4).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharBackward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);
}

#[test]
fn test_shift_n_repeats_find_in_opposite_direction() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "ababa\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('b'),
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 3);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 1);
}

#[test]
fn test_fn_direction_not_flipped_by_repeat() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(
        &mut editor,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
    );

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('C'),
    )));
    let pos_clone = editor.active_document().buffer.cursor();
    assert_eq!(pos_clone, 16, "fC: expected C of Clone at col 16");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    let pos_copy = editor.active_document().buffer.cursor();
    assert_eq!(pos_copy, 23, "n: expected C of Copy at col 23");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_clone);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_clone);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), pos_copy);
}

#[test]
fn test_n_falls_back_to_search_when_no_find_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world hello\n");
    editor.state.last_search_query = Some("hello".to_string());
    editor.state.last_find_char = None;

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 12);
}

#[test]
fn test_shift_n_falls_back_to_prev_search_when_no_find_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world hello\n");
    editor.active_document().buffer.set_cursor(12).unwrap();
    editor.state.last_search_query = Some("hello".to_string());
    editor.state.last_find_char = None;

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_search_clears_last_find_char() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world hello\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::FindCharForward('o'),
    )));
    assert!(editor.state.last_find_char.is_some());

    editor.state.command_line = "hello".to_string();
    editor.handle_mode_management(crate::command::Command::ExecuteSearch);

    assert!(editor.state.last_find_char.is_none());
}

#[test]
fn test_n_with_no_previous_find_stays_put() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn test_n_keybinding_maps_to_repeat_find_forward() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('n'));
    assert_eq!(
        action,
        Some(&Action::Editor(EditorAction::Move(
            Motion::RepeatFindForward
        ))),
        "'n' should map to RepeatFindForward"
    );
}

#[test]
fn test_shift_n_keybinding_maps_to_repeat_find_backward() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::key::Key;
    use crate::keymap::KeyContext;

    let editor = create_editor();
    let action = editor.keymap.get_action(KeyContext::Normal, Key::Char('N'));
    assert_eq!(
        action,
        Some(&Action::Editor(EditorAction::Move(
            Motion::RepeatFindBackward
        ))),
        "'N' should map to RepeatFindBackward"
    );
}

#[test]
fn leading_count_composes_with_nest_count_through_full_key_path() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::key::Key;
    use crate::text_objects::Modifier;

    let mut editor = create_editor();
    load_text(&mut editor, "((((ab))))");
    editor.active_document().buffer.set_cursor(4).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.current_mode, Mode::OperatorPending);
    assert_eq!(editor.pending_count, 2);

    editor.pending_grammar = Some(pending_grammar::PendingGrammar::TextObject(
        text_object_input::PendingTextObject::new(Modifier::Inner),
    ));

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('2'));
    assert!(editor.pending_grammar.is_some());

    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "()");
}

#[test]
fn dd_with_leading_count_deletes_n_lines() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "one\ntwo\nthree\nfour\n");

    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "four\n");
}

#[test]
fn dd_count_does_not_leak_into_next_motion() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "one\ntwo\nthree\nfour\nfive\n");

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.pending_count, 0, "count must not survive past dd");

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "three\nfour\nfive\n"
    );

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));
    assert_eq!(
        editor.active_document().buffer.get_line(),
        1,
        "a leaked count would have moved down 2 lines instead of 1"
    );
}

#[test]
fn x_with_leading_count_deletes_n_chars() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Delete(Motion::Right)));

    assert_eq!(editor.active_document().buffer.to_string(), "lo world");
}

#[test]
fn operator_count_and_motion_count_multiply_not_concatenate() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "one two three four five six seven eight");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.pending_count = 2;
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_operator_count = editor.pending_count.max(1);
    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::NextWord)));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "seven eight");
}

#[test]
fn dd_deletes_the_current_line_via_the_operator_doubling_path_not_a_keymap_sequence() {
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let mut editor = create_editor();
    load_text(&mut editor, "first\nsecond\nthird\n");

    let feed_key = |editor: &mut Editor<MockTerminal>, key: Key| match editor
        .keymap
        .lookup(KeyContext::Normal, std::slice::from_ref(&key))
    {
        MatchResult::Exact(action) | MatchResult::Ambiguous(action) => {
            let action = action.clone();
            editor.handle_action(&action);
        }
        other => panic!("key {key:?} did not resolve: {other:?}"),
    };

    feed_key(&mut editor, Key::Char('d'));
    feed_key(&mut editor, Key::Char('d'));

    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::EnterNormalMode,
    ));
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "second\nthird\n",
        "dd should delete the first line via the operator-doubling path"
    );
}

#[test]
fn ambiguous_non_operator_binding_flushes_to_the_short_action_after_timeout() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::buffer::api::BufferView;
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let mut editor = create_editor();
    load_text(&mut editor, "line one\nline two\nline three\n");

    editor.keymap.register(
        KeyContext::Normal,
        Key::Char('Q'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    editor.keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char('Q'), Key::Char('Q')],
        Action::Editor(EditorAction::Move(Motion::Right)),
    );

    assert_eq!(
        editor.keymap.lookup(KeyContext::Normal, &[Key::Char('Q')]),
        MatchResult::Ambiguous(&Action::Editor(EditorAction::Move(Motion::Down)))
    );

    editor.pending_keys.push(Key::Char('Q'));
    editor.pending_keys_started_at =
        Some(std::time::Instant::now() - std::time::Duration::from_millis(1500));

    editor.flush_pending_keys_on_timeout().unwrap();

    assert!(
        editor.pending_keys.is_empty(),
        "timeout flush should clear pending_keys"
    );
    assert!(
        editor.pending_keys_started_at.is_none(),
        "timeout flush should clear the pending-key timer"
    );
    assert_eq!(
        editor.active_document().buffer.cursor(),
        editor.active_document().buffer.line_start(1),
        "the short 'Q' action (Move Down) should have fired, not the longer sequence"
    );
}

#[test]
fn dollar_in_normal_mode_rests_on_the_last_character() {
    let mut editor = create_editor();
    load_text(&mut editor, "abc\n\nxyz");

    feed_keys(&mut editor, "$");
    assert_eq!(
        editor.active_document().buffer.cursor(),
        2,
        "$ lands on 'c', not the newline"
    );
    feed_keys(&mut editor, "a");
    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(editor.active_document().buffer.cursor(), 3);
    editor.execute_buffer_command(crate::command::Command::InsertChar('Q'));
    feed_keys(&mut editor, "<Esc>");
    assert_eq!(editor.active_document().buffer.to_string(), "abcQ\n\nxyz");

    feed_keys(&mut editor, "j$");
    assert_eq!(editor.active_document().buffer.cursor(), 5);

    feed_keys(&mut editor, "j0D");
    assert_eq!(editor.active_document().buffer.to_string(), "abcQ\n\n");
}
