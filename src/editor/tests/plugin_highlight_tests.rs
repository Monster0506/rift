use super::common::*;
use super::*;
use crate::annotations::kind::well_known;
use crate::annotations::Anchor;
use crate::document::ops::{Edit, EditSource};
use crate::plugin::{HighlightRange, HighlightSpec, PluginMutation};
use crate::test_utils::MockTerminal;

fn highlight_row0(editor: &mut Editor<MockTerminal>, ns: &str, cols: std::ops::Range<usize>) {
    highlight_row0_owned(editor, ns, None, cols);
}

fn highlight_row0_owned(
    editor: &mut Editor<MockTerminal>,
    ns: &str,
    owner: Option<&str>,
    cols: std::ops::Range<usize>,
) {
    let buf = editor.active_document().id;
    editor
        .plugin_host
        .queue_mutation(PluginMutation::AddNsHighlight {
            buf,
            ns: ns.to_string(),
            owner: owner.map(str::to_owned),
            range: HighlightRange::Points {
                start: (0, cols.start),
                end: (0, cols.end),
            },
            hl: HighlightSpec::Group("error".to_string()),
            priority: 0,
        });
}

fn highlight_ranges(editor: &mut Editor<MockTerminal>) -> Vec<(usize, usize)> {
    editor
        .active_document()
        .annotations
        .query_kind(well_known::PLUGIN_HIGHLIGHT)
        .map(|a| {
            let Anchor::Range(s, e) = a.anchor else {
                panic!("expected a range anchor")
            };
            (s.offset, e.offset)
        })
        .collect()
}

#[test]
fn plugin_highlight_shifts_with_a_real_edit_before_it() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");

    highlight_row0(&mut editor, "test", 6..11);
    editor.apply_plugin_mutations();
    assert_eq!(highlight_ranges(&mut editor), vec![(6, 11)]);

    editor
        .active_document()
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();
    editor
        .active_document()
        .edit(Edit::type_text("XX"), EditSource::User)
        .unwrap();

    assert_eq!(
        highlight_ranges(&mut editor),
        vec![(8, 13)],
        "plugin highlight must shift with a real edit before it"
    );
}

#[test]
fn plugin_highlight_survives_a_ghost_cut_paint_cycle() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abc def ghi\n");

    highlight_row0(&mut editor, "test", 8..11);
    editor.apply_plugin_mutations();

    editor
        .active_document()
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.input.count = 4;
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    editor.update_and_render().unwrap();

    assert_eq!(
        highlight_ranges(&mut editor),
        vec![(4, 7)],
        "plugin highlight must track the real delete, not go stale through a ghost paint"
    );
}

#[test]
fn clearing_a_highlight_namespace_leaves_other_namespaces() {
    let mut editor = create_editor();
    load_text(&mut editor, "aaaa bbbb\n");

    highlight_row0(&mut editor, "one", 0..4);
    highlight_row0(&mut editor, "two", 5..9);
    editor.apply_plugin_mutations();
    assert_eq!(highlight_ranges(&mut editor).len(), 2);

    editor
        .plugin_host
        .queue_mutation(PluginMutation::ClearNsHighlights {
            buf: None,
            ns: "one".to_string(),
            owner: None,
        });
    editor.apply_plugin_mutations();
    assert_eq!(
        highlight_ranges(&mut editor),
        vec![(5, 9)],
        "clearing one namespace must not touch another"
    );
}

#[test]
fn point_highlights_clamp_columns_to_the_line() {
    let mut editor = create_editor();
    load_text(&mut editor, "ab\ncd\n");

    highlight_row0(&mut editor, "test", 1..99);
    editor.apply_plugin_mutations();

    assert_eq!(
        highlight_ranges(&mut editor),
        vec![(1, 2)],
        "a column past the line end stops at the line end, not in the next line"
    );
}

#[test]
fn clearing_an_owned_namespace_keeps_other_owners_highlights() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");
    highlight_row0_owned(&mut editor, "marks", Some("a.lua"), 0..5);
    highlight_row0_owned(&mut editor, "marks", Some("b.lua"), 6..11);
    highlight_row0(&mut editor, "marks", 2..4);
    editor.apply_plugin_mutations();
    assert_eq!(highlight_ranges(&mut editor).len(), 3);

    editor
        .plugin_host
        .queue_mutation(PluginMutation::ClearNsHighlights {
            buf: None,
            ns: "marks".to_string(),
            owner: Some("a.lua".to_string()),
        });
    editor.apply_plugin_mutations();
    let mut ranges = highlight_ranges(&mut editor);
    ranges.sort_unstable();
    assert_eq!(ranges, vec![(2, 4), (6, 11)]);
}
