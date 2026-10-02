use super::common::*;
use crate::plugin::PluginMutation;

#[test]
fn plugin_set_option_document_local_setting_updates_active_document() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");

    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetOption {
            name: "tab_width".to_string(),
            value: "8".to_string(),
        });
    editor.apply_plugin_mutations();

    assert_eq!(editor.active_document().options.tab_width, 8);
}

#[test]
fn plugin_set_option_global_setting_updates_user_settings() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");
    assert!(editor.state.settings.ghost_cut);

    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetOption {
            name: "ghostcut".to_string(),
            value: "false".to_string(),
        });
    editor.apply_plugin_mutations();

    assert!(!editor.state.settings.ghost_cut);
}

#[test]
fn plugin_set_option_unknown_name_reports_error_without_panicking() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");

    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetOption {
            name: "not_a_real_setting".to_string(),
            value: "1".to_string(),
        });
    editor.apply_plugin_mutations();

    assert!(!editor.state.error_manager.notifications().is_empty());
}

#[test]
fn lua_get_tab_width_and_expand_tabs_reflect_document_local_overrides() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");
    editor.plugin_host.mark_lua_used();

    editor.execute_command_line("setlocal tabwidth 8".to_string());
    editor.execute_command_line("setlocal expandtabs false".to_string());
    editor.update_lua_state();

    let err = editor.plugin_host.lua_exec(
        "assert(rift.get_tab_width() == 8, 'tab_width=' .. tostring(rift.get_tab_width()))",
    );
    assert!(err.is_none(), "{err:?}");

    let err = editor.plugin_host.lua_exec(
        "assert(rift.get_expand_tabs() == false, 'expand_tabs=' .. tostring(rift.get_expand_tabs()))",
    );
    assert!(err.is_none(), "{err:?}");
}
