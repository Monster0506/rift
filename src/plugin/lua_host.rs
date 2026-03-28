//! Lua 5.4 plugin host — embeds mlua and exposes the `rift.*` API.

use std::sync::{Arc, Mutex};
use mlua::prelude::*;
use crate::notification::NotificationType;
use crate::plugin::{PluginFloat, PluginMutation};
use crate::plugin::events::EditorEvent;

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
    /// Current terminal dimensions (rows, cols).
    window_size: (u16, u16),
    can_undo: bool,
    can_redo: bool,
    is_dirty: bool,
    /// Current scroll position (top_line, left_col).
    scroll: (usize, usize),
    /// Current line ending: "lf" or "crlf".
    line_ending: String,
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
            window_size: (0, 0),
            can_undo: false,
            can_redo: false,
            is_dirty: false,
            scroll: (0, 0),
            line_ending: "lf".to_string(),
        }
    }
}

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
        // _rift_slot_events[slot_id] = event_name  (for rift.off lookup)
        lua.globals().set("_rift_handlers", lua.create_table()?)?;
        lua.globals().set("_rift_slot_events", lua.create_table()?)?;

        let api = lua.create_table()?;

        // rift.on(event_name, handler_fn) → handle (integer)
        // Returns a handle that can be passed to rift.off() to unregister.
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
                // Record slot → event_name for off() lookup
                let slot_events: LuaTable = lua.globals().get("_rift_slot_events")?;
                slot_events.set(slot_id as i64, event_name)?;
                Ok(slot_id as i64)
            })?;
            api.set("on", on_fn)?;
        }

        // rift.off(handle) — unregister a handler by its slot handle
        {
            let f = lua.create_function(|lua, handle: i64| {
                let slot_events: LuaTable = lua.globals().get("_rift_slot_events")?;
                let event_name: Option<String> = slot_events.get(handle)?;
                let event_name = match event_name {
                    Some(n) => n,
                    None => return Ok(()),
                };
                let handlers: LuaTable = lua.globals().get("_rift_handlers")?;
                let list: Option<LuaTable> = handlers.get(event_name.as_str())?;
                if let Some(list) = list {
                    let new_list = lua.create_table()?;
                    for entry in list.sequence_values::<LuaTable>() {
                        let entry = entry?;
                        let slot: i64 = entry.get("slot").unwrap_or(0);
                        if slot != handle {
                            new_list.push(entry)?;
                        }
                    }
                    handlers.set(event_name.as_str(), new_list)?;
                }
                slot_events.set(handle, LuaValue::Nil)?;
                Ok(())
            })?;
            api.set("off", f)?;
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

        // rift.register_command(name, fn [, description [, arg_type]])
        // description — shown in tab completion dropdown
        // arg_type    — drives argument completion: "file", "dir"
        {
            let f = lua.create_function(|lua, (name, callback, desc, arg_type): (String, LuaFunction, Option<String>, Option<String>)| {
                let cmds: LuaTable = lua.globals().get("_rift_commands")?;
                let entry = lua.create_table()?;
                entry.set("fn", callback)?;
                entry.set("description", desc.unwrap_or_default())?;
                if let Some(at) = arg_type {
                    entry.set("arg_type", at)?;
                }
                cmds.set(name, entry)?;
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

        // rift.emit(event_name [, payload])  — fire a UserEvent to all registered handlers
        // Optional payload table keys are merged into the event table alongside `name`.
        {
            let f = lua.create_function(|lua, (name, payload): (String, Option<LuaTable>)| {
                let handlers: LuaTable = lua.globals().get("_rift_handlers")?;
                let list: Option<LuaTable> = handlers.get("UserEvent")?;
                if let Some(list) = list {
                    let ev = lua.create_table()?;
                    ev.set("name", name.as_str())?;
                    if let Some(p) = payload {
                        for pair in p.pairs::<LuaValue, LuaValue>() {
                            let (k, v) = pair?;
                            ev.set(k, v)?;
                        }
                    }
                    for entry in list.sequence_values::<LuaTable>() {
                        let entry = entry?;
                        let f: LuaFunction = entry.get("fn")?;
                        f.call::<()>(ev.clone())?;
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

        // rift.set_cursor_hold_delay(ms) — set the CursorHold idle threshold in milliseconds
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ms: u32| {
                sh.lock().unwrap().mutations.push(PluginMutation::SetCursorHoldDelay(ms));
                Ok(())
            })?;
            api.set("set_cursor_hold_delay", f)?;
        }

        // rift.set_option(name, value) — set a document option
        // Supported: "tab_width", "expand_tabs", "show_line_numbers"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (name, value): (String, LuaValue)| {
                let value_str = match &value {
                    LuaValue::Boolean(b) => b.to_string(),
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

        // rift.get_line_count() → integer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap().buf_lines.len() as i64)
            })?;
            api.set("get_line_count", f)?;
        }

        // rift.can_undo() → bool
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| Ok(sh.lock().unwrap().can_undo))?;
            api.set("can_undo", f)?;
        }

        // rift.can_redo() → bool
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| Ok(sh.lock().unwrap().can_redo))?;
            api.set("can_redo", f)?;
        }

        // rift.is_dirty() → bool
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| Ok(sh.lock().unwrap().is_dirty))?;
            api.set("is_dirty", f)?;
        }

        // rift.get_scroll() → top_line, left_col
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.lock().unwrap();
                Ok((s.scroll.0 as i64, s.scroll.1 as i64))
            })?;
            api.set("get_scroll", f)?;
        }

        // rift.set_scroll(top_line, left_col)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (top, left): (usize, usize)| {
                sh.lock().unwrap().mutations.push(PluginMutation::SetScroll(top, left));
                Ok(())
            })?;
            api.set("set_scroll", f)?;
        }

        // rift.get_line_ending() → "lf" | "crlf"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| Ok(sh.lock().unwrap().line_ending.clone()))?;
            api.set("get_line_ending", f)?;
        }

        // rift.set_line_ending(type) — "lf" | "crlf"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ending: String| {
                sh.lock().unwrap().mutations.push(PluginMutation::SetLineEnding(ending));
                Ok(())
            })?;
            api.set("set_line_ending", f)?;
        }

        // rift.search(needle [, opts]) → array of {row, col_start, col_end}
        // Literal search over the current buffer lines.
        // row is 1-indexed; col_start/col_end are 0-indexed byte offsets within the line.
        // opts.whole_word = true  — only match when surrounded by non-word characters.
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, (needle, opts): (String, Option<LuaTable>)| {
                let whole_word = opts
                    .as_ref()
                    .and_then(|t| t.get::<bool>("whole_word").ok())
                    .unwrap_or(false);
                let lines = sh.lock().unwrap().buf_lines.clone();
                let results = lua.create_table()?;
                if needle.is_empty() {
                    return Ok(results);
                }
                for (row_idx, line) in lines.iter().enumerate() {
                    let bytes = line.as_bytes();
                    let mut start = 0;
                    while let Some(pos) = line[start..].find(needle.as_str()) {
                        let col_start = start + pos;
                        let col_end = col_start + needle.len();
                        if whole_word {
                            let before_ok = col_start == 0
                                || !bytes[col_start - 1].is_ascii_alphanumeric()
                                    && bytes[col_start - 1] != b'_';
                            let after_ok = col_end >= bytes.len()
                                || !bytes[col_end].is_ascii_alphanumeric()
                                    && bytes[col_end] != b'_';
                            if !before_ok || !after_ok {
                                start = col_end;
                                continue;
                            }
                        }
                        let entry = lua.create_table()?;
                        entry.set("row", row_idx + 1)?;
                        entry.set("col_start", col_start)?;
                        entry.set("col_end", col_end)?;
                        results.push(entry)?;
                        start = col_end;
                    }
                }
                Ok(results)
            })?;
            api.set("search", f)?;
        }

        // rift.get_window_size() → rows, cols
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.lock().unwrap();
                Ok((s.window_size.0 as i64, s.window_size.1 as i64))
            })?;
            api.set("get_window_size", f)?;
        }

        // rift.exec_action(action_string) — fire a built-in editor action by name
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, action: String| {
                sh.lock().unwrap().mutations.push(PluginMutation::ExecAction(action));
                Ok(())
            })?;
            api.set("exec_action", f)?;
        }

        // rift.map(mode, keys, action) — register a key binding
        // mode: "n" | "i" | "c" | "s" | "g"
        // keys: vim notation, e.g. "<C-p>", "gg", "<leader>s"
        // action: action string, e.g. "editor:save", or a registered plugin action id
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (mode, keys, action): (String, String, String)| {
                sh.lock().unwrap().mutations.push(PluginMutation::MapKey { mode, keys, action });
                Ok(())
            })?;
            api.set("map", f)?;
        }

        // rift.center_on_line(n) — move cursor to line n (1-indexed) and center it in viewport
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, row: usize| {
                sh.lock().unwrap().mutations.push(PluginMutation::CenterOnLine(row));
                Ok(())
            })?;
            api.set("center_on_line", f)?;
        }

        // rift.unmap(mode, keys) — remove a key binding
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (mode, keys): (String, String)| {
                sh.lock().unwrap().mutations.push(PluginMutation::UnmapKey { mode, keys });
                Ok(())
            })?;
            api.set("unmap", f)?;
        }

        lua.globals().set("rift", api)?;

        // Embedded Lua prelude — convenience wrappers that don't need Rust bindings.
        lua.load(r#"
-- rift.log level constants
rift.log = { DEBUG = "debug", INFO = "info", WARN = "warn", ERROR = "error" }

-- rift.get_current_line() → string
function rift.get_current_line()
    local row = select(1, rift.get_cursor())
    local lines = rift.get_lines(row, row)
    return lines[1] or ""
end

-- rift.set_current_line(text)
function rift.set_current_line(text)
    local row = select(1, rift.get_cursor())
    rift.replace_lines(row, row, {text})
end

-- rift.delete_current_line()
function rift.delete_current_line()
    local row = select(1, rift.get_cursor())
    rift.replace_lines(row, row, {})
end

-- rift.inspect(val) → string  (pretty-prints any Lua value)
function rift.inspect(val, _depth)
    local depth = _depth or 0
    local t = type(val)
    if t == "table" then
        if depth > 4 then return "{...}" end
        local parts = {}
        for k, v in pairs(val) do
            local ks = type(k) == "string" and k or ("[" .. tostring(k) .. "]")
            table.insert(parts, ks .. " = " .. rift.inspect(v, depth + 1))
        end
        return "{" .. table.concat(parts, ", ") .. "}"
    elseif t == "string" then
        return string.format("%q", val)
    else
        return tostring(val)
    end
end

-- rift.json — minimal JSON encode/decode
rift.json = {}
function rift.json.encode(val)
    local t = type(val)
    if t == "nil" then return "null"
    elseif t == "boolean" then return val and "true" or "false"
    elseif t == "number" then return tostring(val)
    elseif t == "string" then
        return '"' .. val:gsub('\\', '\\\\'):gsub('"', '\\"'):gsub('\n', '\\n'):gsub('\t', '\\t') .. '"'
    elseif t == "table" then
        local n = #val
        if n > 0 then
            local parts = {}
            for i = 1, n do parts[i] = rift.json.encode(val[i]) end
            return "[" .. table.concat(parts, ",") .. "]"
        else
            local parts = {}
            for k, v in pairs(val) do
                if type(k) == "string" then
                    table.insert(parts, rift.json.encode(k) .. ":" .. rift.json.encode(v))
                end
            end
            return "{" .. table.concat(parts, ",") .. "}"
        end
    end
    return "null"
end

-- rift.fs — path and file utilities
rift.fs = {}
function rift.fs.basename(path)
    return path:match("([^/\\]+)$") or path
end
function rift.fs.dirname(path)
    return path:match("^(.*)[/\\][^/\\]*$") or "."
end
function rift.fs.joinpath(...)
    return table.concat({...}, "/")
end
function rift.fs.exists(path)
    local f = io.open(path, "r")
    if f then f:close() return true end
    return false
end
function rift.fs.read(path)
    local f = io.open(path, "r")
    if not f then return nil end
    local content = f:read("*a")
    f:close()
    return content
end

-- rift.json.decode(str) → value
-- Minimal recursive descent JSON parser (no unicode escapes, no numbers in exponent form).
do
    local function skip_ws(s, i)
        while i <= #s and s:sub(i,i):match("%s") do i = i + 1 end
        return i
    end
    local parse_value  -- forward declaration

    local function parse_string(s, i)
        -- i points at opening quote
        i = i + 1
        local out = {}
        while i <= #s do
            local c = s:sub(i,i)
            if c == '"' then return table.concat(out), i + 1 end
            if c == '\\' then
                i = i + 1
                local e = s:sub(i,i)
                if     e == '"'  then out[#out+1] = '"'
                elseif e == '\\' then out[#out+1] = '\\'
                elseif e == '/'  then out[#out+1] = '/'
                elseif e == 'n'  then out[#out+1] = '\n'
                elseif e == 't'  then out[#out+1] = '\t'
                elseif e == 'r'  then out[#out+1] = '\r'
                else out[#out+1] = e end
            else
                out[#out+1] = c
            end
            i = i + 1
        end
        error("unterminated string")
    end

    local function parse_array(s, i)
        i = i + 1  -- skip '['
        local arr = {}
        i = skip_ws(s, i)
        if s:sub(i,i) == ']' then return arr, i + 1 end
        while true do
            local v; v, i = parse_value(s, i)
            arr[#arr+1] = v
            i = skip_ws(s, i)
            local c = s:sub(i,i)
            if c == ']' then return arr, i + 1 end
            if c ~= ',' then error("expected ',' or ']'") end
            i = i + 1
        end
    end

    local function parse_object(s, i)
        i = i + 1  -- skip '{'
        local obj = {}
        i = skip_ws(s, i)
        if s:sub(i,i) == '}' then return obj, i + 1 end
        while true do
            i = skip_ws(s, i)
            if s:sub(i,i) ~= '"' then error("expected string key") end
            local k; k, i = parse_string(s, i)
            i = skip_ws(s, i)
            if s:sub(i,i) ~= ':' then error("expected ':'") end
            i = i + 1
            local v; v, i = parse_value(s, i)
            obj[k] = v
            i = skip_ws(s, i)
            local c = s:sub(i,i)
            if c == '}' then return obj, i + 1 end
            if c ~= ',' then error("expected ',' or '}'") end
            i = i + 1
        end
    end

    parse_value = function(s, i)
        i = skip_ws(s, i)
        local c = s:sub(i,i)
        if c == '"' then return parse_string(s, i)
        elseif c == '[' then return parse_array(s, i)
        elseif c == '{' then return parse_object(s, i)
        elseif s:sub(i, i+3) == "true"  then return true,  i + 4
        elseif s:sub(i, i+4) == "false" then return false, i + 5
        elseif s:sub(i, i+3) == "null"  then return nil,   i + 4
        else
            -- number
            local num_str = s:match("^-?%d+%.?%d*", i)
            if num_str then return tonumber(num_str), i + #num_str end
            error("unexpected character: " .. c)
        end
    end

    function rift.json.decode(str)
        local ok, val = pcall(function() return (parse_value(str, 1)) end)
        if ok then return val end
        return nil, val  -- nil, error_message
    end
end

-- rift.get_word_at_cursor() → string
-- Returns the word (alphanumeric + underscore) under the cursor, or "" if none.
function rift.get_word_at_cursor()
    local row, col = rift.get_cursor()
    local line = rift.get_lines(row, row)[1] or ""
    -- col is 0-indexed byte offset; Lua strings are 1-indexed
    local pos = col + 1
    if pos > #line then pos = #line end
    if pos < 1 or not line:sub(pos, pos):match("[%w_]") then return "" end
    -- Walk left to start of word
    local s = pos
    while s > 1 and line:sub(s - 1, s - 1):match("[%w_]") do s = s - 1 end
    -- Walk right to end of word
    local e = pos
    while e < #line and line:sub(e + 1, e + 1):match("[%w_]") do e = e + 1 end
    return line:sub(s, e)
end

-- rift.debounce(fn, polls) → debounced_fn
-- Returns a wrapper that only calls fn after it has been invoked `polls`
-- consecutive times without being reset. Intended for use with
-- TextChangedCoarse handlers where you want to wait for a pause in typing.
-- `polls` maps to main-loop idle cycles (~16 ms each by default).
function rift.debounce(fn, polls)
    polls = polls or 10
    local count = 0
    return function(...)
        count = count + 1
        if count >= polls then
            count = 0
            fn(...)
        end
    end
end
"#).set_name("rift:prelude").exec()?;

        Ok(Self { lua, shared })
    }

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
        window_size: (u16, u16),
        can_undo: bool,
        can_redo: bool,
        is_dirty: bool,
        scroll: (usize, usize),
        line_ending: &str,
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
        s.window_size = window_size;
        s.can_undo = can_undo;
        s.can_redo = can_redo;
        s.is_dirty = is_dirty;
        s.scroll = scroll;
        s.line_ending = line_ending.to_string();
    }

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

    /// Execute a plugin command registered via `rift.register_command`.
    /// Returns `true` if a handler was found and called.
    pub fn execute_command(&self, name: &str, args: &[String]) -> bool {
        let cmds: LuaTable = match self.lua.globals().get("_rift_commands") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let entry: Option<LuaTable> = match cmds.get(name) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let entry = match entry {
            Some(e) => e,
            None => return false,
        };
        let handler: LuaFunction = match entry.get("fn") {
            Ok(f) => f,
            Err(_) => return false,
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

    /// Returns all Lua-registered command names, descriptions, and arg types.
    pub fn command_list(&self) -> Vec<(String, String, Option<String>)> {
        let cmds: LuaTable = match self.lua.globals().get("_rift_commands") {
            Ok(t) => t,
            Err(_) => return vec![],
        };
        let mut list = Vec::new();
        for pair in cmds.pairs::<String, LuaTable>() {
            if let Ok((name, entry)) = pair {
                let desc: String = entry.get("description").unwrap_or_default();
                let arg_type: Option<String> = entry.get("arg_type").ok().flatten();
                list.push((name, desc, arg_type));
            }
        }
        list
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

    /// Drain all mutations queued by Lua API calls.
    pub fn drain_mutations(&self) -> Vec<PluginMutation> {
        std::mem::take(&mut self.shared.lock().unwrap().mutations)
    }

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
        host.update_state(1, vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()], (0, 0), 4, true, "normal", None, None, vec![], (0, 0), false, false, false, (0, 0), "lf");
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
        host.update_state(1, vec![], (4, 2), 4, true, "normal", None, None, vec![], (0, 0), false, false, false, (0, 0), "lf");
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
        host.update_state(42, vec![], (0, 0), 4, true, "normal", None, None, vec![], (0, 0), false, false, false, (0, 0), "lf");
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

}
