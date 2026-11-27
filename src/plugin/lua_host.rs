//! Lua 5.4 plugin host - embeds mlua and exposes the `rift.*` API.

use crate::action::{Action, EditorAction};
use crate::document::ops::{BufferChange, BufferTarget};
use crate::document::DocumentId;
use crate::floating_window::popup::PopupContent;
use crate::job_manager::jobs::plugin_process::{JobCommand, JobSpec, JobStream, PluginJobEvent};
use crate::notification::NotificationType;
use crate::plugin::events::{EventPayload, SaveDecision, SaveHold, SaveRequestId};
pub(crate) use crate::plugin::lua_state::{
    AnnotationView, BufAnnotations, BufEntry, BufMeta, BufUpdate, HistoryAvailability,
    LspDiagnosticsSnapshot, StateUpdate, SyncKeys, WinEntry,
};
use crate::plugin::lua_value::{
    lua_index, lua_int, value_from_lua, value_from_lua_table, value_into_lua,
};
use crate::plugin::PluginMutation;
use mlua::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

/// Parse a `{ fg, bg, bold, italic, underline, strike, reverse }` style subtable.
fn parse_style(st: &LuaTable) -> LuaResult<crate::annotations::presentation::StyleOverride> {
    let color = |key: &str| -> LuaResult<Option<crate::color::Color>> {
        let Some(s) = st.get::<Option<String>>(key)? else {
            return Ok(None);
        };
        crate::color::Color::parse(&s).map(Some).ok_or_else(|| {
            LuaError::RuntimeError(format!(
                "invalid {key} color '{s}': use a color name, \"#rrggbb\", or \"ansiN\""
            ))
        })
    };
    let mut attrs = crate::layer::CellAttrs::default();
    for (key, flag) in crate::layer::CellAttrs::NAMED {
        if st.get(key).unwrap_or(false) {
            attrs = attrs.union(flag);
        }
    }
    Ok(crate::annotations::presentation::StyleOverride {
        fg: color("fg")?,
        bg: color("bg")?,
        attrs,
    })
}

/// Build an optional `Presentation` from an annotation options table's
/// `face` / `style` / `adornment` / `priority` keys (rift.annotations.add).
fn build_presentation(
    opts: &LuaTable,
) -> LuaResult<Option<crate::annotations::presentation::Presentation>> {
    use crate::annotations::presentation::{Adornment, FaceRef, Placement, Presentation};

    let face: Option<String> = opts.get("face")?;
    let style_tbl: Option<LuaTable> = opts.get("style")?;
    let adorn_tbl: Option<LuaTable> = opts.get("adornment")?;
    let priority = opts.get::<Option<i64>>("priority")?.unwrap_or(0);
    let priority =
        i32::try_from(priority.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or_default();

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

fn setting_value_to_lua(
    lua: &Lua,
    value: &crate::command_line::settings::descriptor::SettingValue,
) -> LuaResult<LuaValue> {
    use crate::command_line::settings::descriptor::SettingValue;
    Ok(match value {
        SettingValue::Bool(b) => LuaValue::Boolean(*b),
        SettingValue::Integer(n) => LuaValue::Integer(lua_int(*n)),
        SettingValue::Float(f) => LuaValue::Number(*f),
        SettingValue::Enum(_)
        | SettingValue::Color(_)
        | SettingValue::Path(_)
        | SettingValue::Text(_) => LuaValue::String(lua.create_string(value.to_display_string())?),
    })
}

#[cfg(feature = "lsp")]
fn lua_to_json(v: LuaValue) -> Result<serde_json::Value, String> {
    match v {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        LuaValue::Integer(n) => Ok(serde_json::Value::Number(n.into())),
        LuaValue::Number(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| format!("non-finite float {f}")),
        LuaValue::String(s) => Ok(serde_json::Value::String(s.to_string_lossy())),
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
                        LuaValue::String(s) => s.to_string_lossy().clone(),
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

fn line_bytes(buffer: &crate::buffer::TextBuffer, row: usize) -> Vec<u8> {
    let mut bytes = buffer.get_line_bytes(row);
    while bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    bytes
}

const fn anchor_view(anchor: &crate::plugin::AnnotationAnchorSpec) -> (&'static str, usize, usize) {
    use crate::plugin::AnnotationAnchorSpec;
    match *anchor {
        AnnotationAnchorSpec::Line(l) => ("line", l, l),
        AnnotationAnchorSpec::Point(p) => ("point", p, p),
        AnnotationAnchorSpec::Range(s, e) => ("range", s, e),
    }
}

/// Build the Lua table a query function hands back for one annotation.
fn annotation_view_to_table(lua: &Lua, v: &AnnotationView) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("id", v.id)?;
    t.set("kind", v.kind.as_ref())?;
    t.set("owner", v.owner)?;
    t.set("anchor", v.anchor)?;
    t.set("start", lua_int(v.start))?;
    t.set("end", lua_int(v.end))?;
    t.set("visible", v.visible)?;
    t.set("interactive", v.interactive)?;
    if let Ok(p) = value_into_lua(v.payload.clone(), lua) {
        t.set("payload", p)?;
    }
    Ok(t)
}

/// State shared between Lua API closures and the host.
/// Stored behind `Rc<RefCell>` so closures can be `'static`.
struct LuaSharedState {
    /// Mutations queued by Lua plugin calls; drained by `PluginHost`.
    mutations: Vec<PluginMutation>,
    bufs: std::collections::HashMap<u64, BufSnapshot>,
    creation_ids: Vec<u64>,
    /// Active buffer ID.
    buf_id: usize,
    mode: String,
    /// Snapshot of all open buffers with rich metadata.
    buf_list: Vec<BufEntry>,
    /// Counter used to assign unique slot IDs when `rift.on()` is called.
    next_slot: u32,
    /// Current terminal dimensions (rows, cols).
    window_size: (u16, u16),
    /// Current scroll position (`top_line`, `left_col`).
    scroll: (usize, usize),
    /// Snapshot of all split windows with layout info, updated before each dispatch.
    win_list: Vec<WinEntry>,
    /// ID of the currently focused window.
    focused_win_id: u64,
    /// ID of the previously focused window, if any.
    previous_win_id: Option<u64>,
    /// Snapshot of LSP diagnostics: `normalized_uri` -> [(line, col, severity, message)].
    /// severity: 1=error, 2=warning, 3=info, 4=hint
    lsp_diagnostics: LspDiagnosticsSnapshot,
    current_plugin: Option<String>,
    plugin_health: std::collections::HashMap<String, PluginHealth>,
    is_active: bool,
    save_holds: std::collections::HashMap<DocumentId, (Option<String>, SaveRequestId)>,
    buffer_vars: std::collections::HashMap<
        u64,
        std::collections::HashMap<String, crate::annotations::value::Value>,
    >,
    user_settings: crate::state::UserSettings,
    synced: SyncKeys,
    diverged: std::collections::HashSet<u64>,
    timers: timer_api::Timers,
    #[cfg(feature = "treesitter")]
    language_loader: Arc<crate::syntax::loader::LanguageLoader>,
    #[cfg(feature = "treesitter")]
    syntax: std::collections::HashMap<u64, crate::syntax::snapshot::BufferSyntax>,
    #[cfg(feature = "treesitter")]
    query_cache: std::collections::HashMap<
        (String, crate::syntax::loader::QueryKind),
        (String, Rc<tree_sitter::Query>),
    >,
}

struct BufSnapshot {
    buffer: crate::buffer::TextBuffer,
    meta: BufMeta,
    options: crate::document::definitions::DocumentOptions,
    annotations: Vec<AnnotationView>,
    next_annotation_id: u64,
}

impl Default for BufSnapshot {
    fn default() -> Self {
        Self {
            buffer: crate::buffer::TextBuffer::default(),
            meta: BufMeta::default(),
            options: crate::document::definitions::DocumentOptions::default(),
            annotations: Vec::new(),
            next_annotation_id: 1,
        }
    }
}

impl BufSnapshot {
    fn apply(&mut self, update: BufUpdate) {
        if let Some(buffer) = update.text {
            self.buffer = buffer;
        }
        if let Some(meta) = update.meta {
            let (row, col) = meta.cursor;
            let _ = BufferChange::Cursor { row, col }
                .apply(BufferTarget::Snapshot(&mut self.buffer, meta.edit_policy));
            self.meta = meta;
        }
        if let Some(options) = update.options {
            self.options = options;
        }
        if let Some(annotations) = update.annotations {
            self.annotations = annotations.views;
            self.next_annotation_id = annotations.next_id;
        }
    }
}

#[derive(Default)]
struct PluginHealth {
    budget_failures: u32,
    disabled: bool,
    last_budget_error: Option<String>,
}

fn opts_buf(opts: Option<&LuaTable>) -> LuaResult<Option<u64>> {
    opts.map_or(Ok(None), |opts| opts.get("buf"))
}

fn buf_or_current(shared: &RefCell<LuaSharedState>, buf: Option<u64>) -> LuaResult<u64> {
    shared.borrow().target(buf)
}

fn opts_target(shared: &RefCell<LuaSharedState>, opts: Option<&LuaTable>) -> LuaResult<u64> {
    buf_or_current(shared, opts_buf(opts)?)
}

fn parse_action(api: &str, action: &str) -> LuaResult<Action> {
    action
        .parse::<Action>()
        .map_err(|e| LuaError::RuntimeError(format!("{api}: {}", e.message)))
}

impl LuaSharedState {
    const fn current_id(&self) -> u64 {
        self.buf_id as u64
    }

    fn target(&self, buf: Option<u64>) -> LuaResult<u64> {
        match buf {
            None => Ok(self.current_id()),
            Some(id) if self.bufs.contains_key(&id) => Ok(id),
            Some(id) => Err(LuaError::RuntimeError(format!("no open buffer {id}"))),
        }
    }

    fn snapshot(&self, buf: u64) -> Option<&BufSnapshot> {
        self.bufs.get(&buf)
    }

    fn annotations(&self, buf: u64) -> &[AnnotationView] {
        self.snapshot(buf).map_or(&[], |snap| &snap.annotations)
    }

    fn queue_buffer_change(&mut self, mutation: PluginMutation) {
        if let Some((id, _, change)) = mutation.buffer_change() {
            let buf = id.get();
            if let Some(snap) = self.bufs.get_mut(&buf) {
                let buffer = &mut snap.buffer;
                let revision = buffer.revision();
                let cursor = buffer.cursor();
                let _ = change.apply(BufferTarget::Snapshot(buffer, snap.meta.edit_policy));
                let text_changed = buffer.revision() != revision;
                if text_changed {
                    snap.meta.is_dirty = true;
                    snap.meta.history = HistoryAvailability {
                        can_undo: true,
                        can_redo: false,
                    };
                }
                if text_changed || buffer.cursor() != cursor {
                    let pos = buffer.cursor();
                    let row = buffer.line_index.get_line_at(pos);
                    snap.meta.cursor = (row, pos - buffer.line_index.get_line_start(row));
                    self.diverged.insert(buf);
                }
            }
        }
        self.mutations.push(mutation);
    }

    fn edit_local_snapshot(&mut self, buf: u64, edit: impl FnOnce(&mut BufSnapshot)) {
        let Some(snap) = self.bufs.get_mut(&buf) else {
            return;
        };
        edit(snap);
        self.diverged.insert(buf);
    }

    fn create_local_buf(
        &mut self,
        kind: &str,
        title: Option<&str>,
        lines: &[String],
    ) -> LuaResult<u64> {
        if !self.is_active {
            return Err(LuaError::RuntimeError(
                "cannot create buffer: plugin generation is retired".into(),
            ));
        }
        let id = self.creation_ids.pop().ok_or_else(|| {
            LuaError::RuntimeError("cannot create more than 8 buffers before the next sync".into())
        })?;
        let buffer = crate::buffer::TextBuffer::from_text(&lines.join("\n"));
        self.buf_list.push(BufEntry {
            id: usize::try_from(id).map_err(LuaError::external)?,
            name: title.unwrap_or(kind).to_string(),
            is_dirty: false,
            is_current: true,
            kind: kind.to_string(),
            path: None,
            line_count: buffer.get_total_lines(),
            is_read_only: false,
        });
        self.bufs.insert(
            id,
            BufSnapshot {
                buffer,
                meta: BufMeta {
                    kind: kind.to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        self.buf_id = usize::try_from(id).map_err(LuaError::external)?;
        for entry in &mut self.buf_list {
            entry.is_current = entry.id as u64 == id;
        }
        self.synced.buf_list = None;
        Ok(id)
    }

    fn plugin_disabled(&self, plugin: Option<&str>) -> bool {
        plugin
            .and_then(|plugin| self.plugin_health.get(plugin))
            .is_some_and(|health| health.disabled)
    }

    fn new(language_loader: Arc<crate::syntax::loader::LanguageLoader>) -> Self {
        #[cfg(not(feature = "treesitter"))]
        drop(language_loader);
        Self {
            mutations: Vec::new(),
            bufs: std::collections::HashMap::new(),
            creation_ids: Vec::new(),
            buf_id: 0,
            mode: "normal".to_string(),
            buf_list: Vec::new(),
            next_slot: 1,
            window_size: (0, 0),
            scroll: (0, 0),
            win_list: Vec::new(),
            focused_win_id: 0,
            previous_win_id: None,
            lsp_diagnostics: std::collections::HashMap::new(),
            current_plugin: None,
            plugin_health: std::collections::HashMap::new(),
            is_active: true,
            save_holds: std::collections::HashMap::new(),
            buffer_vars: std::collections::HashMap::new(),
            user_settings: crate::state::UserSettings::new(),
            synced: SyncKeys::default(),
            diverged: std::collections::HashSet::new(),
            timers: timer_api::Timers::default(),
            #[cfg(feature = "treesitter")]
            language_loader,
            #[cfg(feature = "treesitter")]
            syntax: std::collections::HashMap::new(),
            #[cfg(feature = "treesitter")]
            query_cache: std::collections::HashMap::new(),
        }
    }
}

fn registered_commands(lua: &Lua) -> Vec<(String, String, Option<String>)> {
    let Ok(cmds) = lua.globals().get::<LuaTable>("_rift_commands") else {
        return Vec::new();
    };
    cmds.pairs::<String, LuaTable>()
        .flatten()
        .map(|(name, entry)| {
            let desc: String = entry.get("description").unwrap_or_default();
            let arg_type: Option<String> = entry.get("arg_type").ok().flatten();
            (name, desc, arg_type)
        })
        .collect()
}

/// Owns the Lua VM and registers the `rift.*` API.
pub(super) struct LuaHost {
    lua: Lua,
    shared: Rc<RefCell<LuaSharedState>>,
    /// Absolute deadline for the in-flight Lua call, checked by a VM
    /// instruction hook so a runaway handler aborts instead of freezing.
    hook_deadline: Rc<RefCell<Option<std::time::Instant>>>,
    generation: crate::plugin::PluginGeneration,
}

const PRELUDE: &str = r#"
-- rift.log level constants
rift.log = { DEBUG = "debug", INFO = "info", WARN = "warn", ERROR = "error" }

function rift.get_current_line(opts)
    local row = select(1, rift.get_cursor(opts))
    local lines = rift.get_lines(row, row, opts)
    return lines[1] or ""
end

function rift.set_current_line(text, opts)
    local row = select(1, rift.get_cursor(opts))
    rift.replace_lines(row, row, {text}, opts)
end

function rift.delete_current_line(opts)
    local row = select(1, rift.get_cursor(opts))
    rift.replace_lines(row, row, {}, opts)
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

-- Returns the word (alphanumeric + underscore) under the cursor, or "" if none.
function rift.get_word_at_cursor(opts)
    local row, col = rift.get_cursor(opts)
    local line = rift.get_lines(row, row, opts)[1] or ""
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

function rift.debounce(fn, ms)
    local id
    return function(...)
        local args = table.pack(...)
        if id then rift.timer_stop(id) end
        id = rift.timer(ms, function()
            id = nil
            fn(table.unpack(args, 1, args.n))
        end)
    end
end

-- rift.plugins - plugin ownership and lifecycle management
rift.plugins = {}
rift.plugins.enable = _rift_enable_plugin

-- rift.plugins.list() -> string[]
function rift.plugins.list()
    local seen = {}
    for _, name in pairs(_rift_slot_plugin) do seen[name] = true end
    for _, registry in ipairs({
        _rift_commands, _rift_actions, _rift_action_handlers,
        _rift_enter_handlers, _rift_leave_handlers, _rift_buffer_kinds,
        _rift_annotation_kinds, _rift_annotation_bindings, _rift_jobs
    }) do
        for _, entry in pairs(registry) do
            if entry.plugin then seen[entry.plugin] = true end
        end
    end
    for name in pairs(_rift_plugin_keymaps) do seen[name] = true end
    for name in pairs(_rift_plugin_timers()) do seen[name] = true end
    for name in pairs(_rift_highlight_namespaces) do seen[name] = true end
    for name in pairs(_rift_load_errors) do seen[name] = true end
    local result = {}
    for name in pairs(seen) do result[#result+1] = name end
    table.sort(result)
    return result
end

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
    local jobs = 0
    for _, entry in pairs(_rift_jobs) do
        if entry.plugin == name then jobs = jobs + 1 end
    end
    local status, error = _rift_plugin_status(name)
    return {
        handlers=handlers, commands=commands, actions=actions, keys=keys,
        jobs=jobs, timers=_rift_plugin_timers()[name] or 0, status=status, error=error,
    }
end

-- rift.plugins.unload(name)
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
    for _, registry in ipairs({
        _rift_action_handlers, _rift_enter_handlers, _rift_leave_handlers,
        _rift_buffer_kinds, _rift_annotation_kinds, _rift_annotation_bindings
    }) do
        local keys = {}
        for key, entry in pairs(registry) do
            if entry.plugin == name then keys[#keys+1] = key end
        end
        for _, key in ipairs(keys) do registry[key] = nil end
    end
    _rift_unload_owned(name)
    _rift_load_errors[name] = nil
end
"#;

impl LuaHost {
    #[cfg(test)]
    /// Create a new Lua VM with a fresh generation and register the full `rift` API table.
    ///
    /// # Errors
    /// Returns an error if the Lua VM or the `rift` API table cannot be set up.
    fn new() -> LuaResult<Self> {
        let plugin_id = crate::plugin::PluginId::allocate();
        let gen = crate::plugin::PluginGeneration::allocate(plugin_id);
        Self::with_generation(
            gen,
            Arc::new(crate::syntax::loader::LanguageLoader::new(
                std::path::PathBuf::new(),
            )),
        )
    }

    /// Create a new Lua VM bound to a specific plugin generation.
    ///
    /// # Errors
    /// Returns an error if the Lua VM or the `rift` API table cannot be set up.
    pub(super) fn with_generation(
        generation: crate::plugin::PluginGeneration,
        language_loader: Arc<crate::syntax::loader::LanguageLoader>,
    ) -> LuaResult<Self> {
        let lua = Lua::new_with(
            LuaStdLib::COROUTINE
                | LuaStdLib::TABLE
                | LuaStdLib::IO
                | LuaStdLib::OS
                | LuaStdLib::STRING
                | LuaStdLib::UTF8
                | LuaStdLib::MATH
                | LuaStdLib::PACKAGE,
            LuaOptions::default(),
        )?;
        let shared = Rc::new(RefCell::new(LuaSharedState::new(language_loader)));
        let hook_deadline: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));
        {
            let deadline = Rc::clone(&hook_deadline);
            // Checked every 100k VM instructions - cheap enough to run continuously,
            // frequent enough to abort a runaway handler well under a second.
            let _ = lua.set_hook(
                mlua::HookTriggers::new().every_nth_instruction(100_000),
                move |_lua, _debug| {
                    let past_deadline = deadline
                        .borrow_mut()
                        .is_some_and(|dl| std::time::Instant::now() >= dl);
                    if past_deadline {
                        return Err(mlua::Error::RuntimeError(
                            "plugin handler exceeded its time budget and was cancelled".into(),
                        ));
                    }
                    Ok(mlua::VmState::Continue)
                },
            );
        }

        {
            let os_table: LuaTable = lua.globals().get("os")?;
            let err_fn = lua.create_function(|_, _: LuaMultiValue| -> LuaResult<()> {
                Err(LuaError::RuntimeError(
                    "os.execute() is disabled; use rift.job.start(opts) or rift.job.shell(cmd, opts) instead"
                        .to_string(),
                ))
            })?;
            os_table.set("execute", err_fn)?;
        }
        Self::remove_unsafe_stdlib(&lua)?;

        // Internal handler/registration tables: _rift_handlers[event] = {{slot,fn}},
        // _rift_slot_{events,plugin}[slot] and _rift_plugin_{slots,keymaps}[plugin] index them for rift.off/unload.
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
        // _rift_enter_handlers[kind] / _rift_leave_handlers[kind] = fn(ctx),
        // when the cursor enters/leaves an annotation of that kind.
        lua.globals()
            .set("_rift_enter_handlers", lua.create_table()?)?;
        lua.globals()
            .set("_rift_leave_handlers", lua.create_table()?)?;
        lua.globals()
            .set("_rift_buffer_kinds", lua.create_table()?)?;

        let api = lua.create_table()?;

        Self::register_lifecycle(&lua, &shared)?;
        Self::register_handlers(&lua, &api, &shared)?;
        Self::register_float(&lua, &api, &shared)?;
        Self::register_annotations(&lua, &api, &shared)?;
        Self::register_cursor_lines(&lua, &api, &shared)?;
        Self::register_commands(&lua, &api, &shared)?;
        Self::register_emit(&lua, &api, &shared)?;
        Self::register_jobs(&lua, &api, &shared)?;
        Self::register_edits(&lua, &api, &shared)?;
        Self::register_line_edits(&lua, &api, &shared)?;
        Self::register_options(&lua, &api, &shared)?;
        Self::register_buffer_info(&lua, &api, &shared)?;
        Self::register_buffer_ops(&lua, &api, &shared)?;
        Self::register_editor_state(&lua, &api, &shared)?;
        Self::register_action_list(&lua, &api)?;
        Self::register_scroll(&lua, &api, &shared)?;
        Self::register_search(&lua, &api, &shared)?;
        Self::register_keymaps(&lua, &api, &shared)?;
        Self::register_windows(&lua, &api, &shared)?;
        Self::register_filetypes(&lua, &api, &shared)?;
        #[cfg(feature = "lsp")]
        Self::register_lsp(&lua, &api, &shared)?;
        #[cfg(not(feature = "lsp"))]
        Self::register_lsp_stub(&lua, &api)?;
        Self::register_package_path(&lua, &api)?;
        Self::register_buffer_kind(&lua, &api, &shared)?;
        Self::register_buffer_create(&lua, &api, &shared)?;
        Self::register_buffer_vars(&lua, &api, &shared)?;
        highlight_api::register(&lua, &api, &shared)?;
        treesitter_api::register(&lua, &api, &shared)?;
        timer_api::register(&lua, &api, &shared)?;
        Self::finish(lua, api, shared, hook_deadline, generation)
    }

    fn register_handlers(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.on(event_name, handler_fn) -> handle (integer)
        // Returns a handle that can be passed to rift.off() to unregister.
        {
            let sh = Rc::clone(shared);
            let on_fn =
                lua.create_function(move |lua, (event_name, callback): (String, LuaFunction)| {
                    let (slot_id, plugin_name) = {
                        let mut s = sh.borrow_mut();
                        let id = s.next_slot;
                        s.next_slot += 1;
                        (id, s.current_plugin.clone())
                    };
                    let handlers: LuaTable = lua.globals().get("_rift_handlers")?;
                    let list: Option<LuaTable> = handlers.get(event_name.as_str())?;
                    let list = if let Some(t) = list {
                        t
                    } else {
                        let t = lua.create_table()?;
                        handlers.set(event_name.as_str(), t.clone())?;
                        t
                    };
                    let entry = lua.create_table()?;
                    entry.set("slot", slot_id)?;
                    entry.set("fn", callback)?;
                    if let Some(ref name) = plugin_name {
                        entry.set("plugin", name.as_str())?;
                    }
                    list.push(entry)?;
                    let slot_events: LuaTable = lua.globals().get("_rift_slot_events")?;
                    slot_events.set(i64::from(slot_id), event_name)?;
                    if let Some(ref name) = plugin_name {
                        let slot_plugin: LuaTable = lua.globals().get("_rift_slot_plugin")?;
                        slot_plugin.set(i64::from(slot_id), name.as_str())?;
                        let plugin_slots: LuaTable = lua.globals().get("_rift_plugin_slots")?;
                        let slots_list: Option<LuaTable> = plugin_slots.get(name.as_str())?;
                        let slots_list = if let Some(t) = slots_list {
                            t
                        } else {
                            let t = lua.create_table()?;
                            plugin_slots.set(name.as_str(), t.clone())?;
                            t
                        };
                        slots_list.push(i64::from(slot_id))?;
                    }
                    Ok(i64::from(slot_id))
                })?;
            api.set("on", on_fn)?;
        }

        // rift.off(handle) - unregister a handler by its slot handle
        {
            let f = lua.create_function(|lua, handle: i64| {
                let slot_events: LuaTable = lua.globals().get("_rift_slot_events")?;
                let event_name: Option<String> = slot_events.get(handle)?;
                let Some(event_name) = event_name else {
                    return Ok(());
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
                let slot_plugin: LuaTable = lua.globals().get("_rift_slot_plugin")?;
                slot_plugin.set(handle, LuaNil)?;
                Ok(())
            })?;
            api.set("off", f)?;
        }

        // rift.notify(level, message)
        // level: "info" | "warn" | "error" | "success"
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (level, message): (String, String)| {
                let level = match level.as_str() {
                    "warn" | "warning" => NotificationType::Warning,
                    "error" => NotificationType::Error,
                    "success" => NotificationType::Success,
                    _ => NotificationType::Info,
                };
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::Notify { message, level });
                Ok(())
            })?;
            api.set("notify", f)?;
        }

        Ok(())
    }

    fn register_lifecycle(lua: &Lua, shared: &Rc<RefCell<LuaSharedState>>) -> LuaResult<()> {
        for name in [
            "_rift_highlight_namespaces",
            "_rift_annotation_kinds",
            "_rift_annotation_bindings",
            "_rift_load_errors",
            "_rift_jobs",
        ] {
            lua.globals().set(name, lua.create_table()?)?;
        }
        let sh = Rc::clone(shared);
        lua.globals().set(
            "_rift_unload_owned",
            lua.create_function(move |lua, owner: String| {
                let namespaces: LuaTable = lua.globals().get("_rift_highlight_namespaces")?;
                if let Some(owned) = namespaces.get::<Option<LuaTable>>(owner.as_str())? {
                    for ns in owned.pairs::<String, bool>() {
                        let (ns, _) = ns?;
                        sh.borrow_mut()
                            .mutations
                            .push(PluginMutation::ClearNsHighlights {
                                buf: None,
                                ns,
                                owner: Some(owner.clone()),
                            });
                    }
                    namespaces.set(owner.as_str(), LuaNil)?;
                }
                Self::stop_plugin_jobs(lua, &sh, &owner)?;
                let mut s = sh.borrow_mut();
                s.plugin_health.remove(&owner);
                s.timers.remove_owned(&owner);
                s.save_holds.retain(|_, (plugin, request)| {
                    if plugin.as_deref() == Some(owner.as_str()) {
                        request.revoke();
                        false
                    } else {
                        true
                    }
                });
                s.mutations.push(PluginMutation::UnloadPlugin { owner });
                Ok(())
            })?,
        )?;
        let sh = Rc::clone(shared);
        lua.globals().set(
            "_rift_enable_plugin",
            lua.create_function(move |_, name: String| {
                if let Some(health) = sh.borrow_mut().plugin_health.get_mut(&name) {
                    health.disabled = false;
                    health.budget_failures = 0;
                }
                Ok(())
            })?,
        )?;
        let sh = Rc::clone(shared);
        lua.globals().set(
            "_rift_plugin_status",
            lua.create_function(move |lua, name: String| {
                let load_errors: LuaTable = lua.globals().get("_rift_load_errors")?;
                if let Some(error) = load_errors.get::<Option<String>>(name.as_str())? {
                    return Ok(("failed", Some(error)));
                }
                let s = sh.borrow();
                let health = s.plugin_health.get(&name);
                let status = if health.is_some_and(|h| h.disabled) {
                    "disabled"
                } else {
                    "active"
                };
                Ok((status, health.and_then(|h| h.last_budget_error.clone())))
            })?,
        )?;
        Ok(())
    }

    fn resource_owner(
        lua: &Lua,
        shared: &RefCell<LuaSharedState>,
        registry: &str,
        key: &str,
    ) -> LuaResult<Option<String>> {
        let owner = shared.borrow().current_plugin.clone();
        let entries: LuaTable = lua.globals().get(registry)?;
        let entry = lua.create_table()?;
        entry.set("plugin", owner.as_deref())?;
        entries.set(key, entry)?;
        Ok(owner)
    }

    fn register_float(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |_, (lines, opts): (LuaTable, Option<LuaTable>)| {
                    let mut v = Vec::new();
                    for s in lines.sequence_values::<String>() {
                        v.push(s?);
                    }
                    let buf = opts_target(&sh, opts.as_ref())?;
                    let mut s = sh.borrow_mut();
                    s.queue_buffer_change(PluginMutation::AppendLines {
                        buf: DocumentId::new(buf),
                        lines: v,
                    });
                    Ok(())
                })?;
            api.set("append_lines", f)?;
        }

        // rift.open_float(title, lines)
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (title, lines): (String, LuaTable)| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::OpenFloat(PopupContent::new(title, v)));
                Ok(())
            })?;
            api.set("open_float", f)?;
        }

        // rift.close_float()
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                sh.borrow_mut().mutations.push(PluginMutation::CloseFloat);
                Ok(())
            })?;
            api.set("close_float", f)?;
        }

        Ok(())
    }

    fn register_annotations(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.annotations.* - author and handle interactive annotations
        let annotations = lua.create_table()?;
        Self::register_annotation_add(lua, &annotations, shared)?;
        Self::register_annotation_edits(lua, &annotations, shared)?;
        Self::register_annotation_queries(lua, &annotations, shared)?;
        Self::register_annotation_hooks(lua, &annotations, shared)?;
        api.set("annotations", annotations)?;
        Ok(())
    }

    fn register_annotation_add(
        lua: &Lua,
        annotations: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: LuaTable| {
                let kind: String = opts.get("kind")?;
                let line: Option<i64> = opts.get("line")?;
                let point: Option<i64> = opts.get("point")?;
                let range: Option<LuaTable> = opts.get("range")?;
                let anchor = if let Some(l) = line {
                    crate::plugin::AnnotationAnchorSpec::Line(lua_index(l))
                } else if let Some(p) = point {
                    crate::plugin::AnnotationAnchorSpec::Point(lua_index(p))
                } else if let Some(r) = range {
                    let s: i64 = r.get(1)?;
                    let e: i64 = r.get(2)?;
                    crate::plugin::AnnotationAnchorSpec::Range(lua_index(s), lua_index(e))
                } else {
                    return Err(mlua::Error::RuntimeError(
                        "annotation needs line, point, or range".into(),
                    ));
                };
                let payload = match opts.get::<LuaValue>("payload")? {
                    LuaValue::Table(t) => value_from_lua_table(&t)?,
                    _ => crate::annotations::value::Value::Null,
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
                let buf = opts_target(&sh, Some(&opts))?;
                // Pre-claim the id so add{} can return it synchronously.
                let id = {
                    let mut s = sh.borrow_mut();
                    let id = s.bufs.get(&buf).map_or(1, |snap| snap.next_annotation_id);
                    let (anchor_name, start, end) = anchor_view(&anchor);
                    let view = AnnotationView {
                        id,
                        kind: Arc::from(kind.as_str()),
                        owner: crate::plugin::annotation_owner(owner.as_deref()).as_str(),
                        anchor: anchor_name,
                        start,
                        end,
                        payload: payload.clone(),
                        visible,
                        interactive: !actions.is_empty(),
                    };
                    s.edit_local_snapshot(buf, |snap| {
                        snap.next_annotation_id += 1;
                        snap.annotations.push(view);
                    });
                    s.mutations.push(PluginMutation::AddAnnotation {
                        buf: DocumentId::new(buf),
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

        Ok(())
    }

    fn register_annotation_edits(
        lua: &Lua,
        annotations: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (id, opts): (u64, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let mut s = sh.borrow_mut();
                s.edit_local_snapshot(buf, |snap| snap.annotations.retain(|v| v.id != id));
                s.mutations.push(PluginMutation::RemoveAnnotation {
                    buf: DocumentId::new(buf),
                    id,
                });
                Ok(())
            })?;
            annotations.set("remove", f)?;
        }

        // rift.annotations.update(id, { payload=, visible=, style/face/... }):
        // unset fields are left as-is; presentation rebuilds only if any given.
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (id, opts): (u64, LuaTable)| {
                let payload = match opts.get::<LuaValue>("payload")? {
                    LuaValue::Table(t) => Some(value_from_lua_table(&t)?),
                    _ => None,
                };
                let visible: Option<bool> = opts.get("visible")?;
                let presentation = build_presentation(&opts)?;
                let buf = opts_target(&sh, Some(&opts))?;
                let mut s = sh.borrow_mut();
                s.edit_local_snapshot(buf, |snap| {
                    if let Some(view) = snap.annotations.iter_mut().find(|v| v.id == id) {
                        if let Some(p) = &payload {
                            view.payload = p.clone();
                        }
                        if let Some(v) = visible {
                            view.visible = v;
                        }
                    }
                });
                s.mutations.push(PluginMutation::UpdateAnnotation {
                    buf: DocumentId::new(buf),
                    id,
                    payload,
                    visible,
                    presentation,
                });
                Ok(())
            })?;
            annotations.set("update", f)?;
        }

        Ok(())
    }

    fn register_annotation_queries(
        lua: &Lua,
        annotations: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, (id, opts): (u64, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                match s.annotations(buf).iter().find(|v| v.id == id) {
                    Some(v) => Ok(Some(annotation_view_to_table(lua, v)?)),
                    None => Ok(None),
                }
            })?;
            annotations.set("get", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, (offset, opts): (i64, Option<LuaTable>)| {
                let off = lua_index(offset);
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                let out = lua.create_table()?;
                let mut i = 1;
                for v in s.annotations(buf) {
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

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |lua, (start, end_, opts): (i64, i64, Option<LuaTable>)| {
                    let s0 = lua_index(start);
                    let e0 = usize::try_from(end_.max(start)).unwrap_or(usize::MAX);
                    let buf = opts_target(&sh, opts.as_ref())?;
                    let s = sh.borrow();
                    let out = lua.create_table()?;
                    let mut i = 1;
                    for v in s.annotations(buf) {
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
                },
            )?;
            annotations.set("in_range", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |lua, (prefix, opts): (String, Option<LuaTable>)| {
                    let buf = opts_target(&sh, opts.as_ref())?;
                    let s = sh.borrow();
                    let out = lua.create_table()?;
                    let matching = s
                        .annotations(buf)
                        .iter()
                        .filter(|v| v.kind.starts_with(&prefix));
                    for (i, v) in (1..).zip(matching) {
                        out.set(i, annotation_view_to_table(lua, v)?)?;
                    }
                    Ok(out)
                })?;
            annotations.set("by_kind", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (prefix, opts): (String, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let mut s = sh.borrow_mut();
                s.edit_local_snapshot(buf, |snap| {
                    snap.annotations.retain(|v| !v.kind.starts_with(&prefix));
                });
                s.mutations.push(PluginMutation::ClearAnnotations {
                    buf: DocumentId::new(buf),
                    kind_prefix: prefix,
                });
                Ok(())
            })?;
            annotations.set("clear", f)?;
        }

        Ok(())
    }

    fn register_annotation_hooks(
        lua: &Lua,
        annotations: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.annotations.on_action(kind, verb, fn(ctx))
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |lua, (kind, verb, handler): (String, String, mlua::Function)| {
                    let key = format!("{kind}\u{0}{verb}");
                    let handlers: LuaTable = lua.globals().get("_rift_action_handlers")?;
                    handlers.set(key, Self::callback_entry(lua, &sh, handler)?)?;
                    let owner = sh.borrow().current_plugin.clone();
                    sh.borrow_mut()
                        .mutations
                        .push(PluginMutation::RegisterAnnotationAction {
                            kind,
                            verb,
                            command: None,
                            owner,
                        });
                    Ok(())
                },
            )?;
            annotations.set("on_action", f)?;
        }

        // rift.annotations.bind_command(kind, verb, ex_command)
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |lua, (kind, verb, command): (String, String, String)| {
                    let owner = Self::resource_owner(
                        lua,
                        &sh,
                        "_rift_annotation_bindings",
                        &format!("{kind}\u{0}{verb}"),
                    )?;
                    sh.borrow_mut()
                        .mutations
                        .push(PluginMutation::RegisterAnnotationAction {
                            kind,
                            verb,
                            command: Some(command),
                            owner,
                        });
                    Ok(())
                },
            )?;
            annotations.set("bind_command", f)?;
        }

        // rift.annotations.register_kind(kind, { face/style/adornment,
        // description }) - per-kind render/hover defaults (sec 4).
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, (kind, opts): (String, LuaTable)| {
                let presentation = build_presentation(&opts)?;
                let description: Option<String> = opts.get("description").ok().flatten();
                let owner = Self::resource_owner(lua, &sh, "_rift_annotation_kinds", &kind)?;
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::RegisterKindDefaults {
                        kind,
                        presentation,
                        description,
                        owner,
                    });
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
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |lua, (kind, handler): (String, mlua::Function)| {
                    let handlers: LuaTable = lua.globals().get(table_name)?;
                    handlers.set(kind, Self::callback_entry(lua, &sh, handler)?)?;
                    Ok(())
                })?;
            annotations.set(method, f)?;
        }

        Ok(())
    }

    fn register_cursor_lines(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.current_buf() -> integer buffer id
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| Ok(lua_int(sh.borrow_mut().buf_id)))?;
            api.set("current_buf", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                let (row, col) = s.snapshot(buf).map_or((0, 0), |snap| snap.meta.cursor);
                Ok((lua_int(row) + 1, lua_int(col)))
            })?;
            api.set("get_cursor", f)?;
        }

        // start/end are 1-indexed; end = -1 means last line
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |lua, (start, end_, opts): (LuaValue, Option<i64>, Option<LuaTable>)| {
                    let (start, end_, opts) = match start {
                        LuaValue::Table(opts) if end_.is_none() => (1, -1, Some(opts)),
                        LuaValue::Nil => (1, end_.unwrap_or(-1), opts),
                        value => (i64::from_lua(value, lua)?, end_.unwrap_or(-1), opts),
                    };
                    let buf = opts_target(&sh, opts.as_ref())?;
                    let s = sh.borrow();
                    let t = lua.create_table()?;
                    let Some(buffer) = s.snapshot(buf).map(|snap| &snap.buffer) else {
                        return Ok(t);
                    };
                    let count = buffer.get_total_lines();
                    let len = lua_int(count);
                    let start = lua_index(start.saturating_sub(1)).min(count);
                    let end_ = lua_index(if end_ < 0 {
                        len + end_ + 1
                    } else {
                        end_.min(len)
                    });
                    for (i, row) in (start..end_).enumerate() {
                        t.set(i + 1, lua.create_string(line_bytes(buffer, row))?)?;
                    }
                    Ok(t)
                },
            )?;
            api.set("get_lines", f)?;
        }

        Ok(())
    }

    fn register_commands(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.register_command(name, fn [, description [, arg_type]])
        // arg_type drives argument completion: "file", "dir"
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |lua,
                      (name, callback, desc, arg_type): (
                    String,
                    LuaFunction,
                    Option<String>,
                    Option<String>,
                )| {
                    let plugin_name = sh.borrow_mut().current_plugin.clone();
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
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, (id, callback): (String, LuaFunction)| {
                let plugin_name = sh.borrow_mut().current_plugin.clone();
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

        Ok(())
    }

    fn register_emit(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.emit(event_name [, payload])  - fire a UserEvent to all registered handlers
        // Optional payload table keys are merged into the event table alongside `name`.
        {
            let sh = Rc::clone(shared);
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
                            let plugin = Self::entry_plugin(&entry);
                            if sh.borrow().plugin_disabled(plugin.as_deref()) {
                                continue;
                            }
                            let f: LuaFunction = entry.get("fn")?;
                            let previous =
                                std::mem::replace(&mut sh.borrow_mut().current_plugin, plugin);
                            let result = f.call::<()>(ev.clone());
                            sh.borrow_mut().current_plugin = previous;
                            result?;
                        }
                    }
                    Ok(())
                })?;
            api.set("emit", f)?;
        }

        Ok(())
    }

    fn register_jobs(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        let job = lua.create_table()?;
        let sh = Rc::clone(shared);
        job.set(
            "start",
            lua.create_function(move |lua, opts: LuaTable| {
                let program: String = opts.get("cmd")?;
                let args: Option<Vec<String>> = opts.get("args")?;
                let command = JobCommand::Program {
                    program,
                    args: args.unwrap_or_default(),
                };
                Self::start_job(lua, &sh, command, &opts)
            })?,
        )?;
        let sh = Rc::clone(shared);
        job.set(
            "shell",
            lua.create_function(move |lua, (line, opts): (String, Option<LuaTable>)| {
                let opts = match opts {
                    Some(opts) => opts,
                    None => lua.create_table()?,
                };
                Self::start_job(lua, &sh, JobCommand::Shell(line), &opts)
            })?,
        )?;
        let sh = Rc::clone(shared);
        job.set(
            "send",
            lua.create_function(move |lua, (id, data): (u64, LuaString)| {
                let data = data.as_bytes().to_vec();
                Self::queue_job_control(lua, &sh, id, PluginMutation::JobInput { id, data })
            })?,
        )?;
        let sh = Rc::clone(shared);
        job.set(
            "close_stdin",
            lua.create_function(move |lua, id: u64| {
                Self::queue_job_control(lua, &sh, id, PluginMutation::JobCloseStdin(id))
            })?,
        )?;
        let sh = Rc::clone(shared);
        job.set(
            "stop",
            lua.create_function(move |lua, id: u64| {
                Self::queue_job_control(lua, &sh, id, PluginMutation::StopJob(id))
            })?,
        )?;
        api.set("job", job)
    }

    fn start_job(
        lua: &Lua,
        shared: &RefCell<LuaSharedState>,
        command: JobCommand,
        opts: &LuaTable,
    ) -> LuaResult<u64> {
        let cwd: Option<String> = opts.get("cwd")?;
        let env = match opts.get::<Option<LuaTable>>("env")? {
            Some(env) => env.pairs::<String, String>().collect::<LuaResult<_>>()?,
            None => Vec::new(),
        };
        let entry = lua.create_table()?;
        for field in ["on_stdout", "on_stderr", "on_exit"] {
            entry.set(field, opts.get::<Option<LuaFunction>>(field)?)?;
        }
        entry.set("plugin", shared.borrow().current_plugin.as_deref())?;
        let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
        lua.globals()
            .get::<LuaTable>("_rift_jobs")?
            .set(id, entry)?;
        shared
            .borrow_mut()
            .mutations
            .push(PluginMutation::StartJob {
                id,
                spec: JobSpec { command, cwd, env },
            });
        Ok(id)
    }

    fn queue_job_control(
        lua: &Lua,
        shared: &RefCell<LuaSharedState>,
        id: u64,
        mutation: PluginMutation,
    ) -> LuaResult<bool> {
        let jobs: LuaTable = lua.globals().get("_rift_jobs")?;
        if !jobs.contains_key(id)? {
            return Ok(false);
        }
        shared.borrow_mut().mutations.push(mutation);
        Ok(true)
    }

    fn stop_plugin_jobs(lua: &Lua, shared: &RefCell<LuaSharedState>, owner: &str) -> LuaResult<()> {
        let jobs: LuaTable = lua.globals().get("_rift_jobs")?;
        let job_ids: Vec<u64> = jobs
            .pairs::<u64, LuaTable>()
            .filter_map(Result::ok)
            .filter(|(_, entry)| Self::entry_plugin(entry).as_deref() == Some(owner))
            .map(|(id, _)| id)
            .collect();
        for id in job_ids {
            jobs.set(id, LuaNil)?;
            shared
                .borrow_mut()
                .mutations
                .push(PluginMutation::StopJob(id));
        }
        Ok(())
    }

    fn register_edits(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (text, opts): (String, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let mut s = sh.borrow_mut();
                s.queue_buffer_change(PluginMutation::InsertAtCursor {
                    buf: DocumentId::new(buf),
                    text,
                });
                Ok(())
            })?;
            api.set("insert", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (n, opts): (i64, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                if n > 0 {
                    let mut s = sh.borrow_mut();
                    let count = lua_index(n);
                    s.queue_buffer_change(PluginMutation::DeleteBefore {
                        buf: DocumentId::new(buf),
                        count,
                    });
                }
                Ok(())
            })?;
            api.set("delete_before", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (n, opts): (i64, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                if n > 0 {
                    let mut s = sh.borrow_mut();
                    let count = lua_index(n);
                    s.queue_buffer_change(PluginMutation::DeleteForward {
                        buf: DocumentId::new(buf),
                        count,
                    });
                }
                Ok(())
            })?;
            api.set("delete_forward", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |_, (row, col, opts): (i64, i64, Option<LuaTable>)| {
                    let buf = opts_target(&sh, opts.as_ref())?;
                    if row >= 1 {
                        let mut s = sh.borrow_mut();
                        let (row, col) = (lua_index(row), lua_index(col));
                        s.queue_buffer_change(PluginMutation::SetCursor {
                            buf: DocumentId::new(buf),
                            row,
                            col,
                        });
                    }
                    Ok(())
                })?;
            api.set("set_cursor", f)?;
        }

        Ok(())
    }

    fn register_line_edits(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |_, (start, end_, lines, opts): (i64, i64, LuaTable, Option<LuaTable>)| {
                    let mut v = Vec::new();
                    for s in lines.sequence_values::<String>() {
                        v.push(s?);
                    }
                    let buf = opts_target(&sh, opts.as_ref())?;
                    if start >= 1 && end_ >= start {
                        let mut s = sh.borrow_mut();
                        let (start, end) = (lua_index(start), lua_index(end_));
                        s.queue_buffer_change(PluginMutation::ReplaceLines {
                            buf: DocumentId::new(buf),
                            start,
                            end,
                            lines: v,
                        });
                    }
                    Ok(())
                },
            )?;
            api.set("replace_lines", f)?;
        }

        // rift.set_cursor_hold_delay(ms) - set the CursorHold idle threshold in milliseconds
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ms: u32| {
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::SetCursorHoldDelay(ms));
                Ok(())
            })?;
            api.set("set_cursor_hold_delay", f)?;
        }

        Ok(())
    }

    fn register_options(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        let resolver = crate::command_line::settings::resolver::SettingsResolver::new(
            crate::document::definitions::create_document_settings_registry(),
            crate::state::definitions::create_settings_registry(),
        );
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |_, (name, value, opts): (String, LuaValue, Option<LuaTable>)| {
                    let value_str = match &value {
                        LuaValue::Boolean(b) => b.to_string(),
                        LuaValue::Integer(n) => n.to_string(),
                        LuaValue::Number(n) => crate::num::f64_to_i64(*n).to_string(),
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        _ => return Ok(()),
                    };
                    let buf = opts_target(&sh, opts.as_ref())?;
                    let mut guard = sh.borrow_mut();
                    let s = &mut *guard;
                    let local = resolver.is_local(&name);
                    let mut scratch = crate::document::definitions::DocumentOptions::default();
                    let options = s
                        .bufs
                        .get_mut(&buf)
                        .map_or(&mut scratch, |snap| &mut snap.options);
                    let _ = resolver.set(&name, Some(&value_str), options, &mut s.user_settings);
                    if local {
                        s.diverged.insert(buf);
                    } else {
                        s.synced.user_settings = None;
                    }
                    let buf = local.then(|| DocumentId::new(buf));
                    s.mutations.push(PluginMutation::SetOption {
                        buf,
                        name,
                        value: value_str,
                    });
                    Ok(())
                },
            )?;
            api.set("set_option", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, (name, opts): (String, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                let default_options;
                let options = if let Some(snap) = s.snapshot(buf) {
                    &snap.options
                } else {
                    default_options = crate::document::definitions::DocumentOptions::default();
                    &default_options
                };
                resolver.get(&name, options, &s.user_settings).map_or_else(
                    || Ok(LuaValue::Nil),
                    |value| setting_value_to_lua(lua, &value),
                )
            })?;
            api.set("get_option", f)?;
        }

        Ok(())
    }

    fn register_buffer_info(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                match s.snapshot(buf).and_then(|snap| snap.meta.filetype.as_ref()) {
                    Some(ft) => Ok(LuaValue::String(lua.create_string(ft)?)),
                    None => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_filetype", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                match s
                    .snapshot(buf)
                    .and_then(|snap| snap.meta.file_path.as_ref())
                {
                    Some(p) => Ok(LuaValue::String(lua.create_string(p)?)),
                    None => Ok(LuaValue::Nil),
                }
            })?;
            api.set("get_filepath", f)?;
        }

        // rift.get_buf_list() -> sequence of { id, name, is_dirty, is_current, kind, path, line_count, is_read_only }
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.borrow_mut();
                let result = lua.create_table()?;
                for (i, b) in s.buf_list.iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set("id", lua_int(b.id))?;
                    entry.set("name", b.name.as_str())?;
                    entry.set("is_dirty", b.is_dirty)?;
                    entry.set("is_current", b.is_current)?;
                    entry.set("kind", b.kind.as_str())?;
                    entry.set("line_count", lua_int(b.line_count))?;
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

        Ok(())
    }

    fn register_buffer_ops(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                Ok(s.snapshot(buf)
                    .map_or_else(|| "file".to_string(), |snap| snap.meta.kind.clone()))
            })?;
            api.set("buf_kind", f)?;
        }

        // rift.switch_buf(buf_id) - switch the active buffer to the given ID
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, id: u64| {
                let mut s = sh.borrow_mut();
                let id = s.target(Some(id))?;
                s.buf_id = usize::try_from(id).map_err(LuaError::external)?;
                for entry in &mut s.buf_list {
                    entry.is_current = entry.id as u64 == id;
                }
                s.synced.buf_list = None;
                s.mutations
                    .push(PluginMutation::SwitchToBuffer(DocumentId::new(id)));
                Ok(())
            })?;
            api.set("switch_buf", f)?;
        }

        // rift.open_file(path [, force]) - open a file, optionally discarding unsaved changes
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (path, force): (String, Option<bool>)| {
                sh.borrow_mut().mutations.push(PluginMutation::OpenFile {
                    path,
                    force: force.unwrap_or(false),
                });
                Ok(())
            })?;
            api.set("open_file", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |_, (force, opts): (Option<bool>, Option<LuaTable>)| {
                    let buf = opts_target(&sh, opts.as_ref())?;
                    sh.borrow_mut().mutations.push(PluginMutation::CloseBuffer {
                        buf: DocumentId::new(buf),
                        force: force.unwrap_or(false),
                    });
                    Ok(())
                })?;
            api.set("close_buf", f)?;
        }

        // rift.create_scratch_buf(name, lines) - create an in-memory buffer
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (name, lines): (String, LuaTable)| {
                let mut v = Vec::new();
                for s in lines.sequence_values::<String>() {
                    v.push(s?);
                }
                let mut s = sh.borrow_mut();
                let id = s.create_local_buf("scratch", Some(&name), &v)?;
                s.mutations.push(PluginMutation::CreateScratchBuf {
                    id: DocumentId::new(id),
                    name,
                    lines: v,
                });
                Ok(id)
            })?;
            api.set("create_scratch_buf", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |_, (force, opts): (Option<bool>, Option<LuaTable>)| {
                    let buf = opts_target(&sh, opts.as_ref())?;
                    sh.borrow_mut()
                        .mutations
                        .push(PluginMutation::ReloadBuffer {
                            buf: DocumentId::new(buf),
                            force: force.unwrap_or(false),
                        });
                    Ok(())
                })?;
            api.set("reload_buf", f)?;
        }

        Ok(())
    }

    fn register_action_list(lua: &Lua, api: &LuaTable) -> LuaResult<()> {
        let f = lua.create_function(|lua, ()| {
            let result = lua.create_table()?;
            for spec in crate::action::table::ACTIONS {
                let entry = lua.create_table()?;
                entry.set("id", spec.id)?;
                entry.set(
                    "args",
                    lua.create_sequence_from(spec.args.iter().map(|arg| arg.name()))?,
                )?;
                entry.set("operation", spec.op)?;
                entry.set("description", spec.doc)?;
                result.push(entry)?;
            }
            for (kind, actions) in crate::action::table::KIND_ACTIONS {
                for spec in *actions {
                    let entry = lua.create_table()?;
                    entry.set("id", spec.id)?;
                    entry.set("args", lua.create_table()?)?;
                    entry.set("kind", *kind)?;
                    entry.set("operation", spec.op)?;
                    entry.set("description", spec.doc)?;
                    result.push(entry)?;
                }
            }
            Ok(result)
        })?;
        api.set("get_actions", f)
    }

    fn register_editor_state(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.get_commands() -> sequence of { name, description }
        {
            let f = lua.create_function(|lua, ()| {
                let result = lua.create_table()?;
                for (i, (name, desc, _)) in registered_commands(lua).into_iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set("name", name)?;
                    entry.set("description", desc)?;
                    result.set(i + 1, entry)?;
                }
                Ok(result)
            })?;
            api.set("get_commands", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                sh.borrow_mut().mutations.push(PluginMutation::SaveBuffer {
                    buf: DocumentId::new(buf),
                });
                Ok(())
            })?;
            api.set("save", f)?;
        }

        // rift.get_tab_width() -> integer
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.borrow();
                Ok(lua_int(s.snapshot(s.current_id()).map_or(
                    crate::document::definitions::DocumentOptions::default().tab_width(),
                    |snap| snap.options.tab_width(),
                )))
            })?;
            api.set("get_tab_width", f)?;
        }

        // rift.get_expand_tabs() -> boolean
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.borrow();
                Ok(s.snapshot(s.current_id()).map_or(
                    crate::document::definitions::DocumentOptions::default().expand_tabs(),
                    |snap| snap.options.expand_tabs(),
                ))
            })?;
            api.set("get_expand_tabs", f)?;
        }

        // rift.get_mode() -> "normal" | "insert" | "command" | "search"
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| Ok(sh.borrow_mut().mode.clone()))?;
            api.set("get_mode", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let s = sh.borrow();
                Ok(lua_int(
                    s.snapshot(buf)
                        .map_or(0, |snap| snap.buffer.get_total_lines()),
                ))
            })?;
            api.set("get_line_count", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                Ok(sh
                    .borrow()
                    .snapshot(buf)
                    .is_some_and(|snap| snap.meta.history.can_undo))
            })?;
            api.set("can_undo", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                Ok(sh
                    .borrow()
                    .snapshot(buf)
                    .is_some_and(|snap| snap.meta.history.can_redo))
            })?;
            api.set("can_redo", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                Ok(sh
                    .borrow()
                    .snapshot(buf)
                    .is_some_and(|snap| snap.meta.is_dirty))
            })?;
            api.set("is_dirty", f)?;
        }

        Ok(())
    }

    fn register_scroll(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.get_scroll() -> top_line, left_col
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.borrow_mut();
                Ok((lua_int(s.scroll.0), lua_int(s.scroll.1)))
            })?;
            api.set("get_scroll", f)?;
        }

        // rift.set_scroll(top_line, left_col)
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (top, left): (usize, usize)| {
                let mut s = sh.borrow_mut();
                s.scroll = (top, left);
                s.mutations.push(PluginMutation::SetScroll(top, left));
                Ok(())
            })?;
            api.set("set_scroll", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, opts: Option<LuaTable>| {
                let buf = opts_target(&sh, opts.as_ref())?;
                Ok(sh
                    .borrow()
                    .snapshot(buf)
                    .map_or("lf", |snap| snap.options.line_ending().name()))
            })?;
            api.set("get_line_ending", f)?;
        }

        // rift.set_line_ending(type) - "lf" | "crlf"
        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |_, (ending, opts): (String, Option<LuaTable>)| {
                    let buf = opts_target(&sh, opts.as_ref())?;
                    let mut s = sh.borrow_mut();
                    s.edit_local_snapshot(buf, |snap| {
                        let ending = crate::document::LineEnding::from_name(&ending).name();
                        let _ = crate::document::definitions::create_document_settings_registry()
                            .set("line_ending", Some(ending), &mut snap.options);
                    });
                    s.mutations.push(PluginMutation::SetLineEnding {
                        buf: DocumentId::new(buf),
                        ending,
                    });
                    Ok(())
                })?;
            api.set("set_line_ending", f)?;
        }

        Ok(())
    }

    fn register_search(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.search(needle [, opts]) -> array of {row, col_start, col_end}
        // Literal search, 1-indexed rows/0-indexed cols; opts.whole_word restricts to word boundaries.
        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |lua, (needle, opts): (String, Option<LuaTable>)| {
                    let whole_word = opts
                        .as_ref()
                        .and_then(|t| t.get::<bool>("whole_word").ok())
                        .unwrap_or(false);
                    let buf = opts_target(&sh, opts.as_ref())?;
                    let s = sh.borrow();
                    let results = lua.create_table()?;
                    let Some(buffer) = s.snapshot(buf).map(|snap| &snap.buffer) else {
                        return Ok(results);
                    };
                    let needle = needle.as_bytes();
                    if needle.is_empty() {
                        return Ok(results);
                    }
                    for row_idx in 0..buffer.get_total_lines() {
                        let bytes = line_bytes(buffer, row_idx);
                        let mut start = 0;
                        while let Some(pos) = bytes
                            .get(start..)
                            .and_then(|rest| rest.windows(needle.len()).position(|w| w == needle))
                        {
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
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                let s = sh.borrow_mut();
                Ok((i64::from(s.window_size.0), i64::from(s.window_size.1)))
            })?;
            api.set("get_window_size", f)?;
        }

        Ok(())
    }

    fn register_keymaps(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.exec_action(action) - fire an editor action by name, or a plugin action as "plugin:<id>"
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, action: String| {
                let action = parse_action("rift.exec_action", &action)?;
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::ExecAction(action));
                Ok(())
            })?;
            api.set("exec_action", f)?;
        }

        // rift.map(mode, keys, action) - register a key binding.
        // action: action string ("editor:save", "plugin:<id>" for rift.register_action ids) or a
        // function, registered as a generated plugin-owned action that rift.plugins.unload removes.
        {
            let sh = Rc::clone(shared);
            let next_map_action = std::cell::Cell::new(1_u64);
            let f = lua.create_function(
                move |lua, (mode, keys, action): (String, String, LuaValue)| {
                    let plugin_name = sh.borrow_mut().current_plugin.clone();
                    let action = match action {
                        LuaValue::String(s) => {
                            let action = s.to_str()?.to_string();
                            parse_action("rift.map", &action)?;
                            action
                        }
                        LuaValue::Function(callback) => {
                            let id = format!("map#{}", next_map_action.get());
                            next_map_action.set(next_map_action.get() + 1);
                            let actions: LuaTable = lua.globals().get("_rift_actions")?;
                            let entry = lua.create_table()?;
                            entry.set("fn", callback)?;
                            if let Some(name) = &plugin_name {
                                entry.set("plugin", name.as_str())?;
                            }
                            actions.set(id.as_str(), entry)?;
                            format!("plugin:{id}")
                        }
                        other => {
                            return Err(LuaError::RuntimeError(format!(
                                "rift.map: action must be a string or function, got {}",
                                other.type_name()
                            )))
                        }
                    };
                    sh.borrow_mut().mutations.push(PluginMutation::MapKey {
                        mode: mode.clone(),
                        keys: keys.clone(),
                        action,
                    });
                    if let Some(ref name) = plugin_name {
                        let plugin_keymaps: LuaTable = lua.globals().get("_rift_plugin_keymaps")?;
                        let list: Option<LuaTable> = plugin_keymaps.get(name.as_str())?;
                        let list = if let Some(t) = list {
                            t
                        } else {
                            let t = lua.create_table()?;
                            plugin_keymaps.set(name.as_str(), t.clone())?;
                            t
                        };
                        let entry = lua.create_table()?;
                        entry.set("mode", mode)?;
                        entry.set("keys", keys)?;
                        list.push(entry)?;
                    }
                    Ok(())
                },
            )?;
            api.set("map", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (row, opts): (usize, Option<LuaTable>)| {
                let buf = opts_target(&sh, opts.as_ref())?;
                let mut s = sh.borrow_mut();
                s.queue_buffer_change(PluginMutation::CenterOnLine {
                    buf: DocumentId::new(buf),
                    row,
                });
                Ok(())
            })?;
            api.set("center_on_line", f)?;
        }

        // rift.unmap(mode, keys) - remove a key binding
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (mode, keys): (String, String)| {
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::UnmapKey { mode, keys });
                Ok(())
            })?;
            api.set("unmap", f)?;
        }

        Ok(())
    }

    fn register_windows(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.windows - window management sub-table
        let windows = lua.create_table()?;

        // rift.windows.move(direction) - move focused window in a direction
        // direction: "left" | "right" | "up" | "down"
        {
            let sh = Rc::clone(shared);
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
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::MoveWindow { direction });
                Ok(())
            })?;
            windows.set("move", f)?;
        }

        // rift.windows.exchange() - swap focused window contents with previously focused window
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                sh.borrow_mut().mutations.push(PluginMutation::SwapWindows);
                Ok(())
            })?;
            windows.set("exchange", f)?;
        }

        // rift.windows.focus_prev() - focus the previously focused window
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::FocusPreviousWindow);
                Ok(())
            })?;
            windows.set("focus_prev", f)?;
        }

        Self::register_window_queries(lua, &windows, shared)?;
        api.set("windows", windows)?;
        Ok(())
    }

    fn register_window_queries(
        lua: &Lua,
        windows: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.windows.list() -> array of { id, buf, row, col, rows, cols }
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, ()| {
                let s = sh.borrow_mut();
                let t = lua.create_table()?;
                for (i, w) in s.win_list.iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set("id", lua_int(w.id))?;
                    entry.set("buf", lua_int(w.buf))?;
                    entry.set("row", lua_int(w.row))?;
                    entry.set("col", lua_int(w.col))?;
                    entry.set("rows", lua_int(w.rows))?;
                    entry.set("cols", lua_int(w.cols))?;
                    t.set(i + 1, entry)?;
                }
                Ok(t)
            })?;
            windows.set("list", f)?;
        }

        // rift.windows.current() -> integer window id of the focused window
        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |_, ()| Ok(lua_int(sh.borrow_mut().focused_win_id)))?;
            windows.set("current", f)?;
        }

        // rift.windows.previous() -> integer window id of the previously focused window, or nil
        {
            let sh = Rc::clone(shared);
            let f =
                lua.create_function(move |_, ()| Ok(sh.borrow_mut().previous_win_id.map(lua_int)))?;
            windows.set("previous", f)?;
        }

        // rift.windows.navigate(direction) - move focus to adjacent window
        // direction: "left" | "right" | "up" | "down"
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, dir: String| {
                use crate::command_line::commands::types::SplitSubcommand;
                use crate::split::{navigation::Direction, tree::SplitDirection};
                let dir = match dir.as_str() {
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
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::ExecAction(Action::Editor(
                        EditorAction::SplitWindow {
                            direction: SplitDirection::Horizontal,
                            subcommand: SplitSubcommand::Navigate(dir),
                        },
                    )));
                Ok(())
            })?;
            windows.set("navigate", f)?;
        }

        Ok(())
    }

    fn register_filetypes(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.register_filetype(ext, lang_name) - map a file extension to a language name
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, (ext, lang_name): (String, String)| {
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::RegisterFiletype { ext, lang_name });
                Ok(())
            })?;
            api.set("register_filetype", f)?;
        }

        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(
                move |_, (lang_name, so_path, fn_name): (String, String, String)| {
                    sh.borrow_mut()
                        .mutations
                        .push(PluginMutation::RegisterGrammar {
                            lang_name,
                            so_path,
                            fn_name,
                        });
                    Ok(())
                },
            )?;
            api.set("register_grammar", f)?;
        }

        Ok(())
    }

    // rift.lsp - Language Server Protocol sub-table
    #[cfg(feature = "lsp")]
    fn register_lsp(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        let lsp = lua.create_table()?;

        // rift.lsp.register({ language, command, args?, extensions?, root_markers?, capabilities? })
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, tbl: LuaTable| {
                let language: String = tbl.get("language")?;
                let command: String = tbl.get("command")?;
                let args: Vec<String> = tbl.get::<Option<LuaTable>>("args")?.map_or_default(|t| {
                    t.sequence_values::<String>()
                        .filter_map(std::result::Result::ok)
                        .collect()
                });
                tbl.get::<Option<LuaTable>>("extensions")?;
                let root_markers: Vec<String> =
                    tbl.get::<Option<LuaTable>>("root_markers")?.map_or_else(
                        || vec![".git".to_string()],
                        |t| {
                            t.sequence_values::<String>()
                                .filter_map(std::result::Result::ok)
                                .collect()
                        },
                    );
                let capabilities: Vec<crate::lsp::config::LspCapability> = tbl
                    .get::<Option<LuaTable>>("capabilities")?
                    .map_or_default(|t| {
                        t.sequence_values::<String>()
                            .filter_map(std::result::Result::ok)
                            .filter_map(|s| crate::lsp::config::LspCapability::parse(&s))
                            .collect()
                    });
                let initialization_options = tbl
                    .get::<Option<LuaValue>>("initialization_options")?
                    .and_then(|v| lua_to_json(v).ok());
                let keep_alive = tbl.get::<Option<bool>>("keep_alive")?.unwrap_or(true);
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::LspRegisterServer {
                        language,
                        config: crate::lsp::config::LspServerConfig {
                            command,
                            args,
                            root_markers,
                            capabilities,
                            initialization_options,
                            keep_alive,
                        },
                    });
                Ok(())
            })?;
            lsp.set("register", f)?;
        }

        Self::register_lsp_actions(lua, &lsp, shared)?;
        Self::register_lsp_diagnostic_query(lua, &lsp, shared)?;
        api.set("lsp", lsp)?;
        Ok(())
    }

    #[cfg(feature = "lsp")]
    fn register_lsp_actions(
        lua: &Lua,
        lsp: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        for (name, action) in [
            ("goto_definition", EditorAction::LspGotoDefinition),
            ("references", EditorAction::LspReferences),
            ("hover", EditorAction::LspHover),
            ("rename_dialog", EditorAction::LspRename),
            ("format", EditorAction::LspFormat),
            ("code_action", EditorAction::LspCodeAction),
            ("diagnostic_next", EditorAction::LspDiagnosticNext),
            ("diagnostic_prev", EditorAction::LspDiagnosticPrev),
            ("diagnostics_panel", EditorAction::LspDiagnosticsPanel),
        ] {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, ()| {
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::ExecAction(Action::Editor(action.clone())));
                Ok(())
            })?;
            lsp.set(name, f)?;
        }

        // rift.lsp.rename(new_name)
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |_, new_name: String| {
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::LspRename { new_name });
                Ok(())
            })?;
            lsp.set("rename", f)?;
        }

        Ok(())
    }

    #[cfg(feature = "lsp")]
    fn register_lsp_diagnostic_query(
        lua: &Lua,
        lsp: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        // rift.lsp.get_diagnostics([uri]) -> array of {line, col, severity, message}
        // uri defaults to the active file; severity: 1=error, 2=warning, 3=info, 4=hint
        {
            let sh = Rc::clone(shared);
            let f = lua.create_function(move |lua, uri: Option<String>| {
                let s = sh.borrow();
                let result = lua.create_table()?;
                let lookup_key = if let Some(u) = uri {
                    Some(u)
                } else {
                    s.snapshot(s.current_id())
                        .and_then(|snap| snap.meta.file_path.as_deref())
                        .map(|p| {
                            format!("file:///{}", p.replace('\\', "/").trim_start_matches('/'))
                        })
                };
                if let Some(key) = lookup_key {
                    if let Some(diags) = s.lsp_diagnostics.get(&key) {
                        for (i, (line, col, severity, message)) in diags.iter().enumerate() {
                            let entry = lua.create_table()?;
                            entry.set("line", i64::from(*line) + 1)?; // 1-indexed for Lua
                            entry.set("col", i64::from(*col))?;
                            entry.set("severity", i64::from(*severity))?;
                            entry.set("message", message.as_str())?;
                            result.set(i + 1, entry)?;
                        }
                    }
                }
                Ok(result)
            })?;
            lsp.set("get_diagnostics", f)?;
        }

        Ok(())
    }

    // Stand-in rift.lsp when `lsp` is off: entry points stay callable
    // (as no-ops) so a plugin referencing rift.lsp.* doesn't hard-error.
    #[cfg(not(feature = "lsp"))]
    fn register_lsp_stub(lua: &Lua, api: &LuaTable) -> LuaResult<()> {
        let lsp = lua.create_table()?;
        let noop = lua.create_function(|_, _: LuaMultiValue| -> LuaResult<()> { Ok(()) })?;
        for name in [
            "register",
            "goto_definition",
            "references",
            "hover",
            "rename",
            "rename_dialog",
            "diagnostics_panel",
            "format",
            "code_action",
            "diagnostic_next",
            "diagnostic_prev",
        ] {
            lsp.set(name, noop.clone())?;
        }
        let empty_diagnostics =
            lua.create_function(|lua, _: LuaMultiValue| -> LuaResult<LuaTable> {
                lua.create_table()
            })?;
        lsp.set("get_diagnostics", empty_diagnostics)?;
        api.set("lsp", lsp)?;
        Ok(())
    }

    fn remove_unsafe_stdlib(lua: &Lua) -> LuaResult<()> {
        let globals = lua.globals();
        let io: LuaTable = globals.get("io")?;
        io.set("popen", LuaNil)?;
        let os: LuaTable = globals.get("os")?;
        for name in ["exit", "remove", "rename", "tmpname"] {
            os.set(name, LuaNil)?;
        }
        let package: LuaTable = globals.get("package")?;
        package.set("loadlib", LuaNil)?;
        package.set("cpath", LuaNil)?;
        let searchers: LuaTable = package.get("searchers")?;
        for index in (3..=searchers.raw_len()).rev() {
            searchers.raw_set(index, LuaNil)?;
        }
        Ok(())
    }

    fn register_package_path(lua: &Lua, api: &LuaTable) -> LuaResult<()> {
        {
            let f = lua.create_function(|lua, path: String| {
                let path = path.replace('\\', "/");
                let package: LuaTable = lua.globals().get("package")?;
                let current: String = package.get("path")?;
                package.set("path", format!("{current};{path}/?.lua"))
            })?;
            api.set("add_package_path", f)?;
        }

        Ok(())
    }

    fn register_buffer_kind(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let register = lua.create_function(move |lua, (name, opts): (String, LuaTable)| {
                let mut s = sh.borrow_mut();
                if !s.is_active {
                    return Err(LuaError::RuntimeError(
                        "cannot register buffer kind: plugin generation is retired".to_string(),
                    ));
                }

                if name.trim().is_empty() {
                    return Err(LuaError::RuntimeError(
                        "register_buffer_kind: name must not be empty".to_string(),
                    ));
                }

                let on_close: Option<LuaFunction> = opts.get("on_close")?;
                if on_close.is_none() {
                    return Err(LuaError::RuntimeError(
                        "register_buffer_kind requires on_close".to_string(),
                    ));
                }

                let read_only_opt: Option<String> = opts.get("read_only")?;
                if let Some(ro) = &read_only_opt {
                    match ro.as_str() {
                        "readonly" | "read_only" | "fixed" | "fixed_readonly" | "writable"
                        | "fixed_writable" | "override" | "document_override" => {}
                        other => {
                            return Err(LuaError::RuntimeError(format!(
                                "invalid read_only policy: '{other}'"
                            )));
                        }
                    }
                }

                let close_opt: Option<String> = opts.get("close")?;
                if let Some(cl) = &close_opt {
                    match cl.as_str() {
                        "confirm" | "confirm_dirty" | "discard" | "discard_dirty" => {}
                        other => {
                            return Err(LuaError::RuntimeError(format!(
                                "invalid close policy: '{other}'"
                            )));
                        }
                    }
                }

                let key_fallback_opt: Option<String> = opts.get("key_fallback")?;
                if let Some(kf) = &key_fallback_opt {
                    match kf.as_str() {
                        "normal" | "global" | "none" => {}
                        other => {
                            return Err(LuaError::RuntimeError(format!(
                                "invalid key_fallback policy: '{other}'"
                            )));
                        }
                    }
                }

                let kinds: LuaTable = lua.globals().get("_rift_buffer_kinds")?;
                let entry = lua.create_table()?;
                entry.set("opts", opts.clone())?;
                if let Some(plugin) = s.current_plugin.as_deref() {
                    entry.set("plugin", plugin)?;
                }
                kinds.set(name.as_str(), entry)?;
                s.mutations.push(PluginMutation::RegisterBufferKind {
                    name,
                    read_only: read_only_opt,
                    close: close_opt,
                    key_fallback: key_fallback_opt,
                    display_name: opts.get("display_name")?,
                    help_lines: opts
                        .get::<Option<LuaTable>>("help_lines")?
                        .map(|lines| lines.sequence_values::<String>().collect())
                        .transpose()?,
                    has_on_close: true,
                    has_on_action: opts.get::<Option<LuaFunction>>("on_action")?.is_some(),
                    has_on_save: opts.get::<Option<LuaFunction>>("on_save")?.is_some(),
                });
                Ok(())
            })?;
            api.set("register_buffer_kind", register)?;
        }
        Ok(())
    }

    fn register_buffer_create(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let create =
                lua.create_function(move |lua, (kind, opts): (String, Option<LuaTable>)| {
                    let title = opts
                        .as_ref()
                        .and_then(|table| table.get::<Option<String>>("title").ok())
                        .flatten();
                    let lines: Vec<String> = opts
                        .as_ref()
                        .and_then(|table| table.get::<Option<LuaTable>>("lines").ok())
                        .flatten()
                        .map(|table| table.sequence_values::<String>().collect())
                        .transpose()?
                        .unwrap_or_default();
                    let mut vars = Vec::new();
                    if let Some(table) = opts
                        .as_ref()
                        .and_then(|table| table.get::<Option<LuaTable>>("vars").ok())
                        .flatten()
                    {
                        for pair in table.pairs::<String, LuaValue>() {
                            let (key, value) = pair?;
                            vars.push((key, value_from_lua(&value)?));
                        }
                    }
                    let kinds: LuaTable = lua.globals().get("_rift_buffer_kinds")?;
                    let read_only =
                        if let Some(entry) = kinds.get::<Option<LuaTable>>(kind.as_str())? {
                            let options: LuaTable = entry.get("opts")?;
                            matches!(
                                options.get::<Option<String>>("read_only")?.as_deref(),
                                Some("readonly" | "read_only" | "fixed" | "fixed_readonly")
                            )
                        } else {
                            false
                        };
                    let mut s = sh.borrow_mut();
                    let id = s.create_local_buf(&kind, title.as_deref(), &lines)?;
                    if let Some(entry) = s.buf_list.last_mut() {
                        entry.is_read_only = read_only;
                    }
                    let local_vars = s.buffer_vars.entry(id).or_default();
                    local_vars.extend(vars.iter().cloned());
                    if let Some(title) = &title {
                        local_vars.insert(
                            "title".to_string(),
                            crate::annotations::value::Value::Str(title.clone()),
                        );
                    }
                    s.synced.buffer_vars = None;
                    s.mutations.push(PluginMutation::CreateBuffer {
                        id: DocumentId::new(id),
                        kind,
                        title,
                        lines,
                        vars,
                    });
                    Ok(id)
                })?;
            api.set("create_buffer", create)?;
        }

        {
            let sh = Rc::clone(shared);
            let map = lua.create_function(
                move |_, (kind, _, keys, action): (String, String, String, String)| {
                    let mut s = sh.borrow_mut();
                    parse_action("rift.map_buffer_kind", &action)?;
                    if !s.is_active {
                        return Err(LuaError::RuntimeError(
                            "cannot map buffer kind: plugin generation is retired".to_string(),
                        ));
                    }
                    s.mutations
                        .push(PluginMutation::MapKeyKind { kind, keys, action });
                    Ok(())
                },
            )?;
            api.set("map_buffer_kind", map)?;
        }

        {
            let sh = Rc::clone(shared);
            let unmap =
                lua.create_function(move |_, (kind, _, keys): (String, String, String)| {
                    let mut s = sh.borrow_mut();
                    if !s.is_active {
                        return Err(LuaError::RuntimeError(
                            "cannot unmap buffer kind: plugin generation is retired".to_string(),
                        ));
                    }
                    s.mutations
                        .push(PluginMutation::UnmapKeyKind { kind, keys });
                    Ok(())
                })?;
            api.set("unmap_buffer_kind", unmap)?;
        }
        Ok(())
    }

    fn register_buffer_vars(
        lua: &Lua,
        api: &LuaTable,
        shared: &Rc<RefCell<LuaSharedState>>,
    ) -> LuaResult<()> {
        {
            let sh = Rc::clone(shared);
            let get_var =
                lua.create_function(move |lua, (buf_id, key): (Option<u64>, String)| {
                    let target_id = buf_or_current(&sh, buf_id)?;
                    let s = sh.borrow_mut();
                    if !s.is_active {
                        return Ok(LuaValue::Nil);
                    }
                    let val = s
                        .buffer_vars
                        .get(&target_id)
                        .and_then(|vars| vars.get(&key))
                        .cloned();
                    drop(s);
                    val.map_or_else(|| Ok(LuaValue::Nil), |v| value_into_lua(v, lua))
                })?;
            api.set("get_buffer_var", get_var)?;
        }

        {
            let sh = Rc::clone(shared);
            let set_var = lua.create_function(
                move |_, (buf_id, key, value): (Option<u64>, String, LuaValue)| {
                    let target_id = buf_or_current(&sh, buf_id)?;
                    let val = value_from_lua(&value)?;
                    let mut s = sh.borrow_mut();
                    if !s.is_active {
                        return Err(LuaError::RuntimeError(
                            "cannot set buffer var: plugin generation is retired".to_string(),
                        ));
                    }
                    s.buffer_vars
                        .entry(target_id)
                        .or_default()
                        .insert(key.clone(), val.clone());
                    s.synced.buffer_vars = None;
                    s.mutations.push(PluginMutation::SetBufferVar {
                        buf: DocumentId::new(target_id),
                        key,
                        value: val,
                    });
                    Ok(())
                },
            )?;
            api.set("set_buffer_var", set_var)?;
        }

        Ok(())
    }

    fn finish(
        lua: Lua,
        api: LuaTable,
        shared: Rc<RefCell<LuaSharedState>>,
        hook_deadline: Rc<RefCell<Option<std::time::Instant>>>,
        generation: crate::plugin::PluginGeneration,
    ) -> LuaResult<Self> {
        lua.globals().set("rift", api)?;

        // Embedded Lua prelude - convenience wrappers that don't need Rust bindings.
        lua.load(PRELUDE).set_name("rift:prelude").exec()?;

        Ok(Self {
            lua,
            shared,
            hook_deadline,
            generation,
        })
    }

    /// The owning plugin generation for this Lua VM.
    #[must_use]
    pub(super) const fn generation(&self) -> crate::plugin::PluginGeneration {
        self.generation
    }

    /// Mark this Lua host as retiring, rejecting new mutations.
    pub(super) fn mark_retiring(&self) {
        let mut s = self.shared.borrow_mut();
        s.is_active = false;
        for (_, (_, request)) in s.save_holds.drain() {
            request.revoke();
        }
    }

    pub(super) fn synced_keys(&self) -> SyncKeys {
        self.shared.borrow().synced
    }

    pub(super) fn take_creation_ids(&self) -> Vec<u64> {
        std::mem::take(&mut self.shared.borrow_mut().creation_ids)
    }

    pub(super) fn take_divergence(&self) -> Vec<u64> {
        self.shared.borrow_mut().diverged.drain().collect()
    }

    /// Update the buffer snapshot that `rift.get_lines()` and friends read.
    /// Call this before dispatching an event.
    pub(super) fn update_state(&self, update: StateUpdate<'_>) {
        fn replace<T>(slot: &mut T, synced: &mut Option<u64>, value: Option<T>, key: Option<u64>) {
            if let Some(value) = value {
                *slot = value;
                *synced = key;
            }
        }

        let mut guard = self.shared.borrow_mut();
        let s = &mut *guard;
        s.buf_id = update.buf_id;
        s.creation_ids = update.creation_ids;
        if let Some(bufs) = update.bufs {
            s.bufs
                .retain(|id, _| bufs.iter().any(|(live, _)| live == id));
            for (id, fresh) in bufs {
                s.bufs.entry(id).or_default().apply(fresh);
            }
        }
        s.mode = update.mode.to_string();
        s.window_size = update.window_size;
        s.scroll = update.scroll;
        s.focused_win_id = update.focused_win_id;
        s.previous_win_id = update.previous_win_id;
        let keys = update.keys;
        let synced = &mut s.synced;
        replace(
            &mut s.buf_list,
            &mut synced.buf_list,
            update.buf_list,
            keys.buf_list,
        );
        replace(
            &mut s.win_list,
            &mut synced.win_list,
            update.win_list,
            keys.win_list,
        );
        replace(
            &mut s.lsp_diagnostics,
            &mut synced.lsp_diagnostics,
            update.lsp_diagnostics,
            keys.lsp_diagnostics,
        );
        replace(
            &mut s.buffer_vars,
            &mut synced.buffer_vars,
            update.buffer_vars,
            keys.buffer_vars,
        );
        replace(
            &mut s.user_settings,
            &mut synced.user_settings,
            update.user_settings,
            keys.user_settings,
        );
        #[cfg(feature = "treesitter")]
        if let Some(syntax) = update.syntax {
            let mut synced = std::mem::take(&mut s.syntax);
            s.syntax = syntax
                .into_iter()
                .filter_map(|(doc, fresh)| Some((doc, fresh.or_else(|| synced.remove(&doc))?)))
                .collect();
        }
    }

    /// Budget for one automatic (non-user-initiated) Lua call - event
    /// handlers, not explicit commands/actions the user just triggered.
    const AUTO_DISPATCH_BUDGET: std::time::Duration = std::time::Duration::from_millis(150);

    const USER_CALL_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

    /// Runs `f` with `hook_deadline` set so the VM hook can abort it past
    /// budget. Saves/restores the prior deadline for reentrant dispatch.
    fn call_budgeted<T>(
        &self,
        budget: std::time::Duration,
        f: impl FnOnce() -> LuaResult<T>,
    ) -> LuaResult<T> {
        let deadline = std::time::Instant::now() + budget;
        let prev = self.hook_deadline.borrow_mut().replace(deadline);
        let result = f();
        *self.hook_deadline.borrow_mut() = prev;
        result
    }

    fn call_plugin<T>(
        &self,
        plugin: Option<&str>,
        budget: std::time::Duration,
        f: impl FnOnce() -> LuaResult<T>,
    ) -> LuaResult<T> {
        let started = std::time::Instant::now();
        let result = self.with_current_plugin(plugin, || self.call_budgeted(budget, f));
        if let Some(plugin) = plugin {
            self.record_budget_outcome(plugin, &result, started.elapsed() >= budget);
        }
        result
    }

    fn with_current_plugin<T>(&self, plugin: Option<&str>, f: impl FnOnce() -> T) -> T {
        let previous = std::mem::replace(
            &mut self.shared.borrow_mut().current_plugin,
            plugin.map(str::to_owned),
        );
        let result = f();
        self.shared.borrow_mut().current_plugin = previous;
        result
    }

    const BUDGET_FAILURE_LIMIT: u32 = 3;

    fn record_budget_outcome<T>(&self, plugin: &str, result: &LuaResult<T>, over_budget: bool) {
        let disabled = {
            let mut guard = self.shared.borrow_mut();
            let s = &mut *guard;
            match result {
                Ok(_) => {
                    if let Some(health) = s.plugin_health.get_mut(plugin) {
                        health.budget_failures = 0;
                    }
                    false
                }
                Err(error) if over_budget => {
                    let health = s.plugin_health.entry(plugin.to_owned()).or_default();
                    health.budget_failures += 1;
                    health.last_budget_error = Some(error.to_string());
                    let disable =
                        health.budget_failures >= Self::BUDGET_FAILURE_LIMIT && !health.disabled;
                    if disable {
                        health.disabled = true;
                        s.mutations.push(PluginMutation::Notify {
                            message: format!(
                                "plugin {plugin} was disabled after {} consecutive time-budget overruns; run rift.plugins.enable({plugin:?}) to turn it back on",
                                Self::BUDGET_FAILURE_LIMIT
                            ),
                            level: NotificationType::Warning,
                        });
                    }
                    disable
                }
                Err(_) => false,
            }
        };
        if disabled {
            let _ = Self::stop_plugin_jobs(&self.lua, &self.shared, plugin);
        }
    }

    fn is_disabled(&self, plugin: Option<&str>) -> bool {
        self.shared.borrow().plugin_disabled(plugin)
    }

    fn callback_entry(
        lua: &Lua,
        shared: &RefCell<LuaSharedState>,
        callback: LuaFunction,
    ) -> LuaResult<LuaTable> {
        let entry = lua.create_table()?;
        entry.set("fn", callback)?;
        if let Some(plugin) = shared.borrow().current_plugin.as_deref() {
            entry.set("plugin", plugin)?;
        }
        Ok(entry)
    }

    fn entry_plugin(entry: &LuaTable) -> Option<String> {
        entry.get::<Option<String>>("plugin").ok().flatten()
    }

    fn plugin_error(plugin: Option<&str>, origin: &str, error: &dyn std::fmt::Display) -> String {
        let stem = plugin
            .and_then(|p| std::path::Path::new(p).file_stem())
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("lua");
        format!("[{stem}:{origin}] {error}")
    }

    fn report_error(&self, plugin: Option<&str>, origin: &str, error: &dyn std::fmt::Display) {
        self.shared
            .borrow_mut()
            .mutations
            .push(PluginMutation::Notify {
                message: Self::plugin_error(plugin, origin, error),
                level: NotificationType::Error,
            });
    }

    /// Dispatch an `EventPayload` to all registered Lua handlers.
    /// Returns error strings for any handlers that raised a Lua error.
    #[must_use]
    pub(super) fn dispatch_event(&self, event: &EventPayload) -> Vec<String> {
        match event {
            EventPayload::BufClose { buf } => self.revoke_save_hold(*buf),
            EventPayload::EditorQuit => {
                for (_, (_, request)) in self.shared.borrow_mut().save_holds.drain() {
                    request.revoke();
                }
            }
            _ => {}
        }
        self.dispatch_event_impl(event, None)
    }

    pub(super) fn dispatch_save_pre(
        &self,
        event: &EventPayload,
        buf: DocumentId,
    ) -> (Vec<String>, SaveDecision) {
        self.revoke_save_hold(buf);
        let mut decision = SaveDecision::Proceed;
        let errors = self.dispatch_event_impl(event, Some((buf, &mut decision)));
        (errors, decision)
    }

    fn revoke_save_hold(&self, buf: DocumentId) {
        if let Some((_, request)) = self.shared.borrow_mut().save_holds.remove(&buf) {
            request.revoke();
        }
    }

    fn dispatch_event_impl(
        &self,
        event: &EventPayload,
        mut save_pre: Option<(DocumentId, &mut SaveDecision)>,
    ) -> Vec<String> {
        let handlers: LuaTable = match self.lua.globals().get("_rift_handlers") {
            Ok(t) => t,
            Err(_) => return vec![],
        };
        let list: Option<LuaTable> = match handlers.get(event.name()) {
            Ok(v) => v,
            Err(_) => return vec![],
        };
        let Some(list) = list else {
            return vec![];
        };
        let ev_table = if save_pre.is_none() {
            match event.to_lua_table(&self.lua) {
                Ok(t) => Some(t),
                Err(e) => {
                    return vec![format!(
                        "failed to build event table for {}: {}",
                        event.name(),
                        e
                    )];
                }
            }
        } else {
            None
        };

        // Snapshot the handler list before dispatch so handlers that call
        // rift.on/rift.off mid-pass only affect the next dispatch.
        let snapshot: Vec<LuaResult<LuaTable>> = list.sequence_values::<LuaTable>().collect();
        let mut errors = Vec::new();
        for entry in snapshot {
            match entry {
                Ok(entry) => {
                    let plugin = Self::entry_plugin(&entry);
                    if self.is_disabled(plugin.as_deref()) {
                        continue;
                    }
                    let f: LuaFunction = match entry.get("fn") {
                        Ok(f) => f,
                        Err(e) => {
                            errors.push(Self::plugin_error(
                                plugin.as_deref(),
                                event.name(),
                                &format_args!("bad handler entry: {e}"),
                            ));
                            continue;
                        }
                    };
                    let call_result = if let Some((buf, decision)) = save_pre.as_mut() {
                        let can_hold = match &**decision {
                            SaveDecision::Proceed => true,
                            SaveDecision::Hold(hold) => !hold.is_held(),
                            SaveDecision::Cancel => false,
                        };
                        entry.get("slot").and_then(|slot| {
                            self.call_save_pre_handler(
                                event,
                                *buf,
                                slot,
                                plugin.as_deref(),
                                &f,
                                can_hold,
                            )
                            .map(|requested| {
                                if !matches!(requested, SaveDecision::Proceed) {
                                    **decision = requested;
                                }
                            })
                        })
                    } else {
                        self.call_plugin(plugin.as_deref(), Self::AUTO_DISPATCH_BUDGET, || {
                            f.call::<()>(ev_table.clone())
                        })
                    };
                    if let Err(e) = call_result {
                        errors.push(Self::plugin_error(plugin.as_deref(), event.name(), &e));
                    }
                }
                Err(e) => errors.push(Self::plugin_error(
                    None,
                    event.name(),
                    &format_args!("bad handler: {e}"),
                )),
            }
        }
        errors
    }

    fn call_save_pre_handler(
        &self,
        event: &EventPayload,
        buf: DocumentId,
        slot: i64,
        plugin: Option<&str>,
        f: &LuaFunction,
        can_hold: bool,
    ) -> LuaResult<SaveDecision> {
        let ev = event.to_lua_table(&self.lua)?;
        let decision = Rc::new(RefCell::new(Some(SaveDecision::Proceed)));
        let cancel = Rc::clone(&decision);
        ev.set(
            "cancel",
            self.lua.create_function(move |_, ()| {
                let mut cancel = cancel.borrow_mut();
                if matches!(*cancel, None | Some(SaveDecision::Cancel)) {
                    return Ok(false);
                }
                *cancel = Some(SaveDecision::Cancel);
                Ok(true)
            })?,
        )?;
        let hold = Rc::clone(&decision);
        let shared = Rc::clone(&self.shared);
        let owner = plugin.map(str::to_owned);
        ev.set(
            "hold",
            self.lua.create_function(move |lua, ()| {
                let mut decision = hold.borrow_mut();
                if !can_hold || !matches!(*decision, Some(SaveDecision::Proceed)) {
                    return Ok(None);
                }
                let slots: LuaTable = lua.globals().get("_rift_slot_events")?;
                if !slots.contains_key(slot)? {
                    return Ok(None);
                }
                let (hold, request) = SaveHold::new(buf);
                let continuation = request.clone();
                let shared_resume = Rc::clone(&shared);
                let resume = lua.create_function(move |_, ()| {
                    if !continuation.queue_resume() {
                        return Ok(false);
                    }
                    shared_resume
                        .borrow_mut()
                        .mutations
                        .push(PluginMutation::ResumeSave {
                            request: continuation.clone(),
                        });
                    Ok(true)
                })?;
                shared
                    .borrow_mut()
                    .save_holds
                    .insert(buf, (owner.clone(), request));
                *decision = Some(SaveDecision::Hold(hold));
                Ok(Some(resume))
            })?,
        )?;

        let result = self.call_plugin(plugin, Self::AUTO_DISPATCH_BUDGET, || f.call::<()>(ev));
        let requested = decision.borrow_mut().take().unwrap_or_default();
        result.map(|()| requested)
    }

    /// Invoke the Lua handler registered for an annotation (kind, verb) with a
    fn invoke_buffer_callback(
        &self,
        buf_id: u64,
        kind: &str,
        field: &str,
        action: Option<&str>,
    ) -> bool {
        {
            let s = self.shared.borrow_mut();
            if !s.is_active {
                return false;
            }
        }
        let kinds: LuaTable = match self.lua.globals().get("_rift_buffer_kinds") {
            Ok(table) => table,
            Err(_) => return false,
        };
        let entry: LuaTable = match kinds.get::<Option<LuaTable>>(kind) {
            Ok(Some(entry)) => entry,
            _ => return false,
        };
        let plugin = Self::entry_plugin(&entry);
        if self.is_disabled(plugin.as_deref()) {
            return false;
        }
        let Ok(opts) = entry.get::<LuaTable>("opts") else {
            return false;
        };
        let callback: LuaFunction = match opts.get::<Option<LuaFunction>>(field) {
            Ok(Some(callback)) => callback,
            _ => return false,
        };
        let Ok(context) = self.lua.create_table() else {
            return false;
        };
        if context.set("buffer", buf_id).is_err() || context.set("kind", kind).is_err() {
            return false;
        }
        if let Some(action) = action {
            if context.set("action", action).is_err() {
                return false;
            }
        }

        if let Err(error) = self.call_plugin(plugin.as_deref(), Self::AUTO_DISPATCH_BUDGET, || {
            callback.call::<()>(context)
        }) {
            self.report_error(plugin.as_deref(), &format!("{kind}.{field}"), &error);
        }
        true
    }

    #[must_use]
    pub(super) fn invoke_buffer_action(&self, buf_id: u64, kind: &str, action: &str) -> bool {
        self.invoke_buffer_callback(buf_id, kind, "on_action", Some(action))
    }

    #[must_use]
    pub(super) fn invoke_buffer_save(&self, buf_id: u64, kind: &str) -> bool {
        self.invoke_buffer_callback(buf_id, kind, "on_save", None)
    }

    #[must_use]
    pub(super) fn invoke_buffer_close(&self, buf_id: u64, kind: &str) -> bool {
        self.invoke_buffer_callback(buf_id, kind, "on_close", None)
    }

    pub(super) fn deliver_job_event(&self, job: u64, event: PluginJobEvent) {
        let Ok(jobs) = self.lua.globals().get::<LuaTable>("_rift_jobs") else {
            return;
        };
        let Ok(Some(entry)) = jobs.get::<Option<LuaTable>>(job) else {
            return;
        };
        if event.is_final() {
            let _ = jobs.set(job, LuaNil);
        }
        let plugin = Self::entry_plugin(&entry);
        if self.is_disabled(plugin.as_deref()) {
            return;
        }
        let (field, arg) = match event {
            PluginJobEvent::Output(stream, lines) => {
                let field = match stream {
                    JobStream::Stdout => "on_stdout",
                    JobStream::Stderr => "on_stderr",
                };
                let Ok(lines) = self.lua.create_sequence_from(lines) else {
                    return;
                };
                (field, LuaValue::Table(lines))
            }
            PluginJobEvent::Exit(code) => ("on_exit", LuaValue::Integer(i64::from(code))),
            PluginJobEvent::Failed(error) => {
                self.report_error(plugin.as_deref(), "job", &error);
                ("on_exit", LuaValue::Integer(-1))
            }
        };
        let Ok(Some(callback)) = entry.get::<Option<LuaFunction>>(field) else {
            return;
        };
        if let Err(error) = self.call_plugin(plugin.as_deref(), Self::AUTO_DISPATCH_BUDGET, || {
            callback.call::<()>(arg)
        }) {
            self.report_error(plugin.as_deref(), &format!("job.{field}"), &error);
        }
    }

    /// context table. Returns `true` if a handler was found and called.
    #[must_use]
    pub(super) fn invoke_annotation_action(
        &self,
        ctx: &crate::plugin::AnnotationActionCtx,
    ) -> bool {
        let handlers: LuaTable = match self.lua.globals().get("_rift_action_handlers") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let key = format!("{}\u{0}{}", ctx.kind, ctx.verb);
        let entry: LuaTable = match handlers.get::<Option<LuaTable>>(key.as_str()) {
            Ok(Some(entry)) => entry,
            _ => return false,
        };
        let plugin = Self::entry_plugin(&entry);
        if self.is_disabled(plugin.as_deref()) {
            return false;
        }
        let Ok(f) = entry.get::<LuaFunction>("fn") else {
            return false;
        };
        let Ok(t) = self.lua.create_table() else {
            return false;
        };
        let _ = t.set("annotation_id", ctx.annotation_id);
        let _ = t.set("kind", ctx.kind.as_str());
        let _ = t.set("verb", ctx.verb.as_str());
        let _ = t.set("position", lua_int(ctx.position));
        let _ = t.set("buffer", lua_int(ctx.buffer));
        if let Ok(p) = value_into_lua(ctx.payload.clone(), &self.lua) {
            let _ = t.set("payload", p);
        }
        if let Ok(p) = value_into_lua(ctx.params.clone(), &self.lua) {
            let _ = t.set("params", p);
        }
        if let Err(error) = self.call_plugin(plugin.as_deref(), Self::USER_CALL_BUDGET, || {
            f.call::<()>(t)
        }) {
            self.report_error(
                plugin.as_deref(),
                &format!("{}.{}", ctx.kind, ctx.verb),
                &error,
            );
        }
        true
    }

    /// Invoke the Lua cursor enter/leave hook for an annotation kind, if registered.
    /// `enter` selects the handler table; true if a handler ran.
    #[must_use]
    pub(super) fn invoke_annotation_hook(
        &self,
        enter: bool,
        ctx: &crate::plugin::AnnotationHoverCtx,
    ) -> bool {
        let (table_name, hook) = if enter {
            ("_rift_enter_handlers", "on_enter")
        } else {
            ("_rift_leave_handlers", "on_leave")
        };
        let handlers: LuaTable = match self.lua.globals().get(table_name) {
            Ok(t) => t,
            Err(_) => return false,
        };
        let entry: LuaTable = match handlers.get::<Option<LuaTable>>(ctx.kind.as_str()) {
            Ok(Some(entry)) => entry,
            _ => return false,
        };
        let plugin = Self::entry_plugin(&entry);
        if self.is_disabled(plugin.as_deref()) {
            return false;
        }
        let Ok(f) = entry.get::<LuaFunction>("fn") else {
            return false;
        };
        let Ok(t) = self.lua.create_table() else {
            return false;
        };
        let _ = t.set("annotation_id", ctx.annotation_id);
        let _ = t.set("kind", ctx.kind.as_str());
        let _ = t.set("position", lua_int(ctx.position));
        let _ = t.set("buffer", lua_int(ctx.buffer));
        if let Ok(p) = value_into_lua(ctx.payload.clone(), &self.lua) {
            let _ = t.set("payload", p);
        }
        if let Err(error) = self.call_plugin(plugin.as_deref(), Self::AUTO_DISPATCH_BUDGET, || {
            f.call::<()>(t)
        }) {
            self.report_error(plugin.as_deref(), &format!("{}.{hook}", ctx.kind), &error);
        }
        true
    }

    /// Execute a plugin command registered via `rift.register_command`.
    /// Returns `true` if a handler was found and called.
    #[must_use]
    pub(super) fn execute_command(&self, name: &str, args: &[String]) -> bool {
        let cmds: LuaTable = match self.lua.globals().get("_rift_commands") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let entry: Option<LuaTable> = match cmds.get(name) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let Some(entry) = entry else {
            return false;
        };
        let plugin = Self::entry_plugin(&entry);
        if self.is_disabled(plugin.as_deref()) {
            return false;
        }
        let handler: LuaFunction = match entry.get("fn") {
            Ok(f) => f,
            Err(_) => return false,
        };
        let Ok(args_table) = self.lua.create_table() else {
            return false;
        };
        for (i, arg) in args.iter().enumerate() {
            let _ = args_table.set(i + 1, arg.as_str());
        }
        if let Err(e) = self.call_plugin(plugin.as_deref(), Self::USER_CALL_BUDGET, || {
            handler.call::<()>(args_table)
        }) {
            self.report_error(plugin.as_deref(), name, &e);
        }
        true
    }

    /// Returns all Lua-registered command names, descriptions, and arg types.
    #[must_use]
    pub(super) fn command_list(&self) -> Vec<(String, String, Option<String>)> {
        registered_commands(&self.lua)
    }

    /// Execute a plugin action registered via `rift.register_action`.
    /// Returns `true` if a handler was found and called.
    #[must_use]
    pub(super) fn execute_action(&self, id: &str) -> bool {
        let actions: LuaTable = match self.lua.globals().get("_rift_actions") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let entry: Option<LuaTable> = match actions.get(id) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let Some(entry) = entry else {
            return false;
        };
        let plugin = Self::entry_plugin(&entry);
        if self.is_disabled(plugin.as_deref()) {
            return false;
        }
        let handler: LuaFunction = match entry.get("fn") {
            Ok(f) => f,
            Err(_) => return false,
        };
        if let Err(e) = self.call_plugin(plugin.as_deref(), Self::USER_CALL_BUDGET, || {
            handler.call::<()>(())
        }) {
            self.report_error(plugin.as_deref(), id, &e);
        }
        true
    }

    /// Load all top-level `.lua` files in `dir`, in lexicographic order (no recursion,
    /// no `package.path` change). Returns error strings; empty means all loaded OK.
    #[must_use]
    pub(super) fn load_dir(&self, dir: &std::path::Path) -> Vec<String> {
        let mut errors = Vec::new();

        let Ok(rd) = std::fs::read_dir(dir) else {
            return errors;
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
    pub(super) fn load_file(&self, path: &std::path::Path) -> Option<String> {
        let name = path.to_string_lossy().replace('\\', "/");
        let error = self.run_plugin_file(path, &name);
        if let Ok(load_errors) = self.lua.globals().get::<LuaTable>("_rift_load_errors") {
            let _ = load_errors.set(name.as_str(), error.as_deref());
        }
        error
    }

    fn run_plugin_file(&self, path: &std::path::Path, name: &str) -> Option<String> {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => return Some(format!("lua: failed to read {}: {}", path.display(), e)),
        };
        let env = match self.plugin_environment(name) {
            Ok(env) => env,
            Err(e) => return Some(format!("lua: {}: {}", path.display(), e)),
        };
        self.with_current_plugin(Some(name), || {
            self.call_budgeted(Self::USER_CALL_BUDGET, || {
                self.exec_plugin_chunk(&src, name, env)
            })
        })
        .err()
        .map(|e| format!("lua: {}: {}", path.display(), e))
    }

    pub(super) fn has_queued_mutations(&self) -> bool {
        !self.shared.borrow().mutations.is_empty()
    }

    pub(super) fn has_annotation_hook(&self, enter: bool, kind: &str) -> bool {
        let table_name = if enter {
            "_rift_enter_handlers"
        } else {
            "_rift_leave_handlers"
        };
        self.lua
            .globals()
            .get::<LuaTable>(table_name)
            .and_then(|handlers| handlers.get::<Option<LuaTable>>(kind))
            .is_ok_and(|entry| {
                entry.is_some_and(|entry| {
                    entry.get::<LuaFunction>("fn").is_ok()
                        && !self.is_disabled(Self::entry_plugin(&entry).as_deref())
                })
            })
    }

    fn exec_plugin_chunk(&self, src: &str, name: &str, env: LuaTable) -> LuaResult<()> {
        let globals = self.lua.globals();
        let previous: LuaValue = globals.get("_rift_current_plugin_file")?;
        globals.set("_rift_current_plugin_file", name)?;
        let result = self
            .lua
            .load(src)
            .set_name(name)
            .set_environment(env)
            .exec();
        globals.set("_rift_current_plugin_file", previous)?;
        result
    }

    fn plugin_environment(&self, name: &str) -> LuaResult<LuaTable> {
        let env = self.lua.create_table()?;
        env.set("_rift_current_plugin_file", name)?;
        let meta = self.lua.create_table()?;
        meta.set("__index", self.lua.globals())?;
        env.set_metatable(Some(meta))?;
        Ok(env)
    }

    #[must_use]
    pub(super) fn drain_mutations(&self) -> Vec<PluginMutation> {
        std::mem::take(&mut self.shared.borrow_mut().mutations)
    }

    /// Execute a string of Lua code. Returns an error string on failure.
    #[must_use]
    pub(super) fn exec(&self, code: &str) -> Option<String> {
        self.call_budgeted(Self::USER_CALL_BUDGET, || self.lua.load(code).exec())
            .err()
            .map(|e| format!("[lua] {e}"))
    }
}

impl std::fmt::Debug for LuaHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaHost").finish_non_exhaustive()
    }
}

#[path = "lua_highlight.rs"]
mod highlight_api;

#[path = "lua_timers.rs"]
mod timer_api;

#[cfg(feature = "treesitter")]
#[path = "lua_treesitter.rs"]
mod treesitter_api;

#[cfg(not(feature = "treesitter"))]
mod treesitter_api {
    use mlua::prelude::*;

    pub(super) fn register(
        lua: &Lua,
        api: &LuaTable,
        _shared: &std::rc::Rc<std::cell::RefCell<super::LuaSharedState>>,
    ) -> LuaResult<()> {
        let ts = lua.create_table()?;
        let meta = lua.create_table()?;
        meta.set(
            "__index",
            lua.create_function(|lua, (_, name): (LuaTable, String)| {
                lua.create_function(move |_, _: LuaMultiValue| -> LuaResult<()> {
                    Err(LuaError::RuntimeError(format!(
                        "rift.treesitter.{name}: tree-sitter support is not compiled in"
                    )))
                })
            })?,
        )?;
        ts.set_metatable(Some(meta))?;
        api.set("treesitter", ts)
    }
}

#[cfg(test)]
#[path = "lua_host_tests.rs"]
mod tests;
