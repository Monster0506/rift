pub(crate) mod buffers;
#[cfg(feature = "plugins")]
mod events;
#[cfg(feature = "plugins")]
pub(crate) use events::EditorEvent;
mod ex;
mod files;
mod lsp;
mod notify;
mod options;
pub(super) mod save;
mod search;
pub(crate) mod windows;
mod yank;
