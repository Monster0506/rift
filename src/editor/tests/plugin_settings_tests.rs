use super::common::*;
use crate::plugin::PluginMutation;

#[test]
fn plugin_set_option_document_local_setting_updates_active_document() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");

    let buf = Some(editor.active_document().id);
    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetOption {
            buf,
            name: "tab_width".to_string(),
            value: "8".to_string(),
        });
    editor.apply_plugin_mutations();

    assert_eq!(editor.active_document().options().tab_width(), 8);
}

#[test]
fn set_uses_the_same_local_scope_and_aliases_as_plugin_options() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");

    editor.execute_command_line("set tab_width=8");
    editor.execute_command_line("set noet");
    editor.execute_command_line("set ghostcut=false");

    assert_eq!(editor.active_document().options().tab_width(), 8);
    assert!(!editor.active_document().options().expand_tabs());
    assert!(!editor.state.settings.editing().ghost_cut);
}

#[test]
fn plugin_set_option_global_setting_updates_user_settings() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");
    assert!(editor.state.settings.editing().ghost_cut);

    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetOption {
            buf: None,
            name: "ghostcut".to_string(),
            value: "false".to_string(),
        });
    editor.apply_plugin_mutations();

    assert!(!editor.state.settings.editing().ghost_cut);
}

#[test]
fn plugin_clipboard_size_change_trims_existing_entries() {
    let mut editor = create_editor();
    for text in ["old", "new"] {
        editor
            .clipboard_ring
            .push_str(text, crate::wrap::RangeKind::Charwise);
    }

    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetOption {
            buf: None,
            name: "clipboard.size".to_string(),
            value: "1".to_string(),
        });
    editor.apply_plugin_mutations();

    assert_eq!(editor.clipboard_ring.capacity(), 1);
    assert_eq!(ring_text(&editor, 0).as_deref(), Some("new"));
    assert!(ring_text(&editor, 1).is_none());
}

#[test]
fn plugin_set_option_unknown_name_reports_error_without_panicking() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");

    editor
        .plugin_host
        .queue_mutation(PluginMutation::SetOption {
            buf: None,
            name: "not_a_real_setting".to_string(),
            value: "1".to_string(),
        });
    editor.apply_plugin_mutations();

    assert!(!editor.state.notifications.is_empty());
}

#[test]
fn lua_get_tab_width_and_expand_tabs_reflect_document_local_overrides() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");
    editor.plugin_host.mark_lua_used();

    editor.execute_command_line("setlocal tabwidth 8");
    editor.execute_command_line("setlocal expandtabs false");
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

#[test]
fn lua_get_option_reads_every_registered_setting() {
    let mut editor = create_editor();
    load_text(&mut editor, "hello\n");
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();

    let err = editor.plugin_host.lua_exec(
        r#"
        local expected = {
            ["command_line.borderstyle"] = "unicode",
            ["command_line.border"] = true,
            ["command_line.reverse_video"] = false,
            ["appearance.theme"] = "gruvbox",
            ["status_line.show_filename"] = true,
            ["status_line.reverse_video"] = false,
            ["status_line.show_status_line"] = true,
            ["status_line.show_dirty"] = true,
            ["split.equalize_proportional"] = false,
            ["lsp.debug_log"] = false,
            ["lsp.virtual_text"] = true,
            ["lsp.diagnostic_tooltip"] = true,
            ["ghostcut"] = true,
            ["quitonq"] = true,
            ["undofile"] = true,
            ["number"] = true,
            ["line_ending"] = "lf",
        }
        for name, want in pairs(expected) do
            local got = rift.get_option(name)
            assert(got == want, name .. "=" .. tostring(got))
        end
        "#,
    );
    assert!(err.is_none(), "{err:?}");
}

#[test]
fn plugin_selection_options_recolor_selected_cells_and_read_back_after_resync() {
    use crate::action::{Action, EditorAction};
    use crate::color::Color;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world\n");
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();

    let err = editor.plugin_host.lua_exec(
        r##"
        rift.set_option("appearance.selection_background", "#ff0000")
        rift.set_option("appearance.selection_foreground", "#ffffff")
        "##,
    );
    assert!(err.is_none(), "{err:?}");
    editor.apply_plugin_mutations();

    editor
        .active_document()
        .set_cursor(0, crate::document::ops::CursorCause::User)
        .unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor
        .active_document()
        .set_cursor(4, crate::document::ops::CursorCause::User)
        .unwrap();
    editor.update_and_render().unwrap();

    let cols = editor.render_system.compositor.cols();
    let row: String = editor.render_system.compositor.get_composited_slice()[..cols]
        .iter()
        .map(|c| c.to_char())
        .collect();
    let col = row[..row.find("hello").unwrap()].chars().count();
    let cell = editor.render_system.compositor.get_composited_slice()[col];
    assert_eq!(
        (cell.fg, cell.bg),
        (
            Some(Color::Rgb {
                r: 255,
                g: 255,
                b: 255
            }),
            Some(Color::Rgb { r: 255, g: 0, b: 0 })
        )
    );

    editor.update_lua_state();
    let err = editor.plugin_host.lua_exec(
        r##"
        local bg = rift.get_option("appearance.selection_background")
        assert(bg == "#ff0000", "selection_background=" .. tostring(bg))
        "##,
    );
    assert!(err.is_none(), "{err:?}");
}
