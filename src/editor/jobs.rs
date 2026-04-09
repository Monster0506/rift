#[allow(unused_imports)]
use crate::term::TerminalBackend;
use super::Editor;
use crate::error::RiftError;
use crate::mode::Mode;
use std::sync::Arc;

impl<T: TerminalBackend> Editor<T> {
    pub(super) fn spawn_syntax_parse_job(&mut self, doc_id: crate::document::DocumentId) {
        use crate::job_manager::jobs::syntax::SyntaxParseJob;
        use tree_sitter::Parser;

        if let Some(doc) = self.document_manager.get_document(doc_id) {
            if let Some(syntax) = &doc.syntax {
                // Create parser
                let mut parser = Parser::new();
                if parser.set_language(&syntax.language).is_err() {
                    return;
                }

                let job = SyntaxParseJob::new(
                    doc.buffer.clone(),
                    parser,
                    doc.syntax.as_ref().and_then(|s| s.tree.clone()),
                    doc.syntax.as_ref().and_then(|s| s.highlights_query.clone()),
                    doc.syntax
                        .as_ref()
                        .map(|s| s.language_name.clone())
                        .unwrap_or_default(),
                    doc_id,
                );

                self.job_manager.spawn(job);
            }
        }
    }

    /// Handle a message from a background job
    pub(super) fn handle_job_message(&mut self, msg: crate::job_manager::JobMessage) -> Result<(), RiftError> {
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
                            })
                            .collect();
                        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                            // Discard stale results if the doc has navigated to a different path
                            let path_matches = matches!(&doc.kind,
                                crate::document::BufferKind::Directory { path, .. }
                                if *path == listing.path);
                            if path_matches {
                                doc.populate_directory_buffer(entries);
                                self.sync_state_with_active_document();
                                let _ = self.force_full_redraw();
                                self.update_explorer_preview();
                            }
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
                        let preview_doc_id = res.right_doc_id;
                        let preview_path = res.path.clone();
                        let is_file_preview = res.dir_entries.is_none();
                        if let Some(doc) = self.document_manager.get_document_mut(preview_doc_id) {
                            let _ = doc.buffer.set_cursor(0);
                            let len = doc.buffer.len();
                            for _ in 0..len {
                                doc.buffer.delete_forward();
                            }
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

                        if is_file_preview {
                            if let Ok(loaded) = self.language_loader.load_language_for_file(&preview_path) {
                                let highlights = self
                                    .language_loader
                                    .load_query(&loaded.name, "highlights")
                                    .ok()
                                    .and_then(|src| tree_sitter::Query::new(&loaded.language, &src).ok())
                                    .map(Arc::new);
                                if let Ok(syntax) = crate::syntax::Syntax::new(loaded, highlights) {
                                    if let Some(doc) = self.document_manager.get_document_mut(preview_doc_id) {
                                        doc.set_syntax(syntax);
                                        // Synchronously parse so highlights are ready for the
                                        // immediately following render (no async timing gap).
                                        let source = doc.buffer.to_logical_bytes();
                                        if let Some(s) = &mut doc.syntax {
                                            s.incremental_parse(&source);
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

                        if self.pending_quit_job_id == Some(id) {
                            self.should_quit = true;
                        }

                        // Sync state and redraw to update dirty indicator
                        self.sync_state_with_active_document();
                        let _ = self.force_full_redraw();

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

                                if let Ok(syntax) = crate::syntax::Syntax::new(loaded, highlights) {
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

                        self.sync_state_with_active_document();
                        let _ = self.force_full_redraw();

                        self.job_manager
                            .update_job_state(&JobMessage::Finished(id, true));
                        return Ok(());
                    }
                    Err(p) => p,
                };

                match any_payload.downcast::<SyntaxParseResult>() {
                    Ok(result) => {
                        let doc_id = result.document_id;
                        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                            if let Some(syntax) = &mut doc.syntax {
                                syntax.update_from_result(*result);
                            }
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
            }
            JobMessage::TerminalOutput(doc_id, data) => {
                if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                    doc.handle_terminal_data(&data);
                    // Trigger redraw if this is the active document
                    if self.active_document_id() == doc_id {
                        let _ = self.update_and_render();
                    }
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
