#[cfg(feature = "lsp")]
use super::common::*;
#[cfg(feature = "lsp")]
use crate::document::ops::{Edit, EditSource};

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

    editor.execute_command_line("q");

    assert!(
        !editor.lsp_manager.is_tracking(&path),
        "closing the buffer must send textDocument/didClose"
    );

    editor.lsp_manager.shutdown_all();
}

#[cfg(feature = "lsp")]
#[test]
fn failed_split_load_clears_its_pending_jump() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = create_editor();
    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::types::SplitSubcommand::File(
            dir.path().to_string_lossy().into_owned(),
        ),
    );
    let handle = editor.active_document().handle();
    assert!(editor.defer_file_load_goto(handle, 2, 0));

    loop {
        let message = editor
            .job_manager
            .receiver()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let failed = matches!(message, crate::job_manager::JobMessage::Error(..));
        editor.handle_job_message(message);
        if failed {
            break;
        }
    }

    assert!(editor.file_load_jobs.is_empty());
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
    let active_before = editor.active_document_id();
    let uri = crate::lsp::protocol::path_to_uri(&other);
    let edit = serde_json::json!({
        "documentChanges": [
            { "textDocument": { "uri": uri, "version": null },
              "edits": [lsp_edit_json(0, 3, 0, 6, "new")] }
        ]
    });

    assert_eq!(editor.apply_workspace_edit(&edit), 1);

    assert_eq!(
        editor.active_document_id(),
        active_before,
        "a background edit must not switch the active tab"
    );
    let doc_id = editor
        .document_manager
        .find_open_document_id(&other)
        .expect("edited file is now open as a tab");
    let doc = editor.document_manager.get_document(doc_id).unwrap();
    assert_eq!(doc.buffer().to_string(), "fn new() {}\n");
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
    let _ = editor
        .active_document()
        .set_cursor(6, crate::document::ops::CursorCause::User);

    let uri = crate::lsp::protocol::path_to_uri(&path);
    let edit = serde_json::json!({ "changes": { uri: [
        lsp_edit_json(0, 0, 0, 0, "1"),
        lsp_edit_json(0, 0, 0, 0, "2"),
        lsp_edit_json(1, 0, 1, 3, "xyzw"),
    ] } });
    assert_eq!(editor.apply_workspace_edit(&edit), 1);

    let doc = editor.active_document();
    assert_eq!(doc.buffer().to_string(), "12abc\nxyzw\n");
    assert_eq!(doc.buffer().get_line(), 1, "cursor stays on its line");
    assert_eq!(
        doc.buffer().cursor() - doc.buffer().line_index.get_line_start(1),
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
    assert_eq!(editor.active_document().buffer().to_string(), "abc\n");
}

#[cfg(feature = "lsp")]
#[test]
fn workspace_edits_skip_read_only_targets_and_apply_to_writable_ones() {
    let dir = tempfile::tempdir().unwrap();
    let protected = dir.path().join("protected.txt");
    let writable = dir.path().join("writable.txt");
    std::fs::write(&protected, "locked").unwrap();
    std::fs::write(&writable, "open").unwrap();
    let mut editor = create_editor();
    open_and_load(&mut editor, &protected);
    let protected_id = editor.active_document_id();
    editor.active_document().set_read_only(true);
    open_and_load(&mut editor, &writable);

    let protected_uri = crate::lsp::protocol::path_to_uri(&protected);
    let writable_uri = crate::lsp::protocol::path_to_uri(&writable);
    let edit = serde_json::json!({ "changes": {
        protected_uri: [lsp_edit_json(0, 0, 0, 6, "changed")],
        writable_uri: [lsp_edit_json(0, 0, 0, 4, "updated")]
    } });
    assert_eq!(editor.apply_workspace_edit(&edit), 1);
    let doc = editor.document_manager.get_document(protected_id).unwrap();
    assert_eq!(doc.buffer().to_string(), "locked");
    assert!(!doc.is_dirty());
    assert_eq!(editor.active_document().buffer().to_string(), "updated");
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
    editor.handle_goto_result(&[loc(1), loc(5)]);

    let layout = editor
        .panel_layout_of(super::PanelKind::LocationList)
        .expect("ambiguous definitions open a picker instead of jumping blindly");
    let doc = editor
        .document_manager
        .get_document(layout.dir_doc_id)
        .unwrap();
    assert_eq!(doc.buffer().get_total_lines(), 2);
}

#[cfg(all(feature = "lsp", feature = "plugins"))]
#[test]
fn rename_prompt_keeps_its_file_while_direct_rename_uses_current_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("symbol.txt");
    std::fs::write(&path, "original").unwrap();
    let mut editor = create_editor();
    let scratch = editor.active_document_id();
    open_and_load(&mut editor, &path);

    editor.request_lsp_rename();
    editor.focus_document(scratch).unwrap();
    editor.state.command_line = "replacement".into();
    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::Submit,
    ));
    editor.rename_lsp_symbol("replacement".into());

    let messages: Vec<_> = editor
        .state
        .notifications
        .iter_active()
        .map(|notice| notice.message.as_str())
        .collect();
    assert!(messages.contains(&"LSP: no server available for this file"));
    assert!(messages.contains(&"LSP rename: no file open"));
}

#[cfg(all(feature = "lsp", feature = "plugins"))]
#[test]
fn rename_sends_captured_and_current_utf16_positions() {
    use crate::document::ops::CursorCause;
    use crate::lsp::protocol::path_to_uri;
    use serde_json::json;

    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    std::fs::write(&first, "first\n😀symbol").unwrap();
    std::fs::write(&second, "top\nmiddle\né😀 second").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &first);
    let first_id = editor.active_document_id();
    open_and_load(&mut editor, &second);
    let second_id = editor.active_document_id();
    let requests = editor.lsp_manager.capture_wire_requests(&[&first, &second]);

    editor.focus_document(first_id).unwrap();
    editor
        .active_document()
        .set_cursor(7, CursorCause::User)
        .unwrap();
    assert!(editor.request_lsp_rename());
    editor.focus_document(second_id).unwrap();
    editor
        .active_document()
        .set_cursor(14, CursorCause::User)
        .unwrap();
    editor.state.command_line = "from_prompt".into();
    editor.handle_action(&crate::action::Action::Editor(
        crate::action::EditorAction::Submit,
    ));
    editor.call_plugins(|editor| {
        let error = editor.plugin_host.lua_exec("rift.lsp.rename('from_lua')");
        assert!(error.is_none(), "{error:?}");
    });

    for (path, line, character, name) in
        [(&first, 1, 2, "from_prompt"), (&second, 2, 4, "from_lua")]
    {
        let frame = requests
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let body = crate::transport::read_framed(&mut frame.as_slice()).unwrap();
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(request["method"], "textDocument/rename");
        assert_eq!(
            request["params"],
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": character },
                "newName": name,
            })
        );
    }
}

#[cfg(feature = "lsp")]
#[test]
fn goto_past_last_line_lands_on_last_text_line() {
    use crate::lsp::protocol::{LspLocation, LspPosition, LspRange};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eof.rs");
    std::fs::write(&path, "abc\ndef\n").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    let end = LspPosition {
        line: 2,
        character: 0,
    };
    editor.handle_goto_result(&[LspLocation {
        uri: crate::lsp::protocol::path_to_uri(&path),
        range: LspRange {
            start: end.clone(),
            end,
        },
    }]);

    assert_eq!(editor.active_document().buffer().cursor(), 4);
}

#[cfg(feature = "lsp")]
#[test]
fn undo_sends_its_inverse_as_an_incremental_lsp_change() {
    use crate::lsp::protocol::PositionEncoding;
    let mut editor = create_editor();
    let doc = editor.active_document();
    doc.edit(Edit::type_text("abc"), EditSource::User).unwrap();
    assert!(doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .is_some());
    assert!(!doc.has_pending_lsp_edits());

    assert!(doc.undo());
    assert!(doc.has_pending_lsp_edits(), "undo changed the buffer");
    let (range, text) = doc
        .take_incremental_lsp_changes(PositionEncoding::Utf16)
        .expect("undo records the inverse edit");
    assert_eq!((range.start.line, range.start.character), (0, 0));
    assert_eq!((range.end.line, range.end.character), (0, 3));
    assert!(text.is_empty());
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

    editor.execute_command_line(":lsp diag next");
    let doc = editor.active_document();
    assert_eq!(
        (
            doc.buffer().get_line(),
            doc.buffer().cursor() - doc.buffer().line_index.get_line_start(0)
        ),
        (0, 8)
    );

    editor.lsp_diagnostic_next();
    let doc = editor.active_document();
    assert_eq!(
        (
            doc.buffer().get_line(),
            doc.buffer().cursor() - doc.buffer().line_index.get_line_start(1)
        ),
        (1, 4)
    );

    #[cfg(feature = "plugins")]
    {
        let err = editor.plugin_host.lua_exec("rift.lsp.diagnostic_prev()");
        assert!(err.is_none(), "{err:?}");
        editor.apply_plugin_mutations();
        let doc = editor.active_document();
        assert_eq!((doc.buffer().get_line(), doc.buffer().cursor()), (0, 8));
    }
}

#[cfg(feature = "lsp")]
#[test]
fn published_diagnostic_underlines_exactly_its_range_and_keeps_the_eol_message() {
    use crate::lsp::protocol::{LspDiagnostic, LspPosition, LspRange};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("under.rs");
    std::fs::write(&path, "let \u{1F980}x = 1;\nok\n").unwrap();

    let mut editor = create_editor_sized(6, 40);
    editor
        .open_file(Some(path.display().to_string()), false)
        .unwrap();
    drain_jobs(&mut editor);
    editor
        .active_document()
        .set_option("number", Some("false"))
        .unwrap();

    let uri = crate::lsp::protocol::path_to_uri(&path);
    crate::lsp::LspMessage::Diagnostics {
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
    }
    .apply(&mut editor);

    editor.update_and_render().unwrap();
    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    let row: Vec<char> = (0..cols).map(|c| cells[c].to_char()).collect();
    let underlined: Vec<usize> = (0..cols)
        .filter(|&c| cells[c].attrs.is_underline())
        .collect();
    assert_eq!(row[6..11].iter().collect::<String>(), "x = 1");
    assert_eq!(
        underlined,
        vec![6, 7, 8, 9, 10],
        "only the diagnosed span is underlined"
    );
    assert_eq!(row[11..16].iter().collect::<String>(), "; bad");

    let doc = editor.active_document();
    doc.set_cursor(0, crate::document::ops::CursorCause::User)
        .ok();
    doc.edit(Edit::type_text("  "), EditSource::User).unwrap();
    editor.update_and_render().unwrap();
    let cells = editor.render_system.compositor.get_composited_slice();
    let underlined: Vec<usize> = (0..cols)
        .filter(|&c| cells[c].attrs.is_underline())
        .collect();
    assert_eq!(underlined, vec![8, 9, 10, 11, 12]);
}

#[cfg(feature = "lsp")]
fn receive_editor_job(
    editor: &crate::editor::Editor<crate::test_utils::MockTerminal>,
) -> crate::job_manager::JobMessage {
    editor
        .job_manager
        .receiver()
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
}

#[cfg(feature = "lsp")]
fn take_file_load_payload(
    editor: &mut crate::editor::Editor<crate::test_utils::MockTerminal>,
    job: usize,
) -> Box<dyn crate::job_manager::JobPayload> {
    use crate::job_manager::JobMessage;

    loop {
        match receive_editor_job(editor) {
            JobMessage::Custom(id, payload) if id == job => return payload,
            JobMessage::Error(id, err) if id == job => panic!("file load failed: {err}"),
            message => editor.handle_job_message(message),
        }
    }
}

#[cfg(feature = "lsp")]
#[test]
fn goto_on_a_loading_split_waits_for_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.txt");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let mut editor = create_editor();
    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::types::SplitSubcommand::File(
            path.to_string_lossy().into_owned(),
        ),
    );
    let target = editor.active_document_id();
    let job = *editor.file_load_jobs.keys().next().unwrap();

    editor.jump_to_location(&path, 0, 1);
    editor.jump_to_location(&path, 1, 2);
    assert_eq!(editor.active_document().buffer().cursor(), 0);

    let payload = take_file_load_payload(&mut editor, job);
    editor.handle_job_message(crate::job_manager::JobMessage::Custom(job, payload));
    assert_eq!(editor.active_document_id(), target);
    assert_eq!(editor.active_document().buffer().cursor(), 6);
}

#[cfg(feature = "lsp")]
#[test]
fn overlapping_file_loads_keep_each_documents_goto() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    std::fs::write(&first, "first\nsecond\n").unwrap();
    std::fs::write(&second, "x\nyz\n").unwrap();
    let mut editor = create_editor();

    editor.jump_to_location(&first, 1, 1);
    let first_doc = editor.active_document_id();
    editor.jump_to_location(&second, 1, 0);
    let second_doc = editor.active_document_id();
    assert_eq!(editor.file_load_jobs.len(), 2);
    while !editor.file_load_jobs.is_empty() {
        let message = receive_editor_job(&editor);
        editor.handle_job_message(message);
    }

    let first = editor.document_manager.get_document(first_doc).unwrap();
    assert_eq!(first.buffer().cursor(), 7);
    assert_eq!(editor.active_document_id(), second_doc);
    assert_eq!(editor.active_document().buffer().cursor(), 2);
}

#[cfg(all(feature = "lsp", feature = "plugins"))]
#[test]
fn lsp_jump_uses_opened_document_even_if_a_hook_redirects_focus() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.txt");
    std::fs::write(&path, "zero\nfirst\n").unwrap();
    let mut editor = create_editor();
    let scratch = editor.active_document_id();
    let scratch_id = scratch.get();
    let error = editor.plugin_host.lua_exec(&format!(
        "rift.on('BufEnter', function(ev) \
           if ev.buf ~= {scratch_id} then rift.switch_buf({scratch_id}) end \
         end)"
    ));
    assert!(error.is_none(), "{error:?}");

    editor.jump_to_location(&path, 1, 2);
    let job = *editor.file_load_jobs.keys().next().unwrap();
    let payload = take_file_load_payload(&mut editor, job);
    editor.handle_job_message(crate::job_manager::JobMessage::Custom(job, payload));

    assert_eq!(editor.active_document_id(), scratch);
    assert_eq!(editor.active_document().buffer().cursor(), 0);
    let target = editor
        .document_manager
        .find_open_document_id(&path)
        .unwrap();
    let target = editor.document_manager.get_document(target).unwrap();
    assert_eq!(target.buffer().cursor(), 7);
}

#[cfg(feature = "lsp")]
#[test]
fn cancelled_file_load_discards_its_late_result() {
    use crate::job_manager::JobMessage;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancelled.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    let mut editor = create_editor();
    editor.jump_to_location(&path, 1, 1);
    let job = *editor.file_load_jobs.keys().next().unwrap();
    let payload = take_file_load_payload(&mut editor, job);

    editor.handle_job_message(JobMessage::Cancelled(job));
    assert!(editor.file_load_jobs.is_empty());
    editor.handle_job_message(JobMessage::Custom(job, payload));
    assert!(editor.active_document().buffer().is_empty());
}

#[cfg(feature = "lsp")]
#[test]
fn stale_file_load_messages_leave_the_current_goto_intact() {
    use crate::job_manager::JobMessage;

    enum StaleMessage {
        Finished,
        Error,
        Cancelled,
        Custom,
        CustomToken,
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.txt");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
    for stale in [
        StaleMessage::Finished,
        StaleMessage::Error,
        StaleMessage::Cancelled,
        StaleMessage::Custom,
        StaleMessage::CustomToken,
    ] {
        let mut editor = create_editor();
        editor.jump_to_location(&path, 1, 0);
        let target = editor.active_document_id();
        let old_job = *editor.file_load_jobs.keys().next().unwrap();
        let old_token = editor.job_manager.job_token(old_job).unwrap();
        let old_payload = take_file_load_payload(&mut editor, old_job);

        editor.reload_document(target, false).unwrap();
        editor.jump_to_location(&path, 2, 1);
        let fresh_job = *editor
            .file_load_jobs
            .keys()
            .find(|&&id| id != old_job)
            .unwrap();

        let message = match stale {
            StaleMessage::Finished => JobMessage::Finished(old_job, true),
            StaleMessage::Error => JobMessage::Error(old_job, "load failed".into()),
            StaleMessage::Cancelled => JobMessage::Cancelled(old_job),
            StaleMessage::Custom => JobMessage::Custom(old_job, old_payload),
            StaleMessage::CustomToken => JobMessage::CustomToken(old_job, old_token, old_payload),
        };
        editor.handle_job_message(message);
        assert!(!editor.file_load_jobs.contains_key(&old_job));
        assert!(editor.file_load_jobs.contains_key(&fresh_job));
        assert!(editor.active_document().buffer().is_empty());

        let payload = take_file_load_payload(&mut editor, fresh_job);
        editor.handle_job_message(JobMessage::Custom(fresh_job, payload));
        assert_eq!(editor.active_document().buffer().cursor(), 9);
    }
}
