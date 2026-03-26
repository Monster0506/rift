//! Lua 5.4 plugin host — embeds mlua and exposes the `rift.*` API.

use std::sync::{Arc, Mutex};
use mlua::prelude::*;
use crate::notification::NotificationType;
use crate::plugin::{PluginFloat, PluginMutation};
use crate::plugin::events::EditorEvent;

// ─── Shared state ─────────────────────────────────────────────────────────────

/// A lightweight buffer entry for the `rift.get_buf_list()` snapshot.
/// Fields: (id, display_name, is_dirty, is_current)
type BufEntry = (usize, String, bool, bool);

/// State shared between Lua API closures and the host.
/// Stored behind `Arc<Mutex>` so closures can be `'static`.
struct LuaSharedState {
    /// Mutations queued by Lua plugin calls; drained by `PluginHost`.
    mutations: Vec<PluginMutation>,
    /// Snapshot of the active buffer's text lines, updated before each dispatch.
    buf_lines: Vec<String>,
    /// Active buffer ID.
    buf_id: usize,
    /// Cursor position (row 0-indexed, col 0-indexed).
    cursor: (usize, usize),
    tab_width: usize,
    expand_tabs: bool,
    mode: String,
    /// Detected filetype of the active buffer, e.g. "rust", "python".
    filetype: Option<String>,
    /// Absolute path of the active buffer's file, if any.
    file_path: Option<String>,
    /// Snapshot of all open buffers: (id, display_name, is_dirty, is_current).
    buf_list: Vec<BufEntry>,
    /// Slot ID assigned to the handler currently being dispatched.
    /// Each `rift.on()` registration gets a unique stable slot so that
    /// `clear_highlights()` only affects that handler's highlights.
    current_slot: u32,
    /// Counter used to assign unique slot IDs when `rift.on()` is called.
    next_slot: u32,
}

impl Default for LuaSharedState {
    fn default() -> Self {
        Self {
            mutations: Vec::new(),
            buf_lines: Vec::new(),
            buf_id: 0,
            cursor: (0, 0),
            tab_width: 4,
            expand_tabs: true,
            mode: "normal".to_string(),
            filetype: None,
            file_path: None,
            buf_list: Vec::new(),
            current_slot: 0,
            next_slot: 1,
        }
    }
}

// ─── LuaHost ──────────────────────────────────────────────────────────────────

/// Owns the Lua VM and registers the `rift.*` API.
pub struct LuaHost {
    lua: Lua,
    shared: Arc<Mutex<LuaSharedState>>,
}

impl LuaHost {
    /// Create a new Lua VM and register the full `rift` API table.
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();
        let shared = Arc::new(Mutex::new(LuaSharedState::default()));

        // Internal handler registry table: _rift_handlers[event_name] = { {slot=n,fn=f}, ... }
        lua.globals().set("_rift_handlers", lua.create_table()?)?;

        let api = lua.create_table()?;

        // rift.on(event_name, handler_fn)
        // Each registration gets a unique `slot` id so that clear_highlights() only
        // affects the highlights owned by that particular handler.
        {
            let sh = Arc::clone(&shared);
            let on_fn = lua.create_function(move |lua, (event_name, callback): (String, LuaFunction)| {
                let slot_id = {
                    let mut s = sh.lock().unwrap();
                    let id = s.next_slot;
                    s.next_slot += 1;
                    id
                };
                let handlers: LuaTable = lua.globals().get("_rift_handlers")?;
                let list: Option<LuaTable> = handlers.get(event_name.as_str())?;
                let list = match list {
                    Some(t) => t,
                    None => {
                        let t = lua.create_table()?;
                        handlers.set(event_name.as_str(), t.clone())?;
                        t
                    }
                };
                let entry = lua.create_table()?;
                entry.set("slot", slot_id)?;
                entry.set("fn", callback)?;
                list.push(entry)?;
                Ok(())
            })?;
            api.set("on", on_fn)?;
        }

        // rift.notify(level, message)
        // level: "info" | "warn" | "error" | "success"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (level, message): (String, String)| {
                let level = match level.as_str() {
                    "warn" | "warning" => NotificationType::Warning,
                    "error" => NotificationType::Error,
                    "success" => NotificationType::Success,
                    _ => NotificationType::Info,
                };
                sh.lock().unwrap().mutations.push(PluginMutation::Notify { message, level });
                Ok(())
            })?;
            api.set("notify", f)?;
        }

        // rift.append_lines(lines)  — appends a Lua sequence of strings to the active buffer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, lines: LuaTable| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                sh.lock().unwrap().mutations.push(PluginMutation::AppendLines(v));
                Ok(())
            })?;
            api.set("append_lines", f)?;
        }

        // rift.open_float(title, lines)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (title, lines): (String, LuaTable)| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                sh.lock().unwrap().mutations.push(PluginMutation::OpenFloat(PluginFloat::new(title, v)));
                Ok(())
            })?;
            api.set("open_float", f)?;
        }

        // rift.close_float()
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                sh.lock().unwrap().mutations.push(PluginMutation::CloseFloat);
                Ok(())
            })?;
            api.set("close_float", f)?;
        }

        // rift.current_buf() → integer buffer id
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap().buf_id as i64)
            })?;
            api.set("current_buf", f)?;
        }

        // rift.get_cursor() → row (1-indexed), col (0-indexed)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.lock().unwrap();
                Ok((s.cursor.0 as i64 + 1, s.cursor.1 as i64))
            })?;
            api.set("get_cursor", f)?;
        }

        // rift.get_lines(start, end) → sequence of strings
        // start/end are 1-indexed; end = -1 means last line
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, (start, end_): (i64, i64)| {
                let s = sh.lock().unwrap();
                let len = s.buf_lines.len() as i64;
                let start = (start - 1).max(0) as usize;
                let end_ = if end_ < 0 { (len + end_ + 1).max(0) } else { end_.min(len) } as usize;
                let t = lua.create_table()?;
                for (i, line) in s.buf_lines[start..end_].iter().enumerate() {
                    t.set(i + 1, line.as_str())?;
                }
                Ok(t)
            })?;
            api.set("get_lines", f)?;
        }

        // rift.register_command(name, fn)  — register an ex-command handler
        {
            let f = lua.create_function(|lua, (name, callback): (String, LuaFunction)| {
                let cmds: LuaTable = lua.globals().get("_rift_commands")?;
                cmds.set(name, callback)?;
                Ok(())
            })?;
            api.set("register_command", f)?;
        }
        lua.globals().set("_rift_commands", lua.create_table()?)?;

        // rift.register_action(id, fn)  — register a keymap action handler
        {
            let f = lua.create_function(|lua, (id, callback): (String, LuaFunction)| {
                let actions: LuaTable = lua.globals().get("_rift_actions")?;
                actions.set(id, callback)?;
                Ok(())
            })?;
            api.set("register_action", f)?;
        }
        lua.globals().set("_rift_actions", lua.create_table()?)?;

        // rift.emit(event_name)  — fire a UserEvent to all registered handlers
        {
            let f = lua.create_function(|lua, name: String| {
                let handlers: LuaTable = lua.globals().get("_rift_handlers")?;
                let list: Option<LuaTable> = handlers.get("UserEvent")?;
                if let Some(list) = list {
                    let ev = lua.create_table()?;
                    ev.set("name", name.as_str())?;
                    for handler in list.sequence_values::<LuaFunction>() {
                        handler?.call::<()>(ev.clone())?;
                    }
                }
                Ok(())
            })?;
            api.set("emit", f)?;
        }

        // rift.insert(text) — insert text at the current cursor position
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, text: String| {
                sh.lock().unwrap().mutations.push(PluginMutation::InsertAtCursor(text));
                Ok(())
            })?;
            api.set("insert", f)?;
        }

        // rift.delete_before(n) — delete n chars immediately before the cursor
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, n: i64| {
                if n > 0 {
                    sh.lock().unwrap().mutations.push(PluginMutation::DeleteBefore(n as usize));
                }
                Ok(())
            })?;
            api.set("delete_before", f)?;
        }

        // rift.delete_forward(n) — delete n chars immediately after the cursor
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, n: i64| {
                if n > 0 {
                    sh.lock().unwrap().mutations.push(PluginMutation::DeleteForward(n as usize));
                }
                Ok(())
            })?;
            api.set("delete_forward", f)?;
        }

        // rift.set_cursor(row, col) — move cursor (row 1-indexed, col 0-indexed)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (row, col): (i64, i64)| {
                if row >= 1 {
                    sh.lock().unwrap().mutations.push(PluginMutation::SetCursor {
                        row: row as usize,
                        col: col.max(0) as usize,
                    });
                }
                Ok(())
            })?;
            api.set("set_cursor", f)?;
        }

        // rift.replace_lines(start, end, lines) — replace 1-indexed inclusive line range
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (start, end_, lines): (i64, i64, LuaTable)| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                if start >= 1 && end_ >= start {
                    sh.lock().unwrap().mutations.push(PluginMutation::ReplaceLines {
                        start: start as usize,
                        end: end_ as usize,
                        lines: v,
                    });
                }
                Ok(())
            })?;
            api.set("replace_lines", f)?;
        }

        // rift.add_highlight(start_line, start_col, end_line, end_col, color)
        // line numbers are 1-indexed; columns are 0-indexed
        // color: named ("red", "green", …) or hex ("#rrggbb")
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (sl, sc, el, ec, color): (i64, i64, i64, i64, String)| {
                if sl >= 1 && el >= sl {
                    let mut s = sh.lock().unwrap();
                    let slot = s.current_slot;
                    s.mutations.push(PluginMutation::AddHighlight {
                        slot,
                        start_line: sl as usize,
                        start_col: sc.max(0) as usize,
                        end_line: el as usize,
                        end_col: ec.max(0) as usize,
                        color,
                    });
                }
                Ok(())
            })?;
            api.set("add_highlight", f)?;
        }

        // rift.clear_highlights() — remove this handler's highlights from the active buffer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let mut s = sh.lock().unwrap();
                let slot = s.current_slot;
                s.mutations.push(PluginMutation::ClearHighlights { slot });
                Ok(())
            })?;
            api.set("clear_highlights", f)?;
        }

        // rift.set_option(name, value) — set a document option
        // Supported: "tab_width", "expand_tabs", "show_line_numbers"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (name, value): (String, LuaValue)| {
                let value_str = match &value {
                    LuaValue::Boolean(b) => if *b { "true".to_string() } else { "false".to_string() },
                    LuaValue::Integer(n) => n.to_string(),
                    LuaValue::Number(n) => (*n as i64).to_string(),
                    LuaValue::String(s) => s.to_str()?.to_string(),
                    _ => return Ok(()),
                };
                sh.lock().unwrap().mutations.push(PluginMutation::SetOption { name, value: value_str });
                Ok(())
            })?;
            api.set("set_option", f)?;
        }

        // rift.get_option(name) — read a document option from the current snapshot
        // Returns: tab_width (int), expand_tabs (bool), show_line_numbers (bool)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_lua, name: String| {
                let s = sh.lock().unwrap();
                match name.as_str() {
                    "tab_width" | "tabwidth" => Ok(LuaValue::Integer(s.tab_width as i64)),
                    "expand_tabs" | "expandtabs" => Ok(LuaValue::Boolean(s.expand_tabs)),
                    _ => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_option", f)?;
        }

        // rift.get_filetype() → string or nil
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.lock().unwrap();
                match &s.filetype {
                    Some(ft) => Ok(LuaValue::String(lua.create_string(ft)?)),
                    None => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_filetype", f)?;
        }

        // rift.get_filepath() → string or nil
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.lock().unwrap();
                match &s.file_path {
                    Some(p) => Ok(LuaValue::String(lua.create_string(p)?)),
                    None => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_filepath", f)?;
        }

        // rift.get_buf_list() → sequence of { id, name, is_dirty, is_current }
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.lock().unwrap();
                let result = lua.create_table()?;
                for (i, (id, name, is_dirty, is_current)) in s.buf_list.iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set("id", *id as i64)?;
                    entry.set("name", name.as_str())?;
                    entry.set("is_dirty", *is_dirty)?;
                    entry.set("is_current", *is_current)?;
                    result.set(i + 1, entry)?;
                }
                Ok(result)
            })?;
            api.set("get_buf_list", f)?;
        }

        // rift.save() — request a save of the active buffer to disk
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                sh.lock().unwrap().mutations.push(PluginMutation::SaveBuffer);
                Ok(())
            })?;
            api.set("save", f)?;
        }

        // rift.get_tab_width() → integer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap().tab_width as i64)
            })?;
            api.set("get_tab_width", f)?;
        }

        // rift.get_expand_tabs() → boolean
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap().expand_tabs)
            })?;
            api.set("get_expand_tabs", f)?;
        }

        // rift.get_mode() → "normal" | "insert" | "command" | "search"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap().mode.clone())
            })?;
            api.set("get_mode", f)?;
        }

        lua.globals().set("rift", api)?;

        Ok(Self { lua, shared })
    }

    // ── State snapshot ────────────────────────────────────────────────────────

    /// Update the buffer snapshot that `rift.get_lines()` and friends read.
    /// Call this before dispatching an event.
    #[allow(clippy::too_many_arguments)]
    pub fn update_state(
        &self,
        buf_id: usize,
        lines: Vec<String>,
        cursor: (usize, usize),
        tab_width: usize,
        expand_tabs: bool,
        mode: &str,
        filetype: Option<String>,
        file_path: Option<String>,
        buf_list: Vec<BufEntry>,
    ) {
        let mut s = self.shared.lock().unwrap();
        s.buf_id = buf_id;
        s.buf_lines = lines;
        s.cursor = cursor;
        s.tab_width = tab_width;
        s.expand_tabs = expand_tabs;
        s.mode = mode.to_string();
        s.filetype = filetype;
        s.file_path = file_path;
        s.buf_list = buf_list;
    }

    // ── Event dispatch ────────────────────────────────────────────────────────

    /// Dispatch an `EditorEvent` to all registered Lua handlers.
    /// Returns error strings for any handlers that raised a Lua error.
    pub fn dispatch_event(&self, event: &EditorEvent) -> Vec<String> {
        let handlers: LuaTable = match self.lua.globals().get("_rift_handlers") {
            Ok(t) => t,
            Err(_) => return vec![],
        };
        let list: Option<LuaTable> = match handlers.get(event.name()) {
            Ok(v) => v,
            Err(_) => return vec![],
        };
        let list = match list {
            Some(l) => l,
            None => return vec![],
        };
        let ev_table = match event.to_lua_table(&self.lua) {
            Ok(t) => t,
            Err(e) => return vec![format!("failed to build event table for {}: {}", event.name(), e)],
        };

        let mut errors = Vec::new();
        for entry in list.sequence_values::<LuaTable>() {
            match entry {
                Ok(entry) => {
                    let slot_id: u32 = entry.get("slot").unwrap_or(0);
                    let f: LuaFunction = match entry.get("fn") {
                        Ok(f) => f,
                        Err(e) => {
                            errors.push(format!("[lua:{}] bad handler entry: {}", event.name(), e));
                            continue;
                        }
                    };
                    // Set current_slot before the handler runs so that
                    // clear_highlights()/add_highlight() tag mutations with the right slot.
                    {
                        self.shared.lock().unwrap().current_slot = slot_id;
                    }
                    if let Err(e) = f.call::<()>(ev_table.clone()) {
                        errors.push(format!("[lua:{}] {}", event.name(), e));
                    }
                }
                Err(e) => errors.push(format!("[lua:{}] bad handler: {}", event.name(), e)),
            }
        }
        errors
    }

    // ── Lua command/action dispatch ───────────────────────────────────────────

    /// Execute a plugin command registered via `rift.register_command`.
    /// Returns `true` if a handler was found and called.
    pub fn execute_command(&self, name: &str, args: &[String]) -> bool {
        let cmds: LuaTable = match self.lua.globals().get("_rift_commands") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let handler: Option<LuaFunction> = match cmds.get(name) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let handler = match handler {
            Some(f) => f,
            None => return false,
        };
        let args_table = match self.lua.create_table() {
            Ok(t) => t,
            Err(_) => return false,
        };
        for (i, arg) in args.iter().enumerate() {
            let _ = args_table.set(i + 1, arg.as_str());
        }
        if let Err(e) = handler.call::<()>(args_table) {
            self.shared.lock().unwrap().mutations.push(PluginMutation::Notify {
                message: format!("[lua:{}] {}", name, e),
                level: NotificationType::Error,
            });
        }
        true
    }

    /// Execute a plugin action registered via `rift.register_action`.
    /// Returns `true` if a handler was found and called.
    pub fn execute_action(&self, id: &str) -> bool {
        let actions: LuaTable = match self.lua.globals().get("_rift_actions") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let handler: Option<LuaFunction> = match actions.get(id) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let handler = match handler {
            Some(f) => f,
            None => return false,
        };
        if let Err(e) = handler.call::<()>(()) {
            self.shared.lock().unwrap().mutations.push(PluginMutation::Notify {
                message: format!("[lua:{}] {}", id, e),
                level: NotificationType::Error,
            });
        }
        true
    }

    // ── Plugin loading ────────────────────────────────────────────────────────

    /// Load all `.lua` files in `dir`, and set `package.path` to include it.
    /// Returns a list of error strings (empty means all loaded OK).
    pub fn load_dir(&self, dir: &std::path::Path) -> Vec<String> {
        let mut errors = Vec::new();

        // Extend package.path so require() can find modules in this directory.
        let dir_str = dir.to_string_lossy().replace('\\', "/");
        let set_path = format!(
            "package.path = package.path .. ';{d}/?.lua;{d}/?/init.lua'",
            d = dir_str
        );
        if let Err(e) = self.lua.load(set_path.as_str()).exec() {
            errors.push(format!("lua: failed to set package.path: {}", e));
        }

        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return errors, // directory doesn't exist — silently skip
        };

        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "lua") {
                let src = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        errors.push(format!("lua: failed to read {}: {}", path.display(), e));
                        continue;
                    }
                };
                let name = path.to_string_lossy().replace('\\', "/");
                if let Err(e) = self.lua.load(src.as_str()).set_name(name.as_str()).exec() {
                    errors.push(format!("lua: {}: {}", path.display(), e));
                }
            }
        }

        errors
    }

    // ── Mutation drain ────────────────────────────────────────────────────────

    /// Drain all mutations queued by Lua API calls.
    pub fn drain_mutations(&self) -> Vec<PluginMutation> {
        std::mem::take(&mut self.shared.lock().unwrap().mutations)
    }

    // ── Direct Lua execution (for :lua command) ───────────────────────────────

    /// Execute a string of Lua code. Returns an error string on failure.
    pub fn exec(&self, code: &str) -> Option<String> {
        self.lua.load(code).exec().err().map(|e| format!("[lua] {}", e))
    }
}

impl std::fmt::Debug for LuaHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaHost").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::events::EditorEvent;
    use crate::notification::NotificationType;

    fn make_host() -> LuaHost {
        LuaHost::new().expect("LuaHost::new failed")
    }

    #[test]
    fn test_new_succeeds() {
        let _ = make_host();
    }

    #[test]
    fn test_notify_queues_mutation() {
        let host = make_host();
        assert!(host.exec("rift.notify('info', 'hello')").is_none());
        let mutations = host.drain_mutations();
        assert_eq!(mutations.len(), 1);
        match &mutations[0] {
            PluginMutation::Notify { message, level } => {
                assert_eq!(message, "hello");
                assert_eq!(*level, NotificationType::Info);
            }
            _ => panic!("expected Notify"),
        }
    }

    #[test]
    fn test_append_lines_queues_mutation() {
        let host = make_host();
        assert!(host.exec("rift.append_lines({'line1', 'line2'})").is_none());
        let mutations = host.drain_mutations();
        assert_eq!(mutations.len(), 1);
        match &mutations[0] {
            PluginMutation::AppendLines(lines) => {
                assert_eq!(lines, &vec!["line1".to_string(), "line2".to_string()]);
            }
            _ => panic!("expected AppendLines"),
        }
    }

    #[test]
    fn test_open_float_queues_mutation() {
        let host = make_host();
        assert!(host.exec("rift.open_float('My Float', {'line a', 'line b'})").is_none());
        let mutations = host.drain_mutations();
        assert_eq!(mutations.len(), 1);
        match &mutations[0] {
            PluginMutation::OpenFloat(f) => {
                assert_eq!(f.title, "My Float");
                assert_eq!(f.lines, vec!["line a", "line b"]);
            }
            _ => panic!("expected OpenFloat"),
        }
    }

    #[test]
    fn test_close_float_queues_mutation() {
        let host = make_host();
        assert!(host.exec("rift.close_float()").is_none());
        let mutations = host.drain_mutations();
        assert_eq!(mutations.len(), 1);
        assert!(matches!(mutations[0], PluginMutation::CloseFloat));
    }

    #[test]
    fn test_on_and_dispatch_event() {
        let host = make_host();
        assert!(host.exec(
            "rift.on('EditorStart', function(_ev) rift.notify('info', 'started') end)"
        ).is_none());
        let errors = host.dispatch_event(&EditorEvent::EditorStart);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let mutations = host.drain_mutations();
        assert_eq!(mutations.len(), 1);
        match &mutations[0] {
            PluginMutation::Notify { message, .. } => assert_eq!(message, "started"),
            _ => panic!("expected Notify"),
        }
    }

    #[test]
    fn test_get_lines_returns_correct_lines() {
        let host = make_host();
        host.update_state(1, vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()], (0, 0), 4, true, "normal", None, None, vec![]);
        assert!(host.exec("_lines = rift.get_lines(1, -1)").is_none());
        assert!(host.exec("rift.notify('info', _lines[2])").is_none());
        let mutations = host.drain_mutations();
        match &mutations[0] {
            PluginMutation::Notify { message, .. } => assert_eq!(message, "beta"),
            _ => panic!("expected Notify"),
        }
    }

    #[test]
    fn test_get_cursor_returns_1indexed_row() {
        let host = make_host();
        host.update_state(1, vec![], (4, 2), 4, true, "normal", None, None, vec![]);
        assert!(host.exec("local r, c = rift.get_cursor(); rift.notify('info', tostring(r))").is_none());
        let mutations = host.drain_mutations();
        match &mutations[0] {
            PluginMutation::Notify { message, .. } => assert_eq!(message, "5"),
            _ => panic!("expected Notify"),
        }
    }

    #[test]
    fn test_current_buf_returns_id() {
        let host = make_host();
        host.update_state(42, vec![], (0, 0), 4, true, "normal", None, None, vec![]);
        assert!(host.exec("rift.notify('info', tostring(rift.current_buf()))").is_none());
        let mutations = host.drain_mutations();
        match &mutations[0] {
            PluginMutation::Notify { message, .. } => assert_eq!(message, "42"),
            _ => panic!("expected Notify"),
        }
    }

    #[test]
    fn test_exec_returns_none_on_success() {
        let host = make_host();
        assert!(host.exec("local x = 1 + 1").is_none());
    }

    #[test]
    fn test_exec_returns_some_on_bad_lua() {
        let host = make_host();
        let err = host.exec("this is not valid lua @@@@");
        assert!(err.is_some(), "expected Some(err) for bad Lua");
    }

    #[test]
    fn test_load_dir_loads_lua_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir failed");
        let plugin_path = dir.path().join("test_plugin.lua");
        let mut f = std::fs::File::create(&plugin_path).expect("create failed");
        writeln!(f, "rift.notify('info', 'plugin loaded')").expect("write failed");
        drop(f);

        let host = make_host();
        let errors = host.load_dir(dir.path());
        assert!(errors.is_empty(), "load errors: {:?}", errors);
        let mutations = host.drain_mutations();
        assert_eq!(mutations.len(), 1);
        match &mutations[0] {
            PluginMutation::Notify { message, .. } => assert_eq!(message, "plugin loaded"),
            _ => panic!("expected Notify"),
        }
    }

    #[test]
    fn test_error_in_handler_returned_from_dispatch() {
        let host = make_host();
        assert!(host.exec(
            "rift.on('EditorStart', function(_ev) error('handler error') end)"
        ).is_none());
        let errors = host.dispatch_event(&EditorEvent::EditorStart);
        assert!(!errors.is_empty(), "expected errors from bad handler");
        assert!(errors[0].contains("handler error"), "error: {}", errors[0]);
    }

    #[test]
    fn test_insert_queues_mutation() {
        let host = make_host();
        assert!(host.exec("rift.insert('hello')").is_none());
        let mutations = host.drain_mutations();
        assert_eq!(mutations.len(), 1);
        match &mutations[0] {
            PluginMutation::InsertAtCursor(text) => assert_eq!(text, "hello"),
            _ => panic!("expected InsertAtCursor"),
        }
    }

    #[test]
    fn test_get_tab_width_default() {
        let host = make_host();
        assert!(host.exec("assert(rift.get_tab_width() == 4)").is_none());
    }

    #[test]
    fn test_get_expand_tabs_default() {
        let host = make_host();
        assert!(host.exec("assert(rift.get_expand_tabs() == true)").is_none());
    }

    #[test]
    fn test_get_mode_default() {
        let host = make_host();
        assert!(host.exec("assert(rift.get_mode() == 'normal')").is_none());
    }

    // ── autoindent.lua integration tests ────────────────────────────────────

    fn load_autoindent() -> LuaHost {
        let host = make_host();
        let plugin_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("runtime/plugins/autoindent.lua");
        let code = std::fs::read_to_string(&plugin_path)
            .unwrap_or_else(|_| panic!("could not read {:?}", plugin_path));
        let err = host.exec(&code);
        assert!(err.is_none(), "autoindent.lua load error: {:?}", err);
        host
    }

    /// Simulate pressing Enter on a line, which causes:
    ///   1. A new blank line at `new_row` (1-indexed), cursor at col 0
    ///   2. Line count increased by 1
    /// Returns drained mutations after the TextChangedCoarse event.
    fn press_enter(
        host: &LuaHost,
        lines_before: Vec<&str>,
        new_row: usize,   // 1-indexed row of the new blank line
    ) -> Vec<PluginMutation> {
        // Prime ai.prev_line_count by firing BufEnter with the original buffer.
        let orig: Vec<String> = lines_before.iter().map(|s| s.to_string()).collect();
        host.update_state(1, orig, (0, 0), 4, true, "insert", None, None, vec![]);
        let _ = host.dispatch_event(&EditorEvent::BufEnter { buf: 1 });
        host.drain_mutations(); // discard any priming mutations

        // After Enter the buffer has an extra blank line inserted at new_row.
        let mut lines_after: Vec<String> = lines_before.iter().map(|s| s.to_string()).collect();
        lines_after.insert(new_row - 1, String::new());

        // Cursor is at the new blank line, col 0.  row is 0-indexed internally.
        let cursor_row = new_row - 1;
        host.update_state(1, lines_after, (cursor_row, 0), 4, true, "insert", None, None, vec![]);
        let errors = host.dispatch_event(&EditorEvent::TextChangedCoarse { buf: 1 });
        assert!(errors.is_empty(), "handler errors: {:?}", errors);
        host.drain_mutations()
    }

    #[test]
    fn test_autoindent_copies_base_indent() {
        let host = load_autoindent();
        // Line 1: "    hello" (4 spaces).  Enter pressed → new line 2.
        let mutations = press_enter(&host, vec!["    hello"], 2);
        let insert = mutations.iter().find_map(|m| match m {
            PluginMutation::InsertAtCursor(s) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(insert.as_deref(), Some("    "), "expected 4-space indent");
    }

    #[test]
    fn test_autoindent_increases_after_opener() {
        let host = load_autoindent();
        // Line 1 ends with `{` — indent should be one level deeper.
        let opener_line = "fn foo() {";
        let mutations = press_enter(&host, vec![opener_line], 2);
        let insert = mutations.iter().find_map(|m| match m {
            PluginMutation::InsertAtCursor(s) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(insert.as_deref(), Some("    "), "expected one indent level after opener");
    }

    #[test]
    fn test_autoindent_nested_indent() {
        let host = load_autoindent();
        // Line 1: "    if x {" — base indent 4 spaces + opener → 8 spaces.
        let nested_opener = "    if x {";
        let mutations = press_enter(&host, vec![nested_opener], 2);
        let insert = mutations.iter().find_map(|m| match m {
            PluginMutation::InsertAtCursor(s) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(insert.as_deref(), Some("        "), "expected 8-space indent");
    }

    #[test]
    fn test_autoindent_no_indent_on_blank_prev_line() {
        let host = load_autoindent();
        // Previous line is blank — indent is empty, so no InsertAtCursor should fire.
        let mutations = press_enter(&host, vec![""], 2);
        let insert = mutations.iter().find_map(|m| match m {
            PluginMutation::InsertAtCursor(s) => Some(s.clone()),
            _ => None,
        });
        assert!(insert.is_none(), "no indent expected for blank prev line");
    }

    #[test]
    fn test_autoindent_not_fired_outside_insert_mode() {
        let host = load_autoindent();
        // Simulate what press_enter does, but in normal mode.
        let lines = vec!["    hello".to_string(), String::new()];
        host.update_state(1, lines, (1, 0), 4, true, "normal", None, None, vec![]);
        let errors = host.dispatch_event(&EditorEvent::TextChangedCoarse { buf: 1 });
        assert!(errors.is_empty());
        let mutations = host.drain_mutations();
        let insert = mutations.iter().find_map(|m| match m {
            PluginMutation::InsertAtCursor(s) => Some(s.clone()),
            _ => None,
        });
        assert!(insert.is_none(), "should not indent in normal mode");
    }

    #[test]
    fn test_autoindent_uses_tab_width_setting() {
        let host = load_autoindent();
        // tab_width = 2, expand_tabs = true
        let opener = "fn foo() {";
        // Prime prev_line_count with the single-line buffer.
        host.update_state(1, vec![opener.to_string()], (0, 0), 2, true, "insert", None, None, vec![]);
        let _ = host.dispatch_event(&EditorEvent::BufEnter { buf: 1 });
        host.drain_mutations();
        // Now simulate Enter: two lines, cursor on the new blank line.
        let lines = vec![opener.to_string(), String::new()];
        host.update_state(1, lines, (1, 0), 2, true, "insert", None, None, vec![]);
        let errors = host.dispatch_event(&EditorEvent::TextChangedCoarse { buf: 1 });
        assert!(errors.is_empty());
        let mutations = host.drain_mutations();
        let insert = mutations.iter().find_map(|m| match m {
            PluginMutation::InsertAtCursor(s) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(insert.as_deref(), Some("  "), "expected 2-space indent with tab_width=2");
    }
}
