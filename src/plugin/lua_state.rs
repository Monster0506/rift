//! Data snapshots passed to/from the Lua host. Kept mlua-free so editor code
//! can construct and pass them regardless of whether `plugins` is enabled.

/// Undo/redo availability for the Lua host; two named flags instead of four bools.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HistoryAvailability {
    pub(crate) can_undo: bool,
    pub(crate) can_redo: bool,
}

/// A lightweight buffer entry for the `rift.get_buf_list()` snapshot.
#[derive(Debug, Clone)]
pub(crate) struct BufEntry {
    pub(crate) id: usize,
    pub(crate) name: String,
    pub(crate) is_dirty: bool,
    pub(crate) is_current: bool,
    /// Kind string: "file", "terminal", "directory", "undotree", "messages", etc.
    pub(crate) kind: String,
    /// Absolute path if the buffer has one.
    pub(crate) path: Option<String>,
    /// Number of lines in the buffer.
    pub(crate) line_count: usize,
    pub(crate) is_read_only: bool,
}

/// A read-only view of one annotation for the `rift.annotations` query snapshot.
#[derive(Debug, Clone)]
pub(crate) struct AnnotationView {
    pub(crate) id: u64,
    /// Cloning this Arc avoids allocating a string per annotation.
    pub(crate) kind: std::sync::Arc<str>,
    /// Owner tag, always one of a fixed literal set (`AnnotationOwner::as_str`),
    /// so no allocation is needed to carry it here.
    pub(crate) owner: &'static str,
    /// "point", "range", or "line".
    pub(crate) anchor: &'static str,
    /// Byte offset (point/range start) or line number (line anchor).
    pub(crate) start: usize,
    /// Range end byte offset; equals `start` for point/line anchors.
    pub(crate) end: usize,
    pub(crate) payload: crate::annotations::Value,
    pub(crate) visible: bool,
    pub(crate) interactive: bool,
}

/// Deferred source for the active buffer's lines: `update_state` stores a cheap
/// buffer clone, and the getters materialize `Vec<String>` only when Lua reads.
pub(crate) struct BufLinesSource {
    pub(crate) revision: u64,
    pub(crate) line_count: usize,
    pub(crate) buffer: crate::buffer::TextBuffer,
}

/// A lightweight entry for the `rift.windows.list()` snapshot.
#[derive(Debug, Clone)]
pub(crate) struct WinEntry {
    pub(crate) id: u64,
    pub(crate) buf: usize,
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
}

/// How `update_state` treats the shared buffer snapshot this call.
#[derive(Default)]
pub(crate) enum BufSourceUpdate {
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
pub(crate) struct StateUpdate<'a> {
    pub(crate) buf_id: usize,
    pub(crate) buf_kind: String,
    pub(crate) source: BufSourceUpdate,
    pub(crate) cursor: (usize, usize),
    pub(crate) tab_width: usize,
    pub(crate) expand_tabs: bool,
    pub(crate) mode: &'a str,
    pub(crate) filetype: Option<String>,
    pub(crate) file_path: Option<String>,
    pub(crate) buf_list: Vec<BufEntry>,
    pub(crate) window_size: (u16, u16),
    pub(crate) history: HistoryAvailability,
    pub(crate) is_dirty: bool,
    pub(crate) scroll: (usize, usize),
    pub(crate) line_ending: &'a str,
    pub(crate) commands: Vec<(String, String)>,
    pub(crate) win_list: Vec<WinEntry>,
    pub(crate) focused_win_id: u64,
    pub(crate) previous_win_id: Option<u64>,
    /// `normalized_uri` -> `(line, col, severity, message)`.
    pub(crate) lsp_diagnostics: std::collections::HashMap<String, Vec<(u32, u32, u32, String)>>,
    pub(crate) buffer_vars: std::collections::HashMap<
        u64,
        std::collections::HashMap<String, crate::annotations::Value>,
    >,
    pub(crate) doc_options: crate::document::definitions::DocumentOptions,
    pub(crate) user_settings: crate::state::UserSettings,
}
