use super::Document;
use crate::annotations::Annotation;
use crate::selection::Region;

impl Document {
    pub(crate) fn sync_selection_annotations(&mut self, active: Option<Region>, banked: &[Region]) {
        let mut highlights = Vec::with_capacity(banked.len() + usize::from(active.is_some()));
        for region in banked {
            highlights.extend(self.selection_annotation(region, "ui.selection.banked", 4));
        }
        if let Some(region) = active {
            highlights.extend(self.selection_annotation(&region, "ui.selection.active", 6));
        }
        self.set_annotations("ui.selection", highlights);
    }

    fn selection_annotation(
        &self,
        region: &Region,
        kind: &str,
        priority: i32,
    ) -> Option<Annotation> {
        use crate::annotations::kind::Kind;
        use crate::annotations::presentation::{FaceRef, Presentation};
        use crate::annotations::{Anchor, AnnotationOwner};

        let (start_char, end_char) = region.buffer_span(&self.buffer);
        let start = self.buffer.char_to_byte(start_char);
        let end = self.buffer.char_to_byte(end_char);
        if start >= end {
            return None;
        }
        Some(
            Annotation::new(
                Kind::new(kind),
                Anchor::range(start, end),
                AnnotationOwner::System,
            )
            .with_presentation(Presentation::with_face(FaceRef::new(kind)).with_priority(priority)),
        )
    }
}
