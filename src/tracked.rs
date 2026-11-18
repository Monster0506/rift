use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
pub(crate) struct Tracked<T> {
    value: T,
    revision: u64,
}

impl<T> Tracked<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value,
            revision: next_revision(),
        }
    }

    #[must_use]
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }
}

impl<T: Default> Default for Tracked<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> std::ops::Deref for Tracked<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.revision = next_revision();
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::Tracked;

    #[test]
    fn mutable_access_changes_revision_and_reads_do_not() {
        let mut tracked = Tracked::new(vec![1]);
        let start = tracked.revision();
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked.revision(), start);
        tracked.push(2);
        assert_ne!(tracked.revision(), start);
        assert_ne!(Tracked::new(0).revision(), Tracked::new(0).revision());
    }
}
