//! Settings registry
//! Configuration-driven registry for :set command options

pub mod definitions;
pub mod descriptor;
pub mod registry;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use definitions::create_settings_registry;
pub use descriptor::{SettingDescriptor, SettingError, SettingSetter, SettingType, SettingValue};
pub use registry::SettingsRegistry;
