//! Editor events fired to the plugin host.

use crate::document::DocumentId;
use crate::mode::Mode;
#[cfg(feature = "plugins")]
use crate::plugin::lua_value::lua_int;

/// Every significant editor state change produces one of these.
/// The plugin host dispatches them to registered handlers.
#[derive(Debug, Clone)]
pub(crate) enum EditorEvent {
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

    /// Coarse change notification for a modified buffer.
    /// Cheap to fire for tools that only need change awareness.
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
    #[cfg(test)]
    WindowResized { rows: u16, cols: u16 },

    /// An LSP server process spawned and connected (before it finishes indexing).
    LspServerConnected {
        language: String,
        server_name: String,
    },

    /// An LSP server finished indexing and is ready for requests.
    LspServerReady {
        language: String,
        server_name: String,
    },

    /// An LSP server sent a work-done progress tick.
    LspProgress {
        language: String,
        server_name: String,
    },

    /// LSP diagnostics were received for a file.
    LspDiagnosticsChanged {
        /// Normalized URI of the file.
        uri: String,
        /// Number of errors (severity 1).
        error_count: usize,
        /// Number of warnings (severity 2).
        warning_count: usize,
    },

    /// A split window gained focus.
    WinEnter {
        win: u64,
        buf: crate::document::DocumentId,
    },

    /// A split window lost focus.
    WinLeave {
        win: u64,
        buf: crate::document::DocumentId,
    },

    /// A window was repositioned via ^WH/J/K/L move.
    WinMoved { win: u64 },

    /// Two windows had their contents swapped via exchange.
    WinSwapped { win1: u64, win2: u64 },
}

impl EditorEvent {
    /// Build a Lua table representing this event's payload.
    ///
    /// # Errors
    /// Returns an error if the table cannot be created or a field cannot be set.
    #[cfg(feature = "plugins")]
    pub(super) fn to_lua_table(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
        let t = lua.create_table()?;
        match self {
            Self::BufOpen {
                buf,
                path,
                filetype,
            } => {
                t.set("buf", lua_int(buf.get()))?;
                if let Some(p) = path {
                    t.set("path", p.to_string_lossy().as_ref())?;
                }
                if let Some(ft) = filetype {
                    t.set("filetype", ft.as_str())?;
                }
            }
            Self::BufClose { buf }
            | Self::BufEnter { buf }
            | Self::BufLeave { buf }
            | Self::BufReload { buf }
            | Self::TextChangedCoarse { buf } => {
                t.set("buf", lua_int(buf.get()))?;
            }
            Self::BufSavePre { buf, path } | Self::BufSavePost { buf, path } => {
                t.set("buf", lua_int(buf.get()))?;
                t.set("path", path.to_string_lossy().as_ref())?;
            }
            Self::CursorMoved { buf, row, col } => {
                t.set("buf", lua_int(buf.get()))?;
                t.set("row", lua_int(*row + 1))?; // 1-indexed for Lua
                t.set("col", lua_int(*col))?;
            }
            Self::CursorHold { buf, row, col } => {
                t.set("buf", lua_int(buf.get()))?;
                t.set("row", lua_int(*row + 1))?;
                t.set("col", lua_int(*col))?;
            }
            Self::ModeChanged { from, to } => {
                t.set("from", format!("{from:?}").to_lowercase())?;
                t.set("to", format!("{to:?}").to_lowercase())?;
            }
            #[cfg(test)]
            Self::WindowResized { rows, cols } => {
                t.set("rows", i64::from(*rows))?;
                t.set("cols", i64::from(*cols))?;
            }
            Self::LspServerConnected {
                language,
                server_name,
            }
            | Self::LspServerReady {
                language,
                server_name,
            }
            | Self::LspProgress {
                language,
                server_name,
            } => {
                t.set("language", language.as_str())?;
                t.set("server_name", server_name.as_str())?;
            }
            Self::LspDiagnosticsChanged {
                uri,
                error_count,
                warning_count,
            } => {
                t.set("uri", uri.as_str())?;
                t.set("error_count", lua_int(*error_count))?;
                t.set("warning_count", lua_int(*warning_count))?;
            }
            Self::EditorStart | Self::EditorQuit => {}
            Self::WinEnter { win, buf } | Self::WinLeave { win, buf } => {
                t.set("win", lua_int(*win))?;
                t.set("buf", lua_int(buf.get()))?;
            }
            Self::WinMoved { win } => {
                t.set("win", lua_int(*win))?;
            }
            Self::WinSwapped { win1, win2 } => {
                t.set("win1", lua_int(*win1))?;
                t.set("win2", lua_int(*win2))?;
            }
        }
        Ok(t)
    }

    /// Short name used for handler lookup tables.
    #[must_use]
    pub(super) const fn name(&self) -> &'static str {
        match self {
            Self::BufOpen { .. } => "BufOpen",
            Self::BufClose { .. } => "BufClose",
            Self::BufEnter { .. } => "BufEnter",
            Self::BufLeave { .. } => "BufLeave",
            Self::BufSavePre { .. } => "BufSavePre",
            Self::BufSavePost { .. } => "BufSavePost",
            Self::BufReload { .. } => "BufReload",
            Self::TextChangedCoarse { .. } => "TextChangedCoarse",
            Self::CursorMoved { .. } => "CursorMoved",
            Self::CursorHold { .. } => "CursorHold",
            Self::ModeChanged { .. } => "ModeChanged",
            Self::EditorStart => "EditorStart",
            Self::EditorQuit => "EditorQuit",
            #[cfg(test)]
            Self::WindowResized { .. } => "WindowResized",
            Self::WinEnter { .. } => "WinEnter",
            Self::WinLeave { .. } => "WinLeave",
            Self::WinMoved { .. } => "WinMoved",
            Self::WinSwapped { .. } => "WinSwapped",
            Self::LspServerConnected { .. } => "LspServerConnected",
            Self::LspServerReady { .. } => "LspServerReady",
            Self::LspProgress { .. } => "LspProgress",
            Self::LspDiagnosticsChanged { .. } => "LspDiagnosticsChanged",
        }
    }
}
