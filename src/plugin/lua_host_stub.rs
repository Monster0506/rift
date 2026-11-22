//! Inert stand-in for `lua_host` when `plugins` is compiled out: no Lua VM
//! exists, so every operation is a no-op / empty result.

pub(crate) use crate::plugin::lua_state::{
    AnnotationView, BufAnnotations, BufEntry, BufMeta, BufUpdate, HistoryAvailability,
    LspDiagnosticsSnapshot, StateUpdate, SyncKeys, WinEntry,
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
    pub(super) fn with_generation(
        _generation: crate::plugin::PluginGeneration,
        _language_loader: std::sync::Arc<crate::syntax::loader::LanguageLoader>,
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

    pub(super) fn update_state(&self, _update: StateUpdate<'_>) {}

    pub(super) fn synced_keys(&self) -> SyncKeys {
        SyncKeys::default()
    }

    pub(super) fn take_creation_ids(&self) -> Vec<u64> {
        Vec::new()
    }

    pub(super) const fn take_divergence(&self) -> Vec<u64> {
        Vec::new()
    }

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

    pub(super) fn deliver_job_event(
        &self,
        _job: u64,
        _event: crate::job_manager::jobs::plugin_process::PluginJobEvent,
    ) {
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
    pub(super) const fn fire_due_timers(&self, _now: crate::time::Instant) -> bool {
        false
    }

    #[must_use]
    pub(super) const fn next_timer_deadline(&self) -> Option<crate::time::Instant> {
        None
    }

    #[must_use]
    pub(super) const fn exec(&self, _code: &str) -> Option<String> {
        None
    }
}
