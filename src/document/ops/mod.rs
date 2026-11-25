mod history;
mod put;
mod text;

pub(crate) use put::PutAt;
pub(crate) use text::{ContentKind, Edit, EditSource};
pub(in crate::document) use text::{Origin, Resolved};
