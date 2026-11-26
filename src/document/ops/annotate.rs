use crate::annotations::{Annotation, AnnotationId};
use crate::document::Document;

impl Document {
    pub(crate) fn annotate(&mut self, annotation: Annotation) -> AnnotationId {
        self.annotations.add(annotation)
    }

    pub(crate) fn unannotate(&mut self, id: AnnotationId) -> bool {
        self.annotations.remove(id)
    }

    pub(crate) fn set_annotations(
        &mut self,
        namespace: &str,
        annotations: impl IntoIterator<Item = Annotation>,
    ) {
        self.annotations.replace_namespace(namespace, annotations);
    }
}
