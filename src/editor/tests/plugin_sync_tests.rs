use super::common::*;
use super::*;
use crate::test_utils::MockTerminal;

fn lua_editor(text: &str) -> Editor<MockTerminal> {
    let mut editor = create_editor();
    load_text(&mut editor, text);
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();
    editor
}

fn lua_ok<T: crate::term::TerminalBackend>(editor: &Editor<T>, code: &str) {
    let err = editor.plugin_host.lua_exec(code);
    assert!(err.is_none(), "{err:?}");
}

#[test]
fn global_setting_change_reaches_get_option() {
    let mut editor = lua_editor("hello\n");
    lua_ok(&editor, "assert(rift.get_option('clipboard.size') ~= 42)");

    editor.execute_command_line("set clipboard.size 42");
    editor.update_lua_state();

    lua_ok(&editor, "assert(rift.get_option('clipboard.size') == 42)");
}

#[test]
fn dropped_highlight_write_is_restored_on_next_sync() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.highlight.set('SyncProbe', { fg = '#ff0000' }) \
         assert(rift.highlight.get('SyncProbe') ~= nil)",
    );
    let _ = editor.plugin_host.drain_mutation_envelopes().count();

    editor.update_lua_state();

    lua_ok(&editor, "assert(rift.highlight.get('SyncProbe') == nil)");
}

#[test]
fn document_options_follow_setlocal_and_the_active_buffer() {
    let mut editor = lua_editor("hello\n");
    editor.execute_command_line("setlocal tabwidth 8");
    editor.update_lua_state();
    lua_ok(&editor, "assert(rift.get_option('tab_width') == 8)");

    lua_ok(&editor, "rift.create_scratch_buf('other', { 'x' })");
    editor.apply_plugin_mutations();
    editor.update_lua_state();

    lua_ok(&editor, "assert(rift.get_option('tab_width') == 4)");
}

#[test]
fn buffer_list_tracks_line_count_and_new_buffers() {
    let mut editor = lua_editor("one\n");
    lua_ok(
        &editor,
        "local list = rift.get_buf_list() \
         assert(#list == 1 and list[1].line_count == 2, rift.inspect(list))",
    );

    editor.active_document().insert_str("two\nthree\n").unwrap();
    editor.update_lua_state();
    lua_ok(
        &editor,
        "local list = rift.get_buf_list() \
         assert(list[1].line_count == 4 and list[1].is_dirty, rift.inspect(list))",
    );

    lua_ok(&editor, "rift.create_scratch_buf('other', { 'x' })");
    editor.apply_plugin_mutations();
    editor.update_lua_state();
    lua_ok(
        &editor,
        "local list = rift.get_buf_list() \
         assert(#list == 2 and list[2].is_current, rift.inspect(list))",
    );
}

#[test]
fn window_list_tracks_splits() {
    let mut editor = lua_editor("hello\n");
    lua_ok(&editor, "assert(#rift.windows.list() == 1)");

    editor.execute_command_line("vsplit");
    editor.update_lua_state();

    lua_ok(
        &editor,
        "local wins = rift.windows.list() \
         assert(#wins == 2 and wins[1].cols < 80, rift.inspect(wins))",
    );
}

#[cfg(feature = "lsp")]
#[test]
fn lsp_diagnostics_changes_reach_plugin() {
    use crate::lsp::protocol::{LspDiagnostic, LspPosition, LspRange};
    let diagnostic = |message: &str| LspDiagnostic {
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 1,
            },
            end: LspPosition {
                line: 0,
                character: 2,
            },
        },
        severity: Some(1),
        message: message.to_string(),
        source: None,
    };
    let uri = crate::lsp::protocol::normalize_uri("file:///sync/probe.rs");
    let mut editor = lua_editor("hello\n");

    editor
        .lsp_diagnostics
        .insert(uri.clone(), vec![diagnostic("first")]);
    editor.update_lua_state();
    lua_ok(
        &editor,
        &format!("assert(rift.lsp.get_diagnostics('{uri}')[1].message == 'first')"),
    );

    editor
        .lsp_diagnostics
        .insert(uri.clone(), vec![diagnostic("second")]);
    editor.update_lua_state();
    lua_ok(
        &editor,
        &format!("assert(rift.lsp.get_diagnostics('{uri}')[1].message == 'second')"),
    );
}

#[test]
fn buffer_var_set_by_editor_reaches_plugin() {
    let mut editor = lua_editor("hello\n");
    editor.active_document().vars.insert(
        "probe".to_string(),
        crate::annotations::value::Value::Str("editor".to_string()),
    );
    editor.update_lua_state();

    lua_ok(
        &editor,
        "assert(rift.get_buffer_var(nil, 'probe') == 'editor')",
    );
}

#[test]
fn dropped_buffer_var_write_is_restored_on_next_sync() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.set_buffer_var(9999, 'probe', 'local') \
         assert(rift.get_buffer_var(9999, 'probe') == 'local')",
    );
    editor.apply_plugin_mutations();
    editor.update_lua_state();

    lua_ok(&editor, "assert(rift.get_buffer_var(9999, 'probe') == nil)");
}

#[test]
fn registered_command_is_listed_before_any_sync() {
    let editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.register_command('SyncProbe', function() end, 'probe') \
         local found = false \
         for _, c in ipairs(rift.get_commands()) do \
           if c.name == 'SyncProbe' and c.description == 'probe' then found = true end \
         end \
         assert(found)",
    );
}

fn notification_count(editor: &Editor<MockTerminal>, message: &str) -> usize {
    editor
        .state
        .error_manager
        .notifications()
        .iter_active()
        .filter(|n| n.message == message)
        .count()
}

#[test]
fn mutation_queued_by_a_nested_dispatch_applies_in_the_same_call() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.register_buffer_kind('probe', { on_close = function() end }) \
         rift.on('BufOpen', function() rift.set_cursor(2, 1) end)",
    );
    editor.apply_plugin_mutations();

    lua_ok(
        &editor,
        "rift.create_buffer('probe', { lines = { 'aaa', 'bbb' } })",
    );
    editor.apply_plugin_mutations();

    let doc = editor.document_manager.active_document().unwrap();
    assert_eq!(doc.kind.kind_str(), "probe");
    assert_eq!(doc.buffer.cursor(), 5);
}

#[test]
fn mutations_that_keep_requeuing_stop_at_the_round_cap() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.on('BufOpen', function() rift.create_scratch_buf('loop', { 'x' }) end) \
         rift.create_scratch_buf('loop', { 'x' })",
    );

    editor.apply_plugin_mutations();

    assert_eq!(editor.document_manager.documents_iter().count(), 17);
    assert!(editor.plugin_host.has_queued_mutations());
    assert_eq!(
        notification_count(
            &editor,
            "plugin mutations kept re-queuing; stopped after 16 rounds"
        ),
        1
    );
}

#[test]
fn plugin_command_reads_the_cursor_the_previous_key_moved() {
    let mut editor = lua_editor("one\ntwo\nthree\n");
    lua_ok(
        &editor,
        "rift.register_command('zzprobe', function() \
           local row, col = rift.get_cursor() _G.seen = row .. ':' .. col \
         end, 'probe')",
    );
    editor.apply_plugin_mutations();

    editor.active_document().buffer.set_cursor(5).unwrap();
    editor.execute_command_line("zzprobe");

    lua_ok(&editor, "assert(_G.seen == '2:1', tostring(_G.seen))");
}

#[test]
fn mapped_function_reads_the_cursor_moved_earlier_in_the_key_sequence() {
    let mut editor = editor_with_text("one\ntwo\nthree\n");
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();
    lua_ok(
        &editor,
        "rift.map('n', '<F5>', function() \
           local row, col = rift.get_cursor() _G.seen = row .. ':' .. col \
         end)",
    );
    editor.apply_plugin_mutations();

    press(&mut editor, &keys("j<F5>"));

    lua_ok(&editor, "assert(_G.seen == '2:0', tostring(_G.seen))");
}

fn editor_with_probe_buffer() -> Editor<MockTerminal> {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.register_buffer_kind('probe', { \
           on_action = function() _G.seen = rift.get_lines(1, 1)[1] end, \
           on_save = function() _G.seen = rift.get_lines(1, 1)[1] end, \
           on_close = function() _G.seen = rift.get_lines(1, 1)[1] end, \
         })",
    );
    editor.apply_plugin_mutations();
    lua_ok(
        &editor,
        "rift.create_buffer('probe', { lines = { 'first' } })",
    );
    editor.apply_plugin_mutations();
    let doc = editor.active_document();
    doc.buffer.set_cursor(0).unwrap();
    doc.buffer.insert_str("edited ").unwrap();
    editor
}

#[test]
fn buffer_kind_on_action_reads_the_edited_lines() {
    let mut editor = editor_with_probe_buffer();

    editor.handle_action(&crate::action::Action::Buffer("probe:go".to_string()));

    lua_ok(
        &editor,
        "assert(_G.seen == 'edited first', tostring(_G.seen))",
    );
}

#[test]
fn buffer_kind_on_save_reads_the_edited_lines() {
    let mut editor = editor_with_probe_buffer();

    editor.do_save();

    lua_ok(
        &editor,
        "assert(_G.seen == 'edited first', tostring(_G.seen))",
    );
}

#[test]
fn buffer_kind_on_close_reads_the_edited_lines() {
    let mut editor = editor_with_probe_buffer();

    editor.do_quit(true);

    lua_ok(
        &editor,
        "assert(_G.seen == 'edited first', tostring(_G.seen))",
    );
}

#[test]
fn buf_save_pre_reads_the_edit_made_before_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("probe.txt");
    std::fs::write(&path, "saved\n").unwrap();
    let mut editor = lua_editor("");
    open_and_load(&mut editor, &path);
    lua_ok(
        &editor,
        "rift.on('BufSavePre', function() _G.seen = rift.get_lines(1, 1)[1] end)",
    );
    let doc = editor.active_document();
    doc.buffer.set_cursor(0).unwrap();
    doc.buffer.insert_str("unsaved ").unwrap();

    editor.execute_command_line("w");
    drain_jobs(&mut editor);

    lua_ok(
        &editor,
        "assert(_G.seen == 'unsaved saved', tostring(_G.seen))",
    );
}

#[test]
fn buf_enter_after_quit_reads_the_now_active_buffer() {
    let mut editor = lua_editor("hello\n");
    let first = editor.document_manager.active_document_id().unwrap().get();
    lua_ok(&editor, "rift.create_scratch_buf('second', { 'x' })");
    editor.apply_plugin_mutations();
    lua_ok(
        &editor,
        "rift.on('BufClose', function() rift.set_cursor(1, 3) end) \
         rift.on('BufEnter', function() \
           local _, col = rift.get_cursor() _G.seen = rift.current_buf() .. ':' .. col \
         end)",
    );

    editor.do_quit(true);

    lua_ok(
        &editor,
        &format!("assert(_G.seen == '{first}:3', tostring(_G.seen))"),
    );
}

#[test]
fn buf_enter_after_closing_a_split_reads_the_remaining_buffer() {
    let mut editor = lua_editor("first\n");
    let first = editor.active_document_id().get();
    editor.execute_command_line("vsplit");
    lua_ok(&editor, "rift.create_scratch_buf('second', { 'second' })");
    editor.apply_plugin_mutations();
    lua_ok(
        &editor,
        "rift.on('BufEnter', function(ev) \
           _G.seen = { event_buf = ev.buf, current_buf = rift.current_buf(), \
                       line = rift.get_lines(1, 1)[1] } \
         end)",
    );

    editor.execute_command_line("q");

    lua_ok(
        &editor,
        &format!(
            "assert(_G.seen and _G.seen.event_buf == {first} \
             and _G.seen.current_buf == {first} and _G.seen.line == 'first', \
             rift.inspect(_G.seen))"
        ),
    );
}
