//! Inert stand-in for `lua_host` when `plugins` is compiled out: no Lua VM
//! exists, so every operation is a no-op / empty result.

pub use crate::plugin::lua_state::{AnnotationView, BufEntry, BufLinesSource, WinEntry};

/// Stand-in for the real `LuaHost`; holds no state since there is no VM.
#[derive(Debug, Default)]
pub struct LuaHost;

impl LuaHost {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn set_annotations(&self, _views: Vec<AnnotationView>, _next_id: u64) {}

    #[allow(clippy::too_many_arguments)]
    pub fn update_state(
        &self,
        _buf_id: usize,
        _buf_kind: String,
        _source: Option<BufLinesSource>,
        _cursor: (usize, usize),
        _tab_width: usize,
        _expand_tabs: bool,
        _mode: &str,
        _filetype: Option<String>,
        _file_path: Option<String>,
        _buf_list: Vec<BufEntry>,
        _window_size: (u16, u16),
        _can_undo: bool,
        _can_redo: bool,
        _is_dirty: bool,
        _scroll: (usize, usize),
        _line_ending: &str,
        _commands: Vec<(String, String)>,
        _win_list: Vec<WinEntry>,
        _focused_win_id: u64,
        _previous_win_id: Option<u64>,
        _lsp_diagnostics: std::collections::HashMap<String, Vec<(u32, u32, u32, String)>>,
    ) {
    }

    pub fn dispatch_event(&self, _event: &crate::plugin::events::EditorEvent) -> Vec<String> {
        Vec::new()
    }

    pub fn invoke_annotation_action(&self, _ctx: &crate::plugin::AnnotationActionCtx) -> bool {
        false
    }

    pub fn invoke_annotation_hook(
        &self,
        _enter: bool,
        _ctx: &crate::plugin::AnnotationHoverCtx,
    ) -> bool {
        false
    }

    pub fn execute_command(&self, _name: &str, _args: &[String]) -> bool {
        false
    }

    pub fn command_list(&self) -> Vec<(String, String, Option<String>)> {
        Vec::new()
    }

    pub fn execute_action(&self, _id: &str) -> bool {
        false
    }

    pub fn load_dir(&self, _dir: &std::path::Path) -> Vec<String> {
        Vec::new()
    }

    pub fn load_file(&self, _path: &std::path::Path) -> Option<String> {
        None
    }

    pub fn drain_mutations(&self) -> Vec<crate::plugin::PluginMutation> {
        Vec::new()
    }

    pub fn exec(&self, _code: &str) -> Option<String> {
        None
    }
}
