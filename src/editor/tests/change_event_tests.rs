use super::common::*;
use super::*;
use crate::document::ops::{Edit, EditSource};
use crate::plugin::events::EditorEvent;
use crate::plugin::PluginMutation;
use crate::test_utils::MockTerminal;
use std::sync::{Arc, Mutex};

fn record(editor: &mut Editor<MockTerminal>, name: &'static str) -> Arc<Mutex<Vec<EditorEvent>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    editor
        .plugin_host
        .on(name, move |event| sink.lock().unwrap().push(event.clone()));
    seen
}

fn settled(text: &str) -> Editor<MockTerminal> {
    let mut editor = create_editor_sized(24, 80);
    load_text(&mut editor, text);
    editor.update_and_render().unwrap();
    editor
}

#[test]
fn ex_undo_and_redo_raise_text_changed() {
    let mut editor = settled("abc");
    editor
        .active_document()
        .edit(Edit::type_text("x"), EditSource::User)
        .unwrap();
    editor.update_and_render().unwrap();
    let seen = record(&mut editor, "TextChangedCoarse");

    editor.execute_command_line("undo");
    editor.update_and_render().unwrap();
    editor.execute_command_line("redo");
    editor.update_and_render().unwrap();

    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn plugin_edits_and_moves_raise_text_changed_and_cursor_moved() {
    let mut editor = settled("one\ntwo");
    let buf = editor.active_document().id;
    let texts = record(&mut editor, "TextChangedCoarse");
    let moves = record(&mut editor, "CursorMoved");

    editor
        .plugin_host
        .queue_mutation(PluginMutation::AppendLines {
            buf,
            lines: vec!["three".to_string()],
        });
    editor.apply_plugin_mutations();
    editor.update_and_render().unwrap();
    assert_eq!(texts.lock().unwrap().len(), 1);

    moves.lock().unwrap().clear();
    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetCursor {
            buf,
            row: 1,
            col: 0,
        });
    editor.apply_plugin_mutations();
    editor.update_and_render().unwrap();
    assert!(matches!(
        moves.lock().unwrap().as_slice(),
        [EditorEvent::CursorMoved { row: 0, col: 0, .. }]
    ));
}

#[test]
fn search_jumps_raise_cursor_moved() {
    let mut editor = settled("hello world");
    let moves = record(&mut editor, "CursorMoved");

    feed_keys(&mut editor, "/world<CR>");
    editor.update_and_render().unwrap();

    assert!(matches!(
        moves.lock().unwrap().as_slice(),
        [EditorEvent::CursorMoved { row: 0, col: 6, .. }]
    ));
}

#[test]
fn two_buffers_changed_in_one_frame_each_raise_text_changed() {
    let mut editor = settled("first");
    let first = editor.active_document().id;
    let mut second = crate::document::Document::new(editor.document_manager.next_id()).unwrap();
    let second_id = second.id;
    second
        .edit(Edit::type_text("second"), EditSource::User)
        .unwrap();
    editor.document_manager.add_document(second);
    editor.update_and_render().unwrap();
    let seen = record(&mut editor, "TextChangedCoarse");

    for id in [first, second_id] {
        editor
            .document_manager
            .get_document_mut(id)
            .unwrap()
            .edit(Edit::insert(0, "!"), EditSource::User)
            .unwrap();
    }
    editor.update_and_render().unwrap();

    let bufs: Vec<_> = seen
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            EditorEvent::TextChangedCoarse { buf } => Some(*buf),
            _ => None,
        })
        .collect();
    assert_eq!(bufs, vec![first, second_id]);
}

#[test]
fn banking_a_visual_selection_raises_selection_changed() {
    let mut editor = settled("hello world");
    let seen = record(&mut editor, "SelectionChanged");

    feed_keys(&mut editor, "vl<Esc>");
    editor.update_and_render().unwrap();

    assert!(!seen.lock().unwrap().is_empty());
}
