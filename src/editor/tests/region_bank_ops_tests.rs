use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

#[test]
fn apply_to_each_region_runs_f_once_per_region_highest_offset_first() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    let mut seen_starts = Vec::new();
    let handled = editor.apply_to_each_region(|_editor, region| {
        seen_starts.push(region.span().0);
        true
    });

    assert!(handled);
    assert_eq!(seen_starts, vec![5, 0], "highest-offset-first");
    assert!(
        editor.active_document().selection_set.is_empty(),
        "batch must clear the set"
    );
}

#[test]
fn apply_to_each_region_on_empty_set_returns_false() {
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");

    let handled = editor.apply_to_each_region(|_editor, _region| true);

    assert!(!handled);
}

#[test]
fn apply_to_each_region_deletes_are_one_undo_step() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.apply_to_each_region(|editor, region| {
        let (start, end) = region.span();
        if let Some(doc) = editor.document_manager.active_document_mut() {
            doc.delete_range(start, end).is_ok()
        } else {
            false
        }
    });
    assert_eq!(editor.active_document().buffer.to_string(), "234789");

    assert!(editor.active_document().undo());
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0123456789",
        "a single undo must restore both deletions at once"
    );
}

#[test]
fn enter_multi_insert_replays_typed_session_at_every_remaining_anchor() {
    use crate::action::{Action, EditorAction};
    use crate::command::Command;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    let handled =
        editor.enter_multi_insert(Command::EnterInsertMode, |_doc, region| region.span().0);
    assert!(handled);
    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(
        editor.active_document().buffer.cursor(),
        5,
        "starts at the highest-offset anchor"
    );

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "X01234X56789",
        "X inserted at both original anchors: live at 5, replayed at 0"
    );
    assert!(editor.pending_multi_insert_anchors.is_empty());
}

#[test]
fn enter_multi_insert_on_empty_set_returns_false() {
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");

    let handled = editor
        .enter_multi_insert(crate::command::Command::EnterInsertMode, |_doc, region| {
            region.span().0
        });

    assert!(!handled);
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn set_aware_delete_removes_every_banked_region_as_one_op() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 7, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));
    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        3,
        "touching must not have merged"
    );

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "\n\n\n");
    assert!(
        editor.active_document().selection_set.is_empty(),
        "set clears after the batch"
    );

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "\n\n\n");
}

#[test]
fn set_aware_delete_is_one_undo_step() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.active_document().buffer.to_string(), "234789");

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "234789");

    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), "0123456789");
}

#[test]
fn set_aware_yank_captures_each_region_without_mutating() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar baz");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "foo bar baz",
        "yank must not mutate"
    );
    assert!(editor.active_document().selection_set.is_empty());
    assert_eq!(
        ring_text(&editor, 0),
        Some("foo".to_string()),
        "lowest-offset region pushed last = ring[0] (front-insert)"
    );
    assert_eq!(ring_text(&editor, 1), Some("baz".to_string()));
}

#[test]
fn visual_d_commits_active_region_then_runs_the_batch() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.current_mode, Mode::Normal);

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "234789",
        "both the just-committed and pre-banked region deleted as one batch"
    );
}

#[test]
fn plain_d_with_empty_set_is_unaffected() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(
        editor.current_mode,
        Mode::OperatorPending,
        "falls through to today's single-cursor flow"
    );
}

#[test]
fn canonical_change_across_touching_regions_does_not_merge_them() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 7, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));
    assert_eq!(editor.active_document().selection_set.regions.len(), 3);

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Change,
    )));
    assert_eq!(editor.current_mode, Mode::Insert);

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('b')));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('a')));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('r')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "bar\n\nbarbar\n",
        "each touching region gets its own independent 'bar', not one merged replacement"
    );
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn single_region_change_unaffected_by_the_new_batching_branch() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Change,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::NextWord)));
    assert_eq!(
        editor.current_mode,
        Mode::Insert,
        "ordinary single-cursor cw must still work"
    );

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "Xbar");
}

#[test]
fn multi_i_inserts_at_start_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    assert_eq!(editor.current_mode, Mode::Insert);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "X01234X56789");
}

#[test]
fn multi_a_inserts_after_end_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAfter));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "0X12345X6789");
}

#[test]
fn multi_capital_i_inserts_at_line_start_of_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(9, 9, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineStart));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "aaa\nXbbb\nXccc"
    );
}

#[test]
fn multi_capital_a_inserts_at_line_end_of_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 4, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 8, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineEnd));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "aaa\nbbbX\ncccX"
    );
}

#[test]
fn multi_o_opens_a_new_line_below_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::OpenLineBelow));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "aaa\nX\nbbb\nX\nccc"
    );
}

#[test]
fn multi_capital_o_opens_a_new_line_above_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::OpenLineAbove));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "X\naaa\nX\nbbb\nccc"
    );
}

#[test]
fn plain_i_with_empty_set_is_unaffected() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello");
    editor.active_document().buffer.set_cursor(2).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(
        editor.active_document().buffer.cursor(),
        2,
        "ordinary i must still anchor at the live cursor"
    );
}

#[test]
fn set_aware_replace_char_fills_each_region_to_its_own_length() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 8, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::ReplaceCharPending));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('x'));

    assert_eq!(editor.active_document().buffer.to_string(), "xx234xxxx9");
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_sd_strips_surrounding_parens_from_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(1, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "a b");
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_sg_wraps_each_region_independently() {
    use crate::action::Motion;
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 7, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(8, 10, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::AddSurroundChar {
        motion: Motion::NextWord,
        count: 1,
        delim_count: 1,
    });
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"foo\"\n\n\"foo\"\"foo\"\n"
    );
}

#[test]
fn set_aware_put_inserts_same_text_at_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0X12345X6789",
        "p inserts after each region"
    );
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_put_before_inserts_ahead_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: true }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "X01234X56789",
        "P inserts before each region"
    );
}

#[test]
fn bare_repeated_p_after_set_aware_put_only_affects_single_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    let count_of_x = editor
        .active_document()
        .buffer
        .to_string()
        .matches('X')
        .count();
    assert_eq!(
        count_of_x, 3,
        "first put = 2 X's (one per region), second bare put = 1 more, not 2 more"
    );
}

#[test]
fn multi_insert_is_one_undo_step() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "X01234X56789");

    assert!(editor.active_document().undo());
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0123456789",
        "a single undo must remove both inserted X's at once"
    );
}

#[test]
fn region_build_actions_accumulate_while_visual_and_during_bank_occurrence() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert_eq!(
        editor.region_build_recording,
        vec![
            Action::Editor(EditorAction::EnterVisualChar),
            Action::Editor(EditorAction::Move(crate::action::Motion::Right)),
            Action::Editor(EditorAction::Move(crate::action::Motion::Right)),
            Action::Editor(EditorAction::EnterNormalMode),
            Action::Editor(EditorAction::RegionBankOccurrenceNext),
        ]
    );
}

#[test]
fn region_build_recording_does_not_capture_plain_normal_mode_navigation() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "line one\nline two\nline three");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));

    assert_eq!(
        editor.region_build_recording,
        vec![
            Action::Editor(EditorAction::EnterVisualChar),
            Action::Editor(EditorAction::EnterNormalMode),
        ],
        "plain Normal-mode Move actions must not be recorded"
    );
}

#[test]
fn multi_region_put_is_one_undo_step() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    assert_eq!(editor.active_document().buffer.to_string(), "0X12345X6789");

    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), "0123456789");
}

#[test]
fn dot_repeat_destructive_group_reselects_without_reexecuting() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert!(editor.active_document().selection_set.is_empty());

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), " (b)");

    editor.active_document().buffer.set_cursor(1).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        " (b)",
        "destructive group: '.' must NOT re-delete"
    );
    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        1,
        "but the equivalent region must be rebanked for manual review"
    );
}

#[test]
fn dot_repeat_leading_count_overrides_embedded_command_count() {
    use crate::action::Motion;
    use crate::command::Command;

    let mut editor = create_editor();
    load_text(&mut editor, "one two three four five six seven");
    editor.active_document().buffer.set_cursor(0).unwrap();

    let d2w = Command::Delete(Motion::NextWord, 2);
    editor.execute_buffer_command(d2w);
    editor.dot_repeat.record_single(d2w);
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "three four five six seven"
    );

    editor.pending_count = 3;
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "six seven",
        "3. after d2w must delete 3 words once, not 2 words three times"
    );
}

#[test]
fn dot_repeat_non_destructive_group_rebuilds_and_reexecutes() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineStart));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "Xaaa\nbbb");

    let bbb_offset = editor
        .active_document()
        .buffer
        .to_string()
        .find('b')
        .unwrap();
    editor
        .active_document()
        .buffer
        .set_cursor(bbb_offset)
        .unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "Xaaa\nXbbb",
        "non-destructive group: '.' rebuilds AND re-runs the insert"
    );
}

#[test]
fn dot_repeat_sg_fully_replays_using_addsurroundtoset() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo\nbar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, crate::key::Key::Char('g'));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::AddSurroundChar {
        motion: Motion::NextWord,
        count: 1,
        delim_count: 1,
    });
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, crate::key::Key::Char('"'));

    assert_eq!(editor.active_document().buffer.to_string(), "\"foo\"\nbar");

    let bar_offset = editor
        .active_document()
        .buffer
        .to_string()
        .find("bar")
        .unwrap();
    editor
        .active_document()
        .buffer
        .set_cursor(bar_offset)
        .unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"foo\"\n\"bar\"",
        "'.' rebuilds the equivalent region at the new cursor AND re-wraps it -- sg fully replays, unlike d/c/y/sd/sc"
    );
}

#[test]
fn issue_worked_example_full_sequence_including_delete() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "Hello\nworld\nfoo\n");

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    let line3_start = editor.active_document().buffer.line_start(2);
    let _ = editor.active_document().buffer.set_cursor(line3_start);
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert!(editor.active_document().selection_set.is_empty());

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "llo\nworld\noo\n"
    );
}

#[test]
fn undo_of_unrelated_edit_clears_a_banked_set() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().insert_char('!').unwrap();
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    assert!(!editor.active_document().selection_set.is_empty());

    assert!(editor.active_document().undo());

    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn every_set_aware_command_clears_the_set_after_acting() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let fresh_set = |editor: &mut Editor<MockTerminal>| {
        editor
            .active_document()
            .selection_set
            .bank(Region::new(0, 0, RangeKind::Charwise));
        editor
            .active_document()
            .selection_set
            .bank(Region::new(4, 4, RangeKind::Charwise));
    };

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "d must clear the set"
    );

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "y must clear the set"
    );

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "i must clear the set"
    );

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::OpenLineBelow));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "o must clear the set"
    );

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("X".to_string());
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    assert!(
        editor.active_document().selection_set.is_empty(),
        "p must clear the set"
    );
}

#[test]
fn dot_repeat_yank_reselects_without_reexecuting() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    let buffer_before = editor.active_document().buffer.to_string();
    assert!(editor.active_document().selection_set.is_empty());

    editor.active_document().buffer.set_cursor(5).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        buffer_before,
        "yank's dot-repeat must not mutate the buffer"
    );
    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        1,
        "but must rebank the equivalent region"
    );
}

#[test]
fn dot_repeat_paste_genuinely_differs_from_bare_repeat() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz foo");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    editor.clipboard_ring.push_str("X".to_string());
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    let _ = editor.active_document().buffer.set_cursor(0);
    editor.execute_dot_repeat();
    let _ = editor.active_document().buffer.set_cursor(0);
    editor.execute_dot_repeat();

    let count_of_x = editor
        .active_document()
        .buffer
        .to_string()
        .matches('X')
        .count();
    assert_eq!(
        count_of_x, 6,
        "three dot-repeats x two original anchors = 6, not stacked at one spot"
    );
}

#[test]
fn cycle_paste_after_set_clears_only_touches_the_single_most_recent_position() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push_str("Y".to_string());
    editor.clipboard_ring.push_str("X".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 0, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    editor.handle_action(&Action::Editor(EditorAction::CyclePaste { forward: true }));

    let count_of_y = editor
        .active_document()
        .buffer
        .to_string()
        .matches('Y')
        .count();
    assert_eq!(
        count_of_y, 0,
        "multi-region put never sets post_paste_state, so CyclePaste correctly no-ops"
    );
}
