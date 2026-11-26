use super::super::super::Editor;
use crate::error::{ErrorType, RiftError};
use crate::notification::NotificationType;
use crate::test_utils::MockTerminal;

const KINDS: [NotificationType; 4] = [
    NotificationType::Info,
    NotificationType::Warning,
    NotificationType::Error,
    NotificationType::Success,
];

fn editor() -> Editor<MockTerminal> {
    Editor::new(MockTerminal::new(24, 80)).unwrap()
}

fn assert_one_ttl_table(editor: &Editor<MockTerminal>, expected: usize) {
    let active: Vec<_> = editor.state.notifications.iter_active().collect();
    assert_eq!(active.len(), expected);
    for notification in active {
        assert_eq!(
            notification.ttl,
            Some(notification.kind.ttl()),
            "{}",
            notification.message
        );
    }
}

#[test]
fn notify_uses_the_ttl_table() {
    let mut editor = editor();
    for kind in KINDS {
        editor.notify(kind, "direct");
    }
    assert_one_ttl_table(&editor, KINDS.len());
}

#[test]
fn errors_use_the_same_ttl_table_as_notify() {
    let mut editor = editor();
    editor.notify_error(RiftError::new(ErrorType::Io, "E1", "io failure"));
    editor.notify_error(RiftError::warning(ErrorType::Other, "W1", "low disk"));

    let kinds: Vec<_> = editor
        .state
        .notifications
        .iter_active()
        .map(|n| (n.kind, n.message.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (NotificationType::Error, "io failure"),
            (NotificationType::Warning, "low disk")
        ]
    );
    assert_one_ttl_table(&editor, 2);
}

#[test]
fn notify_command_uses_the_same_ttl_table() {
    let mut editor = editor();
    for kind in ["info", "warn", "error", "success"] {
        editor.execute_command_line(&format!("notify {kind} from-ex"));
    }
    editor.execute_command_line("set nosuchoption");
    assert_one_ttl_table(&editor, 5);
}

#[cfg(feature = "plugins")]
#[test]
fn plugin_notify_uses_the_same_ttl_table() {
    let mut editor = editor();
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();
    for kind in ["info", "warn", "error"] {
        let err = editor
            .plugin_host
            .lua_exec(&format!("rift.notify('{kind}', 'from-lua')"));
        assert!(err.is_none(), "{err:?}");
    }
    editor.apply_plugin_mutations();
    assert_one_ttl_table(&editor, 3);
}
