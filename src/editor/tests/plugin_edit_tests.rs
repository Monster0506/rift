use super::common::*;
use crate::plugin::PluginMutation;

fn apply(text: &str, mutation: PluginMutation) -> (String, usize) {
    let mut editor = create_editor();
    load_text(&mut editor, text);
    editor.plugin_host.queue_mutation(mutation);
    editor.apply_plugin_mutations();
    let doc = editor.active_document();
    (doc.buffer.to_string(), doc.buffer.cursor())
}

fn replace_lines(start: usize, end: usize, lines: &[&str]) -> PluginMutation {
    PluginMutation::ReplaceLines {
        start,
        end,
        lines: lines.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn replace_lines_on_a_middle_line_keeps_the_following_line_break() {
    let (text, cursor) = apply("one\ntwo\nthree", replace_lines(2, 2, &["TWO"]));
    assert_eq!(text, "one\nTWO\nthree");
    assert_eq!(cursor, 7);
}

#[test]
fn replace_lines_on_the_last_line_adds_no_line_break() {
    let (text, cursor) = apply("one\ntwo\nthree", replace_lines(3, 3, &["THREE", "four"]));
    assert_eq!(text, "one\ntwo\nTHREE\nfour");
    assert_eq!(cursor, text.len());
}

#[test]
fn replace_lines_with_an_empty_list_deletes_whole_lines() {
    let (text, cursor) = apply("one\ntwo\nthree", replace_lines(2, 2, &[]));
    assert_eq!(text, "one\nthree");
    assert_eq!(cursor, 4);

    let (text, cursor) = apply("one\ntwo\nthree", replace_lines(3, 3, &[]));
    assert_eq!(text, "one\ntwo");
    assert_eq!(cursor, 7);
}

#[test]
fn set_cursor_clamps_the_column_to_its_line() {
    let (_, cursor) = apply(
        "one\ntwo\nthree",
        PluginMutation::SetCursor { row: 1, col: 50 },
    );
    assert_eq!(cursor, 3);

    let (_, cursor) = apply(
        "one\ntwo\nthree",
        PluginMutation::SetCursor { row: 9, col: 2 },
    );
    assert_eq!(cursor, 10);
}
