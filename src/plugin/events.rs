//! Editor events fired to the plugin host.

use crate::document::DocumentId;
use crate::mode::Mode;

/// Every significant editor state change produces one of these.
/// The plugin host dispatches them to registered handlers.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// A new buffer was opened (or created). Fired after the document is ready.
    BufOpen {
        buf: DocumentId,
        /// Absolute path, if the buffer has one.
        path: Option<std::path::PathBuf>,
        /// Detected filetype, e.g. `"rust"`, `"python"`. `None` if unknown.
        filetype: Option<String>,
    },

    /// A buffer was closed / removed from the tab list.
    BufClose { buf: DocumentId },

    /// The focused buffer changed (tab switch, split focus, file open).
    BufEnter { buf: DocumentId },

    /// The focused buffer is about to lose focus.
    BufLeave { buf: DocumentId },

    /// A buffer is about to be written to disk.
    BufSavePre {
        buf: DocumentId,
        path: std::path::PathBuf,
    },

    /// A buffer was successfully written to disk.
    BufSavePost {
        buf: DocumentId,
        path: std::path::PathBuf,
    },

    /// A buffer was reloaded from disk (e.g. after an external change).
    BufReload { buf: DocumentId },

    /// Coarse change notification — something in the buffer changed.
    /// Cheap to fire; suitable for tools that just need to know "something changed".
    TextChangedCoarse { buf: DocumentId },

    /// The cursor moved. Row and col are 0-indexed.
    CursorMoved {
        buf: DocumentId,
        row: usize,
        col: usize,
    },

    /// The cursor has been idle for the configured hold threshold.
    CursorHold {
        buf: DocumentId,
        row: usize,
        col: usize,
    },

    /// The editor mode changed.
    ModeChanged { from: Mode, to: Mode },

    /// Fired once, after all plugins are loaded and the first render is done.
    EditorStart,

    /// Fired just before the editor exits.
    EditorQuit,

    /// The terminal was resized.
    WindowResized { rows: u16, cols: u16 },

    /// A plugin-defined event. Use `rift.emit("MyPlugin:Ready", ...)` in Lua.
    UserEvent { name: String },

    /// A split window gained focus.
    WinEnter { win: u64, buf: crate::document::DocumentId },

    /// A split window lost focus.
    WinLeave { win: u64, buf: crate::document::DocumentId },

    /// A window was repositioned via ^WH/J/K/L move.
    WinMoved { win: u64 },

    /// Two windows had their contents swapped via exchange.
    WinSwapped { win1: u64, win2: u64 },
}

impl EditorEvent {
    /// Build a Lua table representing this event's payload.
    pub fn to_lua_table(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
        let t = lua.create_table()?;
        match self {
            EditorEvent::BufOpen {
                buf,
                path,
                filetype,
            } => {
                t.set("buf", *buf as i64)?;
                if let Some(p) = path {
                    t.set("path", p.to_string_lossy().as_ref())?;
                }
                if let Some(ft) = filetype {
                    t.set("filetype", ft.as_str())?;
                }
            }
            EditorEvent::BufClose { buf } => {
                t.set("buf", *buf as i64)?;
            }
            EditorEvent::BufEnter { buf } => {
                t.set("buf", *buf as i64)?;
            }
            EditorEvent::BufLeave { buf } => {
                t.set("buf", *buf as i64)?;
            }
            EditorEvent::BufSavePre { buf, path } => {
                t.set("buf", *buf as i64)?;
                t.set("path", path.to_string_lossy().as_ref())?;
            }
            EditorEvent::BufSavePost { buf, path } => {
                t.set("buf", *buf as i64)?;
                t.set("path", path.to_string_lossy().as_ref())?;
            }
            EditorEvent::BufReload { buf } => {
                t.set("buf", *buf as i64)?;
            }
            EditorEvent::TextChangedCoarse { buf } => {
                t.set("buf", *buf as i64)?;
            }
            EditorEvent::CursorMoved { buf, row, col } => {
                t.set("buf", *buf as i64)?;
                t.set("row", *row as i64 + 1)?; // 1-indexed for Lua
                t.set("col", *col as i64)?;
            }
            EditorEvent::CursorHold { buf, row, col } => {
                t.set("buf", *buf as i64)?;
                t.set("row", *row as i64 + 1)?;
                t.set("col", *col as i64)?;
            }
            EditorEvent::ModeChanged { from, to } => {
                t.set("from", format!("{:?}", from).to_lowercase())?;
                t.set("to", format!("{:?}", to).to_lowercase())?;
            }
            EditorEvent::WindowResized { rows, cols } => {
                t.set("rows", *rows as i64)?;
                t.set("cols", *cols as i64)?;
            }
            EditorEvent::UserEvent { name } => {
                t.set("name", name.as_str())?;
            }
            EditorEvent::EditorStart | EditorEvent::EditorQuit => {}
            EditorEvent::WinEnter { win, buf } => {
                t.set("win", *win as i64)?;
                t.set("buf", *buf as i64)?;
            }
            EditorEvent::WinLeave { win, buf } => {
                t.set("win", *win as i64)?;
                t.set("buf", *buf as i64)?;
            }
            EditorEvent::WinMoved { win } => {
                t.set("win", *win as i64)?;
            }
            EditorEvent::WinSwapped { win1, win2 } => {
                t.set("win1", *win1 as i64)?;
                t.set("win2", *win2 as i64)?;
            }
        }
        Ok(t)
    }

    /// Short name used for handler lookup tables.
    pub fn name(&self) -> &'static str {
        match self {
            EditorEvent::BufOpen { .. } => "BufOpen",
            EditorEvent::BufClose { .. } => "BufClose",
            EditorEvent::BufEnter { .. } => "BufEnter",
            EditorEvent::BufLeave { .. } => "BufLeave",
            EditorEvent::BufSavePre { .. } => "BufSavePre",
            EditorEvent::BufSavePost { .. } => "BufSavePost",
            EditorEvent::BufReload { .. } => "BufReload",
            EditorEvent::TextChangedCoarse { .. } => "TextChangedCoarse",
            EditorEvent::CursorMoved { .. } => "CursorMoved",
            EditorEvent::CursorHold { .. } => "CursorHold",
            EditorEvent::ModeChanged { .. } => "ModeChanged",
            EditorEvent::EditorStart => "EditorStart",
            EditorEvent::EditorQuit => "EditorQuit",
            EditorEvent::WindowResized { .. } => "WindowResized",
            EditorEvent::UserEvent { .. } => "UserEvent",
            EditorEvent::WinEnter { .. } => "WinEnter",
            EditorEvent::WinLeave { .. } => "WinLeave",
            EditorEvent::WinMoved { .. } => "WinMoved",
            EditorEvent::WinSwapped { .. } => "WinSwapped",
        }
    }
}
