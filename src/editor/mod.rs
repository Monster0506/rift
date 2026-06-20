//! Editor core
//! Main editor logic that ties everything together

pub mod actions;
mod annotations_ops;

#[cfg(test)]
mod terminal_tests;

mod command_exec;
mod command_line_handler;
mod completion;
mod context_impl;
mod document_ops;
mod explorer;
mod file_ops;
mod handle_action;
mod history;
mod init;
mod jobs;
mod lsp_ops;
mod mode_mgmt;
mod multi_region;
mod operators;
mod panel_handlers;
mod pending_grammar;
mod plugin_ops;
mod rendering;
mod run_loop;
mod text_object_input;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
mod split_move_tests;

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

fn user_config_dir() -> std::path::PathBuf {
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

/// Resolve the wrap/tab width a display map would use, or `None` for no
/// soft-wrap. Cheap, so it can validate a cached display map before reuse.
fn resolve_wrap_params(
    doc: &Document,
    content_width: usize,
    global_soft_wrap: bool,
    global_wrap_width: Option<usize>,
) -> Option<(usize, usize)> {
    use crate::document::definitions::WrapMode;
    // Terminal and directory buffers never use soft-wrap: terminals manage their own
    // cursor, and directory buffers have invisible ID prefixes the display map doesn't know about.
    if doc.is_terminal() || doc.is_directory() {
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

/// Main editor struct
pub struct Editor<T: TerminalBackend> {
    /// Terminal backend
    pub term: T,
    pub document_manager: crate::document::DocumentManager,
    pub render_system: crate::render::RenderSystem,
    current_mode: Mode,
    should_quit: bool,
    state: State,
    command_parser: CommandParser,
    settings_registry: SettingsRegistry<UserSettings>,
    document_settings_registry: SettingsRegistry<crate::document::definitions::DocumentOptions>,
    language_loader: Arc<crate::syntax::loader::LanguageLoader>,
    /// Background job manager
    pub job_manager: crate::job_manager::JobManager,
    /// Job ID required to finish before quitting
    pending_quit_job_id: Option<usize>,
    pub keymap: KeyMap,
    pub split_tree: SplitTree,
    // Input state
    pending_keys: Vec<crate::key::Key>,
    pending_count: usize,
    pending_operator: Option<crate::action::OperatorType>,
    pending_grammar: Option<pending_grammar::PendingGrammar>,
    /// Set while `ys` is waiting for its motion/text-object before the delimiter
    /// char. Carries the delimiter-repeat count captured when the first `s` fired.
    pending_surround_add: Option<usize>,
    /// Char offset of the in-progress Visual region's anchor; `None` outside
    /// Visual/VisualLine/VisualBlock. The cursor side is `buffer.cursor()`.
    pub(super) visual_anchor: Option<usize>,
    /// Insert anchors still waiting for the recorded session to replay at,
    /// once the live-typed Insert session at the first anchor finishes.
    pub(super) pending_multi_insert_anchors: Vec<usize>,
    /// Selection-building actions accumulated since the last time a
    /// set-aware command consumed the set (finalized by Task 22 into
    /// `DotRegister::RegionBuildSession`).
    pub(super) region_build_recording: Vec<crate::action::Action>,
    /// Stack of prior selection extents, for `<Shift-Space>` shrink (Task 24).
    /// Cleared whenever a fresh Visual region starts.
    pub(super) expand_history: Vec<(usize, usize)>,
    /// Cached display map keyed by (doc_id, buffer_revision, content_width).
    /// Avoids rebuilding the soft-wrap map on every command when the buffer hasn't changed.
    display_map_cache: Option<(
        crate::document::DocumentId,
        u64,
        usize,
        Option<crate::wrap::DisplayMap>,
    )>,
    /// Doc whose TextChangedCoarse event is pending dispatch at the next render.
    pending_text_changed: Option<crate::document::DocumentId>,
    /// Latest CursorMoved event pending dispatch at the next render.
    pending_cursor_moved: Option<(crate::document::DocumentId, usize, usize)>,
    dot_repeat: DotRepeat,
    pub panel_layout: Option<PanelLayout>,
    /// Last seen notification generation; used to detect when to refresh open messages buffers.
    last_notification_generation: u64,
    /// Plugin host — dispatches editor events to registered plugin handlers.
    pub plugin_host: crate::plugin::PluginHost,
    /// Clipboard ring buffer — stores yanked/deleted text, capacity 10.
    pub clipboard_ring: crate::clipboard::ClipboardRing,
    /// Cached system-clipboard read for the tooltip, refreshed out-of-band.
    system_clipboard_cache: crate::clipboard::SystemClipboardCache,
    /// Tracks the active paste so <C-n> can cycle to the next ring entry.
    post_paste_state: Option<PostPasteState>,
    /// After navigating to a parent directory, the name of the child entry to
    /// restore the cursor to once the listing arrives.
    pending_cursor_entry: Option<String>,
    /// LSP integration layer.
    pub lsp_manager: crate::lsp::LspManager,
    /// Cached LSP diagnostics per document URI for navigation ([d / ]d).
    lsp_diagnostics: std::collections::HashMap<String, Vec<crate::lsp::protocol::LspDiagnostic>>,
    /// Languages whose server has completed initialization and indexing.
    /// Diagnostic notifications are suppressed until the language appears here.
    lsp_ready_servers: std::collections::HashSet<String>,
    /// Code actions returned by the last textDocument/codeAction request.
    /// Used to apply the selection from the code-action picker panel.
    pending_code_actions: Vec<serde_json::Value>,
    /// Stored position when LSP rename dialog was opened (path, line, col).
    rename_context: Option<(std::path::PathBuf, u32, u32)>,
    /// Deferred goto-definition target: set when the destination file wasn't open
    /// yet and had to be loaded asynchronously. The FileLoadResult handler applies
    /// it once the buffer is populated. Tuple is (doc_id, line, col), 0-indexed.
    pending_goto_target: Option<(crate::document::DocumentId, usize, usize)>,
    /// Resolves annotation (kind, verb) activations to handlers (design.md sec 9.2).
    pub dispatch_registry: crate::annotations::registry::DispatchRegistry,
    /// Per-kind presentation/description defaults applied at render and hover time
    /// when an annotation supplies none (design.md sec 4).
    pub kind_registry: crate::annotations::registry::KindRegistry,
    /// Id of the annotation the cursor currently rests on, tracked so cursor
    /// enter/leave hooks fire once per transition (design.md sec 12).
    hovered_annotation: Option<crate::annotations::AnnotationId>,
    /// Debounced background syntax reparse state per document, keyed off a
    /// sync `try_incremental_parse` exceeding its time budget (design.md #1).
    pending_syntax_reparse:
        std::collections::HashMap<crate::document::DocumentId, jobs::PendingSyntaxReparse>,
}

/// State retained between a `Put` and a `CyclePaste` action.
#[derive(Debug, Clone)]
struct PostPasteState {
    /// Which ring index is currently pasted.
    ring_index: usize,
    /// Whether the paste was before the cursor (`P`) or after (`p`).
    before: bool,
    /// Cursor position before the paste, so cycling can restore it after undo.
    original_cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    FileExplorer,
    UndoTree,
    Clipboard,
    /// Diagnostics or references location list.
    LocationList,
}

/// Tracks the two windows and documents that make up a live explorer session.
#[derive(Debug, Clone)]
pub struct PanelLayout {
    pub kind: PanelKind,
    pub dir_win_id: crate::split::window::WindowId,
    pub preview_win_id: crate::split::window::WindowId,
    pub dir_doc_id: DocumentId,
    pub preview_doc_id: DocumentId,
    /// For FileExplorer: the document that was showing in `dir_win_id` before the explorer opened.
    pub original_doc_id: DocumentId,
}

impl<T: TerminalBackend> Drop for Editor<T> {
    fn drop(&mut self) {
        self.term.deinit();
    }
}
