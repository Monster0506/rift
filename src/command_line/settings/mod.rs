//! Settings registry
//! Configuration-driven registry for :set command options

pub(crate) mod definitions;
pub(crate) mod descriptor;
pub(crate) mod registry;
pub(crate) mod resolver;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub(crate) use definitions::create_settings_registry;
pub(crate) use descriptor::{SettingDescriptor, SettingError, SettingType, SettingValue};
pub(crate) use registry::SettingsRegistry;
pub(crate) use resolver::SettingsResolver;
