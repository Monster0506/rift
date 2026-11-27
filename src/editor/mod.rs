mod actions;
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
pub(crate) mod ops;
#[cfg(feature = "plugins")]
pub(crate) use ops::EditorEvent;
mod panel_handlers;
mod pending_grammar;
#[cfg(feature = "plugins")]
mod plugin_jobs;
#[cfg(feature = "plugins")]
mod plugin_ops;
pub(crate) mod rendering;
mod replace_mode;
mod run_loop;
mod scheduler;
mod text_object_input;
mod undo_persist;

#[cfg(test)]
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

use crate::command_line::commands::parser::CommandParser;
use crate::command_line::settings::registry::SettingsRegistry;
use crate::document::{Document, DocumentId};
use crate::dot_repeat::DotRepeat;
use crate::keymap::KeyMap;

use crate::mode::Mode;
use crate::split::tree::SplitTree;
use crate::state::{State, UserSettings};
use crate::term::TerminalBackend;
#[cfg(any(feature = "plugins", feature = "lsp", feature = "treesitter"))]
use std::sync::Arc;

pub(crate) fn user_config_dir() -> std::path::PathBuf {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .map_or_else(|_| std::path::PathBuf::from("."), std::path::PathBuf::from)
            .join("rift")
    } else {
        let base = std::env::var("XDG_CONFIG_HOME").ok().map_or_else(
            || std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"),
            std::path::PathBuf::from,
        );
        base.join("rift")
    }
}

#[cfg(feature = "plugins")]
fn plugin_dirs() -> Vec<std::path::PathBuf> {
    #[cfg(not(test))]
    let runtime = std::env::var("CARGO_MANIFEST_DIR").ok().map(|manifest| {
        std::path::PathBuf::from(manifest)
            .join("runtime")
            .join("plugins")
    });
    #[cfg(test)]
    let runtime: Option<std::path::PathBuf> = None;
    runtime
        .into_iter()
        .chain(std::iter::once(user_config_dir().join("plugins")))
        .collect()
}

fn resolve_wrap_params(
    doc: &Document,
    content_width: usize,
    global_soft_wrap: bool,
    global_wrap_width: Option<usize>,
) -> Option<(usize, usize)> {
    use crate::document::definitions::WrapMode;
    if doc.projection() != crate::document::runtime::TextProjectionPolicy::PlainText {
        return None;
    }
    let w = match doc.options().wrap() {
        Some(WrapMode::Off) => return None,
        Some(mode) => mode.resolve(content_width),
        None => {
            if !global_soft_wrap {
                return None;
            }
            global_wrap_width.unwrap_or(content_width)
        }
    };
    Some((w, doc.options().tab_width()))
}

#[derive(Clone)]
struct ClipboardPanelRegister {
    clip: crate::clipboard::Clip,
    rows: Option<Vec<crate::clipboard::Clip>>,
}

fn resolve_display_map(
    doc: &Document,
    content_width: usize,
    global_soft_wrap: bool,
    global_wrap_width: Option<usize>,
) -> Option<crate::wrap::DisplayMap> {
    let (w, tab_width) =
        resolve_wrap_params(doc, content_width, global_soft_wrap, global_wrap_width)?;
    Some(crate::wrap::DisplayMap::build(doc.buffer(), w, tab_width))
}

pub struct Editor<T: TerminalBackend> {
    pub(crate) term: T,
    document_manager: crate::document::manager::DocumentManager,
    buffer_kinds: crate::document::runtime::BufferKindRegistry,
    render_system: crate::render::system::RenderSystem,
    current_mode: Mode,
    should_quit: bool,
    state: State,
    command_parser: CommandParser,
    settings_registry: SettingsRegistry<UserSettings>,
    document_settings_registry: SettingsRegistry<crate::document::definitions::DocumentOptions>,
    #[cfg(any(feature = "plugins", feature = "lsp", feature = "treesitter"))]
    language_loader: Arc<crate::syntax::loader::LanguageLoader>,
    pub(crate) job_manager: crate::job_manager::JobManager,
    scheduler: scheduler::Scheduler,
    keymap: KeyMap,
    split_tree: SplitTree,
    native_save_handlers: std::collections::HashMap<
        crate::document::runtime::BufferKindId,
        file_ops::NativeSaveHandler<T>,
    >,
    native_reload_handlers: std::collections::HashMap<
        crate::document::runtime::BufferKindId,
        file_ops::NativeReloadHandler<T>,
    >,
    pending_keys: Vec<crate::key::Key>,
    pending_count: usize,
    pending_operator_count: usize,
    pending_operator: Option<PendingOperator>,
    pending_grammar: Option<pending_grammar::PendingGrammar>,
    pending_multi_insert_anchors: Vec<usize>,
    replace_originals: Vec<replace_mode::ReplaceEntry>,
    region_build_recording: Vec<crate::action::Action>,
    expand_history: Vec<(usize, usize)>,
    display_map_cache: Vec<DisplayMapCacheEntry>,
    dot_repeat: DotRepeat,
    panel_layout: Option<PanelLayout>,
    last_notification_generation: u64,
    #[cfg(feature = "plugins")]
    plugin_host: crate::plugin::PluginHost,
    #[cfg(feature = "plugins")]
    plugin_jobs: std::collections::HashMap<u64, plugin_jobs::PluginJob>,
    #[cfg(any(feature = "plugins", feature = "lsp"))]
    popup: crate::floating_window::popup::Popup,
    clipboard_ring: crate::clipboard::ClipboardRing,
    system_clipboard_cache: crate::clipboard::SystemClipboardCache,
    clipboard_panel_register: Option<ClipboardPanelRegister>,
    post_paste_state: Option<PostPasteState>,
    file_load_jobs: std::collections::HashMap<usize, crate::document::DocumentId>,
    #[cfg(feature = "lsp")]
    lsp_manager: crate::lsp::LspManager,
    #[cfg(feature = "lsp")]
    lsp_diagnostics: crate::tracked::Tracked<
        std::collections::BTreeMap<String, Vec<crate::lsp::protocol::LspDiagnostic>>,
    >,
    #[cfg(feature = "lsp")]
    lsp_ready_servers: std::collections::HashSet<String>,
    #[cfg(feature = "lsp")]
    rename_context: Option<(std::path::PathBuf, u32, u32)>,
    dispatch_registry: crate::annotations::registry::DispatchRegistry,
    kind_registry: crate::annotations::registry::KindRegistry,
    #[cfg(feature = "plugins")]
    hovered_annotation: Option<crate::annotations::AnnotationId>,
    #[cfg(feature = "plugins")]
    plugin_mutation_rounds: Option<usize>,
    git_gutter_seen_revision: std::collections::HashMap<crate::document::DocumentId, u64>,
    git_gutter_repo_cache:
        std::collections::HashMap<crate::document::DocumentId, Option<std::path::PathBuf>>,
    git_status_refresh_jobs: std::collections::HashMap<crate::document::DocumentId, usize>,
    search_highlights_synced: Option<(crate::document::DocumentId, u64, String)>,
    startup_first_paint: Option<crate::time::Instant>,
    unrendered_key_count: usize,
    #[cfg(feature = "plugins")]
    entered_document: DocumentId,
    #[cfg(feature = "plugins")]
    frame_events: Vec<EditorEvent>,
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
enum PendingOperator {
    Op(crate::action::OperatorType),
    SurroundAdd(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelKind {
    FileExplorer,
    UndoTree,
    Clipboard,
    #[cfg(feature = "lsp")]
    LocationList,
    Regions,
    BufferList,
}

#[derive(Debug, Clone)]
struct PanelLayout {
    kind: PanelKind,
    dir_win_id: crate::split::window::WindowId,
    preview_win_id: crate::split::window::WindowId,
    dir_doc_id: DocumentId,
    preview_doc_id: DocumentId,
    original_doc_id: DocumentId,
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.term.deinit();
    }
}
