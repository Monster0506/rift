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

fn lua_ok(editor: &Editor<MockTerminal>, code: &str) {
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
