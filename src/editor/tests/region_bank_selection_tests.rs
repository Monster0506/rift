use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

#[test]
fn visual_char_enters_mode_and_anchors_at_cursor() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(3).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    assert_eq!(editor.current_mode, Mode::Visual);
    assert_eq!(editor.visual_anchor, Some(3));
}

#[test]
fn visual_resumes_a_banked_region_under_the_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 0, RangeKind::Charwise));

    editor.active_document().buffer.set_cursor(2).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    assert_eq!(editor.current_mode, Mode::Visual);
    assert_eq!(
        editor.visual_anchor,
        Some(4),
        "anchor side must be restored"
    );
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "cursor side must be restored"
    );
    assert!(
        editor.active_document().selection_set.regions.is_empty(),
        "resumed region must be popped out of the banked set"
    );
}

#[test]
fn visual_motion_extends_through_normal_fallthrough() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    assert_eq!(
        editor.current_mode,
        Mode::Visual,
        "motion must not exit Visual"
    );
    assert_eq!(editor.active_document().buffer.cursor(), 2);
    assert_eq!(
        editor.visual_anchor,
        Some(0),
        "anchor stays fixed while cursor moves"
    );
}

#[test]
fn visual_swap_ends_exchanges_anchor_and_cursor() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));

    editor.handle_action(&Action::Editor(EditorAction::VisualSwapEnds));

    assert_eq!(editor.visual_anchor, Some(2));
    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn expand_region_grows_word_then_quotes_in_verified_order() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "say \"hello world\" now");
    let pos = editor
        .active_document()
        .buffer
        .to_string()
        .find("hello")
        .unwrap();
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let anchor = editor.visual_anchor.unwrap();
    let cursor = editor.active_document().buffer.cursor();
    assert_eq!(
        (anchor, cursor),
        (5, 10),
        "first press: Word around -> \"hello \""
    );

    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let anchor = editor.visual_anchor.unwrap();
    let cursor = editor.active_document().buffer.cursor();
    assert_eq!(
        (anchor, cursor),
        (4, 16),
        "second press: DoubleQuote around -> the full quoted span"
    );
}

#[test]
fn expand_region_noop_when_already_at_buffer_extent() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "x");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let before = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let after = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );

    assert_eq!(
        before, after,
        "expanding past the whole buffer must be a no-op"
    );
}

#[test]
fn shrink_region_pops_the_last_expand_step() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "say \"hello world\" now");
    let pos = editor
        .active_document()
        .buffer
        .to_string()
        .find("hello")
        .unwrap();
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    let before_expand = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let after_expand = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );
    assert_ne!(
        before_expand, after_expand,
        "expand must have actually grown the region"
    );

    editor.handle_action(&Action::Editor(EditorAction::ShrinkRegion));
    let after_shrink = (
        editor.visual_anchor,
        editor.active_document().buffer.cursor(),
    );

    assert_eq!(
        after_shrink, before_expand,
        "shrink must restore the exact pre-expand extent"
    );
}

#[test]
fn shrink_region_with_empty_history_is_a_noop() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    let handled = editor.handle_action(&Action::Editor(EditorAction::ShrinkRegion));

    assert!(!handled);
}

#[test]
fn escape_in_visual_commits_active_region() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.visual_anchor.is_none());
    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
    assert_eq!(
        editor.active_document().selection_set.regions[0].span(),
        (0, 3)
    );
}

#[test]
fn escape_in_normal_clears_a_nonempty_banked_set() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_delete_surround_handles_two_regions_sharing_one_enclosing_pair() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "(a(b)c)");
    {
        let doc = editor.active_document();
        doc.selection_set
            .bank(Region::new(5, 5, RangeKind::Charwise));
        doc.selection_set
            .bank(Region::new(1, 1, RangeKind::Charwise));
    }

    assert!(editor.try_run_set_aware_delete_surround('(', 1));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "a(b)c",
        "must strip only the shared outer pair, leaving the inner \"(b)\" intact"
    );
}

#[test]
fn issue_worked_example_bank_two_regions_no_delete_yet() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::buffer::api::BufferView;

    let mut editor = create_editor();
    load_text(&mut editor, "Hello\nworld\nfoo\n");

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
    assert_eq!(
        editor.active_document().selection_set.regions[0].span(),
        (0, 2)
    );

    let line3_start = editor.active_document().buffer.line_start(2);
    let _ = editor.active_document().buffer.set_cursor(line3_start);
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        2,
        "first region must survive the plain motion to line 3"
    );
    let spans: Vec<(usize, usize)> = editor
        .active_document()
        .selection_set
        .sorted()
        .iter()
        .map(|r| r.span())
        .collect();
    assert_eq!(spans[0], (0, 2), "\"Ho\" region");
    assert_eq!(spans[1].1 - spans[1].0, 1, "\"f\" region is one char");
}

#[test]
fn n_cycles_banked_regions_when_set_is_nonempty() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789abcdefghij");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(10, 11, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 10);

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "wraps to first"
    );
}

#[test]
fn shift_n_cycles_backward_when_set_is_nonempty() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789abcdefghij");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(10, 11, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(15).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindBackward,
    )));
    assert_eq!(editor.active_document().buffer.cursor(), 10);
}

#[test]
fn n_keeps_repeat_find_behavior_when_set_is_empty() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.state.last_find_char = Some(('o', true, false));

    editor.handle_action(&Action::Editor(EditorAction::Move(
        Motion::RepeatFindForward,
    )));

    assert_eq!(editor.active_document().buffer.cursor(), 1);
}

#[test]
fn region_bank_occurrence_next_finds_and_moves_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz foo");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert_eq!(editor.active_document().selection_set.regions.len(), 2);
    assert_eq!(editor.active_document().buffer.cursor(), 8);
}

#[test]
fn region_bank_occurrence_on_empty_set_is_a_noop() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");

    let handled = editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert!(!handled);
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn region_bank_occurrence_disabled_for_blockwise_last_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Blockwise));

    let handled = editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert!(!handled);
    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
}

#[test]
fn gv_toggle_opens_and_closes_regardless_of_focus() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    assert!(editor.panel_layout.is_some(), "gv opens the window");
    assert_eq!(
        editor.active_document().kind.kind_str(),
        "regions",
        "focus moves into the new regions window"
    );

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    assert!(editor.panel_layout.is_none(), "a second gv closes it again");
}

#[test]
fn gv_with_empty_set_does_not_open_a_window() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    assert!(editor.panel_layout.is_none());
}

#[test]
fn regions_window_x_drops_the_selected_entry() {
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
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    editor.handle_action(&Action::Editor(EditorAction::RegionsListDrop));

    let source_id = editor
        .active_document()
        .regions_source_doc_id()
        .expect("expected to still be focused in the regions window");
    assert_eq!(
        editor
            .document_manager
            .get_document(source_id)
            .unwrap()
            .selection_set
            .regions
            .len(),
        1,
        "one entry dropped from the *source* document's set"
    );
}

#[test]
fn regions_window_j_moves_the_list_cursor_and_live_jumps_the_preview() {
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
    let source_id = editor.active_document_id();
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    let list_cursor_before = editor.active_document().buffer.cursor();

    editor.handle_action(&Action::Editor(EditorAction::RegionsListDown));

    assert_ne!(
        editor.active_document().buffer.cursor(),
        list_cursor_before,
        "j must move the regions list's own cursor to line 2, not stay on line 1"
    );
    assert_eq!(
        editor
            .document_manager
            .get_document(source_id)
            .unwrap()
            .buffer
            .cursor(),
        5,
        "and live-jump the source buffer to the second region (sorted order: 0..1, then 5..6)"
    );
}

#[test]
fn regions_window_operator_redirects_to_the_source_document() {
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
    let source_id = editor.active_document_id();
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert!(
        editor.panel_layout.is_none(),
        "firing an operator from the window closes it"
    );

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(
        editor
            .document_manager
            .get_document(source_id)
            .unwrap()
            .buffer
            .to_string(),
        "234789"
    );
}

#[test]
fn visual_block_renders_and_edits_identically_to_charwise() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualBlock));
    editor.handle_action(&Action::Editor(EditorAction::Move(
        crate::action::Motion::Right,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "23456789",
        "Ctrl-V behaves exactly like v -- no rectangle semantics by design"
    );
}

#[test]
fn real_keymap_v_then_l_renders_a_visible_highlight_in_the_composited_cells() {
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    let feed_key = |editor: &mut Editor<MockTerminal>, key: Key| {
        let context = if editor.current_mode.is_visual() {
            KeyContext::Visual
        } else {
            KeyContext::Normal
        };
        match editor.keymap.lookup(context, std::slice::from_ref(&key)) {
            MatchResult::Exact(action) | MatchResult::Ambiguous(action) => {
                let action = action.clone();
                editor.handle_action(&action);
            }
            other => panic!("key {key:?} in context {context:?} did not resolve: {other:?}"),
        }
    };

    feed_key(&mut editor, Key::Char('v'));
    feed_key(&mut editor, Key::Char('l'));

    editor.update_and_render().unwrap();

    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    let highlighted = cells[..cols]
        .iter()
        .filter(|c| {
            matches!(
                c.bg,
                Some(crate::color::Color::Rgb {
                    r: 100,
                    g: 160,
                    b: 220
                })
            )
        })
        .count();
    assert_eq!(
        highlighted, 2,
        "v then l must highlight exactly the 2 selected chars 'h','e'"
    );
}

#[test]
fn visual_highlight_redraws_on_a_frame_after_the_initial_one() {
    use crate::key::Key;
    use crate::keymap::{KeyContext, MatchResult};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.update_and_render().unwrap();

    let feed_key = |editor: &mut Editor<MockTerminal>, key: Key| {
        let context = if editor.current_mode.is_visual() {
            KeyContext::Visual
        } else {
            KeyContext::Normal
        };
        match editor.keymap.lookup(context, std::slice::from_ref(&key)) {
            MatchResult::Exact(action) | MatchResult::Ambiguous(action) => {
                let action = action.clone();
                editor.handle_action(&action);
            }
            other => panic!("key {key:?} did not resolve: {other:?}"),
        }
    };

    feed_key(&mut editor, Key::Char('v'));
    feed_key(&mut editor, Key::Char('l'));

    editor.update_and_render().unwrap();

    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    let highlighted = cells[..cols]
        .iter()
        .filter(|c| {
            matches!(
                c.bg,
                Some(crate::color::Color::Rgb {
                    r: 100,
                    g: 160,
                    b: 220
                })
            )
        })
        .count();
    assert_eq!(
        highlighted, 2,
        "selection highlight must redraw on a later frame, not just the first ever render"
    );
}
