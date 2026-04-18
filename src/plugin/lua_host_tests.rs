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
