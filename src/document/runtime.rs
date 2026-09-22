//! Core runtime buffer-kind types, descriptors, policies, capability-keyed state,
//! and sparse registry.

use super::{DirEntry, DocumentId, GitCommitTarget, LocationEntry};
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

// Identifiers

/// Interned process-local buffer kind identity: a non-zero u32, never reused.
/// Hot paths use this copyable symbol directly, never a hashed/compared string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferKindId(pub NonZeroU32);

impl BufferKindId {
    // Reserved built-in kind IDs (1..=16)
    pub const FILE: Self = Self(NonZeroU32::new(1).unwrap());
    pub const TERMINAL: Self = Self(NonZeroU32::new(2).unwrap());
    pub const DIRECTORY: Self = Self(NonZeroU32::new(3).unwrap());
    pub const UNDO_TREE: Self = Self(NonZeroU32::new(4).unwrap());
    pub const MESSAGES: Self = Self(NonZeroU32::new(5).unwrap());
    pub const CLIPBOARD: Self = Self(NonZeroU32::new(6).unwrap());
    pub const CLIPBOARD_ENTRY: Self = Self(NonZeroU32::new(7).unwrap());
    pub const LOCATION_LIST: Self = Self(NonZeroU32::new(8).unwrap());
    pub const REGIONS: Self = Self(NonZeroU32::new(9).unwrap());
    pub const SCRATCH: Self = Self(NonZeroU32::new(10).unwrap());
    pub const GIT_STATUS: Self = Self(NonZeroU32::new(11).unwrap());
    pub const GIT_COMMIT_MESSAGE: Self = Self(NonZeroU32::new(12).unwrap());
    pub const GIT_BLAME: Self = Self(NonZeroU32::new(13).unwrap());
    pub const GIT_LOG: Self = Self(NonZeroU32::new(14).unwrap());
    pub const GIT_REBASE_TODO: Self = Self(NonZeroU32::new(15).unwrap());
    pub const BUFFER_LIST: Self = Self(NonZeroU32::new(16).unwrap());

    /// Upper bound (inclusive) of the reserved range for built-in buffer kinds.
    pub const RESERVED_BUILTIN_END: u32 = 64;

    /// First ID allocated for dynamic runtime / plugin buffer kinds.
    pub const FIRST_RUNTIME_ID: u32 = 65;

    /// Creates a new `BufferKindId` from a `NonZeroU32`.
    pub const fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    /// Attempts to create a `BufferKindId` from a raw `u32`.
    pub const fn from_u32(raw: u32) -> Option<Self> {
        match NonZeroU32::new(raw) {
            Some(nz) => Some(Self(nz)),
            None => None,
        }
    }

    /// Returns the raw numeric value.
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    /// Returns the underlying `NonZeroU32`.
    pub const fn non_zero(self) -> NonZeroU32 {
        self.0
    }

    /// Returns `true` if this ID belongs to the reserved built-in range.
    pub const fn is_builtin(self) -> bool {
        self.get() <= Self::RESERVED_BUILTIN_END
    }
}

impl std::fmt::Display for BufferKindId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.get())
    }
}

/// Identifies a document incarnation: base ID plus a creation-instance
/// generation, so async completions can't misdeliver to a recycled ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentHandle {
    pub doc_id: DocumentId,
    pub instance: u64,
}

impl DocumentHandle {
    pub const fn new(doc_id: DocumentId, instance: u64) -> Self {
        Self { doc_id, instance }
    }

    pub const fn doc_id(self) -> DocumentId {
        self.doc_id
    }

    pub const fn instance(self) -> u64 {
        self.instance
    }
}

impl std::fmt::Display for DocumentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.doc_id, self.instance)
    }
}

// Policies

/// Whether edits are admitted and whether per-document overrides apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadOnlyPolicy {
    FixedReadOnly,
    FixedWritable,
    DocumentOverride,
}

/// Dirty-close behavior for a buffer kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClosePolicy {
    ConfirmDirty,
    DiscardDirty,
}

/// Whether ordinary edits may change row count/structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructuralEditPolicy {
    FreeText,
    PreserveRows,
}

/// Whether deferred ghost cuts are admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GhostCutPolicy {
    Allow,
    Deny,
}

/// How insert-mode input is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputPolicy {
    EditorText,
    TerminalRaw,
}

/// Rendering and coordinate projection model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextProjectionPolicy {
    PlainText,
    StructuredRows,
    ScreenGrid,
}

/// Alias for `TextProjectionPolicy`.
pub type TextProjection = TextProjectionPolicy;

/// Whether vertical motion snaps to actionable rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavigationPolicy {
    Text,
    ActionRows,
}

/// File, LSP, and filesystem watcher eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageServicesPolicy {
    FileBacked,
    Virtual,
    ProcessBacked,
}

/// Parent key context fallback when a kind-local mapping misses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyFallback {
    Normal,
    Global,
    None,
}

/// Alias for `KeyFallback`.
pub type KeyFallbackPolicy = KeyFallback;

/// Immutable bundle of editor decision policies for a buffer kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferPolicies {
    pub read_only: ReadOnlyPolicy,
    pub close: ClosePolicy,
    pub structural_edit: StructuralEditPolicy,
    pub ghost_cut: GhostCutPolicy,
    pub input: InputPolicy,
    pub projection: TextProjectionPolicy,
    pub navigation: NavigationPolicy,
    pub language_services: LanguageServicesPolicy,
    pub key_fallback: KeyFallback,
}

impl BufferPolicies {
    /// Default policy bundle for normal writable file buffers.
    pub const fn file_default() -> Self {
        Self {
            read_only: ReadOnlyPolicy::DocumentOverride,
            close: ClosePolicy::ConfirmDirty,
            structural_edit: StructuralEditPolicy::FreeText,
            ghost_cut: GhostCutPolicy::Allow,
            input: InputPolicy::EditorText,
            projection: TextProjectionPolicy::PlainText,
            navigation: NavigationPolicy::Text,
            language_services: LanguageServicesPolicy::FileBacked,
            key_fallback: KeyFallback::Normal,
        }
    }

    /// Read-only, structured, action-row panel with system-regenerated
    /// content (git status/blame/log, undo tree); dirty-close never blocks.
    pub const fn read_only_panel() -> Self {
        Self {
            read_only: ReadOnlyPolicy::FixedReadOnly,
            close: ClosePolicy::DiscardDirty,
            structural_edit: StructuralEditPolicy::PreserveRows,
            ghost_cut: GhostCutPolicy::Deny,
            input: InputPolicy::EditorText,
            projection: TextProjectionPolicy::StructuredRows,
            navigation: NavigationPolicy::ActionRows,
            language_services: LanguageServicesPolicy::Virtual,
            key_fallback: KeyFallback::Normal,
        }
    }

    /// Read-only structured panel navigated with ordinary text motion rather
    /// than action-row snapping: location lists, region lists.
    pub const fn read_only_list() -> Self {
        Self {
            navigation: NavigationPolicy::Text,
            ..Self::read_only_panel()
        }
    }

    /// Writable free-text buffer with no backing file: commit messages,
    /// clipboard ring entries.
    pub const fn writable_scratch_text() -> Self {
        Self {
            read_only: ReadOnlyPolicy::FixedWritable,
            close: ClosePolicy::ConfirmDirty,
            structural_edit: StructuralEditPolicy::FreeText,
            ghost_cut: GhostCutPolicy::Allow,
            input: InputPolicy::EditorText,
            projection: TextProjectionPolicy::PlainText,
            navigation: NavigationPolicy::Text,
            language_services: LanguageServicesPolicy::Virtual,
            key_fallback: KeyFallback::Normal,
        }
    }
}

impl Default for BufferPolicies {
    fn default() -> Self {
        Self::file_default()
    }
}

// Descriptor Owner Metadata (independent of crate::plugin)

/// Authority that registered a buffer kind descriptor, decoupled from
/// `crate::plugin` so `crate::document` stays foundational.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DescriptorOwner {
    BuiltIn,
    Plugin {
        plugin_id: NonZeroU32,
        generation: NonZeroU32,
    },
}

impl DescriptorOwner {
    pub const fn is_builtin(&self) -> bool {
        matches!(self, Self::BuiltIn)
    }

    pub const fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin { .. })
    }

    pub const fn plugin_parts(&self) -> Option<(NonZeroU32, NonZeroU32)> {
        match *self {
            Self::BuiltIn => None,
            Self::Plugin {
                plugin_id,
                generation,
            } => Some((plugin_id, generation)),
        }
    }

    pub const fn plugin(plugin_id: NonZeroU32, generation: NonZeroU32) -> Self {
        Self::Plugin {
            plugin_id,
            generation,
        }
    }
}

/// Metadata retained when a descriptor is converted into an inert tombstone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TombstoneMetadata {
    pub reason: String,
}

// Dispatch & Handlers

/// Native action handler function pointer.
pub type NativeActionHandler = fn(DocumentHandle);

/// Buffer action routing dispatch representation.
#[derive(Debug, Clone, Copy)]
pub enum ActionDispatch {
    Native(NativeActionHandler),
    Lua,
    Disabled,
    Reject,
}

/// Structured outcome of a save operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveResult {
    Saved,
    Rejected,
    Failed,
}

/// Native save handler function pointer.
pub type NativeSaveHandler = fn(DocumentHandle) -> SaveResult;

/// Buffer save routing dispatch representation.
#[derive(Debug, Clone, Copy)]
pub enum SaveDispatch {
    Native(NativeSaveHandler),
    Lua,
    Disabled,
    Reject,
}

/// Mandatory close cleanup handler.
pub type NativeCloseHandler = fn(DocumentHandle);

/// Buffer close dispatch representation.
#[derive(Debug, Clone, Copy)]
pub enum CloseHandler {
    Native(NativeCloseHandler),
    Lua,
}

fn noop_close(_handle: DocumentHandle) {}
fn builtin_action(_handle: DocumentHandle) {}

fn builtin_save(_handle: DocumentHandle) -> SaveResult {
    SaveResult::Rejected
}

impl CloseHandler {
    /// Const-safe no-op close handler for built-ins with no external resources.
    pub const NOOP: Self = Self::Native(noop_close);
}

/// Strategy for generating tab/buffer display labels without arbitrary Lua during render.
#[derive(Debug, Clone)]
pub enum DisplayNameStrategy {
    KindName,
    FileName,
    Terminal,
    Label(Box<str>),
    Custom(fn(Option<&Path>, Option<&str>) -> Cow<'static, str>),
}

impl DisplayNameStrategy {
    pub fn resolve<'a>(
        &'a self,
        kind_name: &'a str,
        file_path: Option<&'a Path>,
        terminal_name: Option<&'a str>,
    ) -> Cow<'a, str> {
        match self {
            Self::KindName => Cow::Borrowed(kind_name),
            Self::FileName => match file_path {
                Some(p) => match p.file_name() {
                    Some(name) => name.to_string_lossy(),
                    None => Cow::Borrowed("[No Name]"),
                },
                None => Cow::Borrowed("[No Name]"),
            },
            Self::Terminal => match terminal_name {
                Some(name) => Cow::Borrowed(name),
                None => Cow::Borrowed("terminal"),
            },
            Self::Label(label) => Cow::Borrowed(label.as_ref()),
            Self::Custom(f) => f(file_path, terminal_name),
        }
    }
}

// Capability-Keyed Typed State Access

/// Erased identity of a typed feature state key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateKeyId {
    type_id: std::any::TypeId,
    name: &'static str,
}

impl StateKeyId {
    /// Creates a key ID from a static type and symbolic name.
    pub const fn of<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: std::any::TypeId::of::<T>(),
            name,
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
}

impl std::fmt::Display for StateKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Private capability token held by a feature module to access its typed state.
#[derive(Debug)]
pub struct StateKey<T: 'static> {
    id: StateKeyId,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: 'static> Clone for StateKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for StateKey<T> {}

impl<T: 'static> PartialEq for StateKey<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for StateKey<T> {}

impl<T: 'static> std::hash::Hash for StateKey<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: 'static> StateKey<T> {
    /// Creates a new typed state capability key.
    pub const fn new(name: &'static str) -> Self {
        Self {
            id: StateKeyId::of::<T>(name),
            _marker: std::marker::PhantomData,
        }
    }

    pub const fn id(&self) -> StateKeyId {
        self.id
    }

    pub const fn name(&self) -> &'static str {
        self.id.name()
    }
}

/// State for file buffers.
#[derive(Debug, Clone, Default)]
pub struct FileState;
pub static FILE_STATE_KEY: StateKey<FileState> = StateKey::new("file");

/// State for terminal emulator buffers.
#[derive(Debug, Default)]
pub struct TerminalState {
    pub terminal: Option<crate::term::Terminal>,
    pub terminal_cursor: Option<(usize, usize)>,
    pub terminal_cell_colors: crate::color::CellColorSpans,
}
pub static TERMINAL_STATE_KEY: StateKey<TerminalState> = StateKey::new("terminal");

/// State for directory browser buffers.
#[derive(Debug, Clone)]
pub struct DirectoryState {
    pub path: PathBuf,
    pub entries: Vec<DirEntry>,
    pub show_hidden: bool,
}
pub static DIRECTORY_STATE_KEY: StateKey<DirectoryState> = StateKey::new("directory");

/// State for undo-tree buffers.
#[derive(Debug, Clone)]
pub struct UndoTreeState {
    pub linked_doc_id: DocumentId,
    pub sequences: Vec<crate::history::EditSeq>,
}
pub static UNDO_TREE_STATE_KEY: StateKey<UndoTreeState> = StateKey::new("undotree");

/// State for notifications/messages buffers.
#[derive(Debug, Clone, Default)]
pub struct MessagesState {
    pub show_all: bool,
}
pub static MESSAGES_STATE_KEY: StateKey<MessagesState> = StateKey::new("messages");

/// State for clipboard index buffers.
#[derive(Debug, Clone, Default)]
pub struct ClipboardState {
    pub entries: Vec<Vec<crate::character::Character>>,
}
pub static CLIPBOARD_STATE_KEY: StateKey<ClipboardState> = StateKey::new("clipboard");

/// State for single clipboard entry buffers.
#[derive(Debug, Clone, Default)]
pub struct ClipboardEntryState {
    pub entry_index: Option<usize>,
}
pub static CLIPBOARD_ENTRY_STATE_KEY: StateKey<ClipboardEntryState> =
    StateKey::new("clipboard_entry");

/// State for location list (diagnostics/references) buffers.
#[derive(Debug, Clone)]
pub struct LocationListState {
    pub source_doc_id: DocumentId,
    pub entries: Vec<LocationEntry>,
}
pub static LOCATION_LIST_STATE_KEY: StateKey<LocationListState> = StateKey::new("location_list");

/// State for regions (`gv`) list buffers.
#[derive(Debug, Clone)]
pub struct RegionsState {
    pub source_doc_id: DocumentId,
}
pub static REGIONS_STATE_KEY: StateKey<RegionsState> = StateKey::new("regions");

/// State for scratch buffers.
#[derive(Debug, Clone)]
pub struct ScratchState {
    pub title: String,
}
pub static SCRATCH_STATE_KEY: StateKey<ScratchState> = StateKey::new("scratch");

/// State for git status buffers.
#[derive(Debug, Clone)]
pub struct GitStatusState {
    pub repo_root: PathBuf,
    pub snapshot: crate::git::status::StatusSnapshot,
    pub expanded_diffs: HashMap<(PathBuf, bool), Vec<crate::git::diff::Hunk>>,
    pub head_subject: Option<String>,
}
pub static GIT_STATUS_STATE_KEY: StateKey<GitStatusState> = StateKey::new("git_status");

/// State for git commit message buffers.
#[derive(Debug, Clone)]
pub struct GitCommitMessageState {
    pub repo_root: PathBuf,
    pub target: GitCommitTarget,
}
pub static GIT_COMMIT_MESSAGE_STATE_KEY: StateKey<GitCommitMessageState> =
    StateKey::new("git_commit_message");

/// State for git blame buffers.
#[derive(Debug, Clone)]
pub struct GitBlameState {
    pub repo_root: PathBuf,
    pub linked_doc_id: DocumentId,
    pub linked_window_id: crate::split::window::WindowId,
    pub path: PathBuf,
    pub at_commit: Option<String>,
    pub history: Vec<Option<String>>,
    pub lines: Vec<crate::git::blame::BlameLine>,
    pub wrap_rows: Vec<usize>,
    pub wrap_key: Option<(DocumentId, usize, usize, u64)>,
}
pub static GIT_BLAME_STATE_KEY: StateKey<GitBlameState> = StateKey::new("git_blame");

/// State for git log buffers.
#[derive(Debug, Clone)]
pub struct GitLogState {
    pub repo_root: PathBuf,
    pub path: Option<PathBuf>,
    pub commits: Vec<crate::git::log::CommitSummary>,
    pub expanded: Option<String>,
    pub expanded_body: Option<String>,
}
pub static GIT_LOG_STATE_KEY: StateKey<GitLogState> = StateKey::new("git_log");

/// State for git rebase todo buffers.
#[derive(Debug, Clone)]
pub struct GitRebaseTodoState {
    pub repo_root: PathBuf,
    pub base: String,
    pub saved_head: String,
    pub branch: String,
    pub pause: Option<crate::git::rebase::RebasePause>,
    pub steps: Vec<crate::git::rebase::RebaseStep>,
    pub message_overrides: HashMap<String, String>,
    pub expanded_bodies: HashSet<String>,
    pub original_bodies: HashMap<String, String>,
}
pub static GIT_REBASE_TODO_STATE_KEY: StateKey<GitRebaseTodoState> =
    StateKey::new("git_rebase_todo");

/// State for buffer list panel buffers.
#[derive(Debug, Clone, Default)]
pub struct BufferListState {
    pub entries: Vec<DocumentId>,
}
pub static BUFFER_LIST_STATE_KEY: StateKey<BufferListState> = StateKey::new("buffer_list");

/// Marker state for Lua plugin-defined buffers.
#[derive(Debug, Clone, Default)]
pub struct PluginBufferState;
pub static PLUGIN_BUFFER_STATE_KEY: StateKey<PluginBufferState> =
    StateKey::new("plugin_buffer_state");

/// Key used for buffers without custom local state.
pub static EMPTY_STATE_KEY: StateKey<()> = StateKey::new("empty");

/// Document-owned storage for one kind-local state object; access requires
/// the matching private `StateKey<T>`.
pub struct StateSlot {
    key_id: StateKeyId,
    state: Box<dyn std::any::Any>,
}

impl StateSlot {
    /// Creates a new state slot with the specified capability key and state payload.
    pub fn new<T: 'static>(key: StateKey<T>, state: T) -> Self {
        Self {
            key_id: key.id(),
            state: Box::new(state),
        }
    }

    /// Creates an empty unit state slot.
    pub fn empty() -> Self {
        Self::new(EMPTY_STATE_KEY, ())
    }

    /// Returns the erased identity of the stored state key.
    pub fn key_id(&self) -> StateKeyId {
        self.key_id
    }

    /// Checks whether this slot holds state for the given capability key.
    pub fn matches<T: 'static>(&self, key: StateKey<T>) -> bool {
        self.key_id == key.id()
    }

    /// Obtains a reference to the typed state; panics if `key` doesn't
    /// match this slot's stored key identity.
    pub fn get<T: 'static>(&self, key: StateKey<T>) -> &T {
        assert_eq!(
            self.key_id,
            key.id(),
            "StateSlot key mismatch: expected {:?}, got {:?}",
            key.id(),
            self.key_id
        );
        self.state
            .downcast_ref::<T>()
            .expect("StateSlot internal invariant failed: downcast failed despite key match")
    }

    /// Obtains a mutable reference to the typed state; panics if `key`
    /// doesn't match this slot's stored key identity.
    pub fn get_mut<T: 'static>(&mut self, key: StateKey<T>) -> &mut T {
        assert_eq!(
            self.key_id,
            key.id(),
            "StateSlot key mismatch: expected {:?}, got {:?}",
            key.id(),
            self.key_id
        );
        self.state
            .downcast_mut::<T>()
            .expect("StateSlot internal invariant failed: downcast failed despite key match")
    }

    /// Attempts to obtain a reference to the typed state without panicking on key mismatch.
    pub fn try_get<T: 'static>(&self, key: StateKey<T>) -> Option<&T> {
        if self.key_id == key.id() {
            Some(
                self.state.downcast_ref::<T>().expect(
                    "StateSlot internal invariant failed: downcast failed despite key match",
                ),
            )
        } else {
            None
        }
    }

    /// Attempts to obtain a mutable reference to the typed state without panicking on key mismatch.
    pub fn try_get_mut<T: 'static>(&mut self, key: StateKey<T>) -> Option<&mut T> {
        if self.key_id == key.id() {
            Some(
                self.state.downcast_mut::<T>().expect(
                    "StateSlot internal invariant failed: downcast failed despite key match",
                ),
            )
        } else {
            None
        }
    }
}

// KindDescriptor

/// Immutable descriptor defining the static behavior, policies, and dispatch
/// rules for a buffer kind.
#[derive(Debug, Clone)]
pub struct KindDescriptor {
    pub id: BufferKindId,
    pub name: Box<str>,
    pub policies: BufferPolicies,
    pub action_dispatch: ActionDispatch,
    pub save_dispatch: SaveDispatch,
    pub display_name: DisplayNameStrategy,
    pub help_lines: Option<Arc<[String]>>,
    pub on_close: CloseHandler,
    pub state_key: StateKeyId,
    pub owner: DescriptorOwner,
    pub tombstone: Option<TombstoneMetadata>,
}

impl KindDescriptor {
    pub fn id(&self) -> BufferKindId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn policies(&self) -> &BufferPolicies {
        &self.policies
    }

    pub fn state_key(&self) -> StateKeyId {
        self.state_key
    }

    pub fn owner(&self) -> DescriptorOwner {
        self.owner
    }

    pub fn is_tombstone(&self) -> bool {
        self.tombstone.is_some()
    }

    pub fn is_builtin(&self) -> bool {
        self.owner.is_builtin()
    }

    pub fn is_plugin(&self) -> bool {
        self.owner.is_plugin()
    }

    pub fn help_lines(&self) -> Option<&[String]> {
        self.help_lines.as_deref()
    }

    /// Converts this descriptor into an inert tombstone: keeps identity and
    /// display metadata, forces read-only, disables actions/saves.
    pub fn to_tombstone(&self, reason: String) -> Self {
        let mut policies = self.policies;
        policies.read_only = ReadOnlyPolicy::FixedReadOnly;
        policies.close = ClosePolicy::ConfirmDirty;
        policies.structural_edit = StructuralEditPolicy::PreserveRows;
        policies.ghost_cut = GhostCutPolicy::Deny;
        policies.input = InputPolicy::EditorText;
        policies.language_services = LanguageServicesPolicy::Virtual;
        Self {
            id: self.id,
            name: self.name.clone(),
            policies,
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Disabled,
            display_name: self.display_name.clone(),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: self.state_key,
            owner: self.owner,
            tombstone: Some(TombstoneMetadata { reason }),
        }
    }

    /// Resolves the display label using this descriptor's strategy.
    pub fn resolve_display_name<'a>(
        &'a self,
        file_path: Option<&'a Path>,
        terminal_name: Option<&'a str>,
    ) -> Cow<'a, str> {
        self.display_name
            .resolve(&self.name, file_path, terminal_name)
    }
}

// Registry

/// Registry error variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateName(String),
    InvalidId(BufferKindId),
    IdExhausted,
    ZeroId,
    NotBuiltinId(BufferKindId),
    AlreadyRegistered(BufferKindId),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateName(name) => write!(f, "buffer kind name '{name}' already registered"),
            Self::InvalidId(id) => write!(f, "invalid buffer kind ID {id}"),
            Self::IdExhausted => write!(f, "runtime buffer kind ID space exhausted"),
            Self::ZeroId => write!(f, "buffer kind ID cannot be zero"),
            Self::NotBuiltinId(id) => write!(f, "ID {id} is outside reserved built-in range"),
            Self::AlreadyRegistered(id) => write!(f, "buffer kind ID {id} already registered"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Cold-path registry mapping kind names to interned IDs/descriptors; sparse
/// maps avoid unbounded growth while IDs stay monotonic and never reused.
pub struct BufferKindRegistry {
    names: HashMap<Box<str>, BufferKindId>,
    descriptors: HashMap<BufferKindId, Arc<KindDescriptor>>,
    open_counts: HashMap<BufferKindId, usize>,
    next_runtime_id: u32,
}

impl BufferKindRegistry {
    /// Creates a new, empty registry.
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
            descriptors: HashMap::new(),
            open_counts: HashMap::new(),
            next_runtime_id: BufferKindId::FIRST_RUNTIME_ID,
        }
    }

    /// Creates a registry pre-populated with all 16 built-in descriptors.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register_builtins()
            .expect("built-in registration must not fail");
        reg
    }

    /// Registers a descriptor in the reserved built-in range.
    pub fn register_builtin(
        &mut self,
        descriptor: KindDescriptor,
    ) -> Result<Arc<KindDescriptor>, RegistryError> {
        if !descriptor.id.is_builtin() {
            return Err(RegistryError::NotBuiltinId(descriptor.id));
        }
        if self.descriptors.contains_key(&descriptor.id) {
            return Err(RegistryError::AlreadyRegistered(descriptor.id));
        }
        if self.names.contains_key(&descriptor.name) {
            return Err(RegistryError::DuplicateName(descriptor.name.to_string()));
        }

        let id = descriptor.id;
        let name = descriptor.name.clone();
        let arc = Arc::new(descriptor);
        self.descriptors.insert(id, Arc::clone(&arc));
        self.names.insert(name, id);
        Ok(arc)
    }

    /// Registers a dynamic runtime/plugin descriptor with a newly allocated monotonic ID.
    pub fn register_runtime(
        &mut self,
        name: &str,
        builder: impl FnOnce(BufferKindId) -> KindDescriptor,
    ) -> Result<Arc<KindDescriptor>, RegistryError> {
        if self.names.contains_key(name) {
            return Err(RegistryError::DuplicateName(name.to_string()));
        }

        let id_val = self.next_runtime_id;
        self.next_runtime_id = self
            .next_runtime_id
            .checked_add(1)
            .ok_or(RegistryError::IdExhausted)?;
        let nz = NonZeroU32::new(id_val).ok_or(RegistryError::ZeroId)?;
        let id = BufferKindId(nz);

        let descriptor = builder(id);
        if descriptor.id != id {
            return Err(RegistryError::InvalidId(descriptor.id));
        }

        let arc = Arc::new(descriptor);
        self.descriptors.insert(id, Arc::clone(&arc));
        // Active textual name now resolves to the new ID.
        self.names.insert(name.into(), id);
        Ok(arc)
    }

    /// Looks up an active or tombstone descriptor by interned ID.
    pub fn get_by_id(&self, id: BufferKindId) -> Option<Arc<KindDescriptor>> {
        self.descriptors.get(&id).cloned()
    }

    /// Looks up the current active descriptor by kind name.
    pub fn get_by_name(&self, name: &str) -> Option<Arc<KindDescriptor>> {
        self.names.get(name).and_then(|id| self.get_by_id(*id))
    }

    /// Resolves the current interned ID for an active kind name.
    pub fn id_by_name(&self, name: &str) -> Option<BufferKindId> {
        self.names.get(name).copied()
    }

    /// Increments open document count for this buffer kind.
    pub fn increment_open_count(&mut self, id: BufferKindId) {
        *self.open_counts.entry(id).or_insert(0) += 1;
    }

    /// Decrements open document count; releases a zero-count tombstone
    /// descriptor from memory (its ID still never reused).
    pub fn decrement_open_count(&mut self, id: BufferKindId) -> usize {
        match self.open_counts.entry(id) {
            Entry::Occupied(mut entry) => {
                let count = entry.get_mut();
                *count = count.saturating_sub(1);
                let remaining = *count;
                if remaining == 0 {
                    entry.remove();
                    if let Some(desc) = self.descriptors.get(&id) {
                        if desc.is_tombstone() {
                            self.descriptors.remove(&id);
                        }
                    }
                }
                remaining
            }
            Entry::Vacant(_) => 0,
        }
    }

    /// Returns the current open document count for an ID.
    pub fn open_count(&self, id: BufferKindId) -> usize {
        self.open_counts.get(&id).copied().unwrap_or(0)
    }

    /// Replaces an active descriptor with a tombstone, releasing it
    /// immediately if no documents currently hold this ID.
    pub fn tombstone_descriptor(
        &mut self,
        id: BufferKindId,
        reason: String,
    ) -> Option<Arc<KindDescriptor>> {
        let active = Arc::clone(self.descriptors.get(&id)?);
        let tombstone = Arc::new(active.to_tombstone(reason));

        if self.open_count(id) == 0 {
            self.descriptors.remove(&id);
            // Also remove active name if it still pointed here.
            if self.names.get(active.name()).copied() == Some(id) {
                self.names.remove(active.name());
            }
            None
        } else {
            self.descriptors.insert(id, Arc::clone(&tombstone));
            if self.names.get(active.name()).copied() == Some(id) {
                self.names.remove(active.name());
            }
            Some(tombstone)
        }
    }

    /// Retires all descriptors owned by a plugin generation.
    pub fn retire_plugin(
        &mut self,
        plugin_id: NonZeroU32,
        generation: NonZeroU32,
        reason: String,
    ) -> Vec<BufferKindId> {
        let target_owner = DescriptorOwner::Plugin {
            plugin_id,
            generation,
        };

        let ids_to_retire: Vec<BufferKindId> = self
            .descriptors
            .iter()
            .filter(|(_, desc)| desc.owner == target_owner && !desc.is_tombstone())
            .map(|(id, _)| *id)
            .collect();

        for &id in &ids_to_retire {
            self.tombstone_descriptor(id, reason.clone());
        }

        ids_to_retire
    }

    /// Populates all 16 built-in kind descriptors.
    pub fn register_builtins(&mut self) -> Result<(), RegistryError> {
        self.register_builtin(KindDescriptor {
            id: BufferKindId::FILE,
            name: "file".into(),
            policies: BufferPolicies::file_default(),
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Native(builtin_save),
            display_name: DisplayNameStrategy::FileName,
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: FILE_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::TERMINAL,
            name: "terminal".into(),
            policies: BufferPolicies {
                read_only: ReadOnlyPolicy::FixedWritable,
                close: ClosePolicy::DiscardDirty,
                structural_edit: StructuralEditPolicy::FreeText,
                ghost_cut: GhostCutPolicy::Deny,
                input: InputPolicy::TerminalRaw,
                projection: TextProjectionPolicy::ScreenGrid,
                navigation: NavigationPolicy::Text,
                language_services: LanguageServicesPolicy::ProcessBacked,
                key_fallback: KeyFallback::None,
            },
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Disabled,
            display_name: DisplayNameStrategy::Terminal,
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: TERMINAL_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::DIRECTORY,
            name: "directory".into(),
            policies: BufferPolicies {
                read_only: ReadOnlyPolicy::FixedWritable,
                close: ClosePolicy::ConfirmDirty,
                ..BufferPolicies::read_only_panel()
            },
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Native(builtin_save),
            display_name: DisplayNameStrategy::FileName,
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: DIRECTORY_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::UNDO_TREE,
            name: "undotree".into(),
            policies: BufferPolicies::read_only_panel(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[UndoTree]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: UNDO_TREE_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::MESSAGES,
            name: "messages".into(),
            policies: BufferPolicies {
                structural_edit: StructuralEditPolicy::PreserveRows,
                ghost_cut: GhostCutPolicy::Deny,
                ..BufferPolicies::writable_scratch_text()
            },
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Messages]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: MESSAGES_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::CLIPBOARD,
            name: "clipboard".into(),
            policies: BufferPolicies {
                read_only: ReadOnlyPolicy::FixedWritable,
                close: ClosePolicy::ConfirmDirty,
                ..BufferPolicies::read_only_list()
            },
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Native(builtin_save),
            display_name: DisplayNameStrategy::Label("[Clipboard]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: CLIPBOARD_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::CLIPBOARD_ENTRY,
            name: "clipboard_entry".into(),
            policies: BufferPolicies::writable_scratch_text(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Native(builtin_save),
            display_name: DisplayNameStrategy::Label("[Clipboard:entry]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: CLIPBOARD_ENTRY_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::LOCATION_LIST,
            name: "location_list".into(),
            policies: BufferPolicies::read_only_list(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Locations]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: LOCATION_LIST_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::REGIONS,
            name: "regions".into(),
            policies: BufferPolicies::read_only_list(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Regions]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: REGIONS_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::SCRATCH,
            name: "scratch".into(),
            policies: BufferPolicies {
                read_only: ReadOnlyPolicy::DocumentOverride,
                close: ClosePolicy::DiscardDirty,
                ..BufferPolicies::writable_scratch_text()
            },
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Disabled,
            display_name: DisplayNameStrategy::KindName,
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: SCRATCH_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let status_help: Vec<String> = vec![
            "Git Status".into(),
            "".into(),
            "g?      help".into(),
            "s       stage".into(),
            "u       unstage".into(),
            "-       toggle stage".into(),
            "X       discard".into(),
            "=       toggle diff".into(),
            "]c [c   next/prev hunk".into(),
            "<CR>    expand / open Log (on HEAD line)".into(),
            "b       blame file".into(),
            "r       rebase onto upstream".into(),
            "cc      commit".into(),
            "ca cw   amend".into(),
            "cf      fixup!".into(),
            "j k     move".into(),
        ];
        self.register_builtin(KindDescriptor {
            id: BufferKindId::GIT_STATUS,
            name: "git_status".into(),
            policies: BufferPolicies::read_only_panel(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Git Status]".into()),
            help_lines: Some(status_help.into()),
            on_close: CloseHandler::NOOP,
            state_key: GIT_STATUS_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::GIT_COMMIT_MESSAGE,
            name: "git_commit_message".into(),
            policies: BufferPolicies::writable_scratch_text(),
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Native(builtin_save),
            display_name: DisplayNameStrategy::Label("[Git Commit]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: GIT_COMMIT_MESSAGE_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let blame_help: Vec<String> = vec![
            "Git Blame".into(),
            "".into(),
            "g?      help".into(),
            "<CR>    blame parent".into(),
            "<BS>    blame next revision".into(),
            "<Esc>   close".into(),
        ];
        self.register_builtin(KindDescriptor {
            id: BufferKindId::GIT_BLAME,
            name: "git_blame".into(),
            policies: BufferPolicies::read_only_panel(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Git Blame]".into()),
            help_lines: Some(blame_help.into()),
            on_close: CloseHandler::NOOP,
            state_key: GIT_BLAME_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let log_help: Vec<String> = vec![
            "Git Log".into(),
            "".into(),
            "g?      help".into(),
            "<CR> =  toggle show".into(),
            "r       rebase from here".into(),
            "j k     move".into(),
        ];
        self.register_builtin(KindDescriptor {
            id: BufferKindId::GIT_LOG,
            name: "git_log".into(),
            policies: BufferPolicies::read_only_panel(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Git Log]".into()),
            help_lines: Some(log_help.into()),
            on_close: CloseHandler::NOOP,
            state_key: GIT_LOG_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let rebase_help: Vec<String> = vec![
            "Git Rebase Todo".into(),
            "".into(),
            "g?      help".into(),
            "K J     move commit up/down".into(),
            "p       pick".into(),
            "s       squash".into(),
            "f       fixup".into(),
            "e       edit".into(),
            "dd      drop".into(),
            "c r     reword (opens message editor)".into(),
            "<CR> =  toggle body preview".into(),
            "X       abort".into(),
            ":w      run".into(),
        ];
        self.register_builtin(KindDescriptor {
            id: BufferKindId::GIT_REBASE_TODO,
            name: "git_rebase_todo".into(),
            policies: BufferPolicies {
                close: ClosePolicy::ConfirmDirty,
                ..BufferPolicies::read_only_panel()
            },
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Native(builtin_save),
            display_name: DisplayNameStrategy::Label("[Git Rebase Todo]".into()),
            help_lines: Some(rebase_help.into()),
            on_close: CloseHandler::NOOP,
            state_key: GIT_REBASE_TODO_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        self.register_builtin(KindDescriptor {
            id: BufferKindId::BUFFER_LIST,
            name: "buffer_list".into(),
            policies: BufferPolicies::read_only_panel(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Buffers]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            state_key: BUFFER_LIST_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        Ok(())
    }
}

impl Default for BufferKindRegistry {
    fn default() -> Self {
        Self::new()
    }
}

static BUILTIN_REGISTRY: LazyLock<BufferKindRegistry> =
    LazyLock::new(BufferKindRegistry::with_builtins);

/// Resolves the descriptor for a reserved built-in buffer kind.
pub fn builtin_descriptor(id: BufferKindId) -> Arc<KindDescriptor> {
    BUILTIN_REGISTRY
        .get_by_id(id)
        .expect("built-in descriptor must be registered")
}
