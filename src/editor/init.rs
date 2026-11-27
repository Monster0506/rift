use super::Editor;
#[cfg(feature = "plugins")]
use super::{plugin_dirs, user_config_dir};
use crate::command_line::commands::parser::CommandParser;
use crate::document::{Document, DocumentId};
use crate::dot_repeat::DotRepeat;
use crate::error::{ErrorType, RiftError};
use crate::keymap::KeyMap;
use crate::mode::Mode;
use crate::split::tree::SplitTree;
use crate::state::definitions::create_settings_registry;
use crate::state::State;
use crate::term::TerminalBackend;
#[cfg(any(feature = "plugins", feature = "lsp", feature = "treesitter"))]
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

#[cfg(any(feature = "plugins", feature = "lsp", feature = "treesitter"))]
fn startup_language_loader() -> Arc<crate::syntax::loader::LanguageLoader> {
    let grammar_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join(paths::GRAMMARS_DIR)))
        .unwrap_or_else(|| std::path::PathBuf::from(paths::GRAMMARS_DIR));
    Arc::new(crate::syntax::loader::LanguageLoader::new(grammar_dir))
}

fn startup_document(effective_file_path: Option<&str>) -> Result<Document, RiftError> {
    let id = crate::document::DocumentId::new(1);
    match effective_file_path {
        Some(path) => Document::open_path(id, path),
        None => Document::new(id).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                e.to_string(),
            )
        }),
    }
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
        #[cfg(any(feature = "plugins", feature = "lsp", feature = "treesitter"))]
        let language_loader = startup_language_loader();
        let startup_doc = startup_document(effective_file_path)?;
        let buffer_kinds = crate::document::runtime::BufferKindRegistry::with_builtins();

        terminal.init()?;

        let size = terminal.get_size()?;

        let render_system =
            crate::render::system::RenderSystem::new(size.rows as usize, size.cols as usize);

        let settings_registry = create_settings_registry();
        let command_parser = CommandParser::new(settings_registry);

        let state = State::new();
        let initial_doc_id = startup_doc.id;
        let split_tree = SplitTree::new(initial_doc_id, size.rows as usize, size.cols as usize);

        let mut editor = Self {
            term: terminal,
            render_system,
            document_manager: crate::document::manager::DocumentManager::new(),
            buffer_kinds,
            current_mode: Mode::Normal,
            should_quit: false,
            state,
            command_parser,
            settings_registry,
            document_settings_registry:
                crate::document::definitions::create_document_settings_registry(),
            #[cfg(any(feature = "plugins", feature = "lsp", feature = "treesitter"))]
            language_loader,
            job_manager: crate::job_manager::JobManager::new(),
            scheduler: super::scheduler::Scheduler::new(),
            keymap: KeyMap::new(),
            split_tree,
            input: super::pending_grammar::InputState::default(),
            replace_originals: Vec::new(),
            region_build_recording: Vec::new(),
            expand_history: Vec::new(),
            display_map_cache: Vec::new(),
            dot_repeat: DotRepeat::new(),
            panel_layout: None,
            last_notification_generation: 0,
            #[cfg(feature = "plugins")]
            plugin_host: crate::plugin::PluginHost::new(std::time::Duration::from_millis(400)),
            #[cfg(feature = "plugins")]
            plugin_jobs: std::collections::HashMap::new(),
            #[cfg(any(feature = "plugins", feature = "lsp"))]
            popup: crate::floating_window::popup::Popup::default(),
            clipboard_ring: crate::clipboard::ClipboardRing::new(),
            system_clipboard_cache: crate::clipboard::SystemClipboardCache::new(),
            clipboard_panel_register: None,
            post_paste_state: None,
            file_load_jobs: std::collections::HashMap::new(),
            #[cfg(feature = "lsp")]
            lsp_manager: crate::lsp::LspManager::new(std::env::current_dir().ok()),
            #[cfg(feature = "lsp")]
            lsp_diagnostics: crate::tracked::Tracked::default(),
            #[cfg(feature = "lsp")]
            lsp_ready_servers: std::collections::HashSet::new(),
            #[cfg(feature = "lsp")]
            rename_context: None,
            dispatch_registry: crate::annotations::registry::DispatchRegistry::with_builtins(),
            kind_registry: crate::annotations::registry::KindRegistry::with_core(),
            #[cfg(feature = "plugins")]
            hovered_annotation: None,
            #[cfg(feature = "plugins")]
            plugin_mutation_rounds: None,
            git_gutter_seen_revision: std::collections::HashMap::new(),
            git_gutter_repo_cache: std::collections::HashMap::new(),
            git_status_refresh_jobs: std::collections::HashMap::new(),
            search_highlights_synced: None,
            startup_first_paint: None,
            unrendered_key_count: 0,
            #[cfg(feature = "plugins")]
            entered_document: initial_doc_id,
            #[cfg(feature = "plugins")]
            frame_events: Vec::new(),
        };

        crate::keymap::defaults::register_defaults(&mut editor.keymap);
        editor.insert_buffer(startup_doc, crate::document::manager::Placement::Background);
        #[cfg(feature = "plugins")]
        editor.load_plugins();
        if effective_file_path.is_some() {
            editor.restore_startup_undo(initial_doc_id);
        }
        editor.finish_startup(target_dir);

        Ok(editor)
    }

    fn restore_startup_undo(&mut self, doc_id: DocumentId) {
        let hash = self
            .document_manager
            .get_document(doc_id)
            .map(|doc| crate::history::persist::sha256(&doc.buffer().to_logical_bytes()));
        if let Some(hash) = hash {
            self.restore_persisted_undo(doc_id, hash);
        }
    }

    fn finish_startup(&mut self, target_dir: Option<std::path::PathBuf>) {
        let _ = self.update_and_render();
        self.startup_first_paint = Some(crate::time::Instant::now());

        self.announce_buffer(self.active_document_id());
        self.deliver_frame_events();

        #[cfg(feature = "plugins")]
        self.raise(crate::plugin::events::EventPayload::EditorStart);

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
        let doc_id = self.active_document_id();
        self.document_manager
            .get_document_mut(doc_id)
            .expect("No active document")
    }

    #[cfg(feature = "plugins")]
    fn retire_lua_plugins(&mut self) {
        self.clear_plugin_creation_ids();
        let active_gens = self.plugin_host.active_generations();
        for gen in active_gens {
            self.plugin_host.retire_generation(gen);
            self.stop_generation_jobs(gen);
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
                            doc.kind = crate::document::kind::BufferKind::new(
                                std::sync::Arc::clone(&tombstone_desc),
                            );
                        }
                    }
                }
            }
        }

        self.dispatch_registry.clear_lua_handlers();
    }

    #[cfg(feature = "plugins")]
    pub(super) fn load_plugins(&mut self) {
        let init_lua = user_config_dir().join("init.lua");
        for err in self.load_plugins_from(&plugin_dirs(), &init_lua) {
            self.notify(crate::notification::NotificationType::Error, err);
        }
    }

    #[cfg(feature = "plugins")]
    pub(super) fn load_plugins_from(
        &mut self,
        dirs: &[std::path::PathBuf],
        init_lua: &std::path::Path,
    ) -> Vec<String> {
        self.retire_lua_plugins();
        let mut errors: Vec<String> = self
            .plugin_host
            .init_lua(Arc::clone(&self.language_loader))
            .into_iter()
            .collect();
        if init_lua.is_file() || dirs.iter().any(|dir| crate::plugin::dir_has_lua_files(dir)) {
            self.plugin_host.mark_lua_used();
        }
        self.call_plugins(|editor| {
            for dir in dirs {
                errors.extend(editor.plugin_host.lua_load_dir(dir));
            }
            if init_lua.is_file() {
                errors.extend(editor.plugin_host.lua_load_file(init_lua));
            }
        });
        errors
    }

    #[cfg(feature = "ipc")]
    pub(crate) const fn set_remote(&mut self, remote: bool) {
        self.state.is_remote = remote;
    }
}

#[cfg(any(feature = "plugins", feature = "lsp", feature = "treesitter"))]
mod paths {
    pub(super) const GRAMMARS_DIR: &str = "grammars";
}
