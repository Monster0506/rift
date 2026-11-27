mod annotate;
mod cursor;
#[cfg(test)]
pub(in crate::document) use cursor::move_buffer;
mod history;
mod journal;
mod options;
mod put;
mod selection;
pub(crate) mod standalone;
mod text;

pub(crate) use cursor::CursorCause;
#[cfg(any(feature = "plugins", feature = "lsp"))]
pub(crate) use journal::{ChangeJournal, Changes};
pub(crate) use put::PutAt;
pub(crate) use text::{ContentKind, EditSource};
pub use text::{Edit, IntoText};
pub(in crate::document) use text::{Origin, Resolved};
