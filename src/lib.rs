//! Rift - A terminal-based text editor

#![warn(unreachable_pub)]

#[cfg(feature = "perf_instrumentation")]
mod perf;

mod action;
mod annotations;
mod buffer;
mod character;
mod clipboard;
mod color;
mod command;
mod command_line;
mod constants;
mod cursor;
mod document;
mod dot_repeat;
mod editor;
mod editor_api;
mod error;
mod eval;
mod executor;
mod floating_window;
mod fs_backend;
mod git;
mod history;
mod invariants;
#[cfg(feature = "ipc")]
mod ipc;
mod job_manager;
mod key;
#[cfg(test)]
mod key_handler;
mod keymap;
mod layer;
#[cfg(feature = "lsp")]
mod lsp;
mod message;
mod mode;
mod movement;
mod notification;
mod num;
mod paint;
mod plugin;
mod render;
mod replay;
mod screen_buffer;
mod search;
mod selection;
mod split;
mod state;
mod status;
mod syntax;
mod term;
mod text_objects;
mod time;
mod transport;
mod undotree_view;
mod viewport;
mod wrap;

#[cfg(test)]
mod test_utils;

// Entry points for the `rift` and `rift-replay` binaries.
pub use editor::Editor;
pub use num::u64_to_f64;
pub use replay::{parse as parse_replay, run as run_replay, RunReport};
pub use term::crossterm::CrosstermBackend;

#[cfg(feature = "ipc")]
pub use ipc::{
    client::connect_remote,
    daemon::{detach as detach_daemon, run as run_daemon},
    session::print_newest as print_newest_session,
};
#[cfg(feature = "lsp")]
pub use lsp::broker::run as run_lsp_broker;
#[cfg(feature = "perf_instrumentation")]
pub use perf::{cursor_advance_count, frame_stats, row_paint_counts, span_stats};

/// Internals the benches need. Not a stable API.
#[doc(hidden)]
pub mod bench_api {
    pub mod action {
        pub use crate::action::{Action, EditorAction, Motion};
    }
    pub mod annotations {
        pub use crate::annotations::{
            Action, Anchor, Annotation, AnnotationOwner, AnnotationStore, Kind, Presentation,
            StyleOverride,
        };
    }
    pub mod buffer {
        pub use crate::buffer::line_index::LineIndex;
        pub use crate::buffer::rope::PieceTable;
        pub use crate::buffer::TextBuffer;
    }
    pub mod character {
        pub use crate::character::Character;
    }
    pub mod error {
        pub use crate::error::RiftError;
    }
    pub mod history {
        pub use crate::history::{EditOperation, EditTransaction, Position, UndoTree};
    }
    pub mod job_manager {
        pub use crate::job_manager::{CancellationSignal, Job, JobManager, JobMessage};
    }
    pub mod key {
        pub use crate::key::{InputEvent, Key};
    }
    pub mod keymap {
        pub use crate::keymap::{KeyContext, KeyMap};
    }
    pub mod layer {
        pub use crate::layer::{Cell, CellAttrs};
    }
    pub mod mode {
        pub use crate::mode::Mode;
    }
    pub mod render {
        pub use crate::render::{RenderState, RenderSystem};
    }
    pub mod screen_buffer {
        pub use crate::screen_buffer::DoubleBuffer;
    }
    pub mod search {
        pub use crate::search::{compile_regex, find_all};
    }
    pub mod state {
        pub use crate::state::State;
    }
    pub mod syntax {
        pub use crate::syntax::interval_tree::IntervalTree;
    }
    pub mod term {
        pub use crate::term::{CursorShape, Size, TerminalBackend};
    }
    pub mod wrap {
        pub use crate::wrap::DisplayMap;
    }
}

/// Time a lexical scope and record a `crate::perf::PerfEvent` on exit.
/// Compiles to nothing (including `$fields`) when `perf_instrumentation` is off.
macro_rules! perf_span {
    ($name:expr, $fields:expr) => {
        #[cfg(feature = "perf_instrumentation")]
        let _span = $crate::perf::PerfSpan::new($name, $fields);
    };
}
pub(crate) use perf_span;

/// Count a `.clone()` at a hot render-path call site when `perf_instrumentation`
/// is enabled; expands to just `$val` (the clone itself) otherwise.
macro_rules! perf_clone {
    ($val:expr) => {{
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_clone();
        $val
    }};
}
pub(crate) use perf_clone;

/// Count one fast-forward cursor advance step; a no-op unless
/// `perf_instrumentation` is enabled. Deterministic, unlike timing spans.
macro_rules! perf_cursor_advance {
    () => {
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_cursor_advance();
    };
}
pub(crate) use perf_cursor_advance;

/// Count one visible content row as painted (full pipeline) or skipped
/// (dirty-row scroll blit); a no-op unless `perf_instrumentation` is enabled.
macro_rules! perf_row_painted {
    () => {
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_row_painted();
    };
}
pub(crate) use perf_row_painted;
macro_rules! perf_row_blit_skipped {
    () => {
        #[cfg(feature = "perf_instrumentation")]
        $crate::perf::count_row_blit_skipped();
    };
}
pub(crate) use perf_row_blit_skipped;
