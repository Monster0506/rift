use super::Editor;
use crate::plugin::lua_host::{BufEntry, BufSourceUpdate, HistoryAvailability, SyncKeys};
use crate::plugin::{HighlightRange, HighlightSpec, PluginGeneration, PluginMutation};
use crate::term::TerminalBackend;
use std::hash::{Hash, Hasher};

struct LuaBufState {
    buf_id: usize,
    buf_kind: String,
    source: BufSourceUpdate,
    cursor: (usize, usize),
    filetype: Option<String>,
    file_path: Option<String>,
    history: HistoryAvailability,
    is_dirty: bool,
    line_ending: &'static str,
}

#[derive(Hash)]
struct LuaBufView<'a> {
    id: u64,
    name: std::borrow::Cow<'a, str>,
    is_dirty: bool,
    is_current: bool,
    kind: &'a str,
    path: Option<&'a std::path::Path>,
    line_count: usize,
    is_read_only: bool,
}

impl From<LuaBufView<'_>> for BufEntry {
    fn from(view: LuaBufView<'_>) -> Self {
        Self {
            id: usize::try_from(view.id).unwrap_or(usize::MAX),
            name: view.name.into_owned(),
            is_dirty: view.is_dirty,
            is_current: view.is_current,
            kind: view.kind.to_string(),
            path: view.path.map(|p| p.to_string_lossy().into_owned()),
            line_count: view.line_count,
            is_read_only: view.is_read_only,
        }
    }
}

fn fingerprint<I: Hash>(items: impl IntoIterator<Item = I>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for item in items {
        item.hash(&mut hasher);
    }
    hasher.finish()
}

const PLUGIN_MUTATION_ROUNDS: usize = 16;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn call_plugins<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.update_lua_state();
        let result = f(self);
        self.apply_plugin_mutations();
        result
    }

    pub(super) fn update_lua_state(&self) {
        if !self.plugin_host.lua_state_wanted() {
            return;
        }
        crate::perf_span!("lua_state_sync", crate::perf::PerfFields::default());
        self.plugin_host.resync_diverged_lua_state();

        let synced = self.plugin_host.lua_synced_keys();
        let keys = self.lua_sync_keys();
        let mode = self.current_mode.as_str();
        let LuaBufState {
            buf_id,
            buf_kind,
            source,
            cursor,
            filetype,
            file_path,
            history,
            is_dirty,
            line_ending,
        } = self.lua_buf_state();

        let window_size = (
            u16::try_from(self.render_system.compositor.rows()).unwrap_or(u16::MAX),
            u16::try_from(self.render_system.compositor.cols()).unwrap_or(u16::MAX),
        );
        let scroll = self.render_system.viewport.get_scroll();
        let focused_win_id = self.split_tree.focused_window_id().get();
        let previous_win_id = self
            .split_tree
            .previous_window
            .map(super::super::split::window::WindowId::get);
        let buf_list = (keys.buf_list != synced.buf_list)
            .then(|| self.lua_buf_views().map(BufEntry::from).collect());
        let win_list = (keys.win_list != synced.win_list).then(|| self.lua_win_list());
        #[cfg(feature = "lsp")]
        let lsp_diagnostics =
            (keys.lsp_diagnostics != synced.lsp_diagnostics).then(|| self.lua_lsp_diagnostics());
        #[cfg(not(feature = "lsp"))]
        let lsp_diagnostics = None;
        let buffer_vars = (keys.buffer_vars != synced.buffer_vars).then(|| self.lua_buffer_vars());
        let doc_options = (keys.doc_options != synced.doc_options).then(|| {
            self.document_manager
                .active_document()
                .map_or_else(Default::default, |doc| doc.options.clone())
        });
        let user_settings =
            (keys.user_settings != synced.user_settings).then(|| self.state.settings.clone());
        #[cfg(feature = "treesitter")]
        let syntax = self.lua_syntax_snapshots();

        self.plugin_host
            .lua_update_state(crate::plugin::lua_host::StateUpdate {
                buf_id,
                buf_kind,
                source,
                cursor,
                mode,
                filetype,
                file_path,
                window_size,
                history,
                is_dirty,
                scroll,
                line_ending,
                focused_win_id,
                previous_win_id,
                keys,
                buf_list,
                win_list,
                lsp_diagnostics,
                buffer_vars,
                doc_options,
                user_settings,
                #[cfg(feature = "treesitter")]
                syntax,
            });
        self.sync_lua_annotations();
    }

    fn lua_sync_keys(&self) -> SyncKeys {
        let mut layout = std::collections::hash_map::DefaultHasher::new();
        (
            self.render_system.compositor.rows(),
            self.render_system.compositor.cols(),
        )
            .hash(&mut layout);
        self.split_tree.hash_layout(&mut layout);
        SyncKeys {
            user_settings: Some(self.state.settings.revision()),
            doc_options: Some(
                self.document_manager
                    .active_document()
                    .map_or(0, |doc| doc.options.revision()),
            ),
            buf_list: Some(fingerprint(self.lua_buf_views())),
            win_list: Some(layout.finish()),
            #[cfg(feature = "lsp")]
            lsp_diagnostics: Some(fingerprint(
                std::iter::once((self.lsp_diagnostics.revision(), None)).chain(
                    self.document_manager
                        .documents_iter()
                        .map(|doc| (doc.id.get(), doc.path())),
                ),
            )),
            #[cfg(not(feature = "lsp"))]
            lsp_diagnostics: None,
            buffer_vars: Some(fingerprint(
                self.document_manager
                    .documents_iter()
                    .map(|doc| (doc.id.get(), doc.vars.revision())),
            )),
        }
    }

    #[cfg(feature = "treesitter")]
    fn lua_syntax_snapshots(
        &self,
    ) -> Option<Vec<(u64, Option<crate::syntax::snapshot::BufferSyntax>)>> {
        let parsed = || {
            self.document_manager.documents_iter().filter_map(|doc| {
                let syntax = doc.syntax.as_ref()?;
                let root_id = syntax.tree.as_ref()?.root_node().id();
                Some((doc, syntax, root_id))
            })
        };
        if self.plugin_host.syntax_unchanged(
            parsed().map(|(doc, _, root_id)| (doc.id.get(), doc.buffer.revision, root_id)),
        ) {
            return None;
        }
        let synced: Vec<(u64, Option<crate::syntax::snapshot::BufferSyntax>)> = parsed()
            .map(|(doc, syntax, root_id)| {
                let id = doc.id.get();
                let fresh = self
                    .plugin_host
                    .syntax_needs_sync(id, doc.buffer.revision, root_id)
                    .then(|| syntax.snapshot(id, &doc.buffer))
                    .flatten();
                (id, fresh)
            })
            .collect();
        let live: Vec<u64> = synced.iter().map(|(id, _)| *id).collect();
        self.plugin_host.retain_synced_syntax(&live);
        Some(synced)
    }

    fn lua_buf_state(&self) -> LuaBufState {
        let Some(doc) = self.document_manager.active_document() else {
            self.plugin_host.clear_synced_buf();
            return LuaBufState {
                buf_id: 0,
                buf_kind: "file".to_string(),
                source: BufSourceUpdate::Cleared,
                cursor: (0, 0),
                filetype: None,
                file_path: None,
                history: HistoryAvailability::default(),
                is_dirty: false,
                line_ending: "lf",
            };
        };
        let buf_id = usize::try_from(doc.id.get()).unwrap_or(usize::MAX);
        let buf_kind = doc.kind.kind_str().to_string();
        let source = if self
            .plugin_host
            .synced_buf_matches(doc.id.get(), doc.buffer.revision)
        {
            BufSourceUpdate::Unchanged
        } else {
            self.plugin_host
                .set_synced_buf(doc.id.get(), doc.buffer.revision);
            BufSourceUpdate::Set(Box::new(crate::plugin::lua_host::BufLinesSource {
                revision: doc.buffer.revision,
                line_count: doc.buffer.get_total_lines(),
                buffer: doc.buffer.clone(),
            }))
        };
        let (row, col) = {
            let cursor = doc.buffer.cursor();
            let row = doc.buffer.line_index.get_line_at(cursor);
            let col = cursor.saturating_sub(doc.buffer.line_index.get_line_start(row));
            (row, col)
        };
        let line_ending = doc.options.line_ending.name();
        LuaBufState {
            buf_id,
            buf_kind,
            source,
            cursor: (row, col),
            filetype: doc.syntax.as_ref().map(|s| s.language_name.clone()),
            file_path: doc.path().map(|p| p.to_string_lossy().into_owned()),
            history: HistoryAvailability {
                can_undo: doc.history.can_undo(),
                can_redo: doc.history.can_redo(),
            },
            is_dirty: doc.is_dirty(),
            line_ending,
        }
    }

    fn lua_buf_views(&self) -> impl Iterator<Item = LuaBufView<'_>> {
        self.document_manager
            .tabs()
            .map(|(doc, is_current)| LuaBufView {
                id: doc.id.get(),
                name: doc.display_name(),
                is_dirty: doc.is_dirty(),
                is_current,
                kind: doc.kind.kind_str(),
                path: doc.path(),
                line_count: doc.buffer.get_total_lines(),
                is_read_only: doc.is_read_only(),
            })
    }

    fn lua_win_list(&self) -> Vec<crate::plugin::lua_host::WinEntry> {
        let total_rows = self.render_system.compositor.rows();
        let total_cols = self.render_system.compositor.cols();
        let win_layouts = self.split_tree.compute_layout(total_rows, total_cols);
        win_layouts
            .iter()
            .map(|l| {
                let buf = self.split_tree.get_window(l.window_id).map_or(0, |w| {
                    usize::try_from(w.document_id.get()).unwrap_or(usize::MAX)
                });
                crate::plugin::lua_host::WinEntry {
                    id: l.window_id.get(),
                    buf,
                    row: l.row,
                    col: l.col,
                    rows: l.rows,
                    cols: l.cols,
                }
            })
            .collect()
    }

    #[cfg(feature = "lsp")]
    fn lua_lsp_diagnostics(&self) -> crate::plugin::lua_host::LspDiagnosticsSnapshot {
        self.lsp_diagnostics
            .iter()
            .map(|(uri, diags)| {
                let doc = crate::lsp::protocol::uri_to_path(uri)
                    .and_then(|p| self.document_manager.find_open_document_id(&p))
                    .and_then(|id| self.document_manager.get_document(id));
                let encoding = self.lsp_manager.position_encoding_for_uri(uri);
                let entries = diags
                    .iter()
                    .map(|d| {
                        let line = d.range.start.line;
                        let col = doc.map_or(d.range.start.character, |doc| {
                            u32::try_from(doc.lsp_char_offset_in_line(
                                line as usize,
                                d.range.start.character,
                                encoding,
                            ))
                            .unwrap_or(u32::MAX)
                        });
                        (line, col, d.severity.unwrap_or(1), d.message.clone())
                    })
                    .collect();
                (uri.clone(), entries)
            })
            .collect()
    }

    fn lua_buffer_vars(
        &self,
    ) -> std::collections::HashMap<
        u64,
        std::collections::HashMap<String, crate::annotations::value::Value>,
    > {
        self.document_manager
            .documents_iter()
            .map(|doc| (doc.id.get(), doc.vars.clone()))
            .collect()
    }

    fn sync_lua_annotations(&self) {
        use crate::annotations::Anchor;
        use crate::plugin::lua_host::AnnotationView;
        if let Some(doc) = self.document_manager.active_document() {
            let revision = doc.annotations.revision();
            if self
                .plugin_host
                .synced_annotations_match(doc.id.get(), revision)
            {
                return;
            }
            self.plugin_host
                .set_synced_annotations(doc.id.get(), revision);
            let views = doc
                .annotations
                .iter()
                .map(|a| {
                    let (anchor, start, end) = match a.anchor {
                        Anchor::Point(p) => ("point", p.offset, p.offset),
                        Anchor::Range(s, e) => ("range", s.offset, e.offset),
                        Anchor::Line(l) => ("line", l, l),
                    };
                    AnnotationView {
                        id: a.id.get(),
                        kind: a.kind.as_arc(),
                        owner: a.owner.as_str(),
                        anchor,
                        start,
                        end,
                        payload: a.payload.clone(),
                        visible: a.visible,
                        interactive: a.is_interactive(),
                    }
                })
                .collect();
            let next_id = doc.annotations.peek_next_id();
            self.plugin_host.lua_set_annotations(views, next_id.get());
        } else {
            self.plugin_host.clear_synced_annotations();
            self.plugin_host.lua_set_annotations(Vec::new(), 1);
        }
    }

    pub(super) fn apply_plugin_mutations(&mut self) {
        let outermost = self.plugin_mutation_rounds.is_none();
        while self.plugin_mutation_rounds.unwrap_or(0) < PLUGIN_MUTATION_ROUNDS {
            let envelopes: Vec<crate::plugin::PluginMutationEnvelope> =
                self.plugin_host.drain_mutation_envelopes().collect();
            if envelopes.is_empty() {
                break;
            }
            self.plugin_mutation_rounds = Some(self.plugin_mutation_rounds.unwrap_or(0) + 1);
            for envelope in envelopes {
                let origin = envelope.origin;
                if let Some(gen) = origin {
                    if !self.plugin_host.is_generation_active(gen) {
                        continue;
                    }
                }
                self.apply_plugin_mutation(envelope.mutation, origin);
            }
        }
        if outermost
            && self.plugin_mutation_rounds.take() == Some(PLUGIN_MUTATION_ROUNDS)
            && self.plugin_host.has_queued_mutations()
        {
            self.state.notify(
                crate::notification::NotificationType::Error,
                format!(
                    "plugin mutations kept re-queuing; stopped after {PLUGIN_MUTATION_ROUNDS} rounds"
                ),
            );
        }
    }

    // Exhaustive on purpose: a new PluginMutation variant must be routed here.
    fn apply_plugin_mutation(
        &mut self,
        mutation: PluginMutation,
        origin: Option<PluginGeneration>,
    ) {
        match mutation {
            PluginMutation::Notify { .. }
            | PluginMutation::OpenFloat(_)
            | PluginMutation::CloseFloat
            | PluginMutation::SetCursorHoldDelay(_)
            | PluginMutation::ExecAction(_)
            | PluginMutation::SetOption { .. } => self.apply_ui_mutation(mutation),
            PluginMutation::AppendLines { .. }
            | PluginMutation::InsertAtCursor { .. }
            | PluginMutation::DeleteBefore { .. }
            | PluginMutation::DeleteForward { .. }
            | PluginMutation::ReplaceLines { .. } => self.apply_text_mutation(mutation),
            PluginMutation::SetCursor { .. }
            | PluginMutation::CenterOnLine { .. }
            | PluginMutation::SetScroll(..)
            | PluginMutation::SetLineEnding { .. }
            | PluginMutation::SetBufferVar { .. } => self.apply_cursor_mutation(mutation),
            PluginMutation::MoveWindow { .. }
            | PluginMutation::SwapWindows
            | PluginMutation::FocusPreviousWindow => self.apply_window_mutation(&mutation),
            PluginMutation::SaveBuffer { .. }
            | PluginMutation::SwitchToBuffer(_)
            | PluginMutation::OpenFile { .. }
            | PluginMutation::CloseBuffer { .. }
            | PluginMutation::ReloadBuffer { .. }
            | PluginMutation::CreateScratchBuf { .. }
            | PluginMutation::CreateBuffer { .. }
            | PluginMutation::RegisterBufferKind { .. } => {
                self.apply_buffer_mutation(mutation, origin);
            }
            PluginMutation::MapKey { .. }
            | PluginMutation::UnmapKey { .. }
            | PluginMutation::MapKeyKind { .. }
            | PluginMutation::UnmapKeyKind { .. } => {
                self.apply_keymap_mutation(mutation, origin);
            }
            PluginMutation::RegisterFiletype { .. } | PluginMutation::RegisterGrammar { .. } => {
                self.apply_language_mutation(mutation);
            }
            #[cfg(feature = "treesitter")]
            PluginMutation::SetSyntaxQuery { .. }
            | PluginMutation::StartSyntax { .. }
            | PluginMutation::StopSyntax { .. } => self.apply_language_mutation(mutation),
            PluginMutation::LspGotoDefinition
            | PluginMutation::LspReferences
            | PluginMutation::LspHover
            | PluginMutation::LspRename { .. }
            | PluginMutation::LspRenameDialog
            | PluginMutation::LspFormat
            | PluginMutation::LspCodeAction
            | PluginMutation::LspDiagnosticNext
            | PluginMutation::LspDiagnosticPrev
            | PluginMutation::LspDiagnosticsPanel => self.apply_lsp_mutation(mutation),
            #[cfg(feature = "lsp")]
            PluginMutation::LspRegisterServer { .. } => self.apply_lsp_mutation(mutation),
            PluginMutation::AddAnnotation { .. }
            | PluginMutation::UpdateAnnotation { .. }
            | PluginMutation::RemoveAnnotation { .. }
            | PluginMutation::ClearAnnotations { .. } => {
                self.apply_annotation_mutation(mutation);
            }
            PluginMutation::RegisterAnnotationAction { .. }
            | PluginMutation::RegisterKindDefaults { .. }
            | PluginMutation::UnloadPlugin { .. } => {
                self.apply_registry_mutation(mutation);
            }
            PluginMutation::SetHighlightGroup { .. }
            | PluginMutation::ResetHighlightGroups
            | PluginMutation::AddNsHighlight { .. }
            | PluginMutation::ClearNsHighlights { .. } => {
                self.apply_highlight_mutation(mutation);
            }
            PluginMutation::StartJob { .. }
            | PluginMutation::JobInput { .. }
            | PluginMutation::JobCloseStdin(_)
            | PluginMutation::StopJob(_) => self.apply_job_mutation(mutation, origin),
        }
    }

    // Reached only if apply_plugin_mutation routes a variant to the wrong group.
    pub(super) fn report_misrouted(&mut self) {
        self.state.notify(
            crate::notification::NotificationType::Error,
            "internal error: plugin mutation misrouted".to_string(),
        );
    }

    fn drop_plugin_write(&mut self, buf: crate::document::DocumentId, write: &str) {
        let reason = if self.document_manager.get_document(buf).is_some() {
            "is no longer active"
        } else {
            "is closed"
        };
        self.state.error_manager.notifications_mut().log(
            crate::notification::NotificationType::Info,
            format!("plugin {write} skipped: buffer {} {reason}", buf.get()),
        );
    }

    fn plugin_target_is_active(&mut self, buf: crate::document::DocumentId, write: &str) -> bool {
        if self.document_manager.active_document_id() == Some(buf) {
            return true;
        }
        self.drop_plugin_write(buf, write);
        false
    }

    fn apply_ui_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            PluginMutation::Notify { message, level } => {
                self.state.notify(level, message);
            }
            PluginMutation::OpenFloat(_) | PluginMutation::CloseFloat => {
                self.plugin_host.apply_mutation(mutation);
            }
            PluginMutation::SetCursorHoldDelay(ms) => {
                let poll_ms =
                    u32::try_from(self.state.settings.poll_timeout_ms).unwrap_or(u32::MAX);
                self.plugin_host.set_cursor_hold_delay_ms(ms, poll_ms);
            }
            PluginMutation::ExecAction(action_str) => {
                if let Ok(action) = action_str.parse::<crate::action::Action>() {
                    self.handle_action(&action);
                }
            }
            PluginMutation::SetOption { buf, name, value } => {
                let mut errors = Vec::new();
                let mut error_handler = |e: crate::error::RiftError| errors.push(e);
                match buf {
                    Some(buf) => {
                        let Some(doc) = self.document_manager.get_document_mut(buf) else {
                            self.drop_plugin_write(buf, "set_option");
                            return;
                        };
                        let resolver =
                            crate::command_line::settings::resolver::SettingsResolver::new(
                                self.document_settings_registry,
                                self.settings_registry,
                            );
                        resolver.set(
                            &name,
                            Some(value.as_str()),
                            &mut doc.options,
                            &mut self.state.settings,
                            &mut error_handler,
                        );
                    }
                    None => {
                        self.settings_registry.execute_setting(
                            &name,
                            Some(value.as_str()),
                            &mut self.state.settings,
                            &mut error_handler,
                        );
                    }
                }
                for err in errors {
                    self.state.handle_error(err);
                }
            }
            _ => self.report_misrouted(),
        }
    }

    fn apply_text_mutation(&mut self, mutation: PluginMutation) {
        let buf = match &mutation {
            PluginMutation::AppendLines { buf, .. }
            | PluginMutation::InsertAtCursor { buf, .. }
            | PluginMutation::DeleteBefore { buf, .. }
            | PluginMutation::DeleteForward { buf, .. }
            | PluginMutation::ReplaceLines { buf, .. } => *buf,
            _ => {
                self.report_misrouted();
                return;
            }
        };
        let Some(doc) = self.document_manager.get_document_mut(buf) else {
            self.drop_plugin_write(buf, "text edit");
            return;
        };
        match mutation {
            PluginMutation::AppendLines { lines, .. } => {
                let end = doc.buffer.len();
                let _ = doc.buffer.set_cursor(end);
                let _ = doc.insert_str(&lines.join("\n"));
            }
            PluginMutation::InsertAtCursor { text, .. } => {
                let _ = doc.insert_str(&text);
            }
            PluginMutation::DeleteBefore { count, .. } => {
                for _ in 0..count {
                    doc.delete_backward();
                }
            }
            PluginMutation::DeleteForward { count, .. } => {
                for _ in 0..count {
                    doc.delete_forward();
                }
            }
            PluginMutation::ReplaceLines {
                start, end, lines, ..
            } => {
                if let Some(edit) = doc.buffer.line_replacement(
                    start.saturating_sub(1),
                    end.saturating_sub(1),
                    &lines,
                ) {
                    let _ = doc.delete_range(edit.start, edit.end);
                    let _ = doc.buffer.set_cursor(edit.start);
                    if !edit.text.is_empty() {
                        let _ = doc.insert_str(&edit.text);
                    }
                    let _ = doc.buffer.set_cursor(edit.cursor.min(doc.buffer.len()));
                }
            }
            _ => {}
        }
        self.do_incremental_syntax_parse_for(buf);
    }

    fn apply_cursor_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            PluginMutation::SetCursor { buf, row, col } => {
                let Some(doc) = self.document_manager.get_document_mut(buf) else {
                    self.drop_plugin_write(buf, "set_cursor");
                    return;
                };
                let pos = doc.buffer.line_col_position(row.saturating_sub(1), col);
                let _ = doc.buffer.set_cursor(pos);
            }
            PluginMutation::CenterOnLine { buf, row } => {
                let active = self.document_manager.active_document_id() == Some(buf);
                let Some(doc) = self.document_manager.get_document_mut(buf) else {
                    self.drop_plugin_write(buf, "center_on_line");
                    return;
                };
                let total = doc.buffer.get_total_lines();
                let row0 = row.saturating_sub(1).min(total.saturating_sub(1));
                let _ = doc.buffer.set_cursor(doc.buffer.line_col_position(row0, 0));
                if active {
                    self.render_system.viewport.center_on(row0, total);
                }
            }
            PluginMutation::SetScroll(top, left) => {
                self.render_system.viewport.set_scroll(top, left);
            }
            PluginMutation::SetLineEnding { buf, ending } => {
                let Some(doc) = self.document_manager.get_document_mut(buf) else {
                    self.drop_plugin_write(buf, "set_line_ending");
                    return;
                };
                doc.options.line_ending = crate::document::LineEnding::from_name(&ending);
            }
            PluginMutation::SetBufferVar { buf_id, key, value } => {
                let doc_id = buf_id.or_else(|| self.document_manager.active_document_id());
                if let Some(document) =
                    doc_id.and_then(|id| self.document_manager.get_document_mut(id))
                {
                    document.vars.insert(key, value);
                }
            }
            _ => self.report_misrouted(),
        }
    }

    fn apply_window_mutation(&mut self, mutation: &PluginMutation) {
        match *mutation {
            PluginMutation::MoveWindow { direction } => {
                let size = self
                    .term
                    .get_size()
                    .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
                let layouts = self
                    .split_tree
                    .compute_layout(size.rows as usize, size.cols as usize);
                let moved_id = self.split_tree.focused_window_id();
                if self.split_tree.move_window(direction, &layouts) {
                    self.render_system.viewport.mark_needs_full_redraw();
                    self.call_plugins(|editor| {
                        editor.plugin_host.dispatch(
                            &crate::plugin::events::EditorEvent::WinMoved {
                                win: moved_id.get(),
                            },
                        );
                    });
                }
            }
            PluginMutation::SwapWindows => {
                let focused_id = self.split_tree.focused_window_id();
                if let Some(prev_id) = self.split_tree.previous_window {
                    if self.split_tree.exchange_windows(focused_id, prev_id) {
                        let new_doc = self.split_tree.focused_window().document_id;
                        let new_cursor = self.split_tree.focused_window().cursor_position;
                        let _ = self.document_manager.switch_to_document(new_doc);
                        if let Some(doc) = self.document_manager.get_document_mut(new_doc) {
                            let _ = doc.buffer.set_cursor(new_cursor);
                        }
                        self.sync_state_with_active_document();
                        self.render_system.viewport.mark_needs_full_redraw();
                        self.call_plugins(|editor| {
                            editor.plugin_host.dispatch(
                                &crate::plugin::events::EditorEvent::WinSwapped {
                                    win1: focused_id.get(),
                                    win2: prev_id.get(),
                                },
                            );
                        });
                    }
                }
            }
            PluginMutation::FocusPreviousWindow => {
                if let Some(target_id) = self.split_tree.previous_window {
                    self.switch_focus(target_id);
                }
            }
            _ => self.report_misrouted(),
        }
    }

    fn apply_buffer_mutation(
        &mut self,
        mutation: PluginMutation,
        origin: Option<PluginGeneration>,
    ) {
        match mutation {
            PluginMutation::SaveBuffer { buf } => {
                if self.plugin_target_is_active(buf, "save") {
                    self.do_save();
                }
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
                            return;
                        }
                    }
                }
                let _ = self.open_file(Some(path), force);
            }
            PluginMutation::CloseBuffer { buf, force } => {
                if self.plugin_target_is_active(buf, "close_buf") {
                    self.do_quit(force);
                }
            }
            PluginMutation::CreateScratchBuf { name, lines } => {
                if let Err(e) = self.create_scratch_buffer(name, &lines) {
                    self.state.notify(
                        crate::notification::NotificationType::Error,
                        format!("create_scratch_buf failed: {}", e.message),
                    );
                }
            }
            PluginMutation::ReloadBuffer { buf, force } => {
                if self.plugin_target_is_active(buf, "reload_buf") {
                    let _ = self.open_file(None, force);
                }
            }
            PluginMutation::CreateBuffer {
                kind,
                title,
                lines,
                vars,
            } => {
                self.create_buffer(&kind, title.as_deref(), &lines, vars);
            }
            PluginMutation::RegisterBufferKind {
                name,
                read_only,
                close,
                key_fallback,
                display_name,
                help_lines,
                has_on_close,
                has_on_action,
                has_on_save,
            } => {
                self.register_buffer_kind(
                    BufferKindSpec {
                        name,
                        read_only,
                        close,
                        key_fallback,
                        display_name,
                        help_lines,
                        has_on_close,
                        has_on_action,
                        has_on_save,
                    },
                    origin,
                );
            }
            _ => self.report_misrouted(),
        }
    }

    fn create_buffer(
        &mut self,
        kind: &str,
        title: Option<&str>,
        lines: &[String],
        vars: Vec<(String, crate::annotations::value::Value)>,
    ) {
        let Some(descriptor) = self.buffer_kinds.get_by_name(kind) else {
            self.state.notify(
                crate::notification::NotificationType::Error,
                format!("unknown buffer kind '{kind}'"),
            );
            return;
        };
        let reservation = self.document_manager.reserve_creation();
        let mut document = match crate::document::Document::new(reservation.id()) {
            Ok(document) => document,
            Err(error) => {
                self.state.handle_error(error);
                return;
            }
        };
        document.set_handle(reservation.handle());
        document.kind = crate::document::kind::BufferKind::new(descriptor.clone());
        *document.vars = vars.into_iter().collect();
        if let Some(title_str) = title {
            document.vars.insert(
                "title".to_string(),
                crate::annotations::value::Value::Str(title_str.to_string()),
            );
        }
        document.state = crate::document::runtime::StateSlot::new(
            crate::document::runtime::PLUGIN_BUFFER_STATE_KEY,
            crate::document::runtime::PluginBufferState,
        );
        let text = lines.join("\n");
        if !text.is_empty() {
            let _ = document.buffer.insert_str(&text);
        }
        let draft = match crate::document::manager::DocumentDraft::new(reservation, document) {
            Ok(draft) => draft,
            Err(error) => {
                self.state.handle_error(error);
                return;
            }
        };
        let handle = self.document_manager.commit_draft_active(draft);
        self.buffer_kinds.increment_open_count(descriptor.id());
        self.split_tree.set_focused_document(handle.doc_id());
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        self.call_plugins(|editor| {
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::BufOpen {
                    buf: handle.doc_id(),
                    path: None,
                    filetype: None,
                });
            editor
                .plugin_host
                .dispatch(&crate::plugin::events::EditorEvent::BufEnter {
                    buf: handle.doc_id(),
                });
        });
    }

    fn register_buffer_kind(&mut self, spec: BufferKindSpec, origin: Option<PluginGeneration>) {
        let BufferKindSpec {
            name,
            read_only,
            close,
            key_fallback,
            display_name,
            help_lines,
            has_on_close,
            has_on_action,
            has_on_save,
        } = spec;
        if !has_on_close {
            self.state.notify(
                crate::notification::NotificationType::Error,
                format!("buffer kind '{name}' requires on_close"),
            );
            return;
        }
        let Some(owner) = self.buffer_kind_owner(&name, origin) else {
            return;
        };
        let read_only_policy = match parse_read_only_policy(read_only.as_deref()) {
            Ok(policy) => policy,
            Err(invalid) => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("buffer kind '{name}' has invalid read_only policy: '{invalid}'"),
                );
                return;
            }
        };
        let close_policy = match parse_close_policy(close.as_deref()) {
            Ok(policy) => policy,
            Err(invalid) => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("buffer kind '{name}' has invalid close policy: '{invalid}'"),
                );
                return;
            }
        };
        let key_fallback_policy = match parse_key_fallback_policy(key_fallback.as_deref()) {
            Ok(policy) => policy,
            Err(invalid) => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("buffer kind '{name}' has invalid key_fallback policy: '{invalid}'"),
                );
                return;
            }
        };
        let result = self.buffer_kinds.register_runtime(&name, |id| {
            let mut policies = crate::document::runtime::BufferPolicies::file_default();
            policies.read_only = read_only_policy;
            policies.close = close_policy;
            policies.language_services = crate::document::runtime::LanguageServicesPolicy::Virtual;
            policies.key_fallback = key_fallback_policy;
            crate::document::runtime::KindDescriptor {
                id,
                name: name.clone().into_boxed_str(),
                policies,
                action_dispatch: if has_on_action {
                    crate::document::runtime::ActionDispatch::Lua
                } else {
                    crate::document::runtime::ActionDispatch::Disabled
                },
                save_dispatch: if has_on_save {
                    crate::document::runtime::SaveDispatch::Lua
                } else {
                    crate::document::runtime::SaveDispatch::Disabled
                },
                reload_dispatch: crate::document::runtime::ReloadDispatch::Unsupported,
                display_name: display_name.clone().map_or(
                    crate::document::runtime::DisplayNameStrategy::KindName,
                    |label| {
                        crate::document::runtime::DisplayNameStrategy::Label(label.into_boxed_str())
                    },
                ),
                help_lines: help_lines.clone().map(Into::into),
                on_close: crate::document::runtime::CloseHandler::Lua,
                state_key: crate::document::runtime::PLUGIN_BUFFER_STATE_KEY.id(),
                owner,
                tombstone: None,
            }
        });
        if let Err(error) = result {
            self.state.notify(
                crate::notification::NotificationType::Error,
                error.to_string(),
            );
        }
    }

    fn buffer_kind_owner(
        &mut self,
        name: &str,
        origin: Option<PluginGeneration>,
    ) -> Option<crate::document::runtime::DescriptorOwner> {
        match origin {
            Some(gen) if self.plugin_host.is_generation_active(gen) => {
                Some(crate::document::runtime::DescriptorOwner::plugin(
                    gen.plugin_id().as_nonzero(),
                    gen.generation(),
                ))
            }
            _ => {
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("cannot register buffer kind '{name}': no active plugin generation"),
                );
                None
            }
        }
    }

    fn apply_keymap_mutation(
        &mut self,
        mutation: PluginMutation,
        origin: Option<PluginGeneration>,
    ) {
        match mutation {
            PluginMutation::MapKey { mode, keys, action } => {
                use crate::keymap::KeyContext;
                let ctx = match mode.as_str() {
                    "n" | "normal" => KeyContext::Normal,
                    "i" | "insert" => KeyContext::Insert,
                    "c" | "command" => KeyContext::Command,
                    "s" | "search" => KeyContext::Search,
                    "g" | "global" => KeyContext::Global,
                    "t" | "terminal" => KeyContext::Terminal,
                    "tn" | "terminal_normal" => KeyContext::TerminalNormal,
                    _ => return,
                };
                if let (Some(key_seq), Ok(act)) = (
                    crate::key::notation::parse_key_sequence(&keys),
                    action.parse::<crate::action::Action>(),
                ) {
                    if let Some(gen) = origin {
                        self.keymap.register_with_owner(
                            ctx,
                            key_seq,
                            act,
                            gen.keymap_owner_token(),
                        );
                    } else {
                        self.keymap.register_sequence(ctx, key_seq, act);
                    }
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
                    "t" | "terminal" => KeyContext::Terminal,
                    "tn" | "terminal_normal" => KeyContext::TerminalNormal,
                    _ => return,
                };
                if let Some(key_seq) = crate::key::notation::parse_key_sequence(&keys) {
                    self.keymap.unregister_sequence(ctx, &key_seq);
                }
            }
            PluginMutation::MapKeyKind { kind, keys, action } => {
                let Some(kind_id) = self.buffer_kinds.id_by_name(&kind) else {
                    return;
                };
                if let (Some(key_seq), Ok(action)) = (
                    crate::key::notation::parse_key_sequence(&keys),
                    action.parse::<crate::action::Action>(),
                ) {
                    let ctx = crate::keymap::KeyContext::Buffer(kind_id);
                    if let Some(gen) = origin {
                        self.keymap.register_with_owner(
                            ctx,
                            key_seq,
                            action,
                            gen.keymap_owner_token(),
                        );
                    } else {
                        self.keymap.register_sequence(ctx, key_seq, action);
                    }
                }
            }
            PluginMutation::UnmapKeyKind { kind, keys } => {
                let Some(kind_id) = self.buffer_kinds.id_by_name(&kind) else {
                    return;
                };
                if let Some(key_seq) = crate::key::notation::parse_key_sequence(&keys) {
                    self.keymap
                        .unregister_sequence(crate::keymap::KeyContext::Buffer(kind_id), &key_seq);
                }
            }
            _ => self.report_misrouted(),
        }
    }

    fn apply_language_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            PluginMutation::RegisterFiletype { ext, lang_name } => {
                self.language_loader.register_filetype(&ext, &lang_name);
            }
            #[cfg(feature = "treesitter")]
            PluginMutation::SetSyntaxQuery { lang, kind, source } => {
                self.language_loader.set_query_override(&lang, kind, source);
                self.rebuild_syntax_using(&lang);
            }
            #[cfg(feature = "treesitter")]
            PluginMutation::StartSyntax { buf, lang } => self.start_syntax(buf, lang.as_deref()),
            #[cfg(feature = "treesitter")]
            PluginMutation::StopSyntax { buf } => self.stop_syntax(buf),
            PluginMutation::RegisterGrammar {
                lang_name,
                so_path,
                fn_name,
            } => self.register_grammar(&lang_name, &so_path, &fn_name),
            _ => self.report_misrouted(),
        }
    }

    #[cfg(feature = "treesitter")]
    fn register_grammar(&mut self, lang_name: &str, so_path: &str, fn_name: &str) {
        if let Err(e) = self
            .language_loader
            .register_grammar(lang_name, so_path, fn_name)
        {
            self.state.notify(
                crate::notification::NotificationType::Error,
                format!("register_grammar '{lang_name}': {e}"),
            );
        }
    }

    #[cfg(not(feature = "treesitter"))]
    fn register_grammar(&mut self, lang_name: &str, so_path: &str, fn_name: &str) {
        self.state.notify(
            crate::notification::NotificationType::Error,
            format!(
                "register_grammar '{lang_name}' ({so_path}:{fn_name}): tree-sitter support is not compiled in"
            ),
        );
    }

    fn apply_lsp_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            #[cfg(feature = "lsp")]
            PluginMutation::LspRegisterServer { language, config } => {
                self.lsp_manager.register_server(language, config);
                self.lsp_notify_open_all();
            }
            PluginMutation::LspGotoDefinition => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspGotoDefinition,
                ));
            }
            PluginMutation::LspReferences => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspReferences,
                ));
            }
            PluginMutation::LspHover => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspHover,
                ));
            }
            PluginMutation::LspRename { new_name } => {
                #[cfg(feature = "lsp")]
                match self.cursor_lsp_position() {
                    Some((path, line, col)) => {
                        if self
                            .lsp_manager
                            .rename(&path, line, col, new_name)
                            .is_none()
                        {
                            self.notify_no_lsp_server();
                        }
                    }
                    None => self.state.notify(
                        crate::notification::NotificationType::Warning,
                        "LSP rename: no file open".to_string(),
                    ),
                }
                #[cfg(not(feature = "lsp"))]
                let _ = new_name;
            }
            PluginMutation::LspRenameDialog => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspRename,
                ));
            }
            PluginMutation::LspDiagnosticsPanel => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspDiagnosticsPanel,
                ));
            }
            PluginMutation::LspFormat => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspFormat,
                ));
            }
            PluginMutation::LspCodeAction => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspCodeAction,
                ));
            }
            PluginMutation::LspDiagnosticNext => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspDiagnosticNext,
                ));
            }
            PluginMutation::LspDiagnosticPrev => {
                self.handle_action(&crate::action::Action::Editor(
                    crate::action::EditorAction::LspDiagnosticPrev,
                ));
            }
            _ => self.report_misrouted(),
        }
    }

    fn apply_annotation_mutation(&mut self, mutation: PluginMutation) {
        let buf = match &mutation {
            PluginMutation::AddAnnotation { buf, .. }
            | PluginMutation::UpdateAnnotation { buf, .. }
            | PluginMutation::RemoveAnnotation { buf, .. }
            | PluginMutation::ClearAnnotations { buf, .. } => *buf,
            _ => {
                self.report_misrouted();
                return;
            }
        };
        let Some(doc) = self.document_manager.get_document_mut(buf) else {
            self.drop_plugin_write(buf, "annotation edit");
            return;
        };
        match mutation {
            PluginMutation::AddAnnotation {
                id,
                kind,
                anchor,
                payload,
                presentation,
                actions,
                visible,
                stickiness,
                owner,
                ..
            } => {
                use crate::annotations::{action::Action, kind::Kind, Annotation, Stickiness};
                let mut ann = Annotation::new(
                    Kind::new(kind),
                    annotation_anchor(&anchor),
                    crate::plugin::annotation_owner(owner.as_deref()),
                )
                .with_payload(payload)
                .with_visible(visible)
                .with_actions(
                    actions
                        .into_iter()
                        .map(|(verb, default)| {
                            let a = Action::new(verb);
                            if default {
                                a.with_default()
                            } else {
                                a
                            }
                        })
                        .collect(),
                );
                if matches!(stickiness.as_deref(), Some("persist")) {
                    ann = ann.with_stickiness(Stickiness::Persist);
                }
                if let Some(pres) = presentation {
                    ann = ann.with_presentation(pres);
                }
                doc.annotations
                    .add_with_id(crate::annotations::AnnotationId::new(id), ann);
            }
            PluginMutation::UpdateAnnotation {
                id,
                payload,
                visible,
                presentation,
                ..
            } => {
                doc.annotations
                    .update(crate::annotations::AnnotationId::new(id), |a| {
                        if let Some(p) = payload {
                            a.payload = p;
                        }
                        if let Some(v) = visible {
                            a.visible = v;
                        }
                        if let Some(pres) = presentation {
                            a.presentation = Some(pres);
                        }
                    });
            }
            PluginMutation::RemoveAnnotation { id, .. } => {
                doc.annotations
                    .remove(crate::annotations::AnnotationId::new(id));
            }
            PluginMutation::ClearAnnotations { kind_prefix, .. } => {
                doc.annotations.clear_by_kind_prefix(&kind_prefix);
            }
            _ => {}
        }
    }

    fn apply_registry_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            PluginMutation::RegisterAnnotationAction {
                kind,
                verb,
                command,
                owner,
            } => {
                use crate::annotations::registry::Handler;
                let handler = command.map_or(Handler::Lua, Handler::Command);
                self.dispatch_registry
                    .register_owned(kind, verb, handler, owner);
            }
            PluginMutation::RegisterKindDefaults {
                kind,
                presentation,
                description,
                owner,
            } => {
                self.kind_registry
                    .register_owned(kind, presentation, description, owner);
            }
            PluginMutation::UnloadPlugin { owner } => {
                self.dispatch_registry.remove_owner(&owner);
                self.kind_registry.remove_owner(&owner);
            }
            _ => self.report_misrouted(),
        }
    }

    fn apply_highlight_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            PluginMutation::SetHighlightGroup { name, style } => {
                self.state.settings.set_highlight_override(&name, style);
            }
            PluginMutation::ResetHighlightGroups => self.state.settings.clear_highlight_overrides(),
            PluginMutation::AddNsHighlight {
                buf,
                ns,
                owner,
                range,
                hl,
                priority,
            } => {
                use crate::annotations::presentation::{FaceRef, Presentation};
                let Some(doc) = self.document_manager.get_document_mut(buf) else {
                    return;
                };
                let bytes = highlight_bytes(&doc.buffer, &range);
                if bytes.is_empty() {
                    return;
                }
                let presentation = match hl {
                    HighlightSpec::Group(group) => Presentation::with_face(FaceRef::new(group)),
                    HighlightSpec::Style(style) => Presentation::with_style(style),
                }
                .with_priority(priority);
                doc.annotations.add_ns_highlight(
                    &owned_namespace(ns, owner),
                    bytes.start,
                    bytes.end,
                    presentation,
                );
            }
            PluginMutation::ClearNsHighlights { buf, ns, owner } => {
                let ns = owned_namespace(ns, owner);
                let ids: Vec<crate::document::DocumentId> = match buf {
                    Some(buf) => vec![buf],
                    None => self
                        .document_manager
                        .documents_iter()
                        .map(|d| d.id)
                        .collect(),
                };
                for id in ids {
                    if let Some(doc) = self.document_manager.get_document_mut(id) {
                        doc.annotations.clear_ns_highlights(&ns);
                    }
                }
            }
            _ => self.report_misrouted(),
        }
    }
}

fn owned_namespace(ns: String, owner: Option<String>) -> String {
    match owner {
        Some(owner) => format!("{owner}\u{0}{ns}"),
        None => ns,
    }
}

fn highlight_bytes(
    buffer: &crate::buffer::TextBuffer,
    range: &HighlightRange,
) -> std::ops::Range<usize> {
    match range {
        #[cfg(feature = "treesitter")]
        HighlightRange::Bytes(r) => {
            let len = buffer.byte_len();
            r.start.min(len)..r.end.min(len)
        }
        HighlightRange::Points { start, end } => {
            let at = |(row, col): (usize, usize)| {
                let line = buffer.line_byte_range(row);
                line.start + col.min(line.len())
            };
            at(*start)..at(*end)
        }
    }
}

const fn annotation_anchor(
    anchor: &crate::plugin::AnnotationAnchorSpec,
) -> crate::annotations::Anchor {
    use crate::annotations::Anchor;
    use crate::plugin::AnnotationAnchorSpec;
    match *anchor {
        AnnotationAnchorSpec::Line(l) => Anchor::Line(l),
        AnnotationAnchorSpec::Point(p) => Anchor::point(p),
        AnnotationAnchorSpec::Range(s, e) => Anchor::range(s, e),
    }
}

fn parse_read_only_policy(
    value: Option<&str>,
) -> Result<crate::document::runtime::ReadOnlyPolicy, &str> {
    match value {
        Some("readonly" | "read_only" | "fixed" | "fixed_readonly") => {
            Ok(crate::document::runtime::ReadOnlyPolicy::FixedReadOnly)
        }
        Some("writable" | "fixed_writable") => {
            Ok(crate::document::runtime::ReadOnlyPolicy::FixedWritable)
        }
        Some("override" | "document_override") | None => {
            Ok(crate::document::runtime::ReadOnlyPolicy::DocumentOverride)
        }
        Some(invalid) => Err(invalid),
    }
}

fn parse_close_policy(value: Option<&str>) -> Result<crate::document::runtime::ClosePolicy, &str> {
    match value {
        Some("discard" | "discard_dirty") => {
            Ok(crate::document::runtime::ClosePolicy::DiscardDirty)
        }
        Some("confirm" | "confirm_dirty") | None => {
            Ok(crate::document::runtime::ClosePolicy::ConfirmDirty)
        }
        Some(invalid) => Err(invalid),
    }
}

fn parse_key_fallback_policy(
    value: Option<&str>,
) -> Result<crate::document::runtime::KeyFallback, &str> {
    match value {
        Some("global") => Ok(crate::document::runtime::KeyFallback::Global),
        Some("none") => Ok(crate::document::runtime::KeyFallback::None),
        Some("normal") | None => Ok(crate::document::runtime::KeyFallback::Normal),
        Some(invalid) => Err(invalid),
    }
}

// Grouped so register_buffer_kind stays under the argument-count lint.
struct BufferKindSpec {
    name: String,
    read_only: Option<String>,
    close: Option<String>,
    key_fallback: Option<String>,
    display_name: Option<String>,
    help_lines: Option<Vec<String>>,
    has_on_close: bool,
    has_on_action: bool,
    has_on_save: bool,
}
