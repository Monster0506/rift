use super::common::*;
#[cfg(feature = "plugins")]
use crate::plugin::PluginMutation;

#[cfg(feature = "plugins")]
#[test]
fn save_does_not_write_another_buffers_content_when_a_bufsavepre_hook_closes_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc1.txt");

    let mut editor = create_editor();

    let doc1_id = editor.active_document().id;
    editor.active_document().set_path(&path);
    load_text(&mut editor, "doc one content");

    let doc2 = crate::document::Document::new_scratch(
        editor.document_manager.next_id(),
        "doc2".to_string(),
        &["doc two content".to_string()],
    );
    editor.open_buffer(doc2, crate::document::manager::Placement::Background);
    editor.focus_document(doc1_id).unwrap();
    assert_eq!(editor.active_document().id, doc1_id);

    // Simulate a BufSavePre Lua hook that closes the buffer being saved.
    editor
        .plugin_host
        .queue_mutation(PluginMutation::CloseBuffer {
            buf: doc1_id,
            force: true,
        });

    editor.save_buffer(editor.active_document_id(), None, false);
    drain_jobs(&mut editor);

    if let Ok(written) = std::fs::read_to_string(&path) {
        assert!(
            !written.contains("doc two content"),
            "do_save wrote doc2's content to doc1's path after a BufSavePre hook closed doc1: {written:?}"
        );
    }
}

use crate::editor::Editor;
use crate::test_utils::MockTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn settle(editor: &mut Editor<MockTerminal>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let idle = !editor.job_manager.any_job_thread_alive();
        while let Ok(msg) = editor.job_manager.receiver().try_recv() {
            editor.handle_job_message(msg);
        }
        if idle || Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn open(editor: &mut Editor<MockTerminal>, path: &Path) {
    editor
        .open_file(Some(path.to_string_lossy().into_owned()), false)
        .unwrap();
    settle(editor);
}

fn active_path(editor: &mut Editor<MockTerminal>) -> Option<PathBuf> {
    editor.active_document().path().map(Path::to_path_buf)
}

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    (dir, a, b)
}

#[test]
fn bug_w_existing_other_path_overwrites() {
    let (_dir, a, b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    let before = active_path(&mut editor);

    editor.execute_command_line(&format!(":w {}", b.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
    assert_eq!(active_path(&mut editor), before);
    assert!(!editor.state.notifications.is_empty());
}

#[test]
fn w_bang_existing_other_path_overwrites() {
    let (_dir, a, b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);

    editor.execute_command_line(&format!(":w! {}", b.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&b).unwrap(), "a");
    assert!(active_path(&mut editor).unwrap().ends_with("b.txt"));
}

#[test]
fn bug_wq_existing_other_path_overwrites() {
    let (_dir, a, b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);

    editor.execute_command_line(&format!(":wq {}", b.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
    assert!(!editor.should_quit);
}

#[test]
fn bug_w_failed_save_renames_buffer() {
    let (dir, a, _b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    let before = active_path(&mut editor);
    let target = dir.path().join("missing").join("c.txt");

    editor.execute_command_line(&format!(":w {}", target.display()));
    settle(&mut editor);

    assert!(!target.exists());
    assert_eq!(active_path(&mut editor), before);
}

#[test]
fn bug_w_path_with_space_rejected() {
    let (dir, a, _b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    let target = dir.path().join("sp ace.txt");

    editor.execute_command_line(&format!(":w {}", target.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "a");
}

#[test]
fn bug_e_path_with_space_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("sp ace.txt");
    std::fs::write(&target, "x").unwrap();
    let mut editor = create_editor();

    editor.execute_command_line(&format!(":e {}", target.display()));
    settle(&mut editor);

    assert!(active_path(&mut editor).unwrap().ends_with("sp ace.txt"));
    assert_eq!(editor.active_document().buffer().to_string(), "x");
}

#[test]
fn w_own_path_writes() {
    let (_dir, a, _b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    load_text(&mut editor, "z");

    editor.execute_command_line(&format!(":w {}", a.display()));
    settle(&mut editor);

    assert_eq!(std::fs::read_to_string(&a).unwrap(), "za");
}

#[test]
fn bug_wq_in_split_quits_editor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    editor.execute_command_line("split");
    assert_eq!(editor.split_tree.window_count(), 2);

    type_text(&mut editor, "hello");
    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        ":wq in a split must only close the split"
    );
    assert_eq!(editor.split_tree.window_count(), 1);
    assert!(std::fs::read_to_string(&path).unwrap().contains("hello"));
}

#[test]
fn bug_wq_quits_discarding_other_dirty_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "").unwrap();
    std::fs::write(&b, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &a);
    let a_id = editor.active_document_id();
    type_text(&mut editor, "unsaved");
    open_and_load(&mut editor, &b);

    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(!editor.should_quit, ":wq must not exit with a.txt dirty");
    let a_doc = editor
        .document_manager
        .get_document(a_id)
        .expect("a.txt must stay open");
    assert!(a_doc.is_dirty());
}

#[test]
fn bug_explorer_wq_quits_editor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    editor.open_explorer(dir.path().to_path_buf());
    drain_jobs(&mut editor);
    assert!(editor.panel_layout.is_some());

    editor.execute_command_line("wq");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        ":wq in the explorer must only close it"
    );
    assert!(editor.panel_layout.is_none());
}

#[test]
fn wq_then_typing_before_save_completes_quits_with_unsaved_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "").unwrap();

    let mut editor = create_editor();
    open_and_load(&mut editor, &path);
    type_text(&mut editor, "saved");

    editor.execute_command_line("wq");
    type_text(&mut editor, "!");
    drain_jobs(&mut editor);

    assert!(
        !editor.should_quit,
        "an edit made after :wq started must cancel the quit"
    );
    assert!(editor.active_document().is_dirty());
}

#[cfg(feature = "plugins")]
mod save_pre {
    use super::*;

    fn editable_file() -> (tempfile::TempDir, PathBuf, Editor<MockTerminal>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "original\n").unwrap();
        let mut editor = create_editor();
        let startup = editor.active_document_id();
        open_and_load(&mut editor, &path);
        editor
            .close_buffer(startup, crate::document::manager::RemovalIntent::Force)
            .unwrap();
        editor.plugin_host.mark_lua_used();
        editor.update_lua_state();
        (dir, path, editor)
    }

    fn lua(editor: &Editor<MockTerminal>, code: &str) {
        let error = editor.plugin_host.lua_exec(code);
        assert!(error.is_none(), "{error:?}");
    }

    #[test]
    fn cancel_prevents_write_post_and_quit_but_preserves_hook_edits() {
        let (_dir, path, mut editor) = editable_file();
        let buf = editor.active_document_id();
        lua(
            &editor,
            "rift.on('BufSavePre', function(ev)
               _G.resume = assert(ev.hold())
             end)
             rift.on('BufSavePre', function(ev)
               _G.cancelled = ev.cancel()
             end)
             rift.on('BufSavePre', function()
               rift.append_lines({ 'hook edit' })
             end)
             rift.on('BufSavePost', function() _G.post = true end)",
        );

        editor.execute_command_line("wq");
        drain_jobs(&mut editor);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
        assert!(editor
            .active_document()
            .buffer()
            .to_string()
            .contains("hook edit"));
        assert!(editor.active_document().is_dirty());
        assert_eq!(editor.active_document_id(), buf);
        assert!(!editor.should_quit);
        lua(
            &editor,
            "assert(_G.cancelled and _G.post == nil and not _G.resume())",
        );
    }

    #[test]
    fn held_write_and_quit_resumes_once_with_current_text() {
        let (_dir, path, mut editor) = editable_file();
        lua(
            &editor,
            "rift.on('BufSavePre', function(ev)
               _G.pre = (_G.pre or 0) + 1
               _G.event = ev
               _G.resume = assert(ev.hold())
               assert(not _G.resume())
               rift.append_lines({ 'pre hook' })
             end)
             rift.on('BufSavePost', function()
               _G.post = (_G.post or 0) + 1
             end)",
        );

        editor.execute_command_line("wq");
        drain_jobs(&mut editor);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
        assert!(!editor.should_quit);
        lua(&editor, "assert(_G.pre == 1 and _G.post == nil)");

        lua(
            &editor,
            "assert(not _G.event.cancel() and _G.event.hold() == nil)
             rift.append_lines({ 'after hold' })
             assert(_G.resume())
             assert(not _G.resume())",
        );
        editor.apply_plugin_mutations();
        drain_jobs(&mut editor);

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("pre hook"));
        assert!(written.contains("after hold"));
        assert!(editor.should_quit);
        lua(
            &editor,
            "assert(_G.pre == 1 and _G.post == 1 and not _G.resume())",
        );
    }

    #[test]
    fn closing_and_reopening_buffer_invalidates_held_quit() {
        let (_dir, path, mut editor) = editable_file();
        let original = editor.active_document_id();
        lua(
            &editor,
            "rift.on('BufSavePre', function(ev) _G.resume = assert(ev.hold()) end)",
        );

        type_text(&mut editor, "discarded");
        editor.execute_command_line("wq");
        editor.execute_command_line("bdelete!");
        assert!(editor.document_manager.get_document(original).is_none());
        open_and_load(&mut editor, &path);
        assert_ne!(editor.active_document_id(), original);
        type_text(&mut editor, "new buffer");

        lua(&editor, "assert(not _G.resume())");
        editor.apply_plugin_mutations();
        drain_jobs(&mut editor);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
        assert!(editor.active_document().is_dirty());
        assert!(!editor.should_quit);
    }

    #[test]
    fn resumed_write_and_quit_does_not_close_another_split_showing_the_same_buffer() {
        let (_dir, path, mut editor) = editable_file();
        let original_window = editor.split_tree.focused_window_id();
        editor.execute_command_line("split");
        let buf = editor.active_document_id();
        assert_ne!(editor.split_tree.focused_window_id(), original_window);
        lua(
            &editor,
            "rift.on('BufSavePre', function(ev) _G.resume = assert(ev.hold()) end)",
        );

        type_text(&mut editor, "updated ");
        editor.execute_command_line("wq");
        editor.focus_window(original_window);
        assert_eq!(editor.active_document_id(), buf);
        lua(&editor, "assert(_G.resume())");
        editor.apply_plugin_mutations();
        drain_jobs(&mut editor);

        assert!(std::fs::read_to_string(&path).unwrap().contains("updated "));
        assert_eq!(editor.split_tree.window_count(), 2);
        assert_eq!(editor.split_tree.focused_window_id(), original_window);
        assert!(!editor.should_quit);
    }

    #[test]
    fn newer_write_supersedes_a_held_write_and_quit() {
        let (_dir, path, mut editor) = editable_file();
        lua(
            &editor,
            "rift.on('BufSavePre', function(ev)
               _G.previous_resume = _G.resume
               _G.resume = assert(ev.hold())
             end)",
        );

        type_text(&mut editor, "updated ");
        editor.execute_command_line("wq");
        editor.execute_command_line("w");
        lua(&editor, "assert(not _G.previous_resume() and _G.resume())");
        editor.apply_plugin_mutations();
        drain_jobs(&mut editor);

        assert!(std::fs::read_to_string(&path).unwrap().contains("updated "));
        assert!(!editor.should_quit);
    }

    #[test]
    fn special_buffer_resumes_its_save_handler_but_cannot_resume_while_closing() {
        let mut editor = create_editor();
        editor.plugin_host.mark_lua_used();
        lua(
            &editor,
            "rift.register_buffer_kind('held', {
               on_save = function() _G.saved = (_G.saved or 0) + 1 end,
               on_close = function() _G.resumed_during_close = _G.resume() end,
             })
             rift.on('BufSavePre', function(ev)
               assert(ev.path == nil)
               _G.resume = assert(ev.hold())
             end)
             rift.on('BufSavePost', function()
               _G.post = (_G.post or 0) + 1
             end)",
        );
        editor.apply_plugin_mutations();
        editor.update_lua_state();
        lua(
            &editor,
            "rift.create_buffer('held', { lines = { 'body' } })",
        );
        editor.apply_plugin_mutations();
        let buf = editor.active_document_id();

        editor.execute_command_line("w");
        lua(
            &editor,
            "assert(_G.saved == nil and _G.post == nil and _G.resume())",
        );
        editor.apply_plugin_mutations();
        lua(&editor, "assert(_G.saved == 1 and _G.post == 1)");

        editor.execute_command_line("w");
        editor.execute_command_line("bdelete!");

        assert!(editor.document_manager.get_document(buf).is_none());
        lua(
            &editor,
            "assert(_G.saved == 1 and _G.post == 1 and _G.resumed_during_close == false)",
        );
    }

    #[test]
    fn unload_invalidates_even_a_queued_resume() {
        let (dir, path, mut editor) = editable_file();
        let plugin = dir.path().join("hold.lua");
        std::fs::write(
            &plugin,
            "_G.hold_owner = _rift_current_plugin_file
             rift.on('BufSavePre', function(ev) _G.resume = assert(ev.hold()) end)",
        )
        .unwrap();
        assert!(editor.plugin_host.lua_load_file(&plugin).is_none());
        type_text(&mut editor, "discarded");
        editor.execute_command_line("wq");

        lua(
            &editor,
            "assert(_G.resume())
             rift.plugins.unload(_G.hold_owner)
             assert(not _G.resume())",
        );
        editor.apply_plugin_mutations();
        drain_jobs(&mut editor);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
        assert!(editor.active_document().is_dirty());
        assert!(!editor.should_quit);
    }

    #[test]
    fn failed_callback_does_not_strand_the_save_it_requested_to_hold() {
        let (_dir, path, mut editor) = editable_file();
        lua(
            &editor,
            "rift.on('BufSavePre', function(ev)
               _G.resume = assert(ev.hold())
               rift.append_lines({ 'hook edit' })
               error('failed pre-save hook')
             end)
             rift.on('BufSavePost', function() _G.post = true end)",
        );

        editor.execute_command_line("w");
        drain_jobs(&mut editor);

        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("hook edit"));
        assert!(editor
            .state
            .notifications
            .iter_active()
            .any(|notification| { notification.message.contains("failed pre-save hook") }));
        lua(&editor, "assert(_G.post and not _G.resume())");
    }
}

#[test]
fn all_open_frontends_keep_dirty_buffers_when_opening_another_file() {
    use crate::action::{Action, EditorAction};

    let (_dir, a, b) = setup();
    let frontends = if cfg!(feature = "plugins") { 3 } else { 2 };
    for frontend in 0..frontends {
        let mut editor = create_editor();
        open(&mut editor, &a);
        let original = editor.active_document_id();
        type_text(&mut editor, "edit");
        assert!(editor.active_document().is_dirty());
        let path = b.display().to_string();
        match frontend {
            0 => {
                editor.handle_action(&Action::Editor(EditorAction::OpenFile {
                    path: Some(path),
                    force: false,
                }));
            }
            1 => editor.execute_command_line(&format!(":e {path}")),
            #[cfg(feature = "plugins")]
            _ => {
                editor
                    .plugin_host
                    .queue_mutation(PluginMutation::OpenFile { path, force: false });
                editor.apply_plugin_mutations();
            }
            #[cfg(not(feature = "plugins"))]
            _ => unreachable!(),
        }
        settle(&mut editor);
        assert_eq!(editor.active_document().buffer().to_string(), "b");
        let original = editor.document_manager.get_document(original).unwrap();
        assert!(original.is_dirty());
        assert!(original.buffer().to_string().contains("edit"));
    }
}

#[test]
fn edit_rejects_directory_without_changing_buffers() {
    let (dir, a, _b) = setup();
    let mut editor = create_editor();
    open(&mut editor, &a);
    let original = editor.active_document_id();
    editor.execute_command_line(&format!(":e {}", dir.path().display()));
    assert_eq!(editor.active_document_id(), original);
    assert!(editor
        .state
        .notifications
        .iter_active()
        .any(|n| { n.message == crate::constants::errors::MSG_NOT_A_FILE }));
}
