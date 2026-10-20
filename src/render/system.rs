use crate::error::RiftError;
use crate::layer::{CellStyle, LayerCompositor, LayerPriority};
use crate::num::{f64_to_usize, usize_to_f64};
use crate::render::components::{Rect, Renderable};
use crate::render::ecs::World;
use crate::render::{
    calculate_cursor_column_at, CommandDrawState, ContentDrawState, CursorPosition, DrawContext,
    InlineAdornment, NotificationDrawState, RenderState, StatusDrawState,
};
use crate::term::TerminalBackend;
use crate::viewport::Viewport;
use std::hash::{Hash, Hasher};

use crate::command_line::CommandLine;
use crate::mode::Mode;
use crate::status::StatusBar;
use crate::term::CursorShape;

fn to_u16_saturating(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[derive(Debug)]
pub(crate) struct WindowPaintCache {
    pub(crate) frame: crate::paint::PaintFrame,
    pub(crate) blit_key: Option<crate::render::ContentBlitKey>,
    pub(crate) layout: crate::split::layout::WindowLayout,
}

fn hash_annotation_styles(spans: &[(std::ops::Range<usize>, CellStyle)]) -> u64 {
    let mut h: u64 = 0;
    for (range, style) in spans {
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(range.start as u64);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(range.end as u64 + 1);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        style.hash(&mut hasher);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(hasher.finish());
    }
    h
}

fn hash_virtual_text(
    inline: &[InlineAdornment],
    adornments: &[crate::render::LineAdornment<'_>],
) -> u64 {
    let mut h: u64 = 0;
    for (start, end, text, color, leading) in inline {
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(*start as u64);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(*end as u64 + 1);
        for b in text.bytes() {
            h = h
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(u64::from(b) + 1);
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        color.hash(&mut hasher);
        leading.hash(&mut hasher);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(hasher.finish());
    }
    for (line, text, color) in adornments {
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(*line as u64);
        for b in text.bytes() {
            h = h
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(u64::from(b) + 1);
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        color.hash(&mut hasher);
        h = h
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(hasher.finish());
    }
    h
}

pub struct RenderSystem {
    pub compositor: LayerCompositor,
    pub viewport: Viewport,
    pub world: World,

    content_entity: Option<crate::render::ecs::EntityId>,
    status_entity: Option<crate::render::ecs::EntityId>,
    command_entity: Option<crate::render::ecs::EntityId>,
    notification_entity: Option<crate::render::ecs::EntityId>,
    completion_entity: Option<crate::render::ecs::EntityId>,

    last_render_version: u64,
    last_cursor_pos: Option<CursorPosition>,
    last_command_cursor: Option<CursorPosition>,
    last_soft_cursor: Option<(usize, usize)>,
    last_cursor_shape: Option<CursorShape>,
    cursor_animator: crate::cursor::CursorAnimator,

    pub(crate) content_paint_frame: crate::paint::PaintFrame,
    pub(crate) content_blit_key: Option<crate::render::ContentBlitKey>,
    pub(crate) window_paint_caches:
        std::collections::HashMap<crate::split::window::WindowId, WindowPaintCache>,
    status_paint_frame: crate::paint::PaintFrame,

    render_order: Vec<(crate::render::ecs::EntityId, crate::layer::LayerPriority)>,
}

fn match_index_at_cursor(
    matches: &[crate::search::SearchMatch],
    cursor_offset: usize,
) -> Option<usize> {
    let idx =
        matches.partition_point(|m| m.range.start < cursor_offset && m.range.end <= cursor_offset);
    matches
        .get(idx)
        .filter(|m| m.range.contains(&cursor_offset) || m.range.start == cursor_offset)
        .map(|_| idx + 1)
}

impl RenderSystem {
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            compositor: LayerCompositor::new(rows, cols),
            viewport: Viewport::new(rows, cols),
            world: World::new(),
            content_entity: None,
            status_entity: None,
            command_entity: None,
            notification_entity: None,
            completion_entity: None,
            last_render_version: 0,
            last_cursor_pos: None,
            last_command_cursor: None,
            last_soft_cursor: None,
            last_cursor_shape: None,
            cursor_animator: crate::cursor::CursorAnimator::new(),
            content_paint_frame: crate::paint::PaintFrame::new(0),
            content_blit_key: None,
            window_paint_caches: std::collections::HashMap::new(),
            status_paint_frame: crate::paint::PaintFrame::new(0),
            render_order: Vec::new(),
        }
    }

    #[must_use]
    pub const fn needs_animation_frame(&self) -> bool {
        self.cursor_animator.is_animating()
    }

    #[must_use]
    pub const fn last_soft_cursor(&self) -> Option<(usize, usize)> {
        self.last_soft_cursor
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.viewport.set_size(rows, cols);
        self.compositor.resize(rows, cols);

        self.world.clear();
        self.content_entity = None;
        self.status_entity = None;
        self.command_entity = None;
        self.notification_entity = None;
        self.completion_entity = None;
        self.last_render_version = 0;
        self.last_cursor_pos = None;
        self.last_command_cursor = None;
        self.last_soft_cursor = None;
        self.last_cursor_shape = None;
        self.cursor_animator.reset();
        self.content_blit_key = None;
        self.window_paint_caches.clear();
    }

    fn update_world(&mut self, ctx: &DrawContext) {
        crate::perf_span!("ecs_update_world", crate::perf::PerfFields::default());
        self.world.tick();

        if self.content_entity.is_none() {
            self.content_entity = Some(self.world.create_entity());
        }
        let content_entity = self
            .content_entity
            .expect("entity not initialized: call update_world before draw");

        let content_state = Self::content_draw_state(ctx);
        self.world
            .add_renderable(content_entity, Renderable::TextBuffer(content_state));
        self.world.add_layer(content_entity, LayerPriority::CONTENT);
        self.world.add_rect(
            content_entity,
            Rect::new(
                0,
                0,
                ctx.viewport.visible_rows(),
                ctx.viewport.visible_cols(),
            ),
        );

        if self.status_entity.is_none() {
            self.status_entity = Some(self.world.create_entity());
        }
        let status_entity = self
            .status_entity
            .expect("entity not initialized: call update_world before draw");

        let status_state = Self::status_draw_state(ctx);
        self.world
            .add_renderable(status_entity, Renderable::StatusBar(status_state));
        self.world
            .add_layer(status_entity, LayerPriority::STATUS_BAR);

        self.update_command_entity(ctx);
        self.update_completion_entity(ctx);

        if self.notification_entity.is_none() {
            self.notification_entity = Some(self.world.create_entity());
        }
        let notification_entity = self.notification_entity.unwrap();

        let notification_state = NotificationDrawState {
            generation: ctx.state.error_manager.notifications().generation,
            count: ctx
                .state
                .error_manager
                .notifications()
                .iter_active()
                .count(),
        };

        self.world.add_renderable(
            notification_entity,
            Renderable::Notification(notification_state),
        );
        self.world
            .add_layer(notification_entity, LayerPriority::NOTIFICATION);
    }

    fn content_draw_state(ctx: &DrawContext) -> ContentDrawState {
        let effective_line_numbers = ctx.state.line_numbers_on(ctx.show_line_numbers);
        ContentDrawState {
            revision: ctx.buf.revision,
            top_line: ctx.viewport.top_line(),
            top_visual_row: ctx.viewport.top_visual_row(),
            left_col: ctx.viewport.left_col(),
            rows: ctx.viewport.visible_rows(),
            tab_width: ctx.tab_width,
            show_line_numbers: effective_line_numbers,
            gutter_width: ctx.gutter_cols(),
            search_matches_count: ctx.state.search_matches.len(),
            conceal_hash: {
                let mut h: u64 = 0;
                for (s, e) in ctx.annotation_concealed.unwrap_or(&[]) {
                    h = h
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(*s as u64);
                    h = h
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(*e as u64 + 1);
                }
                h
            },
            annotation_styles_hash: hash_annotation_styles(ctx.annotation_styles.unwrap_or(&[])),
            annotation_text_hash: hash_virtual_text(
                ctx.annotation_inline.unwrap_or(&[]),
                ctx.annotation_adornments.unwrap_or(&[]),
            ),
            editor_bg: ctx.state.settings.editor_bg,
            editor_fg: ctx.state.settings.editor_fg,
            theme: crate::perf_clone!(ctx.state.settings.theme.clone()),
            highlights_hash: ctx.state.settings.syntax_colors.as_ref().map_or(0, |_| {
                let highlights = ctx.highlights.unwrap_or(&[]);
                let mut hash: u64 = highlights.len() as u64;
                for (range, capture) in highlights {
                    hash = hash
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(range.start as u64);
                    hash = hash
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(range.end as u64 + 1);
                    hash = hash
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(u64::from(*capture) + 1);
                }
                hash
            }),
        }
    }

    fn status_draw_state(ctx: &DrawContext) -> StatusDrawState {
        let (search_match_index, search_total_matches) = if ctx.state.search_matches.is_empty() {
            (None, 0)
        } else {
            let cursor_offset = ctx.buf.cursor();
            let idx = match_index_at_cursor(&ctx.state.search_matches, cursor_offset);
            (idx, ctx.state.search_matches.len())
        };

        StatusDrawState {
            mode: ctx.current_mode,
            pending_key: ctx.pending_key,
            pending_count: ctx.pending_count,
            last_keypress: ctx.state.last_keypress,
            file_name: crate::perf_clone!(ctx.state.file_name.clone()),
            is_dirty: ctx.state.is_dirty,
            cursor: crate::render::CursorInfo {
                row: ctx.state.cursor_pos.0,
                col: ctx.state.cursor_pos.1,
            },
            total_lines: ctx.state.total_lines,
            debug_mode: ctx.state.debug_mode,
            cols: ctx.viewport.visible_cols(),
            search_query: crate::perf_clone!(ctx.state.last_search_query.clone()),
            search_match_index,
            search_total_matches,
            status_line: ctx.state.settings.status_line.clone(),
            editor_bg: ctx.state.settings.editor_bg,
            editor_fg: ctx.state.settings.editor_fg,
            lsp_status: crate::perf_clone!(ctx.state.lsp_status.clone()),
            lsp_ok_color: ctx
                .state
                .settings
                .syntax_colors
                .as_ref()
                .and_then(|c| c.get_color("ui.lsp.ok")),
            lsp_error_color: ctx
                .state
                .settings
                .syntax_colors
                .as_ref()
                .and_then(|c| c.get_color("ui.lsp.error")),
            lsp_warn_color: ctx
                .state
                .settings
                .syntax_colors
                .as_ref()
                .and_then(|c| c.get_color("ui.lsp.warn")),
            is_remote: ctx.state.is_remote,
        }
    }

    fn update_command_entity(&mut self, ctx: &DrawContext) {
        if ctx.current_mode == Mode::Command
            || ctx.current_mode == Mode::Search
            || ctx.current_mode == Mode::Rename
        {
            if self.command_entity.is_none() {
                self.command_entity = Some(self.world.create_entity());
            }
            let command_entity = self
                .command_entity
                .expect("entity not initialized: call update_world before draw");

            let command_state = CommandDrawState {
                content: crate::perf_clone!(ctx.state.command_line.clone()),
                cursor: crate::render::CursorInfo {
                    row: 0,
                    col: ctx.state.command_line_cursor,
                },
                width: ctx.viewport.visible_cols(),
                height: ctx.state.settings.command_line_window.height,
                has_border: ctx.state.settings.command_line_window.border,
                reverse_video: ctx.state.settings.command_line_window.reverse_video,
                editor_bg: ctx.state.settings.editor_bg,
                editor_fg: ctx.state.settings.editor_fg,
            };

            self.world
                .add_renderable(command_entity, Renderable::Window(command_state));
            self.world
                .add_layer(command_entity, LayerPriority::FLOATING_WINDOW);
        } else if let Some(entity) = self.command_entity {
            self.world.destroy_entity(entity);
            self.command_entity = None;
            self.last_command_cursor = None;
            self.compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
        }
    }

    fn update_completion_entity(&mut self, ctx: &DrawContext) {
        let show_dropdown = ctx.current_mode == Mode::Command
            && ctx
                .state
                .completion_session
                .as_ref()
                .is_some_and(|s| s.dropdown_open && !s.candidates.is_empty());

        if show_dropdown {
            if self.completion_entity.is_none() {
                self.completion_entity = Some(self.world.create_entity());
            }
            let entity = self.completion_entity.unwrap();
            let session = ctx.state.completion_session.as_ref().unwrap();

            let draw_state = crate::render::CompletionMenuDrawState {
                candidates: session
                    .candidates
                    .iter()
                    .map(|c| crate::perf_clone!((c.text.clone(), c.description.clone())))
                    .collect(),
                selected: session.selected,
                terminal_cols: ctx.viewport.visible_cols(),
                cmd_width_ratio: ctx.state.settings.command_line_window.width_ratio,
                cmd_min_width: ctx.state.settings.command_line_window.min_width,
                cmd_height: ctx.state.settings.command_line_window.height,
                editor_bg: ctx.state.settings.editor_bg,
                editor_fg: ctx.state.settings.editor_fg,
                scroll_offset: session.scroll_offset,
            };

            self.world
                .add_renderable(entity, Renderable::CompletionMenu(draw_state));
            self.world.add_layer(entity, LayerPriority::HOVER);
        } else if let Some(entity) = self.completion_entity.take() {
            self.world.destroy_entity(entity);
            self.compositor.clear_layer(LayerPriority::HOVER);
        }
    }

    /// # Errors
    /// Returns `Err` when the compositor, terminal, or cursor update fails.
    pub fn render<T: TerminalBackend>(
        &mut self,
        term: &mut T,
        state: &RenderState,
    ) -> Result<CursorPosition, RiftError> {
        if self.compositor.rows() != self.viewport.visible_rows()
            || self.compositor.cols() != self.viewport.visible_cols()
        {
            self.compositor
                .resize(self.viewport.visible_rows(), self.viewport.visible_cols());
        }

        let viewport = self.viewport;
        let skip_content = state.skip_content;
        let scroll_hint = state.scroll_hint;
        let cursor_row_offset = state.cursor_row_offset;
        let cursor_col_offset = state.cursor_col_offset;
        let cursor_viewport = state.cursor_viewport;
        let terminal_cursor = state.terminal_cursor;
        let ctx = Self::draw_context(&viewport, state);

        self.update_world(&ctx);

        if ctx.needs_clear {
            self.last_render_version = 0;
        }

        let command_cursor_info = self.draw_layers(&ctx, skip_content);
        let cursor_info = command_cursor_info.unwrap_or_else(|| {
            Self::fallback_cursor(
                &ctx,
                terminal_cursor,
                cursor_viewport,
                cursor_row_offset,
                cursor_col_offset,
            )
        });

        let (cursor_row, cursor_col) = match cursor_info {
            CursorPosition::Absolute(r, c) => (r as usize, c as usize),
        };

        let software = crate::cursor::is_software_cursor(ctx.current_mode);

        let pane_vp = cursor_viewport.unwrap_or(&viewport);
        let viewport_scrolled = pane_vp.top_line() != pane_vp.prev_top_line()
            || pane_vp.top_visual_row() != pane_vp.prev_top_visual_row();

        if software && !viewport_scrolled {
            self.cursor_animator.set_target(cursor_row, cursor_col);
        } else {
            self.cursor_animator.snap_to(cursor_row, cursor_col);
        }
        self.cursor_animator.step(ctx.state.settings.cursor_speed);
        let (anim_row, anim_col) = self.cursor_animator.display_pos();

        if software {
            self.draw_soft_cursor(&ctx, anim_row, anim_col);
        } else if let Some((pr, pc)) = self.last_soft_cursor.take() {
            self.compositor
                .get_layer_mut(LayerPriority::CURSOR)
                .clear_cell(pr, pc);
        }

        self.compositor
            .render_to_terminal(term, ctx.needs_clear, scroll_hint)
            .map_err(|e| {
                RiftError::new(
                    crate::error::ErrorType::Renderer,
                    "RENDER_FAILED",
                    e.to_string(),
                )
            })?;

        if software {
            self.last_cursor_shape = None;
        } else {
            let desired_shape = CursorShape::SteadyBar;
            if self.last_cursor_shape != Some(desired_shape) {
                term.set_cursor_shape(desired_shape)?;
                self.last_cursor_shape = Some(desired_shape);
            }
            term.move_cursor(to_u16_saturating(cursor_row), to_u16_saturating(cursor_col))?;
            term.show_cursor()?;
        }

        self.last_cursor_pos = Some(cursor_info);
        term.flush()?;

        self.last_render_version = self.world.current_version;

        Ok(cursor_info)
    }

    const fn draw_context<'a>(viewport: &'a Viewport, state: &RenderState<'a>) -> DrawContext<'a> {
        DrawContext {
            buf: state.buf,
            viewport,
            current_mode: state.current_mode,
            pending_key: state.pending_key,
            pending_count: state.pending_count,
            state: state.state,
            needs_clear: state.needs_clear,
            tab_width: state.tab_width,
            highlights: state.highlights,
            capture_map: state.capture_map,
            injection_highlights: state.injection_highlights,
            custom_highlights: state.custom_highlights,
            git_gutter_colors: state.git_gutter_colors,
            annotation_styles: state.annotation_styles,
            annotation_adornments: state.annotation_adornments,
            annotation_inline: state.annotation_inline,
            annotation_concealed: state.annotation_concealed,
            terminal_cell_colors: state.terminal_cell_colors,
            show_line_numbers: state.show_line_numbers,
            display_map: state.display_map,
            gutter_width_override: None,
            search_matches_override: None,
            syntax_generation: state.syntax_generation,
            annotations_revision: state.annotations_revision,
            kind_registry_generation: state.kind_registry_generation,
        }
    }

    fn draw_layers(&mut self, ctx: &DrawContext, skip_content: bool) -> Option<CursorPosition> {
        self.render_order.clear();
        self.render_order.extend(
            self.world
                .entities()
                .iter()
                .filter_map(|&e| self.world.layers.get(e).map(|&layer| (e, layer))),
        );
        self.render_order.sort_by_key(|&(_, layer)| layer);

        let mut command_cursor_info = None;

        for i in 0..self.render_order.len() {
            let (entity, priority) = self.render_order[i];
            let Some(renderable) = self.world.renderables.get(entity) else {
                continue;
            };
            let priority = &priority;
            let version = self.world.renderables.get_version(entity).unwrap_or(0);
            let layer_needs_redraw =
                version > self.last_render_version || self.last_render_version == 0;

            match renderable {
                Renderable::TextBuffer(_) => {
                    if layer_needs_redraw && !skip_content {
                        crate::render::render_content_to_layer(
                            self.compositor.get_layer_mut(*priority),
                            ctx,
                            &mut self.content_paint_frame,
                            Some(&mut self.content_blit_key),
                        );
                    }
                }
                Renderable::StatusBar(state) => {
                    if layer_needs_redraw {
                        self.compositor.clear_layer(*priority);
                        StatusBar::render_to_layer(
                            self.compositor.get_layer_mut(*priority),
                            state,
                            &mut self.status_paint_frame,
                        );
                    }
                }
                Renderable::Window(state) => {
                    if layer_needs_redraw {
                        let pos =
                            Self::draw_command_window(&mut self.compositor, ctx, *priority, state);
                        command_cursor_info = Some(pos);
                        self.last_command_cursor = Some(pos);
                    } else {
                        command_cursor_info = self.last_command_cursor;
                    }
                }
                Renderable::Notification(_) => {
                    if layer_needs_redraw {
                        self.compositor.clear_layer(*priority);
                        crate::render::render_notifications(
                            self.compositor.get_layer_mut(*priority),
                            ctx.state,
                            ctx.viewport.visible_rows(),
                            ctx.viewport.visible_cols(),
                        );
                    }
                }
                Renderable::CompletionMenu(state) => {
                    if layer_needs_redraw {
                        self.compositor.clear_layer(*priority);
                        let layer = self.compositor.get_layer_mut(*priority);
                        render_completion_menu(layer, state);
                    }
                }
            }
        }

        if ctx.current_mode != Mode::Command
            && ctx.current_mode != Mode::Search
            && ctx.current_mode != Mode::Rename
            && self.last_command_cursor.is_some()
        {
            self.compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
            self.last_command_cursor = None;
        }

        command_cursor_info
    }

    fn draw_command_window(
        compositor: &mut LayerCompositor,
        ctx: &DrawContext,
        priority: LayerPriority,
        state: &CommandDrawState,
    ) -> CursorPosition {
        compositor.clear_layer(priority);
        let layer = compositor.get_layer_mut(priority);
        let default_border_chars =
            crate::perf_clone!(ctx.state.settings.default_border_chars.clone());

        let (window_row, window_col, _, offset) = CommandLine::render_to_layer(
            layer,
            ctx.viewport,
            &state.content,
            state.cursor.col,
            crate::command_line::RenderOptions {
                default_border_chars,
                window_settings: &ctx.state.settings.command_line_window,
                fg: state.editor_fg,
                bg: state.editor_bg,
                prompt: if ctx.current_mode == Mode::Search {
                    '/'
                } else if ctx.current_mode == Mode::Rename {
                    '@'
                } else {
                    ':'
                },
            },
        );

        let cursor_char = state
            .content
            .get(..state.cursor.col.min(state.content.len()))
            .map_or(0, |s| s.chars().count());
        let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
            (window_row, window_col),
            cursor_char,
            offset,
            state.has_border,
        );
        CursorPosition::Absolute(cursor_row, cursor_col)
    }

    fn fallback_cursor(
        ctx: &DrawContext,
        terminal_cursor: Option<(usize, usize)>,
        cursor_viewport: Option<&Viewport>,
        cursor_row_offset: usize,
        cursor_col_offset: usize,
    ) -> CursorPosition {
        if let Some((term_row, term_col)) = terminal_cursor {
            let gutter_width = ctx.gutter_cols();
            let pane_vp = cursor_viewport.unwrap_or(ctx.viewport);
            let max_content_row = pane_vp.visible_rows().saturating_sub(2);
            let clamped_row = term_row.min(max_content_row);
            let max_content_col = pane_vp.visible_cols().saturating_sub(gutter_width + 1);
            let clamped_col = term_col.min(max_content_col);
            CursorPosition::Absolute(
                to_u16_saturating(clamped_row + cursor_row_offset),
                to_u16_saturating(clamped_col + cursor_col_offset + gutter_width),
            )
        } else {
            let vp = cursor_viewport.unwrap_or(ctx.viewport);
            let gutter_width = ctx.gutter_cols();

            ctx.display_map.map_or_else(
                || {
                    let cursor_line = ctx.buf.get_line();
                    let cursor_line_in_viewport = if cursor_line >= vp.top_line()
                        && cursor_line < vp.top_line() + vp.visible_rows()
                    {
                        cursor_line - vp.top_line()
                    } else {
                        0
                    };

                    let line_start = ctx.buf.line_index.get_start(cursor_line).unwrap_or(0);
                    let cursor_off = ctx.buf.cursor();
                    let leading: usize = ctx
                        .annotation_inline
                        .unwrap_or(&[])
                        .iter()
                        .filter(|(s, _e, _t, _c, lead)| {
                            *lead && *s >= line_start && *s <= cursor_off
                        })
                        .map(|(_s, _e, text, _c, _l)| text.chars().count())
                        .sum();
                    let cursor_col =
                        calculate_cursor_column_at(ctx.buf, cursor_line, ctx.tab_width, cursor_off)
                            + leading;
                    let visual_cursor_col = cursor_col.saturating_sub(vp.left_col());
                    let display_col =
                        (visual_cursor_col + gutter_width).min(vp.visible_cols().saturating_sub(1));

                    CursorPosition::Absolute(
                        to_u16_saturating(cursor_line_in_viewport + cursor_row_offset),
                        to_u16_saturating(display_col + cursor_col_offset),
                    )
                },
                |dm| {
                    let cursor_visual_row = dm.char_to_visual_row(ctx.buf.cursor());
                    let top_visual = vp.top_visual_row();

                    let row_in_viewport = if cursor_visual_row >= top_visual {
                        (cursor_visual_row - top_visual).min(vp.visible_rows().saturating_sub(2))
                    } else {
                        0
                    };

                    let cursor_off = ctx.buf.cursor();
                    let cline = ctx.buf.line_index.get_line_at(cursor_off);
                    let line_start = ctx.buf.line_index.get_start(cline).unwrap_or(0);
                    let leading: usize = ctx
                        .annotation_inline
                        .unwrap_or(&[])
                        .iter()
                        .filter(|(s, _e, _t, _c, lead)| {
                            *lead && *s >= line_start && *s <= cursor_off
                        })
                        .map(|(_s, _e, text, _c, _l)| text.chars().count())
                        .sum();
                    let cursor_visual_col = dm.char_to_visual_col(cursor_off, ctx.buf) + leading;

                    let display_col =
                        (cursor_visual_col + gutter_width).min(vp.visible_cols().saturating_sub(1));

                    CursorPosition::Absolute(
                        to_u16_saturating(row_in_viewport + cursor_row_offset),
                        to_u16_saturating(display_col + cursor_col_offset),
                    )
                },
            )
        }
    }

    fn draw_soft_cursor(&mut self, ctx: &DrawContext, anim_row: usize, anim_col: usize) {
        let layer = self.compositor.get_layer_mut(LayerPriority::CURSOR);
        layer.clear_cell(anim_row, anim_col);
        if let Some((pr, pc)) = self.last_soft_cursor {
            if (pr, pc) != (anim_row, anim_col) {
                layer.clear_cell(pr, pc);
            }
        }

        let underlying = self.compositor.get_composited_cell(anim_row, anim_col);
        let cursor_cell = crate::cursor::SoftCursor::block_cell(
            underlying.as_ref(),
            ctx.state.settings.cursor_color,
            ctx.state.settings.editor_fg,
            ctx.state.settings.editor_bg,
        );

        self.compositor
            .get_layer_mut(LayerPriority::CURSOR)
            .set_cell(anim_row, anim_col, cursor_cell);
        self.last_soft_cursor = Some((anim_row, anim_col));
    }

    /// # Errors
    /// Returns `Err` when rendering fails.
    pub fn force_full_redraw<T: TerminalBackend>(
        &mut self,
        term: &mut T,
        mut state: RenderState,
    ) -> Result<CursorPosition, RiftError> {
        state.needs_clear = true;
        self.render(term, &state)
    }
}

fn render_completion_menu(
    layer: &mut crate::layer::Layer,
    state: &crate::render::CompletionMenuDrawState,
) {
    use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
    use crate::layer::Cell;

    let rows = layer.rows();
    let cols = layer.cols();

    let fg = state.editor_fg;
    let bg = state.editor_bg;
    let sel_text = bg.or(Some(crate::color::Color::Black));
    let sel_back = fg.or(Some(crate::color::Color::White));
    let desc_fg = Some(crate::color::Color::Grey);

    // Ratio-scaled width of a terminal dimension; small and non-negative.
    let cmd_width = f64_to_usize(usize_to_f64(state.terminal_cols) * state.cmd_width_ratio)
        .max(state.cmd_min_width)
        .min(state.terminal_cols);

    let cmd_col = to_u16_saturating(cols.saturating_sub(cmd_width) / 2);
    let cmd_total_height = state.cmd_height;
    let cmd_row = rows.saturating_sub(cmd_total_height) / 2;
    let menu_start_row = to_u16_saturating(cmd_row + cmd_total_height);

    let max_visible = 8usize.min(state.candidates.len());
    if max_visible == 0 {
        return;
    }

    let scroll = state.scroll_offset;
    let visible: Vec<_> = state
        .candidates
        .iter()
        .enumerate()
        .skip(scroll)
        .take(max_visible)
        .collect();

    let content_width = cmd_width.saturating_sub(2);
    let right_col_width = content_width / 2;
    let left_col_width = content_width
        .saturating_sub(right_col_width)
        .saturating_sub(2);

    let mut cell_rows: Vec<Vec<Cell>> = Vec::with_capacity(visible.len());

    for &(abs_i, (ref text, ref desc)) in &visible {
        let selected = state.selected == Some(abs_i);
        let (row_text, row_back) = if selected {
            (sel_text, sel_back)
        } else {
            (fg, bg)
        };

        let mut line: Vec<Cell> = Vec::with_capacity(content_width);

        let indicator = if selected { '▶' } else { ' ' };
        line.push(Cell::from_char(indicator).with_colors(row_text, row_back));
        line.push(Cell::from_char(' ').with_colors(row_text, row_back));

        for ch in text.chars().take(left_col_width) {
            line.push(Cell::from_char(ch).with_colors(row_text, row_back));
        }

        while line.len() < 2 + left_col_width {
            line.push(Cell::from_char(' ').with_colors(row_text, row_back));
        }

        for ch in desc.chars().take(right_col_width.saturating_sub(1)) {
            let d_fg = if selected { row_text } else { desc_fg };
            line.push(Cell::from_char(ch).with_colors(d_fg, row_back));
        }

        while line.len() < content_width {
            line.push(Cell::from_char(' ').with_colors(row_text, row_back));
        }

        cell_rows.push(line);
    }

    let total = state.candidates.len();
    if total > max_visible {
        let from = scroll + 1;
        let to = (scroll + max_visible).min(total);
        let footer_text = format!(" {from}-{to} of {total} ");
        let mut footer_line: Vec<Cell> = footer_text
            .chars()
            .take(content_width)
            .map(|ch| Cell::from_char(ch).with_colors(desc_fg, bg))
            .collect();
        while footer_line.len() < content_width {
            footer_line.push(Cell::from_char(' ').with_colors(fg, bg));
        }
        cell_rows.push(footer_line);
    }

    let menu_height = cell_rows.len();

    let mut style = WindowStyle::new()
        .with_border(true)
        .with_reverse_video(false);
    if let Some(c) = fg {
        style = style.with_fg(c);
    }
    if let Some(c) = bg {
        style = style.with_bg(c);
    }

    let window = FloatingWindow::with_style(
        WindowPosition::Absolute {
            row: menu_start_row,
            col: cmd_col,
        },
        content_width + 2,
        menu_height + 2,
        style,
    );

    window.render_cells(layer, &cell_rows);
}

#[cfg(test)]
#[path = "match_index_tests.rs"]
mod match_index_tests;
