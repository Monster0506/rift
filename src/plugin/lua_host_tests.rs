use super::*;
use crate::notification::NotificationType;
use crate::plugin::events::EventPayload;

fn make_host() -> LuaHost {
    let host = LuaHost::new().expect("LuaHost::new failed");
    host.update_state(StateUpdate {
        creation_ids: (100..108).collect(),
        mode: "normal",
        ..Default::default()
    });
    host
}

fn text_update(lines: &[&str]) -> BufUpdate {
    let mut buffer = crate::buffer::TextBuffer::default();
    crate::document::ops::Edit::type_text(lines.join("\n")).apply_to(&mut buffer);
    BufUpdate {
        text: Some(buffer),
        ..Default::default()
    }
}

fn buf_1(update: BufUpdate) -> Option<Vec<(u64, BufUpdate)>> {
    Some(vec![(1, update)])
}

fn set_annotations(host: &LuaHost, views: Vec<AnnotationView>, next_id: u64) {
    host.update_state(StateUpdate {
        buf_id: 1,
        bufs: buf_1(BufUpdate {
            annotations: Some(BufAnnotations { views, next_id }),
            ..Default::default()
        }),
        ..Default::default()
    });
}

fn open_bufs(host: &LuaHost, ids: &[u64]) {
    host.update_state(StateUpdate {
        buf_id: 1,
        bufs: Some(ids.iter().map(|&id| (id, BufUpdate::default())).collect()),
        ..Default::default()
    });
}

fn load_plugin(host: &LuaHost, file: &str, src: &str) {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join(file);
    std::fs::write(&path, src).expect("write failed");
    let err = host.load_file(&path);
    assert!(err.is_none(), "load error: {err:?}");
}

#[test]
fn test_new_succeeds() {
    let _ = make_host();
}

#[test]
fn test_set_option_queues_mutation_with_string_value() {
    let host = make_host();
    assert!(host.exec("rift.set_option('ghostcut', true)").is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::SetOption {
            buf: None,
            name,
            value,
        } => {
            assert_eq!(name, "ghostcut");
            assert_eq!(value, "true");
        }
        _ => panic!("expected SetOption"),
    }
}

#[test]
fn test_get_option_reads_document_local_setting_via_alias() {
    let host = make_host();
    let mut doc_options = crate::document::definitions::DocumentOptions::default();
    crate::document::definitions::create_document_settings_registry()
        .set("tabwidth", Some("8"), &mut doc_options)
        .unwrap();
    host.update_state(StateUpdate {
        buf_id: 1,
        bufs: buf_1(BufUpdate {
            options: Some(doc_options),
            ..Default::default()
        }),
        mode: "normal",
        ..Default::default()
    });
    assert!(host
        .exec(
            "rift.notify('info', type(rift.get_option('tab_width')) \
             .. ':' .. tostring(rift.get_option('tab_width')))"
        )
        .is_none());
    let mutations = host.drain_mutations();
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "number:8"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_get_option_falls_back_to_global_setting() {
    let host = make_host();
    let mut user_settings = crate::state::UserSettings::new();
    crate::state::definitions::create_settings_registry()
        .set("clipboard.size", Some("42"), &mut user_settings)
        .unwrap();
    host.update_state(StateUpdate {
        buf_id: 1,
        mode: "normal",
        user_settings: Some(user_settings),
        ..Default::default()
    });
    assert!(host
        .exec("rift.notify('info', tostring(rift.get_option('clipboard.size')))")
        .is_none());
    let mutations = host.drain_mutations();
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "42"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn test_get_option_unknown_name_returns_nil() {
    let host = make_host();
    assert!(host
        .exec("assert(rift.get_option('not_a_real_setting') == nil)")
        .is_none());
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
    assert!(err.unwrap().contains("rift.job.start"));
}

#[test]
fn test_unsafe_stdlib_entries_are_nil() {
    let host = make_host();
    let check = host.exec(
        r"
        assert(debug == nil, 'debug')
        assert(io.popen == nil, 'io.popen')
        assert(os.exit == nil, 'os.exit')
        assert(os.remove == nil, 'os.remove')
        assert(os.rename == nil, 'os.rename')
        assert(os.tmpname == nil, 'os.tmpname')
        assert(package.loadlib == nil, 'package.loadlib')
        assert(package.cpath == nil, 'package.cpath')
        assert(#package.searchers == 2, 'searchers: ' .. #package.searchers)
    ",
    );
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_runaway_handler_cannot_remove_the_budget_hook() {
    let host = make_host();
    load_plugin(
        &host,
        "runaway.lua",
        r"
        rift.on('TextChangedCoarse', function(_ev)
            pcall(function() debug.sethook() end)
            while true do end
        end)
    ",
    );
    let started = std::time::Instant::now();
    let errors = host.dispatch_event(&EventPayload::TextChangedCoarse {
        buf: crate::document::DocumentId::new(1),
    });
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("time budget"), "{:?}", errors[0]);
}

#[test]
fn test_fs_read_works_with_restricted_stdlib() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("data.txt");
    std::fs::write(&path, "hello fs").expect("write failed");
    let host = make_host();
    let path = path.to_string_lossy().replace('\\', "/");
    let check = host.exec(&format!(
        "assert(rift.fs.exists({path:?}))
         assert(rift.fs.read({path:?}) == 'hello fs')"
    ));
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_require_finds_module_on_added_package_path() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let lib = dir.path().join("it's]]lib");
    std::fs::create_dir(&lib).expect("mkdir failed");
    std::fs::write(lib.join("isolation_mod.lua"), "return { answer = 42 }").expect("write failed");
    let host = make_host();
    let lib = lib.to_string_lossy().replace('\\', "/");
    let check = host.exec(&format!(
        "rift.add_package_path({lib:?})
         assert(require('isolation_mod').answer == 42)"
    ));
    assert!(check.is_none(), "assertion failed: {check:?}");
}

fn notified_messages(host: &LuaHost) -> Vec<String> {
    host.drain_mutations()
        .into_iter()
        .filter_map(|m| match m {
            PluginMutation::Notify { message, .. } => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn test_plugins_keep_their_own_globals() {
    let host = make_host();
    for name in ["a", "b"] {
        load_plugin(
            &host,
            &format!("{name}.lua"),
            &format!(
                "M = {{ name = '{name}' }}
                 function setup() return M.name end
                 rift.register_command('{name}check', function() rift.notify('info', setup()) end)"
            ),
        );
    }
    assert!(host.execute_command("acheck", &[]));
    assert!(host.execute_command("bcheck", &[]));
    assert_eq!(notified_messages(&host), ["a", "b"]);
}

#[test]
fn test_handler_reads_its_plugin_global_later() {
    let host = make_host();
    load_plugin(
        &host,
        "counter.lua",
        "count = 0
         rift.on('EditorStart', function(_ev)
             count = count + 1
             rift.notify('info', tostring(count))
         end)",
    );
    assert!(host.dispatch_event(&EventPayload::EditorStart).is_empty());
    assert!(host.dispatch_event(&EventPayload::EditorStart).is_empty());
    assert_eq!(notified_messages(&host), ["1", "2"]);
}

#[test]
fn test_exec_cannot_see_plugin_globals() {
    let host = make_host();
    load_plugin(&host, "hidden.lua", "M = {} function setup() end");
    let check = host.exec(
        "assert(M == nil and setup == nil, 'plugin globals leaked')
         assert(_rift_current_plugin_file == nil, 'plugin file name leaked')",
    );
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_plugin_sees_its_own_file_name() {
    let host = make_host();
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("whoami.lua");
    std::fs::write(
        &path,
        "local loaded_as = _rift_current_plugin_file
         rift.register_command('whoami', function()
             rift.notify('info', loaded_as .. '|' .. _rift_current_plugin_file)
         end)",
    )
    .expect("write failed");
    assert!(host.load_file(&path).is_none());
    assert!(host.execute_command("whoami", &[]));
    let name = path.to_string_lossy().replace('\\', "/");
    assert_eq!(notified_messages(&host), [format!("{name}|{name}")]);
}

#[test]
fn test_module_required_during_load_sees_the_loading_file() {
    let host = make_host();
    let dir = tempfile::tempdir().expect("tempdir failed");
    std::fs::write(
        dir.path().join("whereami.lua"),
        "return _rift_current_plugin_file",
    )
    .expect("write failed");
    let path = dir.path().join("loader.lua");
    let lib = dir.path().to_string_lossy().replace('\\', "/");
    std::fs::write(
        &path,
        format!(
            "rift.add_package_path({lib:?})
             rift.notify('info', require('whereami'))"
        ),
    )
    .expect("write failed");
    assert!(host.load_file(&path).is_none());
    let name = path.to_string_lossy().replace('\\', "/");
    assert_eq!(notified_messages(&host), [name]);
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
                Some(&crate::annotations::value::Value::Bool(false))
            );
            assert_eq!(actions, &vec![("toggle".to_string(), true)]);
        }
        _ => panic!("expected AddAnnotation"),
    }
}

#[test]
fn test_annotations_add_returns_preclaimed_id() {
    let host = make_host();
    set_annotations(&host, vec![], 7);
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
    let mut payload = crate::annotations::value::Value::map();
    payload.set("checked", crate::annotations::value::Value::Bool(true));
    set_annotations(
        &host,
        vec![
            AnnotationView {
                id: 1,
                kind: "ui.checkbox".into(),
                owner: "plugin",
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
                owner: "plugin",
                anchor: "point",
                start: 10,
                end: 10,
                payload: crate::annotations::value::Value::Null,
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
            assert(a.owner == "plugin", "owner tag round-trips")
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
        .exec(r"rift.annotations.update(5, { visible = false, payload = { n = 1 } })")
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
        PluginMutation::ClearAnnotations { kind_prefix, .. } => assert_eq!(kind_prefix, "md."),
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

    let mut payload = crate::annotations::value::Value::map();
    payload.set(
        "cmd",
        crate::annotations::value::Value::Str("cargo test foo".into()),
    );
    let mut params = crate::annotations::value::Value::map();
    params.set(
        "scope",
        crate::annotations::value::Value::Str("file".into()),
    );
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
    // The action's serializable params reach the handler.
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
            ..
        } => {
            assert_eq!(kind, "vcs.hunk");
            assert_eq!(description.as_deref(), Some("a staged hunk"));
            let pres = presentation.as_ref().expect("presentation built");
            assert_eq!(pres.face.as_ref().map(|f| f.0.as_str()), Some("diff.added"));
            assert!(pres.style.as_ref().is_some_and(|s| s.attrs.is_underline()));
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

    let mut payload = crate::annotations::value::Value::map();
    payload.set(
        "href",
        crate::annotations::value::Value::Str("docs.md".into()),
    );
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
        payload: crate::annotations::value::Value::Null,
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
        PluginMutation::AppendLines { lines, .. } => {
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
            assert_eq!(f.title(), "My Float");
            assert_eq!(f.lines(), ["line a", "line b"]);
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
    let errors = host.dispatch_event(&EventPayload::EditorStart);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
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
    host.update_state(StateUpdate {
        buf_id: 1,
        bufs: buf_1(text_update(&["alpha", "beta", "gamma"])),
        mode: "normal",
        ..Default::default()
    });
    assert!(host.exec("_lines = rift.get_lines(1, -1)").is_none());
    assert!(host.exec("rift.notify('info', _lines[2])").is_none());
    let mutations = host.drain_mutations();
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "beta"),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn get_lines_reads_a_resent_buffer_with_an_equal_revision() {
    let host = make_host();
    let update = |lines: &[&str]| StateUpdate {
        buf_id: 1,
        bufs: buf_1(text_update(lines)),
        mode: "normal",
        ..Default::default()
    };
    host.update_state(update(&["old"]));
    assert!(host
        .exec("assert(rift.get_lines(1, 1)[1] == 'old')")
        .is_none());

    host.update_state(update(&["new"]));
    let err = host.exec("assert(rift.get_lines(1, 1)[1] == 'new', rift.get_lines(1, 1)[1])");
    assert!(err.is_none(), "{err:?}");
}

fn count_lines_returned(call: &str) -> i64 {
    let host = make_host();
    host.update_state(StateUpdate {
        buf_id: 1,
        bufs: buf_1(text_update(&["alpha", "beta", "gamma"])),
        mode: "normal",
        ..Default::default()
    });
    assert!(host
        .exec(&format!("_n = #rift.get_lines({call})"))
        .is_none());
    assert!(host.exec("rift.notify('info', tostring(_n))").is_none());
    match &host.drain_mutations()[0] {
        PluginMutation::Notify { message, .. } => message.parse().unwrap(),
        _ => panic!("expected Notify"),
    }
}

#[test]
fn get_lines_inverted_range_returns_empty() {
    assert_eq!(count_lines_returned("3, 1"), 0);
}

#[test]
fn get_lines_start_past_end_returns_empty() {
    assert_eq!(count_lines_returned("10, 20"), 0);
    assert_eq!(count_lines_returned("4, -1"), 0);
}

#[test]
fn get_lines_min_integer_start_does_not_overflow() {
    assert_eq!(count_lines_returned("math.mininteger, -1"), 3);
}

#[test]
fn test_line_count_and_find_materialize_from_deferred_source() {
    let host = make_host();
    host.update_state(StateUpdate {
        buf_id: 1,
        bufs: buf_1(text_update(&["needle here", "no match", "another needle"])),
        mode: "normal",
        ..Default::default()
    });
    assert!(host
        .exec("rift.notify('info', tostring(rift.get_line_count()))")
        .is_none());
    assert!(host
        .exec("rift.notify('info', tostring(#rift.search('needle')))")
        .is_none());
    let mutations = host.drain_mutations();
    match &mutations[0] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "3"),
        _ => panic!("expected line count 3"),
    }
    match &mutations[1] {
        PluginMutation::Notify { message, .. } => assert_eq!(message, "2"),
        _ => panic!("expected 2 needle matches"),
    }
}

#[test]
fn test_get_cursor_returns_1indexed_row() {
    let host = make_host();
    host.update_state(StateUpdate {
        buf_id: 1,
        bufs: buf_1(BufUpdate {
            meta: Some(BufMeta {
                cursor: (4, 2),
                ..Default::default()
            }),
            ..Default::default()
        }),
        mode: "normal",
        ..Default::default()
    });
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
    host.update_state(StateUpdate {
        buf_id: 42,
        mode: "normal",
        ..Default::default()
    });
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
    assert!(errors.is_empty(), "load errors: {errors:?}");
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
    load_plugin(
        &host,
        "failing.lua",
        "rift.on('EditorStart', function(_ev) error('handler error') end)",
    );
    let errors = host.dispatch_event(&EventPayload::EditorStart);
    assert_eq!(errors.len(), 1, "expected one error from bad handler");
    assert!(
        errors[0].starts_with("[failing:EditorStart] "),
        "error: {}",
        errors[0]
    );
    assert!(errors[0].contains("handler error"), "error: {}", errors[0]);
}

#[test]
fn test_insert_queues_mutation() {
    let host = make_host();
    assert!(host.exec("rift.insert('hello')").is_none());
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::InsertAtCursor { text, .. } => assert_eq!(text, "hello"),
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
    assert!(errors.is_none(), "load error: {errors:?}");

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
    assert!(check.is_none(), "assertion failed: {check:?}");
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
    assert!(check.is_none(), "assertion failed: {check:?}");
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
    assert!(check.is_none(), "assertion failed: {check:?}");
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
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_execute_action_still_works_after_ownership_change() {
    let host = make_host();
    // register_action stores {fn, plugin}; execute_action unwraps it.
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
    assert!(check.is_none(), "assertion failed: {check:?}");
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
    assert!(check.is_none(), "assertion failed: {check:?}");
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
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_plugins_unload_removes_all_registrations() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("unloadme.lua");
    std::fs::write(
        &path,
        r#"
        local function notify(message)
            return function() rift.notify("info", message) end
        end
        rift.on("EditorStart", notify("event"))
        rift.register_command("DropCmd", notify("command"))
        rift.register_action("drop:act", notify("action"))
        rift.map("n", "zq", "editor:save")
        rift.annotations.on_action("drop.kind", "run", notify("annotation"))
        rift.annotations.on_enter("drop.kind", notify("enter"))
        rift.annotations.on_leave("drop.kind", notify("leave"))
        rift.register_buffer_kind("drop.buffer", {
            on_close = notify("close"),
            on_save = notify("save"),
            on_action = notify("buffer-action"),
        })
        rift.highlight.add("drop", {1, 0, 1, 1}, {underline = true}, {buf = 1})
        rift.highlight.add("drop", {1, 0, 1, 1}, {underline = true}, {buf = 2})
        rift.highlight.add("drop.extra", {1, 0, 1, 1}, {underline = true})
        rift.annotations.register_kind("drop.kind", {description = "drop defaults"})
        rift.annotations.bind_command("drop.kind", "open", "DropCmd")
        "#,
    )
    .expect("write failed");
    let owner = path.to_string_lossy().replace('\\', "/");
    let host = make_host();
    open_bufs(&host, &[1, 2]);
    let error = host.load_file(&path);
    assert!(error.is_none(), "load error: {error:?}");
    let registrations = host.drain_mutations();
    assert!(registrations.iter().any(|mutation| matches!(
        mutation,
        PluginMutation::RegisterAnnotationAction { kind, verb, command: Some(command), owner: Some(name) }
            if kind == "drop.kind" && verb == "open" && command == "DropCmd" && name == &owner
    )));
    assert!(registrations.iter().any(|mutation| matches!(
        mutation,
        PluginMutation::RegisterKindDefaults { kind, owner: Some(name), .. }
            if kind == "drop.kind" && name == &owner
    )));
    let action = crate::plugin::AnnotationActionCtx {
        annotation_id: 1,
        kind: "drop.kind".into(),
        verb: "run".into(),
        payload: crate::annotations::value::Value::Null,
        params: crate::annotations::value::Value::Null,
        position: 0,
        buffer: 1,
    };
    let hover = crate::plugin::AnnotationHoverCtx {
        annotation_id: 1,
        kind: "drop.kind".into(),
        payload: crate::annotations::value::Value::Null,
        position: 0,
        buffer: 1,
    };
    assert!(host.dispatch_event(&EventPayload::EditorStart).is_empty());
    assert!(host.execute_command("DropCmd", &[]));
    assert!(host.execute_action("drop:act"));
    assert!(host.invoke_annotation_action(&action));
    assert!(host.invoke_annotation_hook(true, &hover));
    assert!(host.invoke_annotation_hook(false, &hover));
    assert!(host.invoke_buffer_close(1, "drop.buffer"));
    assert!(host.invoke_buffer_save(1, "drop.buffer"));
    assert!(host.invoke_buffer_action(1, "drop.buffer", "run"));
    assert_eq!(
        notified_messages(&host),
        [
            "event",
            "command",
            "action",
            "annotation",
            "enter",
            "leave",
            "close",
            "save",
            "buffer-action",
        ]
    );

    let check = host.exec(
        r#"
        local names = rift.plugins.list()
        assert(#names == 1, "expected one plugin before unload")
        local name = names[1]
        assert(_rift_annotation_kinds["drop.kind"].plugin == name)
        assert(_rift_annotation_bindings["drop.kind\0open"].plugin == name)
        assert(_rift_highlight_namespaces[name]["drop"])
        assert(_rift_highlight_namespaces[name]["drop.extra"])
        local slot = _rift_plugin_slots[name][1]
        rift.plugins.unload(name)
        local info = rift.plugins.info(name)
        assert(#info.handlers == 0, "handlers not cleared")
        assert(#info.commands == 0, "commands not cleared")
        assert(#info.actions == 0, "actions not cleared")
        assert(#info.keys == 0, "keys not cleared")
        assert(#rift.plugins.list() == 0, "unloaded plugin still listed")
        assert(#_rift_handlers.EditorStart == 0)
        assert(_rift_slot_events[slot] == nil)
        assert(_rift_slot_plugin[slot] == nil)
        assert(_rift_plugin_slots[name] == nil)
        assert(_rift_plugin_keymaps[name] == nil)
        assert(_rift_commands.DropCmd == nil)
        assert(_rift_actions["drop:act"] == nil)
        assert(_rift_action_handlers["drop.kind\0run"] == nil)
        assert(_rift_enter_handlers["drop.kind"] == nil)
        assert(_rift_leave_handlers["drop.kind"] == nil)
        assert(_rift_buffer_kinds["drop.buffer"] == nil)
        assert(_rift_highlight_namespaces[name] == nil)
        assert(_rift_annotation_kinds["drop.kind"] == nil)
        assert(_rift_annotation_bindings["drop.kind\0open"] == nil)
        "#,
    );
    assert!(check.is_none(), "assertion failed: {check:?}");
    let mutations = host.drain_mutations();
    let clears: Vec<_> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            PluginMutation::ClearNsHighlights {
                buf,
                ns,
                owner: Some(name),
            } if buf.is_none() && name == &owner => Some(ns.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(clears.len(), 2);
    assert!(clears.contains(&"drop"));
    assert!(clears.contains(&"drop.extra"));
    assert_eq!(
        mutations
            .iter()
            .filter(|mutation| matches!(mutation, PluginMutation::UnloadPlugin { owner: name } if name == &owner))
            .count(),
        1
    );
    assert!(mutations.iter().any(|mutation| matches!(
        mutation,
        PluginMutation::UnmapKey { mode, keys } if mode == "n" && keys == "zq"
    )));
    assert!(host.dispatch_event(&EventPayload::EditorStart).is_empty());
    assert!(!host.execute_command("DropCmd", &[]));
    assert!(!host.execute_action("drop:act"));
    assert!(!host.invoke_annotation_action(&action));
    assert!(!host.invoke_annotation_hook(true, &hover));
    assert!(!host.invoke_annotation_hook(false, &hover));
    assert!(!host.invoke_buffer_close(1, "drop.buffer"));
    assert!(!host.invoke_buffer_save(1, "drop.buffer"));
    assert!(!host.invoke_buffer_action(1, "drop.buffer", "run"));
    assert!(host.drain_mutations().is_empty());
}

fn plugin_path(dir: &std::path::Path, file: &str) -> (std::path::PathBuf, String) {
    let path = dir.join(file);
    let name = path.to_string_lossy().replace('\\', "/");
    (path, name)
}

fn listed_plugins(host: &LuaHost) -> Vec<String> {
    let list: LuaTable = host
        .lua
        .load("return rift.plugins.list()")
        .eval()
        .expect("list failed");
    list.sequence_values::<String>()
        .collect::<LuaResult<_>>()
        .expect("names")
}

#[test]
fn test_plugins_list_includes_resource_only_and_failed_plugins() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let (kinds, kinds_name) = plugin_path(dir.path(), "kinds.lua");
    let (binds, binds_name) = plugin_path(dir.path(), "binds.lua");
    let (marks, marks_name) = plugin_path(dir.path(), "marks.lua");
    let (broken, broken_name) = plugin_path(dir.path(), "broken.lua");
    let (missing, missing_name) = plugin_path(dir.path(), "missing.lua");
    std::fs::write(
        &kinds,
        "rift.annotations.register_kind('k.x', { description = 'x' })",
    )
    .expect("write failed");
    std::fs::write(
        &binds,
        "rift.annotations.bind_command('k.x', 'open', 'Open')",
    )
    .expect("write failed");
    std::fs::write(
        &marks,
        "local h
         h = rift.on('EditorStart', function()
             rift.highlight.add('marks', {1, 0, 1, 1}, 'error', {buf = 1})
             rift.off(h)
         end)",
    )
    .expect("write failed");
    std::fs::write(&broken, "error('boom')").expect("write failed");

    let host = make_host();
    open_bufs(&host, &[1]);
    for path in [&kinds, &binds, &marks] {
        assert!(host.load_file(path).is_none());
    }
    assert!(host.load_file(&broken).is_some());
    assert!(host.load_file(&missing).is_some());
    let _ = host.drain_mutations();
    assert!(host.dispatch_event(&EventPayload::EditorStart).is_empty());
    assert!(host.drain_mutations().iter().any(|m| matches!(
        m,
        PluginMutation::AddNsHighlight { ns, owner: Some(owner), .. }
            if ns == "marks" && *owner == marks_name
    )));

    let mut expected = vec![
        kinds_name,
        binds_name,
        marks_name,
        broken_name,
        missing_name,
    ];
    expected.sort();
    assert_eq!(listed_plugins(&host), expected);
}

#[test]
fn test_off_clears_handler_owner_from_list() {
    let host = make_host();
    load_plugin(
        &host,
        "transient.lua",
        "rift.off(rift.on('EditorStart', function() end))",
    );
    assert!(listed_plugins(&host).is_empty());
    let check = host.exec("assert(next(_rift_slot_plugin) == nil, 'slot owner left behind')");
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_unload_keeps_other_plugins_registrations() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    open_bufs(&host, &[1]);
    let mut names = Vec::new();
    for plugin in ["a", "b"] {
        let (path, name) = plugin_path(dir.path(), &format!("{plugin}.lua"));
        std::fs::write(
            &path,
            format!(
                "rift.register_command('{plugin}cmd', function() rift.notify('info', '{plugin}') end)
                 rift.annotations.on_enter('{plugin}.kind', function() rift.notify('info', '{plugin}-enter') end)
                 rift.annotations.register_kind('{plugin}.kind', {{ description = '{plugin}' }})
                 rift.highlight.add('marks', {{1, 0, 1, 1}}, 'error', {{buf = 1}})"
            ),
        )
        .expect("write failed");
        assert!(host.load_file(&path).is_none());
        names.push(name);
    }
    let _ = host.drain_mutations();

    assert!(host
        .exec(&format!("rift.plugins.unload({:?})", names[0]))
        .is_none());
    let mutations = host.drain_mutations();
    let clears: Vec<_> = mutations
        .iter()
        .filter_map(|m| match m {
            PluginMutation::ClearNsHighlights { owner, .. } => owner.as_deref(),
            _ => None,
        })
        .collect();
    assert_eq!(clears, [names[0].as_str()]);
    assert!(mutations
        .iter()
        .any(|m| matches!(m, PluginMutation::UnloadPlugin { owner } if *owner == names[0])));

    assert!(!host.execute_command("acmd", &[]));
    assert!(host.execute_command("bcmd", &[]));
    let hover = |kind: &str| crate::plugin::AnnotationHoverCtx {
        annotation_id: 1,
        kind: kind.into(),
        payload: crate::annotations::value::Value::Null,
        position: 0,
        buffer: 1,
    };
    assert!(!host.invoke_annotation_hook(true, &hover("a.kind")));
    assert!(host.invoke_annotation_hook(true, &hover("b.kind")));
    assert_eq!(notified_messages(&host), ["b", "b-enter"]);
    assert_eq!(listed_plugins(&host), [names[1].clone()]);
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
    assert!(check.is_none(), "assertion failed: {check:?}");
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
    assert!(errors.is_empty(), "load errors: {errors:?}");

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
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_emit_does_not_invoke_handlers_registered_during_same_pass() {
    // A handler that re-arms by registering another handler for the same
    // event must not have that new handler invoked within the same emit.
    let host = make_host();
    assert!(host.exec("_G.count = 0").is_none());
    assert!(host
        .exec(
            r"
        rift.on('UserEvent', function(ev)
            _G.count = _G.count + 1
            rift.on('UserEvent', function(_ev) _G.count = _G.count + 1 end)
        end)
    "
        )
        .is_none());
    assert!(host.exec("rift.emit('Ping')").is_none());
    let check = host.exec("assert(_G.count == 1, 'count was ' .. _G.count)");
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_dispatch_event_does_not_invoke_handlers_registered_during_same_pass() {
    let host = make_host();
    assert!(host.exec("_G.count = 0").is_none());
    assert!(host
        .exec(
            r"
        rift.on('EditorStart', function(_ev)
            _G.count = _G.count + 1
            rift.on('EditorStart', function(_ev) _G.count = _G.count + 1 end)
        end)
    "
        )
        .is_none());
    let errors = host.dispatch_event(&EventPayload::EditorStart);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let check = host.exec("assert(_G.count == 1, 'count was ' .. _G.count)");
    assert!(check.is_none(), "assertion failed: {check:?}");
}

#[test]
fn test_json_decode_rejects_deeply_nested_input() {
    let host = make_host();
    let check = host.exec(
        r#"
        local nested = string.rep("[", 5000) .. string.rep("]", 5000)
        local ok, val, err = pcall(rift.json.decode, nested)
        assert(ok, "rift.json.decode must not raise uncatchably: " .. tostring(val))
        assert(val == nil, "expected nil result for too-deeply-nested json")
        assert(err ~= nil, "expected an error message for too-deeply-nested json")
    "#,
    );
    assert!(check.is_none(), "{check:?}");
}

#[cfg(feature = "lsp")]
#[test]
fn test_lsp_register_queues_mutation() {
    let host = make_host();
    let err = host.exec(
        r#"
        rift.lsp.register({
            language = "rust",
            command = "rust-analyzer",
        })
    "#,
    );
    assert!(err.is_none(), "{err:?}");

    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    match &mutations[0] {
        PluginMutation::LspRegisterServer { language, config } => {
            assert_eq!(language, "rust");
            assert_eq!(config.command, "rust-analyzer");
        }
        other => panic!("expected LspRegisterServer, got {other:?}"),
    }
}

/// Without the `lsp` feature, `rift.lsp` is a table of no-ops rather than
/// missing entirely, so a plugin written for a full build doesn't hard-error.
#[cfg(not(feature = "lsp"))]
#[test]
fn test_lsp_stub_functions_are_callable_and_no_op() {
    let host = make_host();
    let err = host.exec(
        r#"
        rift.lsp.register({ language = "rust", command = "rust-analyzer" })
        rift.lsp.goto_definition()
        rift.lsp.references()
        rift.lsp.hover()
        rift.lsp.rename("new_name")
        rift.lsp.rename_dialog()
        rift.lsp.diagnostics_panel()
        rift.lsp.format()
        rift.lsp.code_action()
        rift.lsp.diagnostic_next()
        rift.lsp.diagnostic_prev()
        local diags = rift.lsp.get_diagnostics()
        assert(type(diags) == "table" and #diags == 0)
    "#,
    );
    assert!(err.is_none(), "{err:?}");
    assert!(
        host.drain_mutations().is_empty(),
        "stub rift.lsp functions must not queue any mutation"
    );
}

/// Regression: a plugin handler that loops forever (e.g. a full-document
/// rescan that never yields) must be aborted well under a second.
#[test]
fn test_dispatch_event_aborts_a_runaway_handler_past_its_time_budget() {
    let host = make_host();
    load_plugin(
        &host,
        "runaway.lua",
        r"
        rift.on('TextChangedCoarse', function(_ev)
            while true do end
        end)
    ",
    );

    let started = std::time::Instant::now();
    let errors = host.dispatch_event(&EventPayload::TextChangedCoarse {
        buf: crate::document::DocumentId::new(1),
    });
    let elapsed = started.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "runaway handler must be aborted by its time budget, took {elapsed:?}"
    );
    assert_eq!(errors.len(), 1, "the aborted handler must report an error");
    assert!(
        errors[0].starts_with("[runaway:TextChangedCoarse] ") && errors[0].contains("time budget"),
        "error should name the plugin and explain the abort: {:?}",
        errors[0]
    );
}

/// A well-behaved handler well under the budget must be unaffected.
#[test]
fn test_dispatch_event_does_not_abort_a_fast_handler() {
    let host = make_host();
    assert!(host
        .exec(
            r"
            rift.on('TextChangedCoarse', function(_ev)
                rift.notify('info', 'fast handler ran')
            end)
        "
        )
        .is_none());

    let errors = host.dispatch_event(&EventPayload::TextChangedCoarse {
        buf: crate::document::DocumentId::new(1),
    });
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let mutations = host.drain_mutations();
    assert_eq!(mutations.len(), 1);
    assert!(matches!(&mutations[0], PluginMutation::Notify { .. }));
}

fn assert_cancelled_by_user_budget(started: std::time::Instant, error: &str, prefix: &str) {
    let elapsed = started.elapsed();
    assert!(
        elapsed >= LuaHost::USER_CALL_BUDGET && elapsed < std::time::Duration::from_secs(5),
        "runaway call must run for the user budget, then stop; took {elapsed:?}"
    );
    assert!(
        error.starts_with(prefix) && error.contains("time budget"),
        "error should name the plugin and explain the abort: {error:?}"
    );
}

fn assert_host_still_runs(host: &LuaHost) {
    assert!(host.exec("rift.notify('info', 'alive')").is_none());
    assert!(host.drain_mutations().iter().any(|m| matches!(
        m,
        PluginMutation::Notify { message, .. } if message == "alive"
    )));
}

fn drained_error(host: &LuaHost) -> String {
    host.drain_mutations()
        .into_iter()
        .find_map(|m| match m {
            PluginMutation::Notify {
                level: NotificationType::Error,
                message,
            } => Some(message),
            _ => None,
        })
        .expect("expected an error notification")
}

#[test]
fn test_runaway_command_is_cancelled_by_user_budget() {
    let host = make_host();
    load_plugin(
        &host,
        "runaway.lua",
        "rift.register_command('spin', function() while true do end end)",
    );
    let started = std::time::Instant::now();
    assert!(host.execute_command("spin", &[]));
    assert_cancelled_by_user_budget(started, &drained_error(&host), "[runaway:spin] ");
    assert_host_still_runs(&host);
}

#[test]
fn test_runaway_action_is_cancelled_by_user_budget() {
    let host = make_host();
    load_plugin(
        &host,
        "runaway.lua",
        "rift.register_action('spin', function() while true do end end)",
    );
    let started = std::time::Instant::now();
    assert!(host.execute_action("spin"));
    assert_cancelled_by_user_budget(started, &drained_error(&host), "[runaway:spin] ");
    assert_host_still_runs(&host);
}

#[test]
fn test_runaway_annotation_action_is_cancelled_by_user_budget() {
    let host = make_host();
    load_plugin(
        &host,
        "runaway.lua",
        "rift.annotations.on_action('test.run', 'run', function(_ctx) while true do end end)",
    );
    let _ = host.drain_mutations();
    let ctx = crate::plugin::AnnotationActionCtx {
        annotation_id: 1,
        kind: "test.run".into(),
        verb: "run".into(),
        payload: crate::annotations::value::Value::Null,
        params: crate::annotations::value::Value::Null,
        position: 0,
        buffer: 1,
    };
    let started = std::time::Instant::now();
    assert!(host.invoke_annotation_action(&ctx));
    assert_cancelled_by_user_budget(started, &drained_error(&host), "[runaway:test.run.run] ");
    assert_host_still_runs(&host);
}

#[test]
fn test_runaway_plugin_load_is_cancelled_by_user_budget() {
    let host = make_host();
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("runaway.lua");
    std::fs::write(&path, "while true do end").expect("write failed");
    let started = std::time::Instant::now();
    let error = host.load_file(&path).expect("expected a load error");
    assert_cancelled_by_user_budget(started, &error, "lua: ");
    assert!(error.contains("runaway.lua"), "{error:?}");
    assert_host_still_runs(&host);
}

#[test]
fn test_runaway_exec_is_cancelled_by_user_budget() {
    let host = make_host();
    let started = std::time::Instant::now();
    let error = host
        .exec("while true do end")
        .expect("expected an exec error");
    assert_cancelled_by_user_budget(started, &error, "[lua] ");
    assert_host_still_runs(&host);
}

fn plugin_status(host: &LuaHost, name: &str) -> (String, Option<String>) {
    let info: LuaTable = host
        .lua
        .load(format!("return rift.plugins.info({name:?})"))
        .eval()
        .expect("info failed");
    (
        info.get("status").expect("status"),
        info.get("error").expect("error"),
    )
}

fn load_named(dir: &std::path::Path, host: &LuaHost, file: &str, src: &str) -> String {
    let (path, name) = plugin_path(dir, file);
    std::fs::write(&path, src).expect("write failed");
    assert!(host.load_file(&path).is_none());
    name
}

const SPIN_ON_BUF_1: &str = "rift.on('TextChangedCoarse', function(ev)
    if ev.buf == 1 then while true do end end
end)";

fn coarse(host: &LuaHost, buf: u64) -> Vec<String> {
    host.dispatch_event(&EventPayload::TextChangedCoarse {
        buf: crate::document::DocumentId::new(buf),
    })
}

fn warnings(mutations: &[PluginMutation]) -> Vec<&str> {
    mutations
        .iter()
        .filter_map(|m| match m {
            PluginMutation::Notify {
                message,
                level: NotificationType::Warning,
            } => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn test_plugin_disabled_after_three_budget_overruns_while_others_run() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let slow = load_named(dir.path(), &host, "slow.lua", SPIN_ON_BUF_1);
    let steady = load_named(
        dir.path(),
        &host,
        "steady.lua",
        "rift.on('TextChangedCoarse', function() rift.notify('info', 'steady') end)",
    );

    for _ in 0..2 {
        assert_eq!(coarse(&host, 1).len(), 1);
    }
    assert!(warnings(&host.drain_mutations()).is_empty());
    assert_eq!(plugin_status(&host, &slow).0, "active");

    assert_eq!(coarse(&host, 1).len(), 1);
    let mutations = host.drain_mutations();
    let disabled = warnings(&mutations);
    assert_eq!(disabled.len(), 1, "{disabled:?}");
    assert!(disabled[0].contains(&slow), "{}", disabled[0]);
    assert!(
        disabled[0].contains(&format!("rift.plugins.enable({slow:?})")),
        "{}",
        disabled[0]
    );

    let started = std::time::Instant::now();
    assert!(coarse(&host, 1).is_empty());
    assert!(started.elapsed() < LuaHost::AUTO_DISPATCH_BUDGET);
    let mutations = host.drain_mutations();
    assert!(warnings(&mutations).is_empty());
    assert!(mutations
        .iter()
        .any(|m| matches!(m, PluginMutation::Notify { message, .. } if message == "steady")));

    let (status, error) = plugin_status(&host, &slow);
    assert_eq!(status, "disabled");
    assert!(error.is_some_and(|e| e.contains("time budget")));
    assert_eq!(plugin_status(&host, &steady), ("active".to_string(), None));
}

#[test]
fn test_enable_brings_a_disabled_plugin_back() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let slow = load_named(dir.path(), &host, "slow.lua", SPIN_ON_BUF_1);
    for _ in 0..3 {
        assert_eq!(coarse(&host, 1).len(), 1);
    }
    assert_eq!(plugin_status(&host, &slow).0, "disabled");

    assert!(host
        .exec(&format!("rift.plugins.enable({slow:?})"))
        .is_none());
    assert_eq!(plugin_status(&host, &slow).0, "active");
    assert_eq!(coarse(&host, 1).len(), 1, "enabled handler runs again");
    assert_eq!(plugin_status(&host, &slow).0, "active");
}

#[test]
fn test_successful_call_resets_budget_failures() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let slow = load_named(dir.path(), &host, "slow.lua", SPIN_ON_BUF_1);
    for buf in [1, 1, 2, 1, 1, 2, 2, 2] {
        let _ = coarse(&host, buf);
    }
    assert!(warnings(&host.drain_mutations()).is_empty());
    assert_eq!(plugin_status(&host, &slow).0, "active");
}

#[test]
fn test_fast_handler_never_accumulates_failures() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let fast = load_named(
        dir.path(),
        &host,
        "fast.lua",
        "rift.on('TextChangedCoarse', function() local n = 0 for i = 1, 1000 do n = n + i end end)",
    );
    for _ in 0..10 {
        assert!(coarse(&host, 1).is_empty());
    }
    assert_eq!(plugin_status(&host, &fast), ("active".to_string(), None));
}

#[test]
fn test_info_reports_failed_plugin_load_error() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let (path, name) = plugin_path(dir.path(), "broken.lua");
    std::fs::write(&path, "error('boom')").expect("write failed");
    assert!(host.load_file(&path).is_some());
    let (status, error) = plugin_status(&host, &name);
    assert_eq!(status, "failed");
    assert!(error.is_some_and(|e| e.contains("boom")));
}

#[test]
fn test_disabled_plugin_hooks_are_not_reported_until_enabled() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let slow = load_named(
        dir.path(),
        &host,
        "slow.lua",
        &format!("{SPIN_ON_BUF_1}\nrift.annotations.on_enter('slow.kind', function() end)"),
    );
    assert!(host.has_annotation_hook(true, "slow.kind"));
    for _ in 0..3 {
        assert_eq!(coarse(&host, 1).len(), 1);
    }
    assert!(!host.has_annotation_hook(true, "slow.kind"));
    assert!(host
        .exec(&format!("rift.plugins.enable({slow:?})"))
        .is_none());
    assert!(host.has_annotation_hook(true, "slow.kind"));
}

fn started_job(mutations: &[PluginMutation]) -> (u64, JobSpec) {
    match mutations {
        [PluginMutation::StartJob { id, spec }] => (*id, spec.clone()),
        other => panic!("expected one StartJob, got {other:?}"),
    }
}

fn job_count(host: &LuaHost, name: &str) -> i64 {
    host.lua
        .load(format!("return rift.plugins.info({name:?}).jobs"))
        .eval()
        .expect("info failed")
}

#[test]
fn test_job_start_queues_its_spec_and_counts_for_the_plugin() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let name = load_named(
        dir.path(),
        &host,
        "jobs.lua",
        "_G.id = rift.job.start({ cmd = 'git', args = { 'status', '-s' }, cwd = 'repo', env = { GIT_PAGER = 'cat' } })",
    );
    let (id, spec) = started_job(&host.drain_mutations());
    assert_eq!(
        spec,
        JobSpec {
            command: JobCommand::Program {
                program: "git".to_string(),
                args: vec!["status".to_string(), "-s".to_string()],
            },
            cwd: Some("repo".to_string()),
            env: vec![("GIT_PAGER".to_string(), "cat".to_string())],
        }
    );
    assert!(host.exec(&format!("assert(_G.id == {id})")).is_none());
    assert_eq!(job_count(&host, &name), 1);
    assert_eq!(listed_plugins(&host), vec![name]);
}

#[test]
fn test_job_shell_queues_a_shell_command() {
    let host = make_host();
    assert!(host.exec("rift.job.shell('echo hi')").is_none());
    let (_, spec) = started_job(&host.drain_mutations());
    assert_eq!(
        spec,
        JobSpec {
            command: JobCommand::Shell("echo hi".to_string()),
            cwd: None,
            env: Vec::new(),
        }
    );
}

#[test]
fn test_job_controls_queue_only_for_known_jobs() {
    let host = make_host();
    assert!(host.exec("_G.id = rift.job.shell('cat')").is_none());
    let (id, _) = started_job(&host.drain_mutations());
    let check = host.exec(
        "assert(rift.job.send(_G.id, 'a\\0b')) \
         assert(rift.job.close_stdin(_G.id)) \
         assert(rift.job.stop(_G.id)) \
         assert(not rift.job.send(0, 'x')) \
         assert(not rift.job.close_stdin(0)) \
         assert(not rift.job.stop(0))",
    );
    assert!(check.is_none(), "{check:?}");
    let mutations = host.drain_mutations();
    assert!(
        matches!(
            mutations.as_slice(),
            [
                PluginMutation::JobInput { id: a, data },
                PluginMutation::JobCloseStdin(b),
                PluginMutation::StopJob(c),
            ] if *a == id && *b == id && *c == id && data == b"a\0b"
        ),
        "{mutations:?}"
    );
}

#[test]
fn test_unload_stops_and_forgets_only_that_plugins_jobs() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let watcher = load_named(dir.path(), &host, "watcher.lua", "rift.job.shell('a')");
    let (watcher_job, _) = started_job(&host.drain_mutations());
    let other = load_named(dir.path(), &host, "other.lua", "rift.job.shell('b')");
    let (other_job, _) = started_job(&host.drain_mutations());

    assert!(host
        .exec(&format!("rift.plugins.unload({watcher:?})"))
        .is_none());

    let mutations = host.drain_mutations();
    assert!(mutations
        .iter()
        .any(|m| matches!(m, PluginMutation::StopJob(id) if *id == watcher_job)));
    assert!(!mutations
        .iter()
        .any(|m| matches!(m, PluginMutation::StopJob(id) if *id == other_job)));
    assert_eq!(job_count(&host, &watcher), 0);
    assert_eq!(job_count(&host, &other), 1);
    assert_eq!(listed_plugins(&host), vec![other]);
    assert!(host
        .exec(&format!("assert(not rift.job.stop({watcher_job}))"))
        .is_none());
}

#[test]
fn test_disabling_a_plugin_stops_its_jobs() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let slow = load_named(
        dir.path(),
        &host,
        "slow.lua",
        &format!("{SPIN_ON_BUF_1}\nrift.job.shell('watch')"),
    );
    let (job, _) = started_job(&host.drain_mutations());
    for _ in 0..3 {
        assert_eq!(coarse(&host, 1).len(), 1);
    }
    let mutations = host.drain_mutations();
    assert!(
        mutations
            .iter()
            .any(|m| matches!(m, PluginMutation::StopJob(id) if *id == job)),
        "{mutations:?}"
    );
    assert_eq!(job_count(&host, &slow), 0);
}

#[test]
fn test_job_events_reach_callbacks_until_exit() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    load_named(
        dir.path(),
        &host,
        "events.lua",
        "rift.job.shell('x', { \
           on_stdout = function(lines) _G.out = table.concat(lines, ',') end, \
           on_stderr = function(lines) _G.err = lines[1] end, \
           on_exit = function(code) _G.code = code end })",
    );
    let (id, _) = started_job(&host.drain_mutations());

    host.deliver_job_event(
        id,
        PluginJobEvent::Output(JobStream::Stdout, vec!["a".to_string(), "b".to_string()]),
    );
    host.deliver_job_event(
        id,
        PluginJobEvent::Output(JobStream::Stderr, vec!["oops".to_string()]),
    );
    host.deliver_job_event(id, PluginJobEvent::Exit(5));
    host.deliver_job_event(id, PluginJobEvent::Exit(6));

    let check = host.exec(&format!(
        "assert(_G.out == 'a,b', tostring(_G.out)) \
         assert(_G.err == 'oops', tostring(_G.err)) \
         assert(_G.code == 5, tostring(_G.code)) \
         assert(not rift.job.send({id}, 'x'))"
    ));
    assert!(check.is_none(), "{check:?}");
}

#[test]
fn test_job_callback_error_names_the_plugin() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    load_named(
        dir.path(),
        &host,
        "failing.lua",
        "rift.job.shell('x', { on_stdout = function() local x = nil return x.boom end })",
    );
    let (id, _) = started_job(&host.drain_mutations());

    host.deliver_job_event(
        id,
        PluginJobEvent::Output(JobStream::Stdout, vec!["line".to_string()]),
    );

    let mutations = host.drain_mutations();
    assert!(
        mutations.iter().any(|m| matches!(
            m,
            PluginMutation::Notify { level: NotificationType::Error, message }
                if message.starts_with("[failing:job.on_stdout] ")
        )),
        "{mutations:?}"
    );
}

const MS: std::time::Duration = std::time::Duration::from_millis(1);

fn timer_deadline(host: &LuaHost) -> crate::time::Instant {
    host.next_timer_deadline().expect("a timer is scheduled")
}

fn plugin_timer_count(host: &LuaHost, name: &str) -> i64 {
    host.lua
        .load(format!("return rift.plugins.info({name:?}).timers"))
        .eval()
        .expect("info failed")
}

#[test]
fn test_one_shot_timer_fires_once_at_its_deadline() {
    let host = make_host();
    let before = crate::time::Instant::now();
    assert!(host
        .exec("rift.timer(100, function() rift.notify('info', 'tick') end)")
        .is_none());
    let deadline = timer_deadline(&host);
    assert!(deadline >= before + 100 * MS);

    let just_before = deadline
        .checked_sub(std::time::Duration::from_nanos(1))
        .unwrap();
    assert!(!host.fire_due_timers(just_before));
    assert!(notified_messages(&host).is_empty());

    assert!(host.fire_due_timers(deadline));
    assert_eq!(notified_messages(&host), ["tick"]);
    assert!(!host.fire_due_timers(deadline + 60_000 * MS));
    assert!(notified_messages(&host).is_empty());
    assert_eq!(host.next_timer_deadline(), None);
}

#[test]
fn test_repeating_timer_fires_every_interval_until_stopped() {
    let host = make_host();
    assert!(host
        .exec(
            "_G.id = rift.timer(50, function() rift.notify('info', 'tick') end, \
             { ['repeat'] = true })"
        )
        .is_none());
    let first = timer_deadline(&host);
    for n in 0..3 {
        let due = first + 50 * n * MS;
        assert!(host.fire_due_timers(due));
        assert_eq!(notified_messages(&host), ["tick"]);
        assert_eq!(host.next_timer_deadline(), Some(due + 50 * MS));
    }

    let late = first + 1000 * MS;
    assert!(host.fire_due_timers(late));
    assert_eq!(
        notified_messages(&host),
        ["tick"],
        "a late timer fires once"
    );
    assert_eq!(host.next_timer_deadline(), Some(late + 50 * MS));

    assert!(host.exec("assert(rift.timer_stop(_G.id))").is_none());
    assert_eq!(host.next_timer_deadline(), None);
    assert!(!host.fire_due_timers(late + 60_000 * MS));
    assert!(notified_messages(&host).is_empty());
}

#[test]
fn test_defer_runs_on_the_next_fire() {
    let host = make_host();
    assert!(host
        .exec(
            "rift.defer(function() \
               rift.notify('info', 'first') \
               rift.defer(function() rift.notify('info', 'second') end) \
             end)"
        )
        .is_none());
    assert!(host.fire_due_timers(crate::time::Instant::now()));
    assert_eq!(notified_messages(&host), ["first"]);
    assert!(host.fire_due_timers(crate::time::Instant::now()));
    assert_eq!(notified_messages(&host), ["second"]);
    assert_eq!(host.next_timer_deadline(), None);
}

#[test]
fn test_unload_stops_a_plugins_timers() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let name = load_named(
        dir.path(),
        &host,
        "ticker.lua",
        "rift.timer(100, function() rift.notify('info', 'once') end) \
         rift.timer(50, function() rift.notify('info', 'again') end, { ['repeat'] = true })",
    );
    assert_eq!(listed_plugins(&host), std::slice::from_ref(&name));
    assert_eq!(plugin_timer_count(&host, &name), 2);

    assert!(host
        .exec(&format!("rift.plugins.unload({name:?})"))
        .is_none());
    assert_eq!(host.next_timer_deadline(), None);
    assert!(listed_plugins(&host).is_empty());
    assert_eq!(plugin_timer_count(&host, &name), 0);
    assert!(!host.fire_due_timers(crate::time::Instant::now() + 60_000 * MS));
    assert!(notified_messages(&host).is_empty());
}

#[test]
fn test_erroring_one_shot_timer_is_reported_and_dropped() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    load_named(
        dir.path(),
        &host,
        "broken.lua",
        "rift.timer(0, function() error('boom') end)",
    );
    assert!(host.fire_due_timers(timer_deadline(&host)));
    let error = drained_error(&host);
    assert!(
        error.starts_with("[broken:timer] ") && error.contains("boom"),
        "{error}"
    );
    assert_eq!(host.next_timer_deadline(), None);
}

#[test]
fn test_runaway_repeating_timer_disables_its_plugin() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let host = make_host();
    let slow = load_named(
        dir.path(),
        &host,
        "slow.lua",
        "rift.timer(10, function() while true do end end, { ['repeat'] = true })",
    );
    for _ in 0..3 {
        assert!(host.fire_due_timers(timer_deadline(&host)));
        let error = drained_error(&host);
        assert!(
            error.starts_with("[slow:timer] ") && error.contains("time budget"),
            "{error}"
        );
    }
    assert_eq!(plugin_status(&host, &slow).0, "disabled");

    let started = std::time::Instant::now();
    assert!(!host.fire_due_timers(timer_deadline(&host)));
    assert!(started.elapsed() < LuaHost::AUTO_DISPATCH_BUDGET);
    assert!(host.drain_mutations().is_empty());
}

fn debounce_host() -> LuaHost {
    let host = make_host();
    assert!(host
        .exec(
            "_G.debounced = rift.debounce(function(a, b) \
               rift.notify('info', a .. ':' .. tostring(b)) \
             end, 100)"
        )
        .is_none());
    host
}

#[test]
fn test_debounce_fires_once_with_the_last_arguments_after_a_burst() {
    let host = debounce_host();
    assert!(host.exec("debounced('a', 1) debounced('b', 2)").is_none());
    let last_call = crate::time::Instant::now();
    assert!(host.exec("debounced('c', nil)").is_none());
    let deadline = timer_deadline(&host);
    assert!(deadline >= last_call + 100 * MS);

    let just_before = deadline
        .checked_sub(std::time::Duration::from_nanos(1))
        .unwrap();
    assert!(!host.fire_due_timers(just_before));
    assert!(notified_messages(&host).is_empty());

    assert!(host.fire_due_timers(deadline));
    assert_eq!(notified_messages(&host), ["c:nil"]);
    assert_eq!(host.next_timer_deadline(), None);
}

#[test]
fn test_debounce_fires_each_time_calls_are_spaced_wider_than_the_delay() {
    let host = debounce_host();
    for (call, expected) in [("debounced('a', 1)", "a:1"), ("debounced('b', 2)", "b:2")] {
        assert!(host.exec(call).is_none());
        assert!(host.fire_due_timers(timer_deadline(&host)));
        assert_eq!(notified_messages(&host), [expected]);
    }
}

#[test]
fn test_debounce_without_calls_never_fires() {
    let host = debounce_host();
    assert_eq!(host.next_timer_deadline(), None);
    assert!(!host.fire_due_timers(crate::time::Instant::now() + 60_000 * MS));
    assert!(notified_messages(&host).is_empty());
}
