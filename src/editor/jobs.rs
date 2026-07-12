use super::Editor;
use crate::buffer::api::BufferView;
use crate::document::DocumentId;
use crate::error::RiftError;
use crate::mode::Mode;
use crate::term::TerminalBackend;
#[cfg(feature = "treesitter")]
use std::sync::Arc;

/// Debounce window for backgrounding a syntax parse after a sync attempt
/// exceeds its time budget — coalesces rapid keystrokes into one job.
const SYNTAX_REPARSE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(40);

/// Tracks a document's outstanding background syntax reparse, so a burst of
/// edits debounces into one job and a stale in-flight job gets cancelled
/// before a fresh one is spawned.
#[derive(Default)]
pub(super) struct PendingSyntaxReparse {
    /// When set, the debounce timer is running; fires once `Instant::now() >= deadline`.
    debounce_deadline: Option<std::time::Instant>,
    in_flight_job: Option<usize>,
}

impl<T: TerminalBackend> Editor<T> {
    #[cfg(feature = "treesitter")]
    pub(super) fn spawn_syntax_parse_job(
        &mut self,
        doc_id: crate::document::DocumentId,
    ) -> Option<usize> {
        use crate::job_manager::jobs::syntax::SyntaxParseJob;
        use tree_sitter::Parser;

        let doc = self.document_manager.get_document(doc_id)?;
        let syntax = doc.syntax.as_ref()?;

        let mut parser = Parser::new();
        if parser.set_language(&syntax.language).is_err() {
            return None;
        }

        let buffer = {
            crate::perf_span!(
                "syntax_job_buffer_clone",
                crate::perf::PerfFields {
                    bytes: Some(doc.buffer.byte_len() as u32),
                    ..Default::default()
                }
            );
            doc.buffer.clone()
        };

        let (old_highlights, pending_edits) = syntax.highlights_snapshot();

        let job = SyntaxParseJob::new(
            buffer,
            parser,
            syntax.tree.clone(),
            syntax.highlights_query.clone(),
            syntax.language_name.clone(),
            doc_id,
            doc.buffer.revision,
        )
        .with_lib(syntax.lib())
        .with_highlights_context(old_highlights, &pending_edits);

        Some(self.job_manager.spawn(job))
    }

    /// No-op when tree-sitter is compiled out: there is no syntax state to reparse.
    #[cfg(not(feature = "treesitter"))]
    pub(super) fn spawn_syntax_parse_job(
        &mut self,
        _doc_id: crate::document::DocumentId,
    ) -> Option<usize> {
        None
    }

    /// Schedule a background reparse for `doc_id`, debounced so a burst of
    /// edits within [`SYNTAX_REPARSE_DEBOUNCE`] coalesces into one job.
    /// Called when a sync `try_incremental_parse` aborts on its time budget.
    pub(super) fn debounce_syntax_reparse(&mut self, doc_id: DocumentId) {
        let entry = self.pending_syntax_reparse.entry(doc_id).or_default();
        entry.debounce_deadline = Some(std::time::Instant::now() + SYNTAX_REPARSE_DEBOUNCE);
    }

    /// Cancel any pending/in-flight background reparse for `doc_id` because a
    /// sync parse just brought it fully up to date.
    pub(super) fn cancel_pending_syntax_reparse(&mut self, doc_id: DocumentId) {
        if let Some(entry) = self.pending_syntax_reparse.remove(&doc_id) {
            if let Some(job_id) = entry.in_flight_job {
                self.job_manager.cancel_job(job_id);
            }
        }
    }

    /// Spawn a background reparse immediately (bypassing the debounce timer),
    /// cancelling any job already in flight for this doc first. Used for
    /// discrete one-shot triggers like undo/redo, not routine typing.
    pub(super) fn spawn_syntax_parse_job_immediate(&mut self, doc_id: DocumentId) {
        if let Some(entry) = self.pending_syntax_reparse.get(&doc_id) {
            if let Some(old_job) = entry.in_flight_job {
                self.job_manager.cancel_job(old_job);
            }
        }
        if let Some(job_id) = self.spawn_syntax_parse_job(doc_id) {
            let entry = self.pending_syntax_reparse.entry(doc_id).or_default();
            entry.in_flight_job = Some(job_id);
            entry.debounce_deadline = None;
        } else {
            self.pending_syntax_reparse.remove(&doc_id);
        }
    }

    /// Fire any debounce timers that have elapsed: cancel a stale in-flight
    /// job (if any) and spawn a fresh one for the document's latest content.
    /// Called once per frame from the run loop.
    pub(super) fn poll_pending_syntax_reparse(&mut self) {
        let now = std::time::Instant::now();
        let due: Vec<DocumentId> = self
            .pending_syntax_reparse
            .iter()
            .filter(|(_, p)| p.debounce_deadline.is_some_and(|d| now >= d))
            .map(|(doc_id, _)| *doc_id)
            .collect();

        for doc_id in due {
            if let Some(entry) = self.pending_syntax_reparse.get_mut(&doc_id) {
                entry.debounce_deadline = None;
                if let Some(old_job) = entry.in_flight_job.take() {
                    self.job_manager.cancel_job(old_job);
                }
            }
            if let Some(job_id) = self.spawn_syntax_parse_job(doc_id) {
                if let Some(entry) = self.pending_syntax_reparse.get_mut(&doc_id) {
                    entry.in_flight_job = Some(job_id);
                }
            } else {
                self.pending_syntax_reparse.remove(&doc_id);
            }
        }
    }

    /// Handle a message from a background job
    pub(super) fn handle_job_message(
        &mut self,
        msg: crate::job_manager::JobMessage,
    ) -> Result<(), RiftError> {
        #[cfg(feature = "treesitter")]
        use crate::job_manager::jobs::syntax::SyntaxParseResult;
        use crate::job_manager::JobMessage;
        // Parser import not needed here

        // Update manager state
        self.job_manager.update_job_state(&msg);

        match msg {
            JobMessage::Started(id, silent) => {
                let name = self.job_manager.job_name(id);
                self.state.error_manager.notifications_mut().log_job_event(
                    id,
                    crate::notification::JobEventKind::Started,
                    silent,
                    format!("{}: started", name),
                );
            }
            JobMessage::Progress(id, percentage, msg) => {
                let silent = self.job_manager.is_job_silent(id);
                let name = self.job_manager.job_name(id);
                self.state.error_manager.notifications_mut().log_job_event(
                    id,
                    crate::notification::JobEventKind::Progress(percentage),
                    silent,
                    format!("{}: {}", name, msg),
                );
            }
            JobMessage::Finished(id, silent) => {
                let name = self.job_manager.job_name(id);
                self.state.error_manager.notifications_mut().log_job_event(
                    id,
                    crate::notification::JobEventKind::Finished,
                    silent,
                    format!("{}: finished", name),
                );
            }
            JobMessage::Error(id, err) => {
                let silent = self.job_manager.is_job_silent(id);
                let name = self.job_manager.job_name(id);
                self.state.error_manager.notifications_mut().log_job_event(
                    id,
                    crate::notification::JobEventKind::Error,
                    silent,
                    format!("{}: {}", name, err),
                );
                self.state.notify(
                    crate::notification::NotificationType::Error,
                    format!("{} failed: {}", name, err),
                );
            }
            JobMessage::Cancelled(id) => {
                let silent = self.job_manager.is_job_silent(id);
                let name = self.job_manager.job_name(id);
                self.state.error_manager.notifications_mut().log_job_event(
                    id,
                    crate::notification::JobEventKind::Cancelled,
                    silent,
                    format!("{}: cancelled", name),
                );
                self.state.notify(
                    crate::notification::NotificationType::Warning,
                    format!("{} cancelled", name),
                );

                if self.pending_quit_job_id == Some(id) {
                    self.pending_quit_job_id = None;
                    self.state.notify(
                        crate::notification::NotificationType::Warning,
                        "Quit aborted: save was cancelled".to_string(),
                    );
                }
            }
            JobMessage::Custom(id, payload) => {
                let any_payload = payload.into_any();

                // Try DirectoryListing — route to the document by id
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::explorer::DirectoryListing>(
                ) {
                    Ok(listing) => {
                        let doc_id = listing.doc_id as crate::document::DocumentId;
                        let entries: Vec<crate::document::DirEntry> = listing
                            .entries
                            .iter()
                            .map(|e| crate::document::DirEntry {
                                path: e.path.clone(),
                                is_dir: e.is_dir,
                                id: 0,
                            })
                            .collect();
                        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                            // Discard stale results if the doc has navigated to a different path
                            let path_matches = matches!(&doc.kind,
                                crate::document::BufferKind::Directory { path, .. }
                                if *path == listing.path);
                            if path_matches {
                                doc.populate_directory_buffer(entries);
                            }
                        }
                        // Restore cursor to the child entry we navigated away from, if any.
                        if let Some(target_name) = self.pending_cursor_entry.take() {
                            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                                let path_matches = matches!(&doc.kind,
                                    crate::document::BufferKind::Directory { path, .. }
                                    if *path == listing.path);
                                if path_matches {
                                    let line_pos = doc
                                        .annotations
                                        .directory_entries_by_line()
                                        .into_iter()
                                        .find_map(|(line, eid)| {
                                            if let crate::document::BufferKind::Directory {
                                                entries,
                                                ..
                                            } = &doc.kind
                                            {
                                                entries.iter().find(|e| e.id == eid).and_then(|e| {
                                                    e.path
                                                        .file_name()
                                                        .and_then(|n| n.to_str())
                                                        .filter(|n| *n == target_name)
                                                        .map(|_| {
                                                            doc.buffer
                                                                .line_index
                                                                .get_start(line)
                                                                .unwrap_or(0)
                                                        })
                                                })
                                            } else {
                                                None
                                            }
                                        });
                                    if let Some(pos) = line_pos {
                                        let _ = doc.buffer.set_cursor(pos);
                                    }
                                }
                            }
                        }
                        if matches!(self.document_manager.get_document(doc_id),
                            Some(doc) if matches!(&doc.kind,
                                crate::document::BufferKind::Directory { path, .. }
                                if *path == listing.path))
                        {
                            self.sync_state_with_active_document();
                            let _ = self.force_full_redraw();
                            self.update_explorer_preview();
                        }
                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try UndoTreeRenderResult — populate the matching undotree buffer
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::undotree::UndoTreeRenderResult>(
                ) {
                    Ok(res) => {
                        if let Some(ut_doc) = self.document_manager.get_document_mut(res.ut_doc_id)
                        {
                            ut_doc.populate_undotree_buffer(
                                res.text,
                                res.sequences,
                                res.highlights,
                            );
                        }
                        self.sync_state_with_active_document();
                        let _ = self.force_full_redraw();
                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try ExplorerPreviewResult — populate the preview pane
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::explorer_preview::ExplorerPreviewResult>()
                {
                    Ok(res) => {
                        // Clear in-flight tracking for this job regardless of staleness.
                        if self
                            .pending_explorer_preview
                            .as_ref()
                            .is_some_and(|p| p.job_id == id)
                        {
                            self.pending_explorer_preview = None;
                        }

                        // Discard stale results: the cursor may have moved to a different
                        // entry since this job was spawned.
                        let current_target = self.current_explorer_target_path();
                        if current_target.as_deref() != Some(res.path.as_path()) {
                            self.job_manager
                                .update_job_state(&JobMessage::Finished(id, true));
                            return Ok(());
                        }

                        let preview_doc_id = res.right_doc_id;
                        let preview_path = res.path.clone();
                        #[cfg_attr(not(feature = "treesitter"), allow(unused_variables))]
                        let is_file_preview = res.dir_entries.is_none();
                        if let Some(doc) = self.document_manager.get_document_mut(preview_doc_id) {
                            doc.replace_buffer_content("");
                            doc.syntax = None;
                            doc.custom_highlights.clear();

                            if let Some(entries) = res.dir_entries {
                                doc.kind = crate::document::BufferKind::Directory {
                                    path: res.path.clone(),
                                    entries: entries.clone(),
                                    show_hidden: false,
                                };
                                doc.populate_directory_buffer(entries);
                            } else if let Some(text) = res.file_text {
                                doc.kind = crate::document::BufferKind::File;
                                doc.set_path(&preview_path);
                                let _ = doc.buffer.insert_str(&text);
                                let _ = doc.buffer.set_cursor(0);
                            }
                        }

                        #[cfg(feature = "treesitter")]
                        if is_file_preview {
                            if let Ok(loaded) = self.language_loader.load_language_for_file(&preview_path) {
                                let highlights = self
                                    .language_loader
                                    .load_query(&loaded.name, "highlights")
                                    .ok()
                                    .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
                                    .map(Arc::new);
                                if let Ok(syntax) = crate::syntax::build_syntax(
                                    loaded,
                                    highlights,
                                    self.language_loader.clone(),
                                ) {
                                    if let Some(doc) = self.document_manager.get_document_mut(preview_doc_id) {
                                        doc.set_syntax(syntax);
                                        // Synchronously parse so highlights are ready for the
                                        // immediately following render (no async timing gap).
                                        if doc.buffer.byte_len() <= super::SYNC_PARSE_MAX_BYTES {
                                            let source = doc.buffer.to_logical_bytes();
                                            if let Some(s) = &mut doc.syntax {
                                                s.incremental_parse(&source);
                                            }
                                        }
                                    }
                                    self.spawn_syntax_parse_job(preview_doc_id);
                                }
                            }
                        }

                        self.sync_state_with_active_document();
                        let _ = self.update_and_render();
                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try FileSaveResult
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::file_operations::FileSaveResult>(
                ) {
                    Ok(res) => {
                        if let Some(doc) = self.document_manager.get_document_mut(res.document_id) {
                            doc.mark_as_saved(res.saved_seq);
                            doc.set_path(res.path.clone());

                            // Update cached filename in state
                            let display_name = doc.display_name().to_string();
                            self.state.update_filename(display_name);
                        }

                        // Show success notification
                        self.state.notify(
                            crate::notification::NotificationType::Success,
                            format!("Written to {}", res.path.display()),
                        );

                        self.update_lua_state();
                        self.plugin_host
                            .dispatch(&crate::plugin::EditorEvent::BufSavePost {
                                buf: res.document_id,
                                path: res.path.clone(),
                            });
                        self.apply_plugin_mutations();
                        self.lsp_manager.did_save(&res.path, None);

                        if self.pending_quit_job_id == Some(id) {
                            self.should_quit = true;
                        }

                        // Sync state and redraw to update dirty indicator
                        self.sync_state_with_active_document();
                        let _ = self.update_and_render();

                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                // Try FileLoadResult
                let any_payload = match any_payload
                    .downcast::<crate::job_manager::jobs::file_operations::FileLoadResult>(
                ) {
                    Ok(res) => {
                        // Scope for doc mutation
                        let warming_data = if let Some(doc) =
                            self.document_manager.get_document_mut(res.document_id)
                        {
                            doc.apply_loaded_content(res.line_index, res.line_ending);
                            // Extract data for cache warming
                            let table = doc.buffer.line_index.table.clone();
                            let revision = doc.buffer.revision;
                            let path = doc.path().map(|p| p.to_path_buf());
                            Some((table, revision, path))
                        } else {
                            None
                        };

                        // Re-initialize syntax
                        #[cfg(feature = "treesitter")]
                        if let Some((_, _, Some(path))) = &warming_data {
                            if let Ok(loaded) = self.language_loader.load_language_for_file(path) {
                                let highlights = self
                                    .language_loader
                                    .load_query(&loaded.name, "highlights")
                                    .ok()
                                    .and_then(|source| {
                                        tree_sitter::Query::new(&loaded.language, &source).ok()
                                    })
                                    .map(Arc::new);

                                if let Ok(syntax) = crate::syntax::build_syntax(
                                    loaded,
                                    highlights,
                                    self.language_loader.clone(),
                                ) {
                                    if let Some(doc) =
                                        self.document_manager.get_document_mut(res.document_id)
                                    {
                                        doc.set_syntax(syntax);
                                    }
                                }
                            }
                        }

                        // Spawn syntax parse (requires self)
                        self.spawn_syntax_parse_job(res.document_id);

                        // Spawn cache warming if data extracted
                        if let Some((table, revision, _)) = warming_data {
                            let job = crate::job_manager::jobs::cache_warming::CacheWarmingJob::new(
                                table, revision,
                            );
                            self.job_manager.spawn(job);
                        }

                        if let Some(doc) = self.document_manager.get_document(res.document_id) {
                            let path = doc.path().map(|p| p.to_path_buf());
                            let filetype = doc.syntax.as_ref().map(|s| s.language_name.clone());
                            self.update_lua_state();
                            if res.is_reload {
                                self.plugin_host
                                    .dispatch(&crate::plugin::EditorEvent::BufReload {
                                        buf: res.document_id,
                                    });
                            } else {
                                self.plugin_host
                                    .dispatch(&crate::plugin::EditorEvent::BufOpen {
                                        buf: res.document_id,
                                        path,
                                        filetype,
                                    });
                            }
                            self.apply_plugin_mutations();
                        }

                        // Notify LSP after opening a new (non-reload) file
                        if !res.is_reload {
                            self.lsp_notify_open();
                        }

                        // Apply any deferred goto-definition jump that was stashed
                        // because the file wasn't open when the LSP response arrived.
                        if let Some((goto_doc, goto_line, goto_col)) =
                            self.pending_goto_target.take()
                        {
                            if goto_doc == res.document_id {
                                let encoding = self
                                    .document_manager
                                    .get_document(res.document_id)
                                    .and_then(|d| d.path())
                                    .map(|p| self.lsp_manager.position_encoding_for_path(p))
                                    .unwrap_or_default();
                                if let Some(doc) =
                                    self.document_manager.get_document_mut(res.document_id)
                                {
                                    let char_col = doc.lsp_char_offset_in_line(
                                        goto_line,
                                        goto_col as u32,
                                        encoding,
                                    );
                                    let line_offset = doc.buffer.line_start(goto_line);
                                    let target = (line_offset + char_col).min(doc.buffer.len());
                                    doc.buffer.clear_desired_col();
                                    let _ = doc.buffer.set_cursor(target);
                                }
                            } else {
                                // Wrong document loaded — put the target back
                                self.pending_goto_target = Some((goto_doc, goto_line, goto_col));
                            }
                        }

                        self.sync_state_with_active_document();
                        let _ = self.force_full_redraw();

                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                #[cfg(feature = "treesitter")]
                match any_payload.downcast::<SyntaxParseResult>() {
                    Ok(result) => {
                        let doc_id = result.document_id;
                        let is_current = self
                            .document_manager
                            .get_document(doc_id)
                            .is_some_and(|doc| doc.buffer.revision == result.revision);

                        if is_current {
                            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                                if let Some(syntax) = &mut doc.syntax {
                                    // Capture the pre-update tree/edit so injections can be
                                    // scoped to the changed region below.
                                    let (old_host_tree, edit): (
                                        Option<tree_sitter::Tree>,
                                        Option<tree_sitter::InputEdit>,
                                    ) = if syntax.injections_query.is_some() {
                                        (syntax.tree.clone(), syntax.single_pending_edit())
                                    } else {
                                        (None, None)
                                    };
                                    syntax.update_from_result(*result);
                                    // The background job only parses the host grammar, so
                                    // re-derive injections here from the live source.
                                    if syntax.injections_query.is_some() {
                                        let source = doc.buffer.to_logical_bytes();
                                        syntax.parse_injections_pub(
                                            &source,
                                            old_host_tree.as_ref(),
                                            edit,
                                        );
                                    }
                                }
                            }
                        }

                        if let Some(entry) = self.pending_syntax_reparse.get_mut(&doc_id) {
                            if entry.in_flight_job == Some(id) {
                                entry.in_flight_job = None;
                            }
                            if entry.debounce_deadline.is_none() && entry.in_flight_job.is_none() {
                                self.pending_syntax_reparse.remove(&doc_id);
                            }
                        }

                        // The buffer moved on while this job ran; reparse the current
                        // content instead of leaving highlights permanently stale.
                        if !is_current {
                            self.debounce_syntax_reparse(doc_id);
                        }

                        // Re-render after syntax update; always use update_and_render so that
                        // the display map (soft-wrap) is rebuilt correctly.
                        self.update_and_render()?;
                    }
                    Err(any_payload) => {
                        // Try CompletionPayload
                        let any_payload = match any_payload
                            .downcast::<crate::job_manager::jobs::completion::CompletionPayload>()
                        {
                            Ok(payload) => {
                                self.handle_completion_result(*payload);
                                return Ok(());
                            }
                            Err(p) => p,
                        };

                        // Try ByteLineMap (CacheWarmingJob)
                        if let Ok(map) =
                            any_payload.downcast::<crate::buffer::byte_map::ByteLineMap>()
                        {
                            if let Some(doc) = self.document_manager.active_document_mut() {
                                if doc.buffer.revision == map.revision {
                                    *doc.buffer.byte_map_cache.borrow_mut() = Some(*map);
                                    if self.state.debug_mode {
                                        self.state.notify(
                                            crate::notification::NotificationType::Info,
                                            "Search cache warmed".to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // Without tree-sitter, no job ever produces a SyntaxParseResult;
                // go straight to the rest of the downcast chain.
                #[cfg(not(feature = "treesitter"))]
                {
                    // Try CompletionPayload
                    let any_payload = match any_payload
                        .downcast::<crate::job_manager::jobs::completion::CompletionPayload>(
                    ) {
                        Ok(payload) => {
                            self.handle_completion_result(*payload);
                            return Ok(());
                        }
                        Err(p) => p,
                    };

                    // Try ByteLineMap (CacheWarmingJob)
                    if let Ok(map) = any_payload.downcast::<crate::buffer::byte_map::ByteLineMap>()
                    {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            if doc.buffer.revision == map.revision {
                                *doc.buffer.byte_map_cache.borrow_mut() = Some(*map);
                                if self.state.debug_mode {
                                    self.state.notify(
                                        crate::notification::NotificationType::Info,
                                        "Search cache warmed".to_string(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            JobMessage::TerminalOutput(doc_id, data) => {
                if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                    doc.handle_terminal_data(&data);
                }
                if !self.split_tree.windows_for_document(doc_id).is_empty() {
                    let _ = self.update_and_render();
                }
            }
            JobMessage::TerminalExit(doc_id) => {
                // Switch back to Normal mode if this is the active terminal
                if self.active_document_id() == doc_id {
                    self.set_mode(Mode::Normal);
                }
                // Collect split windows showing this terminal before removing the doc
                let affected_windows = self.split_tree.windows_for_document(doc_id);

                // Force-remove the terminal buffer (skips dirty check)
                match self.document_manager.remove_document_force(doc_id) {
                    Err(e) => {
                        self.state.notify(
                            crate::notification::NotificationType::Error,
                            format!("Failed to close terminal: {}", e),
                        );
                    }
                    Ok(()) => {
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            "Terminal closed".to_string(),
                        );
                        // Close each split showing this terminal; reassign if it's the last window.
                        let new_doc_id = self.document_manager.active_document_id().unwrap_or(1);
                        for window_id in affected_windows {
                            if !self.split_tree.close_window(window_id) {
                                if let Some(w) = self.split_tree.get_window_mut(window_id) {
                                    w.document_id = new_doc_id;
                                }
                            }
                        }
                    }
                }
                self.sync_state_with_active_document();
                let _ = self.update_and_render();
            }
        }

        // Periodic cleanup of finished jobs
        self.job_manager.cleanup_finished_jobs();
        Ok(())
    }
}
