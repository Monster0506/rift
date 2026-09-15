//! Document factory constructors: `Document::new`, `from_file`, `new_terminal`, etc.

use super::definitions;
use super::{BufferKind, Document, LineEnding, ViewState};
use crate::annotations::AnnotationStore;
use crate::buffer::TextBuffer;
use crate::error::{ErrorType, RiftError};
use crate::history::UndoTree;
use crate::term::{Terminal, TerminalEvent};
use definitions::DocumentOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

/// Decode raw file bytes into buffer characters plus line starts, normalizing
/// CRLF (and a standalone `\r`, which is dropped) and skipping a UTF-8 BOM.
pub(crate) fn decode_file_bytes(
    bytes: &[u8],
) -> (Vec<crate::character::Character>, LineEnding, Vec<usize>) {
    use crate::character::Character;
    let mut line_ending = LineEnding::LF;
    let mut chars = Vec::with_capacity(bytes.len());
    let mut starts = vec![0usize];
    let mut remaining = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(s) => {
                push_normalized(s, &mut chars, &mut starts, &mut line_ending);
                break;
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                // SAFETY: from_utf8 guarantees remaining[..valid_up_to] is valid UTF-8
                let valid = unsafe { std::str::from_utf8_unchecked(&remaining[..valid_up_to]) };
                push_normalized(valid, &mut chars, &mut starts, &mut line_ending);
                let error_len = e.error_len().unwrap_or(1);
                for &b in &remaining[valid_up_to..valid_up_to + error_len] {
                    chars.push(Character::Byte(b));
                }
                remaining = &remaining[valid_up_to + error_len..];
            }
        }
    }
    (chars, line_ending, starts)
}

fn push_normalized(
    s: &str,
    out: &mut Vec<crate::character::Character>,
    starts: &mut Vec<usize>,
    line_ending: &mut LineEnding,
) {
    use crate::character::Character;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            if it.peek() == Some(&'\n') {
                *line_ending = LineEnding::CRLF;
                it.next();
                out.push(Character::Newline);
                starts.push(out.len());
            }
        } else if c == '\n' {
            out.push(Character::Newline);
            starts.push(out.len());
        } else {
            out.push(Character::from(c));
        }
    }
}

impl Document {
    /// A document with every field at its default value; only `id` and
    /// `buffer` are meaningful. Callers override fields via `..Self::skeleton(..)`.
    fn skeleton(id: super::DocumentId, buffer: TextBuffer) -> Document {
        Document {
            id,
            buffer,
            options: DocumentOptions::default(),
            file_path: None,
            is_read_only: false,
            interface_mode: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            transaction_depth: 0,
            view_state: ViewState::default(),
            terminal: None,
            terminal_cursor: None,
            kind: BufferKind::File,
            custom_highlights: vec![],
            plugin_highlights: vec![],
            terminal_cell_colors: vec![],
            highlight_slots: std::collections::HashMap::new(),
            annotations: AnnotationStore::new(),
            selection_set: crate::selection::SelectionSet::default(),
            pending_annotation_snapshot: None,
            annotation_undo_stack: Vec::new(),
            annotation_redo_stack: Vec::new(),
            document_version: 0,
            pending_lsp_edits: Vec::new(),
            lsp_synced_revision: 0,
            lsp_full_sync_needed: false,
            pending_ghost: Vec::new(),
        }
    }

    /// Create a new empty document
    pub fn new(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Self::skeleton(id, buffer))
    }

    /// Load document from file
    pub fn from_file(id: super::DocumentId, path: impl AsRef<Path>) -> Result<Self, RiftError> {
        let path = path.as_ref();
        let bytes = crate::fs_backend::backend().read_file(path)?;
        Self::from_bytes(id, Some(path), bytes)
    }

    /// Build a document from raw bytes already in memory, skipping the
    /// filesystem read. `path` sets the document's file path, if any.
    pub fn from_bytes(
        id: super::DocumentId,
        path: Option<&Path>,
        bytes: Vec<u8>,
    ) -> Result<Self, RiftError> {
        // Bulk construction: the chars vec becomes the piece table's original
        // slice directly, skipping the general insert path and its copies.
        let (chars, line_ending, starts) = decode_file_bytes(&bytes);
        drop(bytes);
        let table = crate::buffer::rope::PieceTable::new(chars);
        let line_index =
            crate::buffer::line_index::LineIndex::from_table_with_starts(table, starts);

        let mut buffer = TextBuffer::new(4096).map_err(io::Error::other)?;
        buffer.line_index = line_index;
        buffer.move_to_start();

        Ok(Document {
            options: DocumentOptions {
                line_ending,
                ..DocumentOptions::default()
            },
            file_path: path.map(Self::normalize_path),
            ..Self::skeleton(id, buffer)
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
                options: DocumentOptions {
                    show_line_numbers: false,
                    ..DocumentOptions::default()
                },
                terminal: Some(terminal),
                kind: BufferKind::Terminal,
                ..Self::skeleton(id, buffer)
            },
            rx,
        ))
    }

    /// Create a new directory buffer. Content is populated later when a DirectoryListJob completes.
    pub fn new_directory(id: super::DocumentId, path: PathBuf) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            // The explorer is the canonical interface-mode buffer: its rows are
            // fs.entry annotations activated through the dispatch registry.
            interface_mode: true,
            kind: BufferKind::Directory {
                path,
                entries: vec![],
                show_hidden: false,
            },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create a new undo-tree buffer linked to another document.
    pub fn new_undotree(
        id: super::DocumentId,
        linked_doc_id: super::DocumentId,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            is_read_only: true,
            interface_mode: true,
            kind: BufferKind::UndoTree {
                linked_doc_id,
                sequences: vec![],
            },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create a read-only preview document for the undotree pane.
    pub fn new_undotree_preview(
        id: super::DocumentId,
        linked: &Document,
    ) -> Result<Self, RiftError> {
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..linked.options.clone()
            },
            file_path: linked.file_path.clone(),
            // Read-only copy of a linked file with ordinary line-by-line navigation.
            is_read_only: true,
            history: linked.history.clone(),
            ..Self::skeleton(id, linked.buffer.clone())
        })
    }

    /// Create a new messages buffer showing the accumulated notification log.
    pub fn new_messages(id: super::DocumentId, show_all: bool) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            kind: BufferKind::Messages { show_all },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create an in-memory buffer with no disk path, populated with `lines`.
    /// Used by `rift.create_scratch_buf`
    pub fn new_scratch(
        id: super::DocumentId,
        title: String,
        lines: &[String],
    ) -> Result<Self, RiftError> {
        let content = lines.join("\n");
        let mut buffer = TextBuffer::new(content.len().max(64))?;
        let _ = buffer.insert_str(&content);
        buffer.move_to_start();
        Ok(Document {
            kind: BufferKind::Scratch { title },
            ..Self::skeleton(id, buffer)
        })
    }

    pub fn new_clipboard(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            kind: BufferKind::Clipboard { entries: vec![] },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create a new git status buffer. Content is populated later when a `GitStatusJob` completes. Read-only: changes only happen through its specific key actions (`s`/`u`/`X`/`=`/`c...`), never by editing the rendered text directly (that content is regenerated on every refresh anyway, and the line-anchored.
    pub fn new_git_status(id: super::DocumentId, repo_root: PathBuf) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            // Status buffer rows are git.status_entry/git.hunk annotations
            // These rows are interactive through the action dispatch registry.
            interface_mode: true,
            is_read_only: true,
            kind: BufferKind::GitStatus {
                repo_root,
                snapshot: crate::git::status::StatusSnapshot::default(),
                expanded_diffs: std::collections::HashMap::new(),
                head_subject: None,
            },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create a new commit message buffer, pre-filled with `initial_message`
    /// (empty for a new commit, the previous message for amend/reword).
    pub fn new_git_commit_message(
        id: super::DocumentId,
        repo_root: PathBuf,
        target: super::GitCommitTarget,
        initial_message: &str,
    ) -> Result<Self, RiftError> {
        let mut buffer = TextBuffer::new(initial_message.len().max(64))?;
        if !initial_message.is_empty() {
            let _ = buffer.insert_str(initial_message);
            buffer.move_to_start();
        }
        Ok(Document {
            kind: BufferKind::GitCommitMessage { repo_root, target },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create a new git blame buffer. Content is populated later when a
    /// `GitBlameJob` completes.
    pub fn new_git_blame(
        id: super::DocumentId,
        repo_root: PathBuf,
        linked_doc_id: super::DocumentId,
        path: PathBuf,
        at_commit: Option<String>,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            is_read_only: true,
            interface_mode: true,
            kind: BufferKind::GitBlame {
                repo_root,
                linked_doc_id,
                path,
                at_commit,
                lines: vec![],
            },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create a new git log buffer. Content is populated later when a
    /// `GitLogJob` completes.
    pub fn new_git_log(
        id: super::DocumentId,
        repo_root: PathBuf,
        path: Option<PathBuf>,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            is_read_only: true,
            interface_mode: true,
            kind: BufferKind::GitLog {
                repo_root,
                path,
                commits: vec![],
                expanded: None,
                expanded_body: None,
            },
            ..Self::skeleton(id, buffer)
        })
    }

    /// Create a new git rebase todo buffer, pre-populated with `steps`.
    pub fn new_git_rebase_todo(
        id: super::DocumentId,
        repo_root: PathBuf,
        base: String,
        saved_head: String,
        branch: String,
        steps: &[crate::git::rebase::RebaseStep],
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(64)?;
        let mut doc = Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            interface_mode: true,
            is_read_only: true,
            kind: BufferKind::GitRebaseTodo {
                repo_root,
                base,
                saved_head,
                branch,
                pause: None,
                steps: steps.to_vec(),
                message_overrides: std::collections::HashMap::new(),
                expanded_bodies: std::collections::HashSet::new(),
                original_bodies: std::collections::HashMap::new(),
            },
            ..Self::skeleton(id, buffer)
        };
        doc.render_git_rebase_todo("Initial plan");
        doc.history = UndoTree::new();
        Ok(doc)
    }

    /// Create a new interactive buffer-list panel document (read-only, interface-mode).
    pub fn new_buffer_list(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Document {
            options: DocumentOptions {
                show_line_numbers: false,
                ..DocumentOptions::default()
            },
            is_read_only: true,
            interface_mode: true,
            kind: BufferKind::BufferList { entries: vec![] },
            ..Self::skeleton(id, buffer)
        })
    }
}
