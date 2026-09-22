//! Document management
//! Encapsulates buffer + file metadata for multi-buffer support

pub mod definitions;
mod edit;
mod factories;
mod ghost;
mod history;
mod kind;
pub mod manager;
mod persistence;
mod populate;
pub mod runtime;
mod search;
mod selection_render;

use crate::annotations::AnnotationStore;
#[cfg(feature = "lsp")]
use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::history::{EditTransaction, UndoTree};
use crate::syntax::Syntax;
use crate::term::Terminal;
use definitions::DocumentOptions;
pub(crate) use factories::decode_file_bytes;
pub use kind::BufferKind;
pub use manager::{
    CreationReservation, DocumentDraft, DocumentManager, DraftCommitTarget, PreparedRemoval,
    RemovalIntent,
};
pub use runtime::{
    builtin_descriptor, ActionDispatch, BufferKindId, BufferKindRegistry, BufferListState,
    BufferPolicies, ClipboardEntryState, ClipboardState, CloseHandler, ClosePolicy,
    DescriptorOwner, DirectoryState, DisplayNameStrategy, DocumentHandle, FileState,
    GhostCutPolicy, GitBlameState, GitCommitMessageState, GitLogState, GitRebaseTodoState,
    GitStatusState, InputPolicy, KeyFallback, KeyFallbackPolicy, KindDescriptor,
    LanguageServicesPolicy, LocationListState, MessagesState, NativeActionHandler,
    NativeCloseHandler, NativeSaveHandler, NavigationPolicy, PluginBufferState, ReadOnlyPolicy,
    RegionsState, RegistryError, SaveDispatch, SaveResult, ScratchState, StateKey, StateKeyId,
    StateSlot, StructuralEditPolicy, TerminalState, TextProjection, TextProjectionPolicy,
    TombstoneMetadata, UndoTreeState, BUFFER_LIST_STATE_KEY, CLIPBOARD_ENTRY_STATE_KEY,
    CLIPBOARD_STATE_KEY, DIRECTORY_STATE_KEY, EMPTY_STATE_KEY, FILE_STATE_KEY, GIT_BLAME_STATE_KEY,
    GIT_COMMIT_MESSAGE_STATE_KEY, GIT_LOG_STATE_KEY, GIT_REBASE_TODO_STATE_KEY,
    GIT_STATUS_STATE_KEY, LOCATION_LIST_STATE_KEY, MESSAGES_STATE_KEY, PLUGIN_BUFFER_STATE_KEY,
    REGIONS_STATE_KEY, SCRATCH_STATE_KEY, TERMINAL_STATE_KEY, UNDO_TREE_STATE_KEY,
};
use std::path::{Path, PathBuf};

/// Unique identifier for documents
pub type DocumentId = u64;

/// One entry on the annotation undo/redo stacks. A pure insertion replays
/// its exact inverse shift; deletes/replaces (which can collapse markers) snapshot.
enum AnnotationUndo {
    /// Pure insertion: bytes [start, new_end) inserted, lines inserted at
    /// `line_inserts` in order. Undo/redo replay the inverse/forward edit.
    Insertion {
        start: usize,
        new_end: usize,
        line_inserts: Vec<usize>,
    },
    /// Full pre-edit annotation snapshot (correct for any edit).
    Snapshot(Vec<crate::annotations::Annotation>),
}

/// Hint passed to `record_edit` for the annotation undo entry. Kept separate
/// from [`AnnotationUndo`] so the snapshot is only taken when needed.
pub(crate) enum AnnotationUndoHint {
    Insertion {
        start: usize,
        new_end: usize,
        line_inserts: Vec<usize>,
    },
    Snapshot,
}

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

/// A git-state mutation invoked by GitStatus cursor actions. Unmerged entries require conflict resolution and have no action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitStatusAction {
    /// `git add -- path` (untracked or unstaged -> staged, whole file).
    Stage(PathBuf),
    /// `git restore --staged -- path` (staged -> unstaged, whole file).
    Unstage(PathBuf),
    /// Revert all changes to `path`: tracked files via `git restore --staged --worktree` (also restoring `orig_path`, if this was a rename), untracked files by deleting them from disk.
    Discard {
        path: PathBuf,
        orig_path: Option<PathBuf>,
        was_untracked: bool,
    },
    /// Apply exactly this hunk to the index (`git apply --cached`): a hunk block moved from the Unstaged section into Staged, or a portion of an untracked file's content (`is_new_file`: no index entry exists for `path` yet, so the patch needs a "new file" header).
    StageHunk {
        path: PathBuf,
        hunk: crate::git::diff::Hunk,
        is_new_file: bool,
    },
    /// Reverse-apply this hunk from the index (`git apply --cached -R`): a
    /// hunk block moved from the Staged section into Unstaged.
    UnstageHunk {
        path: PathBuf,
        hunk: crate::git::diff::Hunk,
    },
    /// Reverse-apply this hunk to discard it entirely: for a staged hunk (`staged_side: true`), reverts both the index (`git apply --cached -R`) and the worktree (`git apply -R`); for an unstaged hunk, reverts only the worktree (`git apply -R`).
    DiscardHunk {
        path: PathBuf,
        hunk: crate::git::diff::Hunk,
        staged_side: bool,
    },
}

/// A deferred `d`-cut: `text` still sits in the buffer, greyed out, until
/// a later action turns it into a real delete.
pub struct GhostCut {
    pub start: usize,
    pub end: usize,
    pub text: Vec<crate::character::Character>,
    pub annotation_id: crate::annotations::AnnotationId,
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

/// What saving a git-commit-message buffer does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCommitTarget {
    /// `git commit -F <file>`.
    New,
    /// `git commit --amend -F <file>`.
    Amend,
    /// Message for a paused rebase `reword` step: saving amends the just-cherry-picked commit with this message, then resumes `rebase_doc_id`'s remaining steps.
    RebaseReword { rebase_doc_id: DocumentId },
    /// Message for a commit still being planned in `rebase_doc_id` (not cherry-picked yet; the rebase hasn't started). Saving updates that todo's `message_overrides` for `sha` and returns to it; no git command runs here at all.
    RebasePlanReword {
        rebase_doc_id: DocumentId,
        sha: String,
    },
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
    /// `File`/`Scratch` per-instance read-only override; ignored for any
    /// kind whose read-only-ness is fixed (see `BufferKind::fixed_read_only`).
    readonly_override: bool,
    pub syntax: Option<Syntax>,
    pub history: UndoTree,
    current_transaction: Option<EditTransaction>,
    transaction_depth: usize,
    pub handle: DocumentHandle,
    pub view_state: ViewState,
    pub kind: BufferKind,
    pub state: StateSlot,
    pub vars: std::collections::HashMap<String, crate::annotations::Value>,
    pub custom_highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    pub plugin_highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    pub highlight_slots:
        std::collections::HashMap<u32, Vec<(std::ops::Range<usize>, crate::color::Color)>>,
    /// Structured metadata sidecar.
    pub annotations: AnnotationStore,
    /// Non-contiguous multi-region selection set.
    pub selection_set: crate::selection::SelectionSet,
    /// Full annotation snapshot captured before a transaction, restored on undo.
    pending_annotation_snapshot: Option<Vec<crate::annotations::Annotation>>,
    /// Undo stack parallel to the edit history; one entry per standalone
    /// edit or committed transaction.
    annotation_undo_stack: Vec<AnnotationUndo>,
    /// Redo annotations paired with the edit-history redo stack.
    annotation_redo_stack: Vec<AnnotationUndo>,
    /// Monotonic edit sequence number, incremented once per applied edit.
    /// Lets producers reconcile stale annotation positions.
    document_version: u64,
    /// Edits recorded since the last `take_lsp_edits`, for an LSP client to
    /// express as incremental changes instead of resending the whole document.
    pending_lsp_edits: Vec<crate::history::EditOperation>,
    /// `buffer.revision` when the LSP client last synced this document. Any
    /// drift not explained by `pending_lsp_edits` forces a full resync.
    lsp_synced_revision: u64,
    /// Set when the buffer was swapped wholesale (reload); cleared on sync.
    lsp_full_sync_needed: bool,
    /// Deferred `d`-cuts not yet materialized into real deletes. A banked or
    /// visual-selection delete produces multiple entries from one cut action.
    pub pending_ghost: Vec<GhostCut>,
}

// Rust generics can't reflect into a struct's fields, so each field
// accessor is still hand-written; these macros only cut the try_get wrapper.

/// Read accessor: `self.state.try_get($key).map(|s| $body)`.
/// Use when `$body` returns the field's value directly (not already `Option`).
macro_rules! state_get {
    ($vis:vis fn $name:ident(&self) -> $ret:ty = $key:expr, |$s:ident| $body:expr) => {
        $vis fn $name(&self) -> Option<$ret> {
            self.state.try_get($key).map(|$s| $body)
        }
    };
}

/// Read accessor: `try_get($key).and_then(|s| $body)`, for closures that
/// themselves return `Option<$ret>` (e.g. via `.as_deref()`).
macro_rules! state_get_opt {
    ($vis:vis fn $name:ident(&self) -> $ret:ty = $key:expr, |$s:ident| $body:expr) => {
        $vis fn $name(&self) -> Option<$ret> {
            self.state.try_get($key).and_then(|$s| $body)
        }
    };
}

/// Write accessor: assigns one field if the `StateKey` matches, else no-op.
macro_rules! state_set {
    ($vis:vis fn $name:ident(&mut self, $arg:ident: $arg_ty:ty) = $key:expr, |$s:ident| $body:expr) => {
        $vis fn $name(&mut self, $arg: $arg_ty) {
            if let Some($s) = self.state.try_get_mut($key) {
                $body
            }
        }
    };
}

impl Document {
    /// Monotonic edit sequence number for this document.
    pub fn version(&self) -> u64 {
        self.document_version
    }

    pub fn set_syntax(&mut self, syntax: Syntax) {
        self.syntax = Some(syntax);
    }

    /// Convert an LSP `Position.character` on `line` (in `encoding`'s units)
    /// to a code-point offset. Use before indexing any LSP position.
    #[cfg(feature = "lsp")]
    pub fn lsp_char_offset_in_line(
        &self,
        line: usize,
        character: u32,
        encoding: crate::lsp::protocol::PositionEncoding,
    ) -> usize {
        let chars = self.line_chars(line).map(|c| c.to_char_lossy());
        encoding.char_offset_in_line(chars, character)
    }

    /// Convert a code-point offset on `line` to `encoding`'s wire units. Use
    /// before sending any cursor/selection position to an LSP server.
    #[cfg(feature = "lsp")]
    pub fn lsp_position_units_in_line(
        &self,
        line: usize,
        char_offset: usize,
        encoding: crate::lsp::protocol::PositionEncoding,
    ) -> u32 {
        let chars = self.line_chars(line).map(|c| c.to_char_lossy());
        encoding.units_for_char_offset(chars, char_offset)
    }

    /// Drain edits since the last call as one incremental LSP change: a
    /// single-line insert/delete/replace, or a chained run of inserts. Anything else returns `None`, meaning full sync.
    #[cfg(feature = "lsp")]
    pub fn take_incremental_lsp_changes(
        &mut self,
        encoding: crate::lsp::protocol::PositionEncoding,
    ) -> Option<(crate::lsp::protocol::LspRange, String)> {
        use crate::character::Character;
        use crate::history::EditOperation;
        use crate::lsp::protocol::{LspPosition, LspRange};

        let complete = self.pending_lsp_edits_are_complete();
        let mut ops = std::mem::take(&mut self.pending_lsp_edits);
        self.lsp_synced_revision = self.buffer.revision;
        self.lsp_full_sync_needed = false;
        if !complete {
            return None;
        }
        if ops.len() > 1 {
            let (position, text) = combine_insert_run(&ops)?;
            let units = self.lsp_position_units_in_line(
                position.line as usize,
                position.col as usize,
                encoding,
            );
            let pos = LspPosition {
                line: position.line,
                character: units,
            };
            let text: String = text.iter().map(Character::to_char_lossy).collect();
            return Some((
                LspRange {
                    start: pos.clone(),
                    end: pos,
                },
                text,
            ));
        }
        if ops.is_empty() {
            return None;
        }
        let op = ops.pop().unwrap();

        let single_line = |c: &Character| !matches!(c, Character::Newline);
        match op {
            EditOperation::Insert { position, text, .. } => {
                let units = self.lsp_position_units_in_line(
                    position.line as usize,
                    position.col as usize,
                    encoding,
                );
                let pos = LspPosition {
                    line: position.line,
                    character: units,
                };
                let text: String = text.iter().map(Character::to_char_lossy).collect();
                Some((
                    LspRange {
                        start: pos.clone(),
                        end: pos,
                    },
                    text,
                ))
            }
            EditOperation::Delete {
                range,
                deleted_text,
            } => {
                if range.start.line != range.end.line || !deleted_text.iter().all(single_line) {
                    return None;
                }
                let (start, end) = self.lsp_range_across_removed(
                    range,
                    deleted_text.len(),
                    &deleted_text,
                    encoding,
                );
                Some((LspRange { start, end }, String::new()))
            }
            EditOperation::Replace {
                range,
                old_text,
                new_text,
            } => {
                if range.start.line != range.end.line || !old_text.iter().all(single_line) {
                    return None;
                }
                let (start, end) =
                    self.lsp_range_across_removed(range, old_text.len(), &old_text, encoding);
                let text: String = new_text.iter().map(Character::to_char_lossy).collect();
                Some((LspRange { start, end }, text))
            }
            EditOperation::BlockChange { .. } => None,
        }
    }

    /// LSP start/end for a range since removed: `start` reads the still-valid
    /// current prefix; `end` chains that prefix with the removed text itself.
    #[cfg(feature = "lsp")]
    fn lsp_range_across_removed(
        &self,
        range: crate::history::Range,
        removed_len: usize,
        removed_text: &[crate::character::Character],
        encoding: crate::lsp::protocol::PositionEncoding,
    ) -> (
        crate::lsp::protocol::LspPosition,
        crate::lsp::protocol::LspPosition,
    ) {
        use crate::lsp::protocol::LspPosition;

        let start_units = self.lsp_position_units_in_line(
            range.start.line as usize,
            range.start.col as usize,
            encoding,
        );
        let prefix = self
            .line_chars(range.start.line as usize)
            .map(|c| c.to_char_lossy())
            .take(range.start.col as usize);
        let removed = removed_text.iter().map(|c| c.to_char_lossy());
        let end_char_offset = range.start.col as usize + removed_len;
        let end_units = encoding.units_for_char_offset(prefix.chain(removed), end_char_offset);

        (
            LspPosition {
                line: range.start.line,
                character: start_units,
            },
            LspPosition {
                line: range.end.line,
                character: end_units,
            },
        )
    }

    /// Discard edits recorded since the last drain, so they don't linger for
    /// next time; call this when skipping incremental sync for this edit.
    pub fn discard_pending_lsp_changes(&mut self) {
        self.pending_lsp_edits.clear();
        self.lsp_synced_revision = self.buffer.revision;
        self.lsp_full_sync_needed = false;
    }

    /// Flag the whole buffer as changed behind the LSP client's back (e.g. a
    /// reload swapped it in), so the next sync resends everything.
    pub fn mark_lsp_full_sync(&mut self) {
        self.lsp_full_sync_needed = true;
    }

    /// True when the buffer changed since the LSP client last synced it,
    /// whether or not the change went through `record_edit`.
    pub fn has_pending_lsp_edits(&self) -> bool {
        self.lsp_full_sync_needed
            || !self.pending_lsp_edits.is_empty()
            || self.buffer.revision != self.lsp_synced_revision
    }

    /// True only if every buffer mutation since the last sync was recorded, so
    /// the pending edits can be replayed incrementally (undo/redo bypass recording).
    #[cfg(feature = "lsp")]
    fn pending_lsp_edits_are_complete(&self) -> bool {
        let recorded = self.pending_lsp_edits.len() as u64;
        !self.lsp_full_sync_needed
            && self.buffer.revision.wrapping_sub(self.lsp_synced_revision) == recorded
    }

    #[cfg(feature = "lsp")]
    fn line_chars(&self, line: usize) -> impl Iterator<Item = crate::character::Character> + '_ {
        let start = self.buffer.line_start(line);
        let end = if line + 1 < self.buffer.line_count() {
            self.buffer.line_start(line + 1)
        } else {
            self.buffer.len()
        };
        self.buffer.chars(start..end)
    }

    /// Handle identifying this document incarnation.
    pub fn handle(&self) -> DocumentHandle {
        self.handle
    }

    /// Updates the document handle.
    pub fn set_handle(&mut self, handle: DocumentHandle) {
        self.handle = handle;
    }

    /// Reference to this document's kind descriptor.
    pub fn descriptor(&self) -> &KindDescriptor {
        self.kind.descriptor()
    }

    /// Interned buffer kind ID.
    pub fn buffer_kind_id(&self) -> BufferKindId {
        self.kind.id()
    }

    /// Policies bundle governing generic editor behavior for this buffer.
    pub fn policies(&self) -> &BufferPolicies {
        self.kind.policies()
    }

    /// Key fallback context for this buffer kind.
    pub fn key_fallback(&self) -> KeyFallback {
        self.policies().key_fallback
    }

    /// Text projection model for rendering.
    pub fn projection(&self) -> TextProjection {
        self.policies().projection
    }

    /// Whether this document matches the expected handle.
    pub fn matches_handle(&self, handle: DocumentHandle) -> bool {
        self.handle == handle
    }

    /// Whether this document matches the expected buffer kind ID.
    pub fn matches_kind(&self, id: BufferKindId) -> bool {
        self.buffer_kind_id() == id
    }

    /// Whether this document has an inert tombstone descriptor.
    pub fn is_tombstone(&self) -> bool {
        self.descriptor().is_tombstone()
    }

    /// Optional immutable help lines for this buffer kind.
    pub fn help_lines(&self) -> Option<&[String]> {
        self.descriptor().help_lines()
    }

    // Terminal helpers

    /// Reference to live terminal emulator instance, if any.
    pub fn terminal(&self) -> Option<&Terminal> {
        self.state
            .try_get(TERMINAL_STATE_KEY)
            .and_then(|s| s.terminal.as_ref())
    }

    /// Mutable reference to live terminal emulator instance, if any.
    pub fn terminal_mut(&mut self) -> Option<&mut Terminal> {
        self.state
            .try_get_mut(TERMINAL_STATE_KEY)
            .and_then(|s| s.terminal.as_mut())
    }

    /// Terminal cursor position (line, col).
    pub fn terminal_cursor(&self) -> Option<(usize, usize)> {
        self.state
            .try_get(TERMINAL_STATE_KEY)
            .and_then(|s| s.terminal_cursor)
    }

    /// Sets the terminal cursor position.
    pub fn set_terminal_cursor(&mut self, cursor: Option<(usize, usize)>) {
        if let Some(s) = self.state.try_get_mut(TERMINAL_STATE_KEY) {
            s.terminal_cursor = cursor;
        }
    }

    /// Reference to terminal cell color spans.
    pub fn terminal_cell_colors(&self) -> Option<&[crate::color::CellColorSpan]> {
        self.state
            .try_get(TERMINAL_STATE_KEY)
            .map(|state| state.terminal_cell_colors.as_slice())
    }

    // Predicate helpers

    pub fn is_file(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::FILE
    }

    pub fn is_terminal(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::TERMINAL
    }

    pub fn is_directory(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::DIRECTORY
    }

    pub fn is_undotree(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::UNDO_TREE
    }

    pub fn is_messages(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::MESSAGES
    }

    pub fn is_clipboard(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::CLIPBOARD
    }

    pub fn is_clipboard_entry(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::CLIPBOARD_ENTRY
    }

    pub fn is_any_clipboard(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::CLIPBOARD
            || self.buffer_kind_id() == BufferKindId::CLIPBOARD_ENTRY
    }

    pub fn is_location_list(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::LOCATION_LIST
    }

    pub fn is_regions(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::REGIONS
    }

    pub fn is_scratch(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::SCRATCH
    }

    pub fn is_git_status(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::GIT_STATUS
    }

    pub fn is_git_commit_message(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::GIT_COMMIT_MESSAGE
    }

    pub fn is_git_blame(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::GIT_BLAME
    }

    pub fn is_git_log(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::GIT_LOG
    }

    pub fn is_git_rebase_todo(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::GIT_REBASE_TODO
    }

    pub fn is_buffer_list(&self) -> bool {
        self.buffer_kind_id() == BufferKindId::BUFFER_LIST
    }

    pub fn is_special(&self) -> bool {
        self.buffer_kind_id() != BufferKindId::FILE
    }

    pub fn ghost_cut_allowed(&self) -> bool {
        self.policies().ghost_cut == GhostCutPolicy::Allow
    }

    pub fn is_interface_mode(&self) -> bool {
        self.policies().navigation == NavigationPolicy::ActionRows
    }

    pub fn is_read_only(&self) -> bool {
        match self.policies().read_only {
            ReadOnlyPolicy::FixedReadOnly => true,
            ReadOnlyPolicy::FixedWritable => false,
            ReadOnlyPolicy::DocumentOverride => self.readonly_override,
        }
    }

    pub fn set_read_only(&mut self, on: bool) {
        self.readonly_override = on;
    }

    // Feature state helpers

    state_get!(pub fn directory_path(&self) -> &PathBuf = DIRECTORY_STATE_KEY, |s| &s.path);
    state_get!(pub fn directory_entries(&self) -> &[DirEntry] = DIRECTORY_STATE_KEY, |s| s.entries.as_slice());
    state_get!(pub fn directory_show_hidden(&self) -> bool = DIRECTORY_STATE_KEY, |s| s.show_hidden);
    state_set!(pub fn set_directory_show_hidden(&mut self, show_hidden: bool) = DIRECTORY_STATE_KEY, |s| s.show_hidden = show_hidden);

    pub fn convert_to_file(&mut self) {
        self.kind = BufferKind::for_builtin(BufferKindId::FILE);
        self.state = StateSlot::new(FILE_STATE_KEY, FileState);
    }

    pub fn convert_to_directory(&mut self, path: PathBuf) {
        self.kind = BufferKind::for_builtin(BufferKindId::DIRECTORY);
        self.state = StateSlot::new(
            DIRECTORY_STATE_KEY,
            DirectoryState {
                path,
                entries: vec![],
                show_hidden: false,
            },
        );
    }

    state_get!(pub fn undotree_linked_doc_id(&self) -> DocumentId = UNDO_TREE_STATE_KEY, |s| s.linked_doc_id);
    state_get!(pub fn undotree_sequences(&self) -> &[crate::history::EditSeq] = UNDO_TREE_STATE_KEY, |s| s.sequences.as_slice());
    state_get!(pub fn messages_show_all(&self) -> bool = MESSAGES_STATE_KEY, |s| s.show_all);
    state_set!(pub fn set_messages_show_all(&mut self, show_all: bool) = MESSAGES_STATE_KEY, |s| s.show_all = show_all);
    state_get!(pub fn clipboard_entries(&self) -> &[Vec<crate::character::Character>] = CLIPBOARD_STATE_KEY, |s| s.entries.as_slice());

    pub fn clipboard_entry(&self, idx: usize) -> Option<&[crate::character::Character]> {
        self.state
            .try_get(CLIPBOARD_STATE_KEY)
            .and_then(|s| s.entries.get(idx).map(|v| v.as_slice()))
    }

    state_get!(pub fn clipboard_entry_index(&self) -> Option<usize> = CLIPBOARD_ENTRY_STATE_KEY, |s| s.entry_index);
    state_set!(pub fn set_clipboard_entry_index(&mut self, entry_index: Option<usize>) = CLIPBOARD_ENTRY_STATE_KEY, |s| s.entry_index = entry_index);

    pub fn convert_to_clipboard_entry(&mut self, entry_index: Option<usize>) {
        self.kind = BufferKind::for_builtin(BufferKindId::CLIPBOARD_ENTRY);
        self.state = StateSlot::new(
            CLIPBOARD_ENTRY_STATE_KEY,
            ClipboardEntryState { entry_index },
        );
    }

    state_get!(pub fn location_list_source_doc_id(&self) -> DocumentId = LOCATION_LIST_STATE_KEY, |s| s.source_doc_id);
    state_get!(pub fn location_list_entries(&self) -> &[LocationEntry] = LOCATION_LIST_STATE_KEY, |s| s.entries.as_slice());

    pub fn location_list_entry_at(&self, line: usize) -> Option<LocationEntry> {
        self.state
            .try_get(LOCATION_LIST_STATE_KEY)
            .and_then(|s| s.entries.get(line).cloned())
    }

    pub fn set_location_list(&mut self, source_doc_id: DocumentId, entries: Vec<LocationEntry>) {
        self.kind = BufferKind::for_builtin(BufferKindId::LOCATION_LIST);
        self.state = StateSlot::new(
            LOCATION_LIST_STATE_KEY,
            LocationListState {
                source_doc_id,
                entries,
            },
        );
    }

    state_get!(pub fn regions_source_doc_id(&self) -> DocumentId = REGIONS_STATE_KEY, |s| s.source_doc_id);

    pub fn set_regions(&mut self, source_doc_id: DocumentId) {
        self.kind = BufferKind::for_builtin(BufferKindId::REGIONS);
        self.state = StateSlot::new(REGIONS_STATE_KEY, RegionsState { source_doc_id });
    }

    state_get!(pub fn scratch_title(&self) -> &str = SCRATCH_STATE_KEY, |s| s.title.as_str());

    pub fn convert_to_scratch(&mut self, title: String) {
        self.kind = BufferKind::for_builtin(BufferKindId::SCRATCH);
        self.state = StateSlot::new(SCRATCH_STATE_KEY, ScratchState { title });
    }

    state_get!(pub fn buffer_list_entries(&self) -> &[DocumentId] = BUFFER_LIST_STATE_KEY, |s| s.entries.as_slice());
    state_get!(pub fn git_status_snapshot(&self) -> &crate::git::status::StatusSnapshot = GIT_STATUS_STATE_KEY, |s| &s.snapshot);
    state_get!(pub fn git_status_expanded_diffs(&self) -> &std::collections::HashMap<(PathBuf, bool), Vec<crate::git::diff::Hunk>> = GIT_STATUS_STATE_KEY, |s| &s.expanded_diffs);

    pub fn git_status_hunk(
        &self,
        path: &Path,
        staged_side: bool,
        hunk_index: usize,
    ) -> Option<crate::git::diff::Hunk> {
        self.state.try_get(GIT_STATUS_STATE_KEY).and_then(|s| {
            s.expanded_diffs
                .get(&(path.to_path_buf(), staged_side))
                .and_then(|h| h.get(hunk_index).cloned())
        })
    }

    state_get_opt!(pub fn git_status_head_subject(&self) -> &str = GIT_STATUS_STATE_KEY, |s| s.head_subject.as_deref());
    state_get!(pub fn git_commit_target(&self) -> &GitCommitTarget = GIT_COMMIT_MESSAGE_STATE_KEY, |s| &s.target);
    state_get!(pub fn git_blame_path(&self) -> &Path = GIT_BLAME_STATE_KEY, |s| s.path.as_path());
    state_get!(pub fn git_blame_linked_doc_id(&self) -> DocumentId = GIT_BLAME_STATE_KEY, |s| s.linked_doc_id);
    state_get!(pub fn git_blame_linked_window_id(&self) -> crate::split::window::WindowId = GIT_BLAME_STATE_KEY, |s| s.linked_window_id);
    state_set!(pub fn set_git_blame_linked_window_id(&mut self, window_id: crate::split::window::WindowId) = GIT_BLAME_STATE_KEY, |s| s.linked_window_id = window_id);
    state_get_opt!(pub fn git_blame_at_commit(&self) -> &str = GIT_BLAME_STATE_KEY, |s| s.at_commit.as_deref());
    state_set!(pub fn set_git_blame_at_commit(&mut self, at_commit: Option<String>) = GIT_BLAME_STATE_KEY, |s| s.at_commit = at_commit);
    state_get!(pub fn git_blame_history(&self) -> &[Option<String>] = GIT_BLAME_STATE_KEY, |s| s.history.as_slice());

    pub fn push_git_blame_history(&mut self, commit: Option<String>) {
        if let Some(s) = self.state.try_get_mut(GIT_BLAME_STATE_KEY) {
            s.history.push(commit);
        }
    }

    pub fn pop_git_blame_history(&mut self) -> Option<Option<String>> {
        self.state
            .try_get_mut(GIT_BLAME_STATE_KEY)
            .and_then(|s| s.history.pop())
    }

    state_get!(pub fn git_blame_lines(&self) -> &[crate::git::blame::BlameLine] = GIT_BLAME_STATE_KEY, |s| s.lines.as_slice());

    pub fn git_blame_lines_len(&self) -> usize {
        self.state
            .try_get(GIT_BLAME_STATE_KEY)
            .map(|s| s.lines.len())
            .unwrap_or(0)
    }

    state_get_opt!(pub fn git_blame_wrap_key(&self) -> (DocumentId, usize, usize, u64) = GIT_BLAME_STATE_KEY, |s| s.wrap_key);
    state_get!(pub fn git_blame_wrap_rows(&self) -> &[usize] = GIT_BLAME_STATE_KEY, |s| s.wrap_rows.as_slice());
    state_get!(pub fn git_log_path(&self) -> Option<&Path> = GIT_LOG_STATE_KEY, |s| s.path.as_deref());
    state_get!(pub fn git_log_commits(&self) -> &[crate::git::log::CommitSummary] = GIT_LOG_STATE_KEY, |s| s.commits.as_slice());
    state_get_opt!(pub fn git_log_expanded(&self) -> &str = GIT_LOG_STATE_KEY, |s| s.expanded.as_deref());
    state_get_opt!(pub fn git_log_expanded_body(&self) -> &str = GIT_LOG_STATE_KEY, |s| s.expanded_body.as_deref());
    state_get!(pub fn git_rebase_base(&self) -> &str = GIT_REBASE_TODO_STATE_KEY, |s| s.base.as_str());
    state_get!(pub fn git_rebase_saved_head(&self) -> &str = GIT_REBASE_TODO_STATE_KEY, |s| s.saved_head.as_str());
    state_get!(pub fn git_rebase_branch(&self) -> &str = GIT_REBASE_TODO_STATE_KEY, |s| s.branch.as_str());
    state_get_opt!(pub fn git_rebase_pause(&self) -> &crate::git::rebase::RebasePause = GIT_REBASE_TODO_STATE_KEY, |s| s.pause.as_ref());
    state_set!(pub fn set_git_rebase_pause(&mut self, pause: Option<crate::git::rebase::RebasePause>) = GIT_REBASE_TODO_STATE_KEY, |s| s.pause = pause);
    state_get!(pub fn git_rebase_steps(&self) -> &[crate::git::rebase::RebaseStep] = GIT_REBASE_TODO_STATE_KEY, |s| s.steps.as_slice());
    state_get!(pub fn git_rebase_message_overrides(&self) -> &std::collections::HashMap<String, String> = GIT_REBASE_TODO_STATE_KEY, |s| &s.message_overrides);

    pub fn git_repo_root(&self) -> Option<&Path> {
        if let Some(s) = self.state.try_get(GIT_STATUS_STATE_KEY) {
            return Some(&s.repo_root);
        }
        if let Some(s) = self.state.try_get(GIT_COMMIT_MESSAGE_STATE_KEY) {
            return Some(&s.repo_root);
        }
        if let Some(s) = self.state.try_get(GIT_BLAME_STATE_KEY) {
            return Some(&s.repo_root);
        }
        if let Some(s) = self.state.try_get(GIT_LOG_STATE_KEY) {
            return Some(&s.repo_root);
        }
        if let Some(s) = self.state.try_get(GIT_REBASE_TODO_STATE_KEY) {
            return Some(&s.repo_root);
        }
        None
    }
}

/// The document position immediately after inserting `text` at `start`,
/// tracking line/col across any newlines `text` itself contains.
#[cfg(feature = "lsp")]
fn advance_position(
    start: crate::history::Position,
    text: &[crate::character::Character],
) -> crate::history::Position {
    let mut line = start.line;
    let mut col = start.col;
    for ch in text {
        if matches!(ch, crate::character::Character::Newline) {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    crate::history::Position { line, col }
}

/// Combine a chained run of `Insert` ops (each landing where the previous
/// one ended) into one start position and concatenated text. `None` otherwise.
#[cfg(feature = "lsp")]
fn combine_insert_run(
    ops: &[crate::history::EditOperation],
) -> Option<(crate::history::Position, Vec<crate::character::Character>)> {
    use crate::history::EditOperation;

    let EditOperation::Insert {
        position: first_pos,
        text: first_text,
        ..
    } = ops.first()?
    else {
        return None;
    };

    let mut combined = first_text.clone();
    let mut expected_next = advance_position(*first_pos, first_text);
    for op in &ops[1..] {
        let EditOperation::Insert { position, text, .. } = op else {
            return None;
        };
        if *position != expected_next {
            return None;
        }
        combined.extend_from_slice(text);
        expected_next = advance_position(*position, text);
    }

    Some((*first_pos, combined))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
