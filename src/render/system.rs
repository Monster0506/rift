use crate::error::RiftError;
use crate::layer::{LayerCompositor, LayerPriority};
use crate::render::components::{Rect, Renderable};
use crate::render::ecs::World;
use crate::render::{
    calculate_cursor_column, CommandDrawState, ContentDrawState, CursorPosition, DrawContext,
    NotificationDrawState, RenderCache, RenderState, StatusDrawState,
};
use crate::term::TerminalBackend;
use crate::viewport::Viewport;

use crate::command_line::CommandLine;
use crate::mode::Mode;
use crate::status::StatusBar;

/// Persistent rendering system that holds all render-related state
pub struct RenderSystem {
    pub compositor: LayerCompositor,
    pub viewport: Viewport,
    pub render_cache: RenderCache,
    pub world: World,
}

impl RenderSystem {
    /// Create a new render system with specified dimensions
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            compositor: LayerCompositor::new(rows, cols),
            viewport: Viewport::new(rows, cols),
            render_cache: RenderCache::default(),
            world: World::new(),
        }
    }

    /// Resize the render system
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.viewport.set_size(rows, cols);
        self.compositor.resize(rows, cols);
        self.render_cache.invalidate_all();
        // World is ephemeral and cleared every frame, so no need to clear here explicitly,
        // but for safety we can.
        self.world.clear();
    }

    /// Rebuild the ECS world from the current DrawContext
    // Static helper to avoid self-borrow issues
    fn sync_world_static(world: &mut World, ctx: &DrawContext) {
        world.clear();

        let content_state = ContentDrawState {
            revision: ctx.buf.revision,
            top_line: ctx.viewport.top_line(),
            left_col: ctx.viewport.left_col(),
            rows: ctx.viewport.visible_rows(),
            tab_width: ctx.tab_width,
            show_line_numbers: ctx.state.settings.show_line_numbers,
            gutter_width: if ctx.state.settings.show_line_numbers {
                ctx.state.gutter_width
            } else {
                0
            },
            highlights_hash: ctx
                .state
                .settings
                .syntax_colors
                .as_ref()
                .map(|_| {
                    // Calculate hash based on visible highlights
                    // This is a simple approximation. Ideally we'd have a generation ID from Syntax.
                    // For now, let's use the first and last highlight index/capture if available,
                    // or just rely on the caller invalidating if syntax changed.
                    // Actually, we added highlights_hash to ContentDrawState but we need to compute it.
                    // Let's assume the caller will pass it or we'll compute it from ctx.highlights.
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    if let Some(h) = ctx.highlights {
                        h.len().hash(&mut hasher);
                        if !h.is_empty() {
                            h[0].hash(&mut hasher);
                            h[h.len() - 1].hash(&mut hasher);
                        }
                    }
                    hasher.finish()
                })
                .unwrap_or(0),
            search_matches_count: ctx.state.search_matches.len(),
            editor_bg: ctx.state.settings.editor_bg,
            editor_fg: ctx.state.settings.editor_fg,
            theme: ctx.state.settings.theme.clone(),
        };

        let content_entity = world.create_entity();
        world.add_renderable(content_entity, Renderable::TextBuffer(content_state));
        world.add_layer(content_entity, LayerPriority::CONTENT);
        world.add_rect(
            content_entity,
            Rect::new(
                0,
                0,
                ctx.viewport.visible_rows(),
                ctx.viewport.visible_cols(),
            ),
        );

        let (search_match_index, search_total_matches) = if !ctx.state.search_matches.is_empty() {
            let cursor_offset = ctx.buf.cursor();
            let idx = ctx
                .state
                .search_matches
                .iter()
                .position(|m| m.range.contains(&cursor_offset) || m.range.start == cursor_offset)
                .map(|i| i + 1);
            (idx, ctx.state.search_matches.len())
        } else {
            (None, 0)
        };

        let status_state = StatusDrawState {
            mode: ctx.current_mode,
            pending_key: ctx.pending_key,
            pending_count: ctx.pending_count,
            last_keypress: ctx.state.last_keypress,
            file_name: ctx.state.file_name.clone(),
            is_dirty: ctx.state.is_dirty,
            cursor: crate::render::CursorInfo {
                row: ctx.state.cursor_pos.0,
                col: ctx.state.cursor_pos.1,
            },
            total_lines: ctx.state.total_lines,
            debug_mode: ctx.state.debug_mode,
            cols: ctx.viewport.visible_cols(),
            search_query: ctx.state.last_search_query.clone(),
            search_match_index,
            search_total_matches,
            reverse_video: ctx.state.settings.status_line.reverse_video,
            editor_bg: ctx.state.settings.editor_bg,
            editor_fg: ctx.state.settings.editor_fg,
        };

        let status_entity = world.create_entity();
        world.add_renderable(status_entity, Renderable::StatusBar(status_state));
        world.add_layer(status_entity, LayerPriority::STATUS_BAR);

        if ctx.current_mode == Mode::Command || ctx.current_mode == Mode::Search {
            let command_state = CommandDrawState {
                content: ctx.state.command_line.clone(),
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

            let command_entity = world.create_entity();
            world.add_renderable(command_entity, Renderable::Window(command_state));
            world.add_layer(command_entity, LayerPriority::FLOATING_WINDOW);
        }

        let notification_state = NotificationDrawState {
            generation: ctx.state.error_manager.notifications().generation,
            count: ctx
                .state
                .error_manager
                .notifications()
                .iter_active()
                .count(),
        };

        let notification_entity = world.create_entity();
        world.add_renderable(
            notification_entity,
            Renderable::Notification(notification_state),
        );
        world.add_layer(notification_entity, LayerPriority::NOTIFICATION);

        if let Some(ref modal) = ctx.modal {
            let modal_entity = world.create_entity();
            world.add_renderable(modal_entity, Renderable::RefToModal);
            world.add_layer(modal_entity, modal.layer);
        }
    }

    /// Render to terminal using ECS
    pub fn render<T: TerminalBackend>(
        &mut self,
        term: &mut T,
        state: RenderState,
    ) -> Result<CursorPosition, RiftError> {
        if self.compositor.rows() != self.viewport.visible_rows()
            || self.compositor.cols() != self.viewport.visible_cols()
        {
            self.compositor
                .resize(self.viewport.visible_rows(), self.viewport.visible_cols());
        }

        let mut ctx = DrawContext {
            buf: state.buf,
            viewport: &self.viewport,
            current_mode: state.current_mode,
            pending_key: state.pending_key,
            pending_count: state.pending_count,
            state: state.state,
            needs_clear: state.needs_clear,
            tab_width: state.tab_width,
            highlights: state.highlights,
            capture_map: state.capture_map,
            modal: state.modal,
        };

        Self::sync_world_static(&mut self.world, &ctx);

        if ctx.needs_clear {
            self.render_cache.invalidate_all();
        }

        let mut render_entities: Vec<_> = self
            .world
            .entities()
            .iter()
            .filter_map(|&e| {
                if let (Some(renderable), Some(layer)) =
                    (self.world.renderables.get(e), self.world.layers.get(e))
                {
                    Some((e, renderable, layer))
                } else {
                    None
                }
            })
            .collect();

        render_entities.sort_by(|a, b| a.2.cmp(b.2));

        let mut command_cursor_info = None;

        for (_entity, renderable, priority) in render_entities {
            match renderable {
                Renderable::TextBuffer(state) => {
                    if self.render_cache.content.as_ref() != Some(state) {
                        // Note: We intentionally skip clear_layer here because
                        // render_content_to_layer writes all cells (including spaces),
                        // so clearing first causes double-buffer to see all cells as changed,
                        // resulting in full-screen terminal writes and flickering.
                        crate::render::render_content_to_layer(
                            self.compositor.get_layer_mut(*priority),
                            &ctx,
                        )
                        .map_err(|e| {
                            RiftError::new(crate::error::ErrorType::Renderer, "RENDER_FAILED", e)
                        })?;
                        self.render_cache.content = Some(state.clone());
                    }
                }
                Renderable::StatusBar(state) => {
                    if self.render_cache.status.as_ref() != Some(state) {
                        self.compositor.clear_layer(*priority);
                        StatusBar::render_to_layer(self.compositor.get_layer_mut(*priority), state);
                        self.render_cache.status = Some(state.clone());
                    }
                }
                Renderable::Window(state) => {
                    if self.render_cache.command_line.as_ref() != Some(state) {
                        self.compositor.clear_layer(*priority);
                        let layer = self.compositor.get_layer_mut(*priority);
                        let default_border_chars = ctx.state.settings.default_border_chars.clone();

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
                                } else {
                                    ':'
                                },
                            },
                        );

                        let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
                            (window_row, window_col),
                            state.cursor.col,
                            offset,
                            state.has_border,
                        );
                        let pos = CursorPosition::Absolute(cursor_row, cursor_col);
                        command_cursor_info = Some(pos);
                        self.render_cache.last_command_cursor = Some(pos);
                        self.render_cache.command_line = Some(state.clone());
                    } else {
                        command_cursor_info = self.render_cache.last_command_cursor;
                    }
                }
                Renderable::Notification(state) => {
                    if self.render_cache.notifications.as_ref() != Some(state) {
                        self.compositor.clear_layer(*priority);
                        crate::render::render_notifications(
                            self.compositor.get_layer_mut(*priority),
                            ctx.state,
                            ctx.viewport.visible_rows(),
                            ctx.viewport.visible_cols(),
                        );
                        self.render_cache.notifications = Some(state.clone());
                    }
                }
                Renderable::RefToModal => {
                    if let Some(ref mut modal) = ctx.modal {
                        let layer = self.compositor.get_layer_mut(*priority);
                        layer.clear();
                        modal.component.render(layer);
                    }
                }
            }
        }

        if ctx.current_mode != Mode::Command && ctx.current_mode != Mode::Search {
            if self.render_cache.command_line.is_some() {
                self.compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
                self.render_cache.command_line = None;
                self.render_cache.last_command_cursor = None;
            }
        }

        let cursor_info = if let Some(pos) = command_cursor_info {
            pos
        } else {
            let cursor_line = ctx.buf.get_line();
            let cursor_line_in_viewport = if cursor_line >= ctx.viewport.top_line()
                && cursor_line < ctx.viewport.top_line() + ctx.viewport.visible_rows()
            {
                cursor_line - ctx.viewport.top_line()
            } else {
                0
            };

            let gutter_width = if ctx.state.settings.show_line_numbers {
                ctx.state.gutter_width
            } else {
                0
            };

            let cursor_col = calculate_cursor_column(ctx.buf, cursor_line, ctx.tab_width);
            let visual_cursor_col = cursor_col.saturating_sub(ctx.viewport.left_col());
            let display_col = (visual_cursor_col + gutter_width)
                .min(ctx.viewport.visible_cols().saturating_sub(1));

            CursorPosition::Absolute(cursor_line_in_viewport as u16, display_col as u16)
        };

        let stats = self
            .compositor
            .render_to_terminal(term, ctx.needs_clear)
            .map_err(|e| RiftError::new(crate::error::ErrorType::Renderer, "RENDER_FAILED", e))?;

        if stats.changed_cells > 0 || self.render_cache.last_cursor_pos != Some(cursor_info) {
            match cursor_info {
                CursorPosition::Absolute(row, col) => {
                    term.move_cursor(row, col)?;
                }
            }
            self.render_cache.last_cursor_pos = Some(cursor_info);
        }
        term.show_cursor()?;

        Ok(cursor_info)
    }

    /// Force full redraw
    pub fn force_full_redraw<T: TerminalBackend>(
        &mut self,
        term: &mut T,
        mut state: RenderState,
    ) -> Result<CursorPosition, RiftError> {
        state.needs_clear = true;
        self.render(term, state)
    }
}
