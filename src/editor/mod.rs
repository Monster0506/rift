//! Editor core
//! Main editor logic that ties everything together

pub mod actions;

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
mod mode_mgmt;
mod operators;
mod panel_handlers;
mod plugin_ops;
mod rendering;
mod run_loop;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

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

fn plugin_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        dirs.push(
            std::path::PathBuf::from(manifest)
                .join("runtime")
                .join("plugins"),
        );
    }
    let user_dir = if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(|d| std::path::PathBuf::from(d).join("rift").join("plugins"))
    } else {
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
            });
        Some(base.join("rift").join("plugins"))
    };
    if let Some(d) = user_dir {
        dirs.push(d);
    }
    dirs
}

fn resolve_display_map(
    doc: &Document,
    content_width: usize,
    global_soft_wrap: bool,
    global_wrap_width: Option<usize>,
) -> Option<crate::wrap::DisplayMap> {
    use crate::document::definitions::WrapMode;
    if doc.is_terminal() {
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
    Some(crate::wrap::DisplayMap::build(
        &doc.buffer,
        w,
        doc.options.tab_width,
    ))
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
    pending_find_char_dir: Option<(bool, bool)>,
    dot_repeat: DotRepeat,
    pub panel_layout: Option<PanelLayout>,
    /// Last seen notification generation; used to detect when to refresh open messages buffers.
    last_notification_generation: u64,
    /// Plugin host — dispatches editor events to registered plugin handlers.
    pub plugin_host: crate::plugin::PluginHost,
    /// Clipboard ring buffer — stores yanked/deleted text, capacity 10.
    pub clipboard_ring: crate::clipboard::ClipboardRing,
    /// Tracks the active paste so <C-n> can cycle to the next ring entry.
    post_paste_state: Option<PostPasteState>,
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
