use super::Editor;
use crate::command_line::settings::resolver::OptionScope;
use crate::document::ops::{Edit, EditSource};
use crate::document::DocumentId;
use crate::plugin::lua_host::{
    BufAnnotations, BufEntry, BufMeta, BufUpdate, HistoryAvailability, SyncKeys,
};
use crate::plugin::{BufSyncKey, HighlightRange, HighlightSpec, PluginGeneration, PluginMutation};
use crate::term::TerminalBackend;
use std::hash::{Hash, Hasher};

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

fn lua_buf_cursor(doc: &crate::document::Document) -> (usize, usize) {
    let cursor = doc.buffer().cursor();
    let row = doc.buffer().line_index.get_line_at(cursor);
    let col = cursor.saturating_sub(doc.buffer().line_index.get_line_start(row));
    (row, col)
}

fn lua_buf_sync_key(doc: &crate::document::Document) -> BufSyncKey {
    BufSyncKey {
        text: doc.buffer().revision(),
        meta: fingerprint([(
            lua_buf_cursor(doc),
            doc.kind.kind_str(),
            doc.syntax.as_ref().map(|s| s.language_name.as_str()),
            doc.path().map(std::path::Path::as_os_str),
            doc.history.can_undo(),
            doc.history.can_redo(),
            doc.is_dirty(),
        )]),
        options: doc.options().revision(),
        annotations: doc.annotations.revision(),
    }
}

fn lua_buf_meta(doc: &crate::document::Document) -> BufMeta {
    BufMeta {
        kind: doc.kind.kind_str().to_string(),
        cursor: lua_buf_cursor(doc),
        filetype: doc.syntax.as_ref().map(|s| s.language_name.clone()),
        file_path: doc.path().map(|p| p.to_string_lossy().into_owned()),
        history: HistoryAvailability {
            can_undo: doc.history.can_undo(),
            can_redo: doc.history.can_redo(),
        },
        is_dirty: doc.is_dirty(),
    }
}

fn lua_annotations(doc: &crate::document::Document) -> BufAnnotations {
    use crate::annotations::Anchor;
    use crate::plugin::lua_host::AnnotationView;
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
    BufAnnotations {
        views,
        next_id: doc.annotations.peek_next_id().get(),
    }
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
        let buf_id = self
            .document_manager
            .active_document_id()
            .map_or(0, |id| usize::try_from(id.get()).unwrap_or(usize::MAX));
        let creation_ids =
            self.refill_plugin_creation_ids(self.plugin_host.lua_take_creation_ids());
        let bufs = self.lua_buf_snapshots();

        let window_size = (
            u16::try_from(self.render_system.compositor.rows()).unwrap_or(u16::MAX),
            u16::try_from(self.render_system.compositor.cols()).unwrap_or(u16::MAX),
        );
        let scroll = self.render_system.viewport.get_scroll();
        let focused_win_id = self.split_tree.focused_window_id().get();
        let previous_win_id = self
            .split_tree
            .previous_window()
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
        let user_settings =
            (keys.user_settings != synced.user_settings).then(|| self.state.settings.clone());
        #[cfg(feature = "treesitter")]
        let syntax = self.lua_syntax_snapshots();

        self.plugin_host
            .lua_update_state(crate::plugin::lua_host::StateUpdate {
                buf_id,
                bufs,
                creation_ids,
                mode,
                window_size,
                scroll,
                focused_win_id,
                previous_win_id,
                keys,
                buf_list,
                win_list,
                lsp_diagnostics,
                buffer_vars,
                user_settings,
                #[cfg(feature = "treesitter")]
                syntax,
            });
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
                    .map(|doc| (doc.id.get(), doc.vars().revision())),
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
            parsed().map(|(doc, _, root_id)| (doc.id.get(), doc.buffer().revision(), root_id)),
        ) {
            return None;
        }
        let synced: Vec<(u64, Option<crate::syntax::snapshot::BufferSyntax>)> = parsed()
            .map(|(doc, syntax, root_id)| {
                let id = doc.id.get();
                let fresh = self
                    .plugin_host
                    .syntax_needs_sync(id, doc.buffer().revision(), root_id)
                    .then(|| syntax.snapshot(id, doc.buffer()))
                    .flatten();
                (id, fresh)
            })
            .collect();
        let live: Vec<u64> = synced.iter().map(|(id, _)| *id).collect();
        self.plugin_host.retain_synced_syntax(&live);
        Some(synced)
    }

    pub(super) fn lua_buf_snapshots(&self) -> Option<Vec<(u64, BufUpdate)>> {
        let keyed = || {
            self.document_manager
                .documents_iter()
                .map(|doc| (doc, lua_buf_sync_key(doc)))
        };
        if self
            .plugin_host
            .bufs_unchanged(keyed().map(|(doc, key)| (doc.id.get(), key)))
        {
            return None;
        }
        let updates: Vec<(u64, BufUpdate)> = keyed()
            .map(|(doc, key)| {
                let id = doc.id.get();
                let previous = self.plugin_host.sync_buf(id, key);
                let changed = |part: fn(&BufSyncKey) -> u64| {
                    previous.is_none_or(|previous| part(&previous) != part(&key))
                };
                let update = BufUpdate {
                    text: changed(|k| k.text).then(|| doc.buffer().clone()),
                    meta: changed(|k| k.meta).then(|| lua_buf_meta(doc)),
                    options: changed(|k| k.options).then(|| (**doc.options()).clone()),
                    annotations: changed(|k| k.annotations).then(|| lua_annotations(doc)),
                };
                (id, update)
            })
            .collect();
        let live: Vec<u64> = updates.iter().map(|(id, _)| *id).collect();
        self.plugin_host.retain_synced_bufs(&live);
        Some(updates)
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
                line_count: doc.buffer().get_total_lines(),
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
            .map(|doc| (doc.id.get(), (**doc.vars()).clone()))
            .collect()
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
                        match &envelope.mutation {
                            PluginMutation::CreateBuffer { id, .. }
                            | PluginMutation::CreateScratchBuf { id, .. } => {
                                self.release_plugin_creation_ids([id.get()]);
                            }
                            _ => {}
                        }
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
            self.notify(
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
            PluginMutation::ResumeSave { request } => self.resume_save(&request),
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
            #[cfg(feature = "lsp")]
            PluginMutation::LspRename { new_name } => self.rename_lsp_symbol(new_name),
            #[cfg(feature = "lsp")]
            PluginMutation::LspRegisterServer { language, config } => {
                self.register_lsp_server(language, config);
            }
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
        self.notify(
            crate::notification::NotificationType::Error,
            "internal error: plugin mutation misrouted".to_string(),
        );
    }

    fn drop_plugin_write(&mut self, buf: crate::document::DocumentId, write: &str) {
        self.state.notifications.log(
            crate::notification::NotificationType::Info,
            format!("plugin {write} skipped: buffer {} is closed", buf.get()),
        );
    }

    fn apply_ui_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            PluginMutation::Notify { message, level } => {
                self.notify(level, message);
            }
            PluginMutation::OpenFloat(content) => self.popup.open(content),
            PluginMutation::CloseFloat => self.popup.close(),
            PluginMutation::SetCursorHoldDelay(ms) => {
                self.plugin_host.set_cursor_hold_delay_ms(ms);
            }
            PluginMutation::ExecAction(action) => {
                self.handle_action(&action);
            }
            PluginMutation::SetOption { buf, name, value } => {
                self.plugin_set_option(buf, &name, &value);
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
                doc.append_lines(&lines, EditSource::Plugin);
            }
            PluginMutation::InsertAtCursor { text, .. } => {
                let _ = doc.edit(Edit::type_text(text), EditSource::Plugin);
            }
            PluginMutation::DeleteBefore { count, .. } => {
                doc.delete_near_cursor(count, true, EditSource::Plugin);
            }
            PluginMutation::DeleteForward { count, .. } => {
                doc.delete_near_cursor(count, false, EditSource::Plugin);
            }
            PluginMutation::ReplaceLines {
                start, end, lines, ..
            } => doc.replace_lines(
                start.saturating_sub(1),
                end.saturating_sub(1),
                &lines,
                EditSource::Plugin,
            ),
            _ => {}
        }
    }

    fn apply_cursor_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
            PluginMutation::SetCursor { buf, row, col } => self.plugin_set_cursor(buf, row, col),
            PluginMutation::CenterOnLine { buf, row } => self.plugin_center_on_line(buf, row),
            PluginMutation::SetScroll(top, left) => {
                self.render_system.viewport.set_scroll(top, left);
            }
            PluginMutation::SetLineEnding { buf, ending } => {
                self.plugin_set_line_ending(buf, &ending);
            }
            PluginMutation::SetBufferVar { buf, key, value } => {
                self.plugin_set_buffer_var(buf, key, value);
            }
            _ => self.report_misrouted(),
        }
    }

    fn apply_window_mutation(&mut self, mutation: &PluginMutation) {
        match *mutation {
            PluginMutation::MoveWindow { direction } => {
                self.move_window(direction);
            }
            PluginMutation::SwapWindows => {
                self.swap_windows();
            }
            PluginMutation::FocusPreviousWindow => self.focus_previous_window(),
            _ => self.report_misrouted(),
        }
    }

    fn apply_buffer_mutation(
        &mut self,
        mutation: PluginMutation,
        origin: Option<PluginGeneration>,
    ) {
        match mutation {
            PluginMutation::SaveBuffer { buf } => self.plugin_save_buffer(buf),
            PluginMutation::SwitchToBuffer(id) => self.plugin_switch_to_buffer(id),
            PluginMutation::OpenFile { path, force } => {
                self.open_file_from_input(Some(path), force);
            }
            PluginMutation::CloseBuffer { buf, force } => self.plugin_close_buffer(buf, force),
            PluginMutation::CreateScratchBuf { id, name, lines } => {
                self.create_plugin_scratch(id, name, &lines);
            }
            PluginMutation::ReloadBuffer { buf, force } => self.plugin_reload_buffer(buf, force),
            PluginMutation::CreateBuffer {
                id,
                kind,
                title,
                lines,
                vars,
            } => {
                self.create_buffer(id, &kind, title.as_deref(), &lines, vars);
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

    fn create_plugin_scratch(
        &mut self,
        id: crate::document::DocumentId,
        name: String,
        lines: &[String],
    ) {
        let Some(reservation) = self.claim_buffer_creation(id) else {
            return;
        };
        if let Err(error) = self.create_scratch_buffer(reservation, name, lines) {
            self.notify(
                crate::notification::NotificationType::Error,
                format!("create_scratch_buf failed: {}", error.message),
            );
        }
    }

    fn create_buffer(
        &mut self,
        id: crate::document::DocumentId,
        kind: &str,
        title: Option<&str>,
        lines: &[String],
        vars: Vec<(String, crate::annotations::value::Value)>,
    ) {
        let Some(descriptor) = self.buffer_kinds.get_by_name(kind) else {
            self.release_plugin_creation_ids([id.get()]);
            self.notify(
                crate::notification::NotificationType::Error,
                format!("unknown buffer kind '{kind}'"),
            );
            return;
        };
        let Some(reservation) = self.claim_buffer_creation(id) else {
            return;
        };
        let mut document = match crate::document::Document::new(reservation.id()) {
            Ok(document) => document,
            Err(error) => {
                self.notify_error(error);
                return;
            }
        };
        document.set_handle(reservation.handle());
        document.kind = crate::document::kind::BufferKind::new(descriptor.clone());
        document.initialize_vars(vars);
        if let Some(title_str) = title {
            document.set_var(
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
            document.replace_content(&text, crate::document::ops::ContentKind::Load);
        }
        let draft = match crate::document::manager::DocumentDraft::new(reservation, document) {
            Ok(draft) => draft,
            Err(error) => {
                self.notify_error(error);
                return;
            }
        };
        let id = self.open_buffer(draft, crate::document::manager::Placement::Background);
        let _ = self.focus_document(id);
        let _ = self.force_full_redraw();
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
            self.notify(
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
                self.notify(
                    crate::notification::NotificationType::Error,
                    format!("buffer kind '{name}' has invalid read_only policy: '{invalid}'"),
                );
                return;
            }
        };
        let close_policy = match parse_close_policy(close.as_deref()) {
            Ok(policy) => policy,
            Err(invalid) => {
                self.notify(
                    crate::notification::NotificationType::Error,
                    format!("buffer kind '{name}' has invalid close policy: '{invalid}'"),
                );
                return;
            }
        };
        let key_fallback_policy = match parse_key_fallback_policy(key_fallback.as_deref()) {
            Ok(policy) => policy,
            Err(invalid) => {
                self.notify(
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
            self.notify(
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
                self.notify(
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
                self.plugin_map_key(&mode, &keys, &action, origin);
            }
            PluginMutation::UnmapKey { mode, keys } => self.plugin_unmap_key(&mode, &keys),
            PluginMutation::MapKeyKind { kind, keys, action } => {
                self.plugin_map_key_kind(&kind, &keys, &action, origin);
            }
            PluginMutation::UnmapKeyKind { kind, keys } => self.plugin_unmap_key_kind(&kind, &keys),
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
                self.set_syntax_query(&lang, kind, source);
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
            self.notify(
                crate::notification::NotificationType::Error,
                format!("register_grammar '{lang_name}': {e}"),
            );
        }
    }

    #[cfg(not(feature = "treesitter"))]
    fn register_grammar(&mut self, lang_name: &str, so_path: &str, fn_name: &str) {
        self.notify(crate::notification::NotificationType::Error,
        format!(
            "register_grammar '{lang_name}' ({so_path}:{fn_name}): tree-sitter support is not compiled in"
        ),);
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
                doc.annotate_with_id(crate::annotations::AnnotationId::new(id), ann);
            }
            PluginMutation::UpdateAnnotation {
                id,
                payload,
                visible,
                presentation,
                ..
            } => {
                doc.update_annotation(crate::annotations::AnnotationId::new(id), |a| {
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
                doc.unannotate(crate::annotations::AnnotationId::new(id));
            }
            PluginMutation::ClearAnnotations { kind_prefix, .. } => {
                doc.clear_annotations_by_kind(&kind_prefix);
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
            } => self.plugin_register_annotation_action(kind, verb, command, owner),
            PluginMutation::RegisterKindDefaults {
                kind,
                presentation,
                description,
                owner,
            } => {
                self.kind_registry
                    .register_owned(kind, presentation, description, owner);
            }
            PluginMutation::UnloadPlugin { owner } => self.plugin_unload_plugin(&owner),
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
            } => self.plugin_add_ns_highlight(buf, ns, owner, &range, hl, priority),
            PluginMutation::ClearNsHighlights { buf, ns, owner } => {
                self.plugin_clear_ns_highlights(buf, ns, owner);
            }
            _ => self.report_misrouted(),
        }
    }

    fn plugin_set_option(&mut self, buf: Option<DocumentId>, name: &str, value: &str) {
        if let Some(buf) = buf {
            if self.document_manager.get_document(buf).is_none() {
                self.drop_plugin_write(buf, "set_option");
                return;
            }
        }
        let scope = buf.map_or(OptionScope::Global, OptionScope::Resolve);
        if let Err(err) = self.set_option(name, Some(value), scope) {
            self.notify_error(err);
        }
    }

    fn plugin_set_cursor(&mut self, buf: DocumentId, row: usize, col: usize) {
        let Some(doc) = self.document_manager.get_document_mut(buf) else {
            self.drop_plugin_write(buf, "set_cursor");
            return;
        };
        let pos = doc.buffer().line_col_position(row.saturating_sub(1), col);
        let _ = doc.set_cursor(pos, crate::document::ops::CursorCause::Plugin);
    }

    fn plugin_center_on_line(&mut self, buf: DocumentId, row: usize) {
        let active = self.document_manager.active_document_id() == Some(buf);
        let Some(doc) = self.document_manager.get_document_mut(buf) else {
            self.drop_plugin_write(buf, "center_on_line");
            return;
        };
        let total = doc.buffer().get_total_lines();
        let row0 = row.saturating_sub(1).min(total.saturating_sub(1));
        let target = doc.buffer().line_col_position(row0, 0);
        let _ = doc.set_cursor(target, crate::document::ops::CursorCause::Plugin);
        if active {
            self.render_system.viewport.center_on(row0, total);
        }
    }

    fn plugin_set_line_ending(&mut self, buf: DocumentId, ending: &str) {
        if self.document_manager.get_document(buf).is_none() {
            self.drop_plugin_write(buf, "set_line_ending");
            return;
        }
        let ending = crate::document::LineEnding::from_name(ending).name();
        if let Err(err) = self.set_option("line_ending", Some(ending), OptionScope::Local(buf)) {
            self.notify_error(err);
        }
    }

    fn plugin_set_buffer_var(
        &mut self,
        buf: DocumentId,
        key: String,
        value: crate::annotations::value::Value,
    ) {
        let Some(doc) = self.document_manager.get_document_mut(buf) else {
            self.drop_plugin_write(buf, "set_buffer_var");
            return;
        };
        doc.set_var(key, value);
    }

    fn plugin_save_buffer(&mut self, buf: DocumentId) {
        if self.document_manager.get_document(buf).is_some() {
            self.save_buffer(buf, None, false);
        } else {
            self.drop_plugin_write(buf, "save");
        }
    }

    fn plugin_switch_to_buffer(&mut self, id: DocumentId) {
        if self.document_manager.get_document(id).is_some() {
            let _ = self.focus_document(id);
        }
    }

    fn plugin_close_buffer(&mut self, buf: DocumentId, force: bool) {
        if self.document_manager.get_document(buf).is_none() {
            self.drop_plugin_write(buf, "close_buf");
        } else if self.active_document_id() == buf {
            self.quit(super::ops::quit::QuitRequest::Close { force });
        } else {
            let result = if force {
                self.close_buffer(buf, crate::document::manager::RemovalIntent::Force)
            } else {
                self.close_buffer(buf, crate::document::manager::RemovalIntent::Normal)
            };
            if let Err(error) = result {
                self.notify_error(error);
            }
        }
    }

    fn plugin_reload_buffer(&mut self, buf: DocumentId, force: bool) {
        if self.document_manager.get_document(buf).is_none() {
            self.drop_plugin_write(buf, "reload_buf");
        } else if let Err(error) = self.reload_document(buf, force) {
            self.notify_error(error);
        }
    }

    fn plugin_map_key(
        &mut self,
        mode: &str,
        keys: &str,
        action: &str,
        origin: Option<PluginGeneration>,
    ) {
        let Some(ctx) = crate::keymap::KeyContext::from_mode(mode) else {
            return;
        };
        if let (Some(key_seq), Ok(act)) = (
            crate::key::notation::parse_key_sequence(keys),
            action.parse::<crate::action::Action>(),
        ) {
            if let Some(gen) = origin {
                self.keymap
                    .register_with_owner(ctx, key_seq, act, gen.keymap_owner_token());
            } else {
                self.keymap.register_sequence(ctx, key_seq, act);
            }
        }
    }

    fn plugin_unmap_key(&mut self, mode: &str, keys: &str) {
        let Some(ctx) = crate::keymap::KeyContext::from_mode(mode) else {
            return;
        };
        if let Some(key_seq) = crate::key::notation::parse_key_sequence(keys) {
            self.keymap.unregister_sequence(ctx, &key_seq);
        }
    }

    fn plugin_map_key_kind(
        &mut self,
        kind: &str,
        keys: &str,
        action: &str,
        origin: Option<PluginGeneration>,
    ) {
        let Some(kind_id) = self.buffer_kinds.id_by_name(kind) else {
            return;
        };
        if let (Some(key_seq), Ok(action)) = (
            crate::key::notation::parse_key_sequence(keys),
            action.parse::<crate::action::Action>(),
        ) {
            let ctx = crate::keymap::KeyContext::Buffer(kind_id);
            if let Some(gen) = origin {
                self.keymap
                    .register_with_owner(ctx, key_seq, action, gen.keymap_owner_token());
            } else {
                self.keymap.register_sequence(ctx, key_seq, action);
            }
        }
    }

    fn plugin_unmap_key_kind(&mut self, kind: &str, keys: &str) {
        let Some(kind_id) = self.buffer_kinds.id_by_name(kind) else {
            return;
        };
        if let Some(key_seq) = crate::key::notation::parse_key_sequence(keys) {
            self.keymap
                .unregister_sequence(crate::keymap::KeyContext::Buffer(kind_id), &key_seq);
        }
    }

    fn plugin_register_annotation_action(
        &mut self,
        kind: String,
        verb: String,
        command: Option<String>,
        owner: Option<String>,
    ) {
        use crate::annotations::registry::Handler;
        let handler = command.map_or(Handler::Lua, Handler::Command);
        self.dispatch_registry
            .register_owned(kind, verb, handler, owner);
    }

    fn plugin_unload_plugin(&mut self, owner: &str) {
        self.dispatch_registry.remove_owner(owner);
        self.kind_registry.remove_owner(owner);
    }

    fn plugin_add_ns_highlight(
        &mut self,
        buf: DocumentId,
        ns: String,
        owner: Option<String>,
        range: &HighlightRange,
        hl: HighlightSpec,
        priority: i32,
    ) {
        use crate::annotations::presentation::{FaceRef, Presentation};
        let Some(doc) = self.document_manager.get_document_mut(buf) else {
            self.drop_plugin_write(buf, "highlight.add");
            return;
        };
        let bytes = highlight_bytes(doc.buffer(), range);
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

    fn plugin_clear_ns_highlights(
        &mut self,
        buf: Option<DocumentId>,
        ns: String,
        owner: Option<String>,
    ) {
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
