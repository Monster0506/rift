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

/// Number of base-10 digits in `n` (matches `n.to_string().len()` without
/// allocating - called every frame from `sync_state_with_active_document`).
fn decimal_digit_count(mut n: usize) -> usize {
    let mut count = 1;
    while n >= 10 {
        n /= 10;
        count += 1;
    }
    count
}

impl<T: TerminalBackend> Editor<T> {
    /// Create a new editor instance
    pub fn new(terminal: T) -> Result<Self, RiftError> {
        Self::with_file(terminal, None)
    }

    /// Create a new editor instance with an optional file to load
    pub fn with_file(mut terminal: T, file_path: Option<String>) -> Result<Self, RiftError> {
        // Init language loader
        let grammar_dir = std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|p| p.join(crate::constants::paths::GRAMMARS_DIR))
            })
            .unwrap_or_else(|| std::path::PathBuf::from(crate::constants::paths::GRAMMARS_DIR));

        let language_loader = Arc::new(crate::syntax::loader::LanguageLoader::new(grammar_dir));

        // Create document (either from file or empty)
        // Create document manager
        let mut document_manager = crate::document::DocumentManager::new();

        if let Some(ref path) = file_path {
            document_manager.open_file(Some(path.clone()), false)?;
        } else {
            // Create empty document
            let new_doc = Document::new(1).map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    e.to_string(),
                )
            })?;
            document_manager.add_document(new_doc);
        }

        // Initialize terminal (clears screen, enters raw mode, etc.)
        // We do this AFTER loading the document so we don't mess up the terminal
        // if loading fails
        terminal.init()?;

        // Get terminal size
        let size = terminal.get_size()?;

        // Create render system
        let render_system =
            crate::render::RenderSystem::new(size.rows as usize, size.cols as usize);

        // Create command registry and settings registry
        let settings_registry = create_settings_registry();
        let command_parser = CommandParser::new(settings_registry.clone());

        let mut state = State::new();
        state.set_file_path(file_path.clone());
        if let Some(doc) = document_manager.active_document() {
            state.update_filename(doc.display_name().to_string());
        }

        let initial_doc_id = document_manager
            .active_document_id()
            .expect("No active document after initialization");
        let split_tree = SplitTree::new(initial_doc_id, size.rows as usize, size.cols as usize);

        let mut editor = Self {
            term: terminal,
            render_system,
            document_manager,
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
            pending_keys: Vec::new(),
            pending_count: 0,
            pending_operator_count: 0,
            pending_operator: None,
            pending_grammar: None,
            pending_keys_started_at: None,
            pending_surround_add: None,
            visual_anchor: None,
            pending_multi_insert_anchors: Vec::new(),
            region_build_recording: Vec::new(),
            expand_history: Vec::new(),
            display_map_cache: Vec::new(),
            pending_text_changed: None,
            pending_cursor_moved: None,
            dot_repeat: DotRepeat::new(),
            panel_layout: None,
            last_notification_generation: 0,
            // 25 idle polls * 16 ms = 400 ms before CursorHold fires.
            plugin_host: crate::plugin::PluginHost::new(25),
            clipboard_ring: crate::clipboard::ClipboardRing::new(),
            system_clipboard_cache: crate::clipboard::SystemClipboardCache::new(),
            post_paste_state: None,
            pending_cursor_entry: None,
            #[cfg(feature = "lsp")]
            lsp_manager: crate::lsp::LspManager::new(std::env::current_dir().ok()),
            #[cfg(feature = "lsp")]
            lsp_diagnostics: std::collections::HashMap::new(),
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
            pending_search_refresh: None,
            search_highlights_synced: None,
            pending_explorer_preview: None,
            startup_first_paint: None,
        };

        // Register default keymaps
        crate::keymap::defaults::register_defaults(&mut editor.keymap);

        if let Err(e) = editor.load_plugins() {
            editor
                .state
                .notify(crate::notification::NotificationType::Error, e.to_string())
        }

        // First contentful paint: show the buffer before syntax setup, plugin
        // event dispatch, and job spawns; highlights/annotations repaint later.
        let _ = editor.update_and_render();
        editor.startup_first_paint = Some(crate::time::Instant::now());

        // Trigger background search cache warming for initial document
        if let Some(doc) = editor.document_manager.active_document() {
            let table = doc.buffer.line_index.table.clone();
            let revision = doc.buffer.revision;
            let job =
                crate::job_manager::jobs::cache_warming::CacheWarmingJob::new(table, revision);

            editor.job_manager.spawn(job);
        }

        // Trigger initial syntax parse
        #[cfg(feature = "treesitter")]
        if let Some(doc) = editor.document_manager.active_document_mut() {
            if let Some(path) = doc.path() {
                let path = path.to_path_buf();
                if let Ok(loaded) = editor.language_loader.load_language_for_file(&path) {
                    let highlights_query = editor
                        .language_loader
                        .load_query(&loaded.name, "highlights")
                        .ok()
                        .and_then(|source| tree_sitter::Query::new(&loaded.language, &source).ok())
                        .map(Arc::new);

                    if let Ok(syntax) = crate::syntax::build_syntax(
                        loaded,
                        highlights_query,
                        editor.language_loader.clone(),
                    ) {
                        doc.set_syntax(syntax);
                        let doc_id = doc.id;
                        editor.spawn_syntax_parse_job(doc_id);
                    }
                }
            }
        }

        // Dispatch BufOpen for the initial synchronously-loaded document.
        {
            let buf_info = editor.document_manager.active_document().map(|doc| {
                let buf = doc.id;
                let path = doc.path().map(|p| p.to_path_buf());
                let filetype = doc.syntax.as_ref().map(|s| s.language_name.clone());
                (buf, path, filetype)
            });
            if let Some((buf, path, filetype)) = buf_info {
                editor.update_lua_state();
                editor
                    .plugin_host
                    .dispatch(&crate::plugin::EditorEvent::BufOpen {
                        buf,
                        path,
                        filetype,
                    });
                editor.apply_plugin_mutations();
                #[cfg(feature = "lsp")]
                editor.lsp_notify_open();
            }
        }

        // Dispatch EditorStart after all initialization is complete.
        editor.update_lua_state();
        editor
            .plugin_host
            .dispatch(&crate::plugin::EditorEvent::EditorStart);
        editor.apply_plugin_mutations();

        Ok(editor)
    }

    pub fn active_document_id(&self) -> DocumentId {
        self.split_tree.focused_window().document_id
    }

    pub fn active_document(&mut self) -> &mut Document {
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
                win: old_win_id,
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
                win: target_id,
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

    /// Restore view state from the active document after switching
    pub(super) fn restore_view_state(&mut self) {
        if let Some(doc) = self.document_manager.active_document() {
            let view_state = doc.get_view_state();
            self.render_system
                .viewport
                .set_scroll(view_state.top_line, view_state.left_col);
        }
    }

    /// Sync editor state with the active document
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

        // Update gutter width
        if self.state.settings.show_line_numbers {
            // 1 space padding on each side
            self.state.gutter_width = decimal_digit_count(total_lines) + 2;
        } else {
            self.state.gutter_width = 0;
        }

        self.refresh_search_highlights_if_stale();
    }

    /// Force a full redraw of the editor
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

    pub(super) fn load_plugins(&mut self) -> Result<(), RiftError> {
        // Drop stale Lua annotation-action handlers; the fresh VM re-registers
        // them as plugins run, and any that don't simply fall back to a no-op.
        self.dispatch_registry.clear_lua_handlers();
        if let Some(err) = self.plugin_host.init_lua() {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::PLUGIN_LOAD_FAILED,
                err,
            ));
        }

        // Load bundled runtime plugins first so they are available to user
        // config.  riftpm.lua in particular must be loaded before init.lua so
        // that `require("riftpm")` works from the user's config.
        for dir in plugin_dirs() {
            if let Some(err) = self.plugin_host.lua_load_dir(&dir).into_iter().next() {
                return Err(RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::PLUGIN_LOAD_FAILED,
                    err,
                ));
            }
        }

        // Execute ~/.config/rift/init.lua after bundled plugins.
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

        // Apply any mutations queued by top-level plugin code (e.g. rift.map()).
        self.apply_plugin_mutations();
        Ok(())
    }

    /// Mark this editor as running in a remote IPC daemon session.
    pub fn set_remote(&mut self, remote: bool) {
        self.state.is_remote = remote;
    }
}
