use super::common::*;

#[test]
fn test_cell_style_hash_no_alloc_and_distinguishes_styles() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let style_a = CellStyle {
        fg: Some(Color::Red),
        bg: None,
        attrs: CellAttrs::default(),
    };
    let style_b = CellStyle {
        fg: Some(Color::Blue),
        bg: None,
        attrs: CellAttrs::default(),
    };

    let mut hasher_a1 = DefaultHasher::new();
    style_a.hash(&mut hasher_a1);
    let mut hasher_a2 = DefaultHasher::new();
    style_a.hash(&mut hasher_a2);
    let mut hasher_b = DefaultHasher::new();
    style_b.hash(&mut hasher_b);

    assert_eq!(
        hasher_a1.finish(),
        hasher_a2.finish(),
        "hashing the same style twice must be consistent"
    );
    assert_ne!(
        hasher_a1.finish(),
        hasher_b.finish(),
        "different styles must hash differently"
    );
}

#[test]
fn test_inline_annotation_change_triggers_content_redraw() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    let state = State::new();
    let mut system = RenderSystem::new(10, 80);

    let inline_v1 = vec![(0usize, 0usize, "A".to_string(), Color::Red, true)];
    let inline_v2 = vec![(0usize, 0usize, "BB".to_string(), Color::Red, true)];

    system
        .render(&mut term, inline_render_state(&buf, &state, &inline_v1, 1))
        .unwrap();
    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    let first_char_v1 = layer.get_cell(0, 0).unwrap().content;

    system
        .render(&mut term, inline_render_state(&buf, &state, &inline_v2, 2))
        .unwrap();
    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    let first_char_v2 = layer.get_cell(0, 0).unwrap().content;

    assert_eq!(
        first_char_v1,
        Character::from('A'),
        "first inline annotation render should draw the leading virtual text"
    );
    assert_eq!(
        first_char_v2,
        Character::from('B'),
        "changing inline annotation text alone must trigger a content redraw"
    );
}

#[test]
fn test_highlights_hash_detects_change_beyond_take_16_cap() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(1000).unwrap();
    let state = syntax_colors_state();
    let mut system = RenderSystem::new(10, 80);

    let mut base: Vec<(std::ops::Range<usize>, u32)> =
        (0..20).map(|i| (i * 10..i * 10 + 5, 1)).collect();
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_before = content_highlights_hash(&system);

    base[17] = (170..200, 1);
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_after = content_highlights_hash(&system);

    assert_ne!(
        hash_before, hash_after,
        "changing a highlight range beyond index 16 must change highlights_hash"
    );
}

#[test]
fn test_highlights_hash_detects_capture_change_on_same_range() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(1000).unwrap();
    let state = syntax_colors_state();
    let mut system = RenderSystem::new(10, 80);

    let mut base: Vec<(std::ops::Range<usize>, u32)> =
        (0..20).map(|i| (i * 10..i * 10 + 5, 1)).collect();
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_before = content_highlights_hash(&system);

    base[0].1 = 2;
    system
        .render(&mut term, render_state_with_highlights(&buf, &state, &base))
        .unwrap();
    let hash_after = content_highlights_hash(&system);

    assert_ne!(
        hash_before, hash_after,
        "changing only a highlight's capture index must change highlights_hash"
    );
}

#[test]
fn scroll_blit_delta_allows_a_pure_scroll() {
    let old = base_blit_key();
    let new = ContentBlitKey {
        scroll_top: 13,
        ..base_blit_key()
    };
    assert_eq!(scroll_blit_delta(&old, &new), Some(3));

    let new_up = ContentBlitKey {
        scroll_top: 6,
        ..base_blit_key()
    };
    assert_eq!(scroll_blit_delta(&old, &new_up), Some(-4));
}

#[test]
fn scroll_blit_delta_allows_non_wrap_pure_scroll() {
    let old = ContentBlitKey {
        has_display_map: false,
        ..base_blit_key()
    };
    let new = ContentBlitKey {
        has_display_map: false,
        scroll_top: 13,
        ..base_blit_key()
    };
    assert_eq!(scroll_blit_delta(&old, &new), Some(3));
}

#[test]
fn scroll_blit_delta_rejects_zero_delta() {
    let old = base_blit_key();
    let new = base_blit_key();
    assert_eq!(scroll_blit_delta(&old, &new), None);
}

#[test]
fn scroll_blit_delta_rejects_a_shift_with_no_overlap() {
    let old = base_blit_key();
    let new = ContentBlitKey {
        scroll_top: old.scroll_top + old.visible_rows,
        ..base_blit_key()
    };
    assert_eq!(scroll_blit_delta(&old, &new), None);
}

#[test]
fn scroll_blit_delta_rejects_a_revision_change() {
    let old = base_blit_key();
    let new = ContentBlitKey {
        revision: old.revision + 1,
        scroll_top: 13,
        ..base_blit_key()
    };
    assert_eq!(scroll_blit_delta(&old, &new), None);
}

#[test]
fn scroll_blit_delta_rejects_any_hash_or_structural_change() {
    let old = base_blit_key();
    let mutators: Vec<BlitKeyMutator> = vec![
        Box::new(|k| k.tab_width += 1),
        Box::new(|k| k.show_line_numbers = !k.show_line_numbers),
        Box::new(|k| k.gutter_width += 1),
        Box::new(|k| k.left_col += 1),
        Box::new(|k| k.visible_rows += 1),
        Box::new(|k| k.visible_cols += 1),
        Box::new(|k| k.editor_bg = Some(Color::Red)),
        Box::new(|k| k.editor_fg = Some(Color::Red)),
        Box::new(|k| k.syntax_generation += 1),
        Box::new(|k| k.custom_highlights_hash += 1),
        Box::new(|k| k.terminal_colors_hash += 1),
        Box::new(|k| k.annotation_presentation_generation += 1),
        Box::new(|k| k.search_matches_hash += 1),
        Box::new(|k| k.annotation_concealed_hash += 1),
        Box::new(|k| k.buf_len += 1),
        Box::new(|k| k.has_display_map = !k.has_display_map),
    ];
    for (i, mutate) in mutators.iter().enumerate() {
        let mut new = base_blit_key();
        new.scroll_top = 13;
        mutate(&mut new);
        assert_eq!(
            scroll_blit_delta(&old, &new),
            None,
            "mutator {i} should have rejected the blit"
        );
    }
}

#[test]
fn scroll_blit_delta_rejects_placeholder_to_loaded_transition() {
    let placeholder = ContentBlitKey {
        revision: 0,
        buf_len: 0,
        ..base_blit_key()
    };
    let loaded = ContentBlitKey {
        revision: 0,
        buf_len: 5000,
        scroll_top: placeholder.scroll_top,
        ..base_blit_key()
    };
    assert_eq!(scroll_blit_delta(&placeholder, &loaded), None);
}

#[test]
fn non_wrap_scroll_blit_with_left_col_tabs_and_multibyte_matches_fresh_render() {
    use crate::search::SearchMatch;
    use std::ops::Range;

    let mut text = String::new();
    for i in 0..20 {
        text.push_str(&format!(
            "café\tline_{i}_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx_filler\n"
        ));
    }

    let mut buf = TextBuffer::new(text.len() + 16).unwrap();
    buf.insert_str(&text).unwrap();

    let custom_highlights: Vec<(Range<usize>, Color)> = text
        .match_indices("line_")
        .map(|(b, m)| (b..b + m.len(), Color::Red))
        .collect();
    let search_matches: Vec<SearchMatch> = text
        .match_indices("filler")
        .map(|(b, m)| {
            let char_start = text[..b].chars().count();
            let char_end = text[..b + m.len()].chars().count();
            SearchMatch {
                range: char_start..char_end,
            }
        })
        .collect();

    let mut state = State::new();
    state.update_buffer_stats(20, buf.len(), crate::document::LineEnding::LF);
    state.search_matches = search_matches;

    let render_once = |system: &mut RenderSystem, term: &mut MockTerminal| {
        system
            .render(
                term,
                RenderState {
                    syntax_generation: 0,
                    annotations_revision: 0,
                    kind_registry_generation: 0,
                    buf: &buf,
                    current_mode: Mode::Normal,
                    pending_key: None,
                    pending_count: 0,
                    state: &state,
                    needs_clear: true,
                    tab_width: 4,
                    highlights: None,
                    capture_map: None,
                    injection_highlights: None,
                    skip_content: false,
                    cursor_row_offset: 0,
                    cursor_col_offset: 0,
                    cursor_viewport: None,
                    terminal_cursor: None,
                    custom_highlights: Some(&custom_highlights),
                    git_gutter_colors: None,
                    annotation_styles: None,
                    annotation_adornments: None,
                    annotation_inline: None,
                    annotation_concealed: None,
                    terminal_cell_colors: None,
                    show_line_numbers: false,
                    display_map: None,
                    scroll_hint: None,
                },
            )
            .unwrap();
    };

    let mut system = RenderSystem::new(10, 30);
    let mut term = MockTerminal::new(10, 30);
    system.viewport.set_scroll(0, 6);
    render_once(&mut system, &mut term);

    system.viewport.set_scroll(3, 6);
    render_once(&mut system, &mut term);
    assert!(
        system.content_blit_key.is_some(),
        "test setup problem: blit cache never populated"
    );

    let mut fresh_system = RenderSystem::new(10, 30);
    let mut fresh_term = MockTerminal::new(10, 30);
    fresh_system.viewport.set_scroll(3, 6);
    render_once(&mut fresh_system, &mut fresh_term);

    let live_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    let fresh_layer = fresh_system
        .compositor
        .get_layer_mut(LayerPriority::CONTENT);
    for row in 0..10 {
        for col in 0..30 {
            assert_eq!(
                live_layer.get_cell(row, col),
                fresh_layer.get_cell(row, col),
                "row {row} col {col}: blitted (left_col=6, tabs, multi-byte) \
                 content diverged from a fresh full render"
            );
        }
    }
}

#[test]
fn cursor_snaps_instead_of_animating_across_a_viewport_jump() {
    let mut text = String::new();
    for i in 0..3000 {
        text.push_str(&format!("line {i}\n"));
    }
    let mut buf = TextBuffer::new(text.len() + 16).unwrap();
    buf.insert_str(&text).unwrap();

    let mut state = State::new();
    state.update_buffer_stats(3000, buf.len(), crate::document::LineEnding::LF);

    let render_once = |system: &mut RenderSystem,
                       term: &mut MockTerminal,
                       buf: &TextBuffer,
                       needs_clear: bool| {
        system
            .render(
                term,
                RenderState {
                    syntax_generation: 0,
                    annotations_revision: 0,
                    kind_registry_generation: 0,
                    buf,
                    current_mode: Mode::Normal,
                    pending_key: None,
                    pending_count: 0,
                    state: &state,
                    needs_clear,
                    tab_width: 4,
                    highlights: None,
                    capture_map: None,
                    injection_highlights: None,
                    skip_content: false,
                    cursor_row_offset: 0,
                    cursor_col_offset: 0,
                    cursor_viewport: None,
                    terminal_cursor: None,
                    custom_highlights: None,
                    git_gutter_colors: None,
                    annotation_styles: None,
                    annotation_adornments: None,
                    annotation_inline: None,
                    annotation_concealed: None,
                    terminal_cell_colors: None,
                    show_line_numbers: false,
                    display_map: None,
                    scroll_hint: None,
                },
            )
            .unwrap();
    };

    let mut system = RenderSystem::new(41, 30);
    let mut term = MockTerminal::new(41, 30);

    let _ = buf.set_cursor(0);
    system.viewport.update(0, 0, 3000, 0);
    render_once(&mut system, &mut term, &buf, true);
    assert_eq!(system.last_soft_cursor(), Some((0, 0)));

    let target_line = 1991;
    let target_offset = buf.line_index.get_start(target_line).unwrap();
    let _ = buf.set_cursor(target_offset);
    system.viewport.update(target_line, 0, 3000, 0);
    let expected_row = target_line - system.viewport.top_line();
    render_once(&mut system, &mut term, &buf, false);

    assert_eq!(
        system.last_soft_cursor(),
        Some((expected_row, 0)),
        "cursor must snap directly to the post-jump row, not interpolate from \
         the pre-jump screen position"
    );
}

#[test]
fn needs_clear_forces_repaint_even_when_blit_key_is_unchanged() {
    let text = "hello world\nsecond line\n";
    let mut buf = TextBuffer::new(text.len() + 16).unwrap();
    buf.insert_str(text).unwrap();

    let mut state = State::new();
    state.update_buffer_stats(2, buf.len(), crate::document::LineEnding::LF);

    let render_once = |system: &mut RenderSystem, term: &mut MockTerminal| {
        system
            .render(
                term,
                RenderState {
                    syntax_generation: 0,
                    annotations_revision: 0,
                    kind_registry_generation: 0,
                    buf: &buf,
                    current_mode: Mode::Normal,
                    pending_key: None,
                    pending_count: 0,
                    state: &state,
                    needs_clear: true,
                    tab_width: 4,
                    highlights: None,
                    capture_map: None,
                    injection_highlights: None,
                    skip_content: false,
                    cursor_row_offset: 0,
                    cursor_col_offset: 0,
                    cursor_viewport: None,
                    terminal_cursor: None,
                    custom_highlights: None,
                    git_gutter_colors: None,
                    annotation_styles: None,
                    annotation_adornments: None,
                    annotation_inline: None,
                    annotation_concealed: None,
                    terminal_cell_colors: None,
                    show_line_numbers: false,
                    display_map: None,
                    scroll_hint: None,
                },
            )
            .unwrap();
    };

    let mut system = RenderSystem::new(10, 30);
    let mut term = MockTerminal::new(10, 30);
    render_once(&mut system, &mut term);

    let corrupt = Cell::new(Character::Unicode('X'));
    {
        let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
        layer.set_cell(0, 0, corrupt);
    }
    assert_eq!(
        system
            .compositor
            .get_layer_mut(LayerPriority::CONTENT)
            .get_cell(0, 0)
            .unwrap()
            .content,
        corrupt.content,
        "test setup problem: corruption didn't take"
    );

    render_once(&mut system, &mut term);

    let repainted = system
        .compositor
        .get_layer_mut(LayerPriority::CONTENT)
        .get_cell(0, 0)
        .unwrap();
    assert_ne!(
        repainted.content, corrupt.content,
        "needs_clear=true must force a full repaint even when the blit key \
         matches the previous frame, or leftover content (e.g. from a closed \
         split) survives on screen"
    );
}
