use super::common::*;
use crate::plugin::PluginMutation;

/// A `BufSavePre` hook that closes the buffer being saved (e.g. "close scratch
/// buffers before save") must not cause `do_save` to write some *other*
/// document's content to the original file path. Regression test for a bug
/// where `do_save` re-fetched "whichever document is active now" after
/// running plugin mutations instead of the document it started saving.
#[test]
fn save_does_not_write_another_buffers_content_when_a_bufsavepre_hook_closes_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc1.txt");

    let mut editor = create_editor();

    let doc1_id = editor.active_document().id;
    editor.active_document().set_path(&path);
    load_text(&mut editor, "doc one content");

    let doc2 = crate::document::Document::new_scratch(
        editor.document_manager.next_id(),
        "doc2".to_string(),
        &["doc two content".to_string()],
    )
    .unwrap();
    editor.document_manager.add_document(doc2);
    let _ = editor.document_manager.switch_to_document(doc1_id);
    assert_eq!(editor.active_document().id, doc1_id);

    // Simulate a BufSavePre Lua hook that closes the buffer being saved.
    editor
        .plugin_host
        .queue_mutation(PluginMutation::CloseBuffer { force: true });

    editor.do_save();
    drain_jobs(&mut editor);

    if let Ok(written) = std::fs::read_to_string(&path) {
        assert!(
            !written.contains("doc two content"),
            "do_save wrote doc2's content to doc1's path after a BufSavePre hook closed doc1: {written:?}"
        );
    }
}
