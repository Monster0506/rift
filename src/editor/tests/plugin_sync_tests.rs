use super::common::*;
use super::*;
use crate::document::ops::{Edit, EditSource};
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

    editor
        .active_document()
        .edit(Edit::type_text("two\nthree\n"), EditSource::User)
        .unwrap();
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
        "rift.set_buffer_var(nil, 'probe', 'local') \
         assert(rift.get_buffer_var(nil, 'probe') == 'local')",
    );
    let _ = editor.plugin_host.drain_mutation_envelopes().count();
    editor.update_lua_state();

    lua_ok(&editor, "assert(rift.get_buffer_var(nil, 'probe') == nil)");
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
        .notifications
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
    assert_eq!(doc.buffer().cursor(), 5);
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

    assert!(editor.document_manager.documents_iter().count() <= 17);
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

    editor
        .active_document()
        .set_cursor(5, crate::document::ops::CursorCause::User)
        .unwrap();
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
    let text = format!("edited {}", doc.buffer());
    doc.replace_content(&text, crate::document::ops::ContentKind::Render);
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
    let text = format!("unsaved {}", doc.buffer());
    doc.replace_content(&text, crate::document::ops::ContentKind::Render);

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

#[test]
fn buf_leave_reads_the_buffer_being_left() {
    let (mut editor, first, second) = editor_with_two_buffers();
    lua_ok(
        &editor,
        "rift.on('BufLeave', function(ev) _G.left = ev.buf .. ':' .. rift.current_buf() end)",
    );

    editor.do_buffer_next();

    assert_eq!(editor.active_document_id().get(), first);
    lua_ok(
        &editor,
        &format!("assert(_G.left == '{second}:{second}', tostring(_G.left))"),
    );
}

#[test]
fn opening_a_file_raises_buf_enter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.txt");
    std::fs::write(&path, "other\n").unwrap();
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.on('BufEnter', function(ev) _G.entered = ev.buf end)",
    );

    open_and_load(&mut editor, &path);

    let opened = editor.active_document_id().get();
    lua_ok(
        &editor,
        &format!("assert(_G.entered == {opened}, tostring(_G.entered))"),
    );
}

#[test]
fn win_leave_and_win_enter_read_the_focus_change() {
    let mut editor = lua_editor("hello\n");
    let first = editor.split_tree.focused_window_id();
    editor.execute_command_line("vsplit");
    let second = editor.split_tree.focused_window_id();
    assert_ne!(first, second);
    lua_ok(
        &editor,
        "rift.on('WinLeave', function() _G.leave = rift.windows.current() end) \
         rift.on('WinEnter', function() \
           _G.enter = rift.windows.current() .. ':' .. tostring(rift.windows.previous()) \
         end)",
    );

    editor.focus_window(first);

    let (first, second) = (first.get(), second.get());
    lua_ok(
        &editor,
        &format!(
            "assert(_G.leave == {second}, tostring(_G.leave)) \
             assert(_G.enter == '{first}:{second}', tostring(_G.enter))"
        ),
    );
}

fn split_editor_recording_layout_on(event: &str) -> Editor<MockTerminal> {
    let mut editor = lua_editor("first\n");
    editor.execute_command_line("vsplit");
    lua_ok(&editor, "rift.create_scratch_buf('second', { 'second' })");
    editor.apply_plugin_mutations();
    lua_ok(
        &editor,
        &format!(
            "_G.layout = function() \
               local t = {{}} \
               for _, w in ipairs(rift.windows.list()) do \
                 t[#t + 1] = w.id .. ':' .. w.buf .. ':' .. w.row .. ':' .. w.col \
               end \
               return table.concat(t, ' ') \
             end \
             rift.on('{event}', function() _G.seen = _G.layout() end)"
        ),
    );
    editor.update_lua_state();
    lua_ok(&editor, "_G.before = _G.layout()");
    editor
}

fn assert_handler_saw_the_updated_layout(editor: &Editor<MockTerminal>) {
    editor.update_lua_state();
    lua_ok(
        editor,
        "assert(_G.seen ~= nil and _G.seen ~= _G.before and _G.seen == _G.layout(), \
           tostring(_G.before) .. ' -> ' .. tostring(_G.seen) .. ' vs ' .. _G.layout())",
    );
}

#[test]
fn win_moved_from_split_command_reads_the_updated_window_list() {
    let mut editor = split_editor_recording_layout_on("WinMoved");

    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::types::SplitSubcommand::Move(
            crate::split::navigation::Direction::Left,
        ),
    );

    assert_handler_saw_the_updated_layout(&editor);
}

#[test]
fn win_moved_from_plugin_mutation_reads_the_updated_window_list() {
    let mut editor = split_editor_recording_layout_on("WinMoved");

    lua_ok(&editor, "rift.windows.move('left')");
    editor.apply_plugin_mutations();

    assert_handler_saw_the_updated_layout(&editor);
}

#[test]
fn win_swapped_from_split_command_reads_the_updated_window_list() {
    let mut editor = split_editor_recording_layout_on("WinSwapped");

    editor.do_split_window(
        crate::split::tree::SplitDirection::Vertical,
        crate::command_line::commands::types::SplitSubcommand::Exchange,
    );

    assert_handler_saw_the_updated_layout(&editor);
}

#[test]
fn win_swapped_from_plugin_mutation_reads_the_updated_window_list() {
    let mut editor = split_editor_recording_layout_on("WinSwapped");

    lua_ok(&editor, "rift.windows.exchange()");
    editor.apply_plugin_mutations();

    assert_handler_saw_the_updated_layout(&editor);
}

const RECORD_NEW_BUFFER_IN_LIST: &str = "rift.on('BufOpen', function(ev) \
       _G.seen = false \
       for _, b in ipairs(rift.get_buf_list()) do \
         if b.id == ev.buf and b.is_current then _G.seen = true end \
       end \
     end)";

#[test]
fn buf_open_from_create_buffer_sees_the_new_buffer_in_the_list() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.register_buffer_kind('probe', { on_close = function() end })",
    );
    editor.apply_plugin_mutations();
    lua_ok(&editor, RECORD_NEW_BUFFER_IN_LIST);

    lua_ok(&editor, "rift.create_buffer('probe', { lines = { 'x' } })");
    editor.apply_plugin_mutations();

    lua_ok(&editor, "assert(_G.seen == true, tostring(_G.seen))");
}

#[test]
fn buf_open_from_scratch_buffer_sees_the_new_buffer_in_the_list() {
    let mut editor = lua_editor("hello\n");
    lua_ok(&editor, RECORD_NEW_BUFFER_IN_LIST);

    lua_ok(&editor, "rift.create_scratch_buf('scratch', { 'x' })");
    editor.apply_plugin_mutations();

    lua_ok(&editor, "assert(_G.seen == true, tostring(_G.seen))");
}

#[test]
fn buf_open_after_a_file_load_carries_the_filetype() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    let mut editor = lua_editor("");
    lua_ok(
        &editor,
        "rift.on('BufOpen', function(ev) _G.ft = ev.filetype end)",
    );

    open_and_load(&mut editor, &path);
    let _ = editor.update_and_render();

    lua_ok(&editor, "assert(_G.ft == 'rust', tostring(_G.ft))");
}

#[test]
fn opening_the_explorer_raises_buf_open_for_its_buffers() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "_G.opened = {} rift.on('BufOpen', function(ev) table.insert(_G.opened, ev.buf) end)",
    );

    editor.open_explorer(dir.path().to_path_buf());
    let _ = editor.update_and_render();

    let dir_buf = editor.panel_layout.as_ref().unwrap().dir_doc_id.get();
    lua_ok(
        &editor,
        &format!(
            "local found = false \
             for _, b in ipairs(_G.opened) do if b == {dir_buf} then found = true end end \
             assert(found, #_G.opened)"
        ),
    );
}

#[test]
fn editor_quit_reads_the_final_cursor() {
    let mut editor = lua_editor("one\ntwo\nthree\n");
    lua_ok(
        &editor,
        "rift.on('EditorQuit', function() \
           local row, col = rift.get_cursor() _G.seen = row .. ':' .. col \
         end)",
    );
    editor
        .active_document()
        .set_cursor(9, crate::document::ops::CursorCause::User)
        .unwrap();
    editor.should_quit = true;

    editor.run().unwrap();

    lua_ok(&editor, "assert(_G.seen == '3:1', tostring(_G.seen))");
}

#[test]
fn reloaded_plugin_top_level_code_reads_the_open_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("open.txt");
    std::fs::write(&path, "opened\n").unwrap();
    let plugins = tempfile::tempdir().unwrap();
    std::fs::write(
        plugins.path().join("probe.lua"),
        "_G.loaded = rift.current_buf() .. ':' .. rift.get_lines(1, 1)[1]",
    )
    .unwrap();
    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    let buf = editor.active_document_id().get();

    let errors = editor.load_plugins_from(
        &[plugins.path().to_path_buf()],
        &plugins.path().join("missing_init.lua"),
    );

    assert!(errors.is_empty(), "{errors:?}");
    lua_ok(
        &editor,
        &format!("assert(_G.loaded == '{buf}:opened', tostring(_G.loaded))"),
    );
}

fn editor_with_two_buffers() -> (Editor<MockTerminal>, u64, u64) {
    let mut editor = lua_editor("first");
    let first = editor.active_document_id().get();
    lua_ok(&editor, "rift.create_scratch_buf('second', { 'second' })");
    editor.apply_plugin_mutations();
    let second = editor.active_document_id().get();
    assert_ne!(first, second);
    editor.update_lua_state();
    (editor, first, second)
}

fn buffer_text(editor: &Editor<MockTerminal>, id: u64) -> String {
    editor
        .document_manager
        .get_document(crate::document::DocumentId::new(id))
        .unwrap()
        .buffer()
        .to_string()
}

#[test]
fn edit_queued_after_switch_buf_lands_in_the_buffer_current_when_queued() {
    let (mut editor, first, second) = editor_with_two_buffers();

    lua_ok(
        &editor,
        &format!("rift.switch_buf({first}) assert(rift.current_buf() == {first}) assert(rift.get_lines(1, 1)[1] == 'first') rift.set_cursor(1, 0) rift.insert('x')"),
    );
    editor.apply_plugin_mutations();

    let active = editor.document_manager.active_document_id().unwrap();
    assert_eq!(active.get(), first);
    assert_eq!(buffer_text(&editor, first), "xfirst");
    assert_eq!(buffer_text(&editor, second), "second");
}

#[test]
fn switching_to_an_unknown_buffer_raises_without_changing_the_current_buffer() {
    let (mut editor, _first, second) = editor_with_two_buffers();
    lua_ok(&editor, &format!(
        "local ok, err = pcall(rift.switch_buf, 9999) assert(not ok and tostring(err):find('no open buffer 9999')) assert(rift.current_buf() == {second})"
    ));
    editor.apply_plugin_mutations();
    assert_eq!(editor.active_document_id().get(), second);
}

#[test]
fn edit_queued_before_switch_buf_lands_in_the_buffer_current_when_queued() {
    let (mut editor, first, second) = editor_with_two_buffers();

    lua_ok(
        &editor,
        &format!("rift.set_cursor(1, 0) rift.insert('x') rift.switch_buf({first})"),
    );
    editor.apply_plugin_mutations();

    let active = editor.document_manager.active_document_id().unwrap();
    assert_eq!(active.get(), first);
    assert_eq!(buffer_text(&editor, first), "first");
    assert_eq!(buffer_text(&editor, second), "xsecond");
}

fn drop_queued_writes(editor: &mut Editor<MockTerminal>) {
    let _ = editor.plugin_host.drain_mutation_envelopes().count();
    editor.update_lua_state();
}

fn sync_applied_writes(editor: &mut Editor<MockTerminal>) {
    editor.apply_plugin_mutations();
    editor.update_lua_state();
}

#[test]
fn set_option_reads_back_in_the_same_call() {
    let mut editor = lua_editor("hello\n");
    let reads = "assert(rift.get_option('tab_width') == 7 and rift.get_tab_width() == 7) \
                 assert(rift.get_option('clipboard.size') == 42)";
    lua_ok(
        &editor,
        &format!("rift.set_option('tab_width', 7) rift.set_option('clipboard.size', 42) {reads}"),
    );

    sync_applied_writes(&mut editor);

    lua_ok(&editor, reads);
}

#[test]
fn unapplied_set_option_is_replaced_on_next_sync() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.set_option('tab_width', 7) rift.set_option('clipboard.size', 42)",
    );

    drop_queued_writes(&mut editor);

    lua_ok(
        &editor,
        "assert(rift.get_option('tab_width') == 4) \
         assert(rift.get_option('clipboard.size') ~= 42)",
    );
}

#[test]
fn text_edits_read_back_in_the_same_call_and_match_the_editor() {
    let mut editor = lua_editor("one\ntwo\nthree\n");
    lua_ok(
        &editor,
        r"
        rift.set_cursor(2, 0)
        rift.insert('>')
        local row, col = rift.get_cursor()
        assert(row == 2 and col == 1, row .. ':' .. col)
        assert(rift.get_lines(2, 2)[1] == '>two')
        rift.delete_before(1)
        rift.delete_forward(1)
        assert(rift.get_current_line() == 'wo', rift.get_current_line())
        rift.set_current_line('TWO')
        assert(rift.get_current_line() == 'TWO')
        rift.append_lines({ 'four' })
        assert(rift.get_line_count() == 4 and rift.get_lines(4, 4)[1] == 'four')
        rift.delete_current_line()
        assert(rift.get_line_count() == 3, rift.get_line_count())
        assert(rift.search('TWO')[1].row == 2)
        _G.local_text = table.concat(rift.get_lines(1, -1), '|')
        ",
    );

    sync_applied_writes(&mut editor);

    assert_eq!(
        editor.active_document().buffer().to_string(),
        "one\nTWO\nthree"
    );
    lua_ok(
        &editor,
        "assert(table.concat(rift.get_lines(1, -1), '|') == _G.local_text, _G.local_text)",
    );
}

#[test]
fn unapplied_text_edit_is_replaced_on_next_sync() {
    let mut editor = lua_editor("one\n");
    lua_ok(
        &editor,
        "rift.insert('x') assert(rift.get_lines(1, 1)[1] == 'xone')",
    );

    drop_queued_writes(&mut editor);

    lua_ok(
        &editor,
        "assert(rift.get_lines(1, 1)[1] == 'one') \
         local row, col = rift.get_cursor() assert(row == 1 and col == 0)",
    );
}

#[test]
fn cursor_moves_read_back_in_the_same_call() {
    let mut editor = lua_editor("one\ntwo\nthree\n");
    lua_ok(
        &editor,
        "rift.set_cursor(2, 50) \
         local row, col = rift.get_cursor() assert(row == 2 and col == 3, row .. ':' .. col) \
         rift.center_on_line(3) \
         row, col = rift.get_cursor() assert(row == 3 and col == 0, row .. ':' .. col)",
    );

    sync_applied_writes(&mut editor);

    lua_ok(
        &editor,
        "local row, col = rift.get_cursor() assert(row == 3 and col == 0, row .. ':' .. col)",
    );
}

#[test]
fn unapplied_cursor_move_is_replaced_on_next_sync() {
    let mut editor = lua_editor("one\ntwo\n");
    lua_ok(&editor, "rift.set_cursor(2, 1)");

    drop_queued_writes(&mut editor);

    lua_ok(
        &editor,
        "local row, col = rift.get_cursor() assert(row == 1 and col == 0, row .. ':' .. col)",
    );
}

#[test]
fn scroll_and_line_ending_read_back_in_the_same_call() {
    let mut editor = lua_editor("one\n");
    lua_ok(
        &editor,
        "rift.set_scroll(5, 2) rift.set_line_ending('CRLF') \
         local top, left = rift.get_scroll() assert(top == 5 and left == 2) \
         assert(rift.get_line_ending() == 'crlf')",
    );

    sync_applied_writes(&mut editor);

    lua_ok(&editor, "assert(rift.get_line_ending() == 'crlf')");
}

#[test]
fn unapplied_scroll_and_line_ending_are_replaced_on_next_sync() {
    let mut editor = lua_editor("one\n");
    lua_ok(
        &editor,
        "rift.set_scroll(5, 2) rift.set_line_ending('crlf')",
    );

    drop_queued_writes(&mut editor);

    lua_ok(
        &editor,
        "local top, left = rift.get_scroll() assert(top == 0 and left == 0) \
         assert(rift.get_line_ending() == 'lf')",
    );
}

#[test]
fn annotation_edits_read_back_in_the_same_call() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "_G.kept = rift.annotations.add{ kind = 'keep.mark', point = 1, actions = { { verb = 'go' } } } \
         local a = rift.annotations.get(_G.kept) \
         assert(a and a.kind == 'keep.mark' and a.interactive and a.anchor == 'point') \
         assert(#rift.annotations.at(1) == 1) \
         rift.annotations.update(_G.kept, { visible = false }) \
         assert(rift.annotations.get(_G.kept).visible == false) \
         local gone = rift.annotations.add{ kind = 'probe.a', line = 1 } \
         rift.annotations.remove(gone) \
         assert(rift.annotations.get(gone) == nil) \
         rift.annotations.add{ kind = 'probe.b', line = 1 } \
         rift.annotations.clear('probe.') \
         assert(#rift.annotations.by_kind('probe.') == 0)",
    );

    sync_applied_writes(&mut editor);

    lua_ok(
        &editor,
        "assert(rift.annotations.get(_G.kept).visible == false) \
         assert(#rift.annotations.by_kind('probe.') == 0)",
    );
}

#[test]
fn unapplied_annotation_add_is_replaced_on_next_sync() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "_G.id = rift.annotations.add{ kind = 'probe.mark', line = 1 }",
    );

    drop_queued_writes(&mut editor);

    lua_ok(&editor, "assert(rift.annotations.get(_G.id) == nil)");
}

#[test]
fn timer_callback_reads_the_cursor_moved_after_it_was_scheduled() {
    let mut editor = lua_editor("one\ntwo\n");
    lua_ok(
        &editor,
        "rift.timer(0, function() _G.seen = rift.get_cursor() end)",
    );
    editor
        .active_document()
        .set_cursor(4, crate::document::ops::CursorCause::User)
        .unwrap();

    editor.tick().unwrap();

    lua_ok(&editor, "assert(_G.seen == 2, tostring(_G.seen))");
}

#[test]
fn set_cursor_hold_delay_moves_the_hold_deadline() {
    let mut editor = lua_editor("one\ntwo\n");
    let default = editor.plugin_host.next_timer_deadline().unwrap();
    lua_ok(
        &editor,
        "rift.set_cursor_hold_delay(50) \
         rift.on('CursorHold', function() _G.held = true end)",
    );
    editor.apply_plugin_mutations();

    let shortened = editor.plugin_host.next_timer_deadline().unwrap();
    assert_eq!(
        default.duration_since(shortened),
        std::time::Duration::from_millis(350)
    );
    assert!(editor.plugin_host.fire_due_timers(shortened));
    lua_ok(&editor, "assert(_G.held)");
}

fn resent_texts(editor: &Editor<MockTerminal>) -> Vec<u64> {
    let mut ids: Vec<u64> = editor
        .lua_buf_snapshots()
        .into_iter()
        .flatten()
        .filter(|(_, update)| update.text.is_some())
        .map(|(id, _)| id)
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn switching_between_unchanged_buffers_resends_neither() {
    let (mut editor, first, second) = editor_with_two_buffers();

    for (target, text) in [(first, "first"), (second, "second"), (first, "first")] {
        editor
            .focus_document(crate::document::DocumentId::new(target))
            .unwrap();
        assert_eq!(
            resent_texts(&editor),
            Vec::<u64>::new(),
            "switch to {target}"
        );
        editor.update_lua_state();
        lua_ok(
            &editor,
            &format!(
                "assert(rift.current_buf() == {target}) \
                 assert(rift.get_lines(1, 1)[1] == '{text}')"
            ),
        );
    }
}

#[test]
fn editing_a_background_buffer_resends_only_that_buffer() {
    let (mut editor, first, second) = editor_with_two_buffers();
    editor
        .document_manager
        .get_document_mut(crate::document::DocumentId::new(first))
        .unwrap()
        .replace_content("x", crate::document::ops::ContentKind::Render);

    let updates = editor.lua_buf_snapshots().unwrap();

    let update = |id: u64| &updates.iter().find(|(doc, _)| *doc == id).unwrap().1;
    assert!(update(first).text.is_some());
    let untouched = update(second);
    assert!(untouched.text.is_none());
    assert!(untouched.meta.is_none());
    assert!(untouched.options.is_none());
    assert!(untouched.annotations.is_none());
}

#[test]
fn closed_buffer_drops_out_of_the_snapshot() {
    let (mut editor, first, second) = editor_with_two_buffers();
    lua_ok(&editor, "rift.close_buf(true)");
    editor.apply_plugin_mutations();
    assert!(editor
        .document_manager
        .get_document(crate::document::DocumentId::new(second))
        .is_none());
    editor
        .document_manager
        .get_document_mut(crate::document::DocumentId::new(first))
        .unwrap()
        .replace_content("x", crate::document::ops::ContentKind::Render);

    let ids: Vec<u64> = editor
        .lua_buf_snapshots()
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    assert_eq!(ids, vec![first]);
}

#[test]
fn scratch_open_and_close_expose_the_buffer_being_left() {
    let mut editor = lua_editor("first\n");
    let first = editor.active_document_id().get();
    lua_ok(
        &editor,
        "rift.on('BufLeave', function(ev) _G.left = {ev.buf, rift.current_buf()} end) \
         rift.on('BufEnter', function(ev) _G.entered = {ev.buf, rift.current_buf()} end) \
         rift.create_scratch_buf('scratch', {'second'})",
    );
    editor.apply_plugin_mutations();
    let second = editor.active_document_id();
    assert_ne!(first, second.get());
    lua_ok(
        &editor,
        &format!("assert(left[1] == {first} and left[2] == {first})"),
    );

    editor
        .close_buffer(second, crate::document::manager::RemovalIntent::Force)
        .unwrap();
    let second = second.get();
    lua_ok(
        &editor,
        &format!(
            "assert(left[1] == {second} and left[2] == {second}) \
             assert(entered[1] == {first} and entered[2] == {first})"
        ),
    );
}

#[test]
fn window_lifecycle_hooks_see_the_completed_layout_change() {
    let mut editor = lua_editor("hello\n");
    lua_ok(
        &editor,
        "rift.on('WinNew', function(ev) \
           for _, win in ipairs(rift.windows.list()) do \
             if win.id == ev.win and win.buf == ev.buf then _G.opened = ev.win end \
           end \
         end) \
         rift.on('WinClosed', function(ev) \
           for _, win in ipairs(rift.windows.list()) do \
             if win.id == ev.win then return end \
           end \
           _G.closed = ev.win \
         end)",
    );
    editor.execute_command_line("vsplit");
    let id = editor.split_tree.focused_window_id();
    assert!(editor.close_window(id));
    let id = id.get();
    lua_ok(
        &editor,
        &format!("assert(opened == {id} and closed == {id})"),
    );
}
