//! Plugin system for Rift: an event bus ([`PluginHost::dispatch`]) with Rust closure handlers today, a Lua VM layer to come.
//!
//! Invariant: the host never mutates editor state directly; mutations are queued as [`PluginMutation`] and applied via `execute_command`, preserving undo/dot-repeat.

pub(crate) mod events;
mod lua_state;

#[cfg(feature = "plugins")]
pub(crate) mod lua_host;
#[cfg(not(feature = "plugins"))]
#[path = "lua_host_stub.rs"]
pub(crate) mod lua_host;

#[cfg(feature = "plugins")]
mod lua_value;

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::plugin::events::EditorEvent;

use crate::document::DocumentId;
use crate::notification::NotificationType;

static NEXT_PLUGIN_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_GENERATION_ID: AtomicU32 = AtomicU32::new(1);

fn allocate_next_plugin_id() -> PluginId {
    let raw = NEXT_PLUGIN_ID.fetch_add(1, Ordering::Relaxed);
    let nz = NonZeroU32::new(raw).expect("PluginId counter overflowed nonzero range");
    PluginId(nz)
}

fn allocate_next_generation_id() -> NonZeroU32 {
    let raw = NEXT_GENERATION_ID.fetch_add(1, Ordering::Relaxed);
    NonZeroU32::new(raw).expect("PluginGeneration counter overflowed nonzero range")
}

/// Process-local plugin identity: nonzero u32, monotonic, never reused
/// across reloads of the same plugin (each reload gets a fresh generation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PluginId(NonZeroU32);

impl PluginId {
    #[cfg(not(feature = "plugins"))]
    #[must_use]
    const fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    #[cfg(test)]
    /// Allocate a fresh, process-unique `PluginId`.
    #[must_use]
    fn allocate() -> Self {
        allocate_next_plugin_id()
    }

    /// Return the raw `u32` value.
    #[must_use]
    const fn get(self) -> u32 {
        self.0.get()
    }

    /// Return the underlying `NonZeroU32`.
    #[must_use]
    pub(crate) const fn as_nonzero(self) -> NonZeroU32 {
        self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PluginId({})", self.0)
    }
}

/// One load incarnation of a plugin: identity plus a monotonic generation
/// counter, so a reload's resources never leak into the old generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PluginGeneration {
    plugin_id: PluginId,
    generation: NonZeroU32,
}

impl PluginGeneration {
    /// Create a new `PluginGeneration` from a plugin id and a load generation counter.
    #[must_use]
    const fn new(plugin_id: PluginId, generation: NonZeroU32) -> Self {
        Self {
            plugin_id,
            generation,
        }
    }

    /// Allocate a fresh, globally monotonic generation for the given plugin.
    #[must_use]
    fn allocate(plugin_id: PluginId) -> Self {
        Self::new(plugin_id, allocate_next_generation_id())
    }

    /// The owning plugin identity.
    #[must_use]
    pub(crate) const fn plugin_id(self) -> PluginId {
        self.plugin_id
    }

    /// The monotonic load generation number for this plugin incarnation.
    #[must_use]
    pub(crate) const fn generation(self) -> NonZeroU32 {
        self.generation
    }

    /// Return the raw pair `(plugin_id, generation)` as u32 primitives.
    #[must_use]
    const fn raw(self) -> (u32, u32) {
        (self.plugin_id.get(), self.generation.get())
    }

    /// Converts this `PluginGeneration` into an owner token for keymap layers.
    #[must_use]
    pub(crate) fn keymap_owner_token(self) -> crate::keymap::KeyBindingToken {
        let (pid, gen) = self.raw();
        crate::keymap::KeyBindingToken::new((u64::from(pid) << 32) | u64::from(gen))
    }
}

impl std::fmt::Display for PluginGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PluginGeneration(plugin={}, gen={})",
            self.plugin_id.0, self.generation
        )
    }
}

impl From<(PluginId, NonZeroU32)> for PluginGeneration {
    fn from((plugin_id, generation): (PluginId, NonZeroU32)) -> Self {
        Self::new(plugin_id, generation)
    }
}

/// Lifecycle status of a [`PluginGeneration`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GenerationStatus {
    /// Active generation. Permitted to queue mutations and handle events.
    Active,
    /// Generation is in the process of retiring. New mutations are rejected;
    /// pending work is being drained or discarded.
    Retiring,
    /// Generation has been retired and uninstalled. All references are inert.
    Retired,
}

impl GenerationStatus {}

/// A `PluginMutation` with optional origin generation; native mutations
/// carry `None` and are always active, plugin ones are origin-validated.
#[derive(Debug)]
pub(crate) struct PluginMutationEnvelope {
    pub(crate) mutation: PluginMutation,
    pub(crate) origin: Option<PluginGeneration>,
}

impl PluginMutationEnvelope {
    /// Create a native mutation envelope without plugin origin tracking.
    #[must_use]
    const fn native(mutation: PluginMutation) -> Self {
        Self {
            mutation,
            origin: None,
        }
    }

    /// Create a plugin-originated mutation envelope tagged with the owning generation.
    #[must_use]
    const fn plugin(mutation: PluginMutation, origin: PluginGeneration) -> Self {
        Self {
            mutation,
            origin: Some(origin),
        }
    }

    /// Create a mutation envelope with optional origin generation.
    #[must_use]
    const fn new(mutation: PluginMutation, origin: Option<PluginGeneration>) -> Self {
        Self { mutation, origin }
    }
}

impl From<PluginMutation> for PluginMutationEnvelope {
    fn from(mutation: PluginMutation) -> Self {
        Self::native(mutation)
    }
}

/// An event handler.
type Handler = Box<dyn Fn(&EditorEvent) + Send + 'static>;

/// A command handler. Receives split args, returns mutations to apply.
type CommandHandler = Box<dyn Fn(&[String]) -> Vec<PluginMutation> + Send + 'static>;

/// A keymap action handler. Returns mutations to apply.
type ActionHandler = Box<dyn Fn() -> Vec<PluginMutation> + Send + 'static>;

/// A state change requested by a plugin. Queued during event dispatch and
/// applied by the main loop after dispatch returns.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum PluginMutation {
    /// Display a notification through Rift's notification system.
    Notify {
        message: String,
        level: NotificationType,
    },
    AppendLines {
        buf: DocumentId,
        lines: Vec<String>,
    },
    InsertAtCursor {
        buf: DocumentId,
        text: String,
    },
    DeleteBefore {
        buf: DocumentId,
        count: usize,
    },
    DeleteForward {
        buf: DocumentId,
        count: usize,
    },
    /// Move the cursor to a specific position. `row` is 1-indexed; `col` is 0-indexed.
    SetCursor {
        buf: DocumentId,
        row: usize,
        col: usize,
    },
    /// Replace a line range with new content. `start` and `end` are 1-indexed and inclusive.
    /// The replaced region is deleted and `lines` are inserted in its place.
    ReplaceLines {
        buf: DocumentId,
        start: usize,
        end: usize,
        lines: Vec<String>,
    },
    SetHighlightGroup {
        name: String,
        style: Option<crate::layer::CellStyle>,
    },
    ResetHighlightGroups,
    AddNsHighlight {
        buf: DocumentId,
        ns: String,
        owner: Option<String>,
        range: HighlightRange,
        hl: HighlightSpec,
        priority: i32,
    },
    ClearNsHighlights {
        buf: Option<DocumentId>,
        ns: String,
        owner: Option<String>,
    },
    /// Set a per-document option. Values are strings such as `"4"` or `"true"`.
    SetOption {
        buf: Option<DocumentId>,
        name: String,
        value: String,
    },
    SaveBuffer {
        buf: DocumentId,
    },
    /// Open a plugin-owned floating window.
    OpenFloat(PluginFloat),
    /// Close the currently open plugin float.
    CloseFloat,
    /// Execute an editor action by its string name (e.g. `"editor:save"`, `"mode:normal"`).
    ExecAction(String),
    /// Register a key binding. `mode` is "n", "i", "c", "s", or "g".
    /// `keys` is key notation (e.g. `"<C-p>"`, `"<C-S-Left>"`, `"<F5>"`, `"gg"`; see
    /// `crate::key::notation::parse_key_sequence`). `action` is an action string.
    MapKey {
        mode: String,
        keys: String,
        action: String,
    },
    /// Set the viewport scroll position (`top_line`, `left_col`).
    SetScroll(usize, usize),
    SetLineEnding {
        buf: DocumentId,
        ending: String,
    },
    /// Remove a key binding for the selected mode and key sequence.
    UnmapKey {
        mode: String,
        keys: String,
    },
    /// Move the cursor to `row` (1-indexed) and center the viewport on it.
    CenterOnLine {
        buf: DocumentId,
        row: usize,
    },
    /// Set the `CursorHold` idle delay in milliseconds.
    SetCursorHoldDelay(u32),
    /// Switch the active buffer to the given document ID.
    SwitchToBuffer(DocumentId),
    /// Open (or switch to) a file by path. `force` discards unsaved changes.
    OpenFile {
        path: String,
        force: bool,
    },
    CloseBuffer {
        buf: DocumentId,
        force: bool,
    },
    /// Move the focused window in the given direction.
    MoveWindow {
        direction: crate::split::navigation::Direction,
    },
    /// Swap the contents of the focused window with the previously focused window.
    SwapWindows,
    /// Focus the previously focused window.
    FocusPreviousWindow,
    /// Register a file extension -> language name mapping.
    RegisterFiletype {
        ext: String,
        lang_name: String,
    },
    #[cfg(feature = "treesitter")]
    SetSyntaxQuery {
        lang: String,
        kind: crate::syntax::loader::QueryKind,
        source: Option<String>,
    },
    #[cfg(feature = "treesitter")]
    StartSyntax {
        buf: DocumentId,
        lang: Option<String>,
    },
    #[cfg(feature = "treesitter")]
    StopSyntax {
        buf: DocumentId,
    },
    /// Load a tree-sitter grammar from a compiled shared library at runtime.
    /// `so_path` is the `.so`/`.dll`/`.dylib` path; `fn_name` is the exported C symbol (e.g. `"tree_sitter_toml"`).
    RegisterGrammar {
        lang_name: String,
        so_path: String,
        fn_name: String,
    },
    /// Register a language server for a filetype (plugin-provided config).
    #[cfg(feature = "lsp")]
    LspRegisterServer {
        language: String,
        config: crate::lsp::config::LspServerConfig,
    },
    /// Request goto definition at the current cursor position.
    LspGotoDefinition,
    /// Request all references at the current cursor position.
    LspReferences,
    /// Request hover documentation at the current cursor position.
    LspHover,
    /// Request a rename of the symbol at the current cursor position (direct, no dialog).
    LspRename {
        new_name: String,
    },
    /// Open the rename dialog at the current cursor position.
    LspRenameDialog,
    /// Request document formatting.
    LspFormat,
    /// Request code actions at the current cursor position.
    LspCodeAction,
    /// Jump to the next LSP diagnostic in the active document.
    LspDiagnosticNext,
    /// Jump to the previous LSP diagnostic in the active document.
    LspDiagnosticPrev,
    /// Open the diagnostics panel.
    LspDiagnosticsPanel,
    AddAnnotation {
        buf: DocumentId,
        id: u64,
        kind: String,
        anchor: AnnotationAnchorSpec,
        payload: crate::annotations::value::Value,
        presentation: Option<crate::annotations::presentation::Presentation>,
        actions: Vec<(String, bool)>,
        visible: bool,
        stickiness: Option<String>,
        owner: Option<String>,
    },
    /// Mutate an existing annotation in place; `None` fields are left unchanged.
    UpdateAnnotation {
        buf: DocumentId,
        id: u64,
        payload: Option<crate::annotations::value::Value>,
        visible: Option<bool>,
        presentation: Option<crate::annotations::presentation::Presentation>,
    },
    RemoveAnnotation {
        buf: DocumentId,
        id: u64,
    },
    ClearAnnotations {
        buf: DocumentId,
        kind_prefix: String,
    },
    /// Register a handler for an annotation (kind, verb). With `command` set, the
    /// verb runs that ex command; otherwise it routes back to the Lua host.
    RegisterAnnotationAction {
        kind: String,
        verb: String,
        command: Option<String>,
        owner: Option<String>,
    },
    /// Register per-kind render/hover defaults into the kind registry (sec 4).
    RegisterKindDefaults {
        kind: String,
        presentation: Option<crate::annotations::presentation::Presentation>,
        description: Option<String>,
        owner: Option<String>,
    },
    UnloadPlugin {
        owner: String,
    },
    /// Create and activate an in-memory buffer. Fires `BufOpen` after creation.
    CreateScratchBuf {
        id: DocumentId,
        name: String,
        lines: Vec<String>,
    },
    ReloadBuffer {
        buf: DocumentId,
        force: bool,
    },
    /// Register a custom buffer kind from a plugin.
    RegisterBufferKind {
        name: String,
        read_only: Option<String>,
        close: Option<String>,
        key_fallback: Option<String>,
        display_name: Option<String>,
        help_lines: Option<Vec<String>>,
        has_on_close: bool,
        has_on_action: bool,
        has_on_save: bool,
    },
    /// Create a new buffer with the given registered kind, optional title, lines, and initial buffer vars.
    CreateBuffer {
        id: DocumentId,
        kind: String,
        title: Option<String>,
        lines: Vec<String>,
        vars: Vec<(String, crate::annotations::value::Value)>,
    },
    /// Map a key binding scoped to a specific buffer kind.
    MapKeyKind {
        kind: String,
        keys: String,
        action: String,
    },
    /// Unmap a key binding scoped to a specific buffer kind.
    UnmapKeyKind {
        kind: String,
        keys: String,
    },
    SetBufferVar {
        buf: DocumentId,
        key: String,
        value: crate::annotations::value::Value,
    },
    StartJob {
        id: u64,
        spec: crate::job_manager::jobs::plugin_process::JobSpec,
    },
    JobInput {
        id: u64,
        data: Vec<u8>,
    },
    JobCloseStdin(u64),
    StopJob(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HighlightRange {
    #[cfg(feature = "treesitter")]
    Bytes(std::ops::Range<usize>),
    Points {
        start: (usize, usize),
        end: (usize, usize),
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HighlightSpec {
    Group(String),
    Style(crate::annotations::presentation::StyleOverride),
}

/// Where a plugin-authored annotation is anchored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnnotationAnchorSpec {
    Line(usize),
    Point(usize),
    Range(usize, usize),
}

pub(crate) fn annotation_owner(owner: Option<&str>) -> crate::annotations::AnnotationOwner {
    use crate::annotations::AnnotationOwner;
    match owner {
        Some("system") => AnnotationOwner::System,
        Some("lsp") => AnnotationOwner::Lsp,
        Some("user") => AnnotationOwner::User,
        _ => AnnotationOwner::Plugin(String::new()),
    }
}

/// Context passed to a Lua annotation action handler on activation.
#[derive(Debug, Clone)]
pub(crate) struct AnnotationActionCtx {
    pub(crate) annotation_id: u64,
    pub(crate) kind: String,
    pub(crate) verb: String,
    pub(crate) payload: crate::annotations::value::Value,
    /// The activated action's serializable args.
    pub(crate) params: crate::annotations::value::Value,
    pub(crate) position: usize,
    pub(crate) buffer: u64,
}

/// Context passed to a Lua cursor enter/leave hook.
#[derive(Debug, Clone)]
pub(crate) struct AnnotationHoverCtx {
    pub(crate) annotation_id: u64,
    pub(crate) kind: String,
    pub(crate) payload: crate::annotations::value::Value,
    pub(crate) position: usize,
    pub(crate) buffer: u64,
}

/// A floating window owned by a plugin. Stored in `PluginHost` and rendered
/// on each frame until `CloseFloat` is queued.
#[derive(Debug, Clone)]
pub(crate) struct PluginFloat {
    title: String,
    lines: Vec<String>,
    /// Screen row to keep uncovered: the float sits below it, else above.
    anchor_row: Option<usize>,
    /// First visible line of `lines`.
    scroll: usize,
    /// Content rows shown at the last render; bounds `scroll_float`.
    visible_lines: usize,
}

impl PluginFloat {
    pub(crate) fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
            anchor_row: None,
            scroll: 0,
            visible_lines: 0,
        }
    }

    /// Anchor the float to a screen row (the cursor's) instead of centering.
    #[must_use]
    pub(crate) const fn with_anchor_row(mut self, row: usize) -> Self {
        self.anchor_row = Some(row);
        self
    }
}

/// Tracks idle time for `CursorHold` events.
#[derive(Debug)]
struct CursorHoldState {
    /// Last cursor position seen.
    last_pos: (DocumentId, usize, usize),
    last_move: crate::time::Instant,
    delay: std::time::Duration,
    /// Whether `CursorHold` has already fired for this idle period.
    fired: bool,
}

impl CursorHoldState {
    fn new(delay: std::time::Duration) -> Self {
        Self {
            last_pos: (DocumentId::new(0), 0, 0),
            last_move: crate::time::Instant::now(),
            delay,
            fired: false,
        }
    }

    fn deadline(&self) -> Option<crate::time::Instant> {
        if self.fired {
            None
        } else {
            self.last_move.checked_add(self.delay)
        }
    }

    fn fire(&mut self, now: crate::time::Instant) -> Option<(DocumentId, usize, usize)> {
        if self.deadline().is_none_or(|deadline| deadline > now) {
            return None;
        }
        self.fired = true;
        Some(self.last_pos)
    }

    /// Called when the cursor moves. Resets idle tracking.
    fn on_cursor_move(
        &mut self,
        buf: DocumentId,
        row: usize,
        col: usize,
        now: crate::time::Instant,
    ) {
        let new_pos = (buf, row, col);
        if self.last_pos != new_pos {
            self.last_pos = new_pos;
            self.last_move = now;
            self.fired = false;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BufSyncKey {
    pub(crate) text: u64,
    pub(crate) meta: u64,
    pub(crate) options: u64,
    pub(crate) annotations: u64,
}

/// Central plugin coordinator, owned by the `Editor`: dispatches [`EditorEvent`]s to handlers,
/// queues [`PluginMutation`]s, tracks cursor-hold idle state, and holds registered commands/actions.
pub(crate) struct PluginHost {
    /// Handlers indexed by event name; a `BTreeMap` keeps iteration order deterministic.
    handlers: std::collections::BTreeMap<&'static str, Vec<(Handler, Option<PluginGeneration>)>>,
    /// Registered `:command` handlers with optional origin generation. Key is lowercase command name.
    commands: std::collections::HashMap<String, (CommandHandler, Option<PluginGeneration>)>,
    /// Optional one-line description for each registered command.
    command_descriptions: std::collections::HashMap<String, String>,
    /// Registered keymap action handlers with optional origin generation. Key matches `EditorAction::PluginAction(id)`.
    actions: std::collections::HashMap<String, (ActionHandler, Option<PluginGeneration>)>,
    /// Currently open plugin float, if any.
    open_float: Option<PluginFloat>,
    /// Set to `true` when a float was just closed so the layer can be cleared once.
    float_just_closed: bool,
    /// Mutations queued by handlers during dispatch or command execution.
    mutation_queue: Vec<PluginMutationEnvelope>,
    /// Cursor-hold idle tracker.
    cursor_hold: CursorHoldState,
    /// Embedded Lua VM for script plugins. `None` until `init_lua()` is called.
    lua: Option<lua_host::LuaHost>,
    /// True once any Lua plugin file or `:lua` snippet has run. Editor-state
    /// snapshots are skipped until then (nothing Lua-side can read them).
    lua_used: std::cell::Cell<bool>,
    synced_bufs: std::cell::RefCell<std::collections::HashMap<u64, BufSyncKey>>,
    #[cfg(feature = "treesitter")]
    synced_syntax: std::cell::RefCell<std::collections::HashMap<u64, (u64, usize)>>,
    /// Registered plugin names mapped to stable `PluginId`.
    #[cfg(test)]
    plugin_names: std::collections::HashMap<String, PluginId>,
    /// Reverse mapping of `PluginId` to plugin name.
    #[cfg(test)]
    plugin_ids: std::collections::HashMap<PluginId, String>,
    /// Tracked generation lifecycles.
    generations: std::collections::BTreeMap<PluginGeneration, GenerationStatus>,
    /// Dedicated `PluginId` for Lua plugins.
    lua_plugin_id: PluginId,
    /// Currently active Lua generation, if Lua is initialized.
    current_lua_generation: Option<PluginGeneration>,
}

impl PluginHost {
    #[must_use]
    pub(crate) fn new(cursor_hold_delay: std::time::Duration) -> Self {
        Self {
            handlers: std::collections::BTreeMap::new(),
            commands: std::collections::HashMap::new(),
            command_descriptions: std::collections::HashMap::new(),
            actions: std::collections::HashMap::new(),
            open_float: None,
            float_just_closed: false,
            mutation_queue: Vec::new(),
            cursor_hold: CursorHoldState::new(cursor_hold_delay),
            lua: None,
            lua_used: std::cell::Cell::new(false),
            synced_bufs: std::cell::RefCell::new(std::collections::HashMap::new()),
            #[cfg(feature = "treesitter")]
            synced_syntax: std::cell::RefCell::new(std::collections::HashMap::new()),
            #[cfg(test)]
            plugin_names: std::collections::HashMap::new(),
            #[cfg(test)]
            plugin_ids: std::collections::HashMap::new(),
            generations: std::collections::BTreeMap::new(),
            lua_plugin_id: allocate_next_plugin_id(),
            current_lua_generation: None,
        }
    }

    /// Whether any Lua code has run and can therefore observe editor state.
    pub(crate) const fn lua_state_wanted(&self) -> bool {
        self.lua_used.get()
    }

    /// Record that Lua code is about to run for the first time.
    pub(crate) fn mark_lua_used(&self) {
        self.lua_used.set(true);
    }

    pub(crate) fn bufs_unchanged(&self, mut docs: impl Iterator<Item = (u64, BufSyncKey)>) -> bool {
        let synced = self.synced_bufs.borrow();
        let mut count = 0;
        docs.all(|(doc_id, key)| {
            count += 1;
            synced.get(&doc_id) == Some(&key)
        }) && count == synced.len()
    }

    pub(crate) fn sync_buf(&self, doc_id: u64, key: BufSyncKey) -> Option<BufSyncKey> {
        self.synced_bufs.borrow_mut().insert(doc_id, key)
    }

    pub(crate) fn retain_synced_bufs(&self, live: &[u64]) {
        self.synced_bufs
            .borrow_mut()
            .retain(|doc_id, _| live.contains(doc_id));
    }

    pub(crate) fn resync_diverged_lua_state(&self) {
        let Some(lua) = &self.lua else {
            return;
        };
        let mut synced = self.synced_bufs.borrow_mut();
        for doc_id in lua.take_divergence() {
            synced.remove(&doc_id);
        }
    }

    #[cfg(feature = "treesitter")]
    pub(crate) fn syntax_needs_sync(&self, doc_id: u64, revision: u64, root_id: usize) -> bool {
        self.synced_syntax
            .borrow_mut()
            .insert(doc_id, (revision, root_id))
            != Some((revision, root_id))
    }

    #[cfg(feature = "treesitter")]
    pub(crate) fn syntax_unchanged(
        &self,
        mut docs: impl Iterator<Item = (u64, u64, usize)>,
    ) -> bool {
        let synced = self.synced_syntax.borrow();
        let mut count = 0;
        docs.all(|(doc_id, revision, root_id)| {
            count += 1;
            synced.get(&doc_id) == Some(&(revision, root_id))
        }) && count == synced.len()
    }

    #[cfg(feature = "treesitter")]
    pub(crate) fn retain_synced_syntax(&self, live: &[u64]) {
        self.synced_syntax
            .borrow_mut()
            .retain(|doc_id, _| live.contains(doc_id));
    }

    #[cfg(test)]
    /// Register a handler for a named event. `event_name` must match one of the strings
    /// returned by [`EditorEvent::name`], e.g. `"BufSavePost"`.
    pub(crate) fn on<F>(&mut self, event_name: &'static str, handler: F)
    where
        F: Fn(&EditorEvent) + Send + 'static,
    {
        self.handlers
            .entry(event_name)
            .or_default()
            .push((Box::new(handler), None));
    }

    #[cfg(test)]
    /// Register a command handler owned by a specific plugin generation.
    fn register_command_with_generation<F>(
        &mut self,
        name: &str,
        origin: PluginGeneration,
        handler: F,
    ) -> String
    where
        F: Fn(&[String]) -> Vec<PluginMutation> + Send + 'static,
    {
        let key = name.to_lowercase();
        self.commands
            .insert(key.clone(), (Box::new(handler), Some(origin)));
        key
    }

    #[cfg(test)]
    /// Returns `true` if a plugin command with this name is registered.
    fn has_command(&self, name: &str) -> bool {
        self.commands.contains_key(&name.to_lowercase())
    }

    /// Returns all registered plugin command names, descriptions, and arg types.
    /// Includes both Rust-registered commands and Lua-registered commands.
    pub(crate) fn command_list(&self) -> Vec<(String, String, Option<String>)> {
        let mut list: Vec<(String, String, Option<String>)> = self
            .commands
            .keys()
            .map(|name| {
                let desc = self
                    .command_descriptions
                    .get(name)
                    .cloned()
                    .unwrap_or_default();
                (name.clone(), desc, None)
            })
            .collect();
        if let Some(lua) = &self.lua {
            list.extend(lua.command_list());
        }
        list
    }

    /// Execute a registered plugin command, queuing any returned mutations.
    /// Returns `true` if a handler was found (Rust or Lua).
    pub(crate) fn execute_command(&mut self, name: &str, args: &[String]) -> bool {
        let key = name.to_lowercase();
        let (mutations, origin) = if let Some((handler, origin)) = self.commands.get(&key) {
            let origin = *origin;
            if let Some(gen) = origin {
                if !self.is_generation_active(gen) {
                    return false;
                }
            }
            (Some(handler(args)), origin)
        } else {
            (None, None)
        };

        if let Some(mutations) = mutations {
            for m in mutations {
                if let Some(gen) = origin {
                    self.apply_mutation_with_origin(m, gen);
                } else {
                    self.apply_mutation(m);
                }
            }
            return true;
        }

        if let Some(lua) = &self.lua {
            if lua.execute_command(name, args) {
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    /// Register a keymap action handler owned by a specific plugin generation.
    fn register_action_with_generation<F>(&mut self, id: &str, origin: PluginGeneration, handler: F)
    where
        F: Fn() -> Vec<PluginMutation> + Send + 'static,
    {
        self.actions
            .insert(id.to_string(), (Box::new(handler), Some(origin)));
    }

    /// Execute a registered plugin action, queuing any returned mutations.
    /// Returns `true` if a handler was found (Rust or Lua).
    pub(crate) fn execute_action(&mut self, id: &str) -> bool {
        let (mutations, origin) = if let Some((handler, origin)) = self.actions.get(id) {
            let origin = *origin;
            if let Some(gen) = origin {
                if !self.is_generation_active(gen) {
                    return false;
                }
            }
            (Some(handler()), origin)
        } else {
            (None, None)
        };

        if let Some(mutations) = mutations {
            for m in mutations {
                if let Some(gen) = origin {
                    self.apply_mutation_with_origin(m, gen);
                } else {
                    self.apply_mutation(m);
                }
            }
            return true;
        }

        if let Some(lua) = &self.lua {
            if lua.execute_action(id) {
                return true;
            }
        }
        false
    }

    /// Returns `true` if a plugin float is currently open.
    pub(crate) const fn has_open_float(&self) -> bool {
        self.open_float.is_some()
    }

    /// Close the open float immediately (e.g. when Escape is pressed).
    pub(crate) fn close_float(&mut self) {
        if self.open_float.is_some() {
            self.open_float = None;
            self.float_just_closed = true;
        }
    }

    /// Returns `true` once after a float was closed, so the render layer
    /// can be cleared. Resets the flag on read.
    pub(crate) const fn take_float_closed(&mut self) -> bool {
        let val = self.float_just_closed;
        self.float_just_closed = false;
        val
    }

    /// Scroll the open float's visible lines by `delta`, clamped to its content.
    pub(crate) fn scroll_float(&mut self, delta: isize) {
        if let Some(float) = &mut self.open_float {
            let max = float.lines.len().saturating_sub(float.visible_lines.max(1));
            float.scroll = float.scroll.saturating_add_signed(delta).min(max);
        }
    }

    /// Render the open float (if any) into the given layer. `fg`/`bg` should be the editor's
    /// current theme colors so the float blends with the UI instead of using reverse-video defaults.
    pub(crate) fn render_float_into_layer(
        &mut self,
        layer: &mut crate::layer::Layer,
        fg: Option<crate::color::Color>,
        bg: Option<crate::color::Color>,
    ) {
        use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
        use crate::layer::Cell;
        use unicode_width::UnicodeWidthStr;

        let Some(float) = &mut self.open_float else {
            return;
        };

        let rows = layer.rows();
        let cols = layer.cols();
        let total = float.lines.len();

        // Anchored: below the anchor row if it fits, else above; never on it.
        // The last screen row (status line) is never used. Otherwise centered.
        let (position, content_h, left_aligned) = float.anchor_row.map_or_else(
            || {
                (
                    WindowPosition::Center,
                    total.min(rows.saturating_sub(4)),
                    false,
                )
            },
            |r| {
                let below = rows.saturating_sub(r + 2);
                let above = r;
                let wanted = total + 2;
                let (top, height) = if wanted <= below {
                    (r + 1, wanted)
                } else if wanted <= above {
                    (r - wanted, wanted)
                } else if below >= above {
                    (r + 1, below)
                } else {
                    (0, above)
                };
                let position = WindowPosition::Absolute {
                    row: u16::try_from(top).unwrap_or(u16::MAX),
                    col: 0,
                };
                (position, height.saturating_sub(2), true)
            },
        );
        float.visible_lines = content_h;
        float.scroll = float.scroll.min(total.saturating_sub(content_h));
        let overflow = total > content_h;
        let title = if overflow {
            format!(
                " {} [{}/{}] ",
                float.title,
                (float.scroll + content_h).min(total),
                total
            )
        } else {
            format!(" {} ", float.title)
        };

        // Size to fit content using unicode display width, not `.len()` (byte length),
        // which would give wrong widths for CJK or other multi-byte characters.
        let content_w = float
            .lines
            .iter()
            .map(|l| UnicodeWidthStr::width(l.as_str()))
            .max()
            .unwrap_or(20)
            .max(UnicodeWidthStr::width(title.as_str()))
            .min(cols.saturating_sub(if left_aligned { 2 } else { 4 }));
        let width = (content_w + 2).min(cols);
        let height = (content_h + 2).min(rows);

        let mut style = WindowStyle::default().with_reverse_video(false);
        if let Some(f) = fg {
            style = style.with_fg(f);
        }
        if let Some(b) = bg {
            style = style.with_bg(b);
        }

        let window = FloatingWindow::with_style(position, width, height, style);

        let char_lines: Vec<Vec<char>> = float
            .lines
            .iter()
            .skip(float.scroll)
            .take(content_h)
            .map(|l| l.chars().collect())
            .collect();

        window.render(layer, &char_lines);

        // Title on the top border, clipped to the inner width.
        let (top, left) = window.calculate_position(
            u16::try_from(rows).unwrap_or(u16::MAX),
            u16::try_from(cols).unwrap_or(u16::MAX),
        );
        let (top, left) = (top as usize, left as usize + 1);
        let mut col = 0;
        for ch in title.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w == 0 || col + w > width.saturating_sub(2) {
                continue;
            }
            layer.set_cell(top, left + col, Cell::from_char(ch).with_colors(fg, bg));
            for k in 1..w {
                layer.set_cell(
                    top,
                    left + col + k,
                    Cell::from_char(' ').with_colors(fg, bg),
                );
            }
            col += w;
        }
    }

    /// Initialize the Lua VM. Must be called once at startup or reload.
    /// Returns any error string if Lua initialization fails.
    pub(crate) fn init_lua(
        &mut self,
        language_loader: std::sync::Arc<crate::syntax::loader::LanguageLoader>,
    ) -> Option<String> {
        if let Some(old_gen) = self.current_lua_generation.take() {
            self.retire_generation(old_gen);
        }
        if let Some(lua) = &self.lua {
            lua.mark_retiring();
        }
        self.lua.take();
        self.synced_bufs.borrow_mut().clear();
        #[cfg(feature = "treesitter")]
        self.synced_syntax.borrow_mut().clear();

        let new_gen = self.new_generation(self.lua_plugin_id);
        self.current_lua_generation = Some(new_gen);

        match lua_host::LuaHost::with_generation(new_gen, language_loader) {
            Ok(host) => {
                self.lua = Some(host);
                None
            }
            Err(e) => Some(format!("Failed to initialize Lua: {e}")),
        }
    }

    /// Update the Lua VM's buffer snapshot before dispatching events.
    pub(crate) fn lua_update_state(&self, update: lua_host::StateUpdate<'_>) {
        if let Some(lua) = &self.lua {
            lua.update_state(update);
        }
    }

    pub(crate) fn lua_synced_keys(&self) -> lua_host::SyncKeys {
        self.lua
            .as_ref()
            .map_or_else(lua_host::SyncKeys::default, lua_host::LuaHost::synced_keys)
    }

    pub(crate) fn lua_take_creation_ids(&self) -> Vec<u64> {
        self.lua
            .as_ref()
            .map_or_else(Vec::new, lua_host::LuaHost::take_creation_ids)
    }

    /// Load all top-level `.lua` files in `dir`, sorted lexicographically. Returns error strings.
    pub(crate) fn lua_load_dir(&self, dir: &std::path::Path) -> Vec<String> {
        self.lua
            .as_ref()
            .map_or_else(Vec::new, |lua| lua.load_dir(dir))
    }

    /// Execute a single `.lua` file. Returns an error string on failure.
    pub(crate) fn lua_load_file(&self, path: &std::path::Path) -> Option<String> {
        let lua = self.lua.as_ref()?;
        self.lua_used.set(true);
        lua.load_file(path)
    }

    /// Execute a Lua snippet directly (for `:lua` command).
    pub(crate) fn lua_exec(&self, code: &str) -> Option<String> {
        let lua = self.lua.as_ref()?;
        self.lua_used.set(true);
        lua.exec(code)
    }

    /// Invoke a Lua annotation action handler. Returns `true` if one ran.
    pub(crate) fn invoke_annotation_action(&self, ctx: &AnnotationActionCtx) -> bool {
        self.lua
            .as_ref()
            .is_some_and(|lua| lua.invoke_annotation_action(ctx))
    }

    /// Invoke a Lua cursor enter/leave hook. Returns `true` if one ran.
    pub(crate) fn invoke_annotation_hook(&self, enter: bool, ctx: &AnnotationHoverCtx) -> bool {
        self.lua
            .as_ref()
            .is_some_and(|lua| lua.invoke_annotation_hook(enter, ctx))
    }

    pub(crate) fn has_annotation_hook(&self, enter: bool, kind: &str) -> bool {
        self.lua
            .as_ref()
            .is_some_and(|lua| lua.has_annotation_hook(enter, kind))
    }

    /// Invoke a Lua buffer-kind action callback. Returns `true` if handled.
    pub(crate) fn invoke_buffer_action(
        &self,
        buf_id: DocumentId,
        kind: &str,
        action: &str,
    ) -> bool {
        match &self.lua {
            Some(lua) => {
                if !self.is_generation_active(lua.generation()) {
                    return false;
                }
                lua.invoke_buffer_action(buf_id.get(), kind, action)
            }
            None => false,
        }
    }

    /// Invoke a Lua buffer-kind save callback. Returns `true` if handled.
    pub(crate) fn invoke_buffer_save(&self, buf_id: DocumentId, kind: &str) -> bool {
        match &self.lua {
            Some(lua) => {
                if !self.is_generation_active(lua.generation()) {
                    return false;
                }
                lua.invoke_buffer_save(buf_id.get(), kind)
            }
            None => false,
        }
    }

    /// Invoke a Lua buffer-kind close callback. Returns `true` if handled.
    pub(crate) fn invoke_buffer_close(&self, buf_id: DocumentId, kind: &str) -> bool {
        match &self.lua {
            Some(lua) => {
                if !self.is_generation_active(lua.generation()) {
                    return false;
                }
                lua.invoke_buffer_close(buf_id.get(), kind)
            }
            None => false,
        }
    }

    pub(crate) fn deliver_job_event(
        &self,
        job: u64,
        event: crate::job_manager::jobs::plugin_process::PluginJobEvent,
    ) {
        if let Some(lua) = &self.lua {
            if self.is_generation_active(lua.generation()) {
                lua.deliver_job_event(job, event);
            }
        }
    }

    /// Dispatch an event to all registered handlers, applying any returned mutations via
    /// [`Self::apply_mutation`].
    pub(crate) fn dispatch(&mut self, event: &EditorEvent) {
        if let EditorEvent::CursorMoved { buf, row, col } = event {
            self.cursor_hold
                .on_cursor_move(*buf, *row, *col, crate::time::Instant::now());
        }

        let name = event.name();
        if let Some(handlers) = self.handlers.get(name) {
            for (handler, origin) in handlers {
                if let Some(gen) = origin {
                    if !self.is_generation_active(*gen) {
                        continue;
                    }
                }
                handler(event);
            }
        }

        // Dispatch to Lua handlers and convert any errors to notifications.
        if let Some(lua) = &self.lua {
            let gen = lua.generation();
            if self.is_generation_active(gen) {
                for err in lua.dispatch_event(event) {
                    self.mutation_queue.push(PluginMutationEnvelope::plugin(
                        PluginMutation::Notify {
                            message: err,
                            level: crate::notification::NotificationType::Error,
                        },
                        gen,
                    ));
                }
            }
        }
    }

    pub(crate) fn fire_due_timers(&mut self, now: crate::time::Instant) -> bool {
        let hold = self.cursor_hold.fire(now);
        if let Some((buf, row, col)) = hold {
            self.dispatch(&EditorEvent::CursorHold { buf, row, col });
        }
        let lua_fired = self
            .active_lua()
            .is_some_and(|lua| lua.fire_due_timers(now));
        lua_fired || hold.is_some()
    }

    pub(crate) fn next_timer_deadline(&self) -> Option<crate::time::Instant> {
        self.active_lua()
            .and_then(lua_host::LuaHost::next_timer_deadline)
            .into_iter()
            .chain(self.cursor_hold.deadline())
            .min()
    }

    fn active_lua(&self) -> Option<&lua_host::LuaHost> {
        self.lua
            .as_ref()
            .filter(|lua| self.is_generation_active(lua.generation()))
    }

    pub(crate) fn set_cursor_hold_delay_ms(&mut self, delay_ms: u32) {
        self.cursor_hold.delay = std::time::Duration::from_millis(u64::from(delay_ms));
    }

    #[cfg(test)]
    /// Register or retrieve a process-local `PluginId` for a plugin by name.
    fn register_plugin(&mut self, name: &str) -> PluginId {
        if let Some(&id) = self.plugin_names.get(name) {
            return id;
        }
        let id = PluginId::allocate();
        self.plugin_names.insert(name.to_string(), id);
        self.plugin_ids.insert(id, name.to_string());
        id
    }

    #[cfg(test)]
    /// Look up the `PluginId` for a given plugin name, if registered.
    fn plugin_id_for_name(&self, name: &str) -> Option<PluginId> {
        self.plugin_names.get(name).copied()
    }

    #[cfg(test)]
    /// Look up the plugin name for a given `PluginId`, if known.
    fn plugin_name(&self, id: PluginId) -> Option<&str> {
        self.plugin_ids.get(&id).map(std::string::String::as_str)
    }

    /// Spawn a new active generation for a plugin. Generation numbers are
    /// monotonic and never reused across the process lifetime.
    fn new_generation(&mut self, plugin_id: PluginId) -> PluginGeneration {
        let gen = PluginGeneration::allocate(plugin_id);
        self.generations.insert(gen, GenerationStatus::Active);
        gen
    }

    /// Check whether a generation is currently active.
    pub(crate) fn is_generation_active(&self, generation: PluginGeneration) -> bool {
        self.generations.get(&generation).copied() == Some(GenerationStatus::Active)
    }

    #[cfg(test)]
    /// Check whether a generation is retiring.
    fn is_generation_retiring(&self, generation: PluginGeneration) -> bool {
        self.generations.get(&generation).copied() == Some(GenerationStatus::Retiring)
    }

    #[cfg(test)]
    /// Check whether a generation is retired.
    fn is_generation_retired(&self, generation: PluginGeneration) -> bool {
        self.generations.get(&generation).copied() == Some(GenerationStatus::Retired)
    }

    #[cfg(test)]
    /// Get the current lifecycle status of a generation.
    fn generation_status(&self, generation: PluginGeneration) -> Option<GenerationStatus> {
        self.generations.get(&generation).copied()
    }

    /// Mark a generation as retiring. New mutations carrying this generation will be rejected.
    /// Returns true if the generation was previously active.
    fn mark_generation_retiring(&mut self, generation: PluginGeneration) -> bool {
        if let Some(status) = self.generations.get_mut(&generation) {
            if *status == GenerationStatus::Active {
                *status = GenerationStatus::Retiring;
                return true;
            }
        }
        false
    }

    /// Mark a generation as retired and unregister its handlers.
    /// Returns true if the generation was known and not already retired.
    fn mark_generation_retired(&mut self, generation: PluginGeneration) -> bool {
        let mut changed = false;
        if let Some(status) = self.generations.get_mut(&generation) {
            if *status != GenerationStatus::Retired {
                *status = GenerationStatus::Retired;
                changed = true;
            }
        }
        if changed {
            self.unregister_generation_handlers(generation);
        }
        changed
    }

    /// Discard all queued mutations originating from the given generation.
    /// Returns the number of mutations discarded.
    fn discard_mutations_for_generation(&mut self, generation: PluginGeneration) -> usize {
        let initial_len = self.mutation_queue.len();
        self.mutation_queue
            .retain(|env| env.origin != Some(generation));
        initial_len.saturating_sub(self.mutation_queue.len())
    }

    /// Unregister all commands, actions, and event handlers owned by the given generation.
    fn unregister_generation_handlers(&mut self, generation: PluginGeneration) {
        self.commands
            .retain(|_, (_, origin)| *origin != Some(generation));
        self.command_descriptions
            .retain(|name, _| self.commands.contains_key(name));
        self.actions
            .retain(|_, (_, origin)| *origin != Some(generation));
        for handlers in self.handlers.values_mut() {
            handlers.retain(|(_, origin)| *origin != Some(generation));
        }
    }

    /// Fully retire a generation on the host: mark retiring, discard queued mutations,
    /// unregister handlers, and mark retired. Returns number of discarded mutations.
    pub(crate) fn retire_generation(&mut self, generation: PluginGeneration) -> usize {
        self.mark_generation_retiring(generation);
        let discarded = self.discard_mutations_for_generation(generation);
        self.mark_generation_retired(generation);
        discarded
    }

    /// Return a snapshot of all currently active generations.
    pub(crate) fn active_generations(&self) -> Vec<PluginGeneration> {
        self.generations
            .iter()
            .filter_map(|(&gen, &status)| {
                if status == GenerationStatus::Active {
                    Some(gen)
                } else {
                    None
                }
            })
            .collect()
    }

    #[cfg(test)]
    /// Number of mutations currently queued.
    const fn queued_mutation_count(&self) -> usize {
        self.mutation_queue.len()
    }

    #[cfg(test)]
    /// Queue a mutation to be applied by the main loop after dispatch returns.
    pub(crate) fn queue_mutation(&mut self, mutation: PluginMutation) {
        self.mutation_queue
            .push(PluginMutationEnvelope::native(mutation));
    }

    #[cfg(test)]
    /// Queue a mutation tagged with its originating plugin generation.
    /// Returns false if the generation is not active (discarding the mutation).
    fn queue_mutation_with_origin(
        &mut self,
        mutation: PluginMutation,
        origin: PluginGeneration,
    ) -> bool {
        if !self.is_generation_active(origin) {
            return false;
        }
        self.mutation_queue
            .push(PluginMutationEnvelope::plugin(mutation, origin));
        true
    }

    /// Apply a mutation immediately (used internally by command/action handlers). Float open/close
    /// mutations are applied directly to `open_float`; all others are queued for the main loop.
    pub(crate) fn apply_mutation(&mut self, mutation: PluginMutation) {
        self.apply_mutation_envelope(PluginMutationEnvelope::native(mutation));
    }

    /// Apply a mutation tagged with its originating plugin generation.
    /// Returns false if the generation is not active.
    fn apply_mutation_with_origin(
        &mut self,
        mutation: PluginMutation,
        origin: PluginGeneration,
    ) -> bool {
        if !self.is_generation_active(origin) {
            return false;
        }
        self.apply_mutation_envelope(PluginMutationEnvelope::plugin(mutation, origin))
    }

    /// Apply a mutation envelope. Float open/close mutations are applied directly
    /// to `open_float`; all others are queued for the main loop.
    fn apply_mutation_envelope(&mut self, envelope: PluginMutationEnvelope) -> bool {
        if let Some(origin) = envelope.origin {
            if !self.is_generation_active(origin) {
                return false;
            }
        }
        match envelope.mutation {
            PluginMutation::OpenFloat(f) => {
                self.open_float = Some(f);
                self.float_just_closed = false;
            }
            PluginMutation::CloseFloat => {
                if self.open_float.is_some() {
                    self.float_just_closed = true;
                }
                self.open_float = None;
            }
            other => {
                self.mutation_queue
                    .push(PluginMutationEnvelope::new(other, envelope.origin));
            }
        }
        true
    }

    #[cfg(test)]
    /// Drain all queued mutations, discarding any whose origin generation is no longer active.
    /// Called by the main loop after every `dispatch` call.
    fn drain_mutations(&mut self) -> impl Iterator<Item = PluginMutation> + '_ {
        if let Some(lua) = &self.lua {
            let gen = lua.generation();
            for m in lua.drain_mutations() {
                self.mutation_queue
                    .push(PluginMutationEnvelope::plugin(m, gen));
            }
        }
        let generations = &self.generations;
        self.mutation_queue.retain(|env| {
            env.origin
                .is_none_or(|gen| generations.get(&gen).copied() == Some(GenerationStatus::Active))
        });
        self.mutation_queue.drain(..).map(|env| env.mutation)
    }

    pub(crate) fn has_queued_mutations(&self) -> bool {
        self.mutation_queue
            .iter()
            .any(|env| env.origin.is_none_or(|gen| self.is_generation_active(gen)))
            || self.lua.as_ref().is_some_and(|lua| {
                lua.has_queued_mutations() && self.is_generation_active(lua.generation())
            })
    }

    /// Drain all queued mutation envelopes, discarding any whose origin generation is no longer active.
    pub(crate) fn drain_mutation_envelopes(
        &mut self,
    ) -> impl Iterator<Item = PluginMutationEnvelope> + '_ {
        if let Some(lua) = &self.lua {
            let gen = lua.generation();
            for m in lua.drain_mutations() {
                self.mutation_queue
                    .push(PluginMutationEnvelope::plugin(m, gen));
            }
        }
        let generations = &self.generations;
        self.mutation_queue.retain(|env| {
            env.origin
                .is_none_or(|gen| generations.get(&gen).copied() == Some(GenerationStatus::Active))
        });
        self.mutation_queue.drain(..)
    }
}

pub(crate) fn dir_has_lua_files(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten().any(|e| {
            let p = e.path();
            p.is_file() && p.extension().is_some_and(|x| x == "lua")
        })
    })
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handler_counts: std::collections::BTreeMap<_, _> =
            self.handlers.iter().map(|(k, v)| (k, v.len())).collect();
        f.debug_struct("PluginHost")
            .field("handlers", &handler_counts)
            .field("commands", &self.commands.keys().collect::<Vec<_>>())
            .field("actions", &self.actions.keys().collect::<Vec<_>>())
            .field("open_float", &self.open_float.as_ref().map(|f| &f.title))
            .field("queued_mutations", &self.mutation_queue.len())
            .field("active_generations", &self.active_generations())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "float_tests.rs"]
mod float_tests;

#[cfg(test)]
#[path = "identity_and_generation_tests.rs"]
mod identity_and_generation_tests;

#[cfg(test)]
#[path = "cursor_hold_tests.rs"]
mod cursor_hold_tests;
