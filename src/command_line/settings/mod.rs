//! Settings registry
//! Configuration-driven registry for :set command options

pub mod descriptor;
pub mod registry;

#[cfg(test)]
mod tests;

pub use descriptor::{SettingDescriptor, SettingType, SettingValue, SettingError, SettingSetter};
pub use registry::SettingsRegistry;

