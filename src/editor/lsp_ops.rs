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
            LspMessage::Diagnostics { uri, diagnostics } => {
                self.handle_lsp_diagnostics(uri, diagnostics);
            }
            LspMessage::GotoDefinitionResult { locations } => {
                self.handle_goto_result(locations);
            }
            LspMessage::ReferencesResult { locations } => {
                self.handle_references_result(locations);
            }
            LspMessage::HoverResult { contents } => {
                self.handle_hover_result(contents);
            }
            LspMessage::RenameResult { workspace_edit } => {
                self.handle_rename_result(workspace_edit);
            }
            LspMessage::FormattingResult { uri, edits } => {
                self.handle_formatting_result(uri, edits);
            }
            LspMessage::CodeActionResult { actions } => {
                self.handle_code_action_result(actions);
            }
            LspMessage::CodeActionResolved { action } => {
                if let Some(edit) = action.get("edit").cloned() {
                    self.apply_workspace_edit(&edit);
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
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return;
        };

        // Replace the whole LSP diagnostic set in one pass (single index
        // invalidation, and a correct clear even when `diagnostics` is empty).
        doc.annotations
            .replace_lsp_diagnostics(diagnostics.iter().map(|diag| {
                let line = diag.range.start.line as usize;
                let severity = diag.severity.unwrap_or(1) as i64;
                (line, severity, diag.message.trim())
            }));

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

    fn handle_goto_result(&mut self, locations: Vec<LspLocation>) {
        if locations.is_empty() {
            self.state.notify(
                NotificationType::Info,
                "LSP: no definition found".to_string(),
            );
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

        let target_line = loc.range.start.line as usize;
        // `character` is in the server's negotiated wire units; converted to
        // a code-point offset once the target document's buffer is available.
        let target_units_col = loc.range.start.character;

        // If the file is already buffered we can set the cursor immediately after
        // switching. If it isn't, open_file creates a placeholder and spawns an
        // async FileLoadJob — the content won't be there yet, so we stash the jump
        // target and let the job-completion handler apply it.
        let already_open = self.document_manager.find_open_document_id(&path).is_some();

        if let Err(e) = self.open_file(Some(path.to_string_lossy().into_owned()), false) {
            self.state.handle_error(e);
            return;
        }

        if already_open {
            let encoding = self.lsp_manager.position_encoding_for_path(&path);
            if let Some(doc) = self.document_manager.active_document_mut() {
                let target_col =
                    doc.lsp_char_offset_in_line(target_line, target_units_col, encoding);
                let line_offset = doc.buffer.line_start(target_line);
                let target = line_offset + target_col;
                doc.buffer.clear_desired_col();
                let _ = doc.buffer.set_cursor(target.min(doc.buffer.len()));
            }
            let _ = self.force_full_redraw();
        } else {
            // File is loading asynchronously. Record the jump; FileLoadResult applies it.
            if let Some(doc_id) = self.document_manager.active_document_id() {
                self.pending_goto_target = Some((doc_id, target_line, target_units_col as usize));
            }
        }
    }

    fn handle_references_result(&mut self, locations: Vec<LspLocation>) {
        if locations.is_empty() {
            self.state.notify(
                NotificationType::Info,
                "LSP: no references found".to_string(),
            );
            return;
        }

        let entries: Vec<crate::document::LocationEntry> = locations
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
            .collect();

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
        let float = crate::plugin::PluginFloat::new("LSP Hover", lines);
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

        self.apply_workspace_edit(&workspace_edit);
        self.state
            .notify(NotificationType::Info, "LSP: rename applied".to_string());
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

        // Build location-list entries — uri="" signals a code action row, line = index.
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
            self.state.notify(
                NotificationType::Warning,
                "LSP: no server available for this file".to_string(),
            );
        }
    }

    /// Execute a code action — apply immediately if it has an edit, otherwise resolve it.
    pub(super) fn execute_code_action(&mut self, action: serde_json::Value) {
        if let Some(edit) = action.get("edit").cloned() {
            self.apply_workspace_edit(&edit);
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
                    "LSP: cannot resolve action — no server for this file".to_string(),
                );
            }
        }
    }

    /// Apply a workspace edit from a code action or rename.
    pub(super) fn apply_workspace_edit(&mut self, edit: &serde_json::Value) {
        use crate::lsp::protocol::LspTextEdit;
        let mut modified_docs: Vec<crate::document::DocumentId> = Vec::new();

        if let Some(changes) = edit.get("changes").and_then(|c| c.as_object()) {
            for (uri, edits_val) in changes {
                let edits: Vec<LspTextEdit> = edits_val
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                if !edits.is_empty() {
                    if let Some(doc_id) = self.apply_lsp_edits_for_uri(uri, edits) {
                        modified_docs.push(doc_id);
                    }
                }
            }
        } else if let Some(doc_changes) = edit.get("documentChanges").and_then(|d| d.as_array()) {
            for change in doc_changes {
                if let Some(uri) = change
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                {
                    let edits: Vec<LspTextEdit> = change
                        .get("edits")
                        .and_then(|e| e.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    if !edits.is_empty() {
                        if let Some(doc_id) = self.apply_lsp_edits_for_uri(uri, edits) {
                            modified_docs.push(doc_id);
                        }
                    }
                }
            }
        }

        let had_edits = !modified_docs.is_empty();
        for doc_id in modified_docs {
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                if let Some(syntax) = &mut doc.syntax {
                    syntax.invalidate_trees();
                }
            }
            self.spawn_syntax_parse_job(doc_id);
        }
        if had_edits {
            let _ = self.force_full_redraw();
        }
    }

    /// Apply LSP text edits to the open document for the given URI.
    /// Returns the DocumentId if edits were applied, so the caller can re-highlight.
    fn apply_lsp_edits_for_uri(
        &mut self,
        uri: &str,
        mut edits: Vec<LspTextEdit>,
    ) -> Option<crate::document::DocumentId> {
        let path = crate::lsp::protocol::uri_to_path(uri)?;
        let doc_id = self.document_manager.find_open_document_id(&path)?;
        let encoding = self.lsp_manager.position_encoding_for_path(&path);

        let doc = self.document_manager.get_document_mut(doc_id)?;

        // Apply from bottom to top to preserve byte offsets
        edits.sort_by(|a, b| {
            b.range
                .start
                .line
                .cmp(&a.range.start.line)
                .then(b.range.start.character.cmp(&a.range.start.character))
        });

        doc.begin_transaction("LSP edit");

        for edit in &edits {
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
        Some(doc_id)
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
        // Exclude hints (severity 4) — navigate only errors, warnings, and info.
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

        let current_line = self
            .document_manager
            .active_document()
            .map(|d| d.buffer.get_line())
            .unwrap_or(0) as u32;

        let target_line = if forward {
            diags
                .iter()
                .map(|d| d.range.start.line)
                .filter(|&l| l > current_line)
                .min()
                .or_else(|| diags.iter().map(|d| d.range.start.line).min())
        } else {
            diags
                .iter()
                .map(|d| d.range.start.line)
                .filter(|&l| l < current_line)
                .max()
                .or_else(|| diags.iter().map(|d| d.range.start.line).max())
        };

        let Some(line) = target_line else { return };

        if let Some(doc) = self.document_manager.active_document_mut() {
            let offset = doc.buffer.line_start(line as usize);
            doc.buffer.clear_desired_col();
            let _ = doc.buffer.set_cursor(offset);
        }

        // Show the diagnostic message with severity
        if let Some(diag) = diags.iter().find(|d| d.range.start.line == line) {
            let (notif_type, prefix) = match diag.severity {
                Some(1) => (NotificationType::Error, "error"),
                Some(2) => (NotificationType::Warning, "warning"),
                Some(3) => (NotificationType::Info, "info"),
                _ => (NotificationType::Info, "hint"),
            };
            self.state
                .notify(notif_type, format!("[{}] {}", prefix, diag.message));
        }

        let _ = self.update_and_render();
    }

    /// Send LSP did_open for the currently active document (if applicable).
    pub(super) fn lsp_notify_open(&mut self) {
        // Collect path, optional syntax language, and content first to avoid
        // holding a borrow on document_manager while calling language_loader.
        let info = self.document_manager.active_document().and_then(|doc| {
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
        }
    }

    /// Send LSP did_change for the currently active document.
    pub(super) fn lsp_notify_change(&mut self) {
        // Materializing the full document is O(N); skip it when no live
        // client would receive the notification (the common no-LSP case).
        let lsp_manager = &self.lsp_manager;
        let info = self.document_manager.active_document().and_then(|doc| {
            let path = doc.path()?.to_path_buf();
            if !lsp_manager.is_tracking(&path) {
                return None;
            }
            let content = String::from_utf8_lossy(&doc.buffer.to_logical_bytes()).into_owned();
            Some((path, content))
        });

        if let Some((path, content)) = info {
            self.lsp_manager.did_change(&path, &content);
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
        let diags = match self.lsp_diagnostics.get(&uri) {
            Some(d) if !d.is_empty() => d.clone(),
            _ => {
                self.state
                    .notify(NotificationType::Info, "LSP: no diagnostics".to_string());
                return;
            }
        };

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
                // Collapse multi-line messages so each entry occupies exactly one buffer line.
                // A multi-line message would shift the line→entry index mapping and
                // cause the wrong action to be applied when the user presses Enter/Space.
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
        doc.is_read_only = true;
        let content: String = entries
            .iter()
            .map(|e| e.display.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        doc.replace_buffer_content(&content);
        let _ = doc.buffer.set_cursor(0);
        doc.kind = crate::document::BufferKind::LocationList {
            source_doc_id,
            entries,
        };
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
