//! Document factory constructors — `Document::new`, `from_file`, `new_terminal`, etc.

use super::definitions;
use super::{BufferKind, Document, LineEnding, ViewState};
use crate::buffer::TextBuffer;
use crate::error::{ErrorType, RiftError};
use crate::history::UndoTree;
use crate::term::{Terminal, TerminalEvent};
use definitions::DocumentOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

impl Document {
    /// Create a new empty document
    pub fn new(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            id,
            buffer,
            options: DocumentOptions::default(),
            file_path: None,
            is_read_only: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::File,
            custom_highlights: vec![],
            plugin_highlights: vec![],
            highlight_slots: std::collections::HashMap::new(),
        })
    }

    /// Load document from file
    pub fn from_file(id: super::DocumentId, path: impl AsRef<Path>) -> Result<Self, RiftError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;

        // Detect line endings and normalize
        let mut line_ending = LineEnding::LF;
        let mut normalized_bytes = Vec::with_capacity(bytes.len());
        let mut i = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            3
        } else {
            0
        };
        while i < bytes.len() {
            if bytes[i] == b'\r' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    line_ending = LineEnding::CRLF;
                    normalized_bytes.push(b'\n');
                    i += 2;
                } else {
                    i += 1;
                }
            } else {
                normalized_bytes.push(bytes[i]);
                i += 1;
            }
        }

        let mut buffer =
            TextBuffer::new(normalized_bytes.len().max(4096)).map_err(io::Error::other)?;
        buffer
            .insert_bytes(&normalized_bytes)
            .map_err(io::Error::other)?;
        buffer.move_to_start();

        Ok(Document {
            id,
            buffer,
            options: DocumentOptions {
                line_ending,
                ..DocumentOptions::default()
            },
            file_path: Some(Self::normalize_path(path)),
            is_read_only: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::File,
            custom_highlights: vec![],
            plugin_highlights: vec![],
            highlight_slots: std::collections::HashMap::new(),
        })
    }

    /// Create a new terminal document
    pub fn new_terminal(
        id: super::DocumentId,
        rows: u16,
        cols: u16,
        shell: Option<String>,
    ) -> Result<(Self, Receiver<TerminalEvent>), RiftError> {
        let buffer = TextBuffer::new(4096).map_err(io::Error::other)?;
        let (terminal, rx) = Terminal::new(rows, cols, shell)
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERMINAL_INIT", e.to_string()))?;

        Ok((
            Document {
                id,
                buffer,
                options: DocumentOptions {
                    show_line_numbers: false,
                    ..DocumentOptions::default()
                },
                file_path: None,
                is_read_only: false,
                syntax: None,
                history: UndoTree::new(),
                current_transaction: None,
                view_state: ViewState::default(),
                terminal: Some(terminal),
                terminal_cursor: None,
                kind: BufferKind::Terminal,
                custom_highlights: vec![],
                plugin_highlights: vec![],
                highlight_slots: std::collections::HashMap::new(),
            },
            rx,
        ))
    }

    /// Create a new directory buffer. Content is populated later when a DirectoryListJob completes.
    pub fn new_directory(id: super::DocumentId, path: PathBuf) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            id,
            buffer,
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            file_path: None,
            is_read_only: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::Directory {
                path,
                entries: vec![],
                show_hidden: false,
            },
            custom_highlights: vec![],
            plugin_highlights: vec![],
            highlight_slots: std::collections::HashMap::new(),
        })
    }

    /// Create a new undo-tree buffer linked to another document.
    pub fn new_undotree(
        id: super::DocumentId,
        linked_doc_id: super::DocumentId,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            id,
            buffer,
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            file_path: None,
            is_read_only: true,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::UndoTree {
                linked_doc_id,
                sequences: vec![],
            },
            custom_highlights: vec![],
            plugin_highlights: vec![],
            highlight_slots: std::collections::HashMap::new(),
        })
    }

    /// Create a read-only preview document for the undotree pane.
    pub fn new_undotree_preview(
        id: super::DocumentId,
        linked: &Document,
    ) -> Result<Self, RiftError> {
        Ok(Document {
            id,
            buffer: linked.buffer.clone(),
            options: DocumentOptions {
                show_line_numbers: false,
                ..linked.options.clone()
            },
            file_path: linked.file_path.clone(),
            is_read_only: true,
            syntax: None,
            history: linked.history.clone(),
            current_transaction: None,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::File,
            custom_highlights: vec![],
            plugin_highlights: vec![],
            highlight_slots: std::collections::HashMap::new(),
        })
    }

    /// Create a new messages buffer showing the accumulated notification log.
    pub fn new_messages(id: super::DocumentId, show_all: bool) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            id,
            buffer,
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            file_path: None,
            is_read_only: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::Messages { show_all },
            custom_highlights: vec![],
            plugin_highlights: vec![],
            highlight_slots: std::collections::HashMap::new(),
        })
    }

    pub fn new_clipboard(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            id,
            buffer,
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            file_path: None,
            is_read_only: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::Clipboard { entries: vec![] },
            custom_highlights: vec![],
            plugin_highlights: vec![],
            highlight_slots: std::collections::HashMap::new(),
        })
    }
}
