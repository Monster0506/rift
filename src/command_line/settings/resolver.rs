use super::descriptor::SettingValue;
use super::registry::SettingsRegistry;
use crate::command_line::commands::ExecutionResult;
use crate::error::RiftError;

#[derive(Clone, Copy)]
pub struct SettingsResolver<L: 'static, G: 'static> {
    local: SettingsRegistry<L>,
    global: SettingsRegistry<G>,
}

impl<L, G> SettingsResolver<L, G> {
    #[must_use]
    pub fn new(local: SettingsRegistry<L>, global: SettingsRegistry<G>) -> Self {
        Self { local, global }
    }

    pub fn set(
        &self,
        name: &str,
        value: Option<String>,
        local_target: &mut L,
        global_target: &mut G,
        error_handler: &mut dyn FnMut(RiftError),
    ) -> ExecutionResult {
        if self.local.find_descriptor(name).is_some() {
            self.local
                .execute_setting(name, value, local_target, error_handler)
        } else {
            self.global
                .execute_setting(name, value, global_target, error_handler)
        }
    }

    #[must_use]
    pub fn get(&self, name: &str, local_target: &L, global_target: &G) -> Option<SettingValue> {
        self.local
            .get_setting(name, local_target)
            .or_else(|| self.global.get_setting(name, global_target))
    }
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod resolver_tests;
