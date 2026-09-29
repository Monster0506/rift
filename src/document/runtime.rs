use super::{DirEntry, DocumentId, GitCommitTarget, LocationEntry};
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferKindId(pub NonZeroU32);

impl BufferKindId {
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
    pub const UNDO_FILE_VIEW: Self = Self(NonZeroU32::new(17).unwrap());

    pub const RESERVED_BUILTIN_END: u32 = 64;

    pub const FIRST_RUNTIME_ID: u32 = 65;

    pub const fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    pub const fn from_u32(raw: u32) -> Option<Self> {
        match NonZeroU32::new(raw) {
            Some(nz) => Some(Self(nz)),
            None => None,
        }
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }

    pub const fn non_zero(self) -> NonZeroU32 {
        self.0
    }

    pub const fn is_builtin(self) -> bool {
        self.get() <= Self::RESERVED_BUILTIN_END
    }
}

impl std::fmt::Display for BufferKindId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.get())
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadOnlyPolicy {
    FixedReadOnly,
    FixedWritable,
    DocumentOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClosePolicy {
    ConfirmDirty,
    DiscardDirty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructuralEditPolicy {
    FreeText,
    PreserveRows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GhostCutPolicy {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputPolicy {
    EditorText,
    TerminalRaw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextProjectionPolicy {
    PlainText,
    StructuredRows,
    ScreenGrid,
}

pub type TextProjection = TextProjectionPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavigationPolicy {
    Text,
    ActionRows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageServicesPolicy {
    FileBacked,
    Virtual,
    ProcessBacked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyFallback {
    Normal,
    Global,
    None,
}

pub type KeyFallbackPolicy = KeyFallback;

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

    pub const fn read_only_list() -> Self {
        Self {
            navigation: NavigationPolicy::Text,
            ..Self::read_only_panel()
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TombstoneMetadata {
    pub reason: String,
}

pub type NativeActionHandler = fn(DocumentHandle);

#[derive(Debug, Clone, Copy)]
pub enum ActionDispatch {
    Native(NativeActionHandler),
    Lua,
    Disabled,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveResult {
    Saved,
    Rejected,
    Failed,
}

pub type NativeSaveHandler = fn(DocumentHandle) -> SaveResult;

#[derive(Debug, Clone, Copy)]
pub enum SaveDispatch {
    Native(NativeSaveHandler),
    Lua,
    Disabled,
    Reject,
}

pub type NativeCloseHandler = fn(DocumentHandle);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadOutcome {
    Reloaded,
    Unsupported,
}

pub type NativeReloadHandler = fn(DocumentHandle) -> ReloadOutcome;

#[derive(Debug, Clone, Copy)]
pub enum ReloadDispatch {
    Native(NativeReloadHandler),
    Unsupported,
}

fn builtin_reload(_handle: DocumentHandle) -> ReloadOutcome {
    ReloadOutcome::Unsupported
}

impl CloseHandler {
    pub const NOOP: Self = Self::Native(noop_close);
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateKeyId {
    type_id: std::any::TypeId,
    name: &'static str,
}

impl StateKeyId {
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

#[derive(Debug, Clone, Default)]
pub struct FileState;
pub static FILE_STATE_KEY: StateKey<FileState> = StateKey::new("file");

#[derive(Debug, Default)]
pub struct TerminalState {
    pub terminal: Option<crate::term::Terminal>,
    pub terminal_cursor: Option<(usize, usize)>,
    pub terminal_cell_colors: crate::color::CellColorSpans,
}
pub static TERMINAL_STATE_KEY: StateKey<TerminalState> = StateKey::new("terminal");

#[derive(Debug, Clone)]
pub struct DirectoryState {
    pub path: PathBuf,
    pub entries: Vec<DirEntry>,
    pub show_hidden: bool,
}
pub static DIRECTORY_STATE_KEY: StateKey<DirectoryState> = StateKey::new("directory");

#[derive(Debug, Clone)]
pub struct UndoTreeState {
    pub linked_doc_id: DocumentId,
    pub sequences: Vec<crate::history::EditSeq>,
}
pub static UNDO_TREE_STATE_KEY: StateKey<UndoTreeState> = StateKey::new("undotree");

#[derive(Debug, Clone, Default)]
pub struct MessagesState {
    pub show_all: bool,
}
pub static MESSAGES_STATE_KEY: StateKey<MessagesState> = StateKey::new("messages");

#[derive(Debug, Clone, Default)]
pub struct ClipboardState {
    pub entries: Vec<Vec<crate::character::Character>>,
}
pub static CLIPBOARD_STATE_KEY: StateKey<ClipboardState> = StateKey::new("clipboard");

#[derive(Debug, Clone, Default)]
pub struct ClipboardEntryState {
    pub entry_index: Option<usize>,
}
pub static CLIPBOARD_ENTRY_STATE_KEY: StateKey<ClipboardEntryState> =
    StateKey::new("clipboard_entry");

#[derive(Debug, Clone)]
pub struct LocationListState {
    pub source_doc_id: DocumentId,
    pub entries: Vec<LocationEntry>,
}
pub static LOCATION_LIST_STATE_KEY: StateKey<LocationListState> = StateKey::new("location_list");

#[derive(Debug, Clone)]
pub struct RegionsState {
    pub source_doc_id: DocumentId,
}
pub static REGIONS_STATE_KEY: StateKey<RegionsState> = StateKey::new("regions");

#[derive(Debug, Clone)]
pub struct ScratchState {
    pub title: String,
}
pub static SCRATCH_STATE_KEY: StateKey<ScratchState> = StateKey::new("scratch");

#[derive(Debug, Clone)]
pub struct GitStatusState {
    pub repo_root: PathBuf,
    pub snapshot: crate::git::status::StatusSnapshot,
    pub expanded_diffs: HashMap<(PathBuf, bool), Vec<crate::git::diff::Hunk>>,
    pub head_subject: Option<String>,
}
pub static GIT_STATUS_STATE_KEY: StateKey<GitStatusState> = StateKey::new("git_status");

#[derive(Debug, Clone)]
pub struct GitCommitMessageState {
    pub repo_root: PathBuf,
    pub target: GitCommitTarget,
}
pub static GIT_COMMIT_MESSAGE_STATE_KEY: StateKey<GitCommitMessageState> =
    StateKey::new("git_commit_message");

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

#[derive(Debug, Clone)]
pub struct GitLogState {
    pub repo_root: PathBuf,
    pub path: Option<PathBuf>,
    pub commits: Vec<crate::git::log::CommitSummary>,
    pub expanded: Option<String>,
    pub expanded_body: Option<String>,
}
pub static GIT_LOG_STATE_KEY: StateKey<GitLogState> = StateKey::new("git_log");

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

#[derive(Debug, Clone, Default)]
pub struct BufferListState {
    pub entries: Vec<DocumentId>,
}
pub static BUFFER_LIST_STATE_KEY: StateKey<BufferListState> = StateKey::new("buffer_list");

#[derive(Debug, Clone, Default)]
pub struct UndoFileViewState {
    pub sequences: Vec<crate::history::EditSeq>,
}
pub static UNDO_FILE_VIEW_STATE_KEY: StateKey<UndoFileViewState> = StateKey::new("undo_file_view");

#[derive(Debug, Clone, Default)]
pub struct PluginBufferState;
pub static PLUGIN_BUFFER_STATE_KEY: StateKey<PluginBufferState> =
    StateKey::new("plugin_buffer_state");

pub static EMPTY_STATE_KEY: StateKey<()> = StateKey::new("empty");

pub struct StateSlot {
    key_id: StateKeyId,
    state: Box<dyn std::any::Any>,
}

impl StateSlot {
    pub fn new<T: 'static>(key: StateKey<T>, state: T) -> Self {
        Self {
            key_id: key.id(),
            state: Box::new(state),
        }
    }

    pub fn empty() -> Self {
        Self::new(EMPTY_STATE_KEY, ())
    }

    pub fn key_id(&self) -> StateKeyId {
        self.key_id
    }

    pub fn matches<T: 'static>(&self, key: StateKey<T>) -> bool {
        self.key_id == key.id()
    }

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
    pub reload_dispatch: ReloadDispatch,
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
            reload_dispatch: ReloadDispatch::Unsupported,
            state_key: self.state_key,
            owner: self.owner,
            tombstone: Some(TombstoneMetadata { reason }),
        }
    }

    pub fn resolve_display_name<'a>(
        &'a self,
        file_path: Option<&'a Path>,
        terminal_name: Option<&'a str>,
    ) -> Cow<'a, str> {
        self.display_name
            .resolve(&self.name, file_path, terminal_name)
    }
}

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

pub struct BufferKindRegistry {
    names: HashMap<Box<str>, BufferKindId>,
    descriptors: HashMap<BufferKindId, Arc<KindDescriptor>>,
    open_counts: HashMap<BufferKindId, usize>,
    next_runtime_id: u32,
}

impl BufferKindRegistry {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
            descriptors: HashMap::new(),
            open_counts: HashMap::new(),
            next_runtime_id: BufferKindId::FIRST_RUNTIME_ID,
        }
    }

    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register_builtins()
            .expect("built-in registration must not fail");
        reg
    }

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
        self.names.insert(name.into(), id);
        Ok(arc)
    }

    pub fn get_by_id(&self, id: BufferKindId) -> Option<Arc<KindDescriptor>> {
        self.descriptors.get(&id).cloned()
    }

    pub fn get_by_name(&self, name: &str) -> Option<Arc<KindDescriptor>> {
        self.names.get(name).and_then(|id| self.get_by_id(*id))
    }

    pub fn id_by_name(&self, name: &str) -> Option<BufferKindId> {
        self.names.get(name).copied()
    }

    pub fn increment_open_count(&mut self, id: BufferKindId) {
        *self.open_counts.entry(id).or_insert(0) += 1;
    }

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

    pub fn open_count(&self, id: BufferKindId) -> usize {
        self.open_counts.get(&id).copied().unwrap_or(0)
    }

    pub fn tombstone_descriptor(
        &mut self,
        id: BufferKindId,
        reason: String,
    ) -> Option<Arc<KindDescriptor>> {
        let active = Arc::clone(self.descriptors.get(&id)?);
        let tombstone = Arc::new(active.to_tombstone(reason));

        if self.open_count(id) == 0 {
            self.descriptors.remove(&id);
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Unsupported,
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Unsupported,
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
            reload_dispatch: ReloadDispatch::Unsupported,
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
            reload_dispatch: ReloadDispatch::Unsupported,
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
            reload_dispatch: ReloadDispatch::Unsupported,
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Unsupported,
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
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
            reload_dispatch: ReloadDispatch::Unsupported,
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
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
            state_key: BUFFER_LIST_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let undo_file_view_help: Vec<String> = vec![
            "Persisted Undo File".into(),
            "".into(),
            "Read-only view of a `.undo` file's history.".into(),
        ];
        self.register_builtin(KindDescriptor {
            id: BufferKindId::UNDO_FILE_VIEW,
            name: "undo_file_view".into(),
            policies: BufferPolicies::read_only_list(),
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::FileName,
            help_lines: Some(undo_file_view_help.into()),
            on_close: CloseHandler::NOOP,
            reload_dispatch: ReloadDispatch::Native(builtin_reload),
            state_key: UNDO_FILE_VIEW_STATE_KEY.id(),
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

pub fn builtin_descriptor(id: BufferKindId) -> Arc<KindDescriptor> {
    BUILTIN_REGISTRY
        .get_by_id(id)
        .expect("built-in descriptor must be registered")
}
