use super::Editor;
#[cfg(feature = "lsp")]
use crate::buffer::api::BufferView;
use crate::document::DocumentId;
use crate::error::RiftError;
use crate::job_manager::AsyncToken;
use crate::job_manager::{JobMessage, JobRef, JobTarget};
use crate::term::TerminalBackend;
#[cfg(feature = "treesitter")]
use std::sync::Arc;

#[cfg(feature = "treesitter")]
fn syntax_for(
    loader: &Arc<crate::syntax::loader::LanguageLoader>,
    lang: crate::syntax::loader::LoadedLanguage,
) -> crate::syntax::Syntax {
    let highlights = loader
        .load_query(&lang.name, "highlights")
        .ok()
        .and_then(|source| tree_sitter::Query::new(&lang.language, &source).ok())
        .map(Arc::new);
    crate::syntax::build_syntax(lang, highlights, loader)
}

#[cfg(feature = "treesitter")]
fn attach_syntax(
    docs: &mut crate::document::manager::DocumentManager,
    loader: &std::sync::Arc<crate::syntax::loader::LanguageLoader>,
    doc_id: DocumentId,
) {
    let Some(path) = docs
        .get_document(doc_id)
        .and_then(|d| d.path())
        .map(std::path::Path::to_path_buf)
    else {
        return;
    };
    let Ok(lang) = loader.load_language_for_file(&path) else {
        return;
    };
    let syntax = syntax_for(loader, lang);
    if let Some(doc) = docs.get_document_mut(doc_id) {
        doc.set_syntax(syntax);
    }
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
                    bytes: Some(u32::try_from(doc.buffer().byte_len()).unwrap_or(u32::MAX)),
                    ..Default::default()
                }
            );
            doc.buffer().clone()
        };

        let token = self.job_manager.next_token(
            doc.handle(),
            doc.buffer_kind_id(),
            crate::job_manager::AsyncOpDomain::SyntaxParse,
        );

        let job = SyntaxParseJob::new(
            buffer,
            parser,
            syntax.tree.clone(),
            syntax.language_name.clone(),
            doc_id,
            doc.buffer().revision(),
        )
        .with_token(token)
        .with_lib(syntax.lib());

        Some(self.job_manager.spawn_with_token(job, token))
    }
    #[cfg(not(feature = "treesitter"))]
    // Same signature as the treesitter build.
    pub(super) fn spawn_syntax_parse_job(
        &mut self,
        doc_id: crate::document::DocumentId,
    ) -> Option<usize> {
        self.cancel_pending_syntax_reparse(doc_id);
        None
    }

    pub(super) fn debounce_syntax_reparse(&mut self, doc_id: DocumentId) {
        self.scheduler
            .syntax_reparse
            .arm(doc_id, crate::time::Instant::now());
    }

    pub(super) fn cancel_pending_syntax_reparse(&mut self, doc_id: DocumentId) {
        if let Some(job_id) = self.scheduler.syntax_reparse.cancel(doc_id) {
            self.job_manager.cancel_job(job_id);
        }
    }

    #[cfg(feature = "treesitter")]
    pub(super) fn attach_syntax_for_document(&mut self, doc_id: DocumentId) {
        attach_syntax(&mut self.document_manager, &self.language_loader, doc_id);
    }

    #[cfg(feature = "treesitter")]
    pub(super) fn start_syntax(&mut self, doc_id: DocumentId, lang: Option<&str>) {
        self.cancel_pending_syntax_reparse(doc_id);
        match lang {
            Some(lang) => {
                let Ok(loaded) = self.language_loader.load_language(lang) else {
                    self.notify(
                        crate::notification::NotificationType::Error,
                        format!("treesitter: unknown language '{lang}'"),
                    );
                    return;
                };
                let syntax = syntax_for(&self.language_loader, loaded);
                if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                    doc.set_syntax(syntax);
                }
            }
            None => attach_syntax(&mut self.document_manager, &self.language_loader, doc_id),
        }
        if self
            .document_manager
            .get_document(doc_id)
            .is_some_and(|d| d.syntax.is_some())
        {
            self.reparse_syntax(doc_id);
        } else {
            self.notify(
                crate::notification::NotificationType::Warning,
                "treesitter: no grammar matches this buffer".to_string(),
            );
        }
    }

    #[cfg(feature = "treesitter")]
    pub(super) fn stop_syntax(&mut self, doc_id: DocumentId) {
        self.cancel_pending_syntax_reparse(doc_id);
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            if doc.syntax.take().is_some() {
                self.raise_syntax_updated(doc_id);
            }
        }
    }

    #[cfg(feature = "treesitter")]
    pub(super) fn rebuild_syntax_using(&mut self, lang: &str) {
        let targets: Vec<(DocumentId, String)> = self
            .document_manager
            .documents_iter()
            .filter_map(|doc| {
                let syntax = doc.syntax.as_ref()?;
                syntax
                    .uses_language(lang)
                    .then(|| (doc.id, syntax.language_name.clone()))
            })
            .collect();
        for (doc_id, host) in targets {
            self.start_syntax(doc_id, Some(&host));
        }
    }

    fn on_file_load_failed(&mut self, job_id: usize) {
        let Some(doc_id) = self.file_load_jobs.remove(&job_id) else {
            return;
        };
        #[cfg(feature = "lsp")]
        if self.scheduler.goto_after_load.map(|t| t.0) == Some(doc_id) {
            self.scheduler.goto_after_load = None;
        }
        #[cfg(not(feature = "lsp"))]
        let _ = doc_id;
    }

    pub(super) fn spawn_syntax_parse_job_immediate(&mut self, doc_id: DocumentId) {
        self.cancel_pending_syntax_reparse(doc_id);
        if let Some(job_id) = self.spawn_syntax_parse_job(doc_id) {
            self.scheduler.syntax_reparse.start(doc_id, job_id);
        }
    }

    pub(super) fn poll_pending_syntax_reparse(&mut self) {
        for doc_id in self
            .scheduler
            .syntax_reparse
            .due(crate::time::Instant::now())
        {
            self.spawn_syntax_parse_job_immediate(doc_id);
        }
    }

    fn is_async_token_valid(&self, token: &AsyncToken) -> bool {
        if !self.job_manager.is_token_current(token) {
            return false;
        }
        let Some(doc) = self.document_manager.get_document_by_handle(token.handle) else {
            return false;
        };
        if !doc.matches_kind(token.expected_kind) {
            return false;
        }
        if let Some(owner) = token.owner {
            if !self.plugin_host.is_generation_active(owner) {
                return false;
            }
        }
        true
    }

    fn resolve_and_validate_token(
        &self,
        job_id: usize,
        token: Option<AsyncToken>,
        doc_id: DocumentId,
        expected_domain: crate::job_manager::AsyncOpDomain,
    ) -> Option<AsyncToken> {
        if let Some(token) = token.or_else(|| self.job_manager.job_token(job_id)) {
            if token.doc_id() != doc_id || token.domain != expected_domain {
                return None;
            }
            if !self.is_async_token_valid(&token) {
                return None;
            }
            return Some(token);
        }
        let doc = self.document_manager.get_document(doc_id)?;
        Some(AsyncToken::with_domain(
            doc.handle(),
            doc.buffer_kind_id(),
            0,
            expected_domain,
        ))
    }

    pub(super) fn handle_job_message(&mut self, msg: crate::job_manager::JobMessage) {
        for doc in self.document_manager.documents_iter() {
            self.job_manager
                .register_document_handle(doc.handle(), doc.buffer_kind_id(), None);
        }

        self.job_manager.update_job_state(&msg);
        match msg {
            JobMessage::Started(id, silent) => {
                let name = self.job_manager.job_name(id);
                self.state.notifications.log_job_event(
                    id,
                    crate::notification::JobEventKind::Started,
                    silent,
                    format!("{name}: started"),
                );
            }
            JobMessage::Progress(id, percentage, msg) => {
                let silent = self.job_manager.is_job_silent(id);
                let name = self.job_manager.job_name(id);
                self.state.notifications.log_job_event(
                    id,
                    crate::notification::JobEventKind::Progress(percentage),
                    silent,
                    format!("{name}: {msg}"),
                );
            }
            JobMessage::Finished(id, silent) => {
                let name = self.job_manager.job_name(id);
                self.state.notifications.log_job_event(
                    id,
                    crate::notification::JobEventKind::Finished,
                    silent,
                    format!("{name}: finished"),
                );
            }
            JobMessage::Error(id, err) => {
                if let Some(t) = self.job_manager.job_token(id) {
                    if !self.is_async_token_valid(&t) {
                        return;
                    }
                }
                self.handle_job_error(id, &err);
            }
            JobMessage::Cancelled(id) => {
                if let Some(t) = self.job_manager.job_token(id) {
                    if !self.is_async_token_valid(&t) {
                        return;
                    }
                }
                self.handle_job_cancelled(id);
            }
            JobMessage::CustomToken(id, token, payload) => {
                if !self.is_async_token_valid(&token) {
                    return;
                }
                payload.apply(
                    self,
                    JobRef {
                        id,
                        token: Some(token),
                    },
                );
            }
            JobMessage::Custom(id, payload) => {
                let token = self.job_manager.job_token(id);
                if let Some(t) = token {
                    if !self.is_async_token_valid(&t) {
                        return;
                    }
                }
                payload.apply(self, JobRef { id, token });
            }
        }

        self.job_manager.cleanup_finished_jobs();
    }

    fn handle_job_error(&mut self, id: usize, err: &RiftError) {
        self.on_file_load_failed(id);
        let silent = self.job_manager.is_job_silent(id);
        let name = self.job_manager.job_name(id);
        self.state.notifications.log_job_event(
            id,
            crate::notification::JobEventKind::Error,
            silent,
            format!("{name}: {err}"),
        );
        self.notify(
            crate::notification::NotificationType::Error,
            format!("{name} failed: {err}"),
        );
    }

    fn handle_job_cancelled(&mut self, id: usize) {
        let silent = self.job_manager.is_job_silent(id);
        let name = self.job_manager.job_name(id);
        self.state.notifications.log_job_event(
            id,
            crate::notification::JobEventKind::Cancelled,
            silent,
            format!("{name}: cancelled"),
        );
        self.notify(
            crate::notification::NotificationType::Warning,
            format!("{name} cancelled"),
        );
        self.on_file_load_failed(id);

        if self.scheduler.quit_after_job == Some(id) {
            self.scheduler.quit_after_job = None;
            self.notify(
                crate::notification::NotificationType::Warning,
                "Quit aborted: save was cancelled".to_string(),
            );
        }
    }

    #[cfg(feature = "treesitter")]
    fn attach_preview_syntax(
        &mut self,
        token: &AsyncToken,
        preview_doc_id: DocumentId,
        preview_path: &std::path::Path,
    ) {
        let Ok(loaded) = self.language_loader.load_language_for_file(preview_path) else {
            return;
        };
        let syntax = syntax_for(&self.language_loader, loaded);
        if let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        {
            doc.set_syntax(syntax);
            if doc.buffer().byte_len() <= crate::syntax::SYNC_PARSE_MAX_BYTES {
                doc.with_syntax(crate::syntax::Syntax::incremental_parse);
            }
        }
        self.spawn_syntax_parse_job(preview_doc_id);
    }

    #[cfg(feature = "lsp")]
    fn apply_pending_goto(&mut self, token: &AsyncToken, document_id: DocumentId) {
        let Some((goto_doc, goto_line, goto_col)) = self.scheduler.goto_after_load.take() else {
            return;
        };
        if goto_doc != document_id {
            if self.document_manager.get_document(goto_doc).is_some() {
                self.scheduler.goto_after_load = Some((goto_doc, goto_line, goto_col));
            }
            return;
        }
        let doc_path = self
            .document_manager
            .get_document_by_handle(token.handle)
            .and_then(|doc| doc.path().map(std::path::Path::to_path_buf));
        let encoding = doc_path
            .as_deref()
            .map_or_default(|p| self.lsp_manager.position_encoding_for_path(p));
        if let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        {
            let goto_line = goto_line.min(doc.buffer().last_real_line());
            let char_col = doc.lsp_char_offset_in_line(
                goto_line,
                u32::try_from(goto_col).unwrap_or(u32::MAX),
                encoding,
            );
            let line_offset = doc.buffer().line_start(goto_line);
            let target = (line_offset + char_col).min(doc.buffer().len());
            let _ = doc.set_cursor(target, crate::document::ops::CursorCause::Jump);
        }
    }
}

impl<T: TerminalBackend> JobTarget for Editor<T> {
    fn apply_plugin_job(
        &mut self,
        payload: crate::job_manager::jobs::plugin_process::PluginJobPayload,
    ) {
        self.deliver_plugin_job_payload(payload);
    }

    #[cfg(feature = "terminal_emulation")]
    fn apply_terminal_output(&mut self, token: AsyncToken) {
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            return;
        };
        doc.handle_terminal_data(&[]);
        if !self
            .split_tree
            .windows_for_document(token.doc_id())
            .is_empty()
        {
            self.terminal_frame_pending = true;
        }
    }

    #[cfg(feature = "terminal_emulation")]
    fn apply_terminal_exit(&mut self, token: AsyncToken) {
        let doc_id = token.doc_id();
        if self.active_document_id() == doc_id {
            self.set_mode(crate::mode::Mode::Normal);
        }
        if let Err(e) = self.close_buffer(doc_id, crate::document::manager::RemovalIntent::Force) {
            self.notify(
                crate::notification::NotificationType::Error,
                format!("Failed to close terminal: {e}"),
            );
        } else {
            self.notify(
                crate::notification::NotificationType::Info,
                "Terminal closed".to_string(),
            );
        }
        self.sync_state_with_active_document();
        let _ = self.update_and_render();
    }

    fn apply_completion(
        &mut self,
        payload: crate::job_manager::jobs::completion::CompletionPayload,
    ) {
        self.handle_completion_result(payload);
    }

    fn apply_directory_listing(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        listing: crate::job_manager::jobs::explorer::DirectoryListing,
    ) {
        let doc_id = listing.doc_id;
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::DirectoryListing,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let path_matches = doc.directory_path() == Some(&listing.path);
        if !path_matches {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        }
        let entries: Vec<crate::document::DirEntry> = listing
            .entries
            .iter()
            .map(|e| crate::document::DirEntry {
                path: e.path.clone(),
                is_dir: e.is_dir,
                id: 0,
            })
            .collect();
        doc.populate_directory_buffer(entries);

        if let Some(target_name) = self.scheduler.explorer_cursor_entry.take() {
            let line_pos = doc
                .annotations
                .directory_entries_by_line()
                .into_iter()
                .find_map(|(line, eid)| {
                    doc.directory_entries().and_then(|entries| {
                        entries.iter().find(|e| e.id == eid).and_then(|e| {
                            e.path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .filter(|n| *n == target_name)
                                .map(|_| doc.buffer().line_index.get_start(line).unwrap_or(0))
                        })
                    })
                });
            if let Some(pos) = line_pos {
                let _ = doc.set_cursor(pos, crate::document::ops::CursorCause::User);
            }
        }
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        self.update_explorer_preview();
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git_status(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::git::GitStatusResult,
    ) {
        let doc_id = result.doc_id;
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::GitStatus,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let root_matches =
            doc.is_git_status() && doc.git_repo_root() == Some(result.repo_root.as_path());
        if !root_matches {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        }
        doc.populate_git_status_buffer(result.snapshot, result.head_subject);
        if let Some(staged_side) = self.scheduler.git_status_expand_all.remove(&doc_id) {
            self.spawn_expand_all_git_status_hunks(doc_id, staged_side);
        }
        if self.active_document_id() == doc_id {
            self.sync_state_with_active_document();
            let _ = self.force_full_redraw();
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git_diff(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::git::GitDiffResult,
    ) {
        let doc_id = result.doc_id;
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::GitDiff,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        if doc.is_git_status() {
            doc.set_git_status_expanded(result.path, result.staged_side, result.hunks);
        }
        if self.active_document_id() == doc_id {
            self.sync_state_with_active_document();
            let _ = self.force_full_redraw();
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git_blame(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::git::GitBlameResult,
    ) {
        let doc_id = result.doc_id;
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::GitBlame,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        if doc.is_git_blame() {
            doc.populate_git_blame_buffer(result.lines);
        }
        if self.active_document_id() == doc_id {
            self.sync_state_with_active_document();
            let _ = self.force_full_redraw();
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git_log(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::git::GitLogResult,
    ) {
        let doc_id = result.doc_id;
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::GitLog,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        if doc.is_git_log() {
            doc.populate_git_log_buffer(result.commits);
        }
        if self.scheduler.git_log_expand_head.remove(&doc_id) {
            let head = if doc.is_git_log() {
                doc.git_repo_root().map(std::path::Path::to_path_buf).zip(
                    doc.git_log_commits()
                        .and_then(|c| c.first().map(|c| c.sha.clone())),
                )
            } else {
                None
            };
            if let Some((repo_root, sha)) = head {
                let show_token = self.job_manager.next_token(
                    token.handle,
                    token.expected_kind,
                    crate::job_manager::AsyncOpDomain::GitShow,
                );
                let job = crate::job_manager::jobs::git::GitShowJob::new(doc_id, repo_root, sha)
                    .with_token(show_token);
                self.job_manager.spawn_with_token(job, show_token);
            }
        }
        if self.active_document_id() == doc_id {
            self.sync_state_with_active_document();
            let _ = self.force_full_redraw();
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git_show(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::git::GitShowResult,
    ) {
        let doc_id = result.doc_id;
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::GitShow,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        if doc.is_git_log() {
            doc.set_git_log_expanded(Some(result.sha), Some(result.body));
        }
        if self.active_document_id() == doc_id {
            self.sync_state_with_active_document();
            let _ = self.force_full_redraw();
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git_command(
        &mut self,
        id: usize,
        result: crate::job_manager::jobs::git::GitCommandResult,
    ) {
        self.refresh_git_status_buffers_for(&result.repo_root);
        self.show_git_command_result(&result.args, &result.output, result.success);
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git_gutter(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::git::GitGutterDiffResult,
    ) {
        let doc_id = result.doc_id;
        self.scheduler.git_gutter_diff.finish(doc_id, id);
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::GitGutter,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        if doc.buffer().revision() == result.revision {
            doc.set_git_gutter_signs(&result.signs);
        }
        if self.active_document_id() == doc_id {
            let _ = self.force_full_redraw();
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_undo_tree(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::undotree::UndoTreeRenderResult,
    ) {
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            res.ut_doc_id,
            crate::job_manager::AsyncOpDomain::UndoTree,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(ut_doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        ut_doc.populate_undotree_buffer(&res.text, res.sequences, res.highlights);
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_explorer_preview(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::explorer_preview::ExplorerPreviewResult,
    ) {
        if self
            .scheduler
            .explorer_preview
            .as_ref()
            .is_some_and(|p| p.job_id == id)
        {
            self.scheduler.explorer_preview = None;
        }

        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            res.right_doc_id,
            crate::job_manager::AsyncOpDomain::ExplorerPreview,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };

        let current_target = self.current_explorer_target_path();
        if current_target.as_deref() != Some(res.path.as_path()) {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        }

        let preview_path = res.path.clone();
        #[cfg(feature = "treesitter")]
        let is_file_preview = res.file_text.is_some();
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        doc.replace_content("", crate::document::ops::ContentKind::Load);

        if let Some(entries) = res.dir_entries {
            doc.convert_to_directory(res.path.clone());
            doc.set_directory_show_hidden(false);
            doc.populate_directory_buffer(entries);
        } else if let Some(parsed) = res.undo_file {
            doc.convert_to_undo_file_view();
            doc.set_path(&preview_path);
            doc.populate_undo_file_view(&parsed);
        } else if let Some(image) = res.image {
            doc.set_path(&preview_path);
            doc.convert_to_image_view(image);
        } else if let Some(text) = res.file_text {
            doc.convert_to_file();
            doc.set_path(&preview_path);
            doc.replace_content(&text, crate::document::ops::ContentKind::Load);
        }

        #[cfg(feature = "treesitter")]
        if is_file_preview {
            self.attach_preview_syntax(&token, res.right_doc_id, &preview_path);
        }

        self.sync_state_with_active_document();
        let _ = self.update_and_render();
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_file_save(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::file_operations::FileSaveResult,
    ) {
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            res.document_id,
            crate::job_manager::AsyncOpDomain::FileSave,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };

        doc.mark_as_saved(res.saved_seq);
        doc.set_path(res.path.clone());

        let display_name = doc.display_name().to_string();
        self.persist_undo_after_save(res.document_id, res.content_hash);
        self.state.update_filename(display_name);

        self.notify(
            crate::notification::NotificationType::Success,
            format!("Written to {}", res.path.display()),
        );

        self.raise(crate::plugin::events::EventPayload::BufSavePost {
            buf: res.document_id,
            path: Some(res.path.clone()),
        });
        #[cfg(feature = "lsp")]
        self.lsp_manager.did_save(&res.path, None);

        if self.scheduler.quit_after_job == Some(id) {
            self.scheduler.quit_after_job = None;
            self.quit_after_save(res.document_id);
        }

        self.sync_state_with_active_document();
        let _ = self.update_and_render();

        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_file_load(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::file_operations::FileLoadResult,
    ) {
        self.file_load_jobs.remove(&id);
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            res.document_id,
            crate::job_manager::AsyncOpDomain::FileLoad,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };

        doc.replace_content(res.line_index, crate::document::ops::ContentKind::Load);
        doc.options.line_ending = res.line_ending;
        doc.options.has_bom = res.has_bom;
        if res.is_reload {
            doc.mark_lsp_full_sync();
        }
        self.restore_persisted_undo(res.document_id, res.content_hash);

        if res.is_reload {
            #[cfg(feature = "treesitter")]
            self.attach_syntax_for_document(res.document_id);
            self.spawn_syntax_parse_job(res.document_id);
            self.raise(crate::plugin::events::EventPayload::BufReload {
                buf: res.document_id,
            });
        } else {
            self.announce_buffer(res.document_id);
        }
        #[cfg(feature = "lsp")]
        self.apply_pending_goto(&token, res.document_id);

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();

        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_image_load(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::file_operations::ImageLoadResult,
    ) {
        self.file_load_jobs.remove(&id);
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            res.document_id,
            crate::job_manager::AsyncOpDomain::FileLoad,
        ) else {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        if self
            .document_manager
            .get_document_by_handle(token.handle)
            .is_none()
        {
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        }
        #[cfg(feature = "lsp")]
        {
            // `:e` from raw bytes: the text buffer may be open with a server.
            if res.is_reload {
                self.lsp_notify_close(res.document_id);
            }
            if self.scheduler.goto_after_load.map(|t| t.0) == Some(res.document_id) {
                self.scheduler.goto_after_load = None;
            }
        }
        if let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        {
            doc.convert_to_image_view(res.image);
        }

        if res.is_reload {
            self.raise(crate::plugin::events::EventPayload::BufReload {
                buf: res.document_id,
            });
        } else {
            self.announce_buffer(res.document_id);
        }

        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();

        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    #[cfg(feature = "treesitter")]
    fn apply_syntax_parse(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::syntax::SyntaxParseResult,
    ) {
        let doc_id = result.document_id;
        let Some(token) = self.resolve_and_validate_token(
            id,
            token,
            doc_id,
            crate::job_manager::AsyncOpDomain::SyntaxParse,
        ) else {
            self.scheduler.syntax_reparse.finish(doc_id, id);
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(token.handle)
        else {
            self.scheduler.syntax_reparse.finish(doc_id, id);
            self.job_manager
                .update_job_state(&JobMessage::Finished(id, true));
            return;
        };

        let is_current = doc.buffer().revision() == result.revision;

        if is_current
            && doc
                .with_syntax(|syntax, buffer| syntax.update_from_result(result, buffer))
                .is_some()
        {
            self.raise_syntax_updated(doc_id);
        }

        self.scheduler.syntax_reparse.finish(doc_id, id);

        if !is_current {
            self.debounce_syntax_reparse(doc_id);
        }

        if let Err(err) = self.update_and_render() {
            self.notify_error(err);
            return;
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }
}
