//! Plugin system for Rift: an event bus ([`PluginHost::dispatch`]) with Rust closure handlers today, a Lua VM layer to come.
//! Invariant: the host never mutates editor state directly; mutations are queued as [`PluginMutation`] and applied via `execute_command`, preserving undo/dot-repeat.

pub mod events;
pub mod lua_state;

#[cfg(feature = "plugins")]
pub mod lua_host;
#[cfg(not(feature = "plugins"))]
#[path = "lua_host_stub.rs"]
pub mod lua_host;

#[cfg(feature = "plugins")]
mod lua_value;

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};

pub use events::EditorEvent;

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
pub struct PluginId(NonZeroU32);

impl PluginId {
    /// Create a new `PluginId` wrapping a nonzero u32.
    pub const fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    /// Allocate a fresh, process-unique `PluginId`.
    pub fn allocate() -> Self {
        allocate_next_plugin_id()
    }

    /// Return the raw `u32` value.
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    /// Return the underlying `NonZeroU32`.
    pub const fn as_nonzero(self) -> NonZeroU32 {
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
pub struct PluginGeneration {
    plugin_id: PluginId,
    generation: NonZeroU32,
}

impl PluginGeneration {
    /// Create a new `PluginGeneration` from a plugin id and a load generation counter.
    pub const fn new(plugin_id: PluginId, generation: NonZeroU32) -> Self {
        Self {
            plugin_id,
            generation,
        }
    }

    /// Allocate a fresh, globally monotonic generation for the given plugin.
    pub fn allocate(plugin_id: PluginId) -> Self {
        Self::new(plugin_id, allocate_next_generation_id())
    }

    /// The owning plugin identity.
    pub const fn plugin_id(self) -> PluginId {
        self.plugin_id
    }

    /// The monotonic load generation number for this plugin incarnation.
    pub const fn generation(self) -> NonZeroU32 {
        self.generation
    }

    /// Return the raw pair `(plugin_id, generation)` as u32 primitives.
    pub const fn raw(self) -> (u32, u32) {
        (self.plugin_id.get(), self.generation.get())
    }

    /// Return the raw pair `(plugin_id, generation)` as `NonZeroU32` values.
    pub const fn as_nonzeros(self) -> (NonZeroU32, NonZeroU32) {
        (self.plugin_id.as_nonzero(), self.generation)
    }

    /// Converts this `PluginGeneration` into an owner token for keymap layers.
    pub fn keymap_owner_token(self) -> crate::keymap::KeyBindingToken {
        let (pid, gen) = self.raw();
        crate::keymap::KeyBindingToken::new(((pid as u64) << 32) | (gen as u64))
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
pub enum GenerationStatus {
    /// Active generation. Permitted to queue mutations and handle events.
    Active,
    /// Generation is in the process of retiring. New mutations are rejected;
    /// pending work is being drained or discarded.
    Retiring,
    /// Generation has been retired and uninstalled. All references are inert.
    Retired,
}

impl GenerationStatus {
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    pub const fn is_retiring(self) -> bool {
        matches!(self, Self::Retiring)
    }

    pub const fn is_retired(self) -> bool {
        matches!(self, Self::Retired)
    }
}

/// A `PluginMutation` with optional origin generation; native mutations
/// carry `None` and are always active, plugin ones are origin-validated.
#[derive(Debug)]
pub struct PluginMutationEnvelope {
    pub mutation: PluginMutation,
    pub origin: Option<PluginGeneration>,
}

impl PluginMutationEnvelope {
    /// Create a native mutation envelope without plugin origin tracking.
    pub fn native(mutation: PluginMutation) -> Self {
        Self {
            mutation,
            origin: None,
        }
    }

    /// Create a plugin-originated mutation envelope tagged with the owning generation.
    pub fn plugin(mutation: PluginMutation, origin: PluginGeneration) -> Self {
        Self {
            mutation,
            origin: Some(origin),
        }
    }

    /// Create a mutation envelope with optional origin generation.
    pub fn new(mutation: PluginMutation, origin: Option<PluginGeneration>) -> Self {
        Self { mutation, origin }
    }

    /// Extract the inner [`PluginMutation`].
    pub fn into_mutation(self) -> PluginMutation {
        self.mutation
    }

    /// Reference the inner [`PluginMutation`].
    pub fn mutation(&self) -> &PluginMutation {
        &self.mutation
    }

    /// Mutably reference the inner [`PluginMutation`].
    pub fn mutation_mut(&mut self) -> &mut PluginMutation {
        &mut self.mutation
    }

    /// The origin plugin generation, if plugin-originated.
    pub fn origin(&self) -> Option<PluginGeneration> {
        self.origin
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
pub enum PluginMutation {
    /// Display a notification through Rift's notification system.
    Notify {
        message: String,
        level: NotificationType,
    },
    /// Append lines to the end of the active buffer.
    AppendLines(Vec<String>),
    /// Insert text at the current cursor position in the active buffer.
    InsertAtCursor(String),
    /// Delete `n` characters immediately before the cursor.
    DeleteBefore(usize),
    /// Delete `n` characters immediately after the cursor.
    DeleteForward(usize),
    /// Move the cursor to a specific position. `row` is 1-indexed; `col` is 0-indexed.
    SetCursor { row: usize, col: usize },
    /// Replace a line range with new content. `start` and `end` are 1-indexed and inclusive.
    /// The replaced region is deleted and `lines` are inserted in its place.
    ReplaceLines {
        start: usize,
        end: usize,
        lines: Vec<String>,
    },
    /// Add a foreground color highlight over a character range (lines 1-indexed, columns 0-indexed).
    /// `color` is a named color or hex string; `slot` identifies the owning plugin handler.
    AddHighlight {
        slot: u32,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
        color: String,
    },
    /// Remove all highlights owned by the given plugin slot.
    ClearHighlights { slot: u32 },
    /// Set a per-document option. Values are strings such as `"4"` or `"true"`.
    SetOption { name: String, value: String },
    /// Trigger a save of the active buffer to disk.
    SaveBuffer,
    /// Open a plugin-owned floating window.
    OpenFloat(PluginFloat),
    /// Close the currently open plugin float.
    CloseFloat,
    /// Execute an editor action by its string name (e.g. `"editor:save"`, `"mode:normal"`).
    ExecAction(String),
    /// Register a key binding. `mode` is "n", "i", "c", "s", or "g".
    /// `keys` is vim notation (e.g. `"<C-p>"`, `"gg"`). `action` is an action string.
    MapKey {
        mode: String,
        keys: String,
        action: String,
    },
    /// Set the viewport scroll position (top_line, left_col).
    SetScroll(usize, usize),
    /// Set the line ending for the active document ("lf" or "crlf").
    SetLineEnding(String),
    /// Remove a key binding for the selected mode and key sequence.
    UnmapKey { mode: String, keys: String },
    /// Move the cursor to `row` (1-indexed) and center the viewport on it.
    CenterOnLine(usize),
    /// Set the CursorHold idle delay in milliseconds.
    SetCursorHoldDelay(u32),
    /// Switch the active buffer to the given document ID.
    SwitchToBuffer(DocumentId),
    /// Open (or switch to) a file by path. `force` discards unsaved changes.
    OpenFile { path: String, force: bool },
    /// Close the current buffer. `force` discards unsaved changes.
    CloseBuffer { force: bool },
    /// Move the focused window in the given direction.
    MoveWindow {
        direction: crate::split::navigation::Direction,
    },
    /// Swap the contents of the focused window with the previously focused window.
    SwapWindows,
    /// Focus the previously focused window.
    FocusPreviousWindow,
    /// Register a file extension -> language name mapping.
    RegisterFiletype { ext: String, lang_name: String },
    /// Register (or override) the highlights query for a language.
    RegisterLanguageQuery {
        lang_name: String,
        query_src: String,
    },
    /// Register an injections query for a language (enables embedded-language highlighting).
    RegisterInjectionsQuery {
        lang_name: String,
        query_src: String,
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
    LspRename { new_name: String },
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
    /// Add an annotation to the active document (plugin-authored). `id` is the
    /// caller pre-claimed id (so `add{}` can return it synchronously).
    AddAnnotation {
        id: u64,
        kind: String,
        anchor: AnnotationAnchorSpec,
        payload: crate::annotations::Value,
        presentation: Option<crate::annotations::Presentation>,
        actions: Vec<(String, bool)>,
        visible: bool,
        stickiness: Option<String>,
        owner: Option<String>,
    },
    /// Mutate an existing annotation in place; `None` fields are left unchanged.
    UpdateAnnotation {
        id: u64,
        payload: Option<crate::annotations::Value>,
        visible: Option<bool>,
        presentation: Option<crate::annotations::Presentation>,
    },
    /// Remove an annotation from the active document by id.
    RemoveAnnotation(u64),
    /// Remove every annotation in the active document whose kind starts with this prefix.
    ClearAnnotations { kind_prefix: String },
    /// Register a handler for an annotation (kind, verb). With `command` set, the
    /// verb runs that ex command; otherwise it routes back to the Lua host.
    RegisterAnnotationAction {
        kind: String,
        verb: String,
        command: Option<String>,
    },
    /// Register per-kind render/hover defaults into the kind registry (sec 4).
    RegisterKindDefaults {
        kind: String,
        presentation: Option<crate::annotations::Presentation>,
        description: Option<String>,
    },
    /// Create and activate an in-memory buffer. Fires BufOpen after creation.
    CreateScratchBuf { name: String, lines: Vec<String> },
    /// Reload active buffer content from disk. Force discards unsaved changes.
    ReloadBuffer { force: bool },
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
        kind: String,
        title: Option<String>,
        lines: Vec<String>,
        vars: Vec<(String, crate::annotations::Value)>,
    },
    /// Map a key binding scoped to a specific buffer kind.
    MapKeyKind {
        kind: String,
        mode: String,
        keys: String,
        action: String,
    },
    /// Unmap a key binding scoped to a specific buffer kind.
    UnmapKeyKind {
        kind: String,
        mode: String,
        keys: String,
    },
    /// Set a buffer-local variable. `buf_id == None` targets the active document.
    SetBufferVar {
        buf_id: Option<DocumentId>,
        key: String,
        value: crate::annotations::Value,
    },
}

/// Where a plugin-authored annotation is anchored.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationAnchorSpec {
    Line(usize),
    Point(usize),
    Range(usize, usize),
}

/// Context passed to a Lua annotation action handler on activation.
#[derive(Debug, Clone)]
pub struct AnnotationActionCtx {
    pub annotation_id: u64,
    pub kind: String,
    pub verb: String,
    pub payload: crate::annotations::Value,
    /// The activated action's serializable args.
    pub params: crate::annotations::Value,
    pub position: usize,
    pub buffer: u64,
}

/// Context passed to a Lua cursor enter/leave hook.
#[derive(Debug, Clone)]
pub struct AnnotationHoverCtx {
    pub annotation_id: u64,
    pub kind: String,
    pub payload: crate::annotations::Value,
    pub position: usize,
    pub buffer: u64,
}

/// A floating window owned by a plugin. Stored in `PluginHost` and rendered
/// on each frame until `CloseFloat` is queued.
#[derive(Debug, Clone)]
pub struct PluginFloat {
    pub title: String,
    pub lines: Vec<String>,
    /// Screen row to keep uncovered: the float sits below it, else above.
    pub anchor_row: Option<usize>,
    /// First visible line of `lines`.
    pub scroll: usize,
    /// Content rows shown at the last render; bounds `scroll_float`.
    pub visible_lines: usize,
}

impl PluginFloat {
    pub fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
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
    pub fn with_anchor_row(mut self, row: usize) -> Self {
        self.anchor_row = Some(row);
        self
    }
}

/// Tracks idle time for `CursorHold` events.
#[derive(Debug)]
struct CursorHoldState {
    /// Last cursor position seen.
    last_pos: (DocumentId, usize, usize),
    /// How many consecutive idle polls have passed without cursor movement.
    idle_polls: u32,
    /// Number of idle polls before `CursorHold` fires (configurable).
    threshold_polls: u32,
    /// Whether `CursorHold` has already fired for this idle period.
    fired: bool,
}

impl CursorHoldState {
    fn new(threshold_polls: u32) -> Self {
        Self {
            last_pos: (0, 0, 0),
            idle_polls: 0,
            threshold_polls,
            fired: false,
        }
    }

    /// Called every idle frame. Returns `Some((buf, row, col))` when
    /// `CursorHold` should fire, `None` otherwise.
    fn tick(&mut self) -> Option<(DocumentId, usize, usize)> {
        if self.fired {
            return None;
        }
        self.idle_polls += 1;
        if self.idle_polls >= self.threshold_polls {
            self.fired = true;
            Some(self.last_pos)
        } else {
            None
        }
    }

    /// Called when the cursor moves. Resets idle tracking.
    fn on_cursor_move(&mut self, buf: DocumentId, row: usize, col: usize) {
        let new_pos = (buf, row, col);
        if self.last_pos != new_pos {
            self.last_pos = new_pos;
            self.idle_polls = 0;
            self.fired = false;
        }
    }
}

/// Central plugin coordinator, owned by the `Editor`: dispatches [`EditorEvent`]s to handlers,
/// queues [`PluginMutation`]s, tracks cursor-hold idle state, and holds registered commands/actions.
pub struct PluginHost {
    /// Handlers indexed by event name for O(1) lookup with optional origin generation.
    handlers: std::collections::HashMap<&'static str, Vec<(Handler, Option<PluginGeneration>)>>,
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
    /// `(doc_id, buffer.revision)` last synced to Lua, so an unchanged buffer
    /// isn't re-cloned into a fresh snapshot every sync.
    last_synced_buf: std::cell::Cell<Option<(u64, u64)>>,
    /// `(doc_id, annotations.revision)` last synced to Lua, so an unchanged
    /// annotation set isn't re-snapshotted every sync.
    last_synced_annotations: std::cell::Cell<Option<(u64, u64)>>,
    /// Registered plugin names mapped to stable PluginId.
    plugin_names: std::collections::HashMap<String, PluginId>,
    /// Reverse mapping of PluginId to plugin name.
    plugin_ids: std::collections::HashMap<PluginId, String>,
    /// Tracked generation lifecycles.
    generations: std::collections::HashMap<PluginGeneration, GenerationStatus>,
    /// Dedicated PluginId for Lua plugins.
    lua_plugin_id: PluginId,
    /// Currently active Lua generation, if Lua is initialized.
    current_lua_generation: Option<PluginGeneration>,
}

impl PluginHost {
    /// Create a new plugin host. `cursor_hold_polls` is the number of idle main-loop polls
    /// before `CursorHold` fires (at the default 16ms poll rate, 25 polls = ~400ms).
    pub fn new(cursor_hold_polls: u32) -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
            commands: std::collections::HashMap::new(),
            command_descriptions: std::collections::HashMap::new(),
            actions: std::collections::HashMap::new(),
            open_float: None,
            float_just_closed: false,
            mutation_queue: Vec::new(),
            cursor_hold: CursorHoldState::new(cursor_hold_polls),
            lua: None,
            lua_used: std::cell::Cell::new(false),
            last_synced_buf: std::cell::Cell::new(None),
            last_synced_annotations: std::cell::Cell::new(None),
            plugin_names: std::collections::HashMap::new(),
            plugin_ids: std::collections::HashMap::new(),
            generations: std::collections::HashMap::new(),
            lua_plugin_id: allocate_next_plugin_id(),
            current_lua_generation: None,
        }
    }

    /// Whether any Lua code has run and can therefore observe editor state.
    pub fn lua_state_wanted(&self) -> bool {
        self.lua_used.get()
    }

    /// Record that Lua code is about to run for the first time.
    pub fn mark_lua_used(&self) {
        self.lua_used.set(true);
    }

    /// Whether `doc_id` at `revision` is the same buffer state last synced to
    /// Lua, so the caller can skip re-cloning it into a fresh snapshot.
    pub fn synced_buf_matches(&self, doc_id: u64, revision: u64) -> bool {
        self.last_synced_buf.get() == Some((doc_id, revision))
    }

    /// Record `doc_id` at `revision` as the buffer state just synced to Lua.
    pub fn set_synced_buf(&self, doc_id: u64, revision: u64) {
        self.last_synced_buf.set(Some((doc_id, revision)));
    }

    /// Forget the last-synced buffer, e.g. when there's no active document.
    pub fn clear_synced_buf(&self) {
        self.last_synced_buf.set(None);
    }

    /// Whether `doc_id` at annotation-store `revision` is the same state
    /// last synced to Lua, so the caller can skip rebuilding the snapshot.
    pub fn synced_annotations_match(&self, doc_id: u64, revision: u64) -> bool {
        self.last_synced_annotations.get() == Some((doc_id, revision))
    }

    /// Record `doc_id` at annotation-store `revision` as just synced to Lua.
    pub fn set_synced_annotations(&self, doc_id: u64, revision: u64) {
        self.last_synced_annotations.set(Some((doc_id, revision)));
    }

    /// Forget the last-synced annotations, e.g. when there's no active document.
    pub fn clear_synced_annotations(&self) {
        self.last_synced_annotations.set(None);
    }

    /// Register a handler for a named event. `event_name` must match one of the strings
    /// returned by [`EditorEvent::name`], e.g. `"BufSavePost"`.
    pub fn on<F>(&mut self, event_name: &'static str, handler: F)
    where
        F: Fn(&EditorEvent) + Send + 'static,
    {
        self.handlers
            .entry(event_name)
            .or_default()
            .push((Box::new(handler), None));
    }

    /// Register a handler for a named event owned by a specific plugin generation.
    pub fn on_with_generation<F>(
        &mut self,
        event_name: &'static str,
        origin: PluginGeneration,
        handler: F,
    ) where
        F: Fn(&EditorEvent) + Send + 'static,
    {
        self.handlers
            .entry(event_name)
            .or_default()
            .push((Box::new(handler), Some(origin)));
    }

    /// Register a handler for a `:CommandName [args...]` ex-command.
    /// `name` is case-insensitive. Returns the registered name in lowercase.
    pub fn register_command<F>(&mut self, name: &str, handler: F) -> String
    where
        F: Fn(&[String]) -> Vec<PluginMutation> + Send + 'static,
    {
        let key = name.to_lowercase();
        self.commands.insert(key.clone(), (Box::new(handler), None));
        key
    }

    /// Register a command handler owned by a specific plugin generation.
    pub fn register_command_with_generation<F>(
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

    /// Set an optional one-line description for a command name.
    pub fn set_command_description(&mut self, name: &str, description: impl Into<String>) {
        self.command_descriptions
            .insert(name.to_lowercase(), description.into());
    }

    /// Returns `true` if a plugin command with this name is registered.
    pub fn has_command(&self, name: &str) -> bool {
        self.commands.contains_key(&name.to_lowercase())
    }

    /// Returns all registered plugin command names, descriptions, and arg types.
    /// Includes both Rust-registered commands and Lua-registered commands.
    pub fn command_list(&self) -> Vec<(String, String, Option<String>)> {
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
    pub fn execute_command(&mut self, name: &str, args: &[String]) -> bool {
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

    /// Register a handler for `Action::Editor(EditorAction::PluginAction(id))`.
    /// Call with the same `id` string you pass to `EditorAction::PluginAction`.
    pub fn register_action<F>(&mut self, id: &str, handler: F)
    where
        F: Fn() -> Vec<PluginMutation> + Send + 'static,
    {
        self.actions
            .insert(id.to_string(), (Box::new(handler), None));
    }

    /// Register a keymap action handler owned by a specific plugin generation.
    pub fn register_action_with_generation<F>(
        &mut self,
        id: &str,
        origin: PluginGeneration,
        handler: F,
    ) where
        F: Fn() -> Vec<PluginMutation> + Send + 'static,
    {
        self.actions
            .insert(id.to_string(), (Box::new(handler), Some(origin)));
    }

    /// Execute a registered plugin action, queuing any returned mutations.
    /// Returns `true` if a handler was found (Rust or Lua).
    pub fn execute_action(&mut self, id: &str) -> bool {
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
    pub fn has_open_float(&self) -> bool {
        self.open_float.is_some()
    }

    /// Close the open float immediately (e.g. when Escape is pressed).
    pub fn close_float(&mut self) {
        if self.open_float.is_some() {
            self.open_float = None;
            self.float_just_closed = true;
        }
    }

    /// Returns `true` once after a float was closed, so the render layer
    /// can be cleared. Resets the flag on read.
    pub fn take_float_closed(&mut self) -> bool {
        let val = self.float_just_closed;
        self.float_just_closed = false;
        val
    }

    /// Scroll the open float's visible lines by `delta`, clamped to its content.
    pub fn scroll_float(&mut self, delta: isize) {
        if let Some(float) = &mut self.open_float {
            let max = float.lines.len().saturating_sub(float.visible_lines.max(1));
            float.scroll = float.scroll.saturating_add_signed(delta).min(max);
        }
    }

    /// Render the open float (if any) into the given layer. `fg`/`bg` should be the editor's
    /// current theme colors so the float blends with the UI instead of using reverse-video defaults.
    pub fn render_float_into_layer(
        &mut self,
        layer: &mut crate::layer::Layer,
        fg: Option<crate::color::Color>,
        bg: Option<crate::color::Color>,
    ) {
        use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};
        use crate::layer::Cell;
        use unicode_width::UnicodeWidthStr;

        let float = match &mut self.open_float {
            Some(f) => f,
            None => return,
        };

        let rows = layer.rows();
        let cols = layer.cols();
        let total = float.lines.len();

        // Anchored: below the anchor row if it fits, else above; never on it.
        // The last screen row (status line) is never used. Otherwise centered.
        let (position, content_h, left_aligned) = match float.anchor_row {
            Some(r) => {
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
                    row: top as u16,
                    col: 0,
                };
                (position, height.saturating_sub(2), true)
            }
            None => (
                WindowPosition::Center,
                total.min(rows.saturating_sub(4)),
                false,
            ),
        };
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
        let (top, left) = window.calculate_position(rows as u16, cols as u16);
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
    pub fn init_lua(&mut self) -> Option<String> {
        if let Some(old_gen) = self.current_lua_generation.take() {
            self.retire_generation(old_gen);
        }
        if let Some(lua) = &self.lua {
            lua.mark_retiring();
        }
        self.lua.take();

        let new_gen = self.new_generation(self.lua_plugin_id);
        self.current_lua_generation = Some(new_gen);

        match lua_host::LuaHost::with_generation(new_gen) {
            Ok(host) => {
                self.lua = Some(host);
                None
            }
            Err(e) => Some(format!("Failed to initialize Lua: {}", e)),
        }
    }

    /// Return the currently active Lua generation, if any.
    pub fn current_lua_generation(&self) -> Option<PluginGeneration> {
        self.current_lua_generation
    }

    /// Update the Lua VM's buffer snapshot before dispatching events.
    #[allow(clippy::too_many_arguments)]
    pub fn lua_update_state(
        &self,
        buf_id: usize,
        buf_kind: String,
        source: lua_host::BufSourceUpdate,
        cursor: (usize, usize),
        tab_width: usize,
        expand_tabs: bool,
        mode: &str,
        filetype: Option<String>,
        file_path: Option<String>,
        buf_list: Vec<lua_host::BufEntry>,
        window_size: (u16, u16),
        can_undo: bool,
        can_redo: bool,
        is_dirty: bool,
        scroll: (usize, usize),
        line_ending: &str,
        commands: Vec<(String, String)>,
        win_list: Vec<lua_host::WinEntry>,
        focused_win_id: u64,
        previous_win_id: Option<u64>,
        lsp_diagnostics: std::collections::HashMap<String, Vec<(u32, u32, u32, String)>>,
        buffer_vars: std::collections::HashMap<
            u64,
            std::collections::HashMap<String, crate::annotations::Value>,
        >,
    ) {
        if let Some(lua) = &self.lua {
            lua.update_state(
                buf_id,
                buf_kind,
                source,
                cursor,
                tab_width,
                expand_tabs,
                mode,
                filetype,
                file_path,
                buf_list,
                window_size,
                can_undo,
                can_redo,
                is_dirty,
                scroll,
                line_ending,
                commands,
                win_list,
                focused_win_id,
                previous_win_id,
                lsp_diagnostics,
                buffer_vars,
            );
        }
    }

    /// Refresh the `rift.annotations` query snapshot and the next `add{}` id.
    pub fn lua_set_annotations(
        &self,
        views: Vec<crate::plugin::lua_host::AnnotationView>,
        next_id: u64,
    ) {
        if let Some(lua) = &self.lua {
            lua.set_annotations(views, next_id);
        }
    }

    /// Load all top-level `.lua` files in `dir`, sorted lexicographically. Returns error strings.
    pub fn lua_load_dir(&self, dir: &std::path::Path) -> Vec<String> {
        match &self.lua {
            Some(lua) => {
                let has_lua_files = std::fs::read_dir(dir)
                    .map(|rd| {
                        rd.flatten().any(|e| {
                            let p = e.path();
                            p.is_file() && p.extension().is_some_and(|x| x == "lua")
                        })
                    })
                    .unwrap_or(false);
                if has_lua_files {
                    self.lua_used.set(true);
                }
                lua.load_dir(dir)
            }
            None => vec![],
        }
    }

    /// Execute a single `.lua` file. Returns an error string on failure.
    pub fn lua_load_file(&self, path: &std::path::Path) -> Option<String> {
        let lua = self.lua.as_ref()?;
        self.lua_used.set(true);
        lua.load_file(path)
    }

    /// Execute a Lua snippet directly (for `:lua` command).
    pub fn lua_exec(&self, code: &str) -> Option<String> {
        let lua = self.lua.as_ref()?;
        self.lua_used.set(true);
        lua.exec(code)
    }

    /// Dispatch an event to all registered handlers, applying any returned mutations via [`apply_mutation`].
    /// Invoke a Lua annotation action handler. Returns `true` if one ran.
    pub fn invoke_annotation_action(&self, ctx: &AnnotationActionCtx) -> bool {
        match &self.lua {
            Some(lua) => lua.invoke_annotation_action(ctx),
            None => false,
        }
    }

    /// Invoke a Lua cursor enter/leave hook. Returns `true` if one ran.
    pub fn invoke_annotation_hook(&self, enter: bool, ctx: &AnnotationHoverCtx) -> bool {
        match &self.lua {
            Some(lua) => lua.invoke_annotation_hook(enter, ctx),
            None => false,
        }
    }

    /// Invoke a Lua buffer-kind action callback. Returns `true` if handled.
    pub fn invoke_buffer_action(&self, buf_id: DocumentId, kind: &str, action: &str) -> bool {
        match &self.lua {
            Some(lua) => {
                if !self.is_generation_active(lua.generation()) {
                    return false;
                }
                lua.invoke_buffer_action(buf_id, kind, action)
            }
            None => false,
        }
    }

    /// Invoke a Lua buffer-kind save callback. Returns `true` if handled.
    pub fn invoke_buffer_save(&self, buf_id: DocumentId, kind: &str) -> bool {
        match &self.lua {
            Some(lua) => {
                if !self.is_generation_active(lua.generation()) {
                    return false;
                }
                lua.invoke_buffer_save(buf_id, kind)
            }
            None => false,
        }
    }

    /// Invoke a Lua buffer-kind close callback. Returns `true` if handled.
    pub fn invoke_buffer_close(&self, buf_id: DocumentId, kind: &str) -> bool {
        match &self.lua {
            Some(lua) => {
                if !self.is_generation_active(lua.generation()) {
                    return false;
                }
                lua.invoke_buffer_close(buf_id, kind)
            }
            None => false,
        }
    }

    pub fn dispatch(&mut self, event: &EditorEvent) {
        if let EditorEvent::CursorMoved { buf, row, col } = event {
            self.cursor_hold.on_cursor_move(*buf, *row, *col);
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

    /// Called on every idle frame (no input); fires `CursorHold` if the cursor has been stationary
    /// long enough. Returns the event to dispatch, if any, so the caller avoids a double-borrow.
    pub fn tick_idle(&mut self) -> Option<EditorEvent> {
        self.cursor_hold
            .tick()
            .map(|(buf, row, col)| EditorEvent::CursorHold { buf, row, col })
    }

    /// Update the CursorHold threshold from a millisecond value.
    /// `poll_ms` is the main-loop poll interval (typically 16).
    pub fn set_cursor_hold_delay_ms(&mut self, delay_ms: u32, poll_ms: u32) {
        let polls = (delay_ms / poll_ms.max(1)).max(1);
        self.cursor_hold.threshold_polls = polls;
    }

    /// Register or retrieve a process-local `PluginId` for a plugin by name.
    pub fn register_plugin(&mut self, name: &str) -> PluginId {
        if let Some(&id) = self.plugin_names.get(name) {
            return id;
        }
        let id = PluginId::allocate();
        self.plugin_names.insert(name.to_string(), id);
        self.plugin_ids.insert(id, name.to_string());
        id
    }

    /// Look up the `PluginId` for a given plugin name, if registered.
    pub fn plugin_id_for_name(&self, name: &str) -> Option<PluginId> {
        self.plugin_names.get(name).copied()
    }

    /// Look up the plugin name for a given `PluginId`, if known.
    pub fn plugin_name(&self, id: PluginId) -> Option<&str> {
        self.plugin_ids.get(&id).map(|s| s.as_str())
    }

    /// Spawn a new active generation for a plugin. Generation numbers are
    /// monotonic and never reused across the process lifetime.
    pub fn new_generation(&mut self, plugin_id: PluginId) -> PluginGeneration {
        let gen = PluginGeneration::allocate(plugin_id);
        self.generations.insert(gen, GenerationStatus::Active);
        gen
    }

    /// Check whether a generation is currently active.
    pub fn is_generation_active(&self, generation: PluginGeneration) -> bool {
        self.generations.get(&generation).copied() == Some(GenerationStatus::Active)
    }

    /// Check whether a generation is retiring.
    pub fn is_generation_retiring(&self, generation: PluginGeneration) -> bool {
        self.generations.get(&generation).copied() == Some(GenerationStatus::Retiring)
    }

    /// Check whether a generation is retired.
    pub fn is_generation_retired(&self, generation: PluginGeneration) -> bool {
        self.generations.get(&generation).copied() == Some(GenerationStatus::Retired)
    }

    /// Get the current lifecycle status of a generation.
    pub fn generation_status(&self, generation: PluginGeneration) -> Option<GenerationStatus> {
        self.generations.get(&generation).copied()
    }

    /// Mark a generation as retiring. New mutations carrying this generation will be rejected.
    /// Returns true if the generation was previously active.
    pub fn mark_generation_retiring(&mut self, generation: PluginGeneration) -> bool {
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
    pub fn mark_generation_retired(&mut self, generation: PluginGeneration) -> bool {
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
    pub fn discard_mutations_for_generation(&mut self, generation: PluginGeneration) -> usize {
        let initial_len = self.mutation_queue.len();
        self.mutation_queue
            .retain(|env| env.origin != Some(generation));
        initial_len.saturating_sub(self.mutation_queue.len())
    }

    /// Unregister all commands, actions, and event handlers owned by the given generation.
    pub fn unregister_generation_handlers(&mut self, generation: PluginGeneration) {
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
    pub fn retire_generation(&mut self, generation: PluginGeneration) -> usize {
        self.mark_generation_retiring(generation);
        let discarded = self.discard_mutations_for_generation(generation);
        self.mark_generation_retired(generation);
        discarded
    }

    /// Mark all active generations as retiring, discard their queued mutations,
    /// and unregister their handlers. Returns the total number of discarded mutations.
    pub fn retire_all_generations(&mut self) -> usize {
        for status in self.generations.values_mut() {
            if *status == GenerationStatus::Active {
                *status = GenerationStatus::Retiring;
            }
        }
        let initial_len = self.mutation_queue.len();
        let generations = &self.generations;
        self.mutation_queue.retain(|env| match env.origin {
            Some(gen) => generations.get(&gen).copied() == Some(GenerationStatus::Active),
            None => true,
        });
        let discarded = initial_len.saturating_sub(self.mutation_queue.len());
        let to_retire: Vec<PluginGeneration> = self
            .generations
            .iter()
            .filter_map(|(&gen, &status)| {
                if status == GenerationStatus::Retiring {
                    Some(gen)
                } else {
                    None
                }
            })
            .collect();
        for gen in to_retire {
            self.mark_generation_retired(gen);
        }
        discarded
    }

    /// Return a snapshot of all currently active generations.
    pub fn active_generations(&self) -> Vec<PluginGeneration> {
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

    /// Number of mutations currently queued.
    pub fn queued_mutation_count(&self) -> usize {
        self.mutation_queue.len()
    }

    /// Inspect the currently queued mutation envelopes.
    pub fn queued_mutations(&self) -> &[PluginMutationEnvelope] {
        &self.mutation_queue
    }

    /// Queue a mutation to be applied by the main loop after dispatch returns.
    pub fn queue_mutation(&mut self, mutation: PluginMutation) {
        self.mutation_queue
            .push(PluginMutationEnvelope::native(mutation));
    }

    /// Queue a mutation tagged with its originating plugin generation.
    /// Returns false if the generation is not active (discarding the mutation).
    pub fn queue_mutation_with_origin(
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

    /// Queue a mutation envelope.
    /// Returns false if the envelope carries a generation that is not active.
    pub fn queue_mutation_envelope(&mut self, envelope: PluginMutationEnvelope) -> bool {
        if let Some(origin) = envelope.origin {
            if !self.is_generation_active(origin) {
                return false;
            }
        }
        self.mutation_queue.push(envelope);
        true
    }

    /// Apply a mutation immediately (used internally by command/action handlers). Float open/close
    /// mutations are applied directly to `open_float`; all others are queued for the main loop.
    pub fn apply_mutation(&mut self, mutation: PluginMutation) {
        self.apply_mutation_envelope(PluginMutationEnvelope::native(mutation));
    }

    /// Apply a mutation tagged with its originating plugin generation.
    /// Returns false if the generation is not active.
    pub fn apply_mutation_with_origin(
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
    pub fn apply_mutation_envelope(&mut self, envelope: PluginMutationEnvelope) -> bool {
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

    /// Drain all queued mutations, discarding any whose origin generation is no longer active.
    /// Called by the main loop after every `dispatch` call.
    pub fn drain_mutations(&mut self) -> impl Iterator<Item = PluginMutation> + '_ {
        if let Some(lua) = &self.lua {
            let gen = lua.generation();
            for m in lua.drain_mutations() {
                self.mutation_queue
                    .push(PluginMutationEnvelope::plugin(m, gen));
            }
        }
        let generations = &self.generations;
        self.mutation_queue.retain(|env| match env.origin {
            Some(gen) => generations.get(&gen).copied() == Some(GenerationStatus::Active),
            None => true,
        });
        self.mutation_queue.drain(..).map(|env| env.mutation)
    }

    /// Drain all queued mutation envelopes, discarding any whose origin generation is no longer active.
    pub fn drain_mutation_envelopes(
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
        self.mutation_queue.retain(|env| match env.origin {
            Some(gen) => generations.get(&gen).copied() == Some(GenerationStatus::Active),
            None => true,
        });
        self.mutation_queue.drain(..)
    }
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handler_counts: std::collections::HashMap<_, _> =
            self.handlers.iter().map(|(k, v)| (k, v.len())).collect();
        f.debug_struct("PluginHost")
            .field("handlers", &handler_counts)
            .field("commands", &self.commands.keys().collect::<Vec<_>>())
            .field("actions", &self.actions.keys().collect::<Vec<_>>())
            .field("open_float", &self.open_float.as_ref().map(|f| &f.title))
            .field("queued_mutations", &self.mutation_queue.len())
            .field("active_generations", &self.active_generations())
            .finish()
    }
}

#[cfg(test)]
mod float_tests {
    use super::*;
    use crate::layer::{Layer, LayerPriority};

    fn row_text(layer: &Layer, row: usize) -> String {
        (0..layer.cols())
            .map(|c| match layer.get_cell(row, c).map(|c| c.content) {
                Some(crate::character::Character::Unicode(ch)) => ch,
                _ => '.',
            })
            .collect()
    }

    fn drawn_rows(layer: &Layer) -> Vec<usize> {
        (0..layer.rows())
            .filter(|&r| (0..layer.cols()).any(|c| layer.get_cell(r, c).is_some()))
            .collect()
    }

    fn host_with(float: PluginFloat) -> PluginHost {
        let mut host = PluginHost::new(1);
        host.apply_mutation(PluginMutation::OpenFloat(float));
        host
    }

    #[test]
    fn anchored_float_sits_below_cursor_row_when_it_fits() {
        let mut layer = Layer::new(LayerPriority::POPUP, 20, 40);
        let lines = vec!["one".to_string(), "two".to_string()];
        let mut host = host_with(PluginFloat::new("T", lines).with_anchor_row(3));
        host.render_float_into_layer(&mut layer, None, None);
        assert_eq!(drawn_rows(&layer), vec![4, 5, 6, 7]);
        assert!(
            row_text(&layer, 4).contains(" T "),
            "title sits on the top border"
        );
        assert!(row_text(&layer, 5).contains("one"));
    }

    #[test]
    fn anchored_float_moves_above_cursor_row_when_no_room_below() {
        let mut layer = Layer::new(LayerPriority::POPUP, 20, 40);
        let lines = vec!["one".to_string(), "two".to_string()];
        let mut host = host_with(PluginFloat::new("T", lines).with_anchor_row(17));
        host.render_float_into_layer(&mut layer, None, None);
        // Ends at row 16, never touching the anchor row.
        assert_eq!(drawn_rows(&layer), vec![13, 14, 15, 16]);
    }

    #[test]
    fn anchored_float_clamps_and_scrolls_long_content() {
        let mut layer = Layer::new(LayerPriority::POPUP, 12, 40);
        let lines: Vec<String> = (0..20).map(|i| format!("line{i}")).collect();
        let mut host = host_with(PluginFloat::new("T", lines).with_anchor_row(2));
        host.render_float_into_layer(&mut layer, None, None);
        // Rows 3..=10: 8 rows = 6 content lines; the status row 11 stays clear.
        assert_eq!(drawn_rows(&layer), (3..=10).collect::<Vec<_>>());
        assert!(row_text(&layer, 3).contains("T [6/20]"));
        assert!(row_text(&layer, 4).contains("line0"));

        host.scroll_float(100);
        host.render_float_into_layer(&mut layer, None, None);
        assert!(row_text(&layer, 3).contains("T [20/20]"));
        assert!(row_text(&layer, 4).contains("line14"));

        host.scroll_float(-3);
        host.render_float_into_layer(&mut layer, None, None);
        assert!(row_text(&layer, 3).contains("T [17/20]"));
        assert!(row_text(&layer, 4).contains("line11"));
    }

    #[test]
    fn unanchored_float_stays_centered() {
        let mut layer = Layer::new(LayerPriority::POPUP, 20, 40);
        let lines = vec!["one".to_string()];
        let mut host = host_with(PluginFloat::new("T", lines));
        host.render_float_into_layer(&mut layer, None, None);
        assert_eq!(drawn_rows(&layer), vec![8, 9, 10]);
    }
}

#[cfg(test)]
mod identity_and_generation_tests {
    use super::*;

    #[test]
    fn plugin_id_monotonic_and_nonzero() {
        let id1 = PluginId::allocate();
        let id2 = PluginId::allocate();
        assert!(id1.get() > 0);
        assert!(id2.get() > id1.get());
        assert_eq!(id1.as_nonzero().get(), id1.get());
    }

    #[test]
    fn plugin_generation_monotonic_and_distinct() {
        let pid1 = PluginId::allocate();
        let pid2 = PluginId::allocate();
        let gen1 = PluginGeneration::allocate(pid1);
        let gen2 = PluginGeneration::allocate(pid1);
        let gen3 = PluginGeneration::allocate(pid2);

        assert_eq!(gen1.plugin_id(), pid1);
        assert_eq!(gen2.plugin_id(), pid1);
        assert_eq!(gen3.plugin_id(), pid2);

        assert!(gen2.generation() > gen1.generation());
        assert!(gen3.generation() > gen2.generation());
        assert_ne!(gen1, gen2);
        assert_ne!(gen2, gen3);

        let (raw_pid, raw_gen) = gen1.raw();
        assert_eq!(raw_pid, pid1.get());
        assert_eq!(raw_gen, gen1.generation().get());
    }

    #[test]
    fn host_plugin_registration_and_lookup() {
        let mut host = PluginHost::new(1);
        let id1 = host.register_plugin("git-blame");
        let id2 = host.register_plugin("git-blame");
        let id3 = host.register_plugin("markdown");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(host.plugin_id_for_name("git-blame"), Some(id1));
        assert_eq!(host.plugin_name(id1), Some("git-blame"));
        assert_eq!(host.plugin_name(id3), Some("markdown"));
    }

    #[test]
    fn host_generation_lifecycle_and_mutation_filtering() {
        let mut host = PluginHost::new(1);
        let pid = host.register_plugin("test-plugin");
        let gen = host.new_generation(pid);

        assert!(host.is_generation_active(gen));
        assert_eq!(host.generation_status(gen), Some(GenerationStatus::Active));

        // Native mutation is always queued
        host.queue_mutation(PluginMutation::CloseFloat);

        // Active generation mutation is accepted
        assert!(
            host.queue_mutation_with_origin(
                PluginMutation::InsertAtCursor("active".to_string()),
                gen,
            )
        );
        assert_eq!(host.queued_mutation_count(), 2);

        // Mark retiring
        assert!(host.mark_generation_retiring(gen));
        assert!(host.is_generation_retiring(gen));

        // New mutation from retiring generation is rejected
        assert!(!host.queue_mutation_with_origin(
            PluginMutation::InsertAtCursor("rejected".to_string()),
            gen,
        ));
        assert_eq!(host.queued_mutation_count(), 2);

        // Drain discards the retiring generation's mutation and preserves native
        let drained: Vec<PluginMutation> = host.drain_mutations().collect();
        assert_eq!(drained.len(), 1);
        assert!(matches!(drained[0], PluginMutation::CloseFloat));
    }

    #[test]
    fn discard_mutations_for_generation_explicitly() {
        let mut host = PluginHost::new(1);
        let pid1 = host.register_plugin("p1");
        let pid2 = host.register_plugin("p2");
        let gen1 = host.new_generation(pid1);
        let gen2 = host.new_generation(pid2);

        host.queue_mutation_with_origin(PluginMutation::CloseFloat, gen1);
        host.queue_mutation_with_origin(PluginMutation::SaveBuffer, gen2);
        host.queue_mutation(PluginMutation::SwapWindows);

        assert_eq!(host.queued_mutation_count(), 3);
        let discarded = host.discard_mutations_for_generation(gen1);
        assert_eq!(discarded, 1);
        assert_eq!(host.queued_mutation_count(), 2);

        let drained: Vec<PluginMutation> = host.drain_mutations().collect();
        assert_eq!(drained.len(), 2);
        assert!(matches!(drained[0], PluginMutation::SaveBuffer));
        assert!(matches!(drained[1], PluginMutation::SwapWindows));
    }

    #[test]
    fn retiring_generation_unregisters_handlers() {
        let mut host = PluginHost::new(1);
        let pid = host.register_plugin("p");
        let gen = host.new_generation(pid);

        host.register_command_with_generation("cmd", gen, |_| vec![PluginMutation::SwapWindows]);
        host.register_action_with_generation("act", gen, || vec![PluginMutation::SwapWindows]);

        assert!(host.has_command("cmd"));
        assert!(host.execute_command("cmd", &[]));
        assert!(host.execute_action("act"));
        assert_eq!(host.queued_mutation_count(), 2);

        // Retire the generation
        host.retire_generation(gen);
        assert!(host.is_generation_retired(gen));

        // Handlers are unregistered
        assert!(!host.has_command("cmd"));
        assert!(!host.execute_command("cmd", &[]));
        assert!(!host.execute_action("act"));
    }
}
