//! Document search operations.

use super::Document;
use crate::error::{ErrorType, RiftError};
use crate::search::SearchDirection;

impl Document {
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
