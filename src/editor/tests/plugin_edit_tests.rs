use super::common::*;
use crate::document::DocumentId;
use crate::plugin::PluginMutation;

fn apply(text: &str, mutation: impl FnOnce(DocumentId) -> PluginMutation) -> (String, usize) {
    let mut editor = create_editor();
    load_text(&mut editor, text);
    let buf = editor.active_document().id;
    editor.plugin_host.queue_mutation(mutation(buf));
    editor.apply_plugin_mutations();
    let doc = editor.active_document();
    (doc.buffer.to_string(), doc.buffer.cursor())
}

fn replace_lines(
    start: usize,
    end: usize,
    lines: &[&str],
) -> impl FnOnce(DocumentId) -> PluginMutation {
    let lines = lines.iter().map(ToString::to_string).collect();
    move |buf| PluginMutation::ReplaceLines {
        buf,
        start,
        end,
        lines,
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
fn plugin_line_writes_leave_read_only_buffers_unchanged() {
    for mutation in [
        Box::new(|buf| PluginMutation::AppendLines {
            buf,
            lines: vec!["four".to_string()],
        }) as Box<dyn FnOnce(DocumentId) -> PluginMutation>,
        Box::new(replace_lines(2, 2, &["TWO"])),
    ] {
        let mut editor = create_editor();
        load_text(&mut editor, "one\ntwo\nthree");
        editor.active_document().set_read_only(true);
        let buf = editor.active_document().id;
        editor.plugin_host.queue_mutation(mutation(buf));
        editor.apply_plugin_mutations();
        assert_eq!(
            editor.active_document().buffer.to_string(),
            "one\ntwo\nthree"
        );
    }
}

#[test]
fn set_cursor_clamps_the_column_to_its_line() {
    let (_, cursor) = apply("one\ntwo\nthree", |buf| PluginMutation::SetCursor {
        buf,
        row: 1,
        col: 50,
    });
    assert_eq!(cursor, 3);

    let (_, cursor) = apply("one\ntwo\nthree", |buf| PluginMutation::SetCursor {
        buf,
        row: 9,
        col: 2,
    });
    assert_eq!(cursor, 10);
}

#[test]
fn writes_queued_for_a_closed_buffer_do_not_reach_the_active_one() {
    let (text, _) = apply("one", |_| PluginMutation::InsertAtCursor {
        buf: DocumentId::new(9999),
        text: "stray ".to_string(),
    });
    assert_eq!(text, "one");
}
