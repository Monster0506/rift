pub(super) use crate::buffer::TextBuffer;
pub(super) use crate::character::Character;
pub(super) use crate::color::Color;
pub(super) use crate::key::Key;
pub(super) use crate::layer::Cell;
pub(super) use crate::layer::{CellAttrs, CellStyle};
pub(super) use crate::layer::{Layer, LayerPriority};
pub(super) use crate::mode::Mode;
pub(super) use crate::render::{
    calculate_cursor_column, calculate_cursor_column_at, scroll_blit_delta, ContentBlitKey,
    CursorInfo, RenderState, RenderSystem, StatusDrawState,
};
pub(super) use crate::state::State;
pub(super) use crate::status::StatusBar;
pub(super) use crate::test_utils::MockTerminal;

pub(super) fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\x1b' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                    i += 1;
                }
                i += 1;
            } else {
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

pub(super) fn create_default_statusdrawstate() -> StatusDrawState {
    StatusDrawState {
        mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        last_keypress: None,
        file_name: "".to_string(),
        is_dirty: false,
        cols: 80,
        editor_bg: None,
        editor_fg: None,
        debug_mode: false,
        total_lines: 0,
        search_query: None,
        reverse_video: false,
        show_status_line: true,
        show_filename: true,
        show_dirty_indicator: true,
        search_match_index: None,
        search_total_matches: 0,
        cursor: CursorInfo { row: 0, col: 0 },
        lsp_status: None,
        lsp_ok_color: None,
        lsp_error_color: None,
        lsp_warn_color: None,
        is_remote: false,
    }
}

pub(super) fn render_row_with_adornments(
    cols: usize,
    text: &str,
    adornments: &[crate::render::LineAdornment<'_>],
    left_col: usize,
) -> String {
    let mut term = MockTerminal::new(5, cols as u16);
    let mut buf = TextBuffer::new(64).unwrap();
    buf.insert_str(text).unwrap();
    let mut state = State::new();
    state.update_buffer_stats(1, text.len(), crate::document::LineEnding::LF);
    let mut system = RenderSystem::new(5, cols);
    system.viewport.set_scroll(0, left_col);
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
                annotation_adornments: Some(adornments),
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
    (0..cols)
        .map(|col| match layer.get_cell(0, col).map(|c| c.content) {
            Some(Character::Unicode(ch)) => ch,
            Some(other) => panic!("col {col}: unexpected cell {other:?}"),
            None => '.',
        })
        .collect()
}

pub(super) fn inline_render_state<'a>(
    buf: &'a TextBuffer,
    state: &'a State,
    inline: &'a [(usize, usize, String, Color, bool)],
    annotations_revision: u64,
) -> RenderState<'a> {
    RenderState {
        syntax_generation: 0,
        annotations_revision,
        kind_registry_generation: 0,
        buf,
        current_mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        state,
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
        annotation_inline: Some(inline),
        annotation_concealed: None,
        terminal_cell_colors: None,
        show_line_numbers: false,
        display_map: None,
        scroll_hint: None,
    }
}

pub(super) fn render_state_with_highlights<'a>(
    buf: &'a TextBuffer,
    state: &'a State,
    highlights: &'a [(std::ops::Range<usize>, u32)],
) -> RenderState<'a> {
    RenderState {
        syntax_generation: 0,
        annotations_revision: 0,
        kind_registry_generation: 0,
        buf,
        current_mode: Mode::Normal,
        pending_key: None,
        pending_count: 0,
        state,
        needs_clear: true,
        tab_width: 4,
        highlights: Some(highlights),
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
    }
}

pub(super) fn content_highlights_hash(system: &RenderSystem) -> u64 {
    use crate::render::components::Renderable;
    system
        .world
        .renderables
        .iter()
        .find_map(|(_, r)| match r {
            Renderable::TextBuffer(s) => Some(s.highlights_hash),
            _ => None,
        })
        .expect("content entity not found")
}

pub(super) fn syntax_colors_state() -> State {
    let mut state = State::new();
    state.settings.syntax_colors = Some(crate::color::theme::SyntaxColors::from_base_colors(&[(
        "function",
        crate::color::Color::Red,
    )]));
    state
}

pub(super) fn base_blit_key() -> ContentBlitKey {
    ContentBlitKey {
        revision: 1,
        buf_len: 100,
        tab_width: 4,
        show_line_numbers: false,
        gutter_width: 0,
        left_col: 0,
        visible_rows: 50,
        visible_cols: 80,
        has_display_map: true,
        editor_bg: None,
        editor_fg: None,
        syntax_generation: 0,
        custom_highlights_hash: 0,
        terminal_colors_hash: 0,
        search_matches_hash: 0,
        annotation_presentation_generation: 0,
        annotation_concealed_hash: 0,
        scroll_top: 10,
    }
}

pub(super) type BlitKeyMutator = Box<dyn Fn(&mut ContentBlitKey)>;
