//! Rendering system
//! Handles drawing the editor UI to the terminal using layers

use crate::buffer::api::BufferView;
/// ## render/ Invariants
///
/// - Rendering reads editor state and buffer contents only.
/// - Rendering never mutates editor, buffer, cursor, or viewport state.
/// - Rendering performs no input handling.
/// - Rendering tolerates invalid state but never corrects it.
/// - Displayed cursor position always matches buffer cursor position.
/// - A full redraw is always safe.
/// - Viewport must be updated before calling render functions (viewport updates happen
///   in the state update phase, not during rendering).
/// - All rendering is layer-based and composited before output to terminal.
use crate::buffer::TextBuffer;
use crate::character::Character;
use crate::color::Color;
use crate::key::Key;
use crate::layer::{Cell, Layer};
use crate::mode::Mode;
use crate::state::State;
use crate::viewport::Viewport;
use crate::wrap::DisplayMap;

pub mod components;
pub mod ecs;
pub mod pipeline;
pub mod system;
pub use pipeline::*;
pub use system::RenderSystem;

/// Explicitly tracked cursor information for rendering comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorInfo {
    pub row: usize,
    pub col: usize,
}

/// Minimal state required to trigger a re-render of the buffer content
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDrawState {
    pub revision: u64,
    pub top_line: usize,
    /// Wrapped scroll position; `top_line` does not move under soft wrap.
    pub top_visual_row: usize,
    pub left_col: usize,
    pub rows: usize,
    pub tab_width: usize,
    /// Whether to show line numbers
    pub show_line_numbers: bool,
    /// Hash/Generation of highlights to trigger redraw on syntax update
    pub highlights_hash: u64,
    /// Current gutter width
    pub gutter_width: usize,
    /// Number of search matches (to trigger redraw on search)
    pub search_matches_count: usize,
    /// Number of plugin custom highlights (to trigger redraw when highlights change)
    pub plugin_highlights_len: usize,
    /// Hash of the active conceal ranges (which depend on the cursor line), so
    /// moving onto/off a concealed line redraws. Zero when nothing is concealed.
    pub conceal_hash: u64,
    /// Hash of the generic annotation presentation spans (ui.selection.*,
    /// ui.link, etc.) so changes to them (e.g. Visual-mode selection) redraw
    /// even when no buffer edit or scroll occurred.
    pub annotation_styles_hash: u64,
    /// Hash of inline/adornment virtual text (e.g. LSP inlay hints), so changes
    /// redraw even when every other field stays the same.
    pub annotation_text_hash: u64,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    pub theme: Option<String>,
}

/// Minimal state required to trigger a re-render of the status bar
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
    pub reverse_video: bool,
    /// Whether the status line renders at all; if false the row is blanked.
    pub show_status_line: bool,
    pub show_filename: bool,
    pub show_dirty_indicator: bool,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    /// LSP indexing progress shown in the status bar, e.g. "rust: 2/5".
    pub lsp_status: Option<String>,
    /// Theme colors for LSP status states
    pub lsp_ok_color: Option<crate::color::Color>,
    pub lsp_error_color: Option<crate::color::Color>,
    pub lsp_warn_color: Option<crate::color::Color>,
    /// True when running in an IPC daemon (remote session).
    pub is_remote: bool,
}

/// Minimal state required to trigger a re-render of the command line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDrawState {
    pub content: String,
    pub cursor: CursorInfo,
    pub width: usize,
    pub height: usize,
    pub has_border: bool,
    pub reverse_video: bool,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
}

/// State for rendering the completion dropdown menu
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionMenuDrawState {
    /// (display_text, description) pairs
    pub candidates: Vec<(String, String)>,
    /// Currently highlighted index
    pub selected: Option<usize>,
    /// Terminal columns (for matching the command line width/position)
    pub terminal_cols: usize,
    /// Command line width ratio
    pub cmd_width_ratio: f64,
    /// Command line minimum width
    pub cmd_min_width: usize,
    /// Whether the command line has a border
    pub cmd_has_border: bool,
    /// Command line total height in rows (including borders)
    pub cmd_height: usize,
    /// Theme/Color context
    pub editor_bg: Option<crate::color::Color>,
    pub editor_fg: Option<crate::color::Color>,
    /// First visible candidate (scroll offset)
    pub scroll_offset: usize,
}

/// Minimal state required to trigger a re-render of notifications
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDrawState {
    pub generation: u64,
    pub count: usize,
}

/// Inline (Overlay/Leading) annotation adornment: (start, end, text, color, is_leading).
pub type InlineAdornment = (usize, usize, String, Color, bool);

/// External state passed to RenderSystem::render
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
    /// Injection-layer highlights (capture name resolved, sorted by range start).
    pub injection_highlights: Option<&'a [(std::ops::Range<usize>, String)]>,
    pub skip_content: bool,
    pub cursor_row_offset: usize,
    pub cursor_col_offset: usize,
    pub cursor_viewport: Option<&'a Viewport>,
    /// Terminal cursor (row, col); bypasses text-editor cursor math when set.
    pub terminal_cursor: Option<(usize, usize)>,
    /// Optional per-byte-range foreground color overrides (used by directory/undotree buffers).
    pub custom_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    /// Plugin highlights: rendered as bg color with contrasting fg.
    pub plugin_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    /// Generic annotation presentation styles (fg, bg) composed over base color.
    pub annotation_styles: Option<&'a [(std::ops::Range<usize>, crate::layer::CellStyle)]>,
    /// Trailing end-of-line annotation adornments (line, text, color).
    pub annotation_adornments: Option<&'a [(usize, String, Color)]>,
    /// Inline (Overlay/Leading) adornments (byte_offset, text, color, is_leading).
    pub annotation_inline: Option<&'a [InlineAdornment]>,
    /// Byte ranges hidden by Conceal adornments (already excluding the cursor line).
    pub annotation_concealed: Option<&'a [(usize, usize)]>,
    /// Per-character fg+bg colors from the terminal emulator (terminal documents only).
    pub terminal_cell_colors: Option<&'a [crate::color::CellColorSpan]>,
    /// Per-document line number override (AND-ed with global setting).
    pub show_line_numbers: bool,
    pub display_map: Option<&'a DisplayMap>,
    /// Vertical scroll this frame as `(top, bottom, delta)` content rows, so
    /// the compositor can ride the terminal's scroll region.
    pub scroll_hint: Option<(usize, usize, isize)>,
}

/// Context for rendering passed to helpers
pub struct DrawContext<'a> {
    pub buf: &'a TextBuffer,
    pub viewport: &'a Viewport,
    pub current_mode: Mode,
    pub pending_key: Option<Key>,
    pub custom_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    pub plugin_highlights: Option<&'a [(std::ops::Range<usize>, Color)]>,
    pub annotation_styles: Option<&'a [(std::ops::Range<usize>, crate::layer::CellStyle)]>,
    pub annotation_adornments: Option<&'a [(usize, String, Color)]>,
    pub annotation_inline: Option<&'a [InlineAdornment]>,
    pub annotation_concealed: Option<&'a [(usize, usize)]>,
    pub terminal_cell_colors: Option<&'a [crate::color::CellColorSpan]>,
    pub pending_count: usize,
    pub state: &'a State,
    pub needs_clear: bool,
    pub tab_width: usize,
    pub highlights: Option<&'a [(std::ops::Range<usize>, u32)]>,
    pub capture_map: Option<&'a [&'a str]>,
    /// Injection-layer highlights (capture name resolved, sorted by range start).
    pub injection_highlights: Option<&'a [(std::ops::Range<usize>, String)]>,
    /// Per-document line number override (AND-ed with global setting).
    pub show_line_numbers: bool,
    pub display_map: Option<&'a DisplayMap>,
    /// Overrides state.gutter_width for content rendering (per-window in multi-pane mode).
    pub gutter_width_override: Option<usize>,
    /// When set, use these matches instead of state.search_matches for this pane.
    /// Pass `Some(&[])` for non-active panes to suppress cross-pane highlights.
    pub search_matches_override: Option<&'a [crate::search::SearchMatch]>,
}

impl<'a> DrawContext<'a> {
    pub fn search_matches(&self) -> &'a [crate::search::SearchMatch] {
        self.search_matches_override
            .unwrap_or_else(|| &self.state.search_matches)
    }
}

/// Cursor position information returned from layer-based rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorPosition {
    /// Absolute terminal position (row, col)
    Absolute(u16, u16),
}

/// Render buffer content to the content layer
pub(crate) fn render_content_to_layer(layer: &mut Layer, ctx: &DrawContext) -> Result<(), String> {
    render_content_to_layer_offset(layer, ctx, 0, 0)
}

/// Render buffer content with row/col offsets applied to all cell positions
pub(crate) fn render_content_to_layer_offset(
    layer: &mut Layer,
    ctx: &DrawContext,
    row_offset: usize,
    col_offset: usize,
) -> Result<(), String> {
    crate::perf_span!("render_content", crate::perf::PerfFields::default());
    let mut frame = crate::paint::PaintFrame::new(layer.rows());
    render_content_to_paint_frame(&mut frame, ctx, row_offset, col_offset)?;
    crate::paint::rasterize(&frame, layer);
    Ok(())
}

/// Builds the content layer's PaintFrame; render_content_to_layer_offset
/// rasterizes it onto the actual Layer in a single step.
fn render_content_to_paint_frame(
    frame: &mut crate::paint::PaintFrame,
    ctx: &DrawContext,
    row_offset: usize,
    col_offset: usize,
) -> Result<(), String> {
    let buf = ctx.buf;
    let viewport = ctx.viewport;
    let editor_bg = ctx.state.settings.editor_bg;
    let editor_fg = ctx.state.settings.editor_fg;

    let buf_total_lines = ctx.buf.get_total_lines();
    let gutter_width = if ctx.show_line_numbers && ctx.state.settings.show_line_numbers {
        ctx.gutter_width_override.unwrap_or(ctx.state.gutter_width)
    } else {
        0
    };

    let visible_rows = viewport.visible_rows().saturating_sub(1);
    let visible_cols = viewport.visible_cols();

    if let Some(dm) = ctx.display_map {
        let top_visual_row = viewport.top_visual_row();
        let search_matches = ctx.search_matches();

        let mut highlight_idx: usize = 0;
        let mut search_match_idx: usize = 0;
        let mut last_logical_line: Option<usize> = None;
        let mut highlight_idx_at_line_start: usize = 0;
        let mut search_match_idx_at_line_start: usize = 0;

        if let Some(row_info) = dm.get_visual_row(top_visual_row) {
            let first_char = row_info.char_start;
            search_match_idx = search_matches.partition_point(|m| m.range.end <= first_char);
            search_match_idx_at_line_start = search_match_idx;
        }

        for i in 0..visible_rows {
            let visual_row = top_visual_row + i;
            let row_info = match dm.get_visual_row(visual_row) {
                Some(r) => r,
                None => {
                    if gutter_width > 0 {
                        render_gutter_blank(
                            frame,
                            i + row_offset,
                            col_offset,
                            gutter_width,
                            editor_fg,
                            editor_bg,
                        );
                    }
                    for col in (col_offset + gutter_width)..(col_offset + visible_cols) {
                        frame.set_cell(
                            i + row_offset,
                            col,
                            Cell::from_char(' ').with_colors(editor_fg, editor_bg),
                        );
                    }
                    continue;
                }
            };

            if Some(row_info.logical_line) != last_logical_line {
                last_logical_line = Some(row_info.logical_line);
                highlight_idx_at_line_start = highlight_idx;
                search_match_idx_at_line_start = search_match_idx;
            } else {
                highlight_idx = highlight_idx_at_line_start;
                search_match_idx = search_match_idx_at_line_start;
            }

            if gutter_width > 0 {
                if row_info.is_first {
                    render_gutter(
                        frame,
                        row_info.logical_line,
                        i + row_offset,
                        col_offset,
                        gutter_width,
                        buf_total_lines,
                        editor_fg,
                        editor_bg,
                    );
                } else {
                    render_gutter_blank(
                        frame,
                        i + row_offset,
                        col_offset,
                        gutter_width,
                        editor_fg,
                        editor_bg,
                    );
                }
            }

            render_line(
                frame,
                ctx,
                RenderLineConfig {
                    line_num: row_info.logical_line,
                    row_idx: i + row_offset,
                    gutter_width: col_offset + gutter_width,
                    visible_cols: col_offset + visible_cols,
                    default_fg: editor_fg,
                    default_bg: editor_bg,
                    segment_left_col: Some(row_info.segment_col_start),
                    segment_content_cols: Some(
                        row_info.segment_col_end - row_info.segment_col_start,
                    ),
                },
                &mut highlight_idx,
                &mut search_match_idx,
            );
        }
    } else {
        let top_line = viewport.top_line();
        let search_matches = ctx.search_matches();
        let first_visible_char = buf.line_index.get_start(top_line).unwrap_or(0);

        let mut search_match_idx =
            search_matches.partition_point(|m| m.range.end <= first_visible_char);
        let mut highlight_idx = 0;

        for i in 0..visible_rows {
            let line_num = top_line + i;

            if gutter_width > 0 {
                render_gutter(
                    frame,
                    line_num,
                    i + row_offset,
                    col_offset,
                    gutter_width,
                    buf_total_lines,
                    editor_fg,
                    editor_bg,
                );
            }

            render_line(
                frame,
                ctx,
                RenderLineConfig {
                    line_num,
                    row_idx: i + row_offset,
                    gutter_width: col_offset + gutter_width,
                    visible_cols: col_offset + visible_cols,
                    default_fg: editor_fg,
                    default_bg: editor_bg,
                    segment_left_col: None,
                    segment_content_cols: None,
                },
                &mut highlight_idx,
                &mut search_match_idx,
            );
        }
    }

    Ok(())
}

fn render_gutter_blank(
    frame: &mut crate::paint::PaintFrame,
    row_idx: usize,
    col_start: usize,
    gutter_width: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    for i in 0..gutter_width {
        frame.set_cell(
            row_idx,
            col_start + i,
            Cell::new(Character::from(' ')).with_colors(fg, bg),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_gutter(
    frame: &mut crate::paint::PaintFrame,
    line_num: usize,
    row_idx: usize,
    col_start: usize,
    gutter_width: usize,
    total_lines: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    if line_num < total_lines {
        let line_str = format!("{:width$}", line_num + 1, width = gutter_width - 1);
        for (i, ch) in line_str.chars().enumerate() {
            frame.set_cell(
                row_idx,
                col_start + i,
                Cell::new(Character::from(ch)).with_colors(fg, bg),
            );
        }
        frame.set_cell(
            row_idx,
            col_start + gutter_width - 1,
            Cell::new(Character::from(' ')).with_colors(fg, bg),
        );
    } else {
        for i in 0..gutter_width {
            frame.set_cell(
                row_idx,
                col_start + i,
                Cell::new(Character::from(' ')).with_colors(fg, bg),
            );
        }
    }
}

/// Decision for drawing a single glyph at a given visual column, accounting
/// for horizontal scroll clipping at the left edge.
struct GlyphDrawPlan {
    /// Columns this glyph occupies in the visible area (0 if fully clipped
    /// or zero-width).
    visible_width: usize,
    /// True if this glyph is wider than its visible_width: it straddles the
    /// left scroll edge and must render as a space, not its clipped glyph.
    straddles_left_edge: bool,
}

/// Computes how much of a glyph of `width` columns, starting at
/// `current_visual_col`, is visible given the viewport's `left_col` scroll.
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

fn render_line(
    frame: &mut crate::paint::PaintFrame,
    ctx: &DrawContext,
    config: RenderLineConfig,
    highlight_idx: &mut usize,
    _search_match_idx: &mut usize,
) {
    let buf = ctx.buf;
    if config.line_num >= buf.get_total_lines() {
        // Render empty line (past end of buffer)
        for col in config.gutter_width..config.visible_cols {
            frame.set_cell(
                config.row_idx,
                col,
                Cell::from_char(' ').with_colors(config.default_fg, config.default_bg),
            );
        }
        return;
    }

    let source = LineSource::new(buf, config.line_num);
    let highlights = ctx.highlights.unwrap_or(&[]);
    let custom_highlights = ctx.custom_highlights.unwrap_or(&[]);
    let plugin_highlights = ctx.plugin_highlights.unwrap_or(&[]);
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
        ctx.state.settings.syntax_colors.as_ref(),
    );
    let colored = pipeline::ColorDecorator::new(injected, custom_highlights);
    let term_colored = pipeline::TerminalColorDecorator::new(colored, terminal_cell_colors);
    let plugin = pipeline::PluginHighlightDecorator::new(term_colored, plugin_highlights);
    let annotation_styles = ctx.annotation_styles.unwrap_or(&[]);
    // Search highlights now render via ui.search annotations (PresentationDecorator),
    // so there is no dedicated search decorator in the chain.
    let presented = pipeline::PresentationDecorator::new(plugin, annotation_styles);

    // Layout
    let layout = TabLayout::new(presented, ctx.tab_width);

    let base_content_cols = config
        .segment_content_cols
        .unwrap_or_else(|| config.visible_cols.saturating_sub(config.gutter_width));
    // Leading adornments insert virtual columns, so widen the draw budget by their
    // total width on this line (the segment is sized to buffer content only).
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
    let content_cols = (base_content_cols + leading_extra)
        .min(config.visible_cols.saturating_sub(config.gutter_width));
    let mut rendered_col = 0;

    let left_col = config
        .segment_left_col
        .unwrap_or_else(|| ctx.viewport.left_col());

    // Internal tracker for absolute visual column (including horizontal scroll)
    let mut current_visual_col = 0;

    // Whether this render reached the logical line's end rather than running out
    // of horizontal space. Trailing adornments only draw on that final segment.
    let mut reached_line_end = true;

    // Inline (Overlay/Leading) adornments: record each adornment's display columns
    // as content is laid out, then draw them once the line is done.
    let inline = ctx.annotation_inline.unwrap_or(&[]);
    let mut inline_cols: Vec<(Option<usize>, Option<usize>)> = vec![(None, None); inline.len()];

    let concealed = ctx.annotation_concealed.unwrap_or(&[]);
    for item in layout {
        if rendered_col >= content_cols {
            reached_line_end = false;
            break;
        }

        if item.char == Character::Newline {
            break;
        }

        // Zero-width conceal: drop this char's cell and width so following text
        // reflows left. Cursor-line ranges are pre-excluded (revealed).
        if concealed
            .iter()
            .any(|(s, e)| item.byte_offset >= *s && item.byte_offset < *e)
        {
            continue;
        }

        let width = item.width;
        let next_visual_col = current_visual_col + width;

        // Check visibility against viewport
        if next_visual_col > left_col {
            let plan = plan_glyph_draw(width, current_visual_col, left_col);
            let visible_width = plan.visible_width;

            // Calculate where to draw in the layer (relative to gutter)
            let mut display_col = rendered_col + config.gutter_width;

            // Leading adornments insert virtual text before this item, pushing real
            // content right. Only the display column advances, not the visual column.
            for (start, _end, text, color, is_leading) in inline {
                if *is_leading && *start == item.byte_offset {
                    for ch in text.chars() {
                        if display_col >= config.visible_cols {
                            break;
                        }
                        frame.set_cell(
                            config.row_idx,
                            display_col,
                            Cell::new(Character::from(ch))
                                .with_colors(Some(*color), config.default_bg),
                        );
                        display_col += 1;
                        rendered_col += 1;
                    }
                }
            }

            // Record the display columns of overlay span endpoints (post-leading).
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

                frame.set_cell(
                    config.row_idx,
                    display_col,
                    Cell::new(display_char)
                        .with_colors(fg, bg)
                        .with_attrs(item.attrs),
                );

                // Fill the rest of the visible span with spaces.
                if visible_width > 1 {
                    let empty_cell = Cell {
                        content: Character::from(' '),
                        fg,
                        bg,
                        attrs: item.attrs,
                    };
                    for k in 1..visible_width {
                        if display_col + k < config.visible_cols {
                            frame.set_cell(config.row_idx, display_col + k, empty_cell.clone());
                        }
                    }
                }
            }
            rendered_col += visible_width;
        }

        current_visual_col = next_visual_col;
    }

    // Render a trailing adornment (display-only virtual text with its own color),
    // then fill with background. Drawn only on the line's last visual segment.
    let mut tail_col = rendered_col + config.gutter_width;
    if let Some(adornments) = ctx.annotation_adornments.filter(|_| reached_line_end) {
        if let Some((_, text, color)) = adornments.iter().find(|(l, _, _)| *l == config.line_num) {
            if tail_col < config.visible_cols {
                frame.set_cell(
                    config.row_idx,
                    tail_col,
                    Cell::from_char(' ').with_colors(config.default_fg, config.default_bg),
                );
                tail_col += 1;
            }
            for ch in text.chars() {
                if tail_col >= config.visible_cols {
                    break;
                }
                frame.set_cell(
                    config.row_idx,
                    tail_col,
                    Cell::new(Character::from(ch)).with_colors(Some(*color), config.default_bg),
                );
                tail_col += 1;
            }
        }
    }

    // Draw overlay adornments over the laid-out content (leading was drawn inline
    // during the loop, shifting content right).
    let content_end_col = rendered_col + config.gutter_width;
    for (i, (start, end, text, color, is_leading)) in inline.iter().enumerate() {
        if *is_leading {
            continue;
        }
        let Some(start_col) = inline_cols[i].0 else {
            continue; // anchor not in this visible segment
        };
        let chars: Vec<char> = text.chars().collect();
        {
            // Overlay conceals [start_col, end_col): a range covers its span, a point
            // its own width. Short pads with blanks, long truncates within the span.
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

    // Fill remaining line with background
    for col in tail_col..config.visible_cols {
        frame.set_cell(
            config.row_idx,
            col,
            Cell::from_char(' ').with_colors(config.default_fg, config.default_bg),
        );
    }
}

/// Calculate the cursor column position accounting for tab width and wide characters
pub fn calculate_cursor_column(buf: &TextBuffer, line: usize, tab_width: usize) -> usize {
    calculate_cursor_column_at(buf, line, tab_width, buf.cursor())
}

/// Like `calculate_cursor_column` but with an explicit cursor position.
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

/// Helper to wrap text to a specific width
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

/// Render notifications to the notification layer
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

/// Builds the notification layer's PaintFrame; render_notifications
/// rasterizes it onto the actual Layer in a single step.
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
    let max_width = (viewport_cols as f32 * 0.8) as usize;

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
                        frame.set_cell(current_row, current_col + k, empty_cell.clone());
                    }
                }
                current_col += ch_width;
            }

            current_row = current_row.saturating_sub(1);
        }

        current_row = current_row.saturating_sub(1);
    }
}

/// Render split dividers between windows. Layer-based, not PaintFrame-based:
/// highlight_focused_window_border below reads these cells back to recolor them.
pub(crate) fn render_dividers(
    layer: &mut Layer,
    tree: &crate::split::tree::SplitTree,
    total_rows: usize,
    total_cols: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    render_node_dividers(layer, &tree.root, 0, 0, total_rows, total_cols, fg, bg);
}

#[allow(clippy::too_many_arguments)]
fn render_node_dividers(
    layer: &mut Layer,
    node: &crate::split::tree::SplitNode,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
    fg: Option<Color>,
    bg: Option<Color>,
) {
    use crate::split::layout::{MIN_WINDOW_COLS, MIN_WINDOW_ROWS};
    use crate::split::tree::{SplitDirection, SplitNode};

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
                let first_rows = ((available as f64) * ratio).round() as usize;
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

                render_node_dividers(layer, first, row, col, first_rows, cols, fg, bg);
                render_node_dividers(
                    layer,
                    second,
                    divider_row + 1,
                    col,
                    second_rows,
                    cols,
                    fg,
                    bg,
                );
            }
            SplitDirection::Vertical => {
                let available = cols.saturating_sub(1);
                let first_cols = ((available as f64) * ratio).round() as usize;
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

                render_node_dividers(layer, first, row, col, rows, first_cols, fg, bg);
                render_node_dividers(
                    layer,
                    second,
                    row,
                    divider_col + 1,
                    rows,
                    second_cols,
                    fg,
                    bg,
                );
            }
        }
    }
}

/// Reads existing divider glyphs back from `layer` to selectively recolor
/// them, so this reads/writes Layer directly rather than going via PaintFrame.
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
#[path = "tests.rs"]
mod tests;
