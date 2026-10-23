use super::Editor;
use super::{plugin_dirs, user_config_dir};
use crate::command_line::commands::CommandParser;
use crate::command_line::settings::create_settings_registry;
use crate::document::{Document, DocumentId};
use crate::dot_repeat::DotRepeat;
use crate::error::{ErrorType, RiftError};
use crate::keymap::KeyMap;
use crate::mode::Mode;
use crate::split::tree::SplitTree;
use crate::state::State;
use crate::term::TerminalBackend;
use std::sync::Arc;

/// Splits the startup argument into a directory target and an optional file to open.
fn startup_target(file_path: Option<&str>) -> (Option<std::path::PathBuf>, Option<&str>) {
    let target_dir = file_path
        .map(std::path::Path::new)
        .filter(|p| p.is_dir())
        .map(std::path::Path::to_path_buf);
    let effective_file_path = if target_dir.is_some() {
        None
    } else {
        file_path
    };
    (target_dir, effective_file_path)
}

fn startup_language_loader() -> Arc<crate::syntax::loader::LanguageLoader> {
    let grammar_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join(paths::GRAMMARS_DIR)))
        .unwrap_or_else(|| std::path::PathBuf::from(paths::GRAMMARS_DIR));
    Arc::new(crate::syntax::loader::LanguageLoader::new(grammar_dir))
}

fn open_startup_documents(
    effective_file_path: Option<&str>,
) -> Result<crate::document::DocumentManager, RiftError> {
    let mut document_manager = crate::document::DocumentManager::new();
    if let Some(path) = effective_file_path {
        document_manager.open_file(Some(path.to_string()), false)?;
    } else {
        let new_doc = Document::new(crate::document::DocumentId::new(1)).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                e.to_string(),
            )
        })?;
        document_manager.add_document(new_doc);
    }
    Ok(document_manager)
}

fn startup_state(
    effective_file_path: Option<&str>,
    document_manager: &crate::document::DocumentManager,
) -> State {
    let mut state = State::new();
    state.set_file_path(effective_file_path.map(str::to_string));
    if let Some(doc) = document_manager.active_document() {
        state.update_filename(doc.display_name().to_string());
    }
    state
}

impl<T: TerminalBackend> Editor<T> {
    #[cfg(test)]
    /// # Errors
    /// Returns an error if the terminal cannot be initialized or the startup buffer cannot be created.
    pub(super) fn new(terminal: T) -> Result<Self, RiftError> {
        Self::with_file(terminal, None)
    }

    /// # Errors
    /// Returns an error if the startup file cannot be opened, the startup buffer cannot be created, or the terminal fails to initialize or report its size.
    /// # Panics
    /// Panics if no document is active after the startup buffer is created.
    pub fn with_file(mut terminal: T, file_path: Option<&str>) -> Result<Self, RiftError> {
        let (target_dir, effective_file_path) = startup_target(file_path);
        let language_loader = startup_language_loader();
        let document_manager = open_startup_documents(effective_file_path)?;

        let mut buffer_kinds = crate::document::BufferKindRegistry::with_builtins();
        buffer_kinds.increment_open_count(crate::document::BufferKindId::FILE);

        terminal.init()?;

        let size = terminal.get_size()?;

        let render_system =
            crate::render::RenderSystem::new(size.rows as usize, size.cols as usize);

        let settings_registry = create_settings_registry();
        let command_parser = CommandParser::new(settings_registry);

        let state = startup_state(effective_file_path, &document_manager);
        let initial_doc_id = document_manager
            .active_document_id()
            .expect("No active document after initialization");
        let split_tree = SplitTree::new(initial_doc_id, size.rows as usize, size.cols as usize);

        let mut editor = Self {
            term: terminal,
            render_system,
            document_manager,
            buffer_kinds,
            current_mode: Mode::Normal,
            should_quit: false,
            state,
            command_parser,
            settings_registry,
            document_settings_registry:
                crate::document::definitions::create_document_settings_registry(),
            language_loader,
            job_manager: crate::job_manager::JobManager::new(),
            pending_quit_job_id: None,
            keymap: KeyMap::new(),
            split_tree,
            native_action_handlers: super::handle_action::native_action_handlers(),
            native_save_handlers: super::file_ops::native_save_handlers(),
            native_reload_handlers: super::file_ops::native_reload_handlers(),
            pending_keys: Vec::new(),
            pending_count: 0,
            pending_operator_count: 0,
            pending_operator: None,
            pending_grammar: None,
            pending_keys_started_at: None,
            visual_anchor: None,
            pending_multi_insert_anchors: Vec::new(),
            replace_originals: Vec::new(),
            region_build_recording: Vec::new(),
            expand_history: Vec::new(),
            display_map_cache: Vec::new(),
            pending_text_changed: None,
            pending_cursor_moved: None,
            dot_repeat: DotRepeat::new(),
            panel_layout: None,
            last_notification_generation: 0,
            plugin_host: crate::plugin::PluginHost::new(25),
            clipboard_ring: crate::clipboard::ClipboardRing::new(),
            system_clipboard_cache: crate::clipboard::SystemClipboardCache::new(),
            post_paste_state: None,
            pending_cursor_entry: None,
            file_load_jobs: std::collections::HashMap::new(),
            #[cfg(feature = "lsp")]
            lsp_manager: crate::lsp::LspManager::new(std::env::current_dir().ok()),
            #[cfg(feature = "lsp")]
            lsp_diagnostics: std::collections::BTreeMap::new(),
            #[cfg(feature = "lsp")]
            lsp_ready_servers: std::collections::HashSet::new(),
            #[cfg(feature = "lsp")]
            pending_code_actions: Vec::new(),
            #[cfg(feature = "lsp")]
            rename_context: None,
            #[cfg(feature = "lsp")]
            pending_goto_target: None,
            dispatch_registry: crate::annotations::registry::DispatchRegistry::with_builtins(),
            kind_registry: crate::annotations::registry::KindRegistry::with_core(),
            hovered_annotation: None,
            pending_syntax_reparse: std::collections::HashMap::new(),
            pending_git_gutter_diff: std::collections::HashMap::new(),
            git_gutter_repo_cache: std::collections::HashMap::new(),
            pending_git_status_expand_all: std::collections::HashMap::new(),
            pending_git_log_expand_head: std::collections::HashSet::new(),
            pending_search_refresh: None,
            search_highlights_synced: None,
            pending_explorer_preview: None,
            startup_first_paint: None,
            unrendered_key_count: 0,
        };

        crate::keymap::defaults::register_defaults(&mut editor.keymap);
        editor.load_startup_plugins();
        if effective_file_path.is_some() {
            editor.restore_startup_undo(initial_doc_id);
        }
        editor.finish_startup(target_dir);

        Ok(editor)
    }

    fn load_startup_plugins(&mut self) {
        if let Err(e) = self.load_plugins() {
            self.state
                .notify(crate::notification::NotificationType::Error, e.to_string());
        }
    }

    fn restore_startup_undo(&mut self, doc_id: DocumentId) {
        let hash = self
            .document_manager
            .get_document(doc_id)
            .map(|doc| crate::history::persist::sha256(&doc.buffer.to_logical_bytes()));
        if let Some(hash) = hash {
            self.restore_persisted_undo(doc_id, hash);
        }
    }

    fn finish_startup(&mut self, target_dir: Option<std::path::PathBuf>) {
        let _ = self.update_and_render();
        self.startup_first_paint = Some(crate::time::Instant::now());

        if let Some(doc_id) = self.document_manager.active_document_id() {
            #[cfg(feature = "treesitter")]
            self.attach_syntax_for_document(doc_id);
            if self
                .document_manager
                .get_document(doc_id)
                .is_some_and(|d| d.syntax.is_some())
            {
                self.spawn_syntax_parse_job(doc_id);
            }
        }

        {
            let buf_info = self.document_manager.active_document().map(|doc| {
                let buf = doc.id;
                let path = doc.path().map(std::path::Path::to_path_buf);
                let filetype = doc.syntax.as_ref().map(|s| s.language_name.clone());
                (buf, path, filetype)
            });
            if let Some((buf, path, filetype)) = buf_info {
                self.update_lua_state();
                self.plugin_host
                    .dispatch(&crate::plugin::EditorEvent::BufOpen {
                        buf,
                        path,
                        filetype,
                    });
                self.apply_plugin_mutations();
                #[cfg(feature = "lsp")]
                self.lsp_notify_open(buf);
            }
        }

        self.update_lua_state();
        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::EditorStart);
        self.apply_plugin_mutations();

        if let Some(dir) = target_dir {
            self.open_explorer(dir);
        }
    }

    pub(super) fn active_document_id(&self) -> DocumentId {
        self.split_tree.focused_window().document_id
    }

    /// # Panics
    /// Panics if the focused window's document is missing from the document manager.
    pub(crate) fn active_document(&mut self) -> &mut Document {
        let doc_id = self.split_tree.focused_window().document_id;
        self.document_manager
            .get_document_mut(doc_id)
            .expect("No active document")
    }

    pub(super) fn switch_focus(&mut self, target_id: crate::split::window::WindowId) {
        let old_win_id = self.split_tree.focused_window_id();
        let old_doc_id = self.split_tree.focused_window().document_id;
        if let Some(doc) = self.document_manager.get_document(old_doc_id) {
            let cursor = doc.buffer.cursor();
            self.split_tree.focused_window_mut().cursor_position = cursor;
        }

        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::WinLeave {
                win: old_win_id.get(),
                buf: old_doc_id,
            });
        self.apply_plugin_mutations();

        self.split_tree.set_focus(target_id);

        let new_doc_id = self.split_tree.focused_window().document_id;
        let new_cursor = self.split_tree.focused_window().cursor_position;
        let _ = self.document_manager.switch_to_document(new_doc_id);
        if let Some(doc) = self.document_manager.get_document_mut(new_doc_id) {
            let _ = doc.buffer.set_cursor(new_cursor);
        }

        self.sync_state_with_active_document();

        self.plugin_host
            .dispatch(&crate::plugin::EditorEvent::WinEnter {
                win: target_id.get(),
                buf: new_doc_id,
            });
        self.apply_plugin_mutations();
    }

    pub(super) fn save_current_view_state(&mut self) {
        let (top_line, left_col) = self.render_system.viewport.get_scroll();
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.save_view_state(top_line, left_col);
        }
    }

    pub(super) fn restore_view_state(&mut self) {
        if let Some(doc) = self.document_manager.active_document() {
            let view_state = doc.get_view_state();
            self.render_system
                .viewport
                .set_scroll(view_state.top_line, view_state.left_col);
        }
    }

    pub(super) fn sync_state_with_active_document(&mut self) {
        let (display_name, file_path, is_dirty, line_ending) = {
            let doc = self.active_document();
            (
                doc.display_name().to_string(),
                doc.path().map(|p| p.to_string_lossy().to_string()),
                doc.is_dirty(),
                doc.options.line_ending,
            )
        };

        self.state.update_filename(display_name);
        self.state.set_file_path(file_path);
        self.state.update_dirty(is_dirty);

        let total_lines = self.active_document().buffer.get_total_lines();
        let buffer_size = self.active_document().buffer.len();
        self.state
            .update_buffer_stats(total_lines, buffer_size, line_ending);

        self.refresh_search_highlights_if_stale();
    }

    pub(super) fn force_full_redraw(&mut self) -> Result<(), RiftError> {
        self.render_system.viewport.mark_needs_full_redraw();
        self.update_and_render().map_err(|e| {
            RiftError::new(
                ErrorType::Io,
                crate::constants::errors::RENDER_FAILED,
                e.to_string(),
            )
        })
    }

    fn retire_lua_plugins(&mut self) {
        let active_gens = self.plugin_host.active_generations();
        for gen in active_gens {
            self.plugin_host.retire_generation(gen);
            self.keymap.unregister_owner(gen.keymap_owner_token());

            let retired_kind_ids = self.buffer_kinds.retire_plugin(
                gen.plugin_id().as_nonzero(),
                gen.generation(),
                "plugin reloaded",
            );

            for kind_id in retired_kind_ids {
                if let Some(tombstone_desc) = self.buffer_kinds.get_by_id(kind_id) {
                    let matching_doc_ids: Vec<crate::document::DocumentId> = self
                        .document_manager
                        .documents_iter()
                        .filter(|doc| doc.kind.id == kind_id)
                        .map(|doc| doc.id)
                        .collect();

                    for doc_id in matching_doc_ids {
                        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
                            doc.kind = crate::document::BufferKind::new(std::sync::Arc::clone(
                                &tombstone_desc,
                            ));
                        }
                    }
                }
            }
        }

        self.dispatch_registry.clear_lua_handlers();
    }

    pub(super) fn load_plugins(&mut self) -> Result<(), RiftError> {
        self.retire_lua_plugins();
        if let Some(err) = self.plugin_host.init_lua() {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::PLUGIN_LOAD_FAILED,
                err,
            ));
        }

        for dir in plugin_dirs() {
            if let Some(err) = self.plugin_host.lua_load_dir(&dir).into_iter().next() {
                return Err(RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::PLUGIN_LOAD_FAILED,
                    err,
                ));
            }
        }

        let init_lua = user_config_dir().join("init.lua");
        if init_lua.is_file() {
            if let Some(err) = self.plugin_host.lua_load_file(&init_lua) {
                return Err(RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::PLUGIN_LOAD_FAILED,
                    err,
                ));
            }
        }

        self.apply_plugin_mutations();
        Ok(())
    }

    pub(crate) const fn set_remote(&mut self, remote: bool) {
        self.state.is_remote = remote;
    }
}

mod paths {
    pub(crate) const GRAMMARS_DIR: &str = "grammars";
}
