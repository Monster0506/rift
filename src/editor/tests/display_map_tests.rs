use super::common::*;
use super::*;

#[test]
fn test_display_map_cache_populated_after_command() {
    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    editor.display_map_cache.clear();

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));

    let rev = editor.active_document().buffer.revision;
    match editor.display_map_cache.last() {
        Some(entry) => assert_eq!(entry.revision, rev),
        None => panic!("display_map_cache should be populated after a command"),
    }
}

#[test]
fn test_display_map_cache_revision_stable_across_moves() {
    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));
    let rev_after_first = editor.display_map_cache.last().map(|e| e.revision);

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));
    let rev_after_second = editor.display_map_cache.last().map(|e| e.revision);

    assert_eq!(
        rev_after_first, rev_after_second,
        "cache revision should not change between non-mutating commands"
    );
}

#[test]
fn test_display_map_cache_invalidated_after_mutation() {
    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Right,
        1,
    ));
    let rev_before = editor.display_map_cache.last().map(|e| e.revision).unwrap();

    editor.current_mode = Mode::Insert;
    editor.execute_buffer_command(crate::command::Command::InsertChar('x'));

    let rev_after = editor.display_map_cache.last().map(|e| e.revision).unwrap();

    assert_ne!(
        rev_before, rev_after,
        "mutation must invalidate the display-map cache"
    );
}

#[test]
fn test_resolve_display_map_cached_reuses_across_moves() {
    let mut editor = create_editor_sized(24, 20);
    set_content(
        &mut editor,
        "this is a fairly long first line that wraps\nsecond long line also wraps here\n",
    );
    let doc_id = editor.document_manager.active_document_id().unwrap();

    let first = editor.resolve_display_map_cached(doc_id, 20, 0, 100);
    let rows_first = first.as_ref().map(|m| m.total_visual_rows());
    assert!(rows_first.unwrap() > 2, "long lines should wrap to >2 rows");

    let cached_rev = editor.display_map_cache.last().map(|e| e.revision);
    let second = editor.resolve_display_map_cached(doc_id, 20, 0, 100);
    assert_eq!(second.map(|m| m.total_visual_rows()), rows_first);
    assert_eq!(
        editor.display_map_cache.last().map(|e| e.revision),
        cached_rev,
        "revision must be unchanged (cache hit, no rebuild)"
    );

    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let fresh = super::resolve_display_map(
        doc,
        20,
        editor.state.settings.soft_wrap,
        editor.state.settings.wrap_width,
    );
    assert_eq!(
        editor
            .resolve_display_map_cached(doc_id, 20, 0, 100)
            .map(|m| m.total_visual_rows()),
        fresh.map(|m| m.total_visual_rows()),
    );
}

#[test]
fn test_resolve_display_map_cached_rebuilds_on_tab_width_change() {
    let mut editor = create_editor_sized(24, 20);
    set_content(
        &mut editor,
        "\tindented line that is quite long and wraps\n",
    );
    let doc_id = editor.document_manager.active_document_id().unwrap();

    let before = editor
        .resolve_display_map_cached(doc_id, 20, 0, 100)
        .map(|m| m.tab_width);
    assert_eq!(before, Some(4), "default tab width");

    editor
        .document_manager
        .get_document_mut(doc_id)
        .unwrap()
        .options
        .tab_width = 8;
    let after = editor
        .resolve_display_map_cached(doc_id, 20, 0, 100)
        .map(|m| m.tab_width);
    assert_eq!(
        after,
        Some(8),
        "tab-width change must invalidate the cached map"
    );
}

#[test]
fn test_resolve_display_map_cached_keeps_entries_per_width() {
    let mut editor = create_editor_sized(24, 40);
    set_content(
        &mut editor,
        "this is a fairly long first line that wraps\nsecond long line also wraps here\n",
    );
    let doc_id = editor.document_manager.active_document_id().unwrap();
    editor.display_map_cache.clear();

    let narrow = editor.resolve_display_map_cached(doc_id, 15, 0, 100);
    let wide = editor.resolve_display_map_cached(doc_id, 30, 0, 100);
    assert_eq!(editor.display_map_cache.len(), 2, "one entry per width");

    let narrow_again = editor.resolve_display_map_cached(doc_id, 15, 0, 100);
    let wide_again = editor.resolve_display_map_cached(doc_id, 30, 0, 100);
    assert!(std::sync::Arc::ptr_eq(
        narrow.as_ref().unwrap(),
        narrow_again.as_ref().unwrap()
    ));
    assert!(std::sync::Arc::ptr_eq(
        wide.as_ref().unwrap(),
        wide_again.as_ref().unwrap()
    ));
    assert_eq!(editor.display_map_cache.len(), 2);
}

#[test]
fn test_resolve_display_map_cached_detects_placeholder_to_loaded_swap() {
    let mut editor = create_editor_sized(24, 80);
    let doc_id = editor.document_manager.active_document_id().unwrap();
    editor.resolve_display_map_cached(doc_id, 80, 0, 100);
    assert_eq!(
        editor
            .document_manager
            .get_document(doc_id)
            .unwrap()
            .buffer
            .revision,
        0
    );

    let loaded_text = "line one\nline two\nline three\n";
    let (chars, line_ending, starts) = crate::document::decode_file_bytes(loaded_text.as_bytes());
    let piece_table = crate::buffer::rope::PieceTable::new(chars);
    let line_index =
        crate::buffer::line_index::LineIndex::from_table_with_starts(piece_table, starts);
    editor
        .document_manager
        .get_document_mut(doc_id)
        .unwrap()
        .apply_loaded_content(line_index, line_ending);

    let dm = editor
        .resolve_display_map_cached(doc_id, 80, 0, 100)
        .unwrap();
    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let fresh = super::resolve_display_map(
        doc,
        80,
        editor.state.settings.soft_wrap,
        editor.state.settings.wrap_width,
    );
    assert_eq!(
        dm.total_visual_rows(),
        fresh.unwrap().total_visual_rows(),
        "the cache must detect the placeholder-to-loaded swap and rebuild, \
         not reuse the stale empty map for the newly loaded content"
    );
}

#[test]
fn test_opening_a_file_shows_its_content_immediately_not_after_a_later_edit() {
    let mut editor = create_editor_sized(24, 80);
    let doc_id = editor.document_manager.active_document_id().unwrap();
    editor.force_full_redraw().unwrap();

    let loaded_text = "line one\nline two\nline three\n";
    let (chars, line_ending, starts) = crate::document::decode_file_bytes(loaded_text.as_bytes());
    let piece_table = crate::buffer::rope::PieceTable::new(chars);
    let line_index =
        crate::buffer::line_index::LineIndex::from_table_with_starts(piece_table, starts);
    editor
        .document_manager
        .get_document_mut(doc_id)
        .unwrap()
        .apply_loaded_content(line_index, line_ending);
    editor.force_full_redraw().unwrap();

    let screen = render_ascii(&mut editor);
    assert!(
        screen.contains("line one") && screen.contains("line two") && screen.contains("line three"),
        "loaded content must be visible on screen right after load, not only after a later edit:\n{screen}"
    );
}

#[test]
fn test_display_map_stays_partial_for_large_document_small_viewport() {
    let mut editor = create_editor_sized(10, 30);
    let text = "the quick brown fox jumps over the lazy dog\n".repeat(2000);
    set_content(&mut editor, &text);
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.update_and_render().unwrap();

    let doc_id = editor.document_manager.active_document_id().unwrap();
    let content_width = editor.split_tree.focused_window().viewport.visible_cols();
    let dm = editor
        .resolve_display_map_cached(doc_id, content_width, 0, 0)
        .expect("soft wrap is on by default");
    assert!(
        !dm.is_complete(),
        "a small viewport into a large document must not eagerly wrap the whole thing"
    );
    assert!(
        dm.total_visual_rows() < 2000,
        "only a window around the viewport should be built, got {} rows",
        dm.total_visual_rows()
    );
}

#[test]
fn test_lazy_display_map_extends_correctly_for_far_cursor_jump() {
    let mut editor = create_editor_sized(10, 30);
    let text = "the quick brown fox jumps over the lazy dog\n".repeat(800);
    set_content(&mut editor, &text);
    editor.update_and_render().unwrap();

    let doc_id = editor.document_manager.active_document_id().unwrap();
    let content_width = editor.split_tree.focused_window().viewport.visible_cols();

    let far_char = {
        let doc = editor.document_manager.get_document(doc_id).unwrap();
        doc.buffer.len() * 3 / 4
    };
    editor
        .active_document()
        .buffer
        .set_cursor(far_char)
        .unwrap();
    editor.update_and_render().unwrap();

    let dm = editor
        .resolve_display_map_cached(doc_id, content_width, far_char, 0)
        .unwrap();
    let actual_row = dm.char_to_visual_row(far_char);

    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let reference = super::resolve_display_map(
        doc,
        content_width,
        editor.state.settings.soft_wrap,
        editor.state.settings.wrap_width,
    )
    .unwrap();
    assert_eq!(
        actual_row,
        reference.char_to_visual_row(far_char),
        "a far cursor jump must land on the exact row a full build would report"
    );
}

#[test]
fn test_edit_on_partial_display_map_stays_correct() {
    let mut editor = create_editor_sized(10, 30);
    editor.state.settings.show_line_numbers = false;
    let text = "the quick brown fox jumps over the lazy dog\n".repeat(800);
    set_content(&mut editor, &text);
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.update_and_render().unwrap();

    let doc_id = editor.document_manager.active_document_id().unwrap();
    let content_width = editor.split_tree.focused_window().viewport.visible_cols();

    let dm_before = editor
        .resolve_display_map_cached(doc_id, content_width, 0, 0)
        .unwrap();
    assert!(!dm_before.is_complete());

    editor.current_mode = Mode::Insert;
    editor.execute_buffer_command(crate::command::Command::InsertChar('X'));

    let doc = editor.document_manager.get_document(doc_id).unwrap();
    let mut reference = super::resolve_display_map(
        doc,
        content_width,
        editor.state.settings.soft_wrap,
        editor.state.settings.wrap_width,
    )
    .unwrap();
    reference.extend_to_end(&doc.buffer);

    let mut actual = editor
        .resolve_display_map_cached(doc_id, content_width, 0, 0)
        .unwrap();
    {
        let doc = editor.document_manager.get_document(doc_id).unwrap();
        std::sync::Arc::make_mut(&mut actual).extend_to_end(&doc.buffer);
    }
    assert_eq!(
        *actual, reference,
        "post-edit map, once fully extended, must match a fresh full build"
    );
}

#[test]
fn test_large_count_down_motion_matches_full_build_reference() {
    let mut editor = create_editor_sized(10, 30);
    editor.state.settings.show_line_numbers = false;
    let text = "the quick brown fox jumps over the lazy dog\n".repeat(500);
    set_content(&mut editor, &text);
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.current_mode = Mode::Normal;
    editor.update_and_render().unwrap();

    let doc_id = editor.document_manager.active_document_id().unwrap();
    let content_width = editor.split_tree.focused_window().viewport.visible_cols();

    let count = 137usize;
    let expected = {
        let doc = editor.document_manager.get_document(doc_id).unwrap();
        let reference = super::resolve_display_map(
            doc,
            content_width,
            editor.state.settings.soft_wrap,
            editor.state.settings.wrap_width,
        )
        .unwrap();
        let mut pos = 0usize;
        for _ in 0..count {
            pos = reference.visual_down(pos, &doc.buffer);
        }
        pos
    };

    editor.execute_buffer_command(crate::command::Command::Move(
        crate::action::Motion::Down,
        count,
    ));

    let actual = editor
        .document_manager
        .get_document(doc_id)
        .unwrap()
        .buffer
        .cursor();
    assert_eq!(
        actual, expected,
        "a large-count j must land exactly where a full-build reference would"
    );
}

#[test]
fn test_text_changed_coarse_fires_once_per_render() {
    use std::sync::{Arc, Mutex};

    let mut editor = create_editor();

    let count = Arc::new(Mutex::new(0usize));
    let c = count.clone();
    editor
        .plugin_host
        .on("TextChangedCoarse", move |_| *c.lock().unwrap() += 1);

    editor.current_mode = Mode::Insert;
    for _ in 0..5 {
        editor.execute_buffer_command(crate::command::Command::InsertChar('a'));
    }

    assert_eq!(
        *count.lock().unwrap(),
        0,
        "TextChangedCoarse must not fire inside execute_buffer_command"
    );

    editor.update_and_render().unwrap();
    assert_eq!(
        *count.lock().unwrap(),
        1,
        "TextChangedCoarse must fire exactly once per render cycle"
    );

    editor.update_and_render().unwrap();
    assert_eq!(
        *count.lock().unwrap(),
        1,
        "TextChangedCoarse must not fire on a render with no pending changes"
    );
}

#[test]
fn test_cursor_moved_fires_once_per_render_with_latest_position() {
    use std::sync::{Arc, Mutex};

    let mut editor = create_editor_sized(24, 80);
    set_content(&mut editor, "hello world\n");

    let cols: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let c = cols.clone();
    editor.plugin_host.on("CursorMoved", move |event| {
        if let crate::plugin::EditorEvent::CursorMoved { col, .. } = event {
            c.lock().unwrap().push(*col);
        }
    });

    for _ in 0..5 {
        editor.execute_buffer_command(crate::command::Command::Move(
            crate::action::Motion::Right,
            1,
        ));
    }
    assert!(
        cols.lock().unwrap().is_empty(),
        "CursorMoved must not fire inside execute_buffer_command"
    );

    editor.update_and_render().unwrap();
    assert_eq!(
        *cols.lock().unwrap(),
        vec![5],
        "CursorMoved must fire exactly once per render cycle, with the latest position"
    );

    editor.update_and_render().unwrap();
    assert_eq!(
        cols.lock().unwrap().len(),
        1,
        "CursorMoved must not fire on a render with no pending moves"
    );
}

#[test]
fn exactly_full_wrapped_line_gets_an_eol_row_only_for_its_adornment() {
    let mut editor = create_editor_sized(8, 20);
    editor.state.settings.soft_wrap = true;
    editor.state.settings.show_line_numbers = false;
    load_text(&mut editor, "abcdefghijklmnopqrst\nnext");

    let screen = render_ascii(&mut editor);
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows[0], "abcdefghijklmnopqrst");
    assert_eq!(
        rows[1].trim_end(),
        "next",
        "no blank row without an adornment"
    );

    editor
        .active_document()
        .annotations
        .create_diagnostic(0, 1, "boom");
    let screen = render_ascii(&mut editor);
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows[0], "abcdefghijklmnopqrst");
    assert_eq!(
        rows[1].trim_end(),
        " boom",
        "adornment lands on the EOL row"
    );
    assert_eq!(rows[2].trim_end(), "next");

    editor.state.settings.lsp_virtual_text = false;
    editor.force_full_redraw().unwrap();
    let screen = render_ascii(&mut editor);
    assert_eq!(screen.lines().nth(1).unwrap().trim_end(), "next");
    editor.state.settings.lsp_virtual_text = true;
    editor.force_full_redraw().unwrap();
    let screen = render_ascii(&mut editor);
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows[1].trim_end(), " boom");

    let doc = editor.active_document();
    doc.annotations
        .replace_lsp_diagnostics(Vec::<crate::annotations::LspDiagnosticSpec>::new());
    let screen = render_ascii(&mut editor);
    assert_eq!(screen.lines().nth(1).unwrap().trim_end(), "next");
    editor
        .active_document()
        .annotations
        .create_diagnostic(0, 1, "boom");
    assert_eq!(
        render_ascii(&mut editor).lines().nth(1).unwrap().trim_end(),
        " boom"
    );

    feed_keys(&mut editor, "j");
    assert_eq!(editor.active_document().buffer.get_line(), 1);
    feed_keys(&mut editor, "k");
    assert_eq!(editor.active_document().buffer.get_line(), 0);

    feed_keys(&mut editor, "A");
    editor.update_and_render().unwrap();
    let cursor = editor.active_document().buffer.cursor();
    assert_eq!(cursor, 20);
    let dm = editor
        .resolve_display_map_cached(editor.active_document_id(), 20, cursor, 8)
        .expect("wrap map");
    assert_eq!(dm.char_to_visual_row(cursor), 1);
    assert_eq!(
        dm.char_to_visual_col(cursor, &editor.active_document().buffer),
        0
    );
}
