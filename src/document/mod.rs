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
mod syntax_sync;

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
    NativeCloseHandler, NativeReloadHandler, NativeSaveHandler, NavigationPolicy,
    PluginBufferState, ReadOnlyPolicy, RegionsState, RegistryError, ReloadDispatch, ReloadOutcome,
    SaveDispatch, SaveResult, ScratchState, StateKey, StateKeyId, StateSlot, StructuralEditPolicy,
    TerminalState, TextProjection, TextProjectionPolicy, TombstoneMetadata, UndoFileViewState,
    UndoTreeState, BUFFER_LIST_STATE_KEY, CLIPBOARD_ENTRY_STATE_KEY, CLIPBOARD_STATE_KEY,
    DIRECTORY_STATE_KEY, EMPTY_STATE_KEY, FILE_STATE_KEY, GIT_BLAME_STATE_KEY,
    GIT_COMMIT_MESSAGE_STATE_KEY, GIT_LOG_STATE_KEY, GIT_REBASE_TODO_STATE_KEY,
    GIT_STATUS_STATE_KEY, LOCATION_LIST_STATE_KEY, MESSAGES_STATE_KEY, PLUGIN_BUFFER_STATE_KEY,
    REGIONS_STATE_KEY, SCRATCH_STATE_KEY, TERMINAL_STATE_KEY, UNDO_FILE_VIEW_STATE_KEY,
    UNDO_TREE_STATE_KEY,
};
use std::path::{Path, PathBuf};
pub use syntax_sync::SyntaxSync;

pub type DocumentId = u64;

enum AnnotationUndo {
    Insertion {
        start: usize,
        new_end: usize,
        line_inserts: Vec<usize>,
    },
    Snapshot(Vec<crate::annotations::Annotation>),
}

pub(crate) enum AnnotationUndoHint {
    Insertion {
        start: usize,
        new_end: usize,
        line_inserts: Vec<usize>,
    },
    Snapshot,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub id: u16,
}

#[derive(Debug, Default)]
pub struct DirectoryDiff {
    pub renames: Vec<(PathBuf, String)>,
    pub deletes: Vec<PathBuf>,
    pub creates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitStatusAction {
    Stage(PathBuf),
    Unstage(PathBuf),
    Discard {
        path: PathBuf,
        orig_path: Option<PathBuf>,
        was_untracked: bool,
    },
    StageHunk {
        path: PathBuf,
        hunk: crate::git::diff::Hunk,
        is_new_file: bool,
    },
    UnstageHunk {
        path: PathBuf,
        hunk: crate::git::diff::Hunk,
    },
    DiscardHunk {
        path: PathBuf,
        hunk: crate::git::diff::Hunk,
        staged_side: bool,
    },
}

pub struct GhostCut {
    pub(super) at: usize,
    pub(super) text: Vec<crate::character::Character>,
    pub(super) painted: Option<(usize, crate::annotations::AnnotationId)>,
}

#[derive(Debug, Clone)]
pub struct LocationEntry {
    pub uri: String,
    pub line: u32,
    pub col: u32,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCommitTarget {
    New,
    Amend,
    RebaseReword {
        rebase_doc_id: DocumentId,
    },
    RebasePlanReword {
        rebase_doc_id: DocumentId,
        sha: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    LF,
    CRLF,
}

impl LineEnding {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            LineEnding::LF => b"\n",
            LineEnding::CRLF => b"\r\n",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ViewState {
    pub top_line: usize,
    pub left_col: usize,
}

pub struct Document {
    pub id: DocumentId,
    pub buffer: TextBuffer,
    pub options: DocumentOptions,
    file_path: Option<PathBuf>,
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
    pub annotations: AnnotationStore,
    pub selection_set: crate::selection::SelectionSet,
    pending_annotation_snapshot: Option<Vec<crate::annotations::Annotation>>,
    annotation_undo_stack: Vec<AnnotationUndo>,
    annotation_redo_stack: Vec<AnnotationUndo>,
    document_version: u64,
    pending_lsp_edits: Vec<crate::history::EditOperation>,
    lsp_synced_revision: u64,
    lsp_full_sync_needed: bool,
    pub pending_ghost: Vec<GhostCut>,
    pub(super) ghost_paint_active: bool,
    pub(super) ghost_paint_cursor: Option<usize>,
}

macro_rules! state_get {
    ($vis:vis fn $name:ident(&self) -> $ret:ty = $key:expr, |$s:ident| $body:expr) => {
        $vis fn $name(&self) -> Option<$ret> {
            self.state.try_get($key).map(|$s| $body)
        }
    };
}

macro_rules! state_get_opt {
    ($vis:vis fn $name:ident(&self) -> $ret:ty = $key:expr, |$s:ident| $body:expr) => {
        $vis fn $name(&self) -> Option<$ret> {
            self.state.try_get($key).and_then(|$s| $body)
        }
    };
}

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
    pub fn version(&self) -> u64 {
        self.document_version
    }

    pub fn set_syntax(&mut self, syntax: Syntax) {
        self.syntax = Some(syntax);
    }

    pub(crate) fn check_invariants(
        &self,
        registry: &BufferKindRegistry,
        tier: crate::invariants::InvariantTier,
    ) -> Vec<String> {
        use crate::invariants::InvariantTier;

        let mut out = Vec::new();
        let id = self.id;

        if self.kind.descriptor().state_key() != self.state.key_id() {
            out.push(format!(
                "doc {id}: kind descriptor state_key {:?} != state.key_id() {:?}",
                self.kind.descriptor().state_key(),
                self.state.key_id()
            ));
        }

        if (self.transaction_depth == 0) != self.current_transaction.is_none() {
            out.push(format!(
                "doc {id}: transaction_depth={} but current_transaction.is_some()={}",
                self.transaction_depth,
                self.current_transaction.is_some()
            ));
        }

        for detail in self.history.check_invariants(tier) {
            out.push(format!("doc {id}: {detail}"));
        }
        if self.history.current == self.history.root_seq && !self.annotation_undo_stack.is_empty() {
            out.push(format!(
                "doc {id}: history.current == root_seq but annotation_undo_stack has {} entries",
                self.annotation_undo_stack.len()
            ));
        }

        out.extend(self.buffer.check_invariants(tier));
        out.extend(self.selection_set.check_invariants(&self.buffer, tier));

        if tier >= InvariantTier::Standard && !self.kind.id.is_builtin() {
            if let Some(current) = registry.get_by_id(self.kind.id) {
                if !std::sync::Arc::ptr_eq(&current, &self.kind.descriptor) {
                    out.push(format!(
                        "doc {id}: kind descriptor is stale (registry has since replaced the descriptor for id {:?})",
                        self.kind.id
                    ));
                }
            }
        }

        out
    }

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

    pub fn discard_pending_lsp_changes(&mut self) {
        self.pending_lsp_edits.clear();
        self.lsp_synced_revision = self.buffer.revision;
        self.lsp_full_sync_needed = false;
    }

    pub fn mark_lsp_full_sync(&mut self) {
        self.lsp_full_sync_needed = true;
    }

    pub fn has_pending_lsp_edits(&self) -> bool {
        self.lsp_full_sync_needed
            || !self.pending_lsp_edits.is_empty()
            || self.buffer.revision != self.lsp_synced_revision
    }

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

    pub fn handle(&self) -> DocumentHandle {
        self.handle
    }

    pub fn set_handle(&mut self, handle: DocumentHandle) {
        self.handle = handle;
    }

    pub fn descriptor(&self) -> &KindDescriptor {
        self.kind.descriptor()
    }

    pub fn buffer_kind_id(&self) -> BufferKindId {
        self.kind.id()
    }

    pub fn policies(&self) -> &BufferPolicies {
        self.kind.policies()
    }

    pub fn key_fallback(&self) -> KeyFallback {
        self.policies().key_fallback
    }

    pub fn projection(&self) -> TextProjection {
        self.policies().projection
    }

    pub fn matches_handle(&self, handle: DocumentHandle) -> bool {
        self.handle == handle
    }

    pub fn matches_kind(&self, id: BufferKindId) -> bool {
        self.buffer_kind_id() == id
    }

    pub fn is_tombstone(&self) -> bool {
        self.descriptor().is_tombstone()
    }

    pub fn help_lines(&self) -> Option<&[String]> {
        self.descriptor().help_lines()
    }

    pub fn terminal(&self) -> Option<&Terminal> {
        self.state
            .try_get(TERMINAL_STATE_KEY)
            .and_then(|s| s.terminal.as_ref())
    }

    pub fn terminal_mut(&mut self) -> Option<&mut Terminal> {
        self.state
            .try_get_mut(TERMINAL_STATE_KEY)
            .and_then(|s| s.terminal.as_mut())
    }

    pub fn terminal_cursor(&self) -> Option<(usize, usize)> {
        self.state
            .try_get(TERMINAL_STATE_KEY)
            .and_then(|s| s.terminal_cursor)
    }

    pub fn set_terminal_cursor(&mut self, cursor: Option<(usize, usize)>) {
        if let Some(s) = self.state.try_get_mut(TERMINAL_STATE_KEY) {
            s.terminal_cursor = cursor;
        }
    }

    pub fn terminal_cell_colors(&self) -> Option<&[crate::color::CellColorSpan]> {
        self.state
            .try_get(TERMINAL_STATE_KEY)
            .map(|state| state.terminal_cell_colors.as_slice())
    }

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

    pub fn convert_to_undo_file_view(&mut self) {
        self.kind = BufferKind::for_builtin(BufferKindId::UNDO_FILE_VIEW);
        self.state = StateSlot::new(UNDO_FILE_VIEW_STATE_KEY, UndoFileViewState::default());
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
