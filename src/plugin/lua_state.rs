//! Data snapshots passed to/from the Lua host. Kept mlua-free so editor code
//! can construct and pass them regardless of whether `plugins` is enabled.

/// Undo/redo availability for the Lua host; two named flags instead of four bools.
#[derive(Debug, Clone, Copy, Default)]
pub struct HistoryAvailability {
    pub can_undo: bool,
    pub can_redo: bool,
}

/// A lightweight buffer entry for the `rift.get_buf_list()` snapshot.
#[derive(Debug, Clone)]
pub struct BufEntry {
    pub id: usize,
    pub name: String,
    pub is_dirty: bool,
    pub is_current: bool,
    /// Kind string: "file", "terminal", "directory", "undotree", "messages", etc.
    pub kind: String,
    /// Absolute path if the buffer has one.
    pub path: Option<String>,
    /// Number of lines in the buffer.
    pub line_count: usize,
    pub is_read_only: bool,
}

/// A read-only view of one annotation for the `rift.annotations` query snapshot.
#[derive(Debug, Clone)]
pub struct AnnotationView {
    pub id: u64,
    /// Cloning this Arc avoids allocating a string per annotation.
    pub kind: std::sync::Arc<str>,
    /// Owner tag, always one of a fixed literal set (`AnnotationOwner::as_str`),
    /// so no allocation is needed to carry it here.
    pub owner: &'static str,
    /// "point", "range", or "line".
    pub anchor: &'static str,
    /// Byte offset (point/range start) or line number (line anchor).
    pub start: usize,
    /// Range end byte offset; equals `start` for point/line anchors.
    pub end: usize,
    pub payload: crate::annotations::Value,
    pub visible: bool,
    pub interactive: bool,
}

/// Deferred source for the active buffer's lines: `update_state` stores a cheap
/// buffer clone, and the getters materialize `Vec<String>` only when Lua reads.
pub struct BufLinesSource {
    pub revision: u64,
    pub line_count: usize,
    pub buffer: crate::buffer::TextBuffer,
}

/// A lightweight entry for the `rift.windows.list()` snapshot.
#[derive(Debug, Clone)]
pub struct WinEntry {
    pub id: u64,
    pub buf: usize,
    pub row: usize,
    pub col: usize,
    pub rows: usize,
    pub cols: usize,
}

/// How `update_state` treats the shared buffer snapshot this call.
#[derive(Default)]
pub enum BufSourceUpdate {
    /// The buffer hasn't changed since the last sync; keep the existing snapshot.
    #[default]
    Unchanged,
    /// There's no active document; drop any existing snapshot.
    Cleared,
    /// The buffer changed; replace the snapshot with this one.
    Set(Box<BufLinesSource>),
}

/// Everything the Lua host takes before one dispatch. One value keeps the
/// real host, the plugins-off stub, and `PluginHost` on a single signature.
#[derive(Default)]
pub struct StateUpdate<'a> {
    pub buf_id: usize,
    pub buf_kind: String,
    pub source: BufSourceUpdate,
    pub cursor: (usize, usize),
    pub tab_width: usize,
    pub expand_tabs: bool,
    pub mode: &'a str,
    pub filetype: Option<String>,
    pub file_path: Option<String>,
    pub buf_list: Vec<BufEntry>,
    pub window_size: (u16, u16),
    pub history: HistoryAvailability,
    pub is_dirty: bool,
    pub scroll: (usize, usize),
    pub line_ending: &'a str,
    pub commands: Vec<(String, String)>,
    pub win_list: Vec<WinEntry>,
    pub focused_win_id: u64,
    pub previous_win_id: Option<u64>,
    /// `normalized_uri` -> `(line, col, severity, message)`.
    pub lsp_diagnostics: std::collections::HashMap<String, Vec<(u32, u32, u32, String)>>,
    pub buffer_vars: std::collections::HashMap<
        u64,
        std::collections::HashMap<String, crate::annotations::Value>,
    >,
    pub doc_options: crate::document::definitions::DocumentOptions,
    pub user_settings: crate::state::UserSettings,
}
