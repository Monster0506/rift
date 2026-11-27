use crate::annotations::{Annotation, AnnotationId};
use crate::document::Document;

impl Document {
    pub(crate) fn annotate(&mut self, annotation: Annotation) -> AnnotationId {
        self.annotations.add(annotation)
    }

    pub(crate) fn annotate_with_id(&mut self, id: AnnotationId, annotation: Annotation) {
        self.annotations.add_with_id(id, annotation);
    }

    pub(crate) fn update_annotation(
        &mut self,
        id: AnnotationId,
        update: impl FnOnce(&mut Annotation),
    ) {
        self.annotations.update(id, update);
    }

    pub(crate) fn clear_annotations_by_kind(&mut self, prefix: &str) {
        self.annotations.clear_by_kind_prefix(prefix);
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
