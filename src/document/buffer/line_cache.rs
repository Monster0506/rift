/// Cache of materialized line strings for regex search, keyed to buffer
/// revision - stale (revision-mismatched) entries are cleared on access.
#[derive(Debug, Default, Clone)]
pub struct LineCache;

impl LineCache {
    #[must_use]
    pub(super) fn new() -> Self {
        Self
    }
}
