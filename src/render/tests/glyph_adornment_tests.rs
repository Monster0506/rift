use super::common::*;

#[test]
fn test_tab_rendered_as_space_not_raw_tab() {
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("\thello").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 6, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);
    system
        .render(
            &mut term,
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

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    for col in 0..4 {
        let cell = layer.get_cell(0, col).unwrap();
        assert_ne!(
            cell.content,
            Character::Tab,
            "col {col}: raw tab must not be stored in cell"
        );
        assert_eq!(
            cell.content,
            Character::from(' '),
            "col {col}: expanded tab cell should be a space"
        );
    }
    assert_eq!(layer.get_cell(0, 4).unwrap().content, Character::from('h'));
}

#[test]
fn test_trailing_adornment_advances_by_display_width() {
    let ad = vec![(0, std::borrow::Cow::Borrowed("\u{4e2d}x"), Color::Red)];
    let row = render_row_with_adornments(8, "ab", &ad, 0);
    assert_eq!(row, "ab \u{4e2d} x  ");
}

#[test]
fn test_trailing_adornment_clips_with_ellipsis() {
    let ad = vec![(0, std::borrow::Cow::Borrowed("0123456789"), Color::Red)];
    assert_eq!(render_row_with_adornments(10, "ab", &ad, 0), "ab 0123...");
    assert_eq!(render_row_with_adornments(6, "ab", &ad, 0), "ab    ");
}

#[test]
fn test_trailing_adornment_hidden_when_line_end_scrolled_off() {
    let ad = vec![(0, std::borrow::Cow::Borrowed("msg"), Color::Red)];
    assert_eq!(render_row_with_adornments(8, "ab", &ad, 5), "        ");
    assert_eq!(render_row_with_adornments(2, "ab", &ad, 0), "ab");
}

#[test]
fn test_tab_straddling_left_col_does_not_shift_text() {
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("\thello").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 6, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);

    system.viewport.set_scroll(0, 2);

    system
        .render(
            &mut term,
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

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        layer.get_cell(0, 2).unwrap().content,
        Character::from('h'),
        "after partial tab, 'h' must be at screen col 2"
    );
}

#[test]
fn test_wide_char_straddling_left_col_renders_as_space() {
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("你hello").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 6, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);

    system.viewport.set_scroll(0, 1);

    system
        .render(
            &mut term,
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

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        layer.get_cell(0, 0).unwrap().content,
        Character::from(' '),
        "partially-scrolled wide glyph must render as a space, not a clipped glyph"
    );
    assert_eq!(
        layer.get_cell(0, 1).unwrap().content,
        Character::from('h'),
        "'h' must follow directly after the straddling wide char, not be shifted"
    );
}

#[test]
fn test_zero_width_char_does_not_write_stray_cell() {
    let mut term = MockTerminal::new(5, 40);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str("a\u{0301}").unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, 2, crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, 40);

    system
        .render(
            &mut term,
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

    let layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        layer.get_cell(0, 0).unwrap().content,
        Character::from('a'),
        "'a' must remain in its own cell"
    );
    assert_ne!(
        layer.get_cell(0, 1).unwrap().content,
        Character::from('\u{0301}'),
        "a zero-width combining char must not write its own stray cell"
    );
}

#[test]
fn test_plan_glyph_draw_fully_visible() {
    use crate::render::plan_glyph_draw;
    let plan = plan_glyph_draw(1, 5, 0);
    assert_eq!(plan.visible_width, 1);
    assert!(!plan.straddles_left_edge);
}

#[test]
fn test_plan_glyph_draw_wide_char_straddling_left_edge() {
    use crate::render::plan_glyph_draw;
    let plan = plan_glyph_draw(2, 0, 1);
    assert_eq!(plan.visible_width, 1);
    assert!(plan.straddles_left_edge);
}

#[test]
fn test_plan_glyph_draw_wide_char_fully_off_screen() {
    use crate::render::plan_glyph_draw;
    let plan = plan_glyph_draw(2, 0, 2);
    assert_eq!(plan.visible_width, 0);
    assert!(!plan.straddles_left_edge);
}

#[test]
fn test_plan_glyph_draw_zero_width_char_is_never_drawn() {
    use crate::render::plan_glyph_draw;
    let plan = plan_glyph_draw(0, 3, 0);
    assert_eq!(plan.visible_width, 0);
    assert!(!plan.straddles_left_edge);
}
