use crate::error::RiftError;
use crate::layer::{LayerCompositor, LayerPriority};
use crate::render::components::{Rect, Renderable};
use crate::render::ecs::World;
use crate::render::{
    calculate_cursor_column, CommandDrawState, ContentDrawState, CursorPosition, DrawContext,
    NotificationDrawState, RenderState, StatusDrawState,
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
    pub world: World,

    // Persistent Entities
    content_entity: Option<crate::render::ecs::EntityId>,
    status_entity: Option<crate::render::ecs::EntityId>,
    command_entity: Option<crate::render::ecs::EntityId>,
    notification_entity: Option<crate::render::ecs::EntityId>,
    completion_entity: Option<crate::render::ecs::EntityId>,

    // Rendering State
    last_render_version: u64,
    last_cursor_pos: Option<CursorPosition>,
    last_command_cursor: Option<CursorPosition>,
}

impl RenderSystem {
    /// Create a new render system with specified dimensions
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
        }
    }

    /// Resize the render system
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.viewport.set_size(rows, cols);
        self.compositor.resize(rows, cols);

        // Force full refresh
        self.world.clear();
        self.content_entity = None;
        self.status_entity = None;
        self.command_entity = None;
        self.notification_entity = None;
        self.completion_entity = None;
        self.last_render_version = 0;
        self.last_cursor_pos = None;
        self.last_command_cursor = None;
    }

    /// Rebuild the ECS world from the current DrawContext
    // Static helper to avoid self-borrow issues
    /// Update the ECS world components based on current context
    /// Update the ECS world components based on current context
    fn update_world(&mut self, ctx: &DrawContext) {
        self.world.tick();

        // 1. Content
        if self.content_entity.is_none() {
            self.content_entity = Some(self.world.create_entity());
        }
        let content_entity = self.content_entity.unwrap();

        let effective_line_numbers = ctx.show_line_numbers && ctx.state.settings.show_line_numbers;
        let content_state = ContentDrawState {
            revision: ctx.buf.revision,
            top_line: ctx.viewport.top_line(),
            left_col: ctx.viewport.left_col(),
            rows: ctx.viewport.visible_rows(),
            tab_width: ctx.tab_width,
            show_line_numbers: effective_line_numbers,
            gutter_width: if effective_line_numbers {
                ctx.state.gutter_width
            } else {
                0
            },
            search_matches_count: ctx.state.search_matches.len(),
            plugin_highlights_len: ctx.plugin_highlights.map(|h| h.len()).unwrap_or(0),
            editor_bg: ctx.state.settings.editor_bg,
            editor_fg: ctx.state.settings.editor_fg,
            theme: ctx.state.settings.theme.clone(),
            highlights_hash: ctx
                .state
                .settings
                .syntax_colors
                .as_ref()
                .map(|_| {
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
        };

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

        // 2. Status Bar
        if self.status_entity.is_none() {
            self.status_entity = Some(self.world.create_entity());
        }
        let status_entity = self.status_entity.unwrap();

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

        self.world
            .add_renderable(status_entity, Renderable::StatusBar(status_state));
        self.world
            .add_layer(status_entity, LayerPriority::STATUS_BAR);

        // 3. Command Line Window
        if ctx.current_mode == Mode::Command || ctx.current_mode == Mode::Search {
            if self.command_entity.is_none() {
                self.command_entity = Some(self.world.create_entity());
            }
            let command_entity = self.command_entity.unwrap();

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

            self.world
                .add_renderable(command_entity, Renderable::Window(command_state));
            self.world
                .add_layer(command_entity, LayerPriority::FLOATING_WINDOW);
        } else if let Some(entity) = self.command_entity {
            // Mode changed, destroy entity
            self.world.destroy_entity(entity);
            self.command_entity = None;
            self.last_command_cursor = None;
            // Also need to clear the layer
            self.compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
        }

        // 3b. Completion dropdown
        let show_dropdown = ctx.current_mode == Mode::Command
            && ctx
                .state
                .completion_session
                .as_ref()
                .map(|s| s.dropdown_open && !s.candidates.is_empty())
                .unwrap_or(false);

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
                    .map(|c| (c.text.clone(), c.description.clone()))
                    .collect(),
                selected: session.selected,
                terminal_cols: ctx.viewport.visible_cols(),
                cmd_width_ratio: ctx.state.settings.command_line_window.width_ratio,
                cmd_min_width: ctx.state.settings.command_line_window.min_width,
                cmd_has_border: ctx.state.settings.command_line_window.border,
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

        // 4. Notifications
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

        // Clone viewport to avoid simultaneous borrow of self in update_world
        let viewport = self.viewport.clone();
        let skip_content = state.skip_content;
        let cursor_row_offset = state.cursor_row_offset;
        let cursor_col_offset = state.cursor_col_offset;
        let cursor_viewport = state.cursor_viewport;
        let ctx = DrawContext {
            buf: state.buf,
            viewport: &viewport,
            current_mode: state.current_mode,
            pending_key: state.pending_key,
            pending_count: state.pending_count,
            state: state.state,
            needs_clear: state.needs_clear,
            tab_width: state.tab_width,
            highlights: state.highlights,
            capture_map: state.capture_map,
            custom_highlights: state.custom_highlights,
            plugin_highlights: state.plugin_highlights,
            show_line_numbers: state.show_line_numbers,
            display_map: state.display_map,
            gutter_width_override: None,
        };

        // Update the ECS world
        self.update_world(&ctx);

        if ctx.needs_clear {
            self.last_render_version = 0;
        }

        // Gather entities to render
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
            let version = self.world.renderables.get_version(_entity).unwrap_or(0);
            let layer_needs_redraw =
                version > self.last_render_version || self.last_render_version == 0;

            match renderable {
                Renderable::TextBuffer(_) => {
                    if layer_needs_redraw && !skip_content {
                        crate::render::render_content_to_layer(
                            self.compositor.get_layer_mut(*priority),
                            &ctx,
                        )
                        .map_err(|e| {
                            RiftError::new(crate::error::ErrorType::Renderer, "RENDER_FAILED", e)
                        })?;
                    }
                }
                Renderable::StatusBar(state) => {
                    if layer_needs_redraw {
                        self.compositor.clear_layer(*priority);
                        StatusBar::render_to_layer(self.compositor.get_layer_mut(*priority), state);
                    }
                }
                Renderable::Window(state) => {
                    if layer_needs_redraw {
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
            && self.last_command_cursor.is_some()
        {
            self.compositor.clear_layer(LayerPriority::FLOATING_WINDOW);
            self.last_command_cursor = None;
        }

        let cursor_info = if let Some(pos) = command_cursor_info {
            pos
        } else if let Some((term_row, term_col)) = state.terminal_cursor {
            let gutter_width = if ctx.show_line_numbers && ctx.state.settings.show_line_numbers {
                ctx.state.gutter_width
            } else {
                0
            };
            let max_content_row = viewport.visible_rows().saturating_sub(2);
            let clamped_row = term_row.min(max_content_row);
            CursorPosition::Absolute(
                (clamped_row + cursor_row_offset) as u16,
                (term_col + cursor_col_offset + gutter_width) as u16,
            )
        } else {
            let vp = cursor_viewport.unwrap_or(&viewport);
            let gutter_width = if ctx.show_line_numbers && ctx.state.settings.show_line_numbers {
                ctx.state.gutter_width
            } else {
                0
            };

            if let Some(dm) = ctx.display_map {
                let cursor_visual_row = dm.char_to_visual_row(ctx.buf.cursor());
                let cursor_visual_col = dm.char_to_visual_col(ctx.buf.cursor(), ctx.buf);
                let top_visual = vp.top_visual_row();

                let row_in_viewport = if cursor_visual_row >= top_visual {
                    (cursor_visual_row - top_visual).min(vp.visible_rows().saturating_sub(2))
                } else {
                    0
                };

                let display_col =
                    (cursor_visual_col + gutter_width).min(vp.visible_cols().saturating_sub(1));

                CursorPosition::Absolute(
                    (row_in_viewport + cursor_row_offset) as u16,
                    (display_col + cursor_col_offset) as u16,
                )
            } else {
                let cursor_line = ctx.buf.get_line();
                let cursor_line_in_viewport = if cursor_line >= vp.top_line()
                    && cursor_line < vp.top_line() + vp.visible_rows()
                {
                    cursor_line - vp.top_line()
                } else {
                    0
                };

                let cursor_col = calculate_cursor_column(ctx.buf, cursor_line, ctx.tab_width);
                let visual_cursor_col = cursor_col.saturating_sub(vp.left_col());
                let display_col =
                    (visual_cursor_col + gutter_width).min(vp.visible_cols().saturating_sub(1));

                CursorPosition::Absolute(
                    (cursor_line_in_viewport + cursor_row_offset) as u16,
                    (display_col + cursor_col_offset) as u16,
                )
            }
        };

        let stats = self
            .compositor
            .render_to_terminal(term, ctx.needs_clear)
            .map_err(|e| RiftError::new(crate::error::ErrorType::Renderer, "RENDER_FAILED", e))?;

        if stats.changed_cells > 0 || self.last_cursor_pos != Some(cursor_info) {
            match cursor_info {
                CursorPosition::Absolute(row, col) => {
                    term.move_cursor(row, col)?;
                }
            }
            self.last_cursor_pos = Some(cursor_info);
        }
        term.show_cursor()?;
        term.flush()?;

        // Update version reference
        self.last_render_version = self.world.current_version;

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

/// Render the completion dropdown menu onto a layer using FloatingWindow.
///
/// The menu is positioned directly below the command line window, matching its
/// horizontal position and width.
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
    let sel_fg = bg.or(Some(crate::color::Color::Black));
    let sel_bg = fg.or(Some(crate::color::Color::White));
    let desc_fg = Some(crate::color::Color::Grey);

    let cmd_width = ((state.terminal_cols as f64 * state.cmd_width_ratio) as usize)
        .max(state.cmd_min_width)
        .min(state.terminal_cols);

    let cmd_col = (cols.saturating_sub(cmd_width) / 2) as u16;
    let cmd_total_height = state.cmd_height;
    let cmd_row = rows.saturating_sub(cmd_total_height) / 2;
    let menu_start_row = (cmd_row + cmd_total_height) as u16;

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
        let (row_fg, row_bg) = if selected { (sel_fg, sel_bg) } else { (fg, bg) };

        let mut line: Vec<Cell> = Vec::with_capacity(content_width);

        let indicator = if selected { '▶' } else { ' ' };
        line.push(Cell::from_char(indicator).with_colors(row_fg, row_bg));
        line.push(Cell::from_char(' ').with_colors(row_fg, row_bg));

        for ch in text.chars().take(left_col_width) {
            line.push(Cell::from_char(ch).with_colors(row_fg, row_bg));
        }

        while line.len() < 2 + left_col_width {
            line.push(Cell::from_char(' ').with_colors(row_fg, row_bg));
        }

        for ch in desc.chars().take(right_col_width.saturating_sub(1)) {
            let d_fg = if selected { row_fg } else { desc_fg };
            line.push(Cell::from_char(ch).with_colors(d_fg, row_bg));
        }

        while line.len() < content_width {
            line.push(Cell::from_char(' ').with_colors(row_fg, row_bg));
        }

        cell_rows.push(line);
    }

    let total = state.candidates.len();
    if total > max_visible {
        let from = scroll + 1;
        let to = (scroll + max_visible).min(total);
        let footer_text = format!(" {}-{} of {} ", from, to, total);
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
