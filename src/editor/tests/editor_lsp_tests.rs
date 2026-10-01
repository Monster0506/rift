use super::common::*;

#[cfg(feature = "lsp")]
#[test]
fn closing_a_document_sends_lsp_did_close() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("closed_doc.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    let other_path = dir.path().join("other.rs");
    std::fs::write(&other_path, "fn other() {}\n").unwrap();

    let mut editor = create_editor();
    editor.lsp_manager.register_server(
        "rust".to_string(),
        crate::lsp::config::LspServerConfig {
            command: "ping".to_string(),
            args: vec!["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
            extensions: vec![],
            root_markers: vec![],
            capabilities: vec![],
            initialization_options: None,
            keep_alive: false,
        },
    );

    editor
        .open_file(Some(other_path.display().to_string()), false)
        .unwrap();
    drain_jobs(&mut editor);
    editor
        .open_file(Some(path.display().to_string()), false)
        .unwrap();
    drain_jobs(&mut editor);

    assert!(
        editor.lsp_manager.is_tracking(&path),
        "opening the file should start LSP tracking"
    );

    editor.do_quit(false);

    assert!(
        !editor.lsp_manager.is_tracking(&path),
        "closing the buffer must send textDocument/didClose"
    );

    editor.lsp_manager.shutdown_all();
}

#[cfg(feature = "lsp")]
fn lsp_edit_json(sl: u32, sc: u32, el: u32, ec: u32, text: &str) -> serde_json::Value {
    serde_json::json!({
        "range": { "start": { "line": sl, "character": sc }, "end": { "line": el, "character": ec } },
        "newText": text,
    })
}

#[cfg(feature = "lsp")]
#[test]
fn workspace_edit_opens_unopened_file_in_background_and_applies_edits() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("other.rs");
    std::fs::write(&other, "fn old() {}\n").unwrap();

    let mut editor = create_editor();
    let active_before = editor.document_manager.active_document_id().unwrap();
    let uri = crate::lsp::protocol::path_to_uri(&other);
    let edit = serde_json::json!({
        "documentChanges": [
            { "textDocument": { "uri": uri, "version": null },
              "edits": [lsp_edit_json(0, 3, 0, 6, "new")] }
        ]
    });

    assert_eq!(editor.apply_workspace_edit(&edit), 1);

    assert_eq!(
        editor.document_manager.active_document_id(),
        Some(active_before),
        "a background edit must not switch the active tab"
    );
    let doc_id = editor
        .document_manager
        .find_open_document_id(&other)
        .expect("edited file is now open as a tab");
    let doc = editor.document_manager.get_document(doc_id).unwrap();
    assert_eq!(doc.buffer.to_string(), "fn new() {}\n");
    assert!(doc.is_dirty(), "the edit is applied in memory, not written");
    assert_eq!(
        std::fs::read_to_string(&other).unwrap(),
        "fn old() {}\n",
        "unsupported file operations are reported, never applied to disk"
    );
}

#[cfg(feature = "lsp")]
#[test]
fn workspace_edit_keeps_same_position_insert_order_and_restores_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("order.rs");
    std::fs::write(&path, "abc\ndef\n").unwrap();

    let mut editor = create_editor();
    editor
        .open_file(Some(path.display().to_string()), false)
        .unwrap();
    drain_jobs(&mut editor);
    let _ = editor.active_document().buffer.set_cursor(6);

    let uri = crate::lsp::protocol::path_to_uri(&path);
    let edit = serde_json::json!({ "changes": { uri: [
        lsp_edit_json(0, 0, 0, 0, "1"),
        lsp_edit_json(0, 0, 0, 0, "2"),
        lsp_edit_json(1, 0, 1, 3, "xyzw"),
    ] } });
    assert_eq!(editor.apply_workspace_edit(&edit), 1);

    let doc = editor.active_document();
    assert_eq!(doc.buffer.to_string(), "12abc\nxyzw\n");
    assert_eq!(doc.buffer.get_line(), 1, "cursor stays on its line");
    assert_eq!(
        doc.buffer.cursor() - doc.buffer.line_index.get_line_start(1),
        2,
        "and its column"
    );
}

#[cfg(feature = "lsp")]
#[test]
fn workspace_edit_with_a_malformed_edit_skips_that_document_entirely() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.rs");
    std::fs::write(&path, "abc\n").unwrap();

    let mut editor = create_editor();
    editor
        .open_file(Some(path.display().to_string()), false)
        .unwrap();
    drain_jobs(&mut editor);

    let uri = crate::lsp::protocol::path_to_uri(&path);
    let edit = serde_json::json!({ "changes": { uri: [
        lsp_edit_json(0, 0, 0, 0, "ok"),
        { "range": { "start": { "line": 0 } }, "newText": "broken" },
    ] } });
    assert_eq!(editor.apply_workspace_edit(&edit), 0);
    assert_eq!(editor.active_document().buffer.to_string(), "abc\n");
}

#[cfg(feature = "lsp")]
#[test]
fn goto_with_multiple_definitions_opens_a_location_list() {
    use crate::lsp::protocol::{LspLocation, LspPosition, LspRange};
    let loc = |line: u32| LspLocation {
        uri: String::new(),
        range: LspRange {
            start: LspPosition { line, character: 0 },
            end: LspPosition { line, character: 1 },
        },
    };
    let mut editor = create_editor();
    editor.handle_goto_result(vec![loc(1), loc(5)]);

    let layout = editor
        .panel_layout_of(super::PanelKind::LocationList)
        .expect("ambiguous definitions open a picker instead of jumping blindly");
    let doc = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap();
    assert_eq!(doc.buffer.get_total_lines(), 2);
}

#[cfg(feature = "lsp")]
#[test]
fn undo_forces_a_full_lsp_resync_instead_of_a_bogus_incremental_delta() {
    use crate::lsp::protocol::PositionEncoding;
    let mut editor = create_editor();
    let doc = editor.active_document();
    doc.insert_str("abc").unwrap();
    assert!(doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .is_some());
    assert!(!doc.has_pending_lsp_edits());

    assert!(doc.undo());
    assert!(doc.has_pending_lsp_edits(), "undo changed the buffer");
    assert!(
        doc.take_incremental_lsp_changes(PositionEncoding::Utf16)
            .is_none(),
        "undo bypasses edit recording, so only a full sync is safe"
    );
    assert!(
        !doc.has_pending_lsp_edits(),
        "the drain marks the buffer synced"
    );
}

#[cfg(feature = "lsp")]
#[test]
fn diagnostic_jump_lands_on_the_diagnostic_column() {
    use crate::lsp::protocol::{LspDiagnostic, LspPosition, LspRange};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("diag.rs");
    std::fs::write(&path, "let x = 1;\nlet y = 2;\n").unwrap();

    let mut editor = create_editor();
    editor
        .open_file(Some(path.display().to_string()), false)
        .unwrap();
    drain_jobs(&mut editor);

    let diag = |line: u32, character: u32| LspDiagnostic {
        range: LspRange {
            start: LspPosition { line, character },
            end: LspPosition {
                line,
                character: character + 1,
            },
        },
        severity: Some(1),
        message: "boom".to_string(),
        source: None,
    };
    let key = crate::lsp::protocol::normalize_uri(&crate::lsp::protocol::path_to_uri(&path));
    editor
        .lsp_diagnostics
        .insert(key, vec![diag(1, 4), diag(0, 8)]);

    editor.lsp_diagnostic_next();
    let doc = editor.active_document();
    assert_eq!(
        (
            doc.buffer.get_line(),
            doc.buffer.cursor() - doc.buffer.line_index.get_line_start(0)
        ),
        (0, 8)
    );

    editor.lsp_diagnostic_next();
    let doc = editor.active_document();
    assert_eq!(
        (
            doc.buffer.get_line(),
            doc.buffer.cursor() - doc.buffer.line_index.get_line_start(1)
        ),
        (1, 4)
    );
}

#[cfg(feature = "lsp")]
#[test]
fn published_diagnostic_underlines_exactly_its_range_and_keeps_the_eol_message() {
    use crate::lsp::protocol::{LspDiagnostic, LspPosition, LspRange};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("under.rs");
    std::fs::write(&path, "let \u{1F980}x = 1;\nok\n").unwrap();

    let mut editor = create_editor_sized(6, 40);
    editor.state.settings.show_line_numbers = false;
    editor
        .open_file(Some(path.display().to_string()), false)
        .unwrap();
    drain_jobs(&mut editor);

    let uri = crate::lsp::protocol::path_to_uri(&path);
    editor.handle_lsp_message(crate::lsp::LspMessage::Diagnostics {
        uri,
        version: None,
        diagnostics: vec![LspDiagnostic {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 6,
                },
                end: LspPosition {
                    line: 0,
                    character: 11,
                },
            },
            severity: Some(1),
            message: "bad".to_string(),
            source: None,
        }],
    });

    editor.update_and_render().unwrap();
    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    let row: Vec<char> = (0..cols).map(|c| cells[c].to_char()).collect();
    let underlined: Vec<usize> = (0..cols).filter(|&c| cells[c].attrs.underline).collect();
    assert_eq!(row[6..11].iter().collect::<String>(), "x = 1");
    assert_eq!(
        underlined,
        vec![6, 7, 8, 9, 10],
        "only the diagnosed span is underlined"
    );
    assert_eq!(row[11..16].iter().collect::<String>(), "; bad");

    let doc = editor.active_document();
    doc.buffer.set_cursor(0).ok();
    doc.insert_str("  ").unwrap();
    editor.update_and_render().unwrap();
    let cells = editor.render_system.compositor.get_composited_slice();
    let underlined: Vec<usize> = (0..cols).filter(|&c| cells[c].attrs.underline).collect();
    assert_eq!(underlined, vec![8, 9, 10, 11, 12]);
}
