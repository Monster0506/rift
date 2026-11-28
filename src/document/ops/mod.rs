mod annotate;
mod cursor;
#[cfg(test)]
pub(in crate::document) use cursor::move_buffer;
mod history;
mod journal;
mod options;
#[cfg(feature = "plugins")]
mod plugin;
mod put;
mod selection;
pub(crate) mod standalone;
mod text;

#[cfg(test)]
pub(crate) use cursor::compute_motion_range;
pub(crate) use cursor::CursorCause;
pub(crate) use history::EditSession;
#[cfg(any(feature = "plugins", feature = "lsp"))]
pub(crate) use journal::{ChangeJournal, Changes};
#[cfg(feature = "plugins")]
pub(crate) use plugin::{BufferChange, BufferTarget};
pub(crate) use put::PutAt;
pub(crate) use selection::DeleteLanding;
#[cfg(feature = "plugins")]
pub(crate) use text::EditPolicy;
pub(crate) use text::{ContentKind, DocumentWrite, EditSource, ReadOnlyEdit};
pub use text::{Edit, IntoText};
pub(in crate::document) use text::{Origin, Resolved};
