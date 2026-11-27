#[cfg(any(feature = "plugins", test))]
use super::descriptor::SettingValue;
use super::registry::SettingsRegistry;
use crate::command_line::commands::registry::MatchResult;
#[cfg(any(feature = "plugins", test))]
use crate::error::RiftError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionScope {
    #[cfg(any(feature = "plugins", test))]
    Global,
    Local(crate::document::DocumentId),
    Resolve(crate::document::DocumentId),
}

pub(crate) struct SettingsResolver<L: 'static, G: 'static> {
    local: SettingsRegistry<L>,
    global: SettingsRegistry<G>,
}

impl<L, G> Clone for SettingsResolver<L, G> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L, G> Copy for SettingsResolver<L, G> {}

impl<L, G> SettingsResolver<L, G> {
    #[must_use]
    pub(crate) const fn new(local: SettingsRegistry<L>, global: SettingsRegistry<G>) -> Self {
        Self { local, global }
    }

    #[must_use]
    pub(crate) fn is_local(&self, name: &str) -> bool {
        !matches!(self.local.match_option(name), MatchResult::Unknown(_))
    }

    pub(in crate::command_line) fn match_name(
        &self,
        name: &str,
        local_only: bool,
    ) -> (MatchResult, bool) {
        match self.local.match_option(name) {
            MatchResult::Unknown(_) if !local_only => (self.global.match_option(name), false),
            matched => (matched, true),
        }
    }

    #[cfg(any(feature = "plugins", test))]
    pub(crate) fn set(
        &self,
        name: &str,
        value: Option<&str>,
        local_target: &mut L,
        global_target: &mut G,
    ) -> Result<(), RiftError> {
        if self.is_local(name) {
            self.local.set(name, value, local_target)?;
        } else {
            self.global.set(name, value, global_target)?;
        }
        Ok(())
    }

    #[cfg(any(feature = "plugins", test))]
    #[must_use]
    pub(crate) fn get(
        &self,
        name: &str,
        local_target: &L,
        global_target: &G,
    ) -> Option<SettingValue> {
        if self.is_local(name) {
            self.local.get_setting(name, local_target)
        } else {
            self.global.get_setting(name, global_target)
        }
    }
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod resolver_tests;
