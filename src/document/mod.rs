//! Document management
//! Encapsulates buffer + file metadata for multi-buffer support

pub mod definitions;
mod edit;
mod factories;
mod ghost;
mod history;
pub mod manager;
mod persistence;
mod populate;
mod search;
mod selection_render;

use crate::annotations::AnnotationStore;
#[cfg(feature = "lsp")]
use crate::buffer::api::BufferView;
use crate::buffer::TextBuffer;
use crate::history::{EditSeq, EditTransaction, UndoTree};
use crate::syntax::Syntax;
use crate::term::Terminal;
use definitions::DocumentOptions;
pub(crate) use factories::decode_file_bytes;
pub use manager::DocumentManager;
use std::path::PathBuf;

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
        /// Maps buffer line index -> EditSeq; u64::MAX = non-navigable connector line
        sequences: Vec<EditSeq>,
    },
    /// Messages log buffer showing all editor notifications
    Messages {
        /// When true, shows all job events including silent ones
        show_all: bool,
    },
    /// Clipboard ring index buffer, editable: :w syncs back to the ring
    Clipboard {
        /// Snapshot of ring entries at populate time; used for content-matching on save
        entries: Vec<Vec<crate::character::Character>>,
    },
    /// Scratch buffer for editing a single clipboard ring entry in place.
    ClipboardEntry { entry_index: Option<usize> },
    /// Read-only location list (diagnostics, references, quickfix).
    LocationList {
        source_doc_id: DocumentId,
        entries: Vec<LocationEntry>,
    },
    /// `gv` regions window: a read-only list of the active document's
    /// banked `SelectionSet`, one line per region.
    Regions { source_doc_id: DocumentId },
    /// Plugin-created in-memory buffer with no disk path (`rift.create_scratch_buf`).
    /// `title` is shown as the tab label in place of a filename.
    Scratch { title: String },
    /// Git status buffer: staged/unstaged/untracked/unmerged files. Read-only; changes happen only through its key actions (`s`/`u`/`X`/`=`/`c...`), never by editing the rendered text.
    GitStatus {
        repo_root: PathBuf,
        /// Snapshot of the status listing at the last populate/refresh.
        snapshot: crate::git::status::StatusSnapshot,
        /// Diff hunks fetched for currently-expanded entries, keyed by
        /// `(path, staged_side)` (`staged_side` = hunks came from `git diff --cached`).
        expanded_diffs: std::collections::HashMap<(PathBuf, bool), Vec<crate::git::diff::Hunk>>,
        /// HEAD's subject line, for the `HEAD <sha> <subject>` header summary (Enter on it opens the Log browser). `None` on an unborn branch with no commits yet.
        head_subject: Option<String>,
    },
    /// Commit message buffer; `:w`/`:wq` commits.
    GitCommitMessage {
        repo_root: PathBuf,
        target: GitCommitTarget,
    },
    /// `git blame` view for a file. Read-only navigation walks to a commit parent and opens in an adjacent split.
    GitBlame {
        repo_root: PathBuf,
        linked_doc_id: DocumentId,
        path: PathBuf,
        /// Ancestor commit currently being blamed at, if walked back from HEAD/worktree.
        at_commit: Option<String>,
        lines: Vec<crate::git::blame::BlameLine>,
    },
    /// `git log` browser for the repository (or scoped to one `path`). `=` expands a commit's `git show` inline, the same mechanism as status-buffer hunk expansion. Read-only, pure navigation.
    GitLog {
        repo_root: PathBuf,
        path: Option<PathBuf>,
        commits: Vec<crate::git::log::CommitSummary>,
        /// SHA of the commit currently expanded inline, if any.
        expanded: Option<String>,
        /// Cached `git show` body for `expanded`, so re-collapsing/expanding
        /// the same commit doesn't re-fetch.
        expanded_body: Option<String>,
    },
    /// Rebase todo: a `pick`/`squash`/`fixup`/`reword`/`edit` plan for `base..saved_head`, rendered from `steps`/`message_overrides`/ `expanded_bodies` (the buffer's text is a derived view, not the source of truth; `K`/`J`/verb keys/`dd`/`c`/`r` mutate `steps` or `message_overrides` directly and re-render). `:w`.
    GitRebaseTodo {
        repo_root: PathBuf,
        base: String,
        /// Original branch tip before the rebase started, for `abort`.
        saved_head: String,
        /// Original branch name, moved to the new tip on completion.
        branch: String,
        pause: Option<crate::git::rebase::RebasePause>,
        /// The plan, in execution order. Authoritative: `:w` runs this
        /// list directly, it does not re-parse the rendered text.
        steps: Vec<crate::git::rebase::RebaseStep>,
        /// Per-commit full-message override (sha -> `<subject>\n\n<body>`), set via the `c`/`r` message sub-editor. A step with no entry here executes using its real, current commit message untouched.
        message_overrides: std::collections::HashMap<String, String>,
        /// Which commits currently have their body previewed inline (sha
        /// set). Fresh entries start collapsed (not a member).
        expanded_bodies: std::collections::HashSet<String>,
        /// Cache of each commit's real body text (sha -> body, the part of the message after the subject line), fetched lazily the first time a commit is expanded with no override yet. Kept separate from `message_overrides` so merely *looking* at a commit never counts as editing it.
        original_bodies: std::collections::HashMap<String, String>,
    },
    /// Interactive buffer-list split panel: one line per open buffer.
    /// `entries[line]` is the DocumentId shown on that line.
    BufferList { entries: Vec<DocumentId> },
}

/// What saving a `BufferKind::GitCommitMessage` buffer does.
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
            BufferKind::Regions { .. } => "regions",
            BufferKind::BufferList { .. } => "buffer_list",
            BufferKind::Scratch { .. } => "scratch",
            BufferKind::GitStatus { .. } => "git_status",
            BufferKind::GitCommitMessage { .. } => "git_commit_message",
            BufferKind::GitBlame { .. } => "git_blame",
            BufferKind::GitLog { .. } => "git_log",
            BufferKind::GitRebaseTodo { .. } => "git_rebase_todo",
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
    /// Interface-mode buffer: read-only, vertical navigation snaps between
    /// actionable lines (magit/explorer/undotree as buffers).
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

    /// Check if this document is a `gv` regions list buffer.
    pub fn is_regions(&self) -> bool {
        matches!(self.kind, BufferKind::Regions { .. })
    }

    /// Whether deletes on this buffer may defer through a ghost-cut annotation
    pub fn ghost_cut_allowed(&self) -> bool {
        !matches!(
            self.kind,
            BufferKind::Terminal
                | BufferKind::Directory { .. }
                | BufferKind::Regions { .. }
                | BufferKind::Messages { .. }
                | BufferKind::Clipboard { .. }
                | BufferKind::UndoTree { .. }
                | BufferKind::GitStatus { .. }
                | BufferKind::GitBlame { .. }
                | BufferKind::GitLog { .. }
                | BufferKind::GitRebaseTodo { .. }
        )
    }

    /// Check if this document is a git status buffer.
    pub fn is_git_status(&self) -> bool {
        matches!(self.kind, BufferKind::GitStatus { .. })
    }

    /// Check if this document is a git rebase todo buffer.
    pub fn is_git_rebase_todo(&self) -> bool {
        matches!(self.kind, BufferKind::GitRebaseTodo { .. })
    }

    /// Check if this document is a git blame buffer.
    pub fn is_git_blame(&self) -> bool {
        matches!(self.kind, BufferKind::GitBlame { .. })
    }

    /// Check if this document is a git log buffer.
    pub fn is_git_log(&self) -> bool {
        matches!(self.kind, BufferKind::GitLog { .. })
    }

    /// Check if this document is the interactive buffer-list panel.
    pub fn is_buffer_list(&self) -> bool {
        matches!(self.kind, BufferKind::BufferList { .. })
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
    /// navigation between actionable regions).
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
