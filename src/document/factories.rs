use super::definitions;
use super::{Document, LineEnding, ViewState};
use crate::annotations::AnnotationStore;
use crate::buffer::TextBuffer;
use crate::document::kind::BufferKind;
use crate::document::runtime::{
    BufferListState, ClipboardState, DirectoryState, DocumentHandle, FileState, GitBlameState,
    GitCommitMessageState, GitLogState, GitRebaseTodoState, GitStatusState, MessagesState,
    ScratchState, StateSlot, TerminalState, UndoTreeState, BUFFER_LIST_STATE_KEY,
    CLIPBOARD_STATE_KEY, DIRECTORY_STATE_KEY, FILE_STATE_KEY, GIT_BLAME_STATE_KEY,
    GIT_COMMIT_MESSAGE_STATE_KEY, GIT_LOG_STATE_KEY, GIT_REBASE_TODO_STATE_KEY,
    GIT_STATUS_STATE_KEY, MESSAGES_STATE_KEY, SCRATCH_STATE_KEY, TERMINAL_STATE_KEY,
    UNDO_TREE_STATE_KEY,
};
use crate::error::{ErrorType, RiftError};
use crate::history::UndoTree;
use crate::split::window::WindowId;
use crate::term::terminal::Terminal;
use crate::term::TerminalEvent;
use definitions::DocumentOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

pub(crate) const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

pub(crate) struct DecodedFile {
    pub(crate) chars: Vec<crate::character::Character>,
    pub(crate) line_ending: LineEnding,
    pub(crate) starts: Vec<usize>,
    pub(crate) has_bom: bool,
}

fn detect_line_ending(bytes: &[u8]) -> LineEnding {
    let mut newlines = 0usize;
    let mut prev = 0u8;
    for &b in bytes {
        if b == b'\n' {
            if prev != b'\r' {
                return LineEnding::LF;
            }
            newlines += 1;
        }
        prev = b;
    }

    if newlines > 0 {
        LineEnding::Crlf
    } else {
        LineEnding::LF
    }
}

pub(crate) fn decode_file_bytes(bytes: &[u8]) -> DecodedFile {
    use crate::character::Character;
    let has_bom = bytes.starts_with(&UTF8_BOM);
    let mut remaining = if has_bom {
        &bytes[UTF8_BOM.len()..]
    } else {
        bytes
    };
    let line_ending = detect_line_ending(remaining);
    let mut chars = Vec::with_capacity(bytes.len());
    let mut starts = vec![0usize];
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(s) => {
                push_decoded(s, &mut chars, &mut starts, line_ending);
                break;
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                // SAFETY: `valid_up_to` is the length of the prefix `from_utf8` validated.
                let valid = unsafe { std::str::from_utf8_unchecked(&remaining[..valid_up_to]) };
                push_decoded(valid, &mut chars, &mut starts, line_ending);
                let error_len = e.error_len().unwrap_or(1);
                for &b in &remaining[valid_up_to..valid_up_to + error_len] {
                    chars.push(Character::Byte(b));
                }
                remaining = &remaining[valid_up_to + error_len..];
            }
        }
    }
    DecodedFile {
        chars,
        line_ending,
        starts,
        has_bom,
    }
}

fn push_decoded(
    s: &str,
    out: &mut Vec<crate::character::Character>,
    starts: &mut Vec<usize>,
    line_ending: LineEnding,
) {
    use crate::character::Character;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\n' {
            out.push(Character::Newline);
            starts.push(out.len());
            continue;
        }

        let is_crlf = line_ending == LineEnding::Crlf && c == '\r' && it.peek() == Some(&'\n');
        if is_crlf {
            continue;
        }
        out.push(Character::from(c));
    }
}

impl Document {
    fn skeleton(id: super::DocumentId, buffer: TextBuffer) -> Self {
        Self {
            id,
            buffer,
            options: DocumentOptions::default(),
            file_path: None,
            readonly_override: false,
            syntax: None,
            history: UndoTree::new(),
            current_transaction: None,
            transaction_depth: 0,
            handle: DocumentHandle::new(id, 0),
            view_state: ViewState::default(),
            kind: BufferKind::file(),
            state: StateSlot::new(FILE_STATE_KEY, FileState),
            vars: std::collections::HashMap::new(),
            custom_highlights: Vec::new(),
            annotations: AnnotationStore::new(),
            selection_set: crate::selection::SelectionSet::default(),
            pending_annotation_snapshot: None,
            annotation_undo_stack: Vec::new(),
            annotation_redo_stack: Vec::new(),
            content_version: 0,
            pending_lsp_edits: Vec::new(),
            lsp_synced_revision: 0,
            lsp_full_sync_needed: false,
            pending_ghost: Vec::new(),
            ghost_paint_active: false,
            ghost_paint_cursor: None,
        }
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        Ok(Self::skeleton(id, buffer))
    }

    /// # Errors
    /// Returns an error if the file cannot be read or its contents cannot be loaded.
    pub(crate) fn from_file(
        id: super::DocumentId,
        path: impl AsRef<Path>,
    ) -> Result<Self, RiftError> {
        let path = path.as_ref();
        let bytes = crate::fs_backend::backend().read_file(path)?;
        Self::from_bytes(id, Some(path), bytes)
    }

    /// # Errors
    /// Returns an error if the text buffer cannot be created.
    pub(super) fn from_bytes(
        id: super::DocumentId,
        path: Option<&Path>,
        bytes: Vec<u8>,
    ) -> Result<Self, RiftError> {
        let decoded = decode_file_bytes(&bytes);
        drop(bytes);
        let table = crate::buffer::rope::PieceTable::new(decoded.chars);
        let line_index =
            crate::buffer::line_index::LineIndex::from_table_with_starts(table, decoded.starts);

        let mut buffer = TextBuffer::new(4096).map_err(io::Error::other)?;
        buffer.line_index = line_index;
        buffer.move_to_start();

        Ok(Self {
            options: DocumentOptions {
                line_ending: decoded.line_ending,
                has_bom: decoded.has_bom,
                ..DocumentOptions::default()
            },
            file_path: path.map(Self::normalize_path),
            ..Self::skeleton(id, buffer)
        })
    }

    #[must_use]
    pub(crate) fn sniff_undo_file(
        bytes: &[u8],
    ) -> Option<Result<crate::history::persist::ParsedUndoFile, RiftError>> {
        if !crate::history::persist::has_magic(bytes) {
            return None;
        }
        Some(
            crate::history::persist::parse_for_display(bytes).map_err(|msg| {
                RiftError::new(
                    ErrorType::Parse,
                    crate::constants::errors::UNDOFILE_CORRUPT,
                    msg,
                )
            }),
        )
    }

    #[must_use]
    pub(crate) fn try_open_undo_file(
        id: super::DocumentId,
        path: &Path,
    ) -> Option<Result<Self, RiftError>> {
        let bytes = crate::fs_backend::backend().read_file(path).ok()?;
        Self::sniff_undo_file(&bytes)
            .map(|result| result.map(|parsed| Self::build_undo_file_view(id, path, &parsed)))
    }

    fn build_undo_file_view(
        id: super::DocumentId,
        path: &Path,
        parsed: &crate::history::persist::ParsedUndoFile,
    ) -> Self {
        let buffer = TextBuffer::new(4096).unwrap_or_else(|_| panic!("Failed to create buffer"));
        let mut doc = Self {
            file_path: Some(Self::normalize_path(path)),
            readonly_override: true,
            kind: BufferKind::undo_file_view(),
            state: StateSlot::new(
                crate::document::runtime::UNDO_FILE_VIEW_STATE_KEY,
                crate::document::runtime::UndoFileViewState::default(),
            ),
            ..Self::skeleton(id, buffer)
        };
        doc.populate_undo_file_view(parsed);
        doc
    }

    /// # Errors
    /// Returns an error if the document has no path, the file cannot be read, or it is not a valid undo file.
    pub(crate) fn reload_undo_file_view(&mut self) -> Result<(), RiftError> {
        let path = self
            .path()
            .ok_or_else(|| {
                RiftError::new(
                    ErrorType::Execution,
                    crate::constants::errors::NO_PATH,
                    crate::constants::errors::MSG_NO_FILE_NAME,
                )
            })?
            .to_path_buf();
        let bytes = crate::fs_backend::backend().read_file(&path)?;
        let parsed = Self::sniff_undo_file(&bytes).unwrap_or_else(|| {
            Err(RiftError::new(
                ErrorType::Parse,
                crate::constants::errors::UNDOFILE_CORRUPT,
                "no longer a valid undo file".to_string(),
            ))
        })?;
        self.populate_undo_file_view(&parsed);
        Ok(())
    }

    /// # Errors
    /// Returns an error if the text buffer or the terminal cannot be created.
    pub(crate) fn new_terminal(
        id: super::DocumentId,
        rows: u16,
        cols: u16,
        shell: Option<String>,
    ) -> Result<(Self, Receiver<TerminalEvent>), RiftError> {
        let buffer = TextBuffer::new(4096).map_err(io::Error::other)?;
        let (terminal, rx) = Terminal::new(rows, cols, shell)
            .map_err(|e| RiftError::new(ErrorType::Internal, "TERMINAL_INIT", e.to_string()))?;

        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::terminal();
        doc.state = StateSlot::new(
            TERMINAL_STATE_KEY,
            TerminalState {
                terminal: Some(terminal),
                ..TerminalState::default()
            },
        );

        Ok((doc, rx))
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_directory(id: super::DocumentId, path: PathBuf) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::directory();
        doc.state = StateSlot::new(
            DIRECTORY_STATE_KEY,
            DirectoryState {
                path,
                entries: Vec::new(),
                show_hidden: false,
            },
        );
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_undotree(
        id: super::DocumentId,
        linked_doc_id: super::DocumentId,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::undotree();
        doc.state = StateSlot::new(
            UNDO_TREE_STATE_KEY,
            UndoTreeState {
                linked_doc_id,
                sequences: Vec::new(),
            },
        );
        Ok(doc)
    }

    pub(crate) fn new_undotree_preview(id: super::DocumentId, linked: &Self) -> Self {
        Self {
            options: DocumentOptions {
                show_line_numbers: false,
                ..linked.options.clone()
            },
            file_path: linked.file_path.clone(),
            readonly_override: true,
            history: linked.history.clone(),
            ..Self::skeleton(id, linked.buffer.clone())
        }
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_messages(id: super::DocumentId, show_all: bool) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::messages();
        doc.state = StateSlot::new(MESSAGES_STATE_KEY, MessagesState { show_all });
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_scratch(
        id: super::DocumentId,
        title: String,
        lines: &[String],
    ) -> Result<Self, RiftError> {
        let content = lines.join("\n");
        let mut buffer = TextBuffer::new(content.len().max(64))?;
        let _ = buffer.insert_str(&content);
        buffer.move_to_start();
        let mut doc = Self::skeleton(id, buffer);
        doc.kind = BufferKind::scratch();
        doc.state = StateSlot::new(SCRATCH_STATE_KEY, ScratchState { title });
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_clipboard(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::clipboard();
        doc.state = StateSlot::new(CLIPBOARD_STATE_KEY, ClipboardState::default());
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_git_status(
        id: super::DocumentId,
        repo_root: PathBuf,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::git_status();
        doc.state = StateSlot::new(
            GIT_STATUS_STATE_KEY,
            GitStatusState {
                repo_root,
                snapshot: crate::git::status::StatusSnapshot::default(),
                expanded_diffs: std::collections::HashMap::new(),
                head_subject: None,
            },
        );
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_git_commit_message(
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
        let mut doc = Self::skeleton(id, buffer);
        doc.kind = BufferKind::git_commit_message();
        doc.state = StateSlot::new(
            GIT_COMMIT_MESSAGE_STATE_KEY,
            GitCommitMessageState { repo_root, target },
        );
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_git_blame(
        id: super::DocumentId,
        repo_root: PathBuf,
        linked_doc_id: super::DocumentId,
        path: PathBuf,
        at_commit: Option<String>,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.options.wrap = Some(super::definitions::WrapMode::Off);
        doc.kind = BufferKind::git_blame();
        doc.state = StateSlot::new(
            GIT_BLAME_STATE_KEY,
            GitBlameState {
                repo_root,
                linked_doc_id,
                linked_window_id: WindowId::new(0),
                path,
                at_commit,
                history: Vec::new(),
                lines: Vec::new(),
                wrap_rows: Vec::new(),
                wrap_key: None,
            },
        );
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_git_log(
        id: super::DocumentId,
        repo_root: PathBuf,
        path: Option<PathBuf>,
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::git_log();
        doc.state = StateSlot::new(
            GIT_LOG_STATE_KEY,
            GitLogState {
                repo_root,
                path,
                commits: Vec::new(),
                expanded: None,
                expanded_body: None,
            },
        );
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_git_rebase_todo(
        id: super::DocumentId,
        repo_root: PathBuf,
        base: String,
        branch: String,
        steps: &[crate::git::rebase::RebaseStep],
    ) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(64)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::git_rebase_todo();
        doc.state = StateSlot::new(
            GIT_REBASE_TODO_STATE_KEY,
            GitRebaseTodoState {
                repo_root,
                base,
                branch,
                pause: None,
                steps: steps.to_vec(),
                message_overrides: std::collections::HashMap::new(),
                expanded_bodies: std::collections::HashSet::new(),
                original_bodies: std::collections::HashMap::new(),
            },
        );
        doc.render_git_rebase_todo("Initial plan");
        doc.history = UndoTree::new();
        doc.annotation_undo_stack.clear();
        doc.annotation_redo_stack.clear();
        Ok(doc)
    }

    /// # Errors
    /// Propagates the error from `TextBuffer::new`.
    pub(crate) fn new_buffer_list(id: super::DocumentId) -> Result<Self, RiftError> {
        let buffer = TextBuffer::new(4096)?;
        let mut doc = Self::skeleton(id, buffer);
        doc.options.show_line_numbers = false;
        doc.kind = BufferKind::buffer_list();
        doc.state = StateSlot::new(BUFFER_LIST_STATE_KEY, BufferListState::default());
        Ok(doc)
    }
}
