use super::resolve_display_map;
use super::Editor;
use crate::error::{ErrorType, RiftError};
use crate::render;
use crate::screen_buffer::FrameStats;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn update_state_and_render(
        &mut self,
        keypress: crate::key::Key,
        action: crate::key_handler::KeyAction,
        command: crate::command::Command,
    ) -> Result<(), RiftError> {
        self.handle_key_actions(action);
        self.handle_mode_management(command);

        // Update input tracking (happens during state update, not input handling)
        self.state.update_keypress(keypress);
        self.state.update_command(command);

        self.update_and_render()
    }

    /// State-update phase: syncs viewport/cursor/document-derived state ahead of
    /// rendering. No cell composition or layer writes happen here.
    fn update_state(
        &mut self,
    ) -> Option<(
        crate::document::DocumentId,
        bool,
        Option<std::sync::Arc<crate::wrap::DisplayMap>>,
    )> {
        self.flush_pending_text_changed();
        self.flush_pending_cursor_moved();
        // Fire cursor enter/leave annotation hooks for any transition this frame.
        self.update_annotation_hover();
        if self.split_tree.window_count() == 1 {
            let rows = self.render_system.viewport.visible_rows();
            let cols = self.render_system.viewport.visible_cols();
            self.split_tree
                .focused_window_mut()
                .viewport
                .set_size(rows, cols);
        }

        // Sync buffer cursor to focused window
        let doc_id = self.split_tree.focused_window().document_id;
        if let Some(doc) = self.document_manager.get_document(doc_id) {
            self.split_tree.focused_window_mut().cursor_position = doc.buffer.cursor();
        }

        // Recompute highlights before reading cursor state so the status bar col is correct.
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.recompute_directory_highlights();
        }

        let (cursor_line, cursor_col, total_lines, is_terminal) =
            if let Some(doc) = self.document_manager.get_document(doc_id) {
                let tw = doc.options.tab_width;
                let line = doc.buffer.get_line();
                let cursor = doc.buffer.cursor();
                let line_start = doc.buffer.line_index.get_start(line).unwrap_or(0);
                // Offset the cursor by leading virtual text before it; `cursor + 1`
                // because a marker on the cursor's own char renders before it.
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
        let gutter_width = if self.state.settings.show_line_numbers {
            self.state.gutter_width
        } else {
            0
        };

        let display_map = {
            let content_width = self
                .render_system
                .viewport
                .visible_cols()
                .saturating_sub(gutter_width)
                .max(1);
            self.resolve_display_map_cached(doc_id, content_width)
        };

        let needs_clear = if let Some(ref dm) = display_map {
            let doc = self.document_manager.get_document(doc_id).unwrap();
            let visual_row = dm.char_to_visual_row(doc.buffer.cursor());
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

        Some((doc_id, needs_clear, display_map))
    }

    /// Recompute `ui.selection.*` annotations from the active Visual region
    /// (if any) and the active document's banked `SelectionSet`.
    pub(super) fn update_selection_highlights(&mut self) {
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
        // Nothing selected and no leftover ui.selection.* annotation: skip
        // the full annotation clear + interval-index rebuild.
        if active.is_none()
            && banked.is_empty()
            && doc.annotations.query_kind("ui.selection").next().is_none()
        {
            return;
        }
        doc.sync_selection_annotations(active, &banked);
    }

    pub fn update_and_render(&mut self) -> Result<(), RiftError> {
        let Some((_doc_id, needs_clear, display_map)) = self.update_state() else {
            return Ok(());
        };
        self.update_selection_highlights();

        self.render_plugin_float();

        // Populate the TOOLTIP layer before the main render so it's included
        // in the same compositor flush.
        if self.post_paste_state.is_some() {
            self.render_clipboard_tooltip();
        } else {
            self.render_annotation_tooltip();
        }

        self.render_explorer_diff_tooltip();

        if self.split_tree.window_count() > 1 {
            self.update_window_viewports();
            self.render_multi_window(needs_clear)
        } else {
            self.render(needs_clear, display_map.as_deref())
        }
    }

    /// Resolve the display map, reusing `display_map_cache` when revision,
    /// width, and wrap/tab params match (since `:set wrap` doesn't bump revision).
    /// A cached map that is exactly one buffer edit behind is patched in place.
    pub(super) fn resolve_display_map_cached(
        &mut self,
        doc_id: crate::document::DocumentId,
        content_width: usize,
    ) -> Option<std::sync::Arc<crate::wrap::DisplayMap>> {
        use std::sync::Arc;
        let soft_wrap = self.state.settings.soft_wrap;
        let wrap_width = self.state.settings.wrap_width;
        let doc = self.document_manager.get_document_mut(doc_id)?;
        let revision = doc.buffer.revision;
        let params = super::resolve_wrap_params(doc, content_width, soft_wrap, wrap_width);
        let edits = doc.buffer.take_char_edits();

        if let Some((cid, crev, cw, cached)) = self.display_map_cache.take() {
            if cid == doc_id && cw == content_width {
                let valid = match (&params, &cached) {
                    (None, None) => true,
                    (Some((w, tw)), Some(m)) => m.wrap_width == *w && m.tab_width == *tw,
                    _ => false,
                };
                if valid {
                    if crev == revision {
                        self.display_map_cache = Some((cid, crev, cw, cached.clone()));
                        return cached;
                    }
                    // Exactly one edit behind: rewrap only the affected lines.
                    if edits.len() == 1 && crev.wrapping_add(1) == revision {
                        if let Some(mut map) = cached {
                            let e = edits[0];
                            if Arc::make_mut(&mut map).apply_edit(&doc.buffer, e.pos, e.del, e.ins)
                            {
                                self.display_map_cache =
                                    Some((doc_id, revision, cw, Some(map.clone())));
                                return Some(map);
                            }
                        }
                    }
                }
            }
        }

        let map =
            params.map(|(w, tw)| Arc::new(crate::wrap::DisplayMap::build(&doc.buffer, w, tw)));
        self.display_map_cache = Some((doc_id, revision, content_width, map.clone()));
        map
    }

    /// Render the clipboard ring tooltip to the TOOLTIP layer.
    pub(super) fn render_clipboard_tooltip(&mut self) {
        let selected = self
            .post_paste_state
            .as_ref()
            .map(|s| s.ring_index)
            .unwrap_or(0);
        let editor_fg = self.state.settings.editor_fg;
        let editor_bg = self.state.settings.editor_bg;
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
            editor_fg,
            editor_bg,
        );
    }

    /// Render the editor interface (pure read - no mutations)
    /// Uses the layer compositor for composited rendering
    pub fn render_to_terminal(&mut self, needs_clear: bool) -> Result<FrameStats, RiftError> {
        self.term.hide_cursor()?;
        let stats = self
            .render_system
            .compositor
            .render_to_terminal(&mut self.term, needs_clear)
            .map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::RENDER_FAILED,
                    e,
                )
            })?;
        self.term.show_cursor()?;
        self.term.flush()?;
        Ok(stats)
    }

    /// Render the editor interface (pure read - no mutations)
    /// Uses the layer compositor for composited rendering
    pub(super) fn render(
        &mut self,
        needs_clear: bool,
        display_map: Option<&crate::wrap::DisplayMap>,
    ) -> Result<(), RiftError> {
        let Editor {
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

        // We need mutable access to call syntax.highlights() which potentially
        // updates parse tree, and to recompute directory highlights.
        let doc = match document_manager.active_document_mut() {
            Some(d) => d,
            None => return Ok(()),
        };
        doc.recompute_directory_highlights();

        let (start_logical, end_logical) = if let Some(dm) = display_map {
            let top_vr = render_system.viewport.top_visual_row();
            let bottom_vr = top_vr + render_system.viewport.visible_rows();
            let start_l = dm
                .get_visual_row(top_vr)
                .map(|r| r.logical_line)
                .unwrap_or(0);
            let end_l = dm
                .get_visual_row(
                    bottom_vr
                        .saturating_sub(1)
                        .min(dm.total_visual_rows().saturating_sub(1)),
                )
                .map(|r| r.logical_line + 1)
                .unwrap_or(doc.buffer.get_total_lines());
            (start_l, end_l)
        } else {
            let start = render_system.viewport.top_line();
            let end = start + render_system.viewport.visible_rows();
            (start, end)
        };

        let start_char = doc.buffer.line_index.get_start(start_logical).unwrap_or(0);
        let end_char = if end_logical < doc.buffer.get_total_lines() {
            doc.buffer
                .line_index
                .get_start(end_logical)
                .unwrap_or(doc.buffer.len())
        } else {
            doc.buffer.len()
        };

        // Convert to byte offsets for tree-sitter
        let start_byte = doc.buffer.char_to_byte(start_char);
        let end_byte = doc.buffer.char_to_byte(end_char);

        let highlights = doc
            .syntax
            .as_mut()
            .map(|syntax| syntax.highlights(Some(start_byte..end_byte)));

        let capture_names = doc.syntax.as_ref().map(|s| s.capture_names());
        let injection_hl = doc
            .syntax
            .as_ref()
            .map(|s| s.injection_highlights_named(Some(start_byte..end_byte)));

        // Generic annotation presentation overlay (design.md sec 8), restricted
        // to the visible viewport rather than a full-document scan.
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
        // Conceal ranges, minus those on the cursor's line (reveal-on-cursor-line).
        // `s` is a byte offset but line_index is char-indexed, so convert first.
        let cursor_line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
        let annotation_concealed: Vec<(usize, usize)> = doc
            .annotations
            .concealed_ranges(start_byte..end_byte)
            .into_iter()
            .filter(|(s, _)| {
                doc.buffer
                    .line_index
                    .get_line_at(doc.buffer.byte_to_char(*s))
                    != cursor_line
            })
            .collect();

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
            terminal_cursor: doc.terminal_cursor,
            custom_highlights: if doc.custom_highlights.is_empty() {
                None
            } else {
                Some(&doc.custom_highlights)
            },
            plugin_highlights: if doc.plugin_highlights.is_empty() {
                None
            } else {
                Some(&doc.plugin_highlights)
            },
            annotation_styles: if annotation_styles.is_empty() {
                None
            } else {
                Some(&annotation_styles)
            },
            annotation_adornments: if annotation_adornments.is_empty() {
                None
            } else {
                Some(&annotation_adornments)
            },
            annotation_inline: if annotation_inline.is_empty() {
                None
            } else {
                Some(&annotation_inline)
            },
            annotation_concealed: if annotation_concealed.is_empty() {
                None
            } else {
                Some(&annotation_concealed)
            },
            terminal_cell_colors: if doc.terminal_cell_colors.is_empty() {
                None
            } else {
                Some(&doc.terminal_cell_colors)
            },
            show_line_numbers: doc.options.show_line_numbers,
            display_map,
        };

        let _ = render_system.render(term, state)?;

        Ok(())
    }

    /// If a plugin float is open, render it into the POPUP layer.
    /// Clears the layer once when a float is closed.
    pub(super) fn render_plugin_float(&mut self) {
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
        let global_show_line_numbers = self.state.settings.show_line_numbers;
        let soft_wrap = self.state.settings.soft_wrap;
        let wrap_width = self.state.settings.wrap_width;

        let size = match self.term.get_size() {
            Ok(s) => s,
            Err(_) => return,
        };
        let content_rows = (size.rows as usize).saturating_sub(1);
        let layouts = self
            .split_tree
            .compute_layout(content_rows, size.cols as usize);

        for layout in &layouts {
            let window = match self.split_tree.get_window(layout.window_id) {
                Some(w) => w,
                None => continue,
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
                let doc = match self.document_manager.get_document(doc_id) {
                    Some(d) => d,
                    None => continue,
                };
                let tab_width = doc.options.tab_width;
                let cursor_line = doc.buffer.line_index.get_line_at(cursor_pos);
                let line_start = doc.buffer.line_index.get_start(cursor_line).unwrap_or(0);
                let cursor_col = render::calculate_cursor_column_at(
                    &doc.buffer,
                    cursor_line,
                    tab_width,
                    cursor_pos,
                ) + doc.annotations.leading_width_in(line_start, cursor_pos + 1);
                let total_lines = doc.buffer.get_total_lines();
                let viewport_col = if doc.is_terminal() { 0 } else { cursor_col };
                let doc_show_line_numbers =
                    doc.options.show_line_numbers && global_show_line_numbers;
                let gutter_width = if doc_show_line_numbers {
                    total_lines.to_string().len() + 2
                } else {
                    0
                };
                let terminal_resize = if doc.is_terminal() {
                    let new_rows = layout.rows as u16;
                    let new_cols = layout.cols as u16;
                    let needs = doc
                        .terminal
                        .as_ref()
                        .map(|t| t.size != (new_rows, new_cols))
                        .unwrap_or(false);
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
                    if let Some(terminal) = &mut doc.terminal {
                        let _ = terminal.resize(new_rows, new_cols);
                    }
                    doc.handle_terminal_data(&[]);
                }
            }

            let window = match self.split_tree.get_window_mut(layout.window_id) {
                Some(w) => w,
                None => continue,
            };
            // +1 because render_content_to_layer_offset does saturating_sub(1)
            // for the global status bar; multi-window layouts don't need that.
            window.viewport.set_size(layout.rows + 1, layout.cols);
            window
                .viewport
                .update(cursor_line, viewport_col, total_lines, gutter_width);

            if soft_wrap {
                let content_width = layout.cols.saturating_sub(gutter_width).max(1);
                let doc = match self.document_manager.get_document(doc_id) {
                    Some(d) => d,
                    None => continue,
                };
                if let Some(dm) = resolve_display_map(doc, content_width, soft_wrap, wrap_width) {
                    let cursor_visual_row = dm.char_to_visual_row(cursor_pos);
                    let total_visual = dm.total_visual_rows();
                    window
                        .viewport
                        .update_visual(cursor_visual_row, 0, total_visual, gutter_width);
                }
            }
        }
    }

    pub(super) fn render_multi_window(&mut self, needs_clear: bool) -> Result<(), RiftError> {
        use crate::layer::LayerPriority;

        let Editor {
            document_manager,
            render_system,
            state,
            current_mode,
            term,
            pending_keys,
            pending_count,
            split_tree,
            kind_registry,
            ..
        } = self;

        let size = term
            .get_size()
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERM_SIZE", e))?;
        let total_rows = size.rows as usize;
        let total_cols = size.cols as usize;
        let content_rows = total_rows.saturating_sub(1);
        let layouts = split_tree.compute_layout(content_rows, total_cols);

        if render_system.compositor.rows() != total_rows
            || render_system.compositor.cols() != total_cols
        {
            render_system.compositor.resize(total_rows, total_cols);
        }

        let content_layer = render_system
            .compositor
            .get_layer_mut(LayerPriority::CONTENT);
        content_layer.clear();

        let focused_id = split_tree.focused_window_id();
        let focused_doc_id = split_tree.focused_window().document_id;

        for layout in &layouts {
            let window = match split_tree.get_window(layout.window_id) {
                Some(w) => w,
                None => continue,
            };
            let doc = match document_manager.get_document_mut(window.document_id) {
                Some(d) => d,
                None => continue,
            };
            doc.recompute_directory_highlights();

            let tab_width = doc.options.tab_width;

            let doc_show_line_numbers =
                doc.options.show_line_numbers && state.settings.show_line_numbers;
            let gutter_width = if doc_show_line_numbers {
                doc.buffer.get_total_lines().to_string().len() + 2
            } else {
                0
            };
            let window_cols = layout.cols;
            let content_width = window_cols.saturating_sub(gutter_width).max(1);
            let display_map = resolve_display_map(
                doc,
                content_width,
                state.settings.soft_wrap,
                state.settings.wrap_width,
            );

            let (start_line, end_line) = if let Some(ref dm) = display_map {
                let top_vr = window.viewport.top_visual_row();
                let bottom_vr = top_vr + window.viewport.visible_rows();
                let start_l = dm
                    .get_visual_row(top_vr)
                    .map(|r| r.logical_line)
                    .unwrap_or(0);
                let end_l = dm
                    .get_visual_row(
                        bottom_vr
                            .saturating_sub(1)
                            .min(dm.total_visual_rows().saturating_sub(1)),
                    )
                    .map(|r| r.logical_line + 1)
                    .unwrap_or(doc.buffer.get_total_lines());
                (start_l, end_l)
            } else {
                let start = window.viewport.top_line();
                let end = start + window.viewport.visible_rows();
                (start, end)
            };
            let start_char = doc.buffer.line_index.get_start(start_line).unwrap_or(0);
            let end_char = if end_line < doc.buffer.get_total_lines() {
                doc.buffer
                    .line_index
                    .get_start(end_line)
                    .unwrap_or(doc.buffer.len())
            } else {
                doc.buffer.len()
            };
            let start_byte = doc.buffer.char_to_byte(start_char);
            let end_byte = doc.buffer.char_to_byte(end_char);

            let highlights = doc
                .syntax
                .as_mut()
                .map(|syntax| syntax.highlights(Some(start_byte..end_byte)));
            let capture_names = doc.syntax.as_ref().map(|s| s.capture_names());
            let injection_hl = doc
                .syntax
                .as_ref()
                .map(|s| s.injection_highlights_named(Some(start_byte..end_byte)));
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
            let cursor_line = doc.buffer.line_index.get_line_at(doc.buffer.cursor());
            let annotation_concealed: Vec<(usize, usize)> = doc
                .annotations
                .concealed_ranges(start_byte..end_byte)
                .into_iter()
                .filter(|(s, _)| {
                    doc.buffer
                        .line_index
                        .get_line_at(doc.buffer.byte_to_char(*s))
                        != cursor_line
                })
                .collect();

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
                custom_highlights: if doc.custom_highlights.is_empty() {
                    None
                } else {
                    Some(&doc.custom_highlights)
                },
                plugin_highlights: if doc.plugin_highlights.is_empty() {
                    None
                } else {
                    Some(&doc.plugin_highlights)
                },
                annotation_styles: if annotation_styles.is_empty() {
                    None
                } else {
                    Some(&annotation_styles)
                },
                annotation_adornments: if annotation_adornments.is_empty() {
                    None
                } else {
                    Some(&annotation_adornments)
                },
                annotation_inline: if annotation_inline.is_empty() {
                    None
                } else {
                    Some(&annotation_inline)
                },
                annotation_concealed: if annotation_concealed.is_empty() {
                    None
                } else {
                    Some(&annotation_concealed)
                },
                terminal_cell_colors: if doc.terminal_cell_colors.is_empty() {
                    None
                } else {
                    Some(&doc.terminal_cell_colors)
                },
                show_line_numbers: doc.options.show_line_numbers,
                display_map: display_map.as_ref(),
                gutter_width_override: Some(gutter_width),
                search_matches_override: if window.document_id == focused_doc_id {
                    None
                } else {
                    Some(&[])
                },
            };

            let content_layer = render_system
                .compositor
                .get_layer_mut(LayerPriority::CONTENT);
            render::render_content_to_layer_offset(content_layer, &ctx, layout.row, layout.col)
                .map_err(|e| RiftError::new(ErrorType::Renderer, "RENDER_FAILED", e))?;
        }

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
        let focused_doc = match document_manager.get_document_mut(focused_window.document_id) {
            Some(d) => d,
            None => return Ok(()),
        };

        let (row_off, col_off, focused_cols) = focused_layout
            .as_ref()
            .map(|l| (l.row, l.col, l.cols))
            .unwrap_or((0, 0, total_cols));

        let focused_tab_width = focused_doc.options.tab_width;
        let focused_doc_show_line_numbers =
            focused_doc.options.show_line_numbers && state.settings.show_line_numbers;
        let focused_gutter_width = if focused_doc_show_line_numbers {
            focused_doc.buffer.get_total_lines().to_string().len() + 2
        } else {
            0
        };
        let focused_content_width = focused_cols.saturating_sub(focused_gutter_width).max(1);
        let focused_display_map = resolve_display_map(
            focused_doc,
            focused_content_width,
            state.settings.soft_wrap,
            state.settings.wrap_width,
        );

        let focused_vp = &split_tree.focused_window().viewport;
        let render_state = render::RenderState {
            buf: &focused_doc.buffer,
            state,
            current_mode: *current_mode,
            pending_key: pending_keys.last().copied(),
            pending_count: *pending_count,
            needs_clear,
            tab_width: focused_tab_width,
            // Cursor-only overlay (skip_content): highlights are never drawn
            // from this state, so skip the full-file capture collection.
            highlights: None,
            capture_map: None,
            injection_highlights: None,
            skip_content: true,
            cursor_row_offset: row_off,
            cursor_col_offset: col_off,
            cursor_viewport: Some(focused_vp),
            terminal_cursor: focused_doc.terminal_cursor,
            custom_highlights: if focused_doc.custom_highlights.is_empty() {
                None
            } else {
                Some(&focused_doc.custom_highlights)
            },
            plugin_highlights: if focused_doc.plugin_highlights.is_empty() {
                None
            } else {
                Some(&focused_doc.plugin_highlights)
            },
            // Cursor-only overlay (skip_content); no content styling needed.
            annotation_styles: None,
            annotation_adornments: None,
            annotation_inline: None,
            annotation_concealed: None,
            terminal_cell_colors: if focused_doc.terminal_cell_colors.is_empty() {
                None
            } else {
                Some(&focused_doc.terminal_cell_colors)
            },
            show_line_numbers: focused_doc.options.show_line_numbers,
            display_map: focused_display_map.as_ref(),
        };

        let _ = render_system.render(term, render_state)?;

        Ok(())
    }

    /// Render a hover tooltip for the annotation under the cursor (e.g. an LSP
    /// diagnostic message) into the TOOLTIP layer. Clears it when there is none.
    pub(super) fn render_annotation_tooltip(&mut self) {
        use crate::color::Color;
        use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
        use crate::layer::{Cell, LayerPriority};

        let kind_registry = &self.kind_registry;
        let (tip, affordance) =
            self.document_manager
                .active_document_mut()
                .map_or((None, None), |doc| {
                    let cursor = doc.buffer.cursor();
                    let line = doc.buffer.line_index.get_line_at(cursor);
                    let cursor_byte = doc.buffer.char_to_byte(cursor);
                    let tip = doc
                        .annotations
                        .tooltip_at(cursor_byte, Some(kind_registry))
                        .or_else(|| doc.annotations.tooltip_at_line(line, Some(kind_registry)))
                        .map(|s| s.to_string());
                    // Affordance hint for the interactive annotation under the
                    // cursor, so its actions + key bindings are discoverable.
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

        const MAX_WIDTH: usize = 80;
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        if let Some(tip) = &tip {
            rows.push(
                tip.chars()
                    .take(MAX_WIDTH)
                    .map(|c| Cell::from_char(c).with_colors(Some(Color::White), None))
                    .collect(),
            );
        }
        if let Some(affordance) = &affordance {
            rows.push(
                affordance
                    .chars()
                    .take(MAX_WIDTH)
                    .map(|c| Cell::from_char(c).with_colors(Some(Color::Cyan), None))
                    .collect(),
            );
        }
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(0) + 2;
        let height = rows.len() + 2;
        let editor_fg = self.state.settings.editor_fg;
        let editor_bg = self.state.settings.editor_bg;
        let window = FloatingWindow::with_style(
            WindowPosition::Bottom,
            width,
            height,
            WindowStyle::new()
                .with_border(true)
                .with_reverse_video(false)
                .with_fg(editor_fg.unwrap_or(Color::White))
                .with_bg(editor_bg.unwrap_or(Color::Black)),
        );
        let layer = self
            .render_system
            .compositor
            .get_layer_mut(LayerPriority::TOOLTIP);
        window.render_cells(layer, &rows);
    }

    /// Render a pending-changes tooltip at the top of the screen when a directory
    /// buffer has unsaved edits. Clears the HOVER layer when there is nothing to show.
    pub(super) fn render_explorer_diff_tooltip(&mut self) {
        use crate::color::Color;
        use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
        use crate::layer::{Cell, LayerPriority};

        let doc_id = self.split_tree.focused_window().document_id;
        let is_dir = self
            .document_manager
            .get_document(doc_id)
            .map(|d| d.is_directory())
            .unwrap_or(false);

        if !is_dir {
            self.render_system
                .compositor
                .clear_layer(LayerPriority::HOVER);
            return;
        }

        // parse_directory_diff compares live buffer text against annotated originals,
        // so it correctly reflects in-progress insert-mode edits even before history commits.
        let diff = self
            .document_manager
            .get_document(doc_id)
            .map(|d| d.parse_directory_diff())
            .unwrap_or_default();

        if diff.renames.is_empty() && diff.deletes.is_empty() && diff.creates.is_empty() {
            self.render_system
                .compositor
                .clear_layer(LayerPriority::HOVER);
            return;
        }

        const MAX_WIDTH: usize = 52;
        let mut rows: Vec<Vec<Cell>> = Vec::new();

        for (old_path, new_name) in &diff.renames {
            let old_name = old_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let text = format!("~ {} -> {}", old_name, new_name);
            let row: Vec<Cell> = text
                .chars()
                .take(MAX_WIDTH)
                .map(|c| Cell::from_char(c).with_colors(Some(Color::Yellow), None))
                .collect();
            rows.push(row);
        }
        for path in &diff.deletes {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let text = format!("- {}", name);
            let row: Vec<Cell> = text
                .chars()
                .take(MAX_WIDTH)
                .map(|c| Cell::from_char(c).with_colors(Some(Color::Red), None))
                .collect();
            rows.push(row);
        }
        for name in &diff.creates {
            let text = format!("+ {}", name);
            let row: Vec<Cell> = text
                .chars()
                .take(MAX_WIDTH)
                .map(|c| Cell::from_char(c).with_colors(Some(Color::Green), None))
                .collect();
            rows.push(row);
        }

        let editor_fg = self.state.settings.editor_fg;
        let editor_bg = self.state.settings.editor_bg;

        let window = FloatingWindow::with_style(
            WindowPosition::Top,
            MAX_WIDTH + 2,
            rows.len() + 2,
            WindowStyle::new()
                .with_border(true)
                .with_reverse_video(false)
                .with_fg(editor_fg.unwrap_or(Color::White))
                .with_bg(editor_bg.unwrap_or(Color::Black)),
        );

        let layer = self
            .render_system
            .compositor
            .get_layer_mut(LayerPriority::HOVER);
        layer.clear();
        window.render_cells(layer, &rows);
    }
}
