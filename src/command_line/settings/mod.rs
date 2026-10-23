//! Settings registry
//! Configuration-driven registry for :set command options

mod definitions;
mod descriptor;
mod registry;
mod resolver;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub(crate) use definitions::create_settings_registry;
pub(crate) use descriptor::{SettingDescriptor, SettingError, SettingType, SettingValue};
pub(crate) use registry::SettingsRegistry;
pub(crate) use resolver::SettingsResolver;
