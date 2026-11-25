mod cursor;
mod history;
mod put;
mod text;

pub(crate) use cursor::CursorCause;
pub(crate) use put::PutAt;
pub(crate) use text::{ContentKind, EditSource};
pub use text::{Edit, IntoText};
pub(in crate::document) use text::{Origin, Resolved};
