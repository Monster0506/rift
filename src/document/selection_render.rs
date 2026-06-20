//! Rendering hook for the multi-region selection set: mirrors
//! Document::sync_search_annotations (search.rs) but for ui.selection.*.

use super::Document;
use crate::selection::Region;

// 24-bit RGB for the widest color range (confirmed truecolor-capable
// terminal); not the ANSI named Blue, which renders as a dark, easy-to-miss
// navy. Banked and active regions share this color so the selection reads
// as one consistent highlight, not a rainbow.
const SELECTION_BG: crate::color::Color = crate::color::Color::Rgb {
    r: 100,
    g: 160,
    b: 220,
};
const SELECTION_FG: crate::color::Color = crate::color::Color::Black;

impl Document {
    /// Mirror the active + banked selection regions into `ui.selection.*`
    /// annotations so they render through the presentation pipeline, like search highlights.
    pub fn sync_selection_annotations(&mut self, active: Option<Region>, banked: &[Region]) {
        use crate::annotations::{
            Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride,
        };

        self.annotations.clear_by_kind_prefix("ui.selection");

        let banked_style = StyleOverride {
            fg: Some(SELECTION_FG),
            bg: Some(SELECTION_BG),
            ..Default::default()
        };
        for region in banked {
            let (start_char, end_char) = region.buffer_span(&self.buffer);
            let start = self.buffer.char_to_byte(start_char);
            let end = self.buffer.char_to_byte(end_char);
            if start >= end {
                continue;
            }
            self.annotations.add(
                Annotation::new(
                    Kind::new("ui.selection.banked"),
                    Anchor::range(start, end),
                    AnnotationOwner::System,
                )
                .with_presentation(Presentation::with_style(banked_style).with_priority(4)),
            );
        }

        if let Some(region) = active {
            let (start_char, end_char) = region.buffer_span(&self.buffer);
            let start = self.buffer.char_to_byte(start_char);
            let end = self.buffer.char_to_byte(end_char);
            if start < end {
                let style = StyleOverride {
                    fg: Some(SELECTION_FG),
                    bg: Some(SELECTION_BG),
                    ..Default::default()
                };
                self.annotations.add(
                    Annotation::new(
                        Kind::new("ui.selection.active"),
                        Anchor::range(start, end),
                        AnnotationOwner::System,
                    )
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
