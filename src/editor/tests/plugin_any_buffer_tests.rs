use super::common::*;
use super::*;
use crate::annotations::kind::Kind;
use crate::annotations::{Anchor, Annotation, AnnotationOwner};
use crate::document::ops::{Edit, EditSource};
use crate::document::DocumentId;
use crate::test_utils::MockTerminal;

fn lua_ok(editor: &Editor<MockTerminal>, code: &str) {
    let err = editor.plugin_host.lua_exec(code);
    assert!(err.is_none(), "{err:?}");
}

fn doc(editor: &Editor<MockTerminal>, id: u64) -> &crate::document::Document {
    editor
        .document_manager
        .get_document(DocumentId::new(id))
        .unwrap()
}

fn doc_mut(editor: &mut Editor<MockTerminal>, id: u64) -> &mut crate::document::Document {
    editor
        .document_manager
        .get_document_mut(DocumentId::new(id))
        .unwrap()
}

fn text(editor: &Editor<MockTerminal>, id: u64) -> String {
    doc(editor, id).buffer().to_string()
}

fn sync(editor: &mut Editor<MockTerminal>) {
    editor.apply_plugin_mutations();
    editor.update_lua_state();
}

fn background_file() -> (tempfile::TempDir, Editor<MockTerminal>, u64, u64) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("first.txt");
    std::fs::write(&path, "alpha\nbeta").unwrap();
    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();
    let first = editor.active_document_id().get();
    lua_ok(&editor, "rift.create_scratch_buf('second', { 'second' })");
    sync(&mut editor);
    let second = editor.active_document_id().get();
    assert_ne!(first, second);
    (dir, editor, first, second)
}

fn lua_bool(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[test]
fn reads_target_a_background_buffer() {
    let (_dir, mut editor, first, second) = background_file();
    let first_doc = doc_mut(&mut editor, first);
    first_doc
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();
    first_doc
        .edit(Edit::type_text(">"), EditSource::User)
        .unwrap();
    first_doc
        .set_cursor(9, crate::document::ops::CursorCause::User)
        .unwrap();
    first_doc.set_option("tabwidth", Some("8")).unwrap();
    first_doc
        .set_option(
            "line_ending",
            Some(crate::document::LineEnding::Crlf.name()),
        )
        .unwrap();
    let mark = first_doc
        .annotations
        .add(Annotation::new(
            Kind::new("probe.mark"),
            Anchor::range(1, 4),
            AnnotationOwner::User,
        ))
        .get();
    let second_doc = doc(&editor, second);
    let (second_dirty, second_undo, second_kind, second_ending, second_tab) = (
        second_doc.is_dirty(),
        second_doc.history.can_undo(),
        second_doc.kind.kind_str().to_string(),
        second_doc.options().line_ending().name(),
        second_doc.options().tab_width(),
    );
    editor.update_lua_state();

    lua_ok(
        &editor,
        &format!(
            r"
            local b = {{ buf = {first} }}
            assert(rift.current_buf() == {second})
            assert(table.concat(rift.get_lines(1, -1, b), '|') == '>alpha|beta')
            assert(table.concat(rift.get_lines(1, -1), '|') == 'second')
            assert(rift.get_line_count(b) == 2 and rift.get_line_count() == 1)
            local hits = rift.search('beta', b)
            assert(#hits == 1 and hits[1].row == 2 and hits[1].col_start == 0)
            assert(#rift.search('beta') == 0)
            local row, col = rift.get_cursor(b)
            assert(row == 2 and col == 2, row .. ':' .. col)
            assert(rift.get_line_ending(b) == 'crlf' and rift.get_line_ending() == '{second_ending}')
            assert(rift.is_dirty(b) and rift.is_dirty() == {second_dirty})
            assert(rift.can_undo(b) and rift.can_undo() == {second_undo})
            assert(not rift.can_redo(b))
            assert(rift.get_filepath(b):match('first%.txt$'), rift.get_filepath(b))
            assert(rift.get_filepath() == nil)
            assert(rift.buf_kind(b) == 'file' and rift.buf_kind() == '{second_kind}')
            assert(rift.get_option('tab_width', b) == 8 and rift.get_option('tab_width') == {second_tab})
            assert(rift.annotations.get({mark}, b).kind == 'probe.mark')
            assert(rift.annotations.get({mark}) == nil)
            assert(#rift.annotations.at(2, b) == 1 and #rift.annotations.at(2) == 0)
            assert(#rift.annotations.in_range(0, 9, b) == 1)
            assert(#rift.annotations.by_kind('probe.', b) == 1)
            assert(rift.get_current_line(b) == 'beta' and rift.get_current_line() == 'second')
            assert(rift.get_word_at_cursor(b) == 'beta')
            ",
            second_dirty = lua_bool(second_dirty),
            second_undo = lua_bool(second_undo),
        ),
    );
}

#[test]
fn writes_target_a_background_buffer_and_read_back_in_the_same_call() {
    let (_dir, mut editor, first, second) = background_file();
    let second_text = text(&editor, second);
    let second_ending = doc(&editor, second).options().line_ending().name();
    let second_tab = doc(&editor, second).options().tab_width();

    lua_ok(
        &editor,
        &format!(
            r"
            local b = {{ buf = {first} }}
            rift.set_cursor(1, 0, b)
            rift.insert('>', b)
            rift.append_lines({{ '!' }}, b)
            rift.replace_lines(2, 2, {{ 'BETA!' }}, b)
            rift.set_cursor(2, 2, b)
            rift.delete_before(1, b)
            rift.delete_forward(1, b)
            rift.set_current_line('beta', b)
            rift.append_lines({{ '\ntrash' }}, b)
            rift.delete_current_line(b)
            rift.center_on_line(1, b)
            rift.set_line_ending('crlf', b)
            rift.set_option('tab_width', 8, b)
            _G.kept = rift.annotations.add{{ kind = 'probe.mark', line = 1, buf = {first} }}
            rift.annotations.update(_G.kept, {{ visible = false, buf = {first} }})
            local gone = rift.annotations.add{{ kind = 'probe.gone', line = 1, buf = {first} }}
            rift.annotations.remove(gone, b)
            rift.annotations.add{{ kind = 'probe.cleared', line = 1, buf = {first} }}
            rift.annotations.clear('probe.cleared', b)

            assert(table.concat(rift.get_lines(1, -1, b), '|') == '>alpha|beta')
            assert(rift.is_dirty(b) and rift.can_undo(b) and not rift.can_redo(b))
            local row, col = rift.get_cursor(b)
            assert(row == 1 and col == 0, row .. ':' .. col)
            assert(rift.get_line_ending(b) == 'crlf')
            assert(rift.get_option('tab_width', b) == 8)
            assert(rift.annotations.get(_G.kept, b).visible == false)
            assert(rift.annotations.get(gone, b) == nil)
            assert(#rift.annotations.by_kind('probe.cleared', b) == 0)

            assert(table.concat(rift.get_lines(1, -1), '|') == 'second')
            assert(rift.get_line_ending() == '{second_ending}')
            assert(rift.get_option('tab_width') == {second_tab})
            assert(rift.annotations.get(_G.kept) == nil)
            "
        ),
    );

    sync(&mut editor);

    assert_eq!(editor.active_document_id().get(), second);
    assert_eq!(text(&editor, first), ">alpha\nbeta");
    assert_eq!(text(&editor, second), second_text);
    let first_doc = doc(&editor, first);
    assert_eq!(first_doc.buffer().cursor(), 0);
    assert_eq!(first_doc.options().tab_width(), 8);
    assert_eq!(first_doc.options().line_ending().name(), "crlf");
    let probes = |id: u64| -> Vec<String> {
        doc(&editor, id)
            .annotations
            .iter()
            .map(|a| a.kind.as_arc().to_string())
            .filter(|kind| kind.starts_with("probe."))
            .collect()
    };
    assert_eq!(probes(first), vec!["probe.mark".to_string()]);
    assert!(probes(second).is_empty());
    assert_eq!(doc(&editor, second).options().tab_width(), second_tab);
    lua_ok(
        &editor,
        &format!(
            "assert(table.concat(rift.get_lines(1, -1, {{ buf = {first} }}), '|') == '>alpha|beta') \
             assert(rift.annotations.get(_G.kept, {{ buf = {first} }}).visible == false)"
        ),
    );
}

#[test]
fn unknown_buffer_id_raises_and_queues_nothing() {
    let (_dir, editor, _first, _second) = background_file();
    lua_ok(
        &editor,
        r"
        local bad = { buf = 999 }
        for _, call in ipairs({
            function() rift.get_lines(1, -1, bad) end,
            function() rift.get_cursor(bad) end,
            function() rift.get_option('tab_width', bad) end,
            function() rift.search('x', bad) end,
            function() rift.annotations.get(1, bad) end,
            function() rift.insert('x', bad) end,
            function() rift.set_cursor(1, 0, bad) end,
            function() rift.set_option('tab_width', 2, bad) end,
            function() rift.annotations.add{ kind = 'k', line = 1, buf = 999 } end,
            function() rift.save(bad) end,
            function() rift.close_buf(true, bad) end,
            function() rift.reload_buf(true, bad) end,
        }) do
            local ok, err = pcall(call)
            assert(not ok and tostring(err):find('no open buffer 999'), tostring(err))
        end
        ",
    );
    assert!(!editor.plugin_host.has_queued_mutations());
}

#[test]
fn insert_into_a_read_only_background_buffer_is_refused() {
    let (_dir, mut editor, first, second) = background_file();
    doc_mut(&mut editor, first).set_read_only(true);
    editor.update_lua_state();

    lua_ok(
        &editor,
        &format!(
            "rift.insert('x', {{ buf = {first} }}) \
             assert(rift.get_lines(1, 1, {{ buf = {first} }})[1] == 'alpha')"
        ),
    );
    sync(&mut editor);

    assert_eq!(editor.active_document_id().get(), second);
    assert_eq!(text(&editor, first), "alpha\nbeta");
}

#[test]
fn rejected_background_write_is_repaired_by_the_next_sync() {
    let (_dir, mut editor, first, _second) = background_file();
    doc_mut(&mut editor, first).set_read_only(true);

    lua_ok(
        &editor,
        &format!(
            "rift.set_cursor(1, 1, {{ buf = {first} }}) \
             rift.delete_forward(1, {{ buf = {first} }}) \
             assert(rift.get_lines(1, 1, {{ buf = {first} }})[1] == 'apha')"
        ),
    );
    sync(&mut editor);

    assert_eq!(text(&editor, first), "alpha\nbeta");
    lua_ok(
        &editor,
        &format!("assert(rift.get_lines(1, 1, {{ buf = {first} }})[1] == 'alpha')"),
    );
}

#[test]
fn set_buffer_var_without_buf_targets_the_buffer_current_when_queued() {
    let (_dir, mut editor, first, second) = background_file();

    lua_ok(
        &editor,
        &format!("rift.set_buffer_var(nil, 'probe', 'queued') rift.switch_buf({first})"),
    );
    sync(&mut editor);

    assert_eq!(
        editor.document_manager.active_document_id().unwrap().get(),
        first
    );
    assert!(doc(&editor, first).vars.get("probe").is_none());
    assert_eq!(
        doc(&editor, second).vars.get("probe"),
        Some(&crate::annotations::value::Value::Str("queued".to_string()))
    );
}

#[test]
fn saving_a_background_buffer_writes_its_file_without_switching_windows() {
    let (_dir, mut editor, first, second) = background_file();
    let path = doc(&editor, first).path().unwrap().to_path_buf();
    doc_mut(&mut editor, first)
        .edit(Edit::type_text("saved "), EditSource::User)
        .unwrap();
    editor.update_lua_state();
    lua_ok(&editor, &format!("rift.save({{ buf = {first} }})"));
    sync(&mut editor);
    drain_jobs(&mut editor);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "saved alpha\nbeta");
    assert_eq!(editor.active_document_id().get(), second);
    assert_eq!(text(&editor, second), "second");
}

#[test]
fn reloading_a_background_buffer_updates_only_its_document() {
    let (_dir, mut editor, first, second) = background_file();
    let path = doc(&editor, first).path().unwrap().to_path_buf();
    std::fs::write(path, "reloaded").unwrap();
    lua_ok(
        &editor,
        &format!("rift.reload_buf(true, {{ buf = {first} }})"),
    );
    sync(&mut editor);
    drain_jobs(&mut editor);
    assert_eq!(text(&editor, first), "reloaded");
    assert_eq!(text(&editor, second), "second");
    assert_eq!(editor.active_document_id().get(), second);
}

#[test]
fn closing_a_background_buffer_preserves_the_focused_window_and_cursor() {
    let (_dir, mut editor, first, second) = background_file();
    let win = editor.split_tree.focused_window_id();
    doc_mut(&mut editor, second)
        .set_cursor(3, crate::document::ops::CursorCause::User)
        .unwrap();
    lua_ok(
        &editor,
        &format!("rift.close_buf(true, {{ buf = {first} }})"),
    );
    sync(&mut editor);
    assert!(editor
        .document_manager
        .get_document(DocumentId::new(first))
        .is_none());
    assert_eq!(editor.split_tree.focused_window_id(), win);
    assert_eq!(editor.active_document_id().get(), second);
    assert_eq!(doc(&editor, second).buffer().cursor(), 3);
    assert_eq!(text(&editor, second), "second");
}

#[test]
fn highlight_queued_for_a_closed_buffer_logs_the_drop() {
    let (_dir, mut editor, first, _second) = background_file();
    lua_ok(
        &editor,
        &format!("rift.close_buf(true, {{ buf = {first} }}) rift.highlight.add('probe', {{1, 0, 1, 1}}, 'error', {{ buf = {first} }})"),
    );
    editor.apply_plugin_mutations();
    let expected = format!("plugin highlight.add skipped: buffer {first} is closed");
    assert!(editor.state.notifications.message_log().iter().any(
        |entry| matches!(entry, crate::notification::MessageEntry::Notification { message, .. } if message == &expected)
    ));
}

#[test]
fn created_buffers_return_ids_and_support_immediate_reads_and_edits() {
    let (_dir, mut editor, _first, _second) = background_file();
    lua_ok(
        &editor,
        "rift.register_buffer_kind('created', { on_close = function() end })",
    );
    sync(&mut editor);
    lua_ok(
        &editor,
        r"
        _G.created = rift.create_buffer('created', {
            title = 'created title', lines = { 'one', 'two' }, vars = { probe = 'value' }
        })
        assert(rift.current_buf() == _G.created)
        assert(rift.buf_kind() == 'created')
        assert(table.concat(rift.get_lines(1, -1), '|') == 'one|two')
        local row, col = rift.get_cursor()
        assert(row == 1 and col == 0)
        assert(rift.get_buffer_var(_G.created, 'probe') == 'value')
        _G.scratch = rift.create_scratch_buf('scratch title', { 'scratch' })
        assert(_G.scratch ~= _G.created and rift.current_buf() == _G.scratch)
        assert(rift.get_lines({ buf = _G.created })[2] == 'two')
        rift.insert('>', { buf = _G.created })
        assert(rift.get_lines(1, 1, { buf = _G.created })[1] == '>one')
        local count = 0
        for _, b in ipairs(rift.get_buf_list()) do
            if b.id == _G.created then assert(not b.is_current) count = count + 1 end
            if b.id == _G.scratch then assert(b.is_current) count = count + 1 end
        end
        assert(count == 2)
    ",
    );
    sync(&mut editor);
    let scratch = editor.active_document_id().get();
    lua_ok(&editor, &format!(
        "assert(_G.scratch == {scratch}) assert(rift.get_lines(1, 1, {{ buf = _G.created }})[1] == '>one')"
    ));
    assert_eq!(text(&editor, scratch), "scratch");
    let created = editor
        .document_manager
        .documents_iter()
        .find(|doc| doc.kind.kind_str() == "created")
        .unwrap();
    assert_eq!(created.buffer().to_string(), ">one\ntwo");
    lua_ok(
        &editor,
        &format!("assert(_G.created == {})", created.id.get()),
    );
}

#[test]
fn nested_buf_open_cannot_steal_a_queued_creations_id() {
    let (dir, mut editor, _first, _second) = background_file();
    let path = dir.path().join("nested.txt");
    std::fs::write(&path, "nested file").unwrap();
    let path_arg = serde_json::to_string(&path.to_string_lossy()).unwrap();
    lua_ok(
        &editor,
        &format!(
            r"
        rift.on('BufOpen', function()
            if not _G.opened then
                _G.opened = true
                rift.open_file({path_arg}, true)
            end
        end)
        _G.a = rift.create_scratch_buf('a', {{ 'alpha' }})
        _G.b = rift.create_scratch_buf('b', {{ 'beta' }})
    "
        ),
    );
    sync(&mut editor);
    drain_jobs(&mut editor);
    let nested = editor
        .document_manager
        .find_open_document_id(&path)
        .unwrap()
        .get();
    editor.update_lua_state();
    lua_ok(
        &editor,
        &format!(
            r"
        assert(_G.a ~= {nested} and _G.b ~= {nested} and _G.a ~= _G.b)
        assert(rift.get_lines(1, 1, {{ buf = _G.a }})[1] == 'alpha')
        assert(rift.get_lines(1, 1, {{ buf = _G.b }})[1] == 'beta')
    "
        ),
    );
    assert_eq!(text(&editor, nested), "nested file");
}

#[test]
fn ninth_buffer_creation_fails_without_losing_the_first_eight() {
    let (_dir, mut editor, _first, _second) = background_file();
    lua_ok(
        &editor,
        r"
        _G.ids = {}
        for i = 1, 8 do
            _G.ids[i] = rift.create_scratch_buf('pool', { tostring(i) })
        end
        local current = rift.current_buf()
        local ok, err = pcall(rift.create_scratch_buf, 'overflow', { 'bad' })
        assert(not ok and tostring(err):find('more than 8 buffers'), tostring(err))
        assert(rift.current_buf() == current)
    ",
    );
    sync(&mut editor);
    lua_ok(
        &editor,
        r"
        for i, id in ipairs(_G.ids) do
            assert(rift.get_lines(1, 1, { buf = id })[1] == tostring(i))
        end
        assert(type(rift.create_scratch_buf('refilled', { 'ok' })) == 'number')
    ",
    );
    sync(&mut editor);
    assert_eq!(editor.active_document().buffer().to_string(), "ok");
}
