use super::common::*;

#[test]
fn test_render_line_numbers_enabled() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    crate::document::ops::Edit::type_text("line1\nline2").apply_to(&mut buf);
    let mut state = State::new();
    crate::state::definitions::create_settings_registry()
        .set("number", Some("true"), &mut state.settings)
        .unwrap();
    state.update_buffer_stats(2, 11, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            &RenderState {
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
                intrinsic: crate::render::IntrinsicLayers::default(),
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                git_gutter_colors: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hints: &[],
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
    crate::document::ops::Edit::type_text("line1").apply_to(&mut buf);
    let mut state = State::new();
    crate::state::definitions::create_settings_registry()
        .set("number", Some("false"), &mut state.settings)
        .unwrap();
    state.update_buffer_stats(1, 5, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            &RenderState {
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
                intrinsic: crate::render::IntrinsicLayers::default(),
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                git_gutter_colors: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hints: &[],
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
    crate::state::definitions::create_settings_registry()
        .set("number", Some("true"), &mut state.settings)
        .unwrap();
    state.update_buffer_stats(100, 0, crate::document::LineEnding::LF);

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            &RenderState {
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
                intrinsic: crate::render::IntrinsicLayers::default(),
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                git_gutter_colors: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hints: &[],
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
    crate::document::ops::Edit::type_text("test").apply_to(&mut buf);
    crate::document::ops::standalone::move_cursor(&mut buf, crate::action::Motion::StartOfFile);
    let mut state = State::new();
    crate::state::definitions::create_settings_registry()
        .set("number", Some("true"), &mut state.settings)
        .unwrap();
    state.update_buffer_stats(10, 4, crate::document::LineEnding::LF); // 2 digits -> gutter 4

    let mut system = RenderSystem::new(10, 80);

    system
        .render(
            &mut term,
            &RenderState {
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
                intrinsic: crate::render::IntrinsicLayers::default(),
                skip_content: false,
                cursor_row_offset: 0,
                cursor_col_offset: 0,
                cursor_viewport: None,
                terminal_cursor: None,
                git_gutter_colors: None,
                annotation_styles: None,
                annotation_adornments: None,
                annotation_inline: None,
                annotation_concealed: None,
                show_line_numbers: true,
                display_map: None,
                scroll_hints: &[],
            },
        )
        .unwrap();
}
