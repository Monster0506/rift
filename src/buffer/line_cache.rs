use std::collections::HashMap;

/// Cache for materialized line strings to optimize regex search.
///
/// This cache stores UTF-8 `String` representations of lines.
/// It is sensitive to buffer revisions; if the revision changes, the cache
/// is considered stale and is cleared.
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

    /// Retrieve a line from the cache or insert it if missing.
    ///
    /// If `current_rev` differs from the stored revision, the cache is cleared first.
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
