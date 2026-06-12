//! Document search operations.

use super::Document;
use crate::error::{ErrorType, RiftError};
use crate::search::{SearchDirection, SearchMatch};

impl Document {
    /// Mirror search matches into `ui.search` annotations so highlighting renders
    /// through the presentation pipeline; `current` (if any) gets a distinct face.
    pub fn sync_search_annotations(&mut self, matches: &[SearchMatch], current: Option<usize>) {
        use crate::annotations::{
            Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride,
        };
        use crate::color::Color;
        self.annotations.clear_by_kind_prefix("ui.search");
        let base = StyleOverride {
            fg: Some(Color::Black),
            bg: Some(Color::Yellow),
            ..Default::default()
        };
        let current_style = StyleOverride {
            fg: Some(Color::Black),
            bg: Some(Color::Cyan),
            ..Default::default()
        };
        for (i, m) in matches.iter().enumerate() {
            let start = self.buffer.char_to_byte(m.range.start);
            let end = self.buffer.char_to_byte(m.range.end);
            if start >= end {
                continue;
            }
            let is_current = current == Some(i);
            let style = if is_current { current_style } else { base };
            self.annotations.add(
                Annotation::new(
                    Kind::new("ui.search"),
                    Anchor::range(start, end),
                    AnnotationOwner::System,
                )
                .with_presentation(
                    Presentation::with_style(style).with_priority(if is_current { 10 } else { 5 }),
                ),
            );
        }
    }

    /// Remove all search-highlight annotations.
    pub fn clear_search_annotations(&mut self) {
        self.annotations.clear_by_kind_prefix("ui.search");
    }

    /// Perform a search in the document.
    pub fn perform_search(
        &self,
        query: &str,
        direction: SearchDirection,
        skip_current: bool,
    ) -> Result<
        (
            Option<crate::search::SearchMatch>,
            crate::search::SearchStats,
        ),
        RiftError,
    > {
        let mut cursor = self.buffer.cursor();

        if skip_current && direction == SearchDirection::Forward {
            cursor = cursor.saturating_add(1);
        }

        match crate::search::find_next(&self.buffer, cursor, query, direction) {
            Ok((m, stats)) => Ok((m, stats)),
            Err(e) => Err(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::SEARCH_ERROR,
                e.to_string(),
            )),
        }
    }

    /// Find all occurrences of the pattern in the document.
    pub fn find_all_matches(
        &self,
        query: &str,
    ) -> Result<(Vec<crate::search::SearchMatch>, crate::search::SearchStats), RiftError> {
        crate::search::find_all(&self.buffer, query)
    }
}
