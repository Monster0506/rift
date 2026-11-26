use super::common::*;

#[test]
fn load_plugins_keeps_going_after_a_broken_plugin() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");
    assert!(editor.state.settings.editing().ghost_cut);

    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    std::fs::write(first.path().join("broken.lua"), "this is not lua @@").unwrap();
    std::fs::write(second.path().join("also_broken.lua"), "error('boom')").unwrap();
    std::fs::write(
        second.path().join("good.lua"),
        "rift.set_option('ghostcut', false)",
    )
    .unwrap();
    let init_lua = config.path().join("init.lua");
    std::fs::write(&init_lua, "rift.notify('info', 'init ran')").unwrap();

    let errors = editor.load_plugins_from(
        &[first.path().to_path_buf(), second.path().to_path_buf()],
        &init_lua,
    );

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors[0].contains("broken.lua"), "{errors:?}");
    assert!(errors[1].contains("also_broken.lua"), "{errors:?}");
    assert!(!editor.state.settings.editing().ghost_cut);
    assert!(editor
        .state
        .notifications
        .iter_active()
        .any(|n| n.message == "init ran"));
}
