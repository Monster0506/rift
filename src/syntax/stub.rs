use super::ParseOutcome;

/// Inert stand-in for `Syntax` when tree-sitter is compiled out: no language
/// can ever be loaded, so every operation is a no-op / empty result.
pub(crate) struct Syntax {
    pub(crate) language_name: String,
}

#[allow(clippy::unused_self)]
impl Syntax {
    pub(crate) const fn notify_edit(
        &mut self,
        _start_byte: usize,
        _old_end_byte: usize,
        _new_end_byte: usize,
        _start_point: (usize, usize),
        _old_end_point: (usize, usize),
        _new_end_point: (usize, usize),
    ) {
    }

    pub(crate) const fn invalidate_trees(&mut self) {}

    #[must_use]
    pub(crate) const fn capture_names(&self) -> &[&str] {
        &[]
    }

    #[must_use]
    pub(crate) const fn highlights(
        &self,
        _range: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, u32)> {
        Vec::new()
    }

    #[must_use]
    pub(crate) const fn injection_highlights_named(
        &self,
        _range: Option<&std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, String)> {
        Vec::new()
    }

    /// Always 0: this stub never parses, so highlights never change.
    #[must_use]
    pub(crate) const fn highlights_generation(&self) -> u64 {
        0
    }

    /// Always 0: this stub never parses, so injections never change.
    #[must_use]
    pub(crate) const fn injection_generation(&self) -> u64 {
        0
    }

    pub(crate) const fn try_incremental_parse(
        &mut self,
        _source: &[u8],
        _budget: std::time::Duration,
    ) -> ParseOutcome {
        ParseOutcome::NoLanguage
    }
}
