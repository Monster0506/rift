//! Rift - A terminal-based text editor

#[cfg(feature = "perf_instrumentation")]
pub mod perf;

pub mod action;
pub mod buffer;
pub mod character;
pub mod color;
pub mod command;
pub mod command_line;
pub mod constants;
pub mod document;
pub mod dot_repeat;
pub mod editor;
pub mod editor_api;
pub mod error;
pub mod executor;
pub mod floating_window;
pub mod history;
pub mod job_manager;
pub mod key;
pub mod key_handler;
pub mod keymap;
pub mod layer;
pub mod message;
pub mod mode;
pub mod movement;
pub mod notification;
pub mod render;
pub mod screen_buffer;
pub mod search;
pub mod split;
pub mod state;
pub mod status;
pub mod syntax;
pub mod term;
pub mod undotree_view;
pub mod viewport;

#[cfg(test)]
pub mod test_utils;

/// Time a lexical scope and record a [`crate::perf::PerfEvent`] on exit.
///
/// Compiles to nothing when the `perf_instrumentation` feature is disabled —
/// including the `$fields` expression, so there is truly zero overhead.
///
/// # Example
/// ```rust,ignore
/// use monster_rift::{perf_span, perf::PerfFields};
///
/// fn render(rows: u32) {
///     let _span = perf_span!(
///         "render_frame",
///         PerfFields { lines: Some(rows), ..Default::default() }
///     );
///     // work …
/// }
/// ```
#[macro_export]
macro_rules! perf_span {
    ($name:expr, $fields:expr) => {
        #[cfg(feature = "perf_instrumentation")]
        let _span = $crate::perf::PerfSpan::new($name, $fields);
    };
}
