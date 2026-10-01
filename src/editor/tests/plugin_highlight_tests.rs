use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

#[test]
fn plugin_add_highlight_shifts_with_a_real_edit_before_it() {
    use crate::annotations::{well_known, Anchor};
    use crate::plugin::PluginMutation;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    editor
        .plugin_host
        .queue_mutation(PluginMutation::AddHighlight {
            slot: 1,
            start_line: 1,
            start_col: 6,
            end_line: 1,
            end_col: 11,
            color: "red".to_string(),
        });
    editor.apply_plugin_mutations();

    let anchor_of = |editor: &mut Editor<MockTerminal>| {
        let doc = editor.active_document();
        let a = doc
            .annotations
            .query_kind(well_known::PLUGIN_HIGHLIGHT)
            .next()
            .expect("plugin highlight annotation")
            .anchor;
        let Anchor::Range(s, e) = a else {
            panic!("expected a range anchor")
        };
        (s.offset, e.offset)
    };
    assert_eq!(anchor_of(&mut editor), (6, 11));

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.active_document().insert_str("XX").unwrap();

    assert_eq!(
        anchor_of(&mut editor),
        (8, 13),
        "plugin highlight must shift with a real edit before it"
    );
}

#[test]
fn plugin_add_highlight_survives_a_ghost_cut_paint_cycle() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};
    use crate::annotations::{well_known, Anchor};
    use crate::plugin::PluginMutation;

    let mut editor = create_editor();
    load_text(&mut editor, "abc def ghi\n");

    editor
        .plugin_host
        .queue_mutation(PluginMutation::AddHighlight {
            slot: 1,
            start_line: 1,
            start_col: 8,
            end_line: 1,
            end_col: 11,
            color: "red".to_string(),
        });
    editor.apply_plugin_mutations();

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_count = 4;
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    editor.update_and_render().unwrap();

    let doc = editor.active_document();
    let a = doc
        .annotations
        .query_kind(well_known::PLUGIN_HIGHLIGHT)
        .next()
        .expect("plugin highlight annotation")
        .anchor;
    let Anchor::Range(s, e) = a else {
        panic!("expected a range anchor")
    };
    assert_eq!(
        (s.offset, e.offset),
        (4, 7),
        "plugin highlight must track the real delete, not go stale through a ghost paint"
    );
}

#[test]
fn plugin_clear_highlights_respects_slot_scoping() {
    use crate::annotations::well_known;
    use crate::plugin::PluginMutation;

    let mut editor = create_editor();
    load_text(&mut editor, "aaaa bbbb\n");

    editor
        .plugin_host
        .queue_mutation(PluginMutation::AddHighlight {
            slot: 1,
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 4,
            color: "red".to_string(),
        });
    editor
        .plugin_host
        .queue_mutation(PluginMutation::AddHighlight {
            slot: 2,
            start_line: 1,
            start_col: 5,
            end_line: 1,
            end_col: 9,
            color: "blue".to_string(),
        });
    editor.apply_plugin_mutations();
    assert_eq!(
        editor
            .active_document()
            .annotations
            .query_kind(well_known::PLUGIN_HIGHLIGHT)
            .count(),
        2
    );

    editor
        .plugin_host
        .queue_mutation(PluginMutation::ClearHighlights { slot: 1 });
    editor.apply_plugin_mutations();
    assert_eq!(
        editor
            .active_document()
            .annotations
            .query_kind(well_known::PLUGIN_HIGHLIGHT)
            .count(),
        1,
        "clearing one slot must not touch another"
    );

    editor
        .plugin_host
        .queue_mutation(PluginMutation::ClearHighlights { slot: 0 });
    editor.apply_plugin_mutations();
    assert_eq!(
        editor
            .active_document()
            .annotations
            .query_kind(well_known::PLUGIN_HIGHLIGHT)
            .count(),
        0,
        "slot 0 clears every plugin highlight"
    );
}
