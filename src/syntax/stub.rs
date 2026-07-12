use super::ParseOutcome;
use std::sync::Arc;

/// Inert stand-in for `Syntax` when tree-sitter is compiled out: no language
/// can ever be loaded, so every operation is a no-op / empty result.
pub struct Syntax {
    pub language_name: String,
}

impl Syntax {
    pub fn notify_edit(
        &mut self,
        _start_byte: usize,
        _old_end_byte: usize,
        _new_end_byte: usize,
        _start_point: (usize, usize),
        _old_end_point: (usize, usize),
        _new_end_point: (usize, usize),
    ) {
    }

    pub fn invalidate_trees(&mut self) {}

    pub fn lib(&self) -> Option<Arc<super::loader::RawLib>> {
        None
    }

    pub fn capture_names(&self) -> &[&str] {
        &[]
    }

    pub fn highlights(
        &self,
        _range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, u32)> {
        Vec::new()
    }

    pub fn injection_highlights_named(
        &self,
        _range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, String)> {
        Vec::new()
    }

    pub fn try_incremental_parse(
        &mut self,
        _source: &[u8],
        _budget: std::time::Duration,
    ) -> ParseOutcome {
        ParseOutcome::NoLanguage
    }
}
