//! Rendering hook for the multi-region selection set: mirrors
//! Document::sync_search_annotations (search.rs) but for ui.selection.*.

use super::Document;
use crate::selection::Region;

const BANKED_COLORS: [crate::color::Color; 4] = [
    crate::color::Color::Yellow,
    crate::color::Color::Green,
    crate::color::Color::Magenta,
    crate::color::Color::Cyan,
];

impl Document {
    /// Mirror the active + banked selection regions into `ui.selection.*`
    /// annotations so they render through the presentation pipeline, like search highlights.
    pub fn sync_selection_annotations(&mut self, active: Option<Region>, banked: &[Region]) {
        use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride};

        self.annotations.clear_by_kind_prefix("ui.selection");

        for (i, region) in banked.iter().enumerate() {
            let (start_char, end_char) = region.buffer_span(&self.buffer);
            let start = self.buffer.char_to_byte(start_char);
            let end = self.buffer.char_to_byte(end_char);
            if start >= end {
                continue;
            }
            let style = StyleOverride {
                bg: Some(BANKED_COLORS[i % BANKED_COLORS.len()]),
                ..Default::default()
            };
            self.annotations.add(
                Annotation::new(Kind::new("ui.selection.banked"), Anchor::range(start, end), AnnotationOwner::System)
                    .with_presentation(Presentation::with_style(style).with_priority(4)),
            );
        }

        if let Some(region) = active {
            let (start_char, end_char) = region.buffer_span(&self.buffer);
            let start = self.buffer.char_to_byte(start_char);
            let end = self.buffer.char_to_byte(end_char);
            if start < end {
                let style = StyleOverride {
                    bg: Some(crate::color::Color::Blue),
                    ..Default::default()
                };
                self.annotations.add(
                    Annotation::new(Kind::new("ui.selection.active"), Anchor::range(start, end), AnnotationOwner::System)
                        .with_presentation(Presentation::with_style(style).with_priority(6)),
                );
            }
        }
    }

    /// Remove all selection-highlight annotations.
    pub fn clear_selection_annotations(&mut self) {
        self.annotations.clear_by_kind_prefix("ui.selection");
    }
}
