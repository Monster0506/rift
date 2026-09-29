pub mod actions;
mod annotations_ops;

#[cfg(all(test, feature = "terminal_emulation"))]
mod terminal_tests;

mod command_exec;
mod command_line_handler;
mod completion;
mod context_impl;
mod document_ops;
mod explorer;
mod file_ops;
mod git_blame;
mod git_gutter;
mod git_log;
mod git_rebase;
mod git_status;
mod handle_action;
mod history;
mod init;
mod invariants;
mod jobs;
#[cfg(feature = "lsp")]
mod lsp_ops;
mod mode_mgmt;
mod multi_region;
mod operators;
mod panel_handlers;
mod pending_grammar;
mod plugin_ops;
pub(crate) mod rendering;
mod run_loop;
mod text_object_input;
mod undo_persist;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
mod split_move_tests;

#[cfg(test)]
mod split_nav_stress_tests;

#[cfg(test)]
mod insert_typing_stress_tests;

#[cfg(test)]
mod git_gutter_tests;

#[cfg(test)]
mod git_rebase_tests;

#[cfg(test)]
mod git_status_tests;

use crate::command_line::commands::CommandParser;
use crate::command_line::settings::SettingsRegistry;
use crate::document::{Document, DocumentId};
use crate::dot_repeat::DotRepeat;
use crate::keymap::KeyMap;

use crate::mode::Mode;
use crate::split::tree::SplitTree;
use crate::state::{State, UserSettings};
use crate::term::TerminalBackend;
use std::sync::Arc;

pub(crate) fn user_config_dir() -> std::path::PathBuf {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("rift")
    } else {
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
            });
        base.join("rift")
    }
}

fn plugin_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        dirs.push(
            std::path::PathBuf::from(manifest)
                .join("runtime")
                .join("plugins"),
        );
    }
    dirs.push(user_config_dir().join("plugins"));
    dirs
}


fn resolve_wrap_params(
    doc: &Document,
    content_width: usize,
    global_soft_wrap: bool,
    global_wrap_width: Option<usize>,
) -> Option<(usize, usize)> {
    use crate::document::definitions::WrapMode;
    if doc.projection() != crate::document::TextProjectionPolicy::PlainText {
        return None;
    }
    let w = match &doc.options.wrap {
        Some(WrapMode::Off) => return None,
        Some(mode) => mode.resolve(content_width),
        None => {
            if !global_soft_wrap {
                return None;
            }
            global_wrap_width.unwrap_or(content_width)
        }
    };
    Some((w, doc.options.tab_width))
}

fn resolve_display_map(
    doc: &Document,
    content_width: usize,
    global_soft_wrap: bool,
    global_wrap_width: Option<usize>,
) -> Option<crate::wrap::DisplayMap> {
    let (w, tab_width) =
        resolve_wrap_params(doc, content_width, global_soft_wrap, global_wrap_width)?;
    Some(crate::wrap::DisplayMap::build(&doc.buffer, w, tab_width))
}

pub struct Editor<T: TerminalBackend> {
    pub term: T,
    pub document_manager: crate::document::DocumentManager,
    pub buffer_kinds: crate::document::BufferKindRegistry,
    pub render_system: crate::render::RenderSystem,
    current_mode: Mode,
    should_quit: bool,
    state: State,
    command_parser: CommandParser,
    settings_registry: SettingsRegistry<UserSettings>,
    document_settings_registry: SettingsRegistry<crate::document::definitions::DocumentOptions>,
    language_loader: Arc<crate::syntax::loader::LanguageLoader>,
    pub job_manager: crate::job_manager::JobManager,
    pending_quit_job_id: Option<usize>,
    pub keymap: KeyMap,
    pub split_tree: SplitTree,
    native_action_handlers: std::collections::HashMap<
        crate::document::BufferKindId,
        handle_action::NativeActionHandler<T>,
    >,
    native_save_handlers:
        std::collections::HashMap<crate::document::BufferKindId, file_ops::NativeSaveHandler<T>>,
    native_reload_handlers:
        std::collections::HashMap<crate::document::BufferKindId, file_ops::NativeReloadHandler<T>>,
    pending_keys: Vec<crate::key::Key>,
    pending_count: usize,
    pending_operator_count: usize,
    pending_operator: Option<crate::action::OperatorType>,
    pending_grammar: Option<pending_grammar::PendingGrammar>,
    pending_keys_started_at: Option<crate::time::Instant>,
    pending_surround_add: Option<usize>,
    pub(super) visual_anchor: Option<usize>,
    pub(super) pending_multi_insert_anchors: Vec<usize>,
    pub(super) region_build_recording: Vec<crate::action::Action>,
    pub(super) expand_history: Vec<(usize, usize)>,
    display_map_cache: Vec<DisplayMapCacheEntry>,
    pending_text_changed: Option<crate::document::DocumentId>,
    pending_cursor_moved: Option<(crate::document::DocumentId, usize, usize)>,
    dot_repeat: DotRepeat,
    pub panel_layout: Option<PanelLayout>,
    last_notification_generation: u64,
    pub plugin_host: crate::plugin::PluginHost,
    pub clipboard_ring: crate::clipboard::ClipboardRing,
    system_clipboard_cache: crate::clipboard::SystemClipboardCache,
    post_paste_state: Option<PostPasteState>,
    pending_cursor_entry: Option<String>,
    file_load_jobs: std::collections::HashMap<usize, crate::document::DocumentId>,
    #[cfg(feature = "lsp")]
    pub lsp_manager: crate::lsp::LspManager,
    #[cfg(feature = "lsp")]
    lsp_diagnostics: std::collections::HashMap<String, Vec<crate::lsp::protocol::LspDiagnostic>>,
    #[cfg(feature = "lsp")]
    lsp_ready_servers: std::collections::HashSet<String>,
    #[cfg(feature = "lsp")]
    pending_code_actions: Vec<serde_json::Value>,
    #[cfg(feature = "lsp")]
    rename_context: Option<(std::path::PathBuf, u32, u32)>,
    #[cfg(feature = "lsp")]
    pending_goto_target: Option<(crate::document::DocumentId, usize, usize)>,
    pub dispatch_registry: crate::annotations::registry::DispatchRegistry,
    pub kind_registry: crate::annotations::registry::KindRegistry,
    hovered_annotation: Option<crate::annotations::AnnotationId>,
    pending_syntax_reparse:
        std::collections::HashMap<crate::document::DocumentId, jobs::PendingSyntaxReparse>,
    pending_git_gutter_diff:
        std::collections::HashMap<crate::document::DocumentId, git_gutter::PendingGitGutterDiff>,
    git_gutter_repo_cache:
        std::collections::HashMap<crate::document::DocumentId, Option<std::path::PathBuf>>,
    pending_git_status_expand_all: std::collections::HashMap<crate::document::DocumentId, bool>,
    pending_git_log_expand_head: std::collections::HashSet<crate::document::DocumentId>,
    pending_search_refresh: Option<crate::time::Instant>,
    search_highlights_synced: Option<(crate::document::DocumentId, u64, String)>,
    pending_explorer_preview: Option<explorer::PendingExplorerPreview>,
    pub(crate) startup_first_paint: Option<crate::time::Instant>,
    pub(super) unrendered_key_count: usize,
}

struct DisplayMapCacheEntry {
    doc_id: DocumentId,
    revision: u64,
    buf_len: usize,
    content_width: usize,
    annotations_revision: u64,
    lsp_virtual_text: bool,
    map: Option<std::sync::Arc<crate::wrap::DisplayMap>>,
}

#[derive(Debug, Clone)]
struct PostPasteState {
    ring_index: usize,
    before: bool,
    original_cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    FileExplorer,
    UndoTree,
    Clipboard,
    LocationList,
    Regions,
    BufferList,
}

#[derive(Debug, Clone)]
pub struct PanelLayout {
    pub kind: PanelKind,
    pub dir_win_id: crate::split::window::WindowId,
    pub preview_win_id: crate::split::window::WindowId,
    pub dir_doc_id: DocumentId,
    pub preview_doc_id: DocumentId,
    pub original_doc_id: DocumentId,
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.term.deinit();
    }
}
