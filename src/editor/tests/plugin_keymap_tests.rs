use super::common::*;
use super::*;
use crate::keymap::trie::MatchResult;
use crate::keymap::KeyContext;

fn lua_ok(editor: &mut Editor<KeyQueue>, code: &str) {
    let err = editor.plugin_host.lua_exec(code);
    assert!(err.is_none(), "{err:?}");
    editor.apply_plugin_mutations();
}

#[test]
fn rift_map_runs_plugin_actions_and_functions_until_unload() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    let path = dir.path().join("mapper.lua");
    std::fs::write(
        &path,
        r"
        mapper_hits = {}
        rift.register_action('mapper:hit', function() table.insert(mapper_hits, 'action') end)
        rift.map('n', '<F7>', 'plugin:mapper:hit')
        rift.map('n', '<F8>', function() table.insert(mapper_hits, 'fn') end)
        ",
    )
    .expect("write failed");
    let name = path.to_string_lossy().replace('\\', "/");

    let mut editor = editor_with_text("hello\n");
    let err = editor.plugin_host.lua_load_file(&path);
    assert!(err.is_none(), "{err:?}");
    editor.apply_plugin_mutations();

    press(&mut editor, &keys("<F7>"));
    press(&mut editor, &keys("<F8>"));
    lua_ok(&mut editor, "rift.exec_action('plugin:mapper:hit')");
    lua_ok(
        &mut editor,
        "assert(table.concat(mapper_hits, ',') == 'action,fn,action', table.concat(mapper_hits, ','))",
    );

    lua_ok(&mut editor, &format!("rift.plugins.unload([[{name}]])"));
    for seq in ["<F7>", "<F8>"] {
        assert!(
            matches!(
                editor.keymap.lookup(KeyContext::Normal, &keys(seq)),
                MatchResult::None
            ),
            "{seq} still mapped after unload"
        );
    }
    press(&mut editor, &keys("<F8>"));
    lua_ok(
        &mut editor,
        &format!(
            "local info = rift.plugins.info([[{name}]])
            assert(#info.actions == 0, 'actions left: ' .. #info.actions)
            assert(#info.keys == 0, 'keys left: ' .. #info.keys)
            assert(#mapper_hits == 3, table.concat(mapper_hits, ','))"
        ),
    );
}
