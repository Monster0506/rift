use super::Document;
use crate::selection::Region;

impl Document {
    pub(crate) fn sync_selection_annotations(&mut self, active: Option<Region>, banked: &[Region]) {
        self.annotations.clear_by_kind_prefix("ui.selection");

        for region in banked {
            self.add_selection_annotation(region, "ui.selection.banked", 4);
        }
        if let Some(region) = active {
            self.add_selection_annotation(&region, "ui.selection.active", 6);
        }
    }

    fn add_selection_annotation(&mut self, region: &Region, kind: &str, priority: i32) {
        use crate::annotations::kind::Kind;
        use crate::annotations::presentation::{FaceRef, Presentation};
        use crate::annotations::{Anchor, Annotation, AnnotationOwner};

        let (start_char, end_char) = region.buffer_span(&self.buffer);
        let start = self.buffer.char_to_byte(start_char);
        let end = self.buffer.char_to_byte(end_char);
        if start >= end {
            return;
        }
        self.annotations.add(
            Annotation::new(
                Kind::new(kind),
                Anchor::range(start, end),
                AnnotationOwner::System,
            )
            .with_presentation(Presentation::with_face(FaceRef::new(kind)).with_priority(priority)),
        );
    }
}
