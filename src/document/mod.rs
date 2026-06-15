//! Document management
//! Encapsulates buffer + file metadata for multi-buffer support

pub mod definitions;
mod edit;
mod factories;
mod history;
pub mod manager;
mod persistence;
mod populate;
mod search;

use crate::annotations::AnnotationStore;
use crate::buffer::TextBuffer;
use crate::history::{EditSeq, EditTransaction, UndoTree};
use crate::syntax::Syntax;
use crate::term::Terminal;
use definitions::DocumentOptions;
pub use manager::DocumentManager;
use std::path::PathBuf;

/// Unique identifier for documents
pub type DocumentId = u64;

/// A single entry in a directory buffer
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Stable identifier assigned at populate time. 0 means "not yet assigned".
    pub id: u16,
}

/// Diff produced by parsing a directory buffer before save
#[derive(Debug, Default)]
pub struct DirectoryDiff {
    pub renames: Vec<(PathBuf, String)>,
    pub deletes: Vec<PathBuf>,
    pub creates: Vec<String>,
}

/// A single entry in a location list (diagnostics, references, etc.)
#[derive(Debug, Clone)]
pub struct LocationEntry {
    /// Document URI for this location.
    pub uri: String,
    /// 0-indexed line.
    pub line: u32,
    /// 0-indexed column.
    pub col: u32,
    /// Pre-formatted display string shown in the buffer.
    pub display: String,
}

/// Identifies the role and behaviour of a document
#[derive(Debug, Clone)]
pub enum BufferKind {
    /// Regular file buffer (default)
    File,
    /// Terminal emulator buffer
    Terminal,
    /// Directory browser
    Directory {
        path: PathBuf,
        /// Snapshot of entries at populate time; used to diff user edits on :w
        entries: Vec<DirEntry>,
        /// Whether hidden files (dot-files) are shown
        show_hidden: bool,
    },
    /// Undo tree visualisation for a linked document
    UndoTree {
        linked_doc_id: DocumentId,
        /// Maps buffer line index → EditSeq; u64::MAX = non-navigable connector line
        sequences: Vec<EditSeq>,
    },
    /// Messages log buffer showing all editor notifications
    Messages {
        /// When true, shows all job events including silent ones
        show_all: bool,
    },
    /// Clipboard ring index buffer — editable, :w syncs back to the ring
    Clipboard {
        /// Snapshot of ring entries at populate time; used for content-matching on save
        entries: Vec<String>,
    },
    /// Scratch buffer for editing a single clipboard ring entry in place.
    ClipboardEntry { entry_index: Option<usize> },
    /// Read-only location list (diagnostics, references, quickfix).
    LocationList {
        source_doc_id: DocumentId,
        entries: Vec<LocationEntry>,
    },
}

impl BufferKind {
    /// Short lowercase string identifier for this kind (e.g. "file", "terminal").
    pub fn kind_str(&self) -> &'static str {
        match self {
            BufferKind::File => "file",
            BufferKind::Terminal => "terminal",
            BufferKind::Directory { .. } => "directory",
            BufferKind::UndoTree { .. } => "undotree",
            BufferKind::Messages { .. } => "messages",
            BufferKind::Clipboard { .. } => "clipboard",
            BufferKind::ClipboardEntry { .. } => "clipboard_entry",
            BufferKind::LocationList { .. } => "location_list",
        }
    }
}

/// Line ending types supported by Rift
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix line endings (\n)
    LF,
    /// Windows line endings (\r\n)
    CRLF,
}

impl LineEnding {
    /// Get the byte sequence for this line ending
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            LineEnding::LF => b"\n",
            LineEnding::CRLF => b"\r\n",
        }
    }
}

/// Per-document view state (scroll position, etc.)
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    pub top_line: usize,
    pub left_col: usize,
}

/// Document combining buffer and file metadata
pub struct Document {
    pub id: DocumentId,
    pub buffer: TextBuffer,
    pub options: DocumentOptions,
    file_path: Option<PathBuf>,
    pub is_read_only: bool,
    /// Interface-mode buffer (design.md sec 9.4): read-only, vertical navigation
    /// snaps between actionable lines (magit/explorer/undotree as buffers).
    pub interface_mode: bool,
    pub syntax: Option<Syntax>,
    pub history: UndoTree,
    current_transaction: Option<EditTransaction>,
    transaction_depth: usize,
    pub view_state: ViewState,
    pub terminal: Option<Terminal>,
    pub terminal_cursor: Option<(usize, usize)>,
    pub kind: BufferKind,
    pub custom_highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    pub plugin_highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    pub terminal_cell_colors: crate::color::CellColorSpans,
    pub highlight_slots:
        std::collections::HashMap<u32, Vec<(std::ops::Range<usize>, crate::color::Color)>>,
    /// Structured metadata sidecar.
    pub annotations: AnnotationStore,
    /// Full annotation snapshot captured before a transaction, restored on undo.
    pending_annotation_snapshot: Option<Vec<crate::annotations::Annotation>>,
    /// Undo stack of full annotation snapshots, parallel to the edit history.
    annotation_undo_stack: Vec<Vec<crate::annotations::Annotation>>,
    /// Redo stack of full annotation snapshots.
    annotation_redo_stack: Vec<Vec<crate::annotations::Annotation>>,
    /// Monotonic edit sequence number, incremented once per applied edit.
    /// Lets producers reconcile stale annotation positions (design.md sec 11).
    document_version: u64,
}

impl Document {
    /// Monotonic edit sequence number for this document.
    pub fn version(&self) -> u64 {
        self.document_version
    }

    pub fn set_syntax(&mut self, syntax: Syntax) {
        self.syntax = Some(syntax);
    }

    /// Check if this document is a terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self.kind, BufferKind::Terminal)
    }

    /// Check if this document is a directory buffer
    pub fn is_directory(&self) -> bool {
        matches!(self.kind, BufferKind::Directory { .. })
    }

    /// Check if this document is an undo-tree buffer
    pub fn is_undotree(&self) -> bool {
        matches!(self.kind, BufferKind::UndoTree { .. })
    }

    /// Check if this document is a messages buffer
    pub fn is_messages(&self) -> bool {
        matches!(self.kind, BufferKind::Messages { .. })
    }

    /// Check if this document is a clipboard index buffer
    pub fn is_clipboard(&self) -> bool {
        matches!(self.kind, BufferKind::Clipboard { .. })
    }

    /// Check if this document is a location list buffer (diagnostics/references).
    pub fn is_location_list(&self) -> bool {
        matches!(self.kind, BufferKind::LocationList { .. })
    }

    /// Check if this document is any clipboard-related buffer
    pub fn is_any_clipboard(&self) -> bool {
        matches!(
            self.kind,
            BufferKind::Clipboard { .. } | BufferKind::ClipboardEntry { .. }
        )
    }

    /// Returns true for any non-file buffer.
    pub fn is_special(&self) -> bool {
        !matches!(self.kind, BufferKind::File)
    }

    /// Whether this buffer is in interface mode (read-only + snapping
    /// navigation between actionable regions, design.md sec 9.4).
    pub fn is_interface_mode(&self) -> bool {
        self.interface_mode
    }

    /// Flag this buffer as an interface-mode buffer. Also marks it read-only.
    pub fn set_interface_mode(&mut self, on: bool) {
        self.interface_mode = on;
        if on {
            self.is_read_only = true;
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
