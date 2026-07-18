//! Rift - A terminal-based text editor

#[cfg(feature = "perf_instrumentation")]
pub mod perf;

pub mod action;
pub mod annotations;
pub mod buffer;
pub mod character;
pub mod clipboard;
pub mod color;
pub mod command;
pub mod command_line;
pub mod constants;
pub mod cursor;
pub mod document;
pub mod dot_repeat;
pub mod editor;
pub mod editor_api;
pub mod error;
pub mod eval;
pub mod executor;
pub mod floating_window;
pub mod history;
#[cfg(feature = "ipc")]
pub mod ipc;
pub mod job_manager;
pub mod key;
pub mod key_handler;
pub mod keymap;
pub mod layer;
#[cfg(feature = "lsp")]
pub mod lsp;
pub mod message;
pub mod mode;
pub mod movement;
pub mod notification;
pub mod paint;
pub mod plugin;
pub mod render;
pub mod replay;
pub mod screen_buffer;
pub mod search;
pub mod selection;
pub mod split;
pub mod state;
pub mod status;
pub mod syntax;
pub mod term;
pub mod text_objects;
pub mod transport;
pub mod undotree_view;
pub mod viewport;
pub mod wrap;

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

/// Count a `.clone()` at a hot render-path call site when `perf_instrumentation`
/// is enabled; expands to just `$val` (the clone itself) otherwise.
#[macro_export]
macro_rules! perf_clone {
    ($val:expr) => {{
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_clone();
        $val
    }};
}

/// Count one fast-forward cursor advance step; a no-op unless
/// `perf_instrumentation` is enabled. Deterministic, unlike timing spans.
#[macro_export]
macro_rules! perf_cursor_advance {
    () => {
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_cursor_advance();
    };
}

/// Count one visible content row as painted (full pipeline) or skipped
/// (dirty-row scroll blit); a no-op unless `perf_instrumentation` is enabled.
#[macro_export]
macro_rules! perf_row_painted {
    () => {
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_row_painted();
    };
}
#[macro_export]
macro_rules! perf_row_blit_skipped {
    () => {
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_row_blit_skipped();
    };
}
