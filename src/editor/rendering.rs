use super::git_gutter;
use super::Editor;
use crate::error::{ErrorType, RiftError};
use crate::render;
#[cfg(test)]
use crate::screen_buffer::FrameStats;
use crate::split::layout::WindowLayout;
use crate::term::TerminalBackend;

type FrameState = (
    crate::document::DocumentId,
    bool,
    Option<std::sync::Arc<crate::wrap::DisplayMap>>,
    Option<(usize, usize, isize)>,
);

#[derive(Clone, Copy)]
struct WindowPass {
    needs_clear: bool,
    window_set_changed: bool,
    focused_doc_id: crate::document::DocumentId,
}

const MAX_UNRENDERED_KEYS: usize = 32;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn update_state_and_render(
        &mut self,
        keypress: crate::key::Key,
        action: crate::key_handler::KeyAction,
        command: crate::command::Command,
    ) -> Result<(), RiftError> {
        self.handle_key_actions(action);
        self.handle_mode_management(command);

        self.state.update_keypress(keypress);
        self.state.update_command(command);

        self.unrendered_key_count += 1;
        let more_input_queued = self
            .term
            .poll(std::time::Duration::from_millis(0))
            .unwrap_or(false);
        if more_input_queued && self.unrendered_key_count < MAX_UNRENDERED_KEYS {
            return Ok(());
        }
        self.unrendered_key_count = 0;

        self.update_and_render()
    }

    pub(super) fn flush_coalesced_render_if_idle(&mut self) -> Result<(), RiftError> {
        if self.unrendered_key_count == 0 {
            return Ok(());
        }
        let more_input_queued = self
            .term
            .poll(std::time::Duration::from_millis(0))
            .unwrap_or(false);
        if more_input_queued {
            return Ok(());
        }
        self.unrendered_key_count = 0;
        self.update_and_render()
    }

    fn update_state(&mut self) -> Option<FrameState> {
        crate::perf_span!("update_state", crate::perf::PerfFields::default());
        self.flush_pending_text_changed();
        self.flush_pending_cursor_moved();
        self.update_annotation_hover();
        if self.split_tree.window_count() == 1 {
            let rows = self.render_system.viewport.visible_rows();
            let cols = self.render_system.viewport.visible_cols();
            self.split_tree
                .focused_window_mut()
                .viewport
                .set_size(rows, cols);
        }

        let doc_id = self.split_tree.focused_window().document_id;
        if let Some(doc) = self.document_manager.get_document(doc_id) {
            self.split_tree.focused_window_mut().cursor_position = doc.buffer.cursor();
        }

        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.recompute_directory_highlights();
        }

        let (cursor_line, cursor_col, total_lines, is_terminal) =
            if let Some(doc) = self.document_manager.get_document(doc_id) {
                let tw = doc.options.tab_width;
                let line = doc.buffer.get_line();
                let cursor = doc.buffer.cursor();
                let line_start = doc.buffer.line_index.get_start(line).unwrap_or(0);
                let col = render::calculate_cursor_column_at(&doc.buffer, line, tw, cursor)
                    + doc.annotations.leading_width_in(line_start, cursor + 1);
                let total = doc.buffer.get_total_lines();
                (line, col, total, doc.is_terminal())
            } else {
                return None;
            };
        self.state.update_cursor(cursor_line, cursor_col);

        self.sync_state_with_active_document();
        self.state.error_manager.notifications_mut().prune_expired();
        let gutter_width = self.active_gutter_cols(doc_id);

        let cursor_char = self
            .document_manager
            .get_document(doc_id)
            .map_or(0, |d| d.buffer.cursor());
        let visible_rows = self.render_system.viewport.visible_rows();
        let display_map = {
            let content_width = self
                .render_system
                .viewport
                .visible_cols()
                .saturating_sub(gutter_width)
                .max(1);
            crate::perf_span!("display_map_resolve", crate::perf::PerfFields::default());
            self.resolve_display_map_cached(doc_id, content_width, cursor_char, visible_rows)
        };

        let needs_clear = if let Some(ref dm) = display_map {
            let visual_row = dm.char_to_visual_row(cursor_char);
            let total_visual = dm.total_visual_rows();
            self.render_system
                .viewport
                .update_visual(visual_row, 0, total_visual, gutter_width)
        } else {
            let viewport_col = if is_terminal { 0 } else { cursor_col };
            self.render_system
                .viewport
                .update(cursor_line, viewport_col, total_lines, gutter_width)
        };

        let scroll_hint = {
            let vp = &self.render_system.viewport;
            let content_bottom = vp.visible_rows().saturating_sub(2);
            let (delta, horizontal) = if display_map.is_some() {
                (
                    vp.top_visual_row().cast_signed() - vp.prev_top_visual_row().cast_signed(),
                    false,
                )
            } else {
                (
                    vp.top_line().cast_signed() - vp.prev_top_line().cast_signed(),
                    vp.left_col() != vp.prev_left_col(),
                )
            };
            if !needs_clear && !horizontal && delta != 0 && content_bottom >= 1 {
                Some((0usize, content_bottom, delta))
            } else {
                None
            }
        };

        Some((doc_id, needs_clear, display_map, scroll_hint))
    }

    pub(super) fn update_selection_highlights(&mut self) {
        crate::perf_span!(
            "update_selection_highlights",
            crate::perf::PerfFields::default()
        );
        let is_visual = self.current_mode.is_visual();
        let visual_anchor = self.visual_anchor;
        let visual_kind = self.current_mode.visual_range_kind();
        let Some(doc) = self.document_manager.active_document_mut() else {
            return;
        };
        let active = if is_visual {
            visual_anchor.zip(visual_kind).map(|(anchor, kind)| {
                crate::selection::Region::new(anchor, doc.buffer.cursor(), kind)
            })
        } else {
            None
        };
        let banked = doc.selection_set.sorted();
        if active.is_none()
            && banked.is_empty()
            && doc.annotations.query_kind("ui.selection").next().is_none()
        {
            return;
        }
        doc.sync_selection_annotations(active, &banked);
    }

    fn visible_document_ids(&self) -> Vec<crate::document::DocumentId> {
        let mut ids: Vec<crate::document::DocumentId> = self
            .split_tree
            .windows
            .values()
            .map(|w| w.document_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn begin_ghost_paint(&mut self) {
        for id in self.visible_document_ids() {
            let Some(doc) = self.document_manager.get_document_mut(id) else {
                continue;
            };
            let Some(outcome) = doc.begin_ghost_paint() else {
                continue;
            };
            self.apply_syntax_sync(id, outcome);
        }
    }

    fn end_ghost_paint(&mut self) {
        for id in self.visible_document_ids() {
            let Some(doc) = self.document_manager.get_document_mut(id) else {
                continue;
            };
            let Some(outcome) = doc.end_ghost_paint() else {
                continue;
            };
            self.apply_syntax_sync(id, outcome);
        }
    }

    /// # Errors
    /// Returns an error if the frame fails to render or the terminal cannot be written.
    pub(crate) fn update_and_render(&mut self) -> Result<(), RiftError> {
        self.begin_ghost_paint();
        let result = self.update_and_render_painted();
        self.end_ghost_paint();
        result
    }

    fn update_and_render_painted(&mut self) -> Result<(), RiftError> {
        #[cfg(feature = "perf_instrumentation")]
        let _frame_guard = crate::perf::begin_frame();

        let Some((_doc_id, needs_clear, display_map, scroll_hint)) = self.update_state() else {
            return Ok(());
        };

        self.update_selection_highlights();

        let multi_window = self.split_tree.window_count() > 1;
        if multi_window {
            self.update_window_viewports();
        }

        self.render_plugin_float();

        if self.post_paste_state.is_some() {
            self.render_clipboard_tooltip();
        } else {
            let cursor_row = self.cursor_screen_row();
            self.render_annotation_tooltip(cursor_row);
        }

        self.render_explorer_diff_tooltip();

        if multi_window {
            self.render_multi_window(needs_clear)
        } else {
            self.render(needs_clear, display_map.as_deref(), scroll_hint)
        }
    }

    pub(crate) fn cursor_screen_row(&mut self) -> usize {
        let focused_id = self.split_tree.focused_window_id();
        let doc_id = self.split_tree.focused_window().document_id;
        let (row_off, top_line, top_visual, content_width, rows) =
            if self.split_tree.window_count() > 1 {
                let Ok(size) = self.term.get_size() else {
                    return 0;
                };
                let layouts = self
                    .split_tree
                    .compute_layout((size.rows as usize).saturating_sub(1), size.cols as usize);
                let Some(layout) = layouts.iter().find(|l| l.window_id == focused_id) else {
                    return 0;
                };
                let Some((_, content_width)) = self.window_map_key(doc_id, layout.cols) else {
                    return 0;
                };
                let vp = &self.split_tree.focused_window().viewport;
                (
                    layout.row,
                    vp.top_line(),
                    vp.top_visual_row(),
                    content_width,
                    layout.rows + 1,
                )
            } else {
                let gutter_width = self.active_gutter_cols(doc_id);
                let vp = &self.render_system.viewport;
                (
                    0,
                    vp.top_line(),
                    vp.top_visual_row(),
                    vp.visible_cols().saturating_sub(gutter_width).max(1),
                    vp.visible_rows(),
                )
            };
        let Some(doc) = self.document_manager.get_document(doc_id) else {
            return 0;
        };
        let cursor = doc.buffer.cursor();
        let line = doc.buffer.line_index.get_line_at(cursor);
        let row = self
            .resolve_display_map_cached(doc_id, content_width, cursor, rows)
            .map_or_else(
                || line.saturating_sub(top_line),
                |dm| dm.char_to_visual_row(cursor).saturating_sub(top_visual),
            );
        row_off + row
    }

    pub(super) fn resolve_display_map_cached(
        &mut self,
        doc_id: crate::document::DocumentId,
        content_width: usize,
        cursor_char: usize,
        extend_margin_rows: usize,
    ) -> Option<std::sync::Arc<crate::wrap::DisplayMap>> {
        use std::sync::Arc;
        const CACHE_CAP: usize = 8;
        let soft_wrap = self.state.settings.soft_wrap;
        let wrap_width = self.state.settings.wrap_width;
        let include_lsp = self.state.settings.lsp.virtual_text;
        let doc = self.document_manager.get_document_mut(doc_id)?;
        let revision = doc.buffer.revision;
        let buf_len = doc.buffer.len();
        let annotations_revision = doc.annotations.revision();
        let params = super::resolve_wrap_params(doc, content_width, soft_wrap, wrap_width);
        let edits = doc.buffer.take_char_edits();

        let eol_rows = |doc: &crate::document::Document| {
            doc.annotations.trailing_adornment_lines(include_lsp, |b| {
                doc.buffer
                    .line_index
                    .get_line_at(doc.buffer.byte_to_char(b))
            })
        };

        let mut map: Option<Arc<crate::wrap::DisplayMap>> = None;
        let slot = self
            .display_map_cache
            .iter()
            .position(|e| e.doc_id == doc_id && e.content_width == content_width);
        if let Some(pos) = slot {
            let entry = self.display_map_cache.remove(pos);
            let valid = match (&params, &entry.map) {
                (None, None) => true,
                (Some((w, tw)), Some(m)) => m.wrap_width == *w && m.tab_width == *tw,
                _ => false,
            };
            if valid {
                if entry.revision == revision && entry.buf_len == buf_len {
                    let same_rows = (entry.annotations_revision == annotations_revision
                        && entry.lsp_virtual_text == include_lsp)
                        || entry
                            .map
                            .as_deref()
                            .is_none_or(|m| m.eol_rows() == eol_rows(doc));
                    if same_rows {
                        map = entry.map;
                    }
                } else if entry.revision.wrapping_add(edits.len() as u64) == revision {
                    if let Some(combined) = combine_char_edits(&edits) {
                        if let Some(mut m) = entry.map {
                            if Arc::make_mut(&mut m).apply_edit(
                                &doc.buffer,
                                combined.pos,
                                combined.del,
                                combined.ins,
                                eol_rows(doc),
                            ) {
                                map = Some(m);
                            }
                        }
                    }
                }
            }
        }

        let mut map = map.or_else(|| {
            params.map(|(w, tw)| {
                Arc::new(crate::wrap::DisplayMap::empty(w, tw).with_eol_rows(eol_rows(doc)))
            })
        });

        let already_covered = map
            .as_deref()
            .is_some_and(|m| !m.needs_extension(cursor_char, extend_margin_rows));
        if !already_covered {
            if let Some(m) = map.as_mut() {
                let mm = Arc::make_mut(m);
                mm.extend_to_char(&doc.buffer, cursor_char);
                let row = mm.char_to_visual_row(cursor_char);
                mm.extend_to_row(&doc.buffer, row + extend_margin_rows + 1);
            }
        }

        if self.display_map_cache.len() >= CACHE_CAP {
            self.display_map_cache.remove(0);
        }
        self.display_map_cache.push(super::DisplayMapCacheEntry {
            doc_id,
            revision,
            buf_len,
            content_width,
            annotations_revision,
            lsp_virtual_text: include_lsp,
            map: map.clone(),
        });
        map
    }

    fn window_map_key(
        &self,
        doc_id: crate::document::DocumentId,
        cols: usize,
    ) -> Option<(crate::document::DocumentId, usize)> {
        let doc = self.document_manager.get_document(doc_id)?;
        let gutter_width = Self::line_number_gutter_width(doc, &self.state);
        Some((doc_id, cols.saturating_sub(gutter_width).max(1)))
    }

    pub(super) fn render_clipboard_tooltip(&mut self) {
        crate::perf_span!(
            "render_clipboard_tooltip",
            crate::perf::PerfFields::default()
        );
        let selected = self.post_paste_state.as_ref().map_or(0, |s| s.ring_index);
        let foreground = self.state.settings.editor_fg;
        let background = self.state.settings.editor_bg;
        self.system_clipboard_cache.refresh_if_stale();
        let layer = self
            .render_system
            .compositor
            .get_layer_mut(crate::layer::LayerPriority::TOOLTIP);
        layer.clear();
        crate::clipboard::ClipboardTooltip::render(
            &self.clipboard_ring,
            selected,
            self.system_clipboard_cache.text(),
            layer,
            foreground,
            background,
        );
    }

    #[cfg(test)]
    /// # Errors
    /// Returns an error if the cursor or output cannot be written, or the compositor fails.
    pub(crate) fn render_to_terminal(
        &mut self,
        needs_clear: bool,
    ) -> Result<FrameStats, RiftError> {
        self.term.hide_cursor()?;
        let stats = self
            .render_system
            .compositor
            .render_to_terminal(&mut self.term, needs_clear, None)
            .map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::RENDER_FAILED,
                    e.to_string(),
                )
            })?;
        self.term.show_cursor()?;
        self.term.flush()?;
        Ok(stats)
    }

    pub(super) fn render(
        &mut self,
        needs_clear: bool,
        display_map: Option<&crate::wrap::DisplayMap>,
        scroll_hint: Option<(usize, usize, isize)>,
    ) -> Result<(), RiftError> {
        crate::perf_span!("render_single_window", crate::perf::PerfFields::default());
        let Self {
            document_manager,
            render_system,
            state,
            current_mode,
            term,
            pending_keys,
            pending_count,
            kind_registry,
            ..
        } = self;

        let Some(doc) = document_manager.active_document_mut() else {
            return Ok(());
        };
        doc.recompute_directory_highlights();

        let (start_logical, end_logical, start_byte, end_byte) =
            Self::visible_span(doc, &render_system.viewport, display_map);

        let highlights = doc
            .syntax
            .as_mut()
            .map(|syntax| syntax.highlights(Some(start_byte..end_byte)));

        let capture_names = doc
            .syntax
            .as_ref()
            .map(crate::syntax::Syntax::capture_names);
        let injection_hl = doc
            .syntax
            .as_ref()
            .map(|s| s.injection_highlights_named(Some(&(start_byte..end_byte))));

        let annotation_styles = doc.annotations.presentation_spans(
            state.settings.syntax_colors.as_ref(),
            Some(kind_registry),
            start_byte..end_byte,
        );
        let annotation_adornments = doc.annotations.line_adornments(
            state.settings.syntax_colors.as_ref(),
            Some(kind_registry),
            start_byte..end_byte,
            start_logical..end_logical,
            state.settings.lsp.virtual_text,
            |b| {
                doc.buffer
                    .line_index
                    .get_line_at(doc.buffer.byte_to_char(b))
            },
        );
        let annotation_inline = doc.annotations.inline_adornments(
            state.settings.syntax_colors.as_ref(),
            Some(kind_registry),
            start_byte..end_byte,
        );
        let annotation_concealed = Self::concealed_outside_cursor_line(doc, start_byte..end_byte);

        let syntax_generation = Self::syntax_generation(doc);
        let annotations_revision = doc.annotations.revision();
        let kind_registry_generation = kind_registry.generation();
        let git_gutter_colors_data = git_gutter::git_gutter_render_colors(doc);

        let state = render::RenderState {
            buf: &doc.buffer,
            state,
            current_mode: *current_mode,
            pending_key: pending_keys.last().copied(),
            pending_count: *pending_count,
            needs_clear,
            tab_width: doc.options.tab_width,
            highlights: highlights.as_deref(),
            capture_map: capture_names,
            injection_highlights: injection_hl.as_deref(),
            skip_content: false,
            cursor_row_offset: 0,
            cursor_col_offset: 0,
            cursor_viewport: None,
            terminal_cursor: doc.terminal_cursor(),
            custom_highlights: (!doc.custom_highlights.is_empty())
                .then_some(&doc.custom_highlights),
            git_gutter_colors: (!git_gutter_colors_data.is_empty())
                .then_some(&git_gutter_colors_data),
            annotation_styles: (!annotation_styles.is_empty()).then_some(&annotation_styles),
            annotation_adornments: (!annotation_adornments.is_empty())
                .then_some(&annotation_adornments),
            annotation_inline: (!annotation_inline.is_empty()).then_some(&annotation_inline),
            annotation_concealed: (!annotation_concealed.is_empty())
                .then_some(&annotation_concealed),
            terminal_cell_colors: doc
                .terminal_cell_colors()
                .filter(|colors| !colors.is_empty()),
            show_line_numbers: doc.options.show_line_numbers,
            display_map,
            scroll_hint,
            syntax_generation,
            annotations_revision,
            kind_registry_generation,
        };

        let _ = render_system.render(term, &state)?;

        Ok(())
    }

    fn visible_span(
        doc: &crate::document::Document,
        viewport: &crate::viewport::Viewport,
        display_map: Option<&crate::wrap::DisplayMap>,
    ) -> (usize, usize, usize, usize) {
        let (start_logical, end_logical) = display_map.map_or_else(
            || {
                let start = viewport.top_line();
                let end = start + viewport.visible_rows();
                (start, end)
            },
            |dm| {
                let top_vr = viewport.top_visual_row();
                let bottom_vr = top_vr + viewport.visible_rows();
                let start_l = dm.get_visual_row(top_vr).map_or(0, |r| r.logical_line);
                let end_l = dm
                    .get_visual_row(
                        bottom_vr
                            .saturating_sub(1)
                            .min(dm.total_visual_rows().saturating_sub(1)),
                    )
                    .map_or_else(|| doc.buffer.get_total_lines(), |r| r.logical_line + 1);
                (start_l, end_l)
            },
        );
        let start_char = doc.buffer.line_index.get_start(start_logical).unwrap_or(0);
        let end_char = if end_logical < doc.buffer.get_total_lines() {
            doc.buffer
                .line_index
                .get_start(end_logical)
                .unwrap_or(doc.buffer.len())
        } else {
            doc.buffer.len()
        };
        (
            start_logical,
            end_logical,
            doc.buffer.char_to_byte(start_char),
            doc.buffer.char_to_byte(end_char),
        )
    }

    pub(super) fn render_plugin_float(&mut self) {
        crate::perf_span!("render_plugin_float", crate::perf::PerfFields::default());
        if self.plugin_host.has_open_float() {
            let layer = self
                .render_system
                .compositor
                .get_layer_mut(crate::layer::LayerPriority::POPUP);
            layer.clear();
            let fg = self.state.settings.editor_fg;
            let bg = self.state.settings.editor_bg;
            self.plugin_host.render_float_into_layer(layer, fg, bg);
        } else if self.plugin_host.take_float_closed() {
            self.render_system
                .compositor
                .get_layer_mut(crate::layer::LayerPriority::POPUP)
                .clear();
        }
    }

    pub(super) fn update_window_viewports(&mut self) {
        crate::perf_span!(
            "update_window_viewports",
            crate::perf::PerfFields::default()
        );
        let soft_wrap = self.state.settings.soft_wrap;

        let Ok(size) = self.term.get_size() else {
            return;
        };
        let content_rows = (size.rows as usize).saturating_sub(1);
        let layouts = self
            .split_tree
            .compute_layout(content_rows, size.cols as usize);
        self.update_git_blame_wrap(&layouts, soft_wrap);
        self.sync_git_blame_cursor();

        for layout in &layouts {
            self.update_layout_viewport(layout, soft_wrap);
        }
        self.sync_partner_scroll(&layouts, soft_wrap);
    }

    fn update_git_blame_wrap(&mut self, layouts: &[WindowLayout], soft_wrap: bool) {
        let Some(partner_window_id) = self.git_blame_partner_window_id() else {
            return;
        };
        let focused_window_id = self.split_tree.focused_window_id();
        let focused_is_blame = self
            .document_manager
            .get_document(self.split_tree.focused_window().document_id)
            .is_some_and(super::super::document::Document::is_git_blame);
        let (blame_window_id, source_window_id) = if focused_is_blame {
            (focused_window_id, partner_window_id)
        } else {
            (partner_window_id, focused_window_id)
        };
        let source_cols = layouts
            .iter()
            .find(|layout| layout.window_id == source_window_id)
            .map(|layout| layout.cols);
        let source_doc_id = self
            .split_tree
            .get_window(source_window_id)
            .map(|window| window.document_id);
        let blame_doc_id = self
            .split_tree
            .get_window(blame_window_id)
            .map(|window| window.document_id);
        if let (Some(source_cols), Some(source_doc_id), Some(blame_doc_id)) =
            (source_cols, source_doc_id, blame_doc_id)
        {
            let blame_line_count = self
                .document_manager
                .get_document(blame_doc_id)
                .map_or(0, super::super::document::Document::git_blame_lines_len);
            let wrap_spec = self
                .document_manager
                .get_document(source_doc_id)
                .map(|source_doc| {
                    let gutter_width = Self::line_number_gutter_width(source_doc, &self.state);
                    let content_width = source_cols.saturating_sub(gutter_width).max(1);
                    let params = super::resolve_wrap_params(
                        source_doc,
                        content_width,
                        soft_wrap,
                        self.state.settings.wrap_width,
                    );
                    let (wrap_width, tab_width) =
                        params.unwrap_or((0, source_doc.options.tab_width));
                    (
                        (
                            source_doc_id,
                            wrap_width,
                            tab_width,
                            source_doc.buffer.revision,
                        ),
                        params,
                    )
                });
            let already_current = wrap_spec.as_ref().is_some_and(|(key, _)| {
                self.document_manager
                    .get_document(blame_doc_id)
                    .is_some_and(|doc| doc.git_blame_wrap_key() == Some(*key))
            });
            if let Some((wrap_key, params)) = wrap_spec.filter(|_| !already_current) {
                let wrap_rows = self
                    .document_manager
                    .get_document(source_doc_id)
                    .map(|source_doc| match params {
                        Some((wrap_width, tab_width)) => {
                            let map = crate::wrap::DisplayMap::build(
                                &source_doc.buffer,
                                wrap_width,
                                tab_width,
                            );
                            (0..blame_line_count)
                                .map(|line| {
                                    let first = map.logical_to_first_visual(line);
                                    let next = if line + 1 < source_doc.buffer.get_total_lines() {
                                        map.logical_to_first_visual(line + 1)
                                    } else {
                                        map.total_visual_rows()
                                    };
                                    next.saturating_sub(first).max(1)
                                })
                                .collect()
                        }
                        None => vec![1; blame_line_count],
                    })
                    .unwrap_or_default();
                if let Some(blame_doc) = self.document_manager.get_document_mut(blame_doc_id) {
                    blame_doc.set_git_blame_wrap_rows(wrap_key, wrap_rows);
                    if let Some(window) = self.split_tree.get_window_mut(blame_window_id) {
                        window.cursor_position = blame_doc.buffer.cursor();
                    }
                }
            }
        }
    }

    fn update_layout_viewport(&mut self, layout: &WindowLayout, soft_wrap: bool) {
        let Some(window) = self.split_tree.get_window(layout.window_id) else {
            return;
        };
        let cursor_pos = window.cursor_position;
        let doc_id = window.document_id;

        let (
            _tab_width,
            cursor_line,
            _cursor_col,
            total_lines,
            viewport_col,
            gutter_width,
            terminal_resize,
        ) = {
            let Some(doc) = self.document_manager.get_document(doc_id) else {
                return;
            };
            let tab_width = doc.options.tab_width;
            let cursor_line = doc.buffer.line_index.get_line_at(cursor_pos);
            let line_start = doc.buffer.line_index.get_start(cursor_line).unwrap_or(0);
            let cursor_col =
                render::calculate_cursor_column_at(&doc.buffer, cursor_line, tab_width, cursor_pos)
                    + doc.annotations.leading_width_in(line_start, cursor_pos + 1);
            let total_lines = doc.buffer.get_total_lines();
            let viewport_col = if doc.is_terminal() { 0 } else { cursor_col };
            let gutter_width = Self::line_number_gutter_width(doc, &self.state);
            let terminal_resize = if doc.is_terminal() {
                let new_rows = u16::try_from(layout.rows).unwrap_or(u16::MAX);
                let new_cols = u16::try_from(layout.cols).unwrap_or(u16::MAX);
                let needs = doc
                    .terminal()
                    .is_some_and(|terminal| terminal.size != (new_rows, new_cols));
                if needs {
                    Some((new_rows, new_cols))
                } else {
                    None
                }
            } else {
                None
            };
            (
                tab_width,
                cursor_line,
                cursor_col,
                total_lines,
                viewport_col,
                gutter_width,
                terminal_resize,
            )
        };

        if let Some((new_rows, new_cols)) = terminal_resize {
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                if let Some(terminal) = doc.terminal_mut() {
                    terminal.resize(new_rows, new_cols);
                }
                doc.handle_terminal_data(&[]);
            }
        }

        let Some(window) = self.split_tree.get_window_mut(layout.window_id) else {
            return;
        };
        window.viewport.set_size(layout.rows + 1, layout.cols);
        window
            .viewport
            .update(cursor_line, viewport_col, total_lines, gutter_width);

        if soft_wrap {
            let content_width = layout.cols.saturating_sub(gutter_width).max(1);
            if let Some(dm) =
                self.resolve_display_map_cached(doc_id, content_width, cursor_pos, layout.rows + 1)
            {
                let cursor_visual_row = dm.char_to_visual_row(cursor_pos);
                let total_visual = dm.total_visual_rows();
                if let Some(window) = self.split_tree.get_window_mut(layout.window_id) {
                    window
                        .viewport
                        .update_visual(cursor_visual_row, 0, total_visual, gutter_width);
                }
            }
        }
    }

    fn sync_partner_scroll(&mut self, layouts: &[WindowLayout], soft_wrap: bool) {
        let Some(partner_window_id) = self.git_blame_partner_window_id() else {
            return;
        };
        let focused_window_id = self.split_tree.focused_window_id();
        let viewport_state = |window_id| {
            let layout = layouts
                .iter()
                .find(|layout| layout.window_id == window_id)?;
            let window = self.split_tree.get_window(window_id)?;
            Some((
                layout.cols,
                layout.rows,
                window.document_id,
                window.cursor_position,
                window.viewport.top_line(),
                window.viewport.top_visual_row(),
            ))
        };
        let Some((
            focused_cols,
            focused_rows,
            focused_doc_id,
            focused_cursor,
            focused_top_line,
            focused_top_visual,
        )) = viewport_state(focused_window_id)
        else {
            return;
        };
        let Some((
            partner_cols,
            partner_rows,
            partner_doc_id,
            partner_cursor,
            _,
            current_partner_top_visual,
        )) = viewport_state(partner_window_id)
        else {
            return;
        };

        let focused_map =
            self.window_map_key(focused_doc_id, focused_cols)
                .and_then(|(doc_id, width)| {
                    self.resolve_display_map_cached(doc_id, width, focused_cursor, focused_rows + 1)
                });
        let top_line = if soft_wrap {
            focused_map
                .as_ref()
                .and_then(|map| {
                    map.get_visual_row(focused_top_visual)
                        .map(|row| row.logical_line)
                })
                .unwrap_or(focused_top_line)
        } else {
            focused_top_line
        };
        let partner_map =
            self.window_map_key(partner_doc_id, partner_cols)
                .and_then(|(doc_id, width)| {
                    self.resolve_display_map_cached(doc_id, width, partner_cursor, partner_rows + 1)
                });
        let partner_top_visual = if soft_wrap {
            partner_map
                .as_ref()
                .map_or(top_line, |map| map.logical_to_first_visual(top_line))
        } else {
            current_partner_top_visual
        };
        if let Some(partner) = self.split_tree.get_window_mut(partner_window_id) {
            partner.viewport.set_scroll_with_visual(
                top_line,
                partner_top_visual,
                partner.viewport.left_col(),
            );
        }
    }

    pub(super) fn render_multi_window(&mut self, needs_clear: bool) -> Result<(), RiftError> {
        use crate::layer::LayerPriority;
        crate::perf_span!("render_multi_window", crate::perf::PerfFields::default());

        let size = self
            .term
            .get_size()
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERM_SIZE", e.to_string()))?;
        let total_rows = size.rows as usize;
        let total_cols = size.cols as usize;
        let content_rows = total_rows.saturating_sub(1);
        let layouts = self.split_tree.compute_layout(content_rows, total_cols);

        let mut window_maps: Vec<Option<std::sync::Arc<crate::wrap::DisplayMap>>> =
            Vec::with_capacity(layouts.len());
        for layout in &layouts {
            let window = self.split_tree.get_window(layout.window_id);
            let cursor_pos = window.map_or(0, |w| w.cursor_position);
            let key = window
                .map(|w| w.document_id)
                .and_then(|doc_id| self.window_map_key(doc_id, layout.cols));
            window_maps.push(key.and_then(|(doc_id, cw)| {
                self.resolve_display_map_cached(doc_id, cw, cursor_pos, layout.rows + 1)
            }));
        }
        let focused_display_map = {
            let focused_id = self.split_tree.focused_window_id();
            if let Some(i) = layouts.iter().position(|l| l.window_id == focused_id) {
                window_maps[i].clone()
            } else {
                let focused = self.split_tree.focused_window();
                let doc_id = focused.document_id;
                let cursor_pos = focused.cursor_position;
                self.window_map_key(doc_id, total_cols)
                    .and_then(|(doc_id, cw)| {
                        self.resolve_display_map_cached(doc_id, cw, cursor_pos, content_rows + 1)
                    })
            }
        };

        let Self {
            document_manager,
            render_system,
            split_tree,
            ..
        } = self;

        if render_system.compositor.rows() != total_rows
            || render_system.compositor.cols() != total_cols
        {
            render_system.compositor.resize(total_rows, total_cols);
        }

        let resolvable_window_ids: std::collections::HashSet<_> = layouts
            .iter()
            .filter(|l| {
                split_tree
                    .get_window(l.window_id)
                    .is_some_and(|w| document_manager.get_document(w.document_id).is_some())
            })
            .map(|l| l.window_id)
            .collect();
        let window_set_changed = needs_clear
            || resolvable_window_ids.len() != render_system.window_paint_caches.len()
            || !resolvable_window_ids
                .iter()
                .all(|id| render_system.window_paint_caches.contains_key(id));

        let content_layer = render_system
            .compositor
            .get_layer_mut(LayerPriority::CONTENT);
        if window_set_changed {
            content_layer.clear();
        }

        let focused_doc_id = split_tree.focused_window().document_id;
        let pass = WindowPass {
            needs_clear,
            window_set_changed,
            focused_doc_id,
        };
        let mut rendered_window_ids = std::collections::HashSet::new();

        for (layout, display_map) in layouts.iter().zip(&window_maps) {
            if self.render_window_layer(layout, display_map.as_deref(), pass) {
                rendered_window_ids.insert(layout.window_id);
            }
        }

        self.render_system
            .window_paint_caches
            .retain(|id, _| rendered_window_ids.contains(id));
        self.render_focus_overlay(
            &layouts,
            content_rows,
            total_cols,
            focused_display_map.as_deref(),
            needs_clear,
        )
    }
    fn render_window_layer(
        &mut self,
        layout: &WindowLayout,
        display_map: Option<&crate::wrap::DisplayMap>,
        pass: WindowPass,
    ) -> bool {
        let needs_clear = pass.needs_clear;
        let focused_doc_id = pass.focused_doc_id;
        let Self {
            document_manager,
            render_system,
            state,
            current_mode,
            pending_keys,
            pending_count,
            split_tree,
            kind_registry,
            ..
        } = self;
        let Some(window) = split_tree.get_window(layout.window_id) else {
            return false;
        };
        let Some(doc) = document_manager.get_document_mut(window.document_id) else {
            return false;
        };
        doc.recompute_directory_highlights();

        let tab_width = doc.options.tab_width;
        let gutter_width = Self::line_number_gutter_width(doc, state);
        let (start_line, end_line, start_byte, end_byte) =
            Self::visible_span(doc, &window.viewport, display_map);

        let highlights = doc
            .syntax
            .as_mut()
            .map(|syntax| syntax.highlights(Some(start_byte..end_byte)));
        let capture_names = doc
            .syntax
            .as_ref()
            .map(crate::syntax::Syntax::capture_names);
        let injection_hl = doc
            .syntax
            .as_ref()
            .map(|s| s.injection_highlights_named(Some(&(start_byte..end_byte))));
        let annotation_styles = doc.annotations.presentation_spans(
            state.settings.syntax_colors.as_ref(),
            Some(kind_registry),
            start_byte..end_byte,
        );
        let annotation_adornments = doc.annotations.line_adornments(
            state.settings.syntax_colors.as_ref(),
            Some(kind_registry),
            start_byte..end_byte,
            start_line..end_line,
            state.settings.lsp.virtual_text,
            |b| {
                doc.buffer
                    .line_index
                    .get_line_at(doc.buffer.byte_to_char(b))
            },
        );
        let annotation_inline = doc.annotations.inline_adornments(
            state.settings.syntax_colors.as_ref(),
            Some(kind_registry),
            start_byte..end_byte,
        );
        let annotation_concealed = Self::concealed_outside_cursor_line(doc, start_byte..end_byte);

        let git_gutter_colors_data = git_gutter::git_gutter_render_colors(doc);

        let ctx = render::DrawContext {
            buf: &doc.buffer,
            viewport: &window.viewport,
            current_mode: *current_mode,
            pending_key: pending_keys.last().copied(),
            pending_count: *pending_count,
            state,
            needs_clear,
            tab_width,
            highlights: highlights.as_deref(),
            capture_map: capture_names,
            injection_highlights: injection_hl.as_deref(),
            custom_highlights: (!doc.custom_highlights.is_empty())
                .then_some(&doc.custom_highlights),
            git_gutter_colors: (!git_gutter_colors_data.is_empty())
                .then_some(&git_gutter_colors_data),
            annotation_styles: (!annotation_styles.is_empty()).then_some(&annotation_styles),
            annotation_adornments: (!annotation_adornments.is_empty())
                .then_some(&annotation_adornments),
            annotation_inline: (!annotation_inline.is_empty()).then_some(&annotation_inline),
            annotation_concealed: (!annotation_concealed.is_empty())
                .then_some(&annotation_concealed),
            terminal_cell_colors: doc
                .terminal_cell_colors()
                .filter(|colors| !colors.is_empty()),
            show_line_numbers: doc.options.show_line_numbers,
            display_map,
            gutter_width_override: Some(gutter_width),
            search_matches_override: if window.document_id == focused_doc_id {
                None
            } else {
                Some(&[])
            },
            syntax_generation: Self::syntax_generation(doc),
            annotations_revision: doc.annotations.revision(),
            kind_registry_generation: kind_registry.generation(),
        };

        Self::paint_window_layer(render_system, layout, &ctx, pass);
        true
    }

    fn active_gutter_cols(&self, doc_id: crate::document::DocumentId) -> usize {
        self.document_manager.get_document(doc_id).map_or(0, |doc| {
            self.state.gutter_cols(doc.options.show_line_numbers)
        })
    }

    fn line_number_gutter_width(
        doc: &crate::document::Document,
        state: &crate::state::State,
    ) -> usize {
        state.doc_gutter_cols(doc.options.show_line_numbers, doc.buffer.get_total_lines())
    }

    fn concealed_outside_cursor_line(
        doc: &crate::document::Document,
        span: std::ops::Range<usize>,
    ) -> Vec<(usize, usize)> {
        let cursor_line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
        doc.annotations
            .concealed_ranges(span)
            .into_iter()
            .filter(|(s, _)| {
                doc.buffer
                    .line_index
                    .get_line_at(doc.buffer.byte_to_char(*s))
                    != cursor_line
            })
            .collect()
    }

    fn syntax_generation(doc: &crate::document::Document) -> u64 {
        doc.syntax.as_ref().map_or(0, |s| {
            render::mix_generation(s.highlights_generation(), s.injection_generation())
        })
    }

    fn paint_window_layer(
        render_system: &mut crate::render::RenderSystem,
        layout: &WindowLayout,
        ctx: &render::DrawContext,
        pass: WindowPass,
    ) {
        use crate::layer::LayerPriority;

        let window_cache = render_system
            .window_paint_caches
            .entry(layout.window_id)
            .or_insert_with(|| crate::render::system::WindowPaintCache {
                frame: crate::paint::PaintFrame::new(0),
                blit_key: None,
                layout: layout.clone(),
            });
        if window_cache.layout != *layout || pass.window_set_changed {
            window_cache.blit_key = None;
            window_cache.layout = layout.clone();
        }

        let content_layer = render_system
            .compositor
            .get_layer_mut(LayerPriority::CONTENT);
        render::render_content_to_layer_offset(
            content_layer,
            ctx,
            layout.row,
            layout.col,
            &mut window_cache.frame,
            Some(&mut window_cache.blit_key),
        );
    }

    fn render_focus_overlay(
        &mut self,
        layouts: &[WindowLayout],
        content_rows: usize,
        total_cols: usize,
        focused_display_map: Option<&crate::wrap::DisplayMap>,
        needs_clear: bool,
    ) -> Result<(), RiftError> {
        use crate::layer::LayerPriority;

        let Self {
            document_manager,
            render_system,
            state,
            current_mode,
            term,
            pending_keys,
            pending_count,
            split_tree,
            ..
        } = self;
        let focused_id = split_tree.focused_window_id();
        let divider_fg = state
            .settings
            .syntax_colors
            .as_ref()
            .and_then(|sc| sc.get_color("comment"))
            .or(state.settings.editor_fg);
        let content_layer = render_system
            .compositor
            .get_layer_mut(LayerPriority::CONTENT);
        render::render_dividers(
            content_layer,
            split_tree,
            content_rows,
            total_cols,
            divider_fg,
            state.settings.editor_bg,
        );

        if layouts.len() > 1 {
            if let Some(fl) = layouts.iter().find(|l| l.window_id == focused_id) {
                let focused_border_fg = state
                    .settings
                    .editor_fg
                    .or(Some(crate::color::Color::White));
                let content_layer = render_system
                    .compositor
                    .get_layer_mut(crate::layer::LayerPriority::CONTENT);
                render::highlight_focused_window_border(
                    content_layer,
                    fl,
                    content_rows,
                    total_cols,
                    focused_border_fg,
                    state.settings.editor_bg,
                );
            }
        }

        let focused_layout = layouts.iter().find(|l| l.window_id == focused_id).cloned();

        let focused_window = split_tree.focused_window();
        let Some(focused_doc) = document_manager.get_document_mut(focused_window.document_id)
        else {
            return Ok(());
        };

        let (row_off, col_off) = focused_layout.as_ref().map_or((0, 0), |l| (l.row, l.col));

        let focused_tab_width = focused_doc.options.tab_width;

        let focused_vp = &split_tree.focused_window().viewport;
        let render_state = render::RenderState {
            buf: &focused_doc.buffer,
            state,
            current_mode: *current_mode,
            pending_key: pending_keys.last().copied(),
            pending_count: *pending_count,
            needs_clear,
            tab_width: focused_tab_width,
            highlights: None,
            capture_map: None,
            injection_highlights: None,
            skip_content: true,
            cursor_row_offset: row_off,
            cursor_col_offset: col_off,
            cursor_viewport: Some(focused_vp),
            terminal_cursor: focused_doc.terminal_cursor(),
            custom_highlights: if focused_doc.custom_highlights.is_empty() {
                None
            } else {
                Some(&focused_doc.custom_highlights)
            },
            git_gutter_colors: None,
            annotation_styles: None,
            annotation_adornments: None,
            annotation_inline: None,
            annotation_concealed: None,
            terminal_cell_colors: focused_doc
                .terminal_cell_colors()
                .filter(|colors| !colors.is_empty()),
            show_line_numbers: focused_doc.options.show_line_numbers,
            display_map: focused_display_map,
            scroll_hint: None,
            syntax_generation: 0,
            annotations_revision: 0,
            kind_registry_generation: 0,
        };

        let _ = render_system.render(term, &render_state)?;

        Ok(())
    }

    pub(super) fn render_annotation_tooltip(&mut self, cursor_row: usize) {
        use crate::color::Color;
        use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
        use crate::layer::{Cell, LayerPriority};
        const MAX_WIDTH: usize = 80;
        const MAX_TIP_LINES: usize = 6;
        crate::perf_span!(
            "render_annotation_tooltip",
            crate::perf::PerfFields::default()
        );

        let kind_registry = &self.kind_registry;
        let include_lsp = self.state.settings.lsp.diagnostic_tooltip;
        let (tip, affordance) =
            self.document_manager
                .active_document_mut()
                .map_or((None, None), |doc| {
                    let cursor = doc.buffer.cursor();
                    let line = doc.buffer.line_index.get_line_at(cursor);
                    let cursor_byte = doc.buffer.char_to_byte(cursor);
                    let line_of = |b: usize| {
                        doc.buffer
                            .line_index
                            .get_line_at(doc.buffer.byte_to_char(b))
                    };
                    let tip = doc
                        .annotations
                        .tooltip_at(cursor_byte, Some(kind_registry), include_lsp)
                        .or_else(|| {
                            doc.annotations.tooltip_at_line(
                                line,
                                Some(kind_registry),
                                include_lsp,
                                line_of,
                            )
                        })
                        .map(std::string::ToString::to_string);
                    let affordance = doc
                        .annotations
                        .interactive_at(cursor_byte)
                        .or_else(|| doc.annotations.interactive_at_line(line))
                        .and_then(|a| a.affordance_line("Enter"));
                    (tip, affordance)
                });

        self.render_system
            .compositor
            .clear_layer(LayerPriority::TOOLTIP);
        if tip.is_none() && affordance.is_none() {
            return;
        }

        let layer = self
            .render_system
            .compositor
            .get_layer_mut(LayerPriority::TOOLTIP);
        let screen_rows = layer.rows();
        let wrap_width = MAX_WIDTH.min(layer.cols().saturating_sub(4)).max(1);
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        if let Some(tip) = &tip {
            for line in render::wrap_text(tip, wrap_width)
                .into_iter()
                .take(MAX_TIP_LINES)
            {
                rows.push(
                    line.chars()
                        .map(|c| Cell::from_char(c).with_colors(Some(Color::White), None))
                        .collect(),
                );
            }
        }
        if let Some(affordance) = &affordance {
            rows.push(
                affordance
                    .chars()
                    .take(wrap_width)
                    .map(|c| Cell::from_char(c).with_colors(Some(Color::Cyan), None))
                    .collect(),
            );
        }
        let width = rows.iter().map(std::vec::Vec::len).max().unwrap_or(0) + 2;
        let height = rows.len() + 2;
        let position = if cursor_row + height + 1 >= screen_rows {
            WindowPosition::Top
        } else {
            WindowPosition::Bottom
        };
        let foreground = self.state.settings.editor_fg;
        let background = self.state.settings.editor_bg;
        let window = FloatingWindow::with_style(
            position,
            width,
            height,
            WindowStyle::new()
                .with_border(true)
                .with_reverse_video(false)
                .with_fg(foreground.unwrap_or(Color::White))
                .with_bg(background.unwrap_or(Color::Black)),
        );
        window.render_cells(layer, &rows);
    }

    pub(super) fn render_explorer_diff_tooltip(&mut self) {
        use crate::color::Color;
        use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
        use crate::layer::{Cell, LayerPriority};
        const MAX_WIDTH: usize = 52;
        crate::perf_span!(
            "render_explorer_diff_tooltip",
            crate::perf::PerfFields::default()
        );

        let doc_id = self.split_tree.focused_window().document_id;
        let is_dir = self
            .document_manager
            .get_document(doc_id)
            .is_some_and(super::super::document::Document::is_directory);

        if !is_dir {
            self.render_system
                .compositor
                .clear_layer(LayerPriority::HOVER);
            return;
        }

        let diff = self
            .document_manager
            .get_document(doc_id)
            .map(super::super::document::Document::parse_directory_diff)
            .unwrap_or_default();

        if diff.renames.is_empty() && diff.deletes.is_empty() && diff.creates.is_empty() {
            self.render_system
                .compositor
                .clear_layer(LayerPriority::HOVER);
            return;
        }

        let mut rows: Vec<Vec<Cell>> = Vec::new();

        for (old_path, new_name) in &diff.renames {
            let old_name = old_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let text = format!("~ {old_name} -> {new_name}");
            let row: Vec<Cell> = text
                .chars()
                .take(MAX_WIDTH)
                .map(|c| Cell::from_char(c).with_colors(Some(Color::Yellow), None))
                .collect();
            rows.push(row);
        }
        for path in &diff.deletes {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let text = format!("- {name}");
            let row: Vec<Cell> = text
                .chars()
                .take(MAX_WIDTH)
                .map(|c| Cell::from_char(c).with_colors(Some(Color::Red), None))
                .collect();
            rows.push(row);
        }
        for name in &diff.creates {
            let text = format!("+ {name}");
            let row: Vec<Cell> = text
                .chars()
                .take(MAX_WIDTH)
                .map(|c| Cell::from_char(c).with_colors(Some(Color::Green), None))
                .collect();
            rows.push(row);
        }

        let foreground = self.state.settings.editor_fg;
        let background = self.state.settings.editor_bg;

        let window = FloatingWindow::with_style(
            WindowPosition::Top,
            MAX_WIDTH + 2,
            rows.len() + 2,
            WindowStyle::new()
                .with_border(true)
                .with_reverse_video(false)
                .with_fg(foreground.unwrap_or(Color::White))
                .with_bg(background.unwrap_or(Color::Black)),
        );

        let layer = self
            .render_system
            .compositor
            .get_layer_mut(LayerPriority::HOVER);
        layer.clear();
        window.render_cells(layer, &rows);
    }
}

pub(crate) fn combine_char_edits(
    edits: &[crate::buffer::CharEdit],
) -> Option<crate::buffer::CharEdit> {
    let first = *edits.first()?;
    if edits.len() == 1 {
        return Some(first);
    }

    let mut left = first.pos;
    let mut right_orig = first.pos + first.del;
    let mut right_cur = first.pos + first.ins;

    for e in &edits[1..] {
        if e.pos > right_cur || e.pos + e.del < left {
            return None;
        }
        if e.pos < left {
            left = e.pos;
        }
        right_orig += (e.pos + e.del).saturating_sub(right_cur);
        right_cur = e.pos + e.ins + right_cur.saturating_sub(e.pos + e.del);
    }

    Some(crate::buffer::CharEdit {
        pos: left,
        del: right_orig - left,
        ins: right_cur - left,
    })
}

#[cfg(test)]
#[path = "combine_char_edits_tests.rs"]
mod combine_char_edits_tests;
