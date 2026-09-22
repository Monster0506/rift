use super::Editor;
use crate::buffer::api::BufferView;
use crate::lsp::protocol::{LspDiagnostic, LspLocation, LspTextEdit};
use crate::lsp::LspMessage;
use crate::notification::NotificationType;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// Called from the main loop after draining LSP messages.
    pub(super) fn handle_lsp_message(&mut self, msg: LspMessage) {
        match msg {
            LspMessage::Diagnostics {
                uri,
                version,
                diagnostics,
            } => {
                // Drop diagnostics computed for an older snapshot than the one
                // we've already sent; the server will re-publish for the new one.
                let stale = match (version, self.lsp_manager.document_version(&uri)) {
                    (Some(v), Some(cur)) => v < cur,
                    _ => false,
                };
                if !stale {
                    self.handle_lsp_diagnostics(uri, diagnostics);
                }
            }
            LspMessage::GotoDefinitionResult { locations, uri } => {
                if self.lsp_result_is_current(&uri, "definition") {
                    self.handle_goto_result(locations);
                }
            }
            LspMessage::ReferencesResult { locations, uri } => {
                if self.lsp_result_is_current(&uri, "references") {
                    self.handle_references_result(locations);
                }
            }
            LspMessage::HoverResult { contents, uri } => {
                if self.lsp_result_is_current(&uri, "hover") {
                    self.handle_hover_result(contents);
                }
            }
            LspMessage::RenameResult { workspace_edit } => {
                self.handle_rename_result(workspace_edit);
            }
            LspMessage::FormattingResult { uri, edits } => {
                self.handle_formatting_result(uri, edits);
            }
            LspMessage::CodeActionResult { actions, uri } => {
                if self.lsp_result_is_current(&uri, "code action") {
                    self.handle_code_action_result(actions);
                }
            }
            LspMessage::ApplyWorkspaceEdit { edit } => {
                let applied = self.apply_workspace_edit(&edit);
                self.state.notify(
                    NotificationType::Info,
                    format!("LSP: server edit applied to {} file(s)", applied),
                );
            }
            LspMessage::ShowMessage { kind, message } => {
                let notif_type = match kind {
                    1 => NotificationType::Error,
                    2 => NotificationType::Warning,
                    _ => NotificationType::Info,
                };
                if kind != 4 || self.state.settings.lsp_debug_log {
                    self.state.notify(notif_type, format!("LSP: {}", message));
                }
            }
            LspMessage::ServerExited { language } => {
                self.lsp_ready_servers.remove(&language);
                let name = self
                    .lsp_manager
                    .server_name(&language)
                    .unwrap_or(&language)
                    .to_string();
                self.state.lsp_status = Some(format!("{}: exited", name));
                self.state.notify(
                    NotificationType::Warning,
                    format!("LSP: {} exited; reopening documents", name),
                );
                self.update_lua_state();
                // A fresh did_open respawns the server for its documents.
                self.lsp_notify_open_all();
            }
            LspMessage::CodeActionResolved { action } => {
                if let Some(edit) = action.get("edit").cloned() {
                    self.apply_code_action_edit(&edit);
                } else {
                    self.state.notify(
                        NotificationType::Warning,
                        "LSP: server returned no edit for this action".to_string(),
                    );
                }
            }
            LspMessage::Error { method, message } => {
                self.state.notify(
                    NotificationType::Error,
                    format!("LSP [{}]: {}", method, message),
                );
            }
            LspMessage::ServerConnected {
                language,
                server_name,
            } => {
                self.state.lsp_status = Some(format!("{}: starting", server_name));
                self.update_lua_state();
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::LspServerConnected {
                        language,
                        server_name,
                    });
                self.apply_plugin_mutations();
            }
            LspMessage::ServerReady { language } => {
                self.lsp_ready_servers.insert(language.clone());
                let name = self
                    .lsp_manager
                    .server_name(&language)
                    .unwrap_or(&language)
                    .to_string();
                self.refresh_lsp_diag_status(&language, &name);
                self.update_lua_state();
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::LspServerReady {
                        language,
                        server_name: name,
                    });
                self.apply_plugin_mutations();
            }
            LspMessage::Log { message } => {
                if self.state.settings.lsp_debug_log {
                    self.state.notify(NotificationType::Info, message);
                }
            }
            LspMessage::Progress { language, .. } => {
                let name = self
                    .lsp_manager
                    .server_name(&language)
                    .unwrap_or(&language)
                    .to_string();
                let status = match self.lsp_manager.indexing_progress(&language) {
                    Some((ended, started)) => format!("{}: {}/{}", name, ended, started),
                    None => format!("{}: indexing", name),
                };
                self.state.lsp_status = Some(status);
                self.update_lua_state();
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::LspProgress {
                        language,
                        server_name: name,
                    });
                self.apply_plugin_mutations();
            }
        }
    }

    /// True when `uri` (normalized) is the active document. Stale results for a
    /// document the user has since left are dropped (logged in debug mode).
    fn lsp_result_is_current(&mut self, uri: &str, what: &str) -> bool {
        let active = self
            .document_manager
            .active_document()
            .and_then(|d| d.path())
            .map(|p| crate::lsp::protocol::normalize_uri(&crate::lsp::protocol::path_to_uri(p)));
        if active.as_deref() == Some(uri) {
            return true;
        }
        if self.state.settings.lsp_debug_log {
            self.state.notify(
                NotificationType::Info,
                format!("LSP: dropped stale {} result for {}", what, uri),
            );
        }
        false
    }

    /// Active document's cursor as (path, line, col) in the negotiated LSP
    /// position encoding, for issuing position requests.
    pub(super) fn cursor_lsp_position(&self) -> Option<(std::path::PathBuf, u32, u32)> {
        let doc = self.document_manager.active_document()?;
        let path = doc.path()?.to_path_buf();
        let cur_line = doc.buffer.get_line();
        let line_start = doc.buffer.line_start(cur_line);
        let char_col = doc.buffer.cursor().saturating_sub(line_start);
        let encoding = self.lsp_manager.position_encoding_for_path(&path);
        let col = doc.lsp_position_units_in_line(cur_line, char_col, encoding);
        Some((path, cur_line as u32, col))
    }

    /// `cursor_lsp_position`, but None (with a notice) while the server is
    /// still indexing, since requests would just queue behind it.
    pub(super) fn lsp_request_position(&mut self) -> Option<(std::path::PathBuf, u32, u32)> {
        let pos = self.cursor_lsp_position()?;
        if self.lsp_manager.is_indexing_path(&pos.0) {
            self.state.notify(
                NotificationType::Info,
                "LSP: still indexing, please wait...".to_string(),
            );
            return None;
        }
        Some(pos)
    }

    pub(super) fn notify_no_lsp_server(&mut self) {
        self.state.notify(
            NotificationType::Warning,
            "LSP: no server available for this file".to_string(),
        );
    }

    /// Recount errors and warnings across all files for `language` and update `lsp_status`.
    fn refresh_lsp_diag_status(&mut self, language: &str, server_name: &str) {
        let mut errors = 0usize;
        let mut warnings = 0usize;
        for (uri, diags) in &self.lsp_diagnostics {
            if self.lsp_manager.language_for_uri(uri) == Some(language) {
                errors += diags.iter().filter(|d| d.severity == Some(1)).count();
                warnings += diags.iter().filter(|d| d.severity == Some(2)).count();
            }
        }
        self.state.lsp_status = Some(match (errors, warnings) {
            (0, 0) => format!("{}: ready", server_name),
            (e, 0) => format!("{}: {}E", server_name, e),
            (0, w) => format!("{}: {}W", server_name, w),
            (e, w) => format!("{}: {}E {}W", server_name, e, w),
        });
    }

    fn handle_lsp_diagnostics(&mut self, uri: String, diagnostics: Vec<LspDiagnostic>) {
        // Store for navigation under a normalized key so lookups are consistent
        // regardless of drive-letter case on Windows.
        let key = crate::lsp::protocol::normalize_uri(&uri);
        self.lsp_diagnostics
            .insert(key.clone(), diagnostics.clone());

        // Find the matching document and update its annotations
        let path = crate::lsp::protocol::uri_to_path(&uri);
        let doc_id = path
            .as_ref()
            .and_then(|p| self.document_manager.find_open_document_id(p));

        let Some(doc_id) = doc_id else { return };
        let encoding = self.lsp_manager.position_encoding_for_uri(&key);
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return;
        };

        // Diagnosed span in buffer bytes (None when empty or off the end), so
        // the annotation can underline exactly what the server flagged.
        let line_count = doc.buffer.line_count();
        let byte_at = |doc: &crate::document::Document, pos: &crate::lsp::protocol::LspPosition| {
            let line = pos.line as usize;
            if line >= line_count {
                return None;
            }
            let col = doc.lsp_char_offset_in_line(line, pos.character, encoding);
            let ch = (doc.buffer.line_start(line) + col).min(doc.buffer.len());
            Some(doc.buffer.char_to_byte(ch))
        };
        let specs: Vec<crate::annotations::LspDiagnosticSpec> = diagnostics
            .iter()
            .map(|diag| {
                let line = diag.range.start.line as usize;
                let severity = diag.severity.unwrap_or(1) as i64;
                let bytes = match (
                    byte_at(doc, &diag.range.start),
                    byte_at(doc, &diag.range.end),
                ) {
                    (Some(s), Some(e)) if s < e => Some(s..e),
                    _ => None,
                };
                (line, bytes, severity, diag.message.trim())
            })
            .collect();

        // Replace the whole LSP diagnostic set in one pass (single index
        // invalidation, and a correct clear even when `diagnostics` is empty).
        doc.annotations.replace_lsp_diagnostics(specs);

        let lang = self
            .lsp_manager
            .language_for_uri(&key)
            .map(|s| s.to_string());
        let server_ready = lang
            .as_ref()
            .map(|l| self.lsp_ready_servers.contains(l))
            .unwrap_or(false);
        if server_ready {
            if let Some(ref l) = lang {
                let name = self.lsp_manager.server_name(l).unwrap_or(l).to_string();
                self.refresh_lsp_diag_status(l, &name);
            }
        }

        let error_count = diagnostics.iter().filter(|d| d.severity == Some(1)).count();
        let warning_count = diagnostics.iter().filter(|d| d.severity == Some(2)).count();
        self.update_lua_state();
        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::LspDiagnosticsChanged {
                uri: key.clone(),
                error_count,
                warning_count,
            });
        self.apply_plugin_mutations();

        let _ = self.update_and_render();
    }

    pub(super) fn handle_goto_result(&mut self, locations: Vec<LspLocation>) {
        if locations.is_empty() {
            self.state.notify(
                NotificationType::Info,
                "LSP: no definition found".to_string(),
            );
            return;
        }
        if locations.len() > 1 {
            let entries = Self::location_entries(&locations);
            self.open_location_list_panel(entries, "LSP Definitions");
            return;
        }

        let loc = &locations[0];
        let Some(path) = crate::lsp::protocol::uri_to_path(&loc.uri) else {
            self.state.notify(
                NotificationType::Error,
                "LSP: invalid URI in definition".to_string(),
            );
            return;
        };
        self.jump_to_location(
            path,
            loc.range.start.line as usize,
            loc.range.start.character,
        );
    }

    /// Open `path` and place the cursor at (`line`, `col_units`), where the
    /// column is in the server's position encoding. Deferred if the file loads async.
    pub(super) fn jump_to_location(
        &mut self,
        path: std::path::PathBuf,
        line: usize,
        col_units: u32,
    ) {
        let already_open = self.document_manager.find_open_document_id(&path).is_some();
        // Existing files that are not open load through a job.
        let loads_async = !already_open && path.exists();

        if let Err(e) = self.open_file(Some(path.to_string_lossy().into_owned()), false) {
            self.state.handle_error(e);
            return;
        }

        if loads_async {
            // FileLoadResult applies the jump once the content is in.
            if let Some(doc_id) = self.document_manager.active_document_id() {
                self.pending_goto_target = Some((doc_id, line, col_units as usize));
            }
            return;
        }

        let encoding = self.lsp_manager.position_encoding_for_path(&path);
        if let Some(doc) = self.document_manager.active_document_mut() {
            let target_col = doc.lsp_char_offset_in_line(line, col_units, encoding);
            let target = doc.buffer.line_start(line) + target_col;
            doc.buffer.clear_desired_col();
            let _ = doc.buffer.set_cursor(target.min(doc.buffer.len()));
        }
        let _ = self.force_full_redraw();
    }

    fn location_entries(locations: &[LspLocation]) -> Vec<crate::document::LocationEntry> {
        locations
            .iter()
            .map(|loc| {
                let file = crate::lsp::protocol::uri_to_path(&loc.uri)
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_else(|| loc.uri.clone());
                let line = loc.range.start.line;
                let col = loc.range.start.character;
                crate::document::LocationEntry {
                    uri: loc.uri.clone(),
                    line,
                    col,
                    display: format!("{}:{}:{}", file, line + 1, col + 1),
                }
            })
            .collect()
    }

    fn handle_references_result(&mut self, locations: Vec<LspLocation>) {
        if locations.is_empty() {
            self.state.notify(
                NotificationType::Info,
                "LSP: no references found".to_string(),
            );
            return;
        }
        let entries = Self::location_entries(&locations);
        self.open_location_list_panel(entries, "LSP References");
    }

    fn handle_hover_result(&mut self, contents: String) {
        if contents.trim().is_empty() {
            self.state
                .notify(NotificationType::Info, "LSP: no hover info".to_string());
            return;
        }

        // Wrap long lines to fit the terminal. Leave a margin for borders (4 cols)
        // and use the global wrap_width setting if set, otherwise use terminal width.
        let term_cols = self.term.get_size().map(|s| s.cols as usize).unwrap_or(80);
        let wrap_width = self
            .state
            .settings
            .wrap_width
            .unwrap_or(term_cols)
            .saturating_sub(4)
            .max(20);

        let lines = crate::render::wrap_text(&contents, wrap_width);
        let anchor_row = self.cursor_screen_row();
        let float = crate::plugin::PluginFloat::new("LSP Hover", lines).with_anchor_row(anchor_row);
        self.plugin_host
            .apply_mutation(crate::plugin::PluginMutation::OpenFloat(float));
        let _ = self.update_and_render();
    }

    fn handle_rename_result(&mut self, workspace_edit: serde_json::Value) {
        let had_changes = workspace_edit
            .get("changes")
            .and_then(|c| c.as_object())
            .map(|m| !m.is_empty())
            .unwrap_or_else(|| {
                workspace_edit
                    .get("documentChanges")
                    .and_then(|d| d.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
            });

        if !had_changes {
            self.state.notify(
                NotificationType::Warning,
                "LSP: rename produced no changes".to_string(),
            );
            return;
        }

        let applied = self.apply_workspace_edit(&workspace_edit);
        self.state.notify(
            NotificationType::Info,
            format!("LSP: rename applied to {} file(s)", applied),
        );
    }

    fn handle_formatting_result(&mut self, uri: String, edits: Vec<LspTextEdit>) {
        if edits.is_empty() {
            self.state
                .notify(NotificationType::Info, "LSP: already formatted".to_string());
            return;
        }

        if let Some(doc_id) = self.apply_lsp_edits_for_uri(&uri, edits) {
            // Invalidate before spawning: format often touches many lines and can
            // leave the incremental tree in a corrupt state.
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                if let Some(syntax) = &mut doc.syntax {
                    syntax.invalidate_trees();
                }
            }
            self.spawn_syntax_parse_job(doc_id);
            let _ = self.force_full_redraw();
        }

        self.state
            .notify(NotificationType::Success, "LSP: formatted".to_string());
    }

    fn handle_code_action_result(&mut self, actions: Vec<serde_json::Value>) {
        if actions.is_empty() {
            self.state.notify(
                NotificationType::Info,
                "LSP: no code actions available".to_string(),
            );
            return;
        }

        let entries: Vec<crate::document::LocationEntry> = actions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let raw_title = a
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("(unnamed)");
                // Collapse embedded newlines so the title fits on one line
                let title: String = raw_title
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                let kind = a
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("")
                    .to_string();
                let has_edit = a.get("edit").is_some();
                let prefix = if has_edit { "" } else { "~ " };
                let display = if kind.is_empty() {
                    format!("{}{}", prefix, title)
                } else {
                    format!("{}[{}] {}", prefix, kind, title)
                };
                crate::document::LocationEntry {
                    uri: String::new(),
                    line: i as u32,
                    col: 0,
                    display,
                }
            })
            .collect();

        self.pending_code_actions = actions;
        self.open_location_list_panel(entries, "Code Actions");
    }

    /// Submit the rename dialog: read new name from command line, fire LSP rename.
    pub(super) fn execute_lsp_rename(&mut self) {
        let new_name = self.state.command_line.clone();
        let ctx = self.rename_context.take();
        self.state.clear_command_line();
        self.set_mode(crate::mode::Mode::Normal);

        if new_name.is_empty() {
            return;
        }

        let (path, line, col) = match ctx {
            Some(c) => c,
            None => return,
        };

        if self
            .lsp_manager
            .rename(&path, line, col, new_name)
            .is_none()
        {
            self.notify_no_lsp_server();
        }
    }

    pub(super) fn execute_code_action(&mut self, action: serde_json::Value) {
        if let Some(edit) = action.get("edit").cloned() {
            self.apply_code_action_edit(&edit);
        } else {
            // Action needs a codeAction/resolve round-trip to get the edit.
            // Find the language from the active document.
            let language = self
                .document_manager
                .active_document()
                .and_then(|d| d.path())
                .and_then(|p| {
                    let uri = crate::lsp::protocol::path_to_uri(p);
                    self.lsp_manager.language_for_uri(&uri)
                })
                .map(|s| s.to_string());

            if let Some(lang) = language {
                self.lsp_manager.resolve_code_action(&lang, action);
            } else {
                self.state.notify(
                    NotificationType::Warning,
                    "LSP: cannot resolve action: no server for this file".to_string(),
                );
            }
        }
    }

    fn apply_code_action_edit(&mut self, edit: &serde_json::Value) {
        let applied = self.apply_workspace_edit(edit);
        self.state.notify(
            NotificationType::Info,
            format!("LSP: code action applied to {} file(s)", applied),
        );
    }

    /// Apply a workspace edit from a code action, rename, or the server.
    /// Returns how many documents were modified.
    pub(super) fn apply_workspace_edit(&mut self, edit: &serde_json::Value) -> usize {
        let mut per_doc: Vec<(String, Vec<LspTextEdit>)> = Vec::new();

        let doc_changes = edit
            .get("documentChanges")
            .and_then(|d| d.as_array())
            .filter(|a| !a.is_empty());
        if let Some(doc_changes) = doc_changes {
            for change in doc_changes {
                if let Some(kind) = change.get("kind").and_then(|k| k.as_str()) {
                    let target = change
                        .get("uri")
                        .or_else(|| change.get("newUri"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("?");
                    self.state.notify(
                        NotificationType::Warning,
                        format!(
                            "LSP: unsupported '{}' file operation skipped: {}",
                            kind, target
                        ),
                    );
                    continue;
                }
                let Some(uri) = change
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                else {
                    continue;
                };
                if let Some(edits) = self.parse_text_edits(uri, change.get("edits")) {
                    per_doc.push((uri.to_string(), edits));
                }
            }
        } else if let Some(changes) = edit.get("changes").and_then(|c| c.as_object()) {
            for (uri, edits_val) in changes {
                if let Some(edits) = self.parse_text_edits(uri, Some(edits_val)) {
                    per_doc.push((uri.clone(), edits));
                }
            }
        }

        let mut modified_docs: Vec<crate::document::DocumentId> = Vec::new();
        for (uri, edits) in per_doc {
            if edits.is_empty() {
                continue;
            }
            if let Some(doc_id) = self.apply_lsp_edits_for_uri(&uri, edits) {
                if !modified_docs.contains(&doc_id) {
                    modified_docs.push(doc_id);
                }
            }
        }

        let applied = modified_docs.len();
        for doc_id in modified_docs {
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                if let Some(syntax) = &mut doc.syntax {
                    syntax.invalidate_trees();
                }
            }
            self.spawn_syntax_parse_job(doc_id);
        }
        if applied > 0 {
            let _ = self.force_full_redraw();
        }
        applied
    }

    /// Deserialize one document's edit array; a single malformed edit rejects
    /// the whole set (partial application would corrupt the file).
    fn parse_text_edits(
        &mut self,
        uri: &str,
        edits: Option<&serde_json::Value>,
    ) -> Option<Vec<LspTextEdit>> {
        let Some(edits) = edits else {
            return Some(Vec::new());
        };
        match serde_json::from_value::<Vec<LspTextEdit>>(edits.clone()) {
            Ok(edits) => Some(edits),
            Err(e) => {
                self.state.notify(
                    NotificationType::Warning,
                    format!("LSP: malformed edit for {} skipped: {}", uri, e),
                );
                None
            }
        }
    }

    /// Apply LSP text edits to the document for `uri`, opening it in the
    /// background if needed. Returns the DocumentId so the caller can re-highlight.
    fn apply_lsp_edits_for_uri(
        &mut self,
        uri: &str,
        mut edits: Vec<LspTextEdit>,
    ) -> Option<crate::document::DocumentId> {
        let path = crate::lsp::protocol::uri_to_path(uri)?;
        let encoding = self.lsp_manager.position_encoding_for_path(&path);
        let was_open = self.document_manager.find_open_document_id(&path);
        let doc_id = match was_open {
            Some(id) => id,
            None => match self.open_document_in_background(&path) {
                Some(id) => id,
                None => {
                    self.state.notify(
                        NotificationType::Warning,
                        format!("LSP: could not open {} to apply edits", path.display()),
                    );
                    return None;
                }
            },
        };

        let doc = self.document_manager.get_document_mut(doc_id)?;

        // Apply bottom-up so earlier offsets stay valid; the stable ascending
        // sort keeps same-position inserts in the server's array order.
        edits.sort_by(|a, b| {
            (a.range.start.line, a.range.start.character)
                .cmp(&(b.range.start.line, b.range.start.character))
        });

        let cursor = doc.buffer.cursor();
        let cursor_line = doc.buffer.line_index.get_line_at(cursor);
        let cursor_col = cursor.saturating_sub(doc.buffer.line_start(cursor_line));

        doc.begin_transaction("LSP edit");

        for edit in edits.iter().rev() {
            let start_line = edit.range.start.line as usize;
            let end_line = edit.range.end.line as usize;
            let start_char =
                doc.lsp_char_offset_in_line(start_line, edit.range.start.character, encoding);
            let end_char =
                doc.lsp_char_offset_in_line(end_line, edit.range.end.character, encoding);

            let start_offset = doc.buffer.line_start(start_line) + start_char;
            let end_offset = doc.buffer.line_start(end_line) + end_char;

            // Use Document-level API so edits are recorded in the transaction
            // (for undo) and tree-sitter is updated incrementally (for syntax).
            if end_offset > start_offset {
                let _ = doc.delete_range(start_offset, end_offset);
            }

            if !edit.new_text.is_empty() {
                let _ = doc.buffer.set_cursor(start_offset);
                let _ = doc.insert_str(&edit.new_text);
            }
        }

        doc.commit_transaction();

        // Put the cursor back where the user had it, clamped to the new text.
        let line = cursor_line.min(doc.buffer.get_total_lines().saturating_sub(1));
        let line_start = doc.buffer.line_start(line);
        let line_end = if line + 1 < doc.buffer.get_total_lines() {
            doc.buffer.line_start(line + 1).saturating_sub(1)
        } else {
            doc.buffer.len()
        };
        doc.buffer.clear_desired_col();
        let _ = doc
            .buffer
            .set_cursor((line_start + cursor_col).min(line_end));

        if was_open.is_none() {
            self.lsp_notify_open(doc_id);
        } else {
            self.lsp_flush_pending_changes();
        }
        Some(doc_id)
    }

    /// Load `path` synchronously as a non-active tab (no view switch), with
    /// syntax attached. Returns None if the file can't be read.
    fn open_document_in_background(
        &mut self,
        path: &std::path::Path,
    ) -> Option<crate::document::DocumentId> {
        let id = self.document_manager.next_id();
        let doc = crate::document::Document::from_file(id, path).ok()?;
        self.document_manager.add_document_inactive(doc);
        self.attach_syntax_for_document(id);
        Some(id)
    }

    /// Jump to the next LSP diagnostic in the active document.
    pub(super) fn lsp_diagnostic_next(&mut self) {
        self.lsp_diagnostic_jump(true);
    }

    /// Jump to the previous LSP diagnostic in the active document.
    pub(super) fn lsp_diagnostic_prev(&mut self) {
        self.lsp_diagnostic_jump(false);
    }

    fn lsp_diagnostic_jump(&mut self, forward: bool) {
        let path = self
            .document_manager
            .active_document()
            .and_then(|d| d.path())
            .map(|p| p.to_path_buf());

        let Some(path) = path else {
            self.state
                .notify(NotificationType::Info, "LSP: no file path".to_string());
            return;
        };

        let uri = crate::lsp::protocol::normalize_uri(&crate::lsp::protocol::path_to_uri(&path));
        let diags: Vec<_> = self
            .lsp_diagnostics
            .get(&uri)
            .map(|d| {
                d.iter()
                    .filter(|d| d.severity != Some(4))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if diags.is_empty() {
            self.state
                .notify(NotificationType::Info, "LSP: no diagnostics".to_string());
            return;
        }

        let encoding = self.lsp_manager.position_encoding_for_path(&path);
        let cursor_pos = self
            .document_manager
            .active_document()
            .map(|d| {
                let line = d.buffer.get_line();
                let col = d.buffer.cursor().saturating_sub(d.buffer.line_start(line));
                (
                    line as u32,
                    d.lsp_position_units_in_line(line, col, encoding),
                )
            })
            .unwrap_or((0, 0));

        let pos = |d: &LspDiagnostic| (d.range.start.line, d.range.start.character);
        let target = if forward {
            diags
                .iter()
                .filter(|d| pos(d) > cursor_pos)
                .min_by_key(|d| pos(d))
                .or_else(|| diags.iter().min_by_key(|d| pos(d)))
        } else {
            diags
                .iter()
                .filter(|d| pos(d) < cursor_pos)
                .max_by_key(|d| pos(d))
                .or_else(|| diags.iter().max_by_key(|d| pos(d)))
        };
        let Some(diag) = target else { return };

        if let Some(doc) = self.document_manager.active_document_mut() {
            let line = diag.range.start.line as usize;
            let col = doc.lsp_char_offset_in_line(line, diag.range.start.character, encoding);
            let offset = (doc.buffer.line_start(line) + col).min(doc.buffer.len());
            doc.buffer.clear_desired_col();
            let _ = doc.buffer.set_cursor(offset);
        }

        let (notif_type, prefix) = match diag.severity {
            Some(1) => (NotificationType::Error, "error"),
            Some(2) => (NotificationType::Warning, "warning"),
            Some(3) => (NotificationType::Info, "info"),
            _ => (NotificationType::Info, "hint"),
        };
        self.state
            .notify(notif_type, format!("[{}] {}", prefix, diag.message));

        let _ = self.update_and_render();
    }

    /// Send LSP did_close for `doc_id`, if it has a tracked file path. Call
    /// before removing the document, since the path won't be readable after.
    pub(super) fn lsp_notify_close(&mut self, doc_id: crate::document::DocumentId) {
        if let Some(path) = self
            .document_manager
            .get_document(doc_id)
            .and_then(|doc| doc.path())
            .map(|p| p.to_path_buf())
        {
            self.lsp_manager.did_close(&path);
        }
    }

    /// Send LSP did_open for `doc_id` (no-op without a path or a known language).
    pub(super) fn lsp_notify_open(&mut self, doc_id: crate::document::DocumentId) {
        // Collect path, optional syntax language, and content first to avoid
        // holding a borrow on document_manager while calling language_loader.
        let info = self.document_manager.get_document(doc_id).and_then(|doc| {
            let path = doc.path()?.to_path_buf();
            let syntax_lang = doc.syntax.as_ref().map(|s| s.language_name.clone());
            let content = String::from_utf8_lossy(&doc.buffer.to_logical_bytes()).into_owned();
            Some((path, syntax_lang, content))
        });

        if let Some((path, syntax_lang, content)) = info {
            // Prefer the language from the loaded tree-sitter syntax; fall back to
            // the filetype registry so LSP works even without a grammar.
            let language =
                syntax_lang.or_else(|| self.language_loader.language_name_for_file(&path));
            if let Some(language) = language {
                self.lsp_manager.did_open(&path, &language, &content);
            }
            // The server has this content; nothing older needs replaying.
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                doc.discard_pending_lsp_changes();
            }
        }
    }

    /// Send did_open for every open document, e.g. after a server is
    /// registered or restarted; unrelated languages no-op inside the manager.
    pub(super) fn lsp_notify_open_all(&mut self) {
        let ids: Vec<crate::document::DocumentId> = self
            .document_manager
            .iter_documents()
            .filter(|d| d.path().is_some())
            .map(|d| d.id)
            .collect();
        for id in ids {
            self.lsp_notify_open(id);
        }
    }

    /// Send LSP did_change for `doc_id`: an incremental range+text delta when
    /// possible, otherwise the full document (skipped if nothing tracks it).
    pub(super) fn lsp_notify_change(&mut self, doc_id: crate::document::DocumentId) {
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return;
        };
        let Some(path) = doc.path().map(|p| p.to_path_buf()) else {
            doc.discard_pending_lsp_changes();
            return;
        };
        let uri = crate::lsp::protocol::path_to_uri(&path);
        if !self.lsp_manager.is_tracking_uri(&uri) {
            doc.discard_pending_lsp_changes();
            return;
        }

        let encoding = self.lsp_manager.position_encoding_for_uri(&uri);
        if self.lsp_manager.supports_incremental_sync_uri(&uri) {
            if let Some((range, text)) = doc.take_incremental_lsp_changes(encoding) {
                self.lsp_manager
                    .did_change_incremental_uri(&uri, vec![(range, text)]);
                return;
            }
        } else {
            doc.discard_pending_lsp_changes();
        }

        let content = String::from_utf8_lossy(&doc.buffer.to_logical_bytes()).into_owned();
        self.lsp_manager.did_change_uri(&uri, &content);
    }

    /// Push every document's unsent edits to its server. Runs once per loop
    /// iteration and after LSP-applied edits, so no path can leave a doc stale.
    pub(super) fn lsp_flush_pending_changes(&mut self) {
        let ids: Vec<crate::document::DocumentId> = self
            .document_manager
            .iter_documents()
            .filter(|d| d.path().is_some() && d.has_pending_lsp_edits())
            .map(|d| d.id)
            .collect();
        for id in ids {
            self.lsp_notify_change(id);
        }
    }

    /// Open the diagnostics panel for the current document.
    pub(super) fn open_diagnostics_panel(&mut self) {
        let info = self.document_manager.active_document().and_then(|doc| {
            let path = doc.path()?.to_path_buf();
            let doc_id = doc.id;
            Some((path, doc_id))
        });

        let Some((path, source_doc_id)) = info else {
            self.state
                .notify(NotificationType::Info, "LSP: no file open".to_string());
            return;
        };

        let uri = crate::lsp::protocol::normalize_uri(&crate::lsp::protocol::path_to_uri(&path));
        let mut diags = match self.lsp_diagnostics.get(&uri) {
            Some(d) if !d.is_empty() => d.clone(),
            _ => {
                self.state
                    .notify(NotificationType::Info, "LSP: no diagnostics".to_string());
                return;
            }
        };
        diags.sort_by_key(|d| {
            (
                d.range.start.line,
                d.range.start.character,
                d.severity.unwrap_or(u32::MAX),
            )
        });

        let entries: Vec<crate::document::LocationEntry> = diags
            .iter()
            .map(|d| {
                let severity = match d.severity {
                    Some(1) => "E",
                    Some(2) => "W",
                    Some(3) => "I",
                    _ => "H",
                };
                let line = d.range.start.line;
                let col = d.range.start.character;
                let first_line = d
                    .message
                    .lines()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty())
                    .unwrap_or("");
                crate::document::LocationEntry {
                    uri: uri.clone(),
                    line,
                    col,
                    display: format!("[{}] {}:{}  {}", severity, line + 1, col + 1, first_line),
                }
            })
            .collect();

        let _ = source_doc_id;
        self.open_location_list_panel(entries, "LSP Diagnostics");
    }

    /// Create a location list split panel with the given entries.
    pub(super) fn open_location_list_panel(
        &mut self,
        entries: Vec<crate::document::LocationEntry>,
        title: &str,
    ) {
        // Close any existing panel first.
        if self.panel_layout.is_some() {
            self.close_split_panel();
        }

        let source_doc_id = self
            .document_manager
            .active_document()
            .map(|d| d.id)
            .unwrap_or(0);

        // Create the location list document.
        let loc_doc_id = self.document_manager.next_id();
        let mut doc = match crate::document::Document::new(loc_doc_id) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        let content: String = entries
            .iter()
            .map(|e| e.display.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        doc.replace_buffer_content(&content);
        let _ = doc.buffer.set_cursor(0);
        doc.set_location_list(source_doc_id, entries);
        self.document_manager.add_private_document(doc);

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let rows = size.rows as usize;
        let cols = size.cols as usize;

        // Split the current window horizontally: location list goes below, main stays above.
        let preview_win_id = self.split_tree.focused_window_id();
        let original_doc_id = self.split_tree.focused_window().document_id;

        let dir_win_id = self
            .split_tree
            .split(
                crate::split::tree::SplitDirection::Horizontal,
                preview_win_id,
                loc_doc_id,
                rows,
                cols,
            )
            .expect("preview_win_id is the focused window, which is always a valid leaf");

        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(loc_doc_id);

        self.panel_layout = Some(crate::editor::PanelLayout {
            kind: crate::editor::PanelKind::LocationList,
            dir_win_id,
            preview_win_id,
            dir_doc_id: loc_doc_id,
            preview_doc_id: original_doc_id,
            original_doc_id,
        });

        let _ = title;
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }
}
