//! Lua 5.4 plugin host - embeds mlua and exposes the `rift.*` API.

use crate::notification::NotificationType;
use crate::plugin::events::EditorEvent;
use crate::plugin::{PluginFloat, PluginMutation};
use mlua::prelude::*;
use std::sync::{Arc, Mutex};

/// Parse a `{ fg, bg, bold, italic, underline, strike, reverse }` style subtable.
fn parse_style(st: &LuaTable) -> LuaResult<crate::annotations::StyleOverride> {
    let color = |key: &str| -> LuaResult<Option<crate::color::Color>> {
        let s: Option<String> = st.get(key)?;
        Ok(s.and_then(|s| crate::color::Color::parse(&s)))
    };
    Ok(crate::annotations::StyleOverride {
        fg: color("fg")?,
        bg: color("bg")?,
        bold: st.get("bold").unwrap_or(false),
        italic: st.get("italic").unwrap_or(false),
        underline: st.get("underline").unwrap_or(false),
        strike: st.get("strike").unwrap_or(false),
        reverse: st.get("reverse").unwrap_or(false),
    })
}

/// Build an optional `Presentation` from an annotation options table's
/// `face` / `style` / `adornment` / `priority` keys (rift.annotations.add).
fn build_presentation(opts: &LuaTable) -> LuaResult<Option<crate::annotations::Presentation>> {
    use crate::annotations::{Adornment, FaceRef, Placement, Presentation};

    let face: Option<String> = opts.get("face")?;
    let style_tbl: Option<LuaTable> = opts.get("style")?;
    let adorn_tbl: Option<LuaTable> = opts.get("adornment")?;
    let priority = opts.get::<Option<i64>>("priority")?.unwrap_or(0) as i32;

    if face.is_none() && style_tbl.is_none() && adorn_tbl.is_none() && priority == 0 {
        return Ok(None);
    }

    let mut pres = Presentation {
        priority,
        ..Default::default()
    };
    if let Some(f) = face {
        pres.face = Some(FaceRef::new(f));
    }
    if let Some(st) = style_tbl {
        pres.style = Some(parse_style(&st)?);
    }
    if let Some(ad) = adorn_tbl {
        let text: String = ad.get::<Option<String>>("text")?.unwrap_or_default();
        let placement = match ad.get::<Option<String>>("placement")?.as_deref() {
            Some("leading") => Placement::Leading,
            Some("overlay") => Placement::Overlay,
            Some("conceal") => Placement::Conceal,
            _ => Placement::Trailing,
        };
        let mut adornment = Adornment::new(text, placement);
        if let Some(af) = ad.get::<Option<String>>("face")? {
            adornment = adornment.with_face(FaceRef::new(af));
        }
        if let Some(ast) = ad.get::<Option<LuaTable>>("style")? {
            adornment = adornment.with_style(parse_style(&ast)?);
        }
        pres.adornment = Some(adornment);
    }
    Ok(Some(pres))
}

fn lua_to_json(v: LuaValue) -> Result<serde_json::Value, String> {
    match v {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        LuaValue::Integer(n) => Ok(serde_json::Value::Number(n.into())),
        LuaValue::Number(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| format!("non-finite float {f}")),
        LuaValue::String(s) => Ok(serde_json::Value::String(s.to_string_lossy().to_string())),
        LuaValue::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                let arr: Result<Vec<_>, _> = (1..=len)
                    .map(|i| lua_to_json(t.raw_get::<LuaValue>(i).map_err(|e| e.to_string())?))
                    .collect();
                Ok(serde_json::Value::Array(arr?))
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<LuaValue, LuaValue>() {
                    let (k, val) = pair.map_err(|e| e.to_string())?;
                    let key = match k {
                        LuaValue::String(s) => s.to_string_lossy().to_string(),
                        LuaValue::Integer(n) => n.to_string(),
                        _ => continue,
                    };
                    map.insert(key, lua_to_json(val)?);
                }
                Ok(serde_json::Value::Object(map))
            }
        }
        _ => Err("unsupported Lua type".to_string()),
    }
}

/// A lightweight buffer entry for the `rift.get_buf_list()` snapshot.
#[derive(Debug, Clone)]
pub struct BufEntry {
    pub id: usize,
    pub name: String,
    pub is_dirty: bool,
    pub is_current: bool,
    /// Kind string: "file", "terminal", "directory", "undotree", "messages", etc.
    pub kind: String,
    /// Absolute path if the buffer has one.
    pub path: Option<String>,
    /// Number of lines in the buffer.
    pub line_count: usize,
    pub is_read_only: bool,
}

/// A read-only view of one annotation for the `rift.annotations` query snapshot.
#[derive(Debug, Clone)]
pub struct AnnotationView {
    pub id: u64,
    pub kind: String,
    pub owner: String,
    /// "point", "range", or "line".
    pub anchor: &'static str,
    /// Byte offset (point/range start) or line number (line anchor).
    pub start: usize,
    /// Range end byte offset; equals `start` for point/line anchors.
    pub end: usize,
    pub payload: crate::annotations::Value,
    pub visible: bool,
    pub interactive: bool,
}

/// Build the Lua table a query function hands back for one annotation.
fn annotation_view_to_table(lua: &Lua, v: &AnnotationView) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("id", v.id)?;
    t.set("kind", v.kind.as_str())?;
    t.set("owner", v.owner.as_str())?;
    t.set("anchor", v.anchor)?;
    t.set("start", v.start as i64)?;
    t.set("end", v.end as i64)?;
    t.set("visible", v.visible)?;
    t.set("interactive", v.interactive)?;
    if let Ok(p) = v.payload.clone().into_lua(lua) {
        t.set("payload", p)?;
    }
    Ok(t)
}

/// Deferred source for the active buffer's lines: `update_state` stores a cheap
/// buffer clone, and the getters materialize `Vec<String>` only when Lua reads.
pub struct BufLinesSource {
    pub revision: u64,
    pub line_count: usize,
    pub buffer: crate::buffer::TextBuffer,
}

/// State shared between Lua API closures and the host.
/// Stored behind `Arc<Mutex>` so closures can be `'static`.
struct LuaSharedState {
    /// Mutations queued by Lua plugin calls; drained by `PluginHost`.
    mutations: Vec<PluginMutation>,
    /// Snapshot of the active document's annotations, refreshed before dispatch.
    annotations: Arc<Vec<AnnotationView>>,
    /// The store's next id at snapshot time; `add{}` pre-claims ids from here.
    next_annotation_id: u64,
    /// Deferred source of the active buffer's lines, set before each dispatch.
    buf_source: Option<BufLinesSource>,
    /// Lines materialized from `buf_source` on first read, with the revision
    /// they were built at so an unchanged buffer reuses them.
    buf_lines_cache: Arc<Vec<String>>,
    buf_lines_cache_rev: Option<u64>,
    /// Active buffer ID.
    buf_id: usize,
    /// Kind string for the active buffer ("file", "terminal", "directory", ...).
    buf_kind: String,
    /// Cursor position (row 0-indexed, col 0-indexed).
    cursor: (usize, usize),
    tab_width: usize,
    expand_tabs: bool,
    mode: String,
    /// Detected filetype of the active buffer, e.g. "rust", "python".
    filetype: Option<String>,
    /// Absolute path of the active buffer's file, if any.
    file_path: Option<String>,
    /// Snapshot of all open buffers with rich metadata.
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
    /// Snapshot of registered plugin commands: (name, description).
    commands: Vec<(String, String)>,
    /// Snapshot of all split windows with layout info, updated before each dispatch.
    win_list: Vec<WinEntry>,
    /// ID of the currently focused window.
    focused_win_id: u64,
    /// ID of the previously focused window, if any.
    previous_win_id: Option<u64>,
    /// Snapshot of LSP diagnostics: normalized_uri -> [(line, col, severity, message)].
    /// severity: 1=error, 2=warning, 3=info, 4=hint
    lsp_diagnostics: std::collections::HashMap<String, Vec<(u32, u32, u32, String)>>,
    /// The plugin file currently being loaded, used to tag registrations with their owner.
    /// Set to the file path before executing each plugin file; cleared after.
    current_plugin: Option<String>,
    /// Completed shell commands waiting to be fired as Lua UserEvents.
    /// Each entry is (tag, success, output). Drained in `drain_mutations`.
    pending_shell_events: Vec<(String, bool, String)>,
}

/// A lightweight entry for the `rift.windows.list()` snapshot.
#[derive(Debug, Clone)]
pub struct WinEntry {
    pub id: u64,
    pub buf: usize,
    pub row: usize,
    pub col: usize,
    pub rows: usize,
    pub cols: usize,
}

impl LuaSharedState {
    /// Materialize (and cache) the active buffer's lines, rebuilding only when
    /// the source revision differs from the cached one.
    fn lines(&mut self) -> Arc<Vec<String>> {
        if let Some(src) = &self.buf_source {
            if self.buf_lines_cache_rev != Some(src.revision) {
                let text = src.buffer.to_string();
                self.buf_lines_cache = Arc::new(
                    text.split('\n')
                        .map(|l| l.trim_end_matches('\r').to_string())
                        .collect(),
                );
                self.buf_lines_cache_rev = Some(src.revision);
            }
        }
        self.buf_lines_cache.clone()
    }

    /// Line count without materializing the lines (equals `lines().len()`).
    fn line_count(&self) -> usize {
        self.buf_source.as_ref().map_or(0, |s| s.line_count)
    }
}

impl Default for LuaSharedState {
    fn default() -> Self {
        Self {
            mutations: Vec::new(),
            annotations: Arc::new(vec![]),
            next_annotation_id: 1,
            buf_source: None,
            buf_lines_cache: Arc::new(vec![]),
            buf_lines_cache_rev: None,
            buf_id: 0,
            buf_kind: "file".to_string(),
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
            commands: Vec::new(),
            win_list: Vec::new(),
            focused_win_id: 0,
            previous_win_id: None,
            lsp_diagnostics: std::collections::HashMap::new(),
            current_plugin: None,
            pending_shell_events: Vec::new(),
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
        // plugins are fully trusted user code loaded from the local filesystem, but this unsafe block annoys me greatly
        let lua = unsafe { Lua::unsafe_new() };
        let shared = Arc::new(Mutex::new(LuaSharedState::default()));

        // os.execute() blocks the calling thread on a subprocess; route through
        // rift.spawn_shell instead, which runs off-thread and reports via a UserEvent.
        {
            let os_table: LuaTable = lua.globals().get("os")?;
            let err_fn = lua.create_function(|_, _: LuaMultiValue| -> LuaResult<()> {
                Err(LuaError::RuntimeError(
                    "os.execute() is disabled; use rift.spawn_shell(cmd, tag) or rift.spawn(prog, args, tag) instead"
                        .to_string(),
                ))
            })?;
            os_table.set("execute", err_fn)?;
        }

        // Internal handler registry table: _rift_handlers[event_name] = { {slot=n,fn=f}, ... }
        // _rift_slot_events[slot_id] = event_name  (for rift.off lookup)
        // _rift_slot_plugin[slot_id] = plugin_name (ownership tracking)
        // _rift_plugin_slots[plugin_name] = [slot_ids]  (reverse index for unload)
        // _rift_plugin_keymaps[plugin_name] = [{mode, keys}] (keymap ownership)
        lua.globals().set("_rift_handlers", lua.create_table()?)?;
        lua.globals()
            .set("_rift_slot_events", lua.create_table()?)?;
        lua.globals()
            .set("_rift_slot_plugin", lua.create_table()?)?;
        lua.globals()
            .set("_rift_plugin_slots", lua.create_table()?)?;
        lua.globals()
            .set("_rift_plugin_keymaps", lua.create_table()?)?;
        // _rift_action_handlers["<kind>\0<verb>"] = fn(ctx)  (annotation actions)
        lua.globals()
            .set("_rift_action_handlers", lua.create_table()?)?;
        // _rift_enter_handlers[kind] / _rift_leave_handlers[kind] = fn(ctx)
        // (cursor enters/leaves an annotation of that kind, design.md sec 12).
        lua.globals()
            .set("_rift_enter_handlers", lua.create_table()?)?;
        lua.globals()
            .set("_rift_leave_handlers", lua.create_table()?)?;

        let api = lua.create_table()?;

        // rift.on(event_name, handler_fn) -> handle (integer)
        // Returns a handle that can be passed to rift.off() to unregister.
        {
            let sh = Arc::clone(&shared);
            let on_fn =
                lua.create_function(move |lua, (event_name, callback): (String, LuaFunction)| {
                    let (slot_id, plugin_name) = {
                        let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
                        let id = s.next_slot;
                        s.next_slot += 1;
                        (id, s.current_plugin.clone())
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
                    if let Some(ref name) = plugin_name {
                        entry.set("plugin", name.as_str())?;
                    }
                    list.push(entry)?;
                    let slot_events: LuaTable = lua.globals().get("_rift_slot_events")?;
                    slot_events.set(slot_id as i64, event_name.clone())?;
                    if let Some(ref name) = plugin_name {
                        let slot_plugin: LuaTable = lua.globals().get("_rift_slot_plugin")?;
                        slot_plugin.set(slot_id as i64, name.as_str())?;
                        let plugin_slots: LuaTable = lua.globals().get("_rift_plugin_slots")?;
                        let slots_list: Option<LuaTable> = plugin_slots.get(name.as_str())?;
                        let slots_list = match slots_list {
                            Some(t) => t,
                            None => {
                                let t = lua.create_table()?;
                                plugin_slots.set(name.as_str(), t.clone())?;
                                t
                            }
                        };
                        slots_list.push(slot_id as i64)?;
                    }
                    Ok(slot_id as i64)
                })?;
            api.set("on", on_fn)?;
        }

        // rift.off(handle) - unregister a handler by its slot handle
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
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::Notify { message, level });
                Ok(())
            })?;
            api.set("notify", f)?;
        }

        // rift.append_lines(lines)  - appends a Lua sequence of strings to the active buffer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, lines: LuaTable| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::AppendLines(v));
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
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::OpenFloat(PluginFloat::new(title, v)));
                Ok(())
            })?;
            api.set("open_float", f)?;
        }

        // rift.close_float()
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::CloseFloat);
                Ok(())
            })?;
            api.set("close_float", f)?;
        }

        // rift.annotations.* - author and handle interactive annotations
        {
            let annotations = lua.create_table()?;

            // rift.annotations.add{ kind, line|point|range, payload, face, actions }
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, opts: LuaTable| {
                    let kind: String = opts.get("kind")?;
                    let line: Option<i64> = opts.get("line")?;
                    let point: Option<i64> = opts.get("point")?;
                    let range: Option<LuaTable> = opts.get("range")?;
                    let anchor = if let Some(l) = line {
                        crate::plugin::AnnotationAnchorSpec::Line(l.max(0) as usize)
                    } else if let Some(p) = point {
                        crate::plugin::AnnotationAnchorSpec::Point(p.max(0) as usize)
                    } else if let Some(r) = range {
                        let s: i64 = r.get(1)?;
                        let e: i64 = r.get(2)?;
                        crate::plugin::AnnotationAnchorSpec::Range(
                            s.max(0) as usize,
                            e.max(0) as usize,
                        )
                    } else {
                        return Err(mlua::Error::RuntimeError(
                            "annotation needs line, point, or range".into(),
                        ));
                    };
                    let payload = match opts.get::<LuaValue>("payload")? {
                        LuaValue::Table(t) => crate::annotations::Value::from_lua_table(&t)?,
                        _ => crate::annotations::Value::Null,
                    };
                    let presentation = build_presentation(&opts)?;
                    let mut actions = Vec::new();
                    if let Some(acts) = opts.get::<Option<LuaTable>>("actions")? {
                        for a in acts.sequence_values::<LuaTable>() {
                            let a = a?;
                            let verb: String = a.get("verb")?;
                            let default: bool = a.get("default").unwrap_or(false);
                            actions.push((verb, default));
                        }
                    }
                    let visible: bool = opts.get::<Option<bool>>("visible")?.unwrap_or(true);
                    let stickiness: Option<String> = opts.get("stickiness")?;
                    let owner: Option<String> = opts.get("owner")?;
                    // Pre-claim the id so add{} can return it synchronously.
                    let id = {
                        let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
                        let id = s.next_annotation_id;
                        s.next_annotation_id += 1;
                        s.mutations.push(PluginMutation::AddAnnotation {
                            id,
                            kind,
                            anchor,
                            payload,
                            presentation,
                            actions,
                            visible,
                            stickiness,
                            owner,
                        });
                        id
                    };
                    Ok(id)
                })?;
                annotations.set("add", f)?;
            }

            // rift.annotations.remove(id)
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, id: u64| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::RemoveAnnotation(id));
                    Ok(())
                })?;
                annotations.set("remove", f)?;
            }

            // rift.annotations.update(id, { payload=, visible=, style/face/... }):
            // unset fields are left as-is; presentation rebuilds only if any given.
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, (id, opts): (u64, LuaTable)| {
                    let payload = match opts.get::<LuaValue>("payload")? {
                        LuaValue::Table(t) => Some(crate::annotations::Value::from_lua_table(&t)?),
                        _ => None,
                    };
                    let visible: Option<bool> = opts.get("visible")?;
                    let presentation = build_presentation(&opts)?;
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::UpdateAnnotation {
                            id,
                            payload,
                            visible,
                            presentation,
                        },
                    );
                    Ok(())
                })?;
                annotations.set("update", f)?;
            }

            // Queries read the pre-dispatch snapshot (like rift.get_lines), so they
            // do not reflect adds queued earlier in the same handler.

            // rift.annotations.get(id) -> table | nil
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |lua, id: u64| {
                    let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                    match s.annotations.iter().find(|v| v.id == id) {
                        Some(v) => Ok(Some(annotation_view_to_table(lua, v)?)),
                        None => Ok(None),
                    }
                })?;
                annotations.set("get", f)?;
            }

            // rift.annotations.at(byte_offset) -> array of tables
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |lua, offset: i64| {
                    let off = offset.max(0) as usize;
                    let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                    let out = lua.create_table()?;
                    let mut i = 1;
                    for v in s.annotations.iter() {
                        let hit = match v.anchor {
                            "point" => v.start == off,
                            "range" => v.start <= off && off < v.end,
                            _ => false,
                        };
                        if hit {
                            out.set(i, annotation_view_to_table(lua, v)?)?;
                            i += 1;
                        }
                    }
                    Ok(out)
                })?;
                annotations.set("at", f)?;
            }

            // rift.annotations.in_range(start, end) -> array of tables
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |lua, (start, end_): (i64, i64)| {
                    let s0 = start.max(0) as usize;
                    let e0 = (end_.max(start)) as usize;
                    let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                    let out = lua.create_table()?;
                    let mut i = 1;
                    for v in s.annotations.iter() {
                        let hit = match v.anchor {
                            "point" => v.start >= s0 && v.start < e0,
                            "range" => v.start < e0 && v.end > s0,
                            _ => false,
                        };
                        if hit {
                            out.set(i, annotation_view_to_table(lua, v)?)?;
                            i += 1;
                        }
                    }
                    Ok(out)
                })?;
                annotations.set("in_range", f)?;
            }

            // rift.annotations.by_kind(prefix) -> array of tables (kind starts_with)
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |lua, prefix: String| {
                    let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                    let out = lua.create_table()?;
                    let mut i = 1;
                    for v in s.annotations.iter().filter(|v| v.kind.starts_with(&prefix)) {
                        out.set(i, annotation_view_to_table(lua, v)?)?;
                        i += 1;
                    }
                    Ok(out)
                })?;
                annotations.set("by_kind", f)?;
            }

            // rift.annotations.clear(kind_prefix) - drop every annotation whose
            // kind starts with the prefix (e.g. a plugin clearing its own "md.").
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, prefix: String| {
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::ClearAnnotations {
                            kind_prefix: prefix,
                        },
                    );
                    Ok(())
                })?;
                annotations.set("clear", f)?;
            }

            // rift.annotations.on_action(kind, verb, fn(ctx))
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(
                    move |lua, (kind, verb, handler): (String, String, mlua::Function)| {
                        let key = format!("{}\u{0}{}", kind, verb);
                        let handlers: LuaTable = lua.globals().get("_rift_action_handlers")?;
                        handlers.set(key, handler)?;
                        sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                            PluginMutation::RegisterAnnotationAction {
                                kind,
                                verb,
                                command: None,
                            },
                        );
                        Ok(())
                    },
                )?;
                annotations.set("on_action", f)?;
            }

            // rift.annotations.bind_command(kind, verb, ex_command)
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(
                    move |_, (kind, verb, command): (String, String, String)| {
                        sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                            PluginMutation::RegisterAnnotationAction {
                                kind,
                                verb,
                                command: Some(command),
                            },
                        );
                        Ok(())
                    },
                )?;
                annotations.set("bind_command", f)?;
            }

            // rift.annotations.register_kind(kind, { face/style/adornment,
            // description }) - per-kind render/hover defaults (sec 4).
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, (kind, opts): (String, LuaTable)| {
                    let presentation = build_presentation(&opts)?;
                    let description: Option<String> = opts.get("description").ok().flatten();
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::RegisterKindDefaults {
                            kind,
                            presentation,
                            description,
                        },
                    );
                    Ok(())
                })?;
                annotations.set("register_kind", f)?;
            }

            // rift.annotations.on_enter(kind, fn(ctx)) / on_leave(kind, fn(ctx)):
            // fire when the cursor enters/leaves an annotation of `kind` (sec 12).
            for (method, table_name) in [
                ("on_enter", "_rift_enter_handlers"),
                ("on_leave", "_rift_leave_handlers"),
            ] {
                let f =
                    lua.create_function(move |lua, (kind, handler): (String, mlua::Function)| {
                        let handlers: LuaTable = lua.globals().get(table_name)?;
                        handlers.set(kind, handler)?;
                        Ok(())
                    })?;
                annotations.set(method, f)?;
            }

            api.set("annotations", annotations)?;
        }

        // rift.current_buf() -> integer buffer id
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).buf_id as i64)
            })?;
            api.set("current_buf", f)?;
        }

        // rift.get_cursor() -> row (1-indexed), col (0-indexed)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                Ok((s.cursor.0 as i64 + 1, s.cursor.1 as i64))
            })?;
            api.set("get_cursor", f)?;
        }

        // rift.get_lines(start, end) -> sequence of strings
        // start/end are 1-indexed; end = -1 means last line
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, (start, end_): (i64, i64)| {
                let lines = sh.lock().unwrap_or_else(|e| e.into_inner()).lines();
                let len = lines.len() as i64;
                let start = (start - 1).max(0) as usize;
                let end_ = if end_ < 0 {
                    (len + end_ + 1).max(0)
                } else {
                    end_.min(len)
                } as usize;
                let t = lua.create_table()?;
                for (i, line) in lines[start..end_].iter().enumerate() {
                    t.set(i + 1, line.as_str())?;
                }
                Ok(t)
            })?;
            api.set("get_lines", f)?;
        }

        // rift.register_command(name, fn [, description [, arg_type]])
        // description - shown in tab completion dropdown
        // arg_type    - drives argument completion: "file", "dir"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(
                move |lua,
                      (name, callback, desc, arg_type): (
                    String,
                    LuaFunction,
                    Option<String>,
                    Option<String>,
                )| {
                    let plugin_name = sh
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .current_plugin
                        .clone();
                    let cmds: LuaTable = lua.globals().get("_rift_commands")?;
                    let entry = lua.create_table()?;
                    entry.set("fn", callback)?;
                    entry.set("description", desc.unwrap_or_default())?;
                    if let Some(at) = arg_type {
                        entry.set("arg_type", at)?;
                    }
                    if let Some(name_str) = plugin_name {
                        entry.set("plugin", name_str)?;
                    }
                    cmds.set(name, entry)?;
                    Ok(())
                },
            )?;
            api.set("register_command", f)?;
        }
        lua.globals().set("_rift_commands", lua.create_table()?)?;

        // rift.register_action(id, fn)  - register a keymap action handler
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, (id, callback): (String, LuaFunction)| {
                let plugin_name = sh
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .current_plugin
                    .clone();
                let actions: LuaTable = lua.globals().get("_rift_actions")?;
                let entry = lua.create_table()?;
                entry.set("fn", callback)?;
                if let Some(name) = plugin_name {
                    entry.set("plugin", name)?;
                }
                actions.set(id, entry)?;
                Ok(())
            })?;
            api.set("register_action", f)?;
        }
        lua.globals().set("_rift_actions", lua.create_table()?)?;

        // rift.emit(event_name [, payload])  - fire a UserEvent to all registered handlers
        // Optional payload table keys are merged into the event table alongside `name`.
        {
            let sh = Arc::clone(&shared);
            let f =
                lua.create_function(move |lua, (name, payload): (String, Option<LuaTable>)| {
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
                        // Snapshot the handler list before dispatch so handlers that
                        // call rift.on/rift.off mid-pass only affect the next emit.
                        let snapshot: Vec<LuaTable> = list
                            .sequence_values::<LuaTable>()
                            .collect::<LuaResult<_>>()?;
                        for entry in snapshot {
                            let f: LuaFunction = entry.get("fn")?;
                            let slot_id: u32 = entry.get("slot").unwrap_or(0);
                            // Tag this handler's slot, then restore the caller's slot
                            // afterward so reentrant emit() doesn't corrupt ownership.
                            let prev_slot = {
                                let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
                                let prev = s.current_slot;
                                s.current_slot = slot_id;
                                prev
                            };
                            let result = f.call::<()>(ev.clone());
                            sh.lock().unwrap_or_else(|e| e.into_inner()).current_slot = prev_slot;
                            result?;
                        }
                    }
                    Ok(())
                })?;
            api.set("emit", f)?;
        }

        // rift.spawn_shell(cmd, tag)
        // Run a shell command asynchronously. When it completes, a UserEvent
        // with name="ShellDone", tag=<tag>, success=<bool>, output=<string>
        // is fired on all registered UserEvent handlers.
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (cmd, tag): (String, String)| {
                let sh = Arc::clone(&sh);
                std::thread::spawn(move || {
                    let result = if cfg!(windows) {
                        std::process::Command::new("cmd")
                            .args(["/C", &cmd])
                            .output()
                    } else {
                        std::process::Command::new("sh").args(["-c", &cmd]).output()
                    };
                    let (success, out_text) = match result {
                        Ok(o) => {
                            let text = String::from_utf8_lossy(&o.stdout).to_string()
                                + &String::from_utf8_lossy(&o.stderr);
                            (o.status.success(), text.trim_end().to_string())
                        }
                        Err(e) => (false, e.to_string()),
                    };
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .pending_shell_events
                        .push((tag, success, out_text));
                });
                Ok(())
            })?;
            api.set("spawn_shell", f)?;
        }

        // rift.spawn(prog, args, tag): run a program directly (no shell), one
        // OS arg per list element. Fires the same ShellDone UserEvent as spawn_shell.
        {
            let sh = Arc::clone(&shared);
            let f =
                lua.create_function(move |_, (prog, args, tag): (String, Vec<String>, String)| {
                    let sh = Arc::clone(&sh);
                    std::thread::spawn(move || {
                        let result = std::process::Command::new(&prog).args(&args).output();
                        let (success, out_text) = match result {
                            Ok(o) => {
                                let text = String::from_utf8_lossy(&o.stdout).to_string()
                                    + &String::from_utf8_lossy(&o.stderr);
                                (o.status.success(), text.trim_end().to_string())
                            }
                            Err(e) => (false, e.to_string()),
                        };
                        sh.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .pending_shell_events
                            .push((tag, success, out_text));
                    });
                    Ok(())
                })?;
            api.set("spawn", f)?;
        }

        // rift.insert(text) - insert text at the current cursor position
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, text: String| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::InsertAtCursor(text));
                Ok(())
            })?;
            api.set("insert", f)?;
        }

        // rift.delete_before(n) - delete n chars immediately before the cursor
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, n: i64| {
                if n > 0 {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::DeleteBefore(n as usize));
                }
                Ok(())
            })?;
            api.set("delete_before", f)?;
        }

        // rift.delete_forward(n) - delete n chars immediately after the cursor
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, n: i64| {
                if n > 0 {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::DeleteForward(n as usize));
                }
                Ok(())
            })?;
            api.set("delete_forward", f)?;
        }

        // rift.set_cursor(row, col) - move cursor (row 1-indexed, col 0-indexed)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (row, col): (i64, i64)| {
                if row >= 1 {
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::SetCursor {
                            row: row as usize,
                            col: col.max(0) as usize,
                        },
                    );
                }
                Ok(())
            })?;
            api.set("set_cursor", f)?;
        }

        // rift.replace_lines(start, end, lines) - replace 1-indexed inclusive line range
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (start, end_, lines): (i64, i64, LuaTable)| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                if start >= 1 && end_ >= start {
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::ReplaceLines {
                            start: start as usize,
                            end: end_ as usize,
                            lines: v,
                        },
                    );
                }
                Ok(())
            })?;
            api.set("replace_lines", f)?;
        }

        // rift.add_highlight(start_line, start_col, end_line, end_col, color)
        // line numbers are 1-indexed; columns are 0-indexed
        // color: named ("red", "green", ...) or hex ("#rrggbb")
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(
                move |_, (sl, sc, el, ec, color): (i64, i64, i64, i64, String)| {
                    if sl >= 1 && el >= sl {
                        let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
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
                },
            )?;
            api.set("add_highlight", f)?;
        }

        // rift.clear_highlights() - remove this handler's highlights from the active buffer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
                let slot = s.current_slot;
                s.mutations.push(PluginMutation::ClearHighlights { slot });
                Ok(())
            })?;
            api.set("clear_highlights", f)?;
        }

        // rift.set_cursor_hold_delay(ms) - set the CursorHold idle threshold in milliseconds
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ms: u32| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::SetCursorHoldDelay(ms));
                Ok(())
            })?;
            api.set("set_cursor_hold_delay", f)?;
        }

        // rift.set_option(name, value) - set a document option
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
                sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                    PluginMutation::SetOption {
                        name,
                        value: value_str,
                    },
                );
                Ok(())
            })?;
            api.set("set_option", f)?;
        }

        // rift.get_option(name) - read a document option from the current snapshot
        // Returns: tab_width (int), expand_tabs (bool), show_line_numbers (bool)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_lua, name: String| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                match name.as_str() {
                    "tab_width" | "tabwidth" => Ok(LuaValue::Integer(s.tab_width as i64)),
                    "expand_tabs" | "expandtabs" => Ok(LuaValue::Boolean(s.expand_tabs)),
                    _ => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_option", f)?;
        }

        // rift.get_filetype() -> string or nil
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                match &s.filetype {
                    Some(ft) => Ok(LuaValue::String(lua.create_string(ft)?)),
                    None => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_filetype", f)?;
        }

        // rift.get_filepath() -> string or nil
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                match &s.file_path {
                    Some(p) => Ok(LuaValue::String(lua.create_string(p)?)),
                    None => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_filepath", f)?;
        }

        // rift.get_buf_list() -> sequence of { id, name, is_dirty, is_current, kind, path, line_count, is_read_only }
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                let result = lua.create_table()?;
                for (i, b) in s.buf_list.iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set("id", b.id as i64)?;
                    entry.set("name", b.name.as_str())?;
                    entry.set("is_dirty", b.is_dirty)?;
                    entry.set("is_current", b.is_current)?;
                    entry.set("kind", b.kind.as_str())?;
                    entry.set("line_count", b.line_count as i64)?;
                    entry.set("is_read_only", b.is_read_only)?;
                    if let Some(ref p) = b.path {
                        entry.set("path", p.as_str())?;
                    }
                    result.set(i + 1, entry)?;
                }
                Ok(result)
            })?;
            api.set("get_buf_list", f)?;
        }

        // rift.buf_kind() -> "file" | "terminal" | "directory" | "undotree" | "messages" | ,
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .buf_kind
                    .clone())
            })?;
            api.set("buf_kind", f)?;
        }

        // rift.switch_buf(buf_id) - switch the active buffer to the given ID
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, id: u64| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::SwitchToBuffer(id));
                Ok(())
            })?;
            api.set("switch_buf", f)?;
        }

        // rift.open_file(path [, force]) - open a file, optionally discarding unsaved changes
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (path, force): (String, Option<bool>)| {
                sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                    PluginMutation::OpenFile {
                        path,
                        force: force.unwrap_or(false),
                    },
                );
                Ok(())
            })?;
            api.set("open_file", f)?;
        }

        // rift.close_buf([force]) - close the current buffer, optionally discarding changes
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, force: Option<bool>| {
                sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                    PluginMutation::CloseBuffer {
                        force: force.unwrap_or(false),
                    },
                );
                Ok(())
            })?;
            api.set("close_buf", f)?;
        }

        // rift.create_scratch_buf(name, lines) - create an in-memory buffer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (name, lines): (String, LuaTable)| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::CreateScratchBuf { name, lines: v });
                Ok(())
            })?;
            api.set("create_scratch_buf", f)?;
        }

        // rift.reload_buf([force]) - reload the active buffer's content from disk, discarding in-memory edits if `force`.
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, force: Option<bool>| {
                sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                    PluginMutation::ReloadBuffer {
                        force: force.unwrap_or(false),
                    },
                );
                Ok(())
            })?;
            api.set("reload_buf", f)?;
        }

        // rift.get_commands() -> sequence of { name, description }
        // Returns all registered plugin commands (both Rust and Lua).
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                let result = lua.create_table()?;
                for (i, (name, desc)) in s.commands.iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set("name", name.as_str())?;
                    entry.set("description", desc.as_str())?;
                    result.set(i + 1, entry)?;
                }
                Ok(result)
            })?;
            api.set("get_commands", f)?;
        }

        // rift.save() - request a save of the active buffer to disk
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::SaveBuffer);
                Ok(())
            })?;
            api.set("save", f)?;
        }

        // rift.get_tab_width() -> integer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).tab_width as i64)
            })?;
            api.set("get_tab_width", f)?;
        }

        // rift.get_expand_tabs() -> boolean
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).expand_tabs)
            })?;
            api.set("get_expand_tabs", f)?;
        }

        // rift.get_mode() -> "normal" | "insert" | "command" | "search"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).mode.clone())
            })?;
            api.set("get_mode", f)?;
        }

        // rift.get_line_count() -> integer
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).line_count() as i64)
            })?;
            api.set("get_line_count", f)?;
        }

        // rift.can_undo() -> bool
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).can_undo)
            })?;
            api.set("can_undo", f)?;
        }

        // rift.can_redo() -> bool
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).can_redo)
            })?;
            api.set("can_redo", f)?;
        }

        // rift.is_dirty() -> bool
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).is_dirty)
            })?;
            api.set("is_dirty", f)?;
        }

        // rift.get_scroll() -> top_line, left_col
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                Ok((s.scroll.0 as i64, s.scroll.1 as i64))
            })?;
            api.set("get_scroll", f)?;
        }

        // rift.set_scroll(top_line, left_col)
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (top, left): (usize, usize)| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::SetScroll(top, left));
                Ok(())
            })?;
            api.set("set_scroll", f)?;
        }

        // rift.get_line_ending() -> "lf" | "crlf"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                Ok(sh
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .line_ending
                    .clone())
            })?;
            api.set("get_line_ending", f)?;
        }

        // rift.set_line_ending(type) - "lf" | "crlf"
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ending: String| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::SetLineEnding(ending));
                Ok(())
            })?;
            api.set("set_line_ending", f)?;
        }

        // rift.search(needle [, opts]) -> array of {row, col_start, col_end}
        // Literal search over the current buffer lines.
        // row is 1-indexed; col_start/col_end are 0-indexed byte offsets within the line.
        // opts.whole_word = true  - only match when surrounded by non-word characters.
        {
            let sh = Arc::clone(&shared);
            let f =
                lua.create_function(move |lua, (needle, opts): (String, Option<LuaTable>)| {
                    let whole_word = opts
                        .as_ref()
                        .and_then(|t| t.get::<bool>("whole_word").ok())
                        .unwrap_or(false);
                    let lines = sh.lock().unwrap_or_else(|e| e.into_inner()).lines();
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

        // rift.get_window_size() -> rows, cols
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                Ok((s.window_size.0 as i64, s.window_size.1 as i64))
            })?;
            api.set("get_window_size", f)?;
        }

        // rift.exec_action(action_string) - fire a built-in editor action by name
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, action: String| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::ExecAction(action));
                Ok(())
            })?;
            api.set("exec_action", f)?;
        }

        // rift.map(mode, keys, action) - register a key binding
        // mode: "n" | "i" | "c" | "s" | "g"
        // keys: vim notation, e.g. "<C-p>", "gg", "<leader>s"
        // action: action string, e.g. "editor:save", or a registered plugin action id
        {
            let sh = Arc::clone(&shared);
            let f =
                lua.create_function(move |lua, (mode, keys, action): (String, String, String)| {
                    let plugin_name = sh
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .current_plugin
                        .clone();
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::MapKey {
                            mode: mode.clone(),
                            keys: keys.clone(),
                            action,
                        },
                    );
                    if let Some(ref name) = plugin_name {
                        let plugin_keymaps: LuaTable = lua.globals().get("_rift_plugin_keymaps")?;
                        let list: Option<LuaTable> = plugin_keymaps.get(name.as_str())?;
                        let list = match list {
                            Some(t) => t,
                            None => {
                                let t = lua.create_table()?;
                                plugin_keymaps.set(name.as_str(), t.clone())?;
                                t
                            }
                        };
                        let entry = lua.create_table()?;
                        entry.set("mode", mode)?;
                        entry.set("keys", keys)?;
                        list.push(entry)?;
                    }
                    Ok(())
                })?;
            api.set("map", f)?;
        }

        // rift.center_on_line(n) - move cursor to line n (1-indexed) and center it in viewport
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, row: usize| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::CenterOnLine(row));
                Ok(())
            })?;
            api.set("center_on_line", f)?;
        }

        // rift.unmap(mode, keys) - remove a key binding
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (mode, keys): (String, String)| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::UnmapKey { mode, keys });
                Ok(())
            })?;
            api.set("unmap", f)?;
        }

        // rift.windows - window management sub-table
        {
            let windows = lua.create_table()?;

            // rift.windows.move(direction) - move focused window in a direction
            // direction: "left" | "right" | "up" | "down"
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, dir: String| {
                    use crate::split::navigation::Direction;
                    let direction = match dir.as_str() {
                        "left" => Direction::Left,
                        "right" => Direction::Right,
                        "up" => Direction::Up,
                        "down" => Direction::Down,
                        _ => {
                            return Err(mlua::Error::RuntimeError(format!(
                                "unknown direction: {dir}"
                            )))
                        }
                    };
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::MoveWindow { direction });
                    Ok(())
                })?;
                windows.set("move", f)?;
            }

            // rift.windows.exchange() - swap focused window contents with previously focused window
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::SwapWindows);
                    Ok(())
                })?;
                windows.set("exchange", f)?;
            }

            // rift.windows.focus_prev() - focus the previously focused window
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::FocusPreviousWindow);
                    Ok(())
                })?;
                windows.set("focus_prev", f)?;
            }

            // rift.windows.list() -> array of { id, buf, row, col, rows, cols }
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |lua, ()| {
                    let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                    let t = lua.create_table()?;
                    for (i, w) in s.win_list.iter().enumerate() {
                        let entry = lua.create_table()?;
                        entry.set("id", w.id as i64)?;
                        entry.set("buf", w.buf as i64)?;
                        entry.set("row", w.row as i64)?;
                        entry.set("col", w.col as i64)?;
                        entry.set("rows", w.rows as i64)?;
                        entry.set("cols", w.cols as i64)?;
                        t.set(i + 1, entry)?;
                    }
                    Ok(t)
                })?;
                windows.set("list", f)?;
            }

            // rift.windows.current() -> integer window id of the focused window
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    Ok(sh.lock().unwrap_or_else(|e| e.into_inner()).focused_win_id as i64)
                })?;
                windows.set("current", f)?;
            }

            // rift.windows.previous() -> integer window id of the previously focused window, or nil
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    Ok(sh
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .previous_win_id
                        .map(|id| id as i64))
                })?;
                windows.set("previous", f)?;
            }

            // rift.windows.navigate(direction) - move focus to adjacent window
            // direction: "left" | "right" | "up" | "down"
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, dir: String| {
                    let cmd = match dir.as_str() {
                        "left" => ":split :l",
                        "right" => ":split :r",
                        "up" => ":split :u",
                        "down" => ":split :d",
                        _ => {
                            return Err(mlua::Error::RuntimeError(format!(
                                "unknown direction: {dir}"
                            )))
                        }
                    };
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::ExecAction(cmd.to_string()));
                    Ok(())
                })?;
                windows.set("navigate", f)?;
            }

            api.set("windows", windows)?;
        }

        // rift.register_filetype(ext, lang_name) - map a file extension to a language name
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (ext, lang_name): (String, String)| {
                sh.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mutations
                    .push(PluginMutation::RegisterFiletype { ext, lang_name });
                Ok(())
            })?;
            api.set("register_filetype", f)?;
        }

        // rift.register_language_query(lang_name, query_src) - override highlights query
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (lang_name, query_src): (String, String)| {
                sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                    PluginMutation::RegisterLanguageQuery {
                        lang_name,
                        query_src,
                    },
                );
                Ok(())
            })?;
            api.set("register_language_query", f)?;
        }

        // rift.register_injections_query(lang_name, query_src) - set injections query
        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(move |_, (lang_name, query_src): (String, String)| {
                sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                    PluginMutation::RegisterInjectionsQuery {
                        lang_name,
                        query_src,
                    },
                );
                Ok(())
            })?;
            api.set("register_injections_query", f)?;
        }

        {
            let sh = Arc::clone(&shared);
            let f = lua.create_function(
                move |_, (lang_name, so_path, fn_name): (String, String, String)| {
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::RegisterGrammar {
                            lang_name,
                            so_path,
                            fn_name,
                        },
                    );
                    Ok(())
                },
            )?;
            api.set("register_grammar", f)?;
        }

        // rift.lsp - Language Server Protocol sub-table
        {
            let lsp = lua.create_table()?;

            // rift.lsp.register({ language, command, args?, extensions?, root_markers?, capabilities? })
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, tbl: LuaTable| {
                    let language: String = tbl.get("language")?;
                    let command: String = tbl.get("command")?;
                    let args: Vec<String> = tbl
                        .get::<Option<LuaTable>>("args")?
                        .map(|t| {
                            t.sequence_values::<String>()
                                .filter_map(|v| v.ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    let extensions: Vec<String> = tbl
                        .get::<Option<LuaTable>>("extensions")?
                        .map(|t| {
                            t.sequence_values::<String>()
                                .filter_map(|v| v.ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    let root_markers: Vec<String> = tbl
                        .get::<Option<LuaTable>>("root_markers")?
                        .map(|t| {
                            t.sequence_values::<String>()
                                .filter_map(|v| v.ok())
                                .collect()
                        })
                        .unwrap_or_else(|| vec![".git".to_string()]);
                    let capabilities: Vec<crate::lsp::config::LspCapability> = tbl
                        .get::<Option<LuaTable>>("capabilities")?
                        .map(|t| {
                            t.sequence_values::<String>()
                                .filter_map(|v| v.ok())
                                .filter_map(|s| crate::lsp::config::LspCapability::parse(&s))
                                .collect()
                        })
                        .unwrap_or_default();
                    let initialization_options = tbl
                        .get::<Option<LuaValue>>("initialization_options")?
                        .and_then(|v| lua_to_json(v).ok());
                    let keep_alive = tbl.get::<Option<bool>>("keep_alive")?.unwrap_or(true);
                    sh.lock().unwrap_or_else(|e| e.into_inner()).mutations.push(
                        PluginMutation::LspRegisterServer {
                            language,
                            config: crate::lsp::config::LspServerConfig {
                                command,
                                args,
                                extensions,
                                root_markers,
                                capabilities,
                                initialization_options,
                                keep_alive,
                            },
                        },
                    );
                    Ok(())
                })?;
                lsp.set("register", f)?;
            }

            // rift.lsp.goto_definition()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspGotoDefinition);
                    Ok(())
                })?;
                lsp.set("goto_definition", f)?;
            }

            // rift.lsp.references()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspReferences);
                    Ok(())
                })?;
                lsp.set("references", f)?;
            }

            // rift.lsp.hover()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspHover);
                    Ok(())
                })?;
                lsp.set("hover", f)?;
            }

            // rift.lsp.rename(new_name)
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, new_name: String| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspRename { new_name });
                    Ok(())
                })?;
                lsp.set("rename", f)?;
            }

            // rift.lsp.rename_dialog()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspRenameDialog);
                    Ok(())
                })?;
                lsp.set("rename_dialog", f)?;
            }

            // rift.lsp.diagnostics_panel()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspDiagnosticsPanel);
                    Ok(())
                })?;
                lsp.set("diagnostics_panel", f)?;
            }

            // rift.lsp.format()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspFormat);
                    Ok(())
                })?;
                lsp.set("format", f)?;
            }

            // rift.lsp.code_action()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspCodeAction);
                    Ok(())
                })?;
                lsp.set("code_action", f)?;
            }

            // rift.lsp.diagnostic_next()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspDiagnosticNext);
                    Ok(())
                })?;
                lsp.set("diagnostic_next", f)?;
            }

            // rift.lsp.diagnostic_prev()
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |_, ()| {
                    sh.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mutations
                        .push(PluginMutation::LspDiagnosticPrev);
                    Ok(())
                })?;
                lsp.set("diagnostic_prev", f)?;
            }

            // rift.lsp.get_diagnostics([uri]) -> array of {line, col, severity, message}
            // uri is optional; if omitted, uses the active file's URI.
            // severity: 1=error, 2=warning, 3=info, 4=hint
            {
                let sh = Arc::clone(&shared);
                let f = lua.create_function(move |lua, uri: Option<String>| {
                    let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                    let result = lua.create_table()?;
                    let lookup_key = if let Some(u) = uri {
                        Some(u)
                    } else {
                        s.file_path.as_deref().map(|p| {
                            format!("file:///{}", p.replace('\\', "/").trim_start_matches('/'))
                        })
                    };
                    if let Some(key) = lookup_key {
                        if let Some(diags) = s.lsp_diagnostics.get(&key) {
                            for (i, (line, col, severity, message)) in diags.iter().enumerate() {
                                let entry = lua.create_table()?;
                                entry.set("line", *line as i64 + 1)?; // 1-indexed for Lua
                                entry.set("col", *col as i64)?;
                                entry.set("severity", *severity as i64)?;
                                entry.set("message", message.as_str())?;
                                result.set(i + 1, entry)?;
                            }
                        }
                    }
                    Ok(result)
                })?;
                lsp.set("get_diagnostics", f)?;
            }

            api.set("lsp", lsp)?;
        }

        {
            let f = lua.create_function(|lua, path: String| {
                let path = path.replace('\\', "/");
                let code = format!("package.path = package.path .. ';{}/?.lua'", path);
                lua.load(code.as_str()).exec()?;
                Ok(())
            })?;
            api.set("add_package_path", f)?;
        }

        lua.globals().set("rift", api)?;

        // Embedded Lua prelude - convenience wrappers that don't need Rust bindings.
        lua.load(r#"
-- rift.log level constants
rift.log = { DEBUG = "debug", INFO = "info", WARN = "warn", ERROR = "error" }

-- rift.get_current_line() -> string
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

-- rift.inspect(val) -> string  (pretty-prints any Lua value)
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

-- rift.json - minimal JSON encode/decode
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

-- rift.fs - path and file utilities
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

-- rift.json.decode(str) -> value
-- Minimal recursive descent JSON parser (no unicode escapes, no numbers in exponent form).
do
    local MAX_JSON_DEPTH = 200

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

    local function parse_array(s, i, depth)
        i = i + 1  -- skip '['
        local arr = {}
        i = skip_ws(s, i)
        if s:sub(i,i) == ']' then return arr, i + 1 end
        while true do
            local v; v, i = parse_value(s, i, depth)
            arr[#arr+1] = v
            i = skip_ws(s, i)
            local c = s:sub(i,i)
            if c == ']' then return arr, i + 1 end
            if c ~= ',' then error("expected ',' or ']'") end
            i = i + 1
        end
    end

    local function parse_object(s, i, depth)
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
            local v; v, i = parse_value(s, i, depth)
            obj[k] = v
            i = skip_ws(s, i)
            local c = s:sub(i,i)
            if c == '}' then return obj, i + 1 end
            if c ~= ',' then error("expected ',' or '}'") end
            i = i + 1
        end
    end

    parse_value = function(s, i, depth)
        depth = depth + 1
        if depth > MAX_JSON_DEPTH then error("json too deeply nested") end
        i = skip_ws(s, i)
        local c = s:sub(i,i)
        if c == '"' then return parse_string(s, i)
        elseif c == '[' then return parse_array(s, i, depth)
        elseif c == '{' then return parse_object(s, i, depth)
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
        local ok, val = pcall(function() return (parse_value(str, 1, 0)) end)
        if ok then return val end
        return nil, val  -- nil, error_message
    end
end

-- rift.get_word_at_cursor() -> string
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

-- rift.debounce(fn, polls) -> debounced_fn
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

-- rift.plugins - plugin ownership and lifecycle management
rift.plugins = {}

-- rift.plugins.list() -> string[]
-- Returns all plugin names that have registered anything, sorted lexicographically.
function rift.plugins.list()
    local seen = {}
    for _, name in pairs(_rift_slot_plugin) do seen[name] = true end
    for _, entry in pairs(_rift_commands) do
        if entry.plugin then seen[entry.plugin] = true end
    end
    for _, entry in pairs(_rift_actions) do
        if type(entry) == "table" and entry.plugin then seen[entry.plugin] = true end
    end
    for name in pairs(_rift_plugin_keymaps) do seen[name] = true end
    local result = {}
    for name in pairs(seen) do result[#result+1] = name end
    table.sort(result)
    return result
end

-- rift.plugins.info(name) -> {handlers, commands, actions, keys}
-- Returns a description of everything registered by the named plugin.
-- handlers: array of {slot, event}
-- commands: array of command name strings
-- actions:  array of action id strings
-- keys:     array of {mode, keys}
function rift.plugins.info(name)
    local handlers = {}
    if _rift_plugin_slots[name] then
        for _, slot_id in ipairs(_rift_plugin_slots[name]) do
            local event = _rift_slot_events[slot_id]
            if event then handlers[#handlers+1] = {slot=slot_id, event=event} end
        end
    end
    local commands = {}
    for cmd_name, entry in pairs(_rift_commands) do
        if entry.plugin == name then commands[#commands+1] = cmd_name end
    end
    local actions = {}
    for action_id, entry in pairs(_rift_actions) do
        if type(entry) == "table" and entry.plugin == name then
            actions[#actions+1] = action_id
        end
    end
    local keys = {}
    if _rift_plugin_keymaps[name] then
        for _, k in ipairs(_rift_plugin_keymaps[name]) do
            keys[#keys+1] = {mode=k.mode, keys=k.keys}
        end
    end
    return {handlers=handlers, commands=commands, actions=actions, keys=keys}
end

-- rift.plugins.unload(name)
-- Remove all handlers, commands, actions, and keymaps registered by this plugin.
-- Handlers are unregistered immediately; keymaps queue UnmapKey mutations.
function rift.plugins.unload(name)
    if _rift_plugin_slots[name] then
        for _, slot_id in ipairs(_rift_plugin_slots[name]) do
            rift.off(slot_id)
        end
        _rift_plugin_slots[name] = nil
    end
    local to_remove = {}
    for cmd_name, entry in pairs(_rift_commands) do
        if entry.plugin == name then to_remove[#to_remove+1] = cmd_name end
    end
    for _, cmd_name in ipairs(to_remove) do _rift_commands[cmd_name] = nil end
    to_remove = {}
    for action_id, entry in pairs(_rift_actions) do
        if type(entry) == "table" and entry.plugin == name then
            to_remove[#to_remove+1] = action_id
        end
    end
    for _, action_id in ipairs(to_remove) do _rift_actions[action_id] = nil end
    if _rift_plugin_keymaps[name] then
        for _, k in ipairs(_rift_plugin_keymaps[name]) do
            rift.unmap(k.mode, k.keys)
        end
        _rift_plugin_keymaps[name] = nil
    end
end
"#).set_name("rift:prelude").exec()?;

        Ok(Self { lua, shared })
    }

    /// Refresh the annotation query snapshot and the id the next `add{}` claims.
    /// Call alongside `update_state` before dispatching.
    pub fn set_annotations(&self, views: Vec<AnnotationView>, next_id: u64) {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.annotations = Arc::new(views);
        s.next_annotation_id = next_id;
    }

    /// Update the buffer snapshot that `rift.get_lines()` and friends read.
    /// Call this before dispatching an event.
    #[allow(clippy::too_many_arguments)]
    pub fn update_state(
        &self,
        buf_id: usize,
        buf_kind: String,
        source: Option<BufLinesSource>,
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
        commands: Vec<(String, String)>,
        win_list: Vec<WinEntry>,
        focused_win_id: u64,
        previous_win_id: Option<u64>,
        lsp_diagnostics: std::collections::HashMap<String, Vec<(u32, u32, u32, String)>>,
    ) {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.buf_id = buf_id;
        s.buf_kind = buf_kind;
        s.buf_source = source;
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
        s.commands = commands;
        s.win_list = win_list;
        s.focused_win_id = focused_win_id;
        s.previous_win_id = previous_win_id;
        s.lsp_diagnostics = lsp_diagnostics;
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
            Err(e) => {
                return vec![format!(
                    "failed to build event table for {}: {}",
                    event.name(),
                    e
                )]
            }
        };

        // Snapshot the handler list before dispatch so handlers that call
        // rift.on/rift.off mid-pass only affect the next dispatch.
        let snapshot: Vec<LuaResult<LuaTable>> = list.sequence_values::<LuaTable>().collect();
        let mut errors = Vec::new();
        for entry in snapshot {
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
                    // Restore the prior value after so a reentrant dispatch (e.g. via
                    // rift.emit) doesn't leave current_slot pointing at an inner handler.
                    let prev_slot = {
                        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                        let prev = s.current_slot;
                        s.current_slot = slot_id;
                        prev
                    };
                    let call_result = f.call::<()>(ev_table.clone());
                    self.shared
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .current_slot = prev_slot;
                    if let Err(e) = call_result {
                        errors.push(format!("[lua:{}] {}", event.name(), e));
                    }
                }
                Err(e) => errors.push(format!("[lua:{}] bad handler: {}", event.name(), e)),
            }
        }
        errors
    }

    /// Invoke the Lua handler registered for an annotation (kind, verb) with a
    /// context table. Returns `true` if a handler was found and called.
    pub fn invoke_annotation_action(&self, ctx: &crate::plugin::AnnotationActionCtx) -> bool {
        let handlers: LuaTable = match self.lua.globals().get("_rift_action_handlers") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let key = format!("{}\u{0}{}", ctx.kind, ctx.verb);
        let f: LuaFunction = match handlers.get::<Option<LuaFunction>>(key.as_str()) {
            Ok(Some(f)) => f,
            _ => return false,
        };
        let t = match self.lua.create_table() {
            Ok(t) => t,
            Err(_) => return false,
        };
        let _ = t.set("annotation_id", ctx.annotation_id);
        let _ = t.set("kind", ctx.kind.as_str());
        let _ = t.set("verb", ctx.verb.as_str());
        let _ = t.set("position", ctx.position as i64);
        let _ = t.set("buffer", ctx.buffer as i64);
        if let Ok(p) = ctx.payload.clone().into_lua(&self.lua) {
            let _ = t.set("payload", p);
        }
        if let Ok(p) = ctx.params.clone().into_lua(&self.lua) {
            let _ = t.set("params", p);
        }
        f.call::<()>(t).is_ok()
    }

    /// Invoke the Lua cursor enter/leave hook for an annotation kind, if registered.
    /// `enter` selects the handler table; true if a handler ran (design.md sec 12).
    pub fn invoke_annotation_hook(
        &self,
        enter: bool,
        ctx: &crate::plugin::AnnotationHoverCtx,
    ) -> bool {
        let table_name = if enter {
            "_rift_enter_handlers"
        } else {
            "_rift_leave_handlers"
        };
        let handlers: LuaTable = match self.lua.globals().get(table_name) {
            Ok(t) => t,
            Err(_) => return false,
        };
        let f: LuaFunction = match handlers.get::<Option<LuaFunction>>(ctx.kind.as_str()) {
            Ok(Some(f)) => f,
            _ => return false,
        };
        let t = match self.lua.create_table() {
            Ok(t) => t,
            Err(_) => return false,
        };
        let _ = t.set("annotation_id", ctx.annotation_id);
        let _ = t.set("kind", ctx.kind.as_str());
        let _ = t.set("position", ctx.position as i64);
        let _ = t.set("buffer", ctx.buffer as i64);
        if let Ok(p) = ctx.payload.clone().into_lua(&self.lua) {
            let _ = t.set("payload", p);
        }
        f.call::<()>(t).is_ok()
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
            self.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mutations
                .push(PluginMutation::Notify {
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
        for (name, entry) in cmds.pairs::<String, LuaTable>().flatten() {
            let desc: String = entry.get("description").unwrap_or_default();
            let arg_type: Option<String> = entry.get("arg_type").ok().flatten();
            list.push((name, desc, arg_type));
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
        let entry: Option<LuaTable> = match actions.get(id) {
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
        if let Err(e) = handler.call::<()>(()) {
            self.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mutations
                .push(PluginMutation::Notify {
                    message: format!("[lua:{}] {}", id, e),
                    level: NotificationType::Error,
                });
        }
        true
    }

    /// Load all `.lua` files in `dir`, and set `package.path` to include it.
    /// Returns a list of error strings (empty means all loaded OK).
    /// Load all top-level `.lua` files in `dir`, in lexicographic order.
    /// Does not recurse into subdirectories. Does not modify `package.path`.
    pub fn load_dir(&self, dir: &std::path::Path) -> Vec<String> {
        let mut errors = Vec::new();

        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return errors,
        };

        let mut entries: Vec<std::path::PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "lua"))
            .collect();

        entries.sort();

        for path in entries {
            if let Some(err) = self.load_file(&path) {
                errors.push(err);
            }
        }

        errors
    }

    /// Execute a single `.lua` file. Returns an error string on failure.
    pub fn load_file(&self, path: &std::path::Path) -> Option<String> {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => return Some(format!("lua: failed to read {}: {}", path.display(), e)),
        };
        let name = path.to_string_lossy().replace('\\', "/");
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current_plugin = Some(name.clone());
        let _ = self
            .lua
            .globals()
            .set("_rift_current_plugin_file", name.as_str());
        let result = self
            .lua
            .load(src.as_str())
            .set_name(name.as_str())
            .exec()
            .err()
            .map(|e| format!("lua: {}: {}", path.display(), e));
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current_plugin = None;
        let _ = self
            .lua
            .globals()
            .set("_rift_current_plugin_file", mlua::Value::Nil);
        result
    }

    /// Drain all mutations queued by Lua API calls.
    /// Also fires any pending shell-completion events as Lua UserEvents
    /// before draining, so their handlers can queue further mutations.
    pub fn drain_mutations(&self) -> Vec<PluginMutation> {
        let pending = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut s.pending_shell_events)
        };
        for (tag, success, output) in pending {
            let _ = (|| -> LuaResult<()> {
                let handlers: LuaTable = self.lua.globals().get("_rift_handlers")?;
                let list: Option<LuaTable> = handlers.get("UserEvent")?;
                if let Some(list) = list {
                    let ev = self.lua.create_table()?;
                    ev.set("name", "ShellDone")?;
                    ev.set("tag", tag.as_str())?;
                    ev.set("success", success)?;
                    ev.set("output", output.as_str())?;
                    // Snapshot the handler list before dispatch so handlers that
                    // call rift.on/rift.off mid-pass only affect the next event.
                    let snapshot: Vec<LuaTable> = list
                        .sequence_values::<LuaTable>()
                        .collect::<LuaResult<_>>()?;
                    for entry in snapshot {
                        let f: LuaFunction = entry.get("fn")?;
                        if let Err(e) = f.call::<()>(ev.clone()) {
                            self.shared
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .mutations
                                .push(PluginMutation::Notify {
                                    message: format!("[lua:ShellDone] {}", e),
                                    level: NotificationType::Error,
                                });
                        }
                    }
                }
                Ok(())
            })();
        }
        std::mem::take(
            &mut self
                .shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mutations,
        )
    }

    /// Execute a string of Lua code. Returns an error string on failure.
    pub fn exec(&self, code: &str) -> Option<String> {
        self.lua
            .load(code)
            .exec()
            .err()
            .map(|e| format!("[lua] {}", e))
    }
}

impl std::fmt::Debug for LuaHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaHost").finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "lua_host_tests.rs"]
mod tests;
