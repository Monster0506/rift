use super::Editor;
use crate::document::DocumentId;
use crate::error::RiftError;
use crate::job_manager::AsyncToken;
use crate::job_manager::{JobMessage, JobPayload, JobRef, JobTarget};
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

pub(super) struct FileLoad {
    pub(super) token: AsyncToken,
    revision: u64,
    #[cfg(feature = "lsp")]
    goto: Option<(usize, u32)>,
}

impl FileLoad {
    pub(super) fn new(token: AsyncToken, revision: u64) -> Self {
        Self {
            token,
            revision,
            #[cfg(feature = "lsp")]
            goto: None,
        }
    }
}

impl<T: TerminalBackend> Editor<T> {
    #[cfg(feature = "treesitter")]
    fn spawn_syntax_parse_job(&mut self, doc_id: crate::document::DocumentId) -> Option<usize> {
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
    pub(super) fn drain_syntax_sync(&mut self) {
        use crate::document::syntax_sync::SyntaxNotice;
        for (doc_id, notice) in self.document_manager.take_syntax_notices() {
            self.cancel_pending_syntax_reparse(doc_id);
            match notice {
                #[cfg(feature = "treesitter")]
                SyntaxNotice::Completed => {
                    #[cfg(feature = "plugins")]
                    self.raise_syntax_updated(doc_id);
                }
                SyntaxNotice::Deferred => self
                    .scheduler
                    .syntax_reparse
                    .arm(doc_id, crate::time::Instant::now()),
            }
        }
    }

    fn cancel_pending_syntax_reparse(&mut self, doc_id: DocumentId) {
        if let Some(job_id) = self.scheduler.syntax_reparse.cancel(doc_id) {
            self.job_manager.cancel_job(job_id);
        }
    }

    #[cfg(feature = "treesitter")]
    pub(super) fn attach_syntax_for_document(&mut self, doc_id: DocumentId) -> bool {
        let Some(doc) = self.document_manager.get_document_mut(doc_id) else {
            return false;
        };
        let Some(loaded) = doc
            .path()
            .and_then(|path| self.language_loader.load_language_for_file(path).ok())
        else {
            return false;
        };
        doc.set_syntax(syntax_for(&self.language_loader, loaded));
        true
    }

    #[cfg(all(feature = "treesitter", feature = "plugins"))]
    pub(super) fn start_syntax(&mut self, doc_id: DocumentId, lang: Option<&str>) {
        if let Some(lang) = lang {
            let Ok(loaded) = self.language_loader.load_language(lang) else {
                self.notify(
                    crate::notification::NotificationType::Error,
                    format!("treesitter: unknown language '{lang}'"),
                );
                return;
            };
            if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                doc.set_syntax(syntax_for(&self.language_loader, loaded));
            }
        } else if !self.attach_syntax_for_document(doc_id) {
            if let Some(doc) = self
                .document_manager
                .get_document_mut(doc_id)
                .filter(|doc| doc.syntax.is_some())
            {
                doc.reparse_syntax();
            } else {
                self.notify(
                    crate::notification::NotificationType::Warning,
                    "treesitter: no grammar matches this buffer",
                );
            }
        }
        self.drain_syntax_sync();
    }

    #[cfg(feature = "treesitter")]
    #[cfg(feature = "plugins")]
    pub(super) fn stop_syntax(&mut self, doc_id: DocumentId) {
        self.cancel_pending_syntax_reparse(doc_id);
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            if doc.syntax.take().is_some() {
                doc.take_syntax_notice();
                #[cfg(feature = "plugins")]
                self.raise_syntax_updated(doc_id);
            }
        }
    }

    #[cfg(feature = "treesitter")]
    #[cfg(feature = "plugins")]
    pub(super) fn set_syntax_query(
        &mut self,
        lang: &str,
        kind: crate::syntax::loader::QueryKind,
        source: Option<String>,
    ) {
        self.language_loader.set_query_override(lang, kind, source);
        self.rebuild_syntax_using(lang);
    }

    #[cfg(feature = "treesitter")]
    #[cfg(feature = "plugins")]
    fn rebuild_syntax_using(&mut self, lang: &str) {
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

    #[cfg(feature = "lsp")]
    pub(super) fn defer_file_load_goto(
        &mut self,
        handle: crate::document::runtime::DocumentHandle,
        line: usize,
        col_units: u32,
    ) -> bool {
        let Some((_, load)) = self.file_load_jobs.iter_mut().find(|(id, load)| {
            load.token.handle == handle
                && self.job_manager.is_job_running(**id)
                && self.job_manager.is_token_current(&load.token)
                && self
                    .document_manager
                    .get_document_by_handle(handle)
                    .is_some_and(|doc| doc.matches_kind(load.token.expected_kind))
        }) else {
            return false;
        };
        load.goto = Some((line, col_units));
        true
    }

    fn take_file_load(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        doc_id: DocumentId,
        is_reload: bool,
    ) -> Option<FileLoad> {
        let load = self.file_load_jobs.remove(&id).filter(|load| {
            self.job_manager.is_job_running(id)
                && load.token.doc_id() == doc_id
                && token.is_none_or(|token| token == load.token)
                && self.is_async_token_valid(&load.token)
        })?;
        if self
            .document_manager
            .get_document_by_handle(load.token.handle)
            .is_some_and(|doc| doc.version() == load.revision)
        {
            Some(load)
        } else {
            self.notify(
                crate::notification::NotificationType::Warning,
                "File load ignored: buffer changed during load",
            );
            if !is_reload {
                self.announce_buffer(doc_id);
            }
            None
        }
    }

    pub(super) fn finish_history_syntax(&mut self, doc_id: DocumentId) {
        self.drain_syntax_sync();
        if self.scheduler.syntax_reparse.is_armed(doc_id) {
            self.run_syntax_reparse(doc_id);
        }
    }

    fn run_syntax_reparse(&mut self, doc_id: DocumentId) {
        self.cancel_pending_syntax_reparse(doc_id);
        #[cfg(feature = "treesitter")]
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
            self.run_syntax_reparse(doc_id);
        }
    }

    fn is_async_token_valid(&self, token: &AsyncToken) -> bool {
        if !self.job_manager.is_token_current(token) {
            return false;
        }
        let Some(doc) = self.document_manager.get_document_by_handle(token.handle) else {
            return false;
        };
        doc.matches_kind(token.expected_kind)
    }

    pub(super) fn resolve_and_validate_token(
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
                .register_document_handle(doc.handle(), doc.buffer_kind_id());
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
                self.file_load_jobs.remove(&id);
                let name = self.job_manager.job_name(id);
                self.state.notifications.log_job_event(
                    id,
                    crate::notification::JobEventKind::Finished,
                    silent,
                    format!("{name}: finished"),
                );
                self.settle_file_save(id);
            }
            JobMessage::Error(id, err) => {
                self.file_load_jobs.remove(&id);
                if self.scheduler.running_file_saves.contains_key(&id)
                    || self
                        .job_manager
                        .job_token(id)
                        .is_none_or(|token| self.is_async_token_valid(&token))
                {
                    self.handle_job_error(id, &err);
                }
                self.settle_file_save(id);
            }
            JobMessage::Cancelled(id) => {
                self.file_load_jobs.remove(&id);
                if self.scheduler.running_file_saves.contains_key(&id)
                    || self
                        .job_manager
                        .job_token(id)
                        .is_none_or(|token| self.is_async_token_valid(&token))
                {
                    self.handle_job_cancelled(id);
                }
                self.settle_file_save(id);
            }
            JobMessage::CustomToken(id, token, payload) => {
                self.apply_job_payload(id, Some(token), payload);
            }
            JobMessage::Custom(id, payload) => {
                let token = self.job_manager.job_token(id);
                self.apply_job_payload(id, token, payload);
            }
        }

        self.job_manager.cleanup_finished_jobs();
    }

    fn apply_job_payload(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        payload: Box<dyn JobPayload>,
    ) {
        if self.scheduler.running_file_saves.contains_key(&id)
            || token.is_none_or(|token| self.is_async_token_valid(&token))
        {
            payload.apply(self, JobRef { id, token });
        } else {
            self.file_load_jobs.remove(&id);
        }
    }

    fn handle_job_error(&mut self, id: usize, err: &RiftError) {
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
    }
}

#[cfg(not(feature = "lsp"))]
impl<T: TerminalBackend> crate::job_manager::LspTarget for Editor<T> {}

impl<T: TerminalBackend> JobTarget for Editor<T> {
    #[cfg(feature = "plugins")]
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
            self.scheduler.terminal_frame_dirty = true;
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
        let pending = doc
            .parse_directory_diff()
            .without(&doc.take_directory_applied());
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
        if !pending.is_empty() {
            let fs = crate::fs_backend::backend();
            doc.rebase_directory_edits(&pending, |path| fs.path_exists(path));
        }

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
                let _ = doc.set_cursor(pos, crate::document::ops::CursorCause::Programmatic);
            }
        }
        self.update_explorer_preview();
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_git(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::git::GitResult,
    ) {
        self.finish_git_job(id, token, result);
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
            doc.convert_to_file_preview();
            doc.set_path(&preview_path);
            doc.replace_content(&text, crate::document::ops::ContentKind::Load);
        }

        #[cfg(feature = "treesitter")]
        if is_file_preview {
            self.attach_syntax_for_document(res.right_doc_id);
        }

        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }

    fn apply_file_save(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::file_operations::FileSaveResult,
    ) {
        self.finish_file_save(id, token, res);
    }

    fn apply_file_load(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::file_operations::FileLoadResult,
    ) {
        let Some(load) = self.take_file_load(id, token, res.document_id, res.is_reload) else {
            return;
        };
        let undoable = res.is_reload
            && self
                .document_manager
                .get_document_by_handle(load.token.handle)
                .is_some_and(|doc| doc.image().is_none());
        if !undoable {
            self.preserve_file_save_history(load.token.handle);
        }
        let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(load.token.handle)
        else {
            return;
        };

        if undoable {
            doc.reload_content(res.line_index);
        } else {
            doc.replace_content(res.line_index, crate::document::ops::ContentKind::Load);
        }
        let _ = doc.set_option("line_ending", Some(res.line_ending.name()));
        let _ = doc.set_option("bomb", Some(if res.has_bom { "true" } else { "false" }));
        #[cfg(feature = "treesitter")]
        let needs_syntax = !undoable || doc.syntax.is_none();
        if !undoable {
            #[cfg(feature = "lsp")]
            if res.is_reload {
                doc.mark_lsp_full_sync();
            }
            self.restore_persisted_undo(res.document_id, res.content_hash);
        }

        if res.is_reload {
            #[cfg(feature = "treesitter")]
            if needs_syntax {
                self.attach_syntax_for_document(res.document_id);
            }
            #[cfg(feature = "plugins")]
            self.raise(crate::plugin::events::EventPayload::BufReload {
                buf: res.document_id,
            });
        } else {
            self.announce_buffer(res.document_id);
        }
        #[cfg(feature = "lsp")]
        if let Some((line, col_units)) = load.goto {
            self.place_lsp_goto(load.token.handle, line, col_units);
        }
    }

    fn apply_image_load(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        res: crate::job_manager::jobs::file_operations::ImageLoadResult,
    ) {
        let Some(load) = self.take_file_load(id, token, res.document_id, res.is_reload) else {
            return;
        };
        #[cfg(feature = "lsp")]
        {
            // `:e` from raw bytes: the text buffer may be open with a server.
            if res.is_reload {
                self.lsp_notify_close(res.document_id);
            }
        }
        self.preserve_file_save_history(load.token.handle);
        if let Some(doc) = self
            .document_manager
            .get_document_by_handle_mut(load.token.handle)
        {
            doc.convert_to_image_view(res.image);
        }

        if res.is_reload {
            #[cfg(feature = "plugins")]
            self.raise(crate::plugin::events::EventPayload::BufReload {
                buf: res.document_id,
            });
        } else {
            self.announce_buffer(res.document_id);
        }
    }

    #[cfg(feature = "treesitter")]
    fn apply_syntax_parse(
        &mut self,
        id: usize,
        token: Option<AsyncToken>,
        result: crate::job_manager::jobs::syntax::SyntaxParseResult,
    ) {
        let doc_id = result.document_id;
        if self.scheduler.syntax_reparse.running(doc_id) == Some(id) {
            self.scheduler.syntax_reparse.finish(doc_id, id);
            let token = self.resolve_and_validate_token(
                id,
                token,
                doc_id,
                crate::job_manager::AsyncOpDomain::SyntaxParse,
            );
            let applied = token
                .and_then(|token| {
                    self.document_manager
                        .get_document_by_handle_mut(token.handle)
                })
                .filter(|doc| doc.buffer().revision() == result.revision)
                .and_then(|doc| {
                    doc.with_syntax(|syntax, buffer| syntax.update_from_result(result, buffer))
                })
                .is_some();
            if applied {
                #[cfg(feature = "plugins")]
                self.raise_syntax_updated(doc_id);
                self.request_frame();
            }
        }
        self.job_manager
            .update_job_state(&JobMessage::Finished(id, true));
    }
}
