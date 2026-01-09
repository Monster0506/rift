/// Caches the byte offsets of line starts to optimize regex haystack creation.
/// This is expensive to calculate (O(N) iteration over UTF-8 chars) but cheap to store (8 bytes per line).
#[derive(Debug, Clone)]
pub struct ByteLineMap {
    pub line_starts: Vec<usize>,
    pub revision: u64,
}

impl ByteLineMap {
    pub fn new(line_starts: Vec<usize>, revision: u64) -> Self {
        Self {
            line_starts,
            revision,
        }
    }
}
