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
    pub(crate) const FILE: Self = Self(NonZeroU32::new(1).unwrap());
    pub(crate) const TERMINAL: Self = Self(NonZeroU32::new(2).unwrap());
    pub(crate) const DIRECTORY: Self = Self(NonZeroU32::new(3).unwrap());
    pub(crate) const UNDO_TREE: Self = Self(NonZeroU32::new(4).unwrap());
    pub(crate) const MESSAGES: Self = Self(NonZeroU32::new(5).unwrap());
    pub(crate) const CLIPBOARD: Self = Self(NonZeroU32::new(6).unwrap());
    pub(crate) const CLIPBOARD_ENTRY: Self = Self(NonZeroU32::new(7).unwrap());
    pub(crate) const LOCATION_LIST: Self = Self(NonZeroU32::new(8).unwrap());
    pub(crate) const REGIONS: Self = Self(NonZeroU32::new(9).unwrap());
    pub(crate) const SCRATCH: Self = Self(NonZeroU32::new(10).unwrap());
    pub(crate) const GIT_STATUS: Self = Self(NonZeroU32::new(11).unwrap());
    pub(crate) const GIT_COMMIT_MESSAGE: Self = Self(NonZeroU32::new(12).unwrap());
    pub(crate) const GIT_BLAME: Self = Self(NonZeroU32::new(13).unwrap());
    pub(crate) const GIT_LOG: Self = Self(NonZeroU32::new(14).unwrap());
    pub(crate) const GIT_REBASE_TODO: Self = Self(NonZeroU32::new(15).unwrap());
    pub(crate) const BUFFER_LIST: Self = Self(NonZeroU32::new(16).unwrap());
    pub(crate) const UNDO_FILE_VIEW: Self = Self(NonZeroU32::new(17).unwrap());

    pub(crate) const RESERVED_BUILTIN_END: u32 = 64;

    pub(crate) const FIRST_RUNTIME_ID: u32 = 65;

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    #[must_use]
    pub(crate) const fn get(self) -> u32 {
        self.0.get()
    }

    #[must_use]
    pub(crate) const fn is_builtin(self) -> bool {
        self.get() <= Self::RESERVED_BUILTIN_END
    }
}

impl std::fmt::Display for BufferKindId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.get())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DocumentHandle {
    pub(crate) doc_id: DocumentId,
    pub(crate) instance: u64,
}

impl DocumentHandle {
    #[must_use]
    pub(crate) const fn new(doc_id: DocumentId, instance: u64) -> Self {
        Self { doc_id, instance }
    }

    #[must_use]
    pub(crate) const fn doc_id(self) -> DocumentId {
        self.doc_id
    }
}

impl std::fmt::Display for DocumentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.doc_id, self.instance)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ReadOnlyPolicy {
    FixedReadOnly,
    FixedWritable,
    DocumentOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ClosePolicy {
    ConfirmDirty,
    DiscardDirty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StructuralEditPolicy {
    FreeText,
    PreserveRows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GhostCutPolicy {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum InputPolicy {
    EditorText,
    TerminalRaw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TextProjectionPolicy {
    PlainText,
    StructuredRows,
    ScreenGrid,
}

pub(crate) type TextProjection = TextProjectionPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NavigationPolicy {
    Text,
    ActionRows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LanguageServicesPolicy {
    FileBacked,
    Virtual,
    ProcessBacked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum KeyFallback {
    Normal,
    Global,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BufferPolicies {
    pub(crate) read_only: ReadOnlyPolicy,
    pub(crate) close: ClosePolicy,
    pub(crate) structural_edit: StructuralEditPolicy,
    pub(crate) ghost_cut: GhostCutPolicy,
    pub(crate) input: InputPolicy,
    pub(crate) projection: TextProjectionPolicy,
    pub(crate) navigation: NavigationPolicy,
    pub(crate) language_services: LanguageServicesPolicy,
    pub(crate) key_fallback: KeyFallback,
}

impl BufferPolicies {
    #[must_use]
    pub(crate) const fn file_default() -> Self {
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

    #[must_use]
    pub(crate) const fn read_only_panel() -> Self {
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

    #[must_use]
    pub(crate) const fn read_only_list() -> Self {
        Self {
            navigation: NavigationPolicy::Text,
            ..Self::read_only_panel()
        }
    }

    #[must_use]
    pub(crate) const fn writable_scratch_text() -> Self {
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
pub(crate) enum DescriptorOwner {
    BuiltIn,
    Plugin {
        plugin_id: NonZeroU32,
        generation: NonZeroU32,
    },
}

impl DescriptorOwner {
    #[must_use]
    pub(crate) const fn plugin(plugin_id: NonZeroU32, generation: NonZeroU32) -> Self {
        Self::Plugin {
            plugin_id,
            generation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TombstoneMetadata {
    pub(crate) reason: String,
}

pub(crate) type NativeActionHandler = fn(DocumentHandle);

#[derive(Debug, Clone, Copy)]
pub(crate) enum ActionDispatch {
    Native(NativeActionHandler),
    Lua,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveResult {
    Saved,
    Rejected,
    Failed,
}

pub(crate) type NativeSaveHandler = fn(DocumentHandle) -> SaveResult;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SaveDispatch {
    Native(NativeSaveHandler),
    Lua,
    Disabled,
    Reject,
}

pub(crate) type NativeCloseHandler = fn(DocumentHandle);

#[derive(Debug, Clone, Copy)]
pub(crate) enum CloseHandler {
    Native(NativeCloseHandler),
    Lua,
}

const fn noop_close(_handle: DocumentHandle) {}
const fn builtin_action(_handle: DocumentHandle) {}

const fn builtin_save(_handle: DocumentHandle) -> SaveResult {
    SaveResult::Rejected
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ReloadDispatch {
    Native,
    Unsupported,
}

impl CloseHandler {
    pub(crate) const NOOP: Self = Self::Native(noop_close);
}

#[derive(Debug, Clone)]
pub(crate) enum DisplayNameStrategy {
    KindName,
    FileName,
    Terminal,
    Label(Box<str>),
}

impl DisplayNameStrategy {
    #[must_use]
    pub(crate) fn resolve<'a>(
        &'a self,
        kind_name: &'a str,
        file_path: Option<&'a Path>,
        terminal_name: Option<&'a str>,
    ) -> Cow<'a, str> {
        match self {
            Self::KindName => Cow::Borrowed(kind_name),
            Self::FileName => file_path
                .and_then(Path::file_name)
                .map_or(Cow::Borrowed("[No Name]"), |name| name.to_string_lossy()),
            Self::Terminal => terminal_name.map_or(Cow::Borrowed("terminal"), Cow::Borrowed),
            Self::Label(label) => Cow::Borrowed(label.as_ref()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StateKeyId {
    type_id: std::any::TypeId,
    name: &'static str,
}

impl StateKeyId {
    #[must_use]
    pub(crate) const fn of<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: std::any::TypeId::of::<T>(),
            name,
        }
    }
}

impl std::fmt::Display for StateKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug)]
pub(crate) struct StateKey<T: 'static> {
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
    #[must_use]
    pub(crate) const fn new(name: &'static str) -> Self {
        Self {
            id: StateKeyId::of::<T>(name),
            _marker: std::marker::PhantomData,
        }
    }

    #[must_use]
    pub(crate) const fn id(&self) -> StateKeyId {
        self.id
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FileState;
pub(crate) static FILE_STATE_KEY: StateKey<FileState> = StateKey::new("file");

#[derive(Debug, Default)]
pub(crate) struct TerminalState {
    pub(crate) terminal: Option<crate::term::Terminal>,
    pub(crate) terminal_cursor: Option<(usize, usize)>,
    pub(crate) terminal_cell_colors: crate::color::CellColorSpans,
}
pub(crate) static TERMINAL_STATE_KEY: StateKey<TerminalState> = StateKey::new("terminal");

#[derive(Debug, Clone)]
pub(crate) struct DirectoryState {
    pub(crate) path: PathBuf,
    pub(crate) entries: Vec<DirEntry>,
    pub(crate) show_hidden: bool,
}
pub(crate) static DIRECTORY_STATE_KEY: StateKey<DirectoryState> = StateKey::new("directory");

#[derive(Debug, Clone)]
pub(crate) struct UndoTreeState {
    pub(crate) linked_doc_id: DocumentId,
    pub(crate) sequences: Vec<crate::history::EditSeq>,
}
pub(crate) static UNDO_TREE_STATE_KEY: StateKey<UndoTreeState> = StateKey::new("undotree");

#[derive(Debug, Clone, Default)]
pub(crate) struct MessagesState {
    pub(crate) show_all: bool,
}
pub(crate) static MESSAGES_STATE_KEY: StateKey<MessagesState> = StateKey::new("messages");

#[derive(Debug, Clone, Default)]
pub(crate) struct ClipboardState {
    pub(crate) entries: Vec<Vec<crate::character::Character>>,
}
pub(crate) static CLIPBOARD_STATE_KEY: StateKey<ClipboardState> = StateKey::new("clipboard");

#[derive(Debug, Clone, Default)]
pub(crate) struct ClipboardEntryState {
    pub(crate) entry_index: Option<usize>,
}
pub(crate) static CLIPBOARD_ENTRY_STATE_KEY: StateKey<ClipboardEntryState> =
    StateKey::new("clipboard_entry");

#[derive(Debug, Clone)]
pub(crate) struct LocationListState {
    pub(crate) entries: Vec<LocationEntry>,
}
pub(crate) static LOCATION_LIST_STATE_KEY: StateKey<LocationListState> =
    StateKey::new("location_list");

#[derive(Debug, Clone)]
pub(crate) struct RegionsState {
    pub(crate) source_doc_id: DocumentId,
}
pub(crate) static REGIONS_STATE_KEY: StateKey<RegionsState> = StateKey::new("regions");

#[derive(Debug, Clone)]
pub(crate) struct ScratchState {
    pub(crate) title: String,
}
pub(crate) static SCRATCH_STATE_KEY: StateKey<ScratchState> = StateKey::new("scratch");

#[derive(Debug, Clone)]
pub(crate) struct GitStatusState {
    pub(crate) repo_root: PathBuf,
    pub(crate) snapshot: crate::git::status::StatusSnapshot,
    pub(crate) expanded_diffs: HashMap<(PathBuf, bool), Vec<crate::git::diff::Hunk>>,
    pub(crate) head_subject: Option<String>,
}
pub(crate) static GIT_STATUS_STATE_KEY: StateKey<GitStatusState> = StateKey::new("git_status");

#[derive(Debug, Clone)]
pub(crate) struct GitCommitMessageState {
    pub(crate) repo_root: PathBuf,
    pub(crate) target: GitCommitTarget,
}
pub(crate) static GIT_COMMIT_MESSAGE_STATE_KEY: StateKey<GitCommitMessageState> =
    StateKey::new("git_commit_message");

#[derive(Debug, Clone)]
pub(crate) struct GitBlameState {
    pub(crate) repo_root: PathBuf,
    pub(crate) linked_doc_id: DocumentId,
    pub(crate) linked_window_id: crate::split::window::WindowId,
    pub(crate) path: PathBuf,
    pub(crate) at_commit: Option<String>,
    pub(crate) history: Vec<Option<String>>,
    pub(crate) lines: Vec<crate::git::blame::BlameLine>,
    pub(crate) wrap_rows: Vec<usize>,
    pub(crate) wrap_key: Option<(DocumentId, usize, usize, u64)>,
}
pub(crate) static GIT_BLAME_STATE_KEY: StateKey<GitBlameState> = StateKey::new("git_blame");

#[derive(Debug, Clone)]
pub(crate) struct GitLogState {
    pub(crate) repo_root: PathBuf,
    pub(crate) path: Option<PathBuf>,
    pub(crate) commits: Vec<crate::git::log::CommitSummary>,
    pub(crate) expanded: Option<String>,
    pub(crate) expanded_body: Option<String>,
}
pub(crate) static GIT_LOG_STATE_KEY: StateKey<GitLogState> = StateKey::new("git_log");

#[derive(Debug, Clone)]
pub(crate) struct GitRebaseTodoState {
    pub(crate) repo_root: PathBuf,
    pub(crate) base: String,
    pub(crate) branch: String,
    pub(crate) pause: Option<crate::git::rebase::RebasePause>,
    pub(crate) steps: Vec<crate::git::rebase::RebaseStep>,
    pub(crate) message_overrides: HashMap<String, String>,
    pub(crate) expanded_bodies: HashSet<String>,
    pub(crate) original_bodies: HashMap<String, String>,
}
pub(crate) static GIT_REBASE_TODO_STATE_KEY: StateKey<GitRebaseTodoState> =
    StateKey::new("git_rebase_todo");

#[derive(Debug, Clone, Default)]
pub(crate) struct BufferListState {
    pub(crate) entries: Vec<DocumentId>,
}
pub(crate) static BUFFER_LIST_STATE_KEY: StateKey<BufferListState> = StateKey::new("buffer_list");

#[derive(Debug, Clone, Default)]
pub(crate) struct UndoFileViewState {
    pub(crate) sequences: Vec<crate::history::EditSeq>,
}
pub(crate) static UNDO_FILE_VIEW_STATE_KEY: StateKey<UndoFileViewState> =
    StateKey::new("undo_file_view");

#[derive(Debug, Clone, Default)]
pub(crate) struct PluginBufferState;
pub(crate) static PLUGIN_BUFFER_STATE_KEY: StateKey<PluginBufferState> =
    StateKey::new("plugin_buffer_state");

pub(crate) struct StateSlot {
    key_id: StateKeyId,
    state: Box<dyn std::any::Any>,
}

impl StateSlot {
    pub(crate) fn new<T: 'static>(key: StateKey<T>, state: T) -> Self {
        Self {
            key_id: key.id(),
            state: Box::new(state),
        }
    }

    #[must_use]
    pub(crate) const fn key_id(&self) -> StateKeyId {
        self.key_id
    }

    /// Returns the stored state if `key` matches and the stored type is `T`.
    #[must_use]
    pub(crate) fn try_get<T: 'static>(&self, key: StateKey<T>) -> Option<&T> {
        if self.key_id == key.id() {
            self.state.downcast_ref::<T>()
        } else {
            None
        }
    }

    /// Mutable counterpart of [`Self::try_get`].
    pub(crate) fn try_get_mut<T: 'static>(&mut self, key: StateKey<T>) -> Option<&mut T> {
        if self.key_id == key.id() {
            self.state.downcast_mut::<T>()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct KindDescriptor {
    pub(crate) id: BufferKindId,
    pub(crate) name: Box<str>,
    pub(crate) policies: BufferPolicies,
    pub(crate) action_dispatch: ActionDispatch,
    pub(crate) save_dispatch: SaveDispatch,
    pub(crate) display_name: DisplayNameStrategy,
    pub(crate) help_lines: Option<Arc<[String]>>,
    pub(crate) on_close: CloseHandler,
    pub(crate) reload_dispatch: ReloadDispatch,
    pub(crate) state_key: StateKeyId,
    pub(crate) owner: DescriptorOwner,
    pub(crate) tombstone: Option<TombstoneMetadata>,
}

impl KindDescriptor {
    #[must_use]
    pub(crate) const fn id(&self) -> BufferKindId {
        self.id
    }

    #[must_use]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub(crate) const fn policies(&self) -> &BufferPolicies {
        &self.policies
    }

    #[must_use]
    pub(crate) const fn state_key(&self) -> StateKeyId {
        self.state_key
    }

    #[must_use]
    pub(crate) const fn is_tombstone(&self) -> bool {
        self.tombstone.is_some()
    }

    #[must_use]
    pub(crate) fn help_lines(&self) -> Option<&[String]> {
        self.help_lines.as_deref()
    }

    #[must_use]
    pub(crate) fn to_tombstone(&self, reason: String) -> Self {
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

    #[must_use]
    pub(crate) fn resolve_display_name<'a>(
        &'a self,
        file_path: Option<&'a Path>,
        terminal_name: Option<&'a str>,
    ) -> Cow<'a, str> {
        self.display_name
            .resolve(&self.name, file_path, terminal_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegistryError {
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

pub(crate) struct BufferKindRegistry {
    names: HashMap<Box<str>, BufferKindId>,
    descriptors: HashMap<BufferKindId, Arc<KindDescriptor>>,
    open_counts: HashMap<BufferKindId, usize>,
    next_runtime_id: u32,
}

impl BufferKindRegistry {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            names: HashMap::new(),
            descriptors: HashMap::new(),
            open_counts: HashMap::new(),
            next_runtime_id: BufferKindId::FIRST_RUNTIME_ID,
        }
    }

    /// # Panics
    ///
    /// Panics if built-in registration fails; the built-in set is fixed, so this
    /// does not happen in practice.
    #[must_use]
    pub(crate) fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register_builtins()
            .expect("built-in registration must not fail");
        reg
    }

    /// # Errors
    ///
    /// Returns `NotBuiltinId` for a non-builtin id, `AlreadyRegistered` if the id is
    /// taken, or `DuplicateName` if the name is taken.
    pub(crate) fn register_builtin(
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

    /// # Errors
    ///
    /// Returns `DuplicateName` if the name is taken, `IdExhausted` if the runtime id
    /// counter overflows, `ZeroId` if the allocated id is zero, or `InvalidId` if the
    /// builder returns a descriptor with a different id.
    pub(crate) fn register_runtime(
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

    #[must_use]
    pub(crate) fn get_by_id(&self, id: BufferKindId) -> Option<Arc<KindDescriptor>> {
        self.descriptors.get(&id).cloned()
    }

    #[must_use]
    pub(crate) fn get_by_name(&self, name: &str) -> Option<Arc<KindDescriptor>> {
        self.names.get(name).and_then(|id| self.get_by_id(*id))
    }

    #[must_use]
    pub(crate) fn id_by_name(&self, name: &str) -> Option<BufferKindId> {
        self.names.get(name).copied()
    }

    pub(crate) fn increment_open_count(&mut self, id: BufferKindId) {
        *self.open_counts.entry(id).or_insert(0) += 1;
    }

    pub(crate) fn decrement_open_count(&mut self, id: BufferKindId) -> usize {
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

    #[must_use]
    pub(crate) fn open_count(&self, id: BufferKindId) -> usize {
        self.open_counts.get(&id).copied().unwrap_or(0)
    }

    pub(crate) fn tombstone_descriptor(
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

    pub(crate) fn retire_plugin(
        &mut self,
        plugin_id: NonZeroU32,
        generation: NonZeroU32,
        reason: &str,
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
            self.tombstone_descriptor(id, reason.to_string());
        }

        ids_to_retire
    }

    /// # Errors
    ///
    /// Returns the first registration error from the built-in groups; the built-in set
    /// is fixed, so this does not occur in practice.
    pub(crate) fn register_builtins(&mut self) -> Result<(), RegistryError> {
        self.register_core_builtins()?;
        self.register_view_builtins()?;
        self.register_git_status_builtins()?;
        self.register_git_history_builtins()?;
        self.register_panel_builtins()
    }

    fn register_core_builtins(&mut self) -> Result<(), RegistryError> {
        self.register_builtin(KindDescriptor {
            id: BufferKindId::FILE,
            name: "file".into(),
            policies: BufferPolicies::file_default(),
            action_dispatch: ActionDispatch::Disabled,
            save_dispatch: SaveDispatch::Native(builtin_save),
            display_name: DisplayNameStrategy::FileName,
            help_lines: None,
            on_close: CloseHandler::NOOP,
            reload_dispatch: ReloadDispatch::Native,
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
            reload_dispatch: ReloadDispatch::Native,
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
            reload_dispatch: ReloadDispatch::Native,
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
            reload_dispatch: ReloadDispatch::Native,
            state_key: MESSAGES_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;
        Ok(())
    }

    fn register_view_builtins(&mut self) -> Result<(), RegistryError> {
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
            reload_dispatch: ReloadDispatch::Native,
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
        Ok(())
    }

    fn register_git_status_builtins(&mut self) -> Result<(), RegistryError> {
        let status_help: Vec<String> = vec![
            "Git Status".into(),
            String::new(),
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
            reload_dispatch: ReloadDispatch::Native,
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
        Ok(())
    }

    fn register_git_history_builtins(&mut self) -> Result<(), RegistryError> {
        let blame_help: Vec<String> = vec![
            "Git Blame".into(),
            String::new(),
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
            reload_dispatch: ReloadDispatch::Native,
            state_key: GIT_BLAME_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let log_help: Vec<String> = vec![
            "Git Log".into(),
            String::new(),
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
            reload_dispatch: ReloadDispatch::Native,
            state_key: GIT_LOG_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let rebase_help: Vec<String> = vec![
            "Git Rebase Todo".into(),
            String::new(),
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
        Ok(())
    }

    fn register_panel_builtins(&mut self) -> Result<(), RegistryError> {
        self.register_builtin(KindDescriptor {
            id: BufferKindId::BUFFER_LIST,
            name: "buffer_list".into(),
            policies: BufferPolicies::read_only_panel(),
            action_dispatch: ActionDispatch::Native(builtin_action),
            save_dispatch: SaveDispatch::Reject,
            display_name: DisplayNameStrategy::Label("[Buffers]".into()),
            help_lines: None,
            on_close: CloseHandler::NOOP,
            reload_dispatch: ReloadDispatch::Native,
            state_key: BUFFER_LIST_STATE_KEY.id(),
            owner: DescriptorOwner::BuiltIn,
            tombstone: None,
        })?;

        let undo_file_view_help: Vec<String> = vec![
            "Persisted Undo File".into(),
            String::new(),
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
            reload_dispatch: ReloadDispatch::Native,
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

/// # Panics
///
/// Panics if `id` is not registered in the built-in registry.
pub(crate) fn builtin_descriptor(id: BufferKindId) -> Arc<KindDescriptor> {
    BUILTIN_REGISTRY
        .get_by_id(id)
        .expect("built-in descriptor must be registered")
}
