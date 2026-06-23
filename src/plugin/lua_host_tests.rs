use super::*;
use crate::notification::NotificationType;
use crate::plugin::events::EditorEvent;
use std::sync::Arc;

fn make_host() -> LuaHost {
    LuaHost::new().expect("LuaHost::new failed")
}

#[test]
fn test_new_succeeds() {
    let _ = make_host();
}

#[test]
fn test_notify_queues_mutation() {
    let host = make_host();
    assert!(host.exec("rift.notify('info', 'hello')").is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::Notify { message, level } => {
            assert_eq!(message, "hello");
            assert_eq!(*level, NotificationType::Info);
        }
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_os_execute_is_disabled() {
    let host = make_host();
    let err = host.exec("os.execute('echo hi')");
    assert!(err.is_some(), "os.execute() must raise a Lua error");
    assert!(err.unwrap().contains("spawn_shell"));
}

#[test]
fn test_annotations_add_queues_mutation() {
    let host = make_host();
    assert!(host
        .exec(
            r#"rift.annotations.add{ kind="ui.checkbox", line=3, payload={checked=false},
                actions={ {verb="toggle", default=true} } }"#
        )
        .is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::AddAnnotation {
            kind,
            anchor,
            payload,
            actions,
            ..
        } => {
            assert_eq!(kind, "ui.checkbox");
            assert_eq!(*anchor, crate::plugin::AnnotationAnchorSpec::Line(3));
            assert_eq!(
                payload.get("checked"),
                Some(&crate::annotations::Value::Bool(false))
            );
            assert_eq!(actions, &vec![("toggle".to_string(), true)]);
        }
        _ => panic!("expected AddAnnotation"),
    }
}

#[test]
fn test_annotations_add_returns_preclaimed_id() {
    let host = make_host();
    // Seed the next id the snapshot reports; add{} claims from there.
    host.set_annotations(vec![], 7);
    assert!(host
        .exec(
            r#"
            _G.id1 = rift.annotations.add{ kind = "md.x", point = 0 }
            _G.id2 = rift.annotations.add{ kind = "md.y", point = 1 }
        "#
        )
        .is_none());
    assert!(host.exec("assert(_G.id1 == 7, 'first id')").is_none());
    assert!(host.exec("assert(_G.id2 == 8, 'second id')").is_none());
    let muts = host.drain_mutations();
    assert!(matches!(
        &muts[0],
        PluginMutation::AddAnnotation { id: 7, .. }
    ));
    assert!(matches!(
        &muts[1],
        PluginMutation::AddAnnotation { id: 8, .. }
    ));
}

#[test]
fn test_annotations_query_reads_snapshot() {
    let host = make_host();
    let mut payload = crate::annotations::Value::map();
    payload.set("checked", crate::annotations::Value::Bool(true));
    host.set_annotations(
        vec![
            AnnotationView {
                id: 1,
                kind: "ui.checkbox".into(),
                owner: "plugin".into(),
                anchor: "range",
                start: 2,
                end: 5,
                payload,
                visible: true,
                interactive: true,
            },
            AnnotationView {
                id: 2,
                kind: "md.link".into(),
                owner: "plugin".into(),
                anchor: "point",
                start: 10,
                end: 10,
                payload: crate::annotations::Value::Null,
                visible: true,
                interactive: false,
            },
        ],
        3,
    );
    assert!(host
        .exec(
            r#"
            local a = rift.annotations.get(1)
            assert(a and a.kind == "ui.checkbox", "get by id")
            assert(a.payload.checked == true, "payload round-trips")
            assert(a.start == 2 and a["end"] == 5, "range offsets")
            assert(rift.annotations.get(99) == nil, "missing id -> nil")
            assert(#rift.annotations.at(3) == 1, "covered by range")
            assert(#rift.annotations.at(7) == 0, "uncovered offset")
            assert(#rift.annotations.at(10) == 1, "point at offset")
            assert(#rift.annotations.in_range(0, 6) == 1, "range overlap")
            assert(#rift.annotations.in_range(0, 20) == 2, "both in range")
            assert(#rift.annotations.by_kind("md.") == 1, "by kind prefix")
            assert(#rift.annotations.by_kind("ui.") == 1, "other prefix")
        "#
        )
        .is_none());
}

#[test]
fn test_annotations_update_queues_mutation() {
    let host = make_host();
    assert!(host
        .exec(r#"rift.annotations.update(5, { visible = false, payload = { n = 1 } })"#)
        .is_none());
    let muts = host.drain_mutations();
    assert_eq!(muts.len(), 1);
    match &muts[0] {
        PluginMutation::UpdateAnnotation {
            id,
            payload,
            visible,
            ..
        } => {
            assert_eq!(*id, 5);
            assert_eq!(*visible, Some(false));
            assert!(payload.is_some());
        }
        _ => panic!("expected UpdateAnnotation"),
    }
}

#[test]
fn test_annotations_clear_queues_mutation() {
    let host = make_host();
    assert!(host.exec(r#"rift.annotations.clear("md.")"#).is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::ClearAnnotations { kind_prefix } => assert_eq!(kind_prefix, "md."),
        _ => panic!("expected ClearAnnotations"),
    }
}

#[test]
fn test_annotations_on_action_registers_and_invokes() {
    let host = make_host();
    assert!(host
        .exec(
            r#"
            _G.ran_verb = nil
            rift.annotations.on_action("test.runnable", "run", function(ctx)
                _G.ran_verb = ctx.verb
                _G.ran_cmd = ctx.payload.cmd
                _G.ran_param = ctx.params.scope
            end)
        "#
        )
        .is_none());
    let mutations = host.drain_mutations();
    assert!(mutations.iter().any(|m| matches!(
        m,
        PluginMutation::RegisterAnnotationAction { kind, verb, .. }
            if kind == "test.runnable" && verb == "run"
    )));

    let mut payload = crate::annotations::Value::map();
    payload.set(
        "cmd",
        crate::annotations::Value::Str("cargo test foo".into()),
    );
    let mut params = crate::annotations::Value::map();
    params.set("scope", crate::annotations::Value::Str("file".into()));
    let ctx = crate::plugin::AnnotationActionCtx {
        annotation_id: 7,
        kind: "test.runnable".into(),
        verb: "run".into(),
        payload,
        params,
        position: 0,
        buffer: 1,
    };
    assert!(host.invoke_annotation_action(&ctx));
    assert!(host.exec("assert(_G.ran_verb == 'run')").is_none());
    assert!(host
        .exec("assert(_G.ran_cmd == 'cargo test foo')")
        .is_none());
    // The action's serializable params reach the handler (design.md sec 9.1).
    assert!(host.exec("assert(_G.ran_param == 'file')").is_none());
}

#[test]
fn test_annotations_register_kind_queues_mutation() {
    let host = make_host();
    assert!(host
        .exec(
            r#"rift.annotations.register_kind("vcs.hunk", {
                face = "diff.added",
                style = { underline = true },
                description = "a staged hunk",
            })"#
        )
        .is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::RegisterKindDefaults {
            kind,
            presentation,
            description,
        } => {
            assert_eq!(kind, "vcs.hunk");
            assert_eq!(description.as_deref(), Some("a staged hunk"));
            let pres = presentation.as_ref().expect("presentation built");
            assert_eq!(pres.face.as_ref().map(|f| f.0.as_str()), Some("diff.added"));
            assert!(pres.style.as_ref().map(|s| s.underline).unwrap_or(false));
        }
        _ => panic!("expected RegisterKindDefaults"),
    }
}

#[test]
fn test_annotations_enter_leave_hooks_invoke() {
    let host = make_host();
    assert!(host
        .exec(
            r#"
            _G.entered = nil
            _G.left = nil
            rift.annotations.on_enter("ui.link", function(ctx)
                _G.entered = ctx.annotation_id
                _G.entered_href = ctx.payload.href
            end)
            rift.annotations.on_leave("ui.link", function(ctx)
                _G.left = ctx.annotation_id
            end)
        "#
        )
        .is_none());

    let mut payload = crate::annotations::Value::map();
    payload.set("href", crate::annotations::Value::Str("docs.md".into()));
    let ctx = crate::plugin::AnnotationHoverCtx {
        annotation_id: 42,
        kind: "ui.link".into(),
        payload,
        position: 3,
        buffer: 1,
    };
    assert!(host.invoke_annotation_hook(true, &ctx));
    assert!(host.exec("assert(_G.entered == 42)").is_none());
    assert!(host.exec("assert(_G.entered_href == 'docs.md')").is_none());
    assert!(host.invoke_annotation_hook(false, &ctx));
    assert!(host.exec("assert(_G.left == 42)").is_none());

    // A kind with no registered hook does nothing.
    let other = crate::plugin::AnnotationHoverCtx {
        annotation_id: 1,
        kind: "ui.button".into(),
        payload: crate::annotations::Value::Null,
        position: 0,
        buffer: 1,
    };
    assert!(!host.invoke_annotation_hook(true, &other));
}

#[test]
fn test_append_lines_queues_mutation() {
    let host = make_host();
    assert!(host.exec("rift.append_lines({'line1', 'line2'})").is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::AppendLines(lines) => {
            assert_eq!(lines, &vec!["line1".to_string(), "line2".to_string()]);
        }
        _ => panic!("expected AppendLines"),
    }
}

#[test]
fn test_open_float_queues_mutation() {
    let host = make_host();
    assert!(host
        .exec("rift.open_float('My Float', {'line a', 'line b'})")
        .is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::OpenFloat(f) => {
            assert_eq!(f.title, "My Float");
            assert_eq!(f.lines, vec!["line a", "line b"]);
        }
        _ => panic!("expected OpenFloat"),
    }
}

#[test]
fn test_close_float_queues_mutation() {
    let host = make_host();
    assert!(host.exec("rift.close_float()").is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    assert!(matches!(mutations[0], PluginMutation::CloseFloat));
}

#[test]
fn test_on_and_dispatch_event() {
    let host = make_host();
    assert!(host
        .exec("rift.on('EditorStart', function(_ev) rift.notify('info', 'started') end)")
        .is_none());
    let errors = host.dispatch_event(&EditorEvent::EditorStart);
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "started"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_get_lines_returns_correct_lines() {
    let host = make_host();
    host.update_state(
        1,
        "file".to_string(),
        Arc::new(vec![
            "alpha".to_string(),
            "beta".to_string(),
            "gamma".to_string(),
        ]),
        (0, 0),
        4,
        true,
        "normal",
        None,
        None,
        vec![],
        (0, 0),
        false,
        false,
        false,
        (0, 0),
        "lf",
        vec![],
        vec![],
        0,
        None,
        std::collections::HashMap::new(),
    );
    assert!(host.exec("_lines = rift.get_lines(1, -1)").is_none());
    assert!(host.exec("rift.notify('info', _lines[2])").is_none());
    let mutations = host.drain_mutations();
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "beta"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_get_cursor_returns_1indexed_row() {
    let host = make_host();
    host.update_state(
        1,
        "file".to_string(),
        Arc::new(vec![]),
        (4, 2),
        4,
        true,
        "normal",
        None,
        None,
        vec![],
        (0, 0),
        false,
        false,
        false,
        (0, 0),
        "lf",
        vec![],
        vec![],
        0,
        None,
        std::collections::HashMap::new(),
    );
    assert!(host
        .exec("local r, c = rift.get_cursor(); rift.notify('info', tostring(r))")
        .is_none());
    let mutations = host.drain_mutations();
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "5"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_current_buf_returns_id() {
    let host = make_host();
    host.update_state(
        42,
        "file".to_string(),
        Arc::new(vec![]),
        (0, 0),
        4,
        true,
        "normal",
        None,
        None,
        vec![],
        (0, 0),
        false,
        false,
        false,
        (0, 0),
        "lf",
        vec![],
        vec![],
        0,
        None,
        std::collections::HashMap::new(),
    );
    assert!(host
        .exec("rift.notify('info', tostring(rift.current_buf()))")
        .is_none());
    let mutations = host.drain_mutations();
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "42"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_exec_returns_none_on_success() {
    let host = make_host();
    assert!(host.exec("local x = 1 + 1").is_none());
}

#[test]
fn test_exec_returns_some_on_bad_lua() {
    let host = make_host();
    let err = host.exec("this is not valid lua @@@@");
    assert!(err.is_some(), "expected Some(err) for bad Lua");
}

#[test]
fn test_load_dir_loads_lua_file() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let plugin_path = dir.path().join("test_plugin.lua");
    let mut f = std::fs::File::create(&plugin_path).expect("create failed");
    writeln!(f, "rift.notify('info', 'plugin loaded')").expect("write failed");
    drop(f);

    let host = make_host();
    let errors = host.load_dir(dir.path());
    assert!(errors.is_empty(), "load errors: {:?}", errors);
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "plugin loaded"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_error_in_handler_returned_from_dispatch() {
    let host = make_host();
    assert!(host
        .exec("rift.on('EditorStart', function(_ev) error('handler error') end)")
        .is_none());
    let errors = host.dispatch_event(&EditorEvent::EditorStart);
    assert!(!errors.is_empty(), "expected errors from bad handler");
    assert!(errors[0].contains("handler error"), "error: {}", errors[0]);
}

#[test]
fn test_insert_queues_mutation() {
    let host = make_host();
    assert!(host.exec("rift.insert('hello')").is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::InsertAtCursor(text) => assert_eq!(text, "hello"),
        _ => panic!("expected InsertAtCursor"),
    }
}

#[test]
fn test_get_tab_width_default() {
    let host = make_host();
    assert!(host.exec("assert(rift.get_tab_width() == 4)").is_none());
}

#[test]
fn test_get_expand_tabs_default() {
    let host = make_host();
    assert!(host
        .exec("assert(rift.get_expand_tabs() == true)")
        .is_none());
}

#[test]
fn test_get_mode_default() {
    let host = make_host();
    assert!(host.exec("assert(rift.get_mode() == 'normal')").is_none());
}

// --- Ownership tracking tests ---

#[test]
fn test_load_file_sets_current_plugin_during_exec() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("myplugin.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    // During load, current_plugin should be the file path; capture it via notify.
    writeln!(
        f,
        "rift.on('EditorStart', function(_) rift.notify('info', 'handler') end)"
    )
    .unwrap();
    drop(f);

    let host = make_host();
    let errors = host.load_file(&path);
    assert!(errors.is_none(), "load error: {:?}", errors);

    // _rift_plugin_slots should have an entry for the plugin file path.
    let check = host.exec(
        r#"
        local found = false
        for name, slots in pairs(_rift_plugin_slots) do
            if #slots > 0 then found = true end
        end
        assert(found, "no plugin slot entries recorded")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_on_handler_tagged_with_plugin() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("tagging.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    writeln!(f, "rift.on('EditorStart', function(_) end)").unwrap();
    drop(f);

    let host = make_host();
    host.load_file(&path);

    // The handler entry in _rift_handlers should carry a plugin field.
    let check = host.exec(
        r#"
        local list = _rift_handlers["EditorStart"]
        assert(list and #list == 1, "expected one handler")
        assert(list[1].plugin ~= nil, "handler missing plugin field")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_register_command_tagged_with_plugin() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("cmdplugin.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    writeln!(
        f,
        "rift.register_command('MyCmd', function() end, 'does stuff')"
    )
    .unwrap();
    drop(f);

    let host = make_host();
    host.load_file(&path);

    let check = host.exec(
        r#"
        local entry = _rift_commands["MyCmd"]
        assert(entry ~= nil, "command not registered")
        assert(entry.plugin ~= nil, "command missing plugin field")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_register_action_tagged_with_plugin() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("actplugin.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    writeln!(f, "rift.register_action('my:action', function() end)").unwrap();
    drop(f);

    let host = make_host();
    host.load_file(&path);

    let check = host.exec(
        r#"
        local entry = _rift_actions["my:action"]
        assert(type(entry) == "table", "action should be table, got " .. type(entry))
        assert(type(entry.fn) == "function", "action.fn should be function")
        assert(entry.plugin ~= nil, "action missing plugin field")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_execute_action_still_works_after_ownership_change() {
    let host = make_host();
    // register_action now stores {fn, plugin} — execute_action must unwrap correctly.
    assert!(host
        .exec("rift.register_action('ping', function() rift.notify('info', 'pong') end)")
        .is_none());
    let found = host.execute_action("ping");
    assert!(found, "execute_action returned false");
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "pong"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_map_keymap_tracked_in_plugin_keymaps() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("mapplugin.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    writeln!(f, "rift.map('n', '<leader>x', 'editor:save')").unwrap();
    drop(f);

    let host = make_host();
    host.load_file(&path);

    let check = host.exec(
        r#"
        local found = false
        for name, keys in pairs(_rift_plugin_keymaps) do
            for _, k in ipairs(keys) do
                if k.mode == "n" and k.keys == "<leader>x" then found = true end
            end
        end
        assert(found, "keymap not recorded in _rift_plugin_keymaps")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_plugins_list_returns_loaded_plugin_names() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("listed.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    writeln!(f, "rift.on('EditorStart', function(_) end)").unwrap();
    drop(f);

    let host = make_host();
    host.load_file(&path);

    let check = host.exec(
        r#"
        local list = rift.plugins.list()
        assert(#list >= 1, "expected at least one plugin in list")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_plugins_info_returns_correct_counts() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("infoplugin.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    writeln!(f, "rift.on('EditorStart', function(_) end)").unwrap();
    writeln!(f, "rift.register_command('InfoCmd', function() end)").unwrap();
    writeln!(f, "rift.register_action('info:act', function() end)").unwrap();
    writeln!(f, "rift.map('n', 'zi', 'editor:save')").unwrap();
    drop(f);

    let host = make_host();
    host.load_file(&path);

    // Get the plugin name (the file path) from the list, then check info.
    let check = host.exec(
        r#"
        local names = rift.plugins.list()
        assert(#names == 1, "expected exactly one plugin")
        local info = rift.plugins.info(names[1])
        assert(#info.handlers == 1, "expected 1 handler, got " .. #info.handlers)
        assert(#info.commands == 1, "expected 1 command, got " .. #info.commands)
        assert(#info.actions  == 1, "expected 1 action, got " .. #info.actions)
        assert(#info.keys     == 1, "expected 1 keymap, got " .. #info.keys)
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_plugins_unload_removes_all_registrations() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("unloadme.lua");
    let mut f = std::fs::File::create(&path).expect("create failed");
    writeln!(f, "rift.on('EditorStart', function(_) end)").unwrap();
    writeln!(f, "rift.register_command('DropCmd', function() end)").unwrap();
    writeln!(f, "rift.register_action('drop:act', function() end)").unwrap();
    writeln!(f, "rift.map('n', 'zq', 'editor:save')").unwrap();
    drop(f);

    let host = make_host();
    host.load_file(&path);

    let check = host.exec(
        r#"
        local names = rift.plugins.list()
        assert(#names == 1, "expected one plugin before unload")
        local name = names[1]
        rift.plugins.unload(name)
        local info = rift.plugins.info(name)
        assert(#info.handlers == 0, "handlers not cleared: " .. #info.handlers)
        assert(#info.commands == 0, "commands not cleared: " .. #info.commands)
        assert(#info.actions  == 0, "actions not cleared: " .. #info.actions)
        assert(#info.keys     == 0, "keys not cleared: " .. #info.keys)
        -- _rift_plugin_slots entry should be gone
        assert(_rift_plugin_slots[name] == nil, "plugin_slots not cleared")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_no_plugin_tag_when_exec_direct() {
    // Handlers registered via rift.exec (not load_file) should have no plugin field.
    let host = make_host();
    assert!(host
        .exec("rift.on('EditorStart', function(_) end)")
        .is_none());

    let check = host.exec(
        r#"
        local list = _rift_handlers["EditorStart"]
        assert(list and #list == 1, "expected one handler")
        assert(list[1].plugin == nil, "inline handler should have no plugin field")
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_load_dir_ownership_tracks_each_file_separately() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir failed");

    let path_a = dir.path().join("a_plugin.lua");
    let mut f = std::fs::File::create(&path_a).expect("create failed");
    writeln!(f, "rift.on('EditorStart', function(_) end)").unwrap();
    drop(f);

    let path_b = dir.path().join("b_plugin.lua");
    let mut f = std::fs::File::create(&path_b).expect("create failed");
    writeln!(f, "rift.register_command('BCmd', function() end)").unwrap();
    drop(f);

    let host = make_host();
    let errors = host.load_dir(dir.path());
    assert!(errors.is_empty(), "load errors: {:?}", errors);

    let check = host.exec(
        r#"
        local names = rift.plugins.list()
        assert(#names == 2, "expected 2 plugins, got " .. #names)
        -- Each plugin owns exactly what it registered.
        local info_a = rift.plugins.info(names[1])
        local info_b = rift.plugins.info(names[2])
        -- One plugin has 1 handler, the other has 1 command (order may vary by sort).
        local handlers_total = #info_a.handlers + #info_b.handlers
        local commands_total = #info_a.commands + #info_b.commands
        assert(handlers_total == 1, "expected 1 handler total, got " .. handlers_total)
        assert(commands_total == 1, "expected 1 command total, got " .. commands_total)
    "#,
    );
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_emit_does_not_invoke_handlers_registered_during_same_pass() {
    // A handler that re-arms by registering another handler for the same
    // event must not have that new handler invoked within the same emit.
    let host = make_host();
    assert!(host.exec("_G.count = 0").is_none());
    assert!(host
        .exec(
            r#"
        rift.on('UserEvent', function(ev)
            _G.count = _G.count + 1
            rift.on('UserEvent', function(_ev) _G.count = _G.count + 1 end)
        end)
    "#
        )
        .is_none());
    assert!(host.exec("rift.emit('Ping')").is_none());
    let check = host.exec("assert(_G.count == 1, 'count was ' .. _G.count)");
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_dispatch_event_does_not_invoke_handlers_registered_during_same_pass() {
    let host = make_host();
    assert!(host.exec("_G.count = 0").is_none());
    assert!(host
        .exec(
            r#"
        rift.on('EditorStart', function(_ev)
            _G.count = _G.count + 1
            rift.on('EditorStart', function(_ev) _G.count = _G.count + 1 end)
        end)
    "#
        )
        .is_none());
    let errors = host.dispatch_event(&EditorEvent::EditorStart);
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    let check = host.exec("assert(_G.count == 1, 'count was ' .. _G.count)");
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_shelldone_fanout_does_not_invoke_handlers_registered_during_same_pass() {
    let host = make_host();
    assert!(host.exec("_G.count = 0").is_none());
    assert!(host
        .exec(
            r#"
        rift.on('UserEvent', function(ev)
            _G.count = _G.count + 1
            rift.on('UserEvent', function(_ev) _G.count = _G.count + 1 end)
        end)
    "#
        )
        .is_none());
    host.shared.lock().unwrap().pending_shell_events.push((
        "tag1".to_string(),
        true,
        "out".to_string(),
    ));
    let _ = host.drain_mutations();
    let check = host.exec("assert(_G.count == 1, 'count was ' .. _G.count)");
    assert!(check.is_none(), "assertion failed: {:?}", check);
}

#[test]
fn test_current_slot_restored_after_reentrant_emit() {
    let host = make_host();
    // Handler B is registered for a UserEvent and adds a highlight of its own
    // when fired reentrantly from inside handler A.
    assert!(host
        .exec(
            r#"
        _G.b_slot = rift.on('UserEvent', function(_ev)
            rift.add_highlight(2, 0, 2, 1, "blue")
        end)
    "#
        )
        .is_none());
    // Handler A emits the UserEvent (running B reentrantly), then adds its
    // own highlight afterward. Each highlight must be tagged with its own
    // handler's slot, not leak into the other.
    assert!(host
        .exec(
            r#"
        _G.a_slot = rift.on('EditorStart', function(_ev)
            rift.emit('Reentrant')
            rift.add_highlight(1, 0, 1, 1, "red")
        end)
    "#
        )
        .is_none());

    let errors = host.dispatch_event(&EditorEvent::EditorStart);
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    let mutations = host.drain_mutations();
    let slots: Vec<u32> = mutations
        .iter()
        .filter_map(|m| match m {
            PluginMutation::AddHighlight { slot, .. } => Some(*slot),
            _ => None,
        })
        .collect();
    assert_eq!(slots.len(), 2, "expected two AddHighlight mutations");
    let b_highlight_slot = slots[0];
    let a_highlight_slot = slots[1];

    let check = host.exec(&format!(
        r#"
        assert({} == _G.b_slot, "B's highlight should be tagged with B's slot")
        assert({} == _G.a_slot, "A's highlight should be tagged with A's slot")
    "#,
        b_highlight_slot, a_highlight_slot
    ));
    assert!(check.is_none(), "{:?}", check);
}
