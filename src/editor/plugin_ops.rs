use super::Editor;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn update_lua_state(&self) {
        use crate::plugin::lua_host::BufEntry;

        let tab_width = self.state.settings.tab_width;
        let expand_tabs = self.state.settings.expand_tabs;
        let mode = self.current_mode.as_str();

        let (
            buf_id,
            buf_kind,
            lines,
            cursor,
            filetype,
            file_path,
            can_undo,
            can_redo,
            is_dirty,
            line_ending,
        ) = if let Some(doc) = self.document_manager.active_document() {
            let buf_id = doc.id as usize;
            let buf_kind = doc.kind.kind_str().to_string();
            let text = doc.buffer.to_string();
            let lines: Vec<String> = text
                .split('\n')
                .map(|l| l.trim_end_matches('\r').to_string())
                .collect();
            let (row, col) = {
                let cursor = doc.buffer.cursor();
                let row = doc.buffer.line_index.get_line_at(cursor);
                let col = cursor.saturating_sub(doc.buffer.line_index.get_line_start(row));
                (row, col)
            };
            let filetype = doc.syntax.as_ref().map(|s| s.language_name.clone());
            let file_path = doc.path().map(|p| p.to_string_lossy().into_owned());
            let can_undo = doc.history.can_undo();
            let can_redo = doc.history.can_redo();
            let is_dirty = doc.is_dirty();
            let line_ending = match doc.options.line_ending {
                crate::document::LineEnding::LF => "lf",
                crate::document::LineEnding::CRLF => "crlf",
            };
            (
                buf_id,
                buf_kind,
                lines,
                (row, col),
                filetype,
                file_path,
                can_undo,
                can_redo,
                is_dirty,
                line_ending,
            )
        } else {
            (
                0,
                "file".to_string(),
                vec![],
                (0, 0),
                None,
                None,
                false,
                false,
                false,
                "lf",
            )
        };

        let buf_list: Vec<BufEntry> = self
            .document_manager
            .get_buffer_list()
            .into_iter()
            .filter_map(|b| {
                let doc = self.document_manager.get_document(b.id)?;
                Some(BufEntry {
                    id: b.id as usize,
                    name: b.name,
                    is_dirty: b.is_dirty,
                    is_current: b.is_current,
                    kind: doc.kind.kind_str().to_string(),
                    path: doc.path().map(|p| p.to_string_lossy().into_owned()),
                    line_count: doc.buffer.get_total_lines(),
                    is_read_only: b.is_read_only,
                })
            })
            .collect();

        let window_size = (
            self.render_system.compositor.rows() as u16,
            self.render_system.compositor.cols() as u16,
        );

        let scroll = self.render_system.viewport.get_scroll();

        let commands: Vec<(String, String)> = self
            .plugin_host
            .command_list()
            .into_iter()
            .map(|(name, desc, _)| (name, desc))
            .collect();

        self.plugin_host.lua_update_state(
            buf_id,
            buf_kind,
            lines,
            cursor,
            tab_width,
            expand_tabs,
            mode,
            filetype,
            file_path,
            buf_list,
            window_size,
            can_undo,
            can_redo,
            is_dirty,
            scroll,
            line_ending,
            commands,
        );
    }

    pub(super) fn adjust_plugin_highlights_for_edits(&mut self) {
        use crate::buffer::ByteEdit;

        fn adjust(range: std::ops::Range<usize>, e: &ByteEdit) -> Option<std::ops::Range<usize>> {
            let s = range.start;
            let end = range.end;
            let edit_end = e.byte_pos + e.del_bytes;
            let delta = e.ins_bytes as isize - e.del_bytes as isize;

            if end <= e.byte_pos {
                Some(s..end)
            } else if s >= edit_end {
                Some(((s as isize + delta) as usize)..((end as isize + delta) as usize))
            } else if s >= e.byte_pos && end <= edit_end {
                None
            } else if s < e.byte_pos && end > edit_end {
                Some(s..((end as isize + delta) as usize))
            } else if s >= e.byte_pos && end > edit_end {
                let ns = e.byte_pos + e.ins_bytes;
                let ne = (end as isize + delta) as usize;
                if ns < ne {
                    Some(ns..ne)
                } else {
                    None
                }
            } else if s < e.byte_pos {
                Some(s..e.byte_pos)
            } else {
                None
            }
        }

        let edits: Vec<ByteEdit> = if let Some(doc) = self.document_manager.active_document_mut() {
            doc.buffer.edit_log.drain(..).collect()
        } else {
            return;
        };

        if edits.is_empty() {
            return;
        }

        if let Some(doc) = self.document_manager.active_document_mut() {
            for slot in doc.highlight_slots.values_mut() {
                let mut new_slot: Vec<(std::ops::Range<usize>, crate::color::Color)> =
                    Vec::with_capacity(slot.len());
                for (range, color) in slot.drain(..) {
                    let mut cur = range;
                    let mut keep = true;
                    for e in &edits {
                        match adjust(cur.clone(), e) {
                            Some(r) => cur = r,
                            None => {
                                keep = false;
                                break;
                            }
                        }
                    }
                    if keep {
                        new_slot.push((cur, color));
                    }
                }
                *slot = new_slot;
            }
            let mut merged: Vec<(std::ops::Range<usize>, crate::color::Color)> =
                doc.highlight_slots.values().flatten().cloned().collect();
            merged.sort_by_key(|(r, _)| r.start);
            doc.plugin_highlights = merged;
        }
    }

    /// Drain the plugin mutation queue and apply each mutation.
    pub(super) fn apply_plugin_mutations(&mut self) {
        use crate::color::Color;
        use crate::plugin::PluginMutation;

        /// Parse a color name or "#rrggbb" hex string into a `Color`.
        fn plugin_color(s: &str) -> Color {
            match s.to_lowercase().as_str() {
                "red" => Color::Red,
                "darkred" => Color::DarkRed,
                "green" => Color::Green,
                "darkgreen" => Color::DarkGreen,
                "blue" => Color::Blue,
                "darkblue" => Color::DarkBlue,
                "yellow" => Color::Yellow,
                "darkyellow" => Color::DarkYellow,
                "cyan" => Color::Cyan,
                "darkcyan" => Color::DarkCyan,
                "magenta" => Color::Magenta,
                "darkmagenta" => Color::DarkMagenta,
                "white" => Color::White,
                "black" => Color::Black,
                "grey" | "gray" => Color::Grey,
                "darkgrey" | "darkgray" => Color::DarkGrey,
                s if s.starts_with('#') && s.len() == 7 => {
                    let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(255);
                    let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(255);
                    let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(255);
                    Color::Rgb { r, g, b }
                }
                _ => Color::Yellow,
            }
        }

        // Drain into a Vec first so we don't hold a borrow on plugin_host.
        let mutations: Vec<PluginMutation> = self.plugin_host.drain_mutations().collect();

        let mut needs_highlight_merge = false;
        for mutation in mutations {
            match mutation {
                PluginMutation::Notify { message, level } => {
                    self.state.notify(level, message);
                }
                PluginMutation::AppendLines(ref lines) => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        let end = doc.buffer.len();
                        let _ = doc.buffer.set_cursor(end);
                        let text = lines.join("\n");
                        let _ = doc.insert_str(&text);
                    }
                    self.do_incremental_syntax_parse();
                }
                PluginMutation::InsertAtCursor(text) => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        let _ = doc.insert_str(&text);
                    }
                    self.do_incremental_syntax_parse();
                }
                PluginMutation::DeleteBefore(n) => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        for _ in 0..n {
                            doc.delete_backward();
                        }
                    }
                    self.do_incremental_syntax_parse();
                }
                PluginMutation::DeleteForward(n) => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        for _ in 0..n {
                            doc.delete_forward();
                        }
                    }
                    self.do_incremental_syntax_parse();
                }
                PluginMutation::SetCursor { row, col } => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        let row0 = row.saturating_sub(1);
                        let line_count = doc.buffer.line_index.line_count();
                        let row0 = row0.min(line_count.saturating_sub(1));
                        let line_start = doc.buffer.line_index.get_line_start(row0);
                        let total = doc.buffer.len();
                        let pos = (line_start + col).min(total.saturating_sub(1));
                        let _ = doc.buffer.set_cursor(pos);
                    }
                }
                PluginMutation::ReplaceLines { start, end, lines } => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        let start0 = start.saturating_sub(1);
                        let end0 = end.saturating_sub(1);
                        let line_count = doc.buffer.line_index.line_count();
                        if start0 < line_count {
                            let range_start = doc.buffer.line_index.get_line_start(start0);
                            let range_end = if end0 + 1 < line_count {
                                doc.buffer.line_index.get_line_start(end0 + 1)
                            } else {
                                doc.buffer.len()
                            };
                            let _ = doc.delete_range(range_start, range_end);
                            let _ = doc.buffer.set_cursor(range_start);
                            if !lines.is_empty() {
                                let _ = doc.insert_str(&lines.join("\n"));
                            }
                        }
                    }
                    self.do_incremental_syntax_parse();
                }
                PluginMutation::AddHighlight {
                    slot,
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                    color,
                } => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        let parsed_color = plugin_color(&color);
                        let sl = start_line.saturating_sub(1);
                        let el = end_line.saturating_sub(1);
                        let line_count = doc.buffer.line_index.line_count();
                        if sl < line_count {
                            let el = el.min(line_count.saturating_sub(1));
                            let start_char = doc.buffer.line_index.get_line_start(sl) + start_col;
                            let end_char = (doc.buffer.line_index.get_line_start(el) + end_col)
                                .min(doc.buffer.len());
                            let start = doc.buffer.char_to_byte(start_char);
                            let end = doc.buffer.char_to_byte(end_char);
                            if start < end {
                                doc.highlight_slots
                                    .entry(slot)
                                    .or_default()
                                    .push((start..end, parsed_color));
                            }
                        }
                    }
                    needs_highlight_merge = true;
                }
                PluginMutation::ClearHighlights { slot } => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        if slot == 0 {
                            doc.highlight_slots.clear();
                        } else if let Some(s) = doc.highlight_slots.get_mut(&slot) {
                            s.clear();
                        }
                    }
                    needs_highlight_merge = true;
                }
                PluginMutation::SetOption { name, value } => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        match name.as_str() {
                            "tab_width" | "tabwidth" => {
                                if let Ok(n) = value.parse::<usize>() {
                                    if n > 0 {
                                        doc.options.tab_width = n;
                                    }
                                }
                            }
                            "expand_tabs" | "expandtabs" => {
                                doc.options.expand_tabs =
                                    matches!(value.as_str(), "true" | "1" | "yes");
                            }
                            "show_line_numbers" | "number" => {
                                doc.options.show_line_numbers =
                                    matches!(value.as_str(), "true" | "1" | "yes");
                            }
                            _ => {}
                        }
                    }
                }
                PluginMutation::SaveBuffer => {
                    self.do_save();
                }
                PluginMutation::OpenFloat(_) | PluginMutation::CloseFloat => {
                    self.plugin_host.apply_mutation(mutation);
                }
                PluginMutation::SetScroll(top, left) => {
                    self.render_system.viewport.set_scroll(top, left);
                }
                PluginMutation::SetLineEnding(ending) => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.options.line_ending = match ending.to_lowercase().as_str() {
                            "crlf" => crate::document::LineEnding::CRLF,
                            _ => crate::document::LineEnding::LF,
                        };
                    }
                }
                PluginMutation::ExecAction(action_str) => {
                    if let Ok(action) = action_str.parse::<crate::action::Action>() {
                        self.handle_action(&action);
                    }
                }
                PluginMutation::MapKey { mode, keys, action } => {
                    use crate::keymap::KeyContext;
                    let ctx = match mode.as_str() {
                        "n" | "normal" => KeyContext::Normal,
                        "i" | "insert" => KeyContext::Insert,
                        "c" | "command" => KeyContext::Command,
                        "s" | "search" => KeyContext::Search,
                        "g" | "global" => KeyContext::Global,
                        _ => continue,
                    };
                    if let (Some(key_seq), Ok(act)) = (
                        crate::key::parse_key_sequence(&keys),
                        action.parse::<crate::action::Action>(),
                    ) {
                        self.keymap.register_sequence(ctx, key_seq, act);
                    }
                }
                PluginMutation::CenterOnLine(row) => {
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        let row0 = row.saturating_sub(1);
                        let total = doc.buffer.get_total_lines();
                        let row0 = row0.min(total.saturating_sub(1));
                        let line_start = doc.buffer.line_index.get_line_start(row0);
                        let _ = doc.buffer.set_cursor(line_start);
                        self.render_system.viewport.center_on(row0, total);
                    }
                }
                PluginMutation::UnmapKey { mode, keys } => {
                    use crate::keymap::KeyContext;
                    let ctx = match mode.as_str() {
                        "n" | "normal" => KeyContext::Normal,
                        "i" | "insert" => KeyContext::Insert,
                        "c" | "command" => KeyContext::Command,
                        "s" | "search" => KeyContext::Search,
                        "g" | "global" => KeyContext::Global,
                        _ => continue,
                    };
                    if let Some(key_seq) = crate::key::parse_key_sequence(&keys) {
                        self.keymap.unregister_sequence(ctx, &key_seq);
                    }
                }
                PluginMutation::SetCursorHoldDelay(ms) => {
                    let poll_ms = self.state.settings.poll_timeout_ms as u32;
                    self.plugin_host.set_cursor_hold_delay_ms(ms, poll_ms);
                }
                PluginMutation::SwitchToBuffer(id) => {
                    if self.document_manager.switch_to_document(id).is_ok() {
                        self.sync_state_with_active_document();
                    }
                }
                PluginMutation::OpenFile { path, force } => {
                    if !force {
                        if let Some(doc) = self.document_manager.active_document() {
                            if doc.is_dirty() {
                                self.state.notify(
                                    crate::notification::NotificationType::Warning,
                                    "unsaved changes — use force=true to discard",
                                );
                                continue;
                            }
                        }
                    }
                    let _ = self.open_file(Some(path), force);
                }
                PluginMutation::CloseBuffer { force } => {
                    self.do_quit(force);
                }
            }
        }

        if needs_highlight_merge {
            if let Some(doc) = self.document_manager.active_document_mut() {
                let mut merged: Vec<(std::ops::Range<usize>, Color)> =
                    doc.highlight_slots.values().flatten().cloned().collect();
                merged.sort_by_key(|(r, _)| r.start);
                doc.plugin_highlights = merged;
            }
        }
    }
}
