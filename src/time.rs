//! Portable `Instant`/`SystemTime`: std's panics on wasm32 ("time not
//! implemented on this platform"); `web_time` is an API-compatible drop-in.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_arch = "wasm32")]
pub use web_time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "time_tests.rs"]
mod time_tests;
