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
}

/// Diff produced by parsing a directory buffer before save
#[derive(Debug)]
pub struct DirectoryDiff {
    pub renames: Vec<(PathBuf, String)>,
    pub deletes: Vec<PathBuf>,
    pub creates: Vec<String>,
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
    pub syntax: Option<Syntax>,
    pub history: UndoTree,
    current_transaction: Option<EditTransaction>,
    pub view_state: ViewState,
    pub terminal: Option<Terminal>,
    pub terminal_cursor: Option<(usize, usize)>,
    pub kind: BufferKind,
    pub custom_highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    pub plugin_highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    pub terminal_cell_colors: crate::color::CellColorSpans,
    pub highlight_slots:
        std::collections::HashMap<u32, Vec<(std::ops::Range<usize>, crate::color::Color)>>,
}

impl Document {
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
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
