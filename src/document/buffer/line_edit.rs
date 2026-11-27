use super::TextBuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(feature = "plugins")]
pub(crate) struct LineReplacement {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) text: String,
    pub(crate) cursor: usize,
}

impl TextBuffer {
    #[must_use]
    #[cfg(feature = "plugins")]
    pub(crate) fn line_replacement(
        &self,
        first: usize,
        last: usize,
        lines: &[String],
    ) -> Option<LineReplacement> {
        let line_count = self.line_index.line_count();
        if first >= line_count {
            return None;
        }
        let reaches_end = last.saturating_add(1) >= line_count;
        let mut start = self.line_index.get_line_start(first);
        let end = if reaches_end {
            self.len()
        } else {
            self.line_index.get_line_start(last + 1)
        };
        let mut text = lines.join("\n");
        let cursor = if lines.is_empty() {
            if reaches_end && first > 0 {
                start -= 1;
            }
            start
        } else {
            let cursor = start + text.chars().count();
            if !reaches_end {
                text.push('\n');
            }
            cursor
        };
        Some(LineReplacement {
            start,
            end,
            text,
            cursor,
        })
    }

    #[must_use]
    #[cfg(feature = "plugins")]
    pub(crate) fn line_col_position(&self, row: usize, col: usize) -> usize {
        let row = row.min(self.line_index.line_count().saturating_sub(1));
        let start = self.line_index.get_line_start(row);
        let end = self.line_index.get_end(row, self.len()).unwrap_or(start);
        start + col.min(end.saturating_sub(start))
    }
}
