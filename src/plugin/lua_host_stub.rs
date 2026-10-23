//! Inert stand-in for `lua_host` when `plugins` is compiled out: no Lua VM
//! exists, so every operation is a no-op / empty result.

pub(crate) use crate::plugin::lua_state::{
    AnnotationView, BufEntry, BufLinesSource, BufSourceUpdate, HistoryAvailability, StateUpdate,
    WinEntry,
};

/// Stand-in for the real `LuaHost`; holds no state since there is no VM.
#[derive(Debug, Default)]
pub(super) struct LuaHost;

impl LuaHost {
    /// # Errors
    /// Never fails; the stub holds no state.
    const fn new() -> Result<Self, String> {
        Ok(Self)
    }

    /// # Errors
    /// Never fails; the stub holds no state.
    pub(super) const fn with_generation(
        _generation: crate::plugin::PluginGeneration,
    ) -> Result<Self, String> {
        Ok(Self)
    }

    #[must_use]
    pub(super) const fn generation(&self) -> crate::plugin::PluginGeneration {
        crate::plugin::PluginGeneration::new(
            crate::plugin::PluginId::new(std::num::NonZeroU32::MIN),
            std::num::NonZeroU32::MIN,
        )
    }

    pub(super) const fn mark_retiring(&self) {}
    pub(super) fn set_annotations(&self, _views: Vec<AnnotationView>, _next_id: u64) {}

    pub(super) fn update_state(&self, _update: StateUpdate<'_>) {}

    #[must_use]
    pub(super) const fn dispatch_event(
        &self,
        _event: &crate::plugin::events::EditorEvent,
    ) -> Vec<String> {
        Vec::new()
    }

    #[must_use]
    pub(super) const fn invoke_annotation_action(
        &self,
        _ctx: &crate::plugin::AnnotationActionCtx,
    ) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn invoke_annotation_hook(
        &self,
        _enter: bool,
        _ctx: &crate::plugin::AnnotationHoverCtx,
    ) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn invoke_buffer_action(
        &self,
        _buf_id: u64,
        _kind: &str,
        _action: &str,
    ) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn invoke_buffer_save(&self, _buf_id: u64, _kind: &str) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn invoke_buffer_close(&self, _buf_id: u64, _kind: &str) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn execute_command(&self, _name: &str, _args: &[String]) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn command_list(&self) -> Vec<(String, String, Option<String>)> {
        Vec::new()
    }

    #[must_use]
    pub(super) const fn execute_action(&self, _id: &str) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn load_dir(&self, _dir: &std::path::Path) -> Vec<String> {
        Vec::new()
    }

    #[must_use]
    pub(super) const fn load_file(&self, _path: &std::path::Path) -> Option<String> {
        None
    }

    #[must_use]
    pub(super) const fn drain_mutations(&self) -> Vec<crate::plugin::PluginMutation> {
        Vec::new()
    }

    #[must_use]
    pub(super) const fn exec(&self, _code: &str) -> Option<String> {
        None
    }
}
