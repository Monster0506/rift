//! Plugin system for Rift.
//!
//! This module implements the plugin infrastructure in two stages:
//!
//! - **Stage 1** (this file): The event bus. All editor events flow through
//!   [`PluginHost::dispatch`]. Handlers are registered as Rust closures for now;
//!   the Lua runtime will be wired in on top of this foundation.
//!
//! - **Stage 2** (future): Lua VM (`mlua`) + external-process RPC over stdio.
//!
//! ## Invariant
//!
//! The plugin host never mutates editor state directly. Mutations requested by
//! plugins are queued as [`PluginMutation`] values and applied by the main loop
//! via the normal `execute_command` path, preserving undo history and
//! dot-repeat.

pub mod events;
pub mod lua_host;

pub use events::EditorEvent;

use crate::document::DocumentId;
use crate::notification::NotificationType;
use std::sync::Arc;

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
    /// Add a foreground color highlight over a character range in the active buffer.
    /// Line numbers are 1-indexed; columns are 0-indexed.
    /// `color` is a named color ("red", "green", …) or an HTML hex string ("#rrggbb").
    /// `slot` identifies the plugin handler that owns this highlight.
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
    /// Set a per-document option. Supported names: `tab_width`, `expand_tabs`,
    /// `show_line_numbers`. Value is always a string ("4", "true", "false", …).
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
    /// Remove a key binding. `mode` and `keys` are the same as `MapKey`.
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
}

/// A floating window owned by a plugin. Stored in `PluginHost` and rendered
/// on each frame until `CloseFloat` is queued.
#[derive(Debug, Clone)]
pub struct PluginFloat {
    pub title: String,
    pub lines: Vec<String>,
}

impl PluginFloat {
    pub fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
        }
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

/// Central plugin coordinator. Owned by the `Editor`.
///
/// Responsibilities:
/// - Dispatch [`EditorEvent`]s to registered handlers
/// - Queue [`PluginMutation`]s returned by handlers
/// - Track cursor-hold idle state
/// - Hold registered commands and keymap actions
pub struct PluginHost {
    /// Handlers indexed by event name for O(1) lookup.
    handlers: std::collections::HashMap<&'static str, Vec<Handler>>,
    /// Registered `:command` handlers. Key is lowercase command name.
    commands: std::collections::HashMap<String, CommandHandler>,
    /// Optional one-line description for each registered command.
    command_descriptions: std::collections::HashMap<String, String>,
    /// Registered keymap action handlers. Key matches `EditorAction::PluginAction(id)`.
    actions: std::collections::HashMap<String, ActionHandler>,
    /// Currently open plugin float, if any.
    open_float: Option<PluginFloat>,
    /// Set to `true` when a float was just closed so the layer can be cleared once.
    float_just_closed: bool,
    /// Mutations queued by handlers during the current dispatch call.
    mutation_queue: Vec<PluginMutation>,
    /// Cursor-hold idle tracker.
    cursor_hold: CursorHoldState,
    /// Embedded Lua VM for script plugins. `None` until `init_lua()` is called.
    lua: Option<lua_host::LuaHost>,
}

impl PluginHost {
    /// Create a new plugin host.
    ///
    /// `cursor_hold_polls` — number of idle main-loop polls before
    /// `CursorHold` fires. At the default 16 ms poll rate, 25 polls ≈ 400 ms.
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
        }
    }

    /// Register a handler for a named event.
    ///
    /// `event_name` must match one of the strings returned by
    /// [`EditorEvent::name`], e.g. `"BufSavePost"`.
    ///
    /// # Example
    /// ```rust,ignore
    /// host.on("BufSavePost", |ev| {
    ///     if let EditorEvent::BufSavePost { path, .. } = ev {
    ///         eprintln!("saved: {}", path.display());
    ///     }
    /// });
    /// ```
    pub fn on<F>(&mut self, event_name: &'static str, handler: F)
    where
        F: Fn(&EditorEvent) + Send + 'static,
    {
        self.handlers
            .entry(event_name)
            .or_default()
            .push(Box::new(handler));
    }

    /// Register a handler for a `:CommandName [args...]` ex-command.
    ///
    /// `name` is case-insensitive. Returns the registered name in lowercase.
    pub fn register_command<F>(&mut self, name: &str, handler: F) -> String
    where
        F: Fn(&[String]) -> Vec<PluginMutation> + Send + 'static,
    {
        let key = name.to_lowercase();
        self.commands.insert(key.clone(), Box::new(handler));
        key
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
        if let Some(handler) = self.commands.get(&key) {
            let mutations = handler(args);
            for m in mutations {
                self.apply_mutation(m);
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
    ///
    /// Call with the same `id` string you pass to `EditorAction::PluginAction`.
    pub fn register_action<F>(&mut self, id: &str, handler: F)
    where
        F: Fn() -> Vec<PluginMutation> + Send + 'static,
    {
        self.actions.insert(id.to_string(), Box::new(handler));
    }

    /// Execute a registered plugin action, queuing any returned mutations.
    /// Returns `true` if a handler was found (Rust or Lua).
    pub fn execute_action(&mut self, id: &str) -> bool {
        if let Some(handler) = self.actions.get(id) {
            let mutations = handler();
            for m in mutations {
                self.apply_mutation(m);
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

    /// Render the open float (if any) into the given layer.
    ///
    /// `fg` and `bg` should be the editor's current theme colors so the float
    /// blends with the rest of the UI instead of relying on the terminal's
    /// default reverse-video colors.
    pub fn render_float_into_layer(
        &self,
        layer: &mut crate::layer::Layer,
        fg: Option<crate::color::Color>,
        bg: Option<crate::color::Color>,
    ) {
        use crate::floating_window::{FloatingWindow, WindowPosition, WindowStyle};

        let float = match &self.open_float {
            Some(f) => f,
            None => return,
        };

        let rows = layer.rows();
        let cols = layer.cols();

        // Size the window to fit content, bounded by terminal dimensions.
        let content_w = float
            .lines
            .iter()
            .map(|l| l.len())
            .max()
            .unwrap_or(20)
            .max(float.title.len() + 2)
            .min(cols.saturating_sub(4));
        let content_h = float.lines.len().min(rows.saturating_sub(4));
        let width = (content_w + 2).min(cols);
        let height = (content_h + 2).min(rows);

        let mut style = WindowStyle::default().with_reverse_video(false);
        if let Some(f) = fg {
            style = style.with_fg(f);
        }
        if let Some(b) = bg {
            style = style.with_bg(b);
        }

        let window = FloatingWindow::with_style(WindowPosition::Center, width, height, style);

        let char_lines: Vec<Vec<char>> = float
            .lines
            .iter()
            .take(content_h)
            .map(|l| l.chars().take(content_w).collect())
            .collect();

        window.render(layer, &char_lines);
    }

    /// Initialize the Lua VM. Must be called once at startup.
    /// Returns any error string if Lua initialization fails.
    pub fn init_lua(&mut self) -> Option<String> {
        if let Some(host) = self.lua.take() {
            drop(host)
        }

        match lua_host::LuaHost::new() {
            Ok(host) => {
                self.lua = Some(host);
                None
            }
            Err(e) => Some(format!("Failed to initialize Lua: {}", e)),
        }
    }

    /// Update the Lua VM's buffer snapshot before dispatching events.
    #[allow(clippy::too_many_arguments)]
    pub fn lua_update_state(
        &self,
        buf_id: usize,
        buf_kind: String,
        lines: Arc<Vec<String>>,
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
    ) {
        if let Some(lua) = &self.lua {
            lua.update_state(
                buf_id,
                buf_kind,
                lines,
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
            );
        }
    }

    /// Load all `.lua` files in the given directory. Returns error strings.
    pub fn lua_load_dir(&self, dir: &std::path::Path) -> Vec<String> {
        match &self.lua {
            Some(lua) => lua.load_dir(dir),
            None => vec![],
        }
    }

    /// Execute a Lua snippet directly (for `:lua` command).
    pub fn lua_exec(&self, code: &str) -> Option<String> {
        self.lua.as_ref()?.exec(code)
    }

    /// Dispatch an event to all registered handlers.
    ///
    /// Handlers run synchronously on the calling thread (the main loop).
    /// Any mutations they return are applied via [`apply_mutation`].
    pub fn dispatch(&mut self, event: &EditorEvent) {
        if let EditorEvent::CursorMoved { buf, row, col } = event {
            self.cursor_hold.on_cursor_move(*buf, *row, *col);
        }

        let name = event.name();
        if let Some(handlers) = self.handlers.get(name) {
            for handler in handlers {
                handler(event);
            }
        }

        // Dispatch to Lua handlers and convert any errors to notifications.
        if let Some(lua) = &self.lua {
            for err in lua.dispatch_event(event) {
                self.mutation_queue.push(PluginMutation::Notify {
                    message: err,
                    level: crate::notification::NotificationType::Error,
                });
            }
        }
    }

    /// Called on every idle frame (no input). Fires `CursorHold` if the cursor
    /// has been stationary long enough.
    ///
    /// Returns the event to dispatch, if any, so the caller can call
    /// `dispatch()` with it (avoiding a double-borrow).
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

    /// Queue a mutation to be applied by the main loop after dispatch returns.
    pub fn queue_mutation(&mut self, mutation: PluginMutation) {
        self.mutation_queue.push(mutation);
    }

    /// Apply a mutation immediately (used internally by command/action handlers).
    /// Float open/close mutations are applied directly to `open_float`; all
    /// others are queued for the main loop.
    pub fn apply_mutation(&mut self, mutation: PluginMutation) {
        match mutation {
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
            other => self.mutation_queue.push(other),
        }
    }

    /// Drain all queued mutations. Called by the main loop after every
    /// `dispatch` call.
    pub fn drain_mutations(&mut self) -> impl Iterator<Item = PluginMutation> + '_ {
        if let Some(lua) = &self.lua {
            self.mutation_queue.extend(lua.drain_mutations());
        }
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
            .finish()
    }
}
