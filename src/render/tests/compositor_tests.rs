use super::common::*;

#[test]
fn test_render_status_bar_normal_mode_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);

    let statusdrawstate = create_default_statusdrawstate();

    let mut paint_frame = crate::paint::PaintFrame::new(0);
    StatusBar::render_to_layer(&mut layer, &statusdrawstate, &mut paint_frame);

    let cell = layer.get_cell(9, 0);
    assert!(cell.is_some());
}

#[test]
fn test_render_status_bar_insert_mode_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);
    let mut statusdrawstate = create_default_statusdrawstate();
    statusdrawstate.mode = Mode::Insert;

    let mut paint_frame = crate::paint::PaintFrame::new(0);
    StatusBar::render_to_layer(&mut layer, &statusdrawstate, &mut paint_frame);

    let cell = layer.get_cell(9, 0);
    assert!(cell.is_some());
}

#[test]
fn test_render_status_bar_pending_key_layer() {
    let mut layer = Layer::new(LayerPriority::STATUS_BAR, 10, 80);
    let mut statusdrawstate = create_default_statusdrawstate();
    statusdrawstate.pending_key = Some(Key::Ctrl(b'd'));

    let mut paint_frame = crate::paint::PaintFrame::new(0);
    StatusBar::render_to_layer(&mut layer, &statusdrawstate, &mut paint_frame);

    let cell = layer.get_cell(9, 0);
    assert!(cell.is_some());
}

#[test]
fn test_render_does_not_clear_screen() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(100).unwrap();
    let state = State::new();
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

    assert_eq!(term.clear_screen_calls, 0);
}

#[test]
fn test_render_cursor_positioning() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("hello").unwrap();
    let state = State::new();
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

    assert!(!term.cursor_moves.is_empty());
}

#[test]
fn test_render_empty_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let buf = TextBuffer::new(100).unwrap();
    let state = State::new();
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

    assert_eq!(term.clear_screen_calls, 0);
    assert!(!term.writes.is_empty());
}

#[test]
fn test_render_multiline_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("line1\nline2\nline3\nline4\nline5").unwrap();
    let state = State::new();
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

    let written = term.get_written_string();
    assert!(written.contains("line1"));
    assert!(written.contains("line2"));
    assert!(written.contains("line3"));
}

#[test]
fn test_render_file_loaded_at_start() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();

    buf.insert_bytes(b"line1\nline2\nline3\n").unwrap();
    buf.move_to_start();

    let state = State::new();
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

    assert_eq!(term.clear_screen_calls, 0);

    let raw = term.get_written_string();
    let plain = strip_ansi(&raw);
    assert!(plain.contains("line1"), "expected 'line1' in: {plain:?}");
    assert!(plain.contains("line2"), "expected 'line2' in: {plain:?}");
    assert!(plain.contains("line3"), "expected 'line3' in: {plain:?}");

    assert_eq!(buf.get_line(), 0);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn test_render_viewport_scrolling() {
    let mut term = MockTerminal::new(5, 80); // Small viewport
    let mut buf = TextBuffer::new(100).unwrap();
    for i in 0..10 {
        buf.insert_str(&format!("line{}\n", i)).unwrap();
    }
    for _ in 0..8 {
        buf.move_up();
    }
    let state = State::new();
    let mut system = RenderSystem::new(5, 80);

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

    assert!(system.viewport.top_line() <= 8);
}

#[test]
fn test_render_viewport_edge_cases() {
    let mut term = MockTerminal::new(1, 1); // Minimal viewport
    let buf = TextBuffer::new(100).unwrap();
    let state = State::new();
    let mut system = RenderSystem::new(1, 1);

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

#[test]
fn test_render_large_buffer() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(10000).unwrap();
    for i in 0..100 {
        buf.insert_str(&format!("line {}\n", i)).unwrap();
    }
    let state = State::new();
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

    assert_eq!(term.clear_screen_calls, 0);
    assert!(!term.writes.is_empty());
}

#[test]
fn test_render_cursor_at_viewport_boundaries() {
    let mut term = MockTerminal::new(5, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    for i in 0..20 {
        buf.insert_str(&format!("line {}\n", i)).unwrap();
    }
    let state = State::new();
    let mut system = RenderSystem::new(5, 80);

    for _ in 0..20 {
        buf.move_up();
    }
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
    assert_eq!(term.clear_screen_calls, 0);

    term.clear_screen_calls = 0;
    term.cursor_moves.clear();
    term.writes.clear();

    for _ in 0..20 {
        buf.move_down();
    }
    system
        .viewport
        .update(buf.get_line(), 0, buf.get_total_lines(), 0);

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
                needs_clear: false,
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
    assert_eq!(term.clear_screen_calls, 0);
    assert!(system.viewport.top_line() > 0);
}

#[test]
fn test_compositor_content_layer() {
    let mut system = RenderSystem::new(10, 80);

    let content_layer = system.compositor.get_layer_mut(LayerPriority::CONTENT);
    assert_eq!(content_layer.rows(), 10);
    assert_eq!(content_layer.cols(), 80);
}

#[test]
fn test_compositor_status_bar_layer() {
    let mut system = RenderSystem::new(10, 80);

    let status_layer = system.compositor.get_layer_mut(LayerPriority::STATUS_BAR);
    assert_eq!(status_layer.rows(), 10);
    assert_eq!(status_layer.cols(), 80);
}

#[test]
fn test_compositor_floating_window_layer() {
    let mut system = RenderSystem::new(10, 80);

    let floating_layer = system
        .compositor
        .get_layer_mut(LayerPriority::FLOATING_WINDOW);
    assert_eq!(floating_layer.rows(), 10);
    assert_eq!(floating_layer.cols(), 80);
}

#[test]
fn test_no_redraw_on_noop() {
    let mut term = MockTerminal::new(10, 80);
    let mut buf = TextBuffer::new(100).unwrap();
    buf.insert_str("test").unwrap();
    let mut state = State::new();
    state.settings.show_line_numbers = false;
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
        Character::from('t')
    );

    content_layer.set_cell(0, 0, Cell::from_char('X'));
    assert_eq!(
        content_layer.get_cell(0, 0).unwrap().content,
        Character::from('X')
    );

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
                needs_clear: false,
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
        Character::from('X')
    );
}
