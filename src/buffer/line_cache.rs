use std::collections::HashMap;

/// Cache of materialized line strings for regex search, keyed to buffer
/// revision - stale (revision-mismatched) entries are cleared on access.
#[derive(Debug, Default, Clone)]
pub struct LineCache {
    cache: HashMap<usize, String>,
    revision: u64,
}

impl LineCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            revision: 0,
        }
    }

    /// Retrieve a line from the cache or insert it if missing; clears the
    /// cache first if `current_rev` differs from the stored revision.
    pub fn get_or_insert(
        &mut self,
        line_idx: usize,
        current_rev: u64,
        factory: impl FnOnce() -> String,
    ) -> &str {
        if current_rev != self.revision {
            self.cache.clear();
            self.revision = current_rev;
        }
        self.cache.entry(line_idx).or_insert_with(factory)
    }

    /// Explicitly clear the cache (e.g. on massive changes or memory pressure)
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}
