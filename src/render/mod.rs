use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::color::Color;
use crate::key::Key;
use crate::layer::{Cell, Layer};
use crate::mode::Mode;
use crate::num::{f64_to_usize, usize_to_f64};
use crate::state::State;
use crate::viewport::Viewport;
use crate::wrap::DisplayMap;

pub mod components;
pub mod ecs;
pub mod pipeline;
pub mod system;
pub use pipeline::*;
pub use system::RenderSystem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorInfo {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDrawState {
    pub revision: u64,
    pub top_line: usize,
    pub top_visual_row: usize,
    pub left_col: usize,
    pub rows: usize,
    pub tab_width: usize,
    pub show_line_numbers: bool,
    pub highlights_hash: u64,
    pub gutter_width: usize,
    pub search_matches_count: usize,
    pub conceal_hash: u64,
    pub annotation_styles_hash: u64,
    pub annotation_text_hash: u64,
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    pub theme: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContentBlitKey {
    revision: u64,
    buf_len: usize,
    tab_width: usize,
    show_line_numbers: bool,
    gutter_width: usize,
    left_col: usize,
    visible_rows: usize,
    visible_cols: usize,
    has_display_map: bool,
    editor_bg: Option<Color>,
    editor_fg: Option<Color>,
    syntax_generation: u64,
    custom_highlights_hash: u64,
    terminal_colors_hash: u64,
    search_matches_hash: u64,
    annotation_presentation_generation: u64,
    annotation_concealed_hash: u64,
    scroll_top: usize,
}

fn hash_ranged_spans<E: std::hash::Hash>(spans: &[(std::ops::Range<usize>, E)]) -> u64 {
    use std::hash::Hasher;
    let mut h: u64 = spans.len() as u64;
    for (range, extra) in spans {
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(range.start as u64);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(range.end as u64 + 1);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        extra.hash(&mut hasher);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(hasher.finish());
    }
    h
}

fn hash_search_matches(matches: &[crate::search::SearchMatch]) -> u64 {
    let mut h: u64 = matches.len() as u64;
    for m in matches {
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(m.range.start as u64);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(m.range.end as u64 + 1);
    }
    h
}

fn hash_concealed_ranges(items: &[(usize, usize)]) -> u64 {
    let mut h: u64 = items.len() as u64;
    for (s, e) in items {
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(*s as u64);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(*e as u64 + 1);
    }
    h
}

pub(crate) const fn mix_generation(a: u64, b: u64) -> u64 {
    a.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(b)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = bytes.len() as u64;
    for b in bytes {
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(u64::from(*b) + 1);
    }
    h
}

fn compute_content_blit_key(ctx: &DrawContext, visible_rows: usize) -> ContentBlitKey {
    let visible_cols = ctx.viewport.visible_cols();
    let gutter_width = ctx.gutter_cols();
    let scroll_top = if ctx.display_map.is_some() {
        ctx.viewport.top_visual_row()
    } else {
        ctx.viewport.top_line()
    };
    let theme_hash = hash_bytes(ctx.state.settings.theme.as_deref().unwrap_or("").as_bytes());
    let annotation_presentation_generation = mix_generation(
        mix_generation(ctx.annotations_revision, theme_hash),
        ctx.kind_registry_generation,
    );
    ContentBlitKey {
        revision: ctx.buf.revision,
        buf_len: ctx.buf.len(),
        tab_width: ctx.tab_width,
        show_line_numbers: ctx.state.line_numbers_on(ctx.show_line_numbers),
        gutter_width,
        left_col: ctx.viewport.left_col(),
        visible_rows,
        visible_cols,
        has_display_map: ctx.display_map.is_some(),
        editor_bg: ctx.state.settings.editor_bg,
        editor_fg: ctx.state.settings.editor_fg,
        syntax_generation: ctx.syntax_generation,
        custom_highlights_hash: hash_ranged_spans(ctx.custom_highlights.unwrap_or(&[])),
        terminal_colors_hash: hash_ranged_spans(ctx.terminal_cell_colors.unwrap_or(&[])),
        search_matches_hash: hash_search_matches(ctx.search_matches()),
        annotation_presentation_generation,
        annotation_concealed_hash: hash_concealed_ranges(ctx.annotation_concealed.unwrap_or(&[])),
        scroll_top,
    }
}

fn scroll_blit_delta(old: &ContentBlitKey, new: &ContentBlitKey) -> Option<isize> {
    let same_except_scroll = ContentBlitKey {
        scroll_top: old.scroll_top,
        ..new.clone()
    } == *old;
    if !same_except_scroll {
        return None;
    }
    let delta = new.scroll_top.cast_signed() - old.scroll_top.cast_signed();
    if delta == 0 || delta.unsigned_abs() >= new.visible_rows {
        return None;
    }
    Some(delta)
}

fn advance_idx_past<T>(
    spans: &[T],
    idx: &mut usize,
    boundary: usize,
    end_of: impl Fn(&T) -> usize,
) {
    while *idx < spans.len() && end_of(&spans[*idx]) < boundary {
        *idx += 1;
        crate::perf_cursor_advance!();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusDrawState {
    pub mode: Mode,
    pub pending_key: Option<crate::key::Key>,
    pub pending_count: usize,
    pub last_keypress: Option<crate::key::Key>,
    pub file_name: String,
    pub is_dirty: bool,
    pub cursor: CursorInfo,
    pub total_lines: usize,
    pub debug_mode: bool,
    pub cols: usize,
    pub search_query: Option<String>,
    pub search_match_index: Option<usize>,
    pub search_total_matches: usize,
    pub status_line: crate::state::StatusLineSettings,
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    pub lsp_status: Option<String>,
    pub lsp_ok_color: Option<crate::color::Color>,
    pub lsp_error_color: Option<crate::color::Color>,
    pub lsp_warn_color: Option<crate::color::Color>,
    pub is_remote: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDrawState {
    pub content: String,
    pub cursor: CursorInfo,
    pub width: usize,
    pub height: usize,
    pub has_border: bool,
    pub reverse_video: bool,
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionMenuDrawState {
    pub candidates: Vec<(String, String)>,
    pub selected: Option<usize>,
    pub terminal_cols: usize,
    pub cmd_width_ratio: f64,
    pub cmd_min_width: usize,
    pub cmd_height: usize,
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDrawState {
    pub generation: u64,
    pub count: usize,
}

pub type InlineAdornment = (usize, usize, String, Color, bool);

pub type LineAdornment<'a> = (usize, std::borrow::Cow<'a, str>, Color);

pub struct RenderState<'a> {
    pub buf: &'a TextBuffer,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
    pub highlights: Option<&'a [(std::ops::Range<usize>, u32)]>,
    pub capture_map: Option<&'a [&'a str]>,
    pub injection_highlights: Option<&'a [(std::ops::Range<usize>, String)]>,
    pub skip_content: bool,
    pub cursor_row_offset: usize,
    pub cursor_col_offset: usize,
    pub cursor_viewport: Option<&'a Viewport>,
    pub terminal_cursor: Option<(usize, usize)>,
    pub custom_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    pub git_gutter_colors: Option<&'a [(usize, Color)]>,
    pub annotation_styles: Option<&'a [(std::ops::Range<usize>, crate::layer::CellStyle)]>,
    pub annotation_adornments: Option<&'a [LineAdornment<'a>]>,
    pub annotation_inline: Option<&'a [InlineAdornment]>,
    pub annotation_concealed: Option<&'a [(usize, usize)]>,
    pub terminal_cell_colors: Option<&'a [crate::color::CellColorSpan]>,
    pub show_line_numbers: bool,
    pub display_map: Option<&'a DisplayMap>,
    pub scroll_hint: Option<(usize, usize, isize)>,
    pub syntax_generation: u64,
    pub annotations_revision: u64,
    pub kind_registry_generation: u64,
}

pub struct DrawContext<'a> {
    pub buf: &'a TextBuffer,
    pub viewport: &'a Viewport,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub custom_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    pub git_gutter_colors: Option<&'a [(usize, Color)]>,
    pub annotation_styles: Option<&'a [(std::ops::Range<usize>, crate::layer::CellStyle)]>,
    pub annotation_adornments: Option<&'a [LineAdornment<'a>]>,
    pub annotation_inline: Option<&'a [InlineAdornment]>,
    pub annotation_concealed: Option<&'a [(usize, usize)]>,
    pub terminal_cell_colors: Option<&'a [crate::color::CellColorSpan]>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
    pub highlights: Option<&'a [(std::ops::Range<usize>, u32)]>,
    pub capture_map: Option<&'a [&'a str]>,
    pub injection_highlights: Option<&'a [(std::ops::Range<usize>, String)]>,
    pub show_line_numbers: bool,
    pub display_map: Option<&'a DisplayMap>,
    pub gutter_width_override: Option<usize>,
    pub search_matches_override: Option<&'a [crate::search::SearchMatch]>,
    pub syntax_generation: u64,
    pub annotations_revision: u64,
    pub kind_registry_generation: u64,
}

impl<'a> DrawContext<'a> {
    /// Gutter columns for this draw, 0 when line numbers are hidden.
    #[must_use]
    pub fn gutter_cols(&self) -> usize {
        if !self.state.line_numbers_on(self.show_line_numbers) {
            return 0;
        }
        self.gutter_width_override
            .unwrap_or(self.state.gutter_width)
    }

    #[must_use]
    pub fn search_matches(&self) -> &'a [crate::search::SearchMatch] {
        self.search_matches_override
            .unwrap_or_else(|| &self.state.search_matches)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorPosition {
    Absolute(u16, u16),
}

pub(crate) fn render_content_to_layer(
    layer: &mut Layer,
    ctx: &DrawContext,
    frame: &mut crate::paint::PaintFrame,
    blit_cache: Option<&mut Option<ContentBlitKey>>,
) {
    render_content_to_layer_offset(layer, ctx, 0, 0, frame, blit_cache);
}

enum RowPolicy {
    All,
    Only(std::ops::Range<usize>),
}

impl RowPolicy {
    fn paints(&self, i: usize) -> bool {
        match self {
            Self::All => true,
            Self::Only(r) => r.contains(&i),
        }
    }
}

pub(crate) fn render_content_to_layer_offset(
    layer: &mut Layer,
    ctx: &DrawContext,
    row_offset: usize,
    col_offset: usize,
    frame: &mut crate::paint::PaintFrame,
    blit_cache: Option<&mut Option<ContentBlitKey>>,
) {
    crate::perf_span!("render_content", crate::perf::PerfFields::default());

    let painted_rows = ctx.viewport.visible_rows().saturating_sub(1);
    let frame_rows = ctx.viewport.visible_rows();

    let new_key = compute_content_blit_key(ctx, painted_rows);

    if !ctx.needs_clear {
        if let Some(cache) = &blit_cache {
            if frame.rows.len() == frame_rows && cache.as_ref() == Some(&new_key) {
                return;
            }
        }
    }

    let blit_delta = if ctx.needs_clear {
        None
    } else {
        blit_cache.as_ref().and_then(|cache| {
            cache
                .as_ref()
                .filter(|_| frame.rows.len() == frame_rows)
                .and_then(|old_key| scroll_blit_delta(old_key, &new_key))
        })
    };

    let policy = if let Some(delta) = blit_delta {
        let painted = &mut frame.rows[..painted_rows];
        if delta > 0 {
            painted.rotate_left(delta.cast_unsigned());
            RowPolicy::Only((painted_rows - delta.cast_unsigned())..painted_rows)
        } else {
            painted.rotate_right(delta.unsigned_abs());
            RowPolicy::Only(0..delta.unsigned_abs())
        }
    } else {
        frame.reset(frame_rows);
        RowPolicy::All
    };

    for row in painted_rows..frame.rows.len() {
        frame.reset_row(row);
    }

    render_content_to_paint_frame(frame, ctx, &policy);
    crate::paint::rasterize_offset(frame, layer, row_offset, col_offset);

    if let Some(cache) = blit_cache {
        *cache = Some(new_key);
    }
}

/// Per-span list positions, advanced monotonically as rows are painted.
#[derive(Clone, Copy, Default)]
struct RowCursors {
    highlight: usize,
    search: usize,
    annotation: usize,
    injection: usize,
    custom: usize,
    terminal: usize,
}

struct RowGeometry {
    gutter_width: usize,
    visible_rows: usize,
    visible_cols: usize,
    buf_total_lines: usize,
    editor_fg: Option<Color>,
    editor_bg: Option<Color>,
}

fn render_content_to_paint_frame(
    frame: &mut crate::paint::PaintFrame,
    ctx: &DrawContext,
    policy: &RowPolicy,
) {
    let gutter_width = ctx.gutter_cols();
    let geom = RowGeometry {
        gutter_width,
        visible_rows: ctx.viewport.visible_rows().saturating_sub(1),
        visible_cols: ctx.viewport.visible_cols(),
        buf_total_lines: ctx.buf.get_total_lines(),
        editor_fg: ctx.state.settings.editor_fg,
        editor_bg: ctx.state.settings.editor_bg,
    };

    match ctx.display_map {
        Some(dm) => render_display_rows(frame, ctx, dm, policy, &geom),
        None => render_plain_rows(frame, ctx, policy, &geom),
    }
}

fn skip_row_cursors(
    ctx: &DrawContext,
    cursors: &mut RowCursors,
    char_boundary: usize,
    byte_boundary: usize,
) {
    advance_idx_past(
        ctx.highlights.unwrap_or(&[]),
        &mut cursors.highlight,
        byte_boundary,
        |(r, _)| r.end,
    );
    advance_idx_past(
        ctx.search_matches(),
        &mut cursors.search,
        char_boundary,
        |m| m.range.end,
    );
    advance_idx_past(
        ctx.annotation_styles.unwrap_or(&[]),
        &mut cursors.annotation,
        byte_boundary,
        |(r, _)| r.end,
    );
    advance_idx_past(
        ctx.injection_highlights.unwrap_or(&[]),
        &mut cursors.injection,
        byte_boundary,
        |(r, _)| r.end,
    );
    advance_idx_past(
        ctx.custom_highlights.unwrap_or(&[]),
        &mut cursors.custom,
        byte_boundary,
        |(r, _)| r.end,
    );
    advance_idx_past(
        ctx.terminal_cell_colors.unwrap_or(&[]),
        &mut cursors.terminal,
        byte_boundary,
        |(r, _)| r.end,
    );
}

fn render_display_rows(
    frame: &mut crate::paint::PaintFrame,
    ctx: &DrawContext,
    dm: &DisplayMap,
    policy: &RowPolicy,
    geom: &RowGeometry,
) {
    let top_visual_row = ctx.viewport.top_visual_row();
    let search_matches = ctx.search_matches();
    let mut gutter_scratch = String::with_capacity(geom.gutter_width);
    let mut cursors = RowCursors::default();
    let mut line_start = cursors;
    let mut last_logical_line: Option<usize> = None;

    if let Some(row_info) = dm.get_visual_row(top_visual_row) {
        cursors.search = search_matches.partition_point(|m| m.range.end <= row_info.char_start);
        line_start = cursors;
    }

    for i in 0..geom.visible_rows {
        let visual_row = top_visual_row + i;
        let Some(row_info) = dm.get_visual_row(visual_row) else {
            if policy.paints(i) {
                if geom.gutter_width > 0 {
                    render_gutter_blank(
                        frame,
                        i,
                        geom.gutter_width,
                        geom.editor_fg,
                        geom.editor_bg,
                    );
                }
                for col in geom.gutter_width..geom.visible_cols {
                    frame.set_cell(
                        i,
                        col,
                        Cell::from_char(' ').with_colors(geom.editor_fg, geom.editor_bg),
                    );
                }
            }
            continue;
        };

        if Some(row_info.logical_line) == last_logical_line {
            cursors = line_start;
        } else {
            last_logical_line = Some(row_info.logical_line);
            line_start = cursors;
        }

        if !policy.paints(i) {
            let byte_boundary = ctx.buf.char_to_byte(row_info.char_end);
            skip_row_cursors(ctx, &mut cursors, row_info.char_end, byte_boundary);
            crate::perf_row_blit_skipped!();
            continue;
        }

        paint_display_row(
            frame,
            ctx,
            geom,
            row_info,
            i,
            &mut cursors,
            &mut gutter_scratch,
        );
    }
}

fn paint_display_row(
    frame: &mut crate::paint::PaintFrame,
    ctx: &DrawContext,
    geom: &RowGeometry,
    row_info: &crate::wrap::VisualRowInfo,
    row_idx: usize,
    cursors: &mut RowCursors,
    gutter_scratch: &mut String,
) {
    crate::perf_row_painted!();
    frame.reset_row(row_idx);

    if geom.gutter_width > 0 {
        if row_info.is_first {
            let line_fg = gutter_color_for_line(ctx.git_gutter_colors, row_info.logical_line)
                .or(geom.editor_fg);
            render_gutter(
                frame,
                GutterSlot {
                    row_idx,
                    gutter_width: geom.gutter_width,
                    fg: line_fg,
                    bg: geom.editor_bg,
                },
                row_info.logical_line,
                geom.buf_total_lines,
                gutter_scratch,
            );
        } else {
            render_gutter_blank(
                frame,
                row_idx,
                geom.gutter_width,
                geom.editor_fg,
                geom.editor_bg,
            );
        }
    }

    render_line(
        frame,
        ctx,
        &RenderLineConfig {
            line_num: row_info.logical_line,
            row_idx,
            gutter_width: geom.gutter_width,
            visible_cols: geom.visible_cols,
            default_fg: geom.editor_fg,
            default_bg: geom.editor_bg,
            segment_left_col: Some(row_info.segment_col_start),
            segment_content_cols: Some(row_info.segment_col_end - row_info.segment_col_start),
        },
        cursors,
    );
}

fn render_plain_rows(
    frame: &mut crate::paint::PaintFrame,
    ctx: &DrawContext,
    policy: &RowPolicy,
    geom: &RowGeometry,
) {
    let top_line = ctx.viewport.top_line();
    let search_matches = ctx.search_matches();
    let first_visible_char = ctx.buf.line_index.get_start(top_line).unwrap_or(0);
    let mut gutter_scratch = String::with_capacity(geom.gutter_width);
    let mut cursors = RowCursors {
        search: search_matches.partition_point(|m| m.range.end <= first_visible_char),
        ..RowCursors::default()
    };

    for i in 0..geom.visible_rows {
        let line_num = top_line + i;
        let config = RenderLineConfig {
            line_num,
            row_idx: i,
            gutter_width: geom.gutter_width,
            visible_cols: geom.visible_cols,
            default_fg: geom.editor_fg,
            default_bg: geom.editor_bg,
            segment_left_col: None,
            segment_content_cols: None,
        };

        if !policy.paints(i) {
            if let Some(char_boundary) = find_line_render_boundary(ctx, &config) {
                let byte_boundary = ctx.buf.char_to_byte(char_boundary);
                skip_row_cursors(ctx, &mut cursors, char_boundary, byte_boundary);
            }
            crate::perf_row_blit_skipped!();
            continue;
        }

        crate::perf_row_painted!();
        frame.reset_row(i);

        if geom.gutter_width > 0 {
            let line_fg = gutter_color_for_line(ctx.git_gutter_colors, line_num).or(geom.editor_fg);
            render_gutter(
                frame,
                GutterSlot {
                    row_idx: i,
                    gutter_width: geom.gutter_width,
                    fg: line_fg,
                    bg: geom.editor_bg,
                },
                line_num,
                geom.buf_total_lines,
                &mut gutter_scratch,
            );
        }

        render_line(frame, ctx, &config, &mut cursors);
    }
}

fn gutter_color_for_line(colors: Option<&[(usize, Color)]>, line: usize) -> Option<Color> {
    colors?.iter().find(|&&(l, _)| l == line).map(|&(_, c)| c)
}

fn render_gutter_blank(
    frame: &mut crate::paint::PaintFrame,
    row_idx: usize,
    gutter_width: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    for i in 0..gutter_width {
        frame.set_cell(
            row_idx,
            i,
            Cell::new(Character::from(' ')).with_colors(fg, bg),
        );
    }
}

/// Placement and colors of one gutter column.
#[derive(Clone, Copy)]
struct GutterSlot {
    row_idx: usize,
    gutter_width: usize,
    fg: Option<Color>,
    bg: Option<Color>,
}

fn render_gutter(
    frame: &mut crate::paint::PaintFrame,
    slot: GutterSlot,
    line_num: usize,
    total_lines: usize,
    scratch: &mut String,
) {
    let GutterSlot {
        row_idx,
        gutter_width,
        fg,
        bg,
    } = slot;
    if line_num < total_lines {
        use std::fmt::Write as _;
        scratch.clear();
        let _ = write!(scratch, "{:width$}", line_num + 1, width = gutter_width - 1);
        for (i, ch) in scratch.chars().enumerate() {
            frame.set_cell(
                row_idx,
                i,
                Cell::new(Character::from(ch)).with_colors(fg, bg),
            );
        }
        frame.set_cell(
            row_idx,
            gutter_width - 1,
            Cell::new(Character::from(' ')).with_colors(fg, bg),
        );
    } else {
        for i in 0..gutter_width {
            frame.set_cell(
                row_idx,
                i,
                Cell::new(Character::from(' ')).with_colors(fg, bg),
            );
        }
    }
}

struct GlyphDrawPlan {
    visible_width: usize,
    straddles_left_edge: bool,
}

fn plan_glyph_draw(width: usize, current_visual_col: usize, left_col: usize) -> GlyphDrawPlan {
    let next_visual_col = current_visual_col + width;
    let visible_width = next_visual_col.saturating_sub(left_col.max(current_visual_col));
    GlyphDrawPlan {
        visible_width,
        straddles_left_edge: visible_width > 0 && visible_width < width,
    }
}

struct RenderLineConfig {
    line_num: usize,
    row_idx: usize,
    gutter_width: usize,
    visible_cols: usize,
    default_fg: Option<Color>,
    default_bg: Option<Color>,
    segment_left_col: Option<usize>,
    segment_content_cols: Option<usize>,
}

fn line_content_cols(ctx: &DrawContext, config: &RenderLineConfig) -> usize {
    let buf = ctx.buf;
    let base_content_cols = config
        .segment_content_cols
        .unwrap_or_else(|| config.visible_cols.saturating_sub(config.gutter_width));
    let line_start_b = buf.line_index.get_start(config.line_num).unwrap_or(0);
    let line_end_b = buf
        .line_index
        .get_end(config.line_num, buf.len())
        .unwrap_or(buf.len());
    let leading_extra: usize = ctx
        .annotation_inline
        .unwrap_or(&[])
        .iter()
        .filter(|(s, _e, _t, _c, lead)| *lead && *s >= line_start_b && *s < line_end_b)
        .map(|(_s, _e, text, _c, _l)| text.chars().count())
        .sum();
    (base_content_cols + leading_extra).min(config.visible_cols.saturating_sub(config.gutter_width))
}

fn find_line_render_boundary(ctx: &DrawContext, config: &RenderLineConfig) -> Option<usize> {
    let buf = ctx.buf;
    if config.line_num >= buf.get_total_lines() {
        return None;
    }

    let line_start_char = buf.line_index.get_start(config.line_num).unwrap_or(0);
    let line_end_char = buf
        .line_index
        .get_end(config.line_num, buf.len())
        .unwrap_or(buf.len());
    let content_cols = line_content_cols(ctx, config);
    let left_col = config
        .segment_left_col
        .unwrap_or_else(|| ctx.viewport.left_col());
    let inline = ctx.annotation_inline.unwrap_or(&[]);
    let concealed = ctx.annotation_concealed.unwrap_or(&[]);

    let layout = TabLayout::new(
        pipeline::new_line_source(buf, config.line_num),
        ctx.tab_width,
    );
    let mut rendered_col = 0usize;
    let mut current_visual_col = 0usize;
    let mut last_pulled_char: Option<usize> = None;

    for (k, item) in layout.enumerate() {
        last_pulled_char = Some(line_start_char + k);

        if rendered_col >= content_cols && current_visual_col >= left_col {
            break;
        }
        if item.char == Character::Newline {
            break;
        }
        if concealed
            .iter()
            .any(|(s, e)| item.byte_offset >= *s && item.byte_offset < *e)
        {
            continue;
        }

        let width = item.width;
        let next_visual_col = current_visual_col + width;

        if next_visual_col > left_col {
            let plan = plan_glyph_draw(width, current_visual_col, left_col);
            let mut display_col = rendered_col + config.gutter_width;
            for (start, _end, text, _color, is_leading) in inline {
                if *is_leading && *start == item.byte_offset {
                    for _ch in text.chars() {
                        if display_col >= config.visible_cols {
                            break;
                        }
                        display_col += 1;
                        rendered_col += 1;
                    }
                }
            }
            rendered_col += plan.visible_width;
        }
        current_visual_col = next_visual_col;
    }

    Some(
        last_pulled_char
            .map_or(line_start_char, |c| c + 1)
            .min(line_end_char),
    )
}

fn render_line(
    frame: &mut crate::paint::PaintFrame,
    ctx: &DrawContext,
    config: &RenderLineConfig,
    cursors: &mut RowCursors,
) {
    if config.line_num >= ctx.buf.get_total_lines() {
        fill_blank_from(frame, config, config.gutter_width);
        return;
    }

    let layout = layout_line(
        ctx,
        config.line_num,
        &mut cursors.highlight,
        &mut cursors.annotation,
        &mut cursors.injection,
        &mut cursors.custom,
        &mut cursors.terminal,
    );

    let content_cols = line_content_cols(ctx, config);
    let mut rendered_col = 0;

    let left_col = config
        .segment_left_col
        .unwrap_or_else(|| ctx.viewport.left_col());

    let mut current_visual_col = 0;

    let mut reached_line_end = true;

    let inline = ctx.annotation_inline.unwrap_or(&[]);
    let mut inline_cols: Vec<(Option<usize>, Option<usize>)> = vec![(None, None); inline.len()];

    let concealed = ctx.annotation_concealed.unwrap_or(&[]);
    for item in layout {
        if item.char == Character::Newline {
            break;
        }

        if rendered_col >= content_cols && current_visual_col >= left_col {
            reached_line_end = false;
            break;
        }

        if concealed
            .iter()
            .any(|(s, e)| item.byte_offset >= *s && item.byte_offset < *e)
        {
            continue;
        }

        let width = item.width;
        let next_visual_col = current_visual_col + width;

        if next_visual_col > left_col {
            let plan = plan_glyph_draw(width, current_visual_col, left_col);
            let visible_width = plan.visible_width;

            rendered_col += draw_leading_inline(
                frame,
                config,
                inline,
                item.byte_offset,
                rendered_col + config.gutter_width,
            );
            let display_col = rendered_col + config.gutter_width;

            for (i, (start, end, _t, _c, _l)) in inline.iter().enumerate() {
                if *start == item.byte_offset {
                    inline_cols[i].0 = Some(display_col);
                }
                if *end != *start && *end == item.byte_offset {
                    inline_cols[i].1 = Some(display_col);
                }
            }

            if display_col < config.visible_cols && visible_width > 0 {
                let fg = item.fg.or(config.default_fg);
                let bg = item.bg.or(config.default_bg);

                let display_char = if item.char == Character::Tab || plan.straddles_left_edge {
                    Character::from(' ')
                } else {
                    item.char
                };

                draw_glyph_cells(
                    frame,
                    config,
                    display_col,
                    visible_width,
                    Cell::new(display_char)
                        .with_colors(fg, bg)
                        .with_attrs(item.attrs),
                    Cell {
                        content: Character::from(' '),
                        fg,
                        bg,
                        attrs: item.attrs,
                    },
                );
            }
            rendered_col += visible_width;
        }

        current_visual_col = next_visual_col;
    }

    let mut tail_col = rendered_col + config.gutter_width;
    let line_end_visible = reached_line_end && current_visual_col >= left_col;
    if let Some(adornments) = ctx.annotation_adornments.filter(|_| line_end_visible) {
        if let Some((_, text, color)) = adornments.iter().find(|(l, _, _)| *l == config.line_num) {
            tail_col = render_trailing_adornment(frame, config, tail_col, text, *color);
        }
    }

    let content_end_col = rendered_col + config.gutter_width;
    draw_inline_tails(frame, config, inline, &inline_cols, content_end_col);
    fill_blank_from(frame, config, tail_col);
}

fn layout_line<'a>(
    ctx: &DrawContext<'a>,
    line_num: usize,
    highlight_idx: &'a mut usize,
    annotation_style_idx: &'a mut usize,
    injection_idx: &'a mut usize,
    custom_color_idx: &'a mut usize,
    terminal_color_idx: &'a mut usize,
) -> impl Iterator<Item = pipeline::LayoutItem> + 'a {
    let source = pipeline::new_line_source(ctx.buf, line_num);
    let highlights = ctx.highlights.unwrap_or(&[]);
    let custom_highlights = ctx.custom_highlights.unwrap_or(&[]);
    let terminal_cell_colors = ctx.terminal_cell_colors.unwrap_or(&[]);

    let syntax = SyntaxDecorator::new(
        source,
        highlights,
        highlight_idx,
        ctx.state.settings.syntax_colors.as_ref(),
        ctx.capture_map,
    );
    let injection_highlights = ctx.injection_highlights.unwrap_or(&[]);
    let injected = pipeline::InjectionDecorator::new(
        syntax,
        injection_highlights,
        injection_idx,
        ctx.state.settings.syntax_colors.as_ref(),
    );
    let colored = pipeline::ColorDecorator::new(injected, custom_highlights, custom_color_idx);
    let term_colored =
        pipeline::TerminalColorDecorator::new(colored, terminal_cell_colors, terminal_color_idx);
    let annotation_styles = ctx.annotation_styles.unwrap_or(&[]);
    let presented =
        pipeline::PresentationDecorator::new(term_colored, annotation_styles, annotation_style_idx);

    TabLayout::new(presented, ctx.tab_width)
}

fn fill_blank_from(
    frame: &mut crate::paint::PaintFrame,
    config: &RenderLineConfig,
    from_col: usize,
) {
    for col in from_col..config.visible_cols {
        frame.set_cell(
            config.row_idx,
            col,
            Cell::from_char(' ').with_colors(config.default_fg, config.default_bg),
        );
    }
}

fn draw_leading_inline(
    frame: &mut crate::paint::PaintFrame,
    config: &RenderLineConfig,
    inline: &[InlineAdornment],
    byte_offset: usize,
    start_col: usize,
) -> usize {
    let mut display_col = start_col;
    let mut written = 0;
    for (start, _end, text, color, is_leading) in inline {
        if *is_leading && *start == byte_offset {
            for ch in text.chars() {
                if display_col >= config.visible_cols {
                    break;
                }
                frame.set_cell(
                    config.row_idx,
                    display_col,
                    Cell::new(Character::from(ch)).with_colors(Some(*color), config.default_bg),
                );
                display_col += 1;
                written += 1;
            }
        }
    }
    written
}

fn draw_glyph_cells(
    frame: &mut crate::paint::PaintFrame,
    config: &RenderLineConfig,
    display_col: usize,
    visible_width: usize,
    glyph: Cell,
    filler: Cell,
) {
    frame.set_cell(config.row_idx, display_col, glyph);
    if visible_width > 1 {
        for k in 1..visible_width {
            if display_col + k < config.visible_cols {
                frame.set_cell(config.row_idx, display_col + k, filler);
            }
        }
    }
}

fn draw_inline_tails(
    frame: &mut crate::paint::PaintFrame,
    config: &RenderLineConfig,
    inline: &[InlineAdornment],
    inline_cols: &[(Option<usize>, Option<usize>)],
    content_end_col: usize,
) {
    for (i, (start, end, text, color, is_leading)) in inline.iter().enumerate() {
        if *is_leading {
            continue;
        }
        let Some(start_col) = inline_cols[i].0 else {
            continue; // anchor not in this visible segment
        };
        let chars: Vec<char> = text.chars().collect();
        let end_col = if *end > *start {
            inline_cols[i].1.unwrap_or(content_end_col)
        } else {
            start_col + chars.len()
        };
        for (k, col) in (start_col..end_col).enumerate() {
            if col >= config.visible_cols {
                break;
            }
            let ch = chars.get(k).copied().unwrap_or(' ');
            frame.set_cell(
                config.row_idx,
                col,
                Cell::new(Character::from(ch)).with_colors(Some(*color), config.default_bg),
            );
        }
    }
}

fn render_trailing_adornment(
    frame: &mut crate::paint::PaintFrame,
    config: &RenderLineConfig,
    mut col: usize,
    text: &str,
    color: Color,
) -> usize {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    let limit = config.visible_cols;
    if col >= limit {
        return col;
    }
    frame.set_cell(
        config.row_idx,
        col,
        Cell::from_char(' ').with_colors(config.default_fg, config.default_bg),
    );
    col += 1;

    let room = limit - col;
    let fits = UnicodeWidthStr::width(text) <= room;
    if !fits && room < 4 {
        return col;
    }
    let text_limit = if fits { limit } else { limit - 3 };
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width == 0 {
            continue;
        }
        if col + width > text_limit {
            break;
        }
        let cell = Cell::new(Character::from(ch)).with_colors(Some(color), config.default_bg);
        frame.set_cell(config.row_idx, col, cell);
        let filler = Cell::from_char(' ').with_colors(Some(color), config.default_bg);
        for k in 1..width {
            frame.set_cell(config.row_idx, col + k, filler);
        }
        col += width;
    }
    if !fits {
        for _ in 0..3 {
            frame.set_cell(
                config.row_idx,
                col,
                Cell::from_char('.').with_colors(Some(color), config.default_bg),
            );
            col += 1;
        }
    }
    col
}

pub fn calculate_cursor_column(buf: &TextBuffer, line: usize, tab_width: usize) -> usize {
    calculate_cursor_column_at(buf, line, tab_width, buf.cursor())
}

pub fn calculate_cursor_column_at(
    buf: &TextBuffer,
    line: usize,
    tab_width: usize,
    cursor_pos: usize,
) -> usize {
    if line >= buf.get_total_lines() {
        return 0;
    }

    let line_start = buf.line_index.get_start(line).unwrap_or(0);
    let target_char_idx = cursor_pos.saturating_sub(line_start);

    let mut col = 0;
    let end = buf.len();

    for (current_idx, ch) in BufferView::chars(buf, line_start..end).enumerate() {
        if current_idx >= target_char_idx {
            break;
        }

        if ch == Character::Newline {
            break;
        }

        if ch == Character::Tab {
            col += tab_width - (col % tab_width);
        } else {
            col += ch.render_width(col, tab_width);
        }
    }

    col
}

#[must_use]
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;

    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::with_capacity(width);
        for word in words {
            let current_w = UnicodeWidthStr::width(current_line.as_str());
            let word_w = UnicodeWidthStr::width(word);
            if current_w + word_w + 1 > width && !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::with_capacity(width);
            }
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    lines
}

pub(crate) fn render_notifications(
    layer: &mut Layer,
    state: &State,
    viewport_rows: usize,
    viewport_cols: usize,
) {
    let mut frame = crate::paint::PaintFrame::new(layer.rows());
    render_notifications_to_paint_frame(&mut frame, state, viewport_rows, viewport_cols);
    crate::paint::rasterize(&frame, layer);
}

const NOTIFICATION_WIDTH_RATIO: f64 = 0.8;

fn render_notifications_to_paint_frame(
    frame: &mut crate::paint::PaintFrame,
    state: &State,
    viewport_rows: usize,
    viewport_cols: usize,
) {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    let notifications = state.error_manager.notifications();
    if notifications.is_empty() {
        return;
    }

    let mut current_row = viewport_rows.saturating_sub(2);
    let max_width = f64_to_usize(usize_to_f64(viewport_cols) * NOTIFICATION_WIDTH_RATIO);

    for notification in notifications.iter_active().rev() {
        let message = &notification.message;
        let (fg_color, bg_color) = match notification.kind {
            crate::notification::NotificationType::Error => (Color::White, Color::Red),
            crate::notification::NotificationType::Warning => (Color::Black, Color::Yellow),
            crate::notification::NotificationType::Info => (Color::White, Color::Blue),
            crate::notification::NotificationType::Success => (Color::Black, Color::Green),
        };

        let lines = wrap_text(message, max_width);
        let content_width = lines
            .iter()
            .map(|l| UnicodeWidthStr::width(l.as_str()))
            .max()
            .unwrap_or(0);
        let box_width = content_width + 3;
        let start_col = viewport_cols.saturating_sub(box_width);

        for line in lines.iter().rev() {
            if current_row == 0 {
                break;
            }

            for i in 0..box_width {
                frame.set_cell(
                    current_row,
                    start_col + i,
                    Cell::new(Character::from(' ')).with_colors(Some(fg_color), Some(bg_color)),
                );
            }

            let mut current_col = start_col + 2;
            for ch in line.chars() {
                let ch_width = ch.width().unwrap_or(1);
                frame.set_cell(
                    current_row,
                    current_col,
                    Cell::from_char(ch).with_colors(Some(fg_color), Some(bg_color)),
                );

                if ch_width > 1 {
                    let empty_cell = Cell {
                        content: Character::from(' '),
                        fg: Some(fg_color),
                        bg: Some(bg_color),
                        attrs: crate::layer::CellAttrs::default(),
                    };
                    for k in 1..ch_width {
                        frame.set_cell(current_row, current_col + k, empty_cell);
                    }
                }
                current_col += ch_width;
            }

            current_row = current_row.saturating_sub(1);
        }

        current_row = current_row.saturating_sub(1);
    }
}

pub(crate) fn render_dividers(
    layer: &mut Layer,
    tree: &crate::split::tree::SplitTree,
    total_rows: usize,
    total_cols: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    let area = SplitArea {
        row: 0,
        col: 0,
        rows: total_rows,
        cols: total_cols,
    };
    render_node_dividers(layer, &tree.root, area, fg, bg);
}

/// Screen rectangle occupied by a split node.
#[derive(Clone, Copy)]
struct SplitArea {
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
}

fn render_node_dividers(
    layer: &mut Layer,
    node: &crate::split::tree::SplitNode,
    area: SplitArea,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    use crate::split::layout::{MIN_WINDOW_COLS, MIN_WINDOW_ROWS};
    use crate::split::tree::{SplitDirection, SplitNode};
    let SplitArea {
        row,
        col,
        rows,
        cols,
    } = area;

    if let SplitNode::Split {
        direction,
        ratio,
        first,
        second,
    } = node
    {
        match direction {
            SplitDirection::Horizontal => {
                let available = rows.saturating_sub(1);
                let first_rows = f64_to_usize((usize_to_f64(available) * ratio).round());
                let first_rows = first_rows
                    .max(MIN_WINDOW_ROWS)
                    .min(available.saturating_sub(MIN_WINDOW_ROWS));
                let second_rows = available.saturating_sub(first_rows);
                let divider_row = row + first_rows;

                for c in col..col + cols {
                    layer.set_cell(
                        divider_row,
                        c,
                        Cell::new(Character::from('─')).with_colors(fg, bg),
                    );
                }

                render_node_dividers(
                    layer,
                    first,
                    SplitArea {
                        row,
                        col,
                        rows: first_rows,
                        cols,
                    },
                    fg,
                    bg,
                );
                render_node_dividers(
                    layer,
                    second,
                    SplitArea {
                        row: divider_row + 1,
                        col,
                        rows: second_rows,
                        cols,
                    },
                    fg,
                    bg,
                );
            }
            SplitDirection::Vertical => {
                let available = cols.saturating_sub(1);
                let first_cols = f64_to_usize((usize_to_f64(available) * ratio).round());
                let first_cols = first_cols
                    .max(MIN_WINDOW_COLS)
                    .min(available.saturating_sub(MIN_WINDOW_COLS));
                let second_cols = available.saturating_sub(first_cols);
                let divider_col = col + first_cols;

                for r in row..row + rows {
                    layer.set_cell(
                        r,
                        divider_col,
                        Cell::new(Character::from('│')).with_colors(fg, bg),
                    );
                }

                render_node_dividers(
                    layer,
                    first,
                    SplitArea {
                        row,
                        col,
                        rows,
                        cols: first_cols,
                    },
                    fg,
                    bg,
                );
                render_node_dividers(
                    layer,
                    second,
                    SplitArea {
                        row,
                        col: divider_col + 1,
                        rows,
                        cols: second_cols,
                    },
                    fg,
                    bg,
                );
            }
        }
    }
}

pub(crate) fn highlight_focused_window_border(
    layer: &mut Layer,
    layout: &crate::split::layout::WindowLayout,
    content_rows: usize,
    total_cols: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    let is_divider = |ch: char| ch == '│' || ch == '─';

    if layout.col > 0 {
        let bc = layout.col - 1;
        for r in layout.row..layout.row + layout.rows {
            if layer
                .get_cell(r, bc)
                .is_some_and(|c| is_divider(c.content.to_char_lossy()))
            {
                layer.set_cell(r, bc, Cell::new(Character::from('│')).with_colors(fg, bg));
            }
        }
    }
    let right = layout.col + layout.cols;
    if right < total_cols {
        for r in layout.row..layout.row + layout.rows {
            if layer
                .get_cell(r, right)
                .is_some_and(|c| is_divider(c.content.to_char_lossy()))
            {
                layer.set_cell(
                    r,
                    right,
                    Cell::new(Character::from('│')).with_colors(fg, bg),
                );
            }
        }
    }
    if layout.row > 0 {
        let br = layout.row - 1;
        for c in layout.col..layout.col + layout.cols {
            if layer
                .get_cell(br, c)
                .is_some_and(|c2| is_divider(c2.content.to_char_lossy()))
            {
                layer.set_cell(br, c, Cell::new(Character::from('─')).with_colors(fg, bg));
            }
        }
    }
    let bottom = layout.row + layout.rows;
    if bottom < content_rows {
        for c in layout.col..layout.col + layout.cols {
            if layer
                .get_cell(bottom, c)
                .is_some_and(|c2| is_divider(c2.content.to_char_lossy()))
            {
                layer.set_cell(
                    bottom,
                    c,
                    Cell::new(Character::from('─')).with_colors(fg, bg),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
