use super::common::*;

#[test]
fn test_render_line_numbers_enabled() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2").unwrap();
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(2, 11, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

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
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 1).unwrap().content,
        Character::from('1')
    );
    assert_eq!(
        content_layer.get_cell(0, 2).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 3).unwrap().content,
        Character::from('l')
    ); // Content starts here
}

#[test]
fn test_render_line_numbers_disabled() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1").unwrap();
    let mut state = State::new();
    state.settings.show_line_numbers = false;
    state.update_buffer_stats(1, 5, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

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
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('l')
    );
}

#[test]
fn test_render_line_numbers_gutter_width() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(100).unwrap();
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(100, 0, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

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
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 1).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 2).unwrap().content,
        Character::from(' ')
    );
    assert_eq!(
        content_layer.get_cell(0, 3).unwrap().content,
        Character::from('1')
    );
    assert_eq!(
        content_layer.get_cell(0, 4).unwrap().content,
        Character::from(' ')
    );
}

#[test]
fn test_render_cursor_position_with_line_numbers() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("test").unwrap();
    buf.move_to_start();
    let mut state = State::new();
    state.settings.show_line_numbers = true;
    state.update_buffer_stats(10, 4, crate::document::LineEnding::LF); // 2 digits -> gutter 4

    let mut system = RenderSystem::new(10, 80);

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
                show_line_numbers: true,
                display_map: None,
                scroll_hint: None,
            },
        )
        .unwrap();
}
