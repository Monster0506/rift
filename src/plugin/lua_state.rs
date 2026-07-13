//! Data snapshots passed to/from the Lua host. Kept mlua-free so editor code
//! can construct and pass them regardless of whether `plugins` is enabled.

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
    pub kind: String,
    pub owner: String,
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
