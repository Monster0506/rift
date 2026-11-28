use crate::document::runtime::DocumentHandle;
use crate::document::{Document, DocumentId};
use crate::error::{ErrorType, RiftError};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

pub(crate) fn parent_dir_missing(path: &Path) -> bool {
    crate::fs_backend::backend().parent_dir_missing(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemovalIntent {
    Normal,
    Force,
}

impl RemovalIntent {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CreationReservation {
    id: DocumentId,
    handle: DocumentHandle,
}

impl CreationReservation {
    #[must_use]
    #[cfg(any(feature = "plugins", test))]
    const fn new(id: DocumentId, handle: DocumentHandle) -> Self {
        Self { id, handle }
    }

    #[must_use]
    #[cfg(any(feature = "plugins", test))]
    pub(crate) const fn id(&self) -> DocumentId {
        self.id
    }

    #[must_use]
    pub(crate) const fn handle(&self) -> DocumentHandle {
        self.handle
    }
}

pub(crate) struct DocumentDraft {
    reservation: CreationReservation,
    document: Document,
}

impl DocumentDraft {
    /// # Errors
    /// Returns an error if the document ID or state key does not match the reservation or
    /// descriptor.
    #[cfg(any(feature = "plugins", test))]
    pub(crate) fn new(
        reservation: CreationReservation,
        document: Document,
    ) -> Result<Self, RiftError> {
        if document.id != reservation.id() {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!(
                    "Draft document ID {} does not match reserved ID {}",
                    document.id,
                    reservation.id()
                ),
            ));
        }
        if document.descriptor().state_key() != document.state.key_id() {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!(
                    "Document {} state key '{}' does not match descriptor key '{}'",
                    document.id,
                    document.state.key_id(),
                    document.descriptor().state_key()
                ),
            ));
        }

        Ok(Self {
            reservation,
            document,
        })
    }

    fn into_document(self) -> Document {
        self.document
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PreparedRemoval {
    id: DocumentId,
    pub(super) replacement: Option<DocumentId>,
}

impl PreparedRemoval {
    #[cfg(test)]
    const fn id(&self) -> DocumentId {
        self.id
    }

    #[cfg(test)]
    const fn needs_replacement(&self) -> bool {
        self.replacement.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    Background,
    Private,
}

pub(crate) enum NewDocument {
    Plain(Document),
    Draft(DocumentDraft),
}

impl From<Document> for NewDocument {
    fn from(document: Document) -> Self {
        Self::Plain(document)
    }
}

impl From<DocumentDraft> for NewDocument {
    fn from(draft: DocumentDraft) -> Self {
        Self::Draft(draft)
    }
}

pub(crate) struct DocumentManager {
    documents: BTreeMap<DocumentId, Document>,
    tab_order: Vec<DocumentId>,
    next_document_id: DocumentId,
    private_document_ids: BTreeSet<DocumentId>,
    most_recent_ghost_doc: Option<DocumentId>,
    next_instance: u64,
    handles: HashMap<DocumentId, DocumentHandle>,
    plugin_ids: std::cell::RefCell<Vec<DocumentId>>,
}

impl DocumentManager {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            documents: BTreeMap::new(),
            tab_order: Vec::new(),
            next_document_id: DocumentId::new(1),
            private_document_ids: BTreeSet::new(),
            most_recent_ghost_doc: None,
            next_instance: 1,
            handles: HashMap::new(),
            plugin_ids: std::cell::RefCell::new(Vec::with_capacity(8)),
        }
    }

    #[must_use]
    pub(crate) const fn most_recent_ghost_doc(&self) -> Option<DocumentId> {
        self.most_recent_ghost_doc
    }

    pub(super) const fn set_most_recent_ghost_doc(&mut self, id: Option<DocumentId>) {
        self.most_recent_ghost_doc = id;
    }

    #[cfg(feature = "terminal_emulation")]
    #[must_use]
    pub(crate) fn get_handle(&self, id: DocumentId) -> Option<DocumentHandle> {
        self.handles.get(&id).copied()
    }

    #[must_use]
    pub(crate) fn get_document_by_handle(&self, handle: DocumentHandle) -> Option<&Document> {
        if self.handles.get(&handle.doc_id) == Some(&handle) {
            self.documents.get(&handle.doc_id)
        } else {
            None
        }
    }

    pub(crate) fn get_document_by_handle_mut(
        &mut self,
        handle: DocumentHandle,
    ) -> Option<&mut Document> {
        if self.handles.get(&handle.doc_id) == Some(&handle) {
            self.documents.get_mut(&handle.doc_id)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub(super) fn reserve_creation(&mut self) -> CreationReservation {
        let id = self.next_id();
        self.next_document_id = id.next();
        let instance = self.next_instance;
        self.next_instance += 1;
        let handle = DocumentHandle::new(id, instance);
        CreationReservation::new(id, handle)
    }

    pub(super) fn insert(
        &mut self,
        new: impl Into<NewDocument>,
        placement: Placement,
    ) -> DocumentHandle {
        let mut document = match new.into() {
            NewDocument::Plain(document) => document,
            NewDocument::Draft(draft) => {
                self.handles
                    .insert(draft.document.id, draft.reservation.handle());
                draft.into_document()
            }
        };
        let id = document.id;
        if id >= self.next_document_id {
            self.next_document_id = id.next();
        }
        let handle = if let Some(&handle) = self.handles.get(&id) {
            handle
        } else {
            let handle = DocumentHandle::new(id, self.next_instance);
            self.next_instance += 1;
            self.handles.insert(id, handle);
            handle
        };
        document.set_handle(handle);

        self.documents.insert(id, document);
        self.tab_order.push(id);
        if placement == Placement::Private {
            self.private_document_ids.insert(id);
        }
        handle
    }

    #[must_use]
    pub(crate) fn get_document(&self, id: DocumentId) -> Option<&Document> {
        self.documents.get(&id)
    }

    pub(crate) fn get_document_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        self.documents.get_mut(&id)
    }

    pub(crate) fn documents_iter(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    pub(crate) fn take_syntax_notices(
        &mut self,
    ) -> Vec<(DocumentId, crate::document::syntax_sync::SyntaxNotice)> {
        self.documents
            .values_mut()
            .filter_map(|doc| Some((doc.id, doc.take_syntax_notice()?)))
            .collect()
    }

    #[cfg(any(feature = "plugins", feature = "lsp"))]
    pub(crate) fn drain_changes(&mut self) -> Vec<(DocumentId, crate::document::ops::Changes)> {
        self.documents
            .values_mut()
            .map(|doc| (doc.id, doc.drain_changes()))
            .filter(|(_, changes)| *changes != crate::document::ops::Changes::default())
            .collect()
    }

    #[cfg(feature = "plugins")]
    pub(crate) fn tabs(&self) -> impl Iterator<Item = &Document> {
        self.public_tabs().filter_map(|id| self.documents.get(&id))
    }

    pub(crate) fn check_invariants(&self) -> Vec<String> {
        let mut out = Vec::new();

        for id in &self.tab_order {
            if !self.documents.contains_key(id) {
                out.push(format!("tab_order references missing document {id}"));
            }
        }
        for id in self.documents.keys() {
            match self.handles.get(id) {
                None => out.push(format!("document {id} has no matching handle entry")),
                Some(h) if h.doc_id != *id => out.push(format!(
                    "handle for document {id} carries doc_id {}",
                    h.doc_id
                )),
                _ => {}
            }
        }

        for id in &self.private_document_ids {
            if !self.documents.contains_key(id) {
                out.push(format!(
                    "private_document_ids references missing document {id}"
                ));
            }
        }

        if let Some(id) = self.most_recent_ghost_doc {
            if !self.documents.contains_key(&id) {
                out.push(format!(
                    "most_recent_ghost_doc {id} is not an open document"
                ));
            }
        }

        let max_id = self
            .documents
            .keys()
            .copied()
            .max()
            .unwrap_or(DocumentId::new(0));
        if !self.documents.is_empty() && self.next_document_id <= max_id {
            out.push(format!(
                "next_document_id {} <= max existing document id {max_id}",
                self.next_document_id
            ));
        }

        out
    }

    #[must_use]
    pub(crate) fn find_messages_doc_id(&self) -> Option<DocumentId> {
        self.documents
            .iter()
            .find(|(_, d)| d.is_messages())
            .map(|(id, _)| *id)
    }

    #[must_use]
    pub(crate) fn next_id(&self) -> DocumentId {
        let reserved = self.plugin_ids.borrow();
        let mut id = self.next_document_id;
        while reserved.contains(&id) {
            id = id.next();
        }
        id
    }

    #[cfg(any(feature = "plugins", test))]
    pub(super) fn refill_plugin_ids(&self, mut ids: Vec<u64>) -> Vec<u64> {
        self.release_plugin_ids(ids.iter().copied());
        ids.clear();
        let mut reserved = self.plugin_ids.borrow_mut();
        let mut id = self.next_document_id;
        while ids.len() < 8 {
            if !reserved.contains(&id) {
                reserved.push(id);
                ids.push(id.get());
            }
            id = id.next();
        }
        ids.reverse();
        ids
    }

    #[cfg(feature = "plugins")]
    pub(super) fn clear_plugin_ids(&self) {
        self.plugin_ids.borrow_mut().clear();
    }

    #[cfg(any(feature = "plugins", test))]
    pub(super) fn release_plugin_ids(&self, ids: impl IntoIterator<Item = u64>) {
        let mut reserved = self.plugin_ids.borrow_mut();
        for id in ids {
            if let Some(index) = reserved.iter().position(|reserved| reserved.get() == id) {
                reserved.swap_remove(index);
            }
        }
    }

    #[cfg(any(feature = "plugins", test))]
    pub(super) fn claim_plugin_creation(&mut self, id: DocumentId) -> Option<CreationReservation> {
        let mut reserved = self.plugin_ids.borrow_mut();
        let index = reserved.iter().position(|reserved| *reserved == id)?;
        reserved.swap_remove(index);
        let handle = DocumentHandle::new(id, self.next_instance);
        self.next_instance += 1;
        Some(CreationReservation::new(id, handle))
    }

    /// # Errors
    /// Returns an error if the document is missing or not in the tab order, has unsaved changes
    /// under a `Normal` removal, or its replacement cannot be created.
    pub(super) fn prepare_removal(
        &self,
        id: DocumentId,
        intent: RemovalIntent,
    ) -> Result<PreparedRemoval, RiftError> {
        let doc = self.documents.get(&id).ok_or_else(|| {
            RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!("Document {id} not found"),
            )
        })?;

        let Some(position) = self.tab_index(id) else {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!("Document {id} in storage but not in tab order"),
            ));
        };

        if intent == RemovalIntent::Normal
            && doc.policies().close == crate::document::runtime::ClosePolicy::ConfirmDirty
            && doc.is_dirty()
        {
            return Err(RiftError::warning(
                ErrorType::Execution,
                crate::constants::errors::UNSAVED_CHANGES,
                crate::constants::errors::MSG_UNSAVED_CHANGES,
            ));
        }

        Ok(PreparedRemoval {
            id,
            replacement: self.tab_order[position + 1..]
                .iter()
                .chain(self.tab_order[..position].iter().rev())
                .copied()
                .find(|&id| !self.is_private(id)),
        })
    }

    pub(super) fn commit_removal(&mut self, plan: PreparedRemoval) -> Option<Document> {
        if let Some(pos) = self.tab_index(plan.id) {
            self.tab_order.remove(pos);
        }

        self.private_document_ids.remove(&plan.id);
        self.handles.remove(&plan.id);
        if self.most_recent_ghost_doc == Some(plan.id) {
            self.most_recent_ghost_doc = None;
        }

        self.documents.remove(&plan.id)
    }

    #[cfg(test)]
    /// # Errors
    /// Returns an error if the document has unsaved changes, is not in the tab order, or its
    /// replacement cannot be created.
    pub(super) fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        if !self.documents.contains_key(&id) {
            return Ok(());
        }
        let plan = self.prepare_removal(id, RemovalIntent::Normal)?;
        if plan.needs_replacement() {
            let replacement = Document::new(self.next_id())?;
            self.insert(replacement, Placement::Background);
        }
        self.commit_removal(plan);
        Ok(())
    }

    #[must_use]
    pub(crate) fn neighbor_tab(&self, active: DocumentId, forward: bool) -> Option<DocumentId> {
        let is_public = |&id: &DocumentId| !self.is_private(id);
        let next = |tabs: &[DocumentId]| tabs.iter().copied().find(is_public);
        let Some(index) = self.tab_index(active).filter(|_| is_public(&active)) else {
            return next(&self.tab_order);
        };
        let previous = |tabs: &[DocumentId]| tabs.iter().rev().copied().find(is_public);
        if forward {
            next(&self.tab_order[index + 1..]).or_else(|| next(&self.tab_order[..=index]))
        } else {
            previous(&self.tab_order[..index]).or_else(|| previous(&self.tab_order[index..]))
        }
    }

    #[must_use]
    pub(crate) fn tab_count(&self) -> usize {
        self.public_tabs().count()
    }

    fn public_tabs(&self) -> impl Iterator<Item = DocumentId> + '_ {
        self.tab_order
            .iter()
            .copied()
            .filter(|&id| !self.is_private(id))
    }

    #[must_use]
    pub(crate) fn public_tab_at(&self, index: usize) -> Option<DocumentId> {
        self.public_tabs().nth(index)
    }

    #[must_use]
    pub(crate) fn tab_index(&self, id: DocumentId) -> Option<usize> {
        self.tab_order.iter().position(|&tab| tab == id)
    }

    pub(crate) fn iter_documents(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    pub(crate) fn iter_documents_mut(&mut self) -> impl Iterator<Item = &mut Document> {
        self.documents.values_mut()
    }

    #[must_use]
    pub(crate) fn get_document_id_at(&self, index: usize) -> Option<DocumentId> {
        if index < self.tab_order.len() {
            Some(self.tab_order[index])
        } else {
            None
        }
    }

    fn find_open_document(&self, path: &Path) -> Option<usize> {
        let normalized_path = crate::fs_backend::backend().canonicalize(path);

        for (idx, &id) in self.tab_order.iter().enumerate() {
            if self.is_private(id) {
                continue;
            }
            if let Some(doc) = self.documents.get(&id) {
                if doc.path() == Some(normalized_path.as_path()) {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// # Panics
    /// Panics if a tab ID has no stored document, which the manager's invariants rule out.
    #[must_use]
    pub(crate) fn get_buffer_list(&self, active: DocumentId) -> Vec<BufferInfo> {
        self.public_tabs()
            .enumerate()
            .map(|(i, id)| {
                let doc = self.documents.get(&id).unwrap();
                BufferInfo {
                    id,
                    index: i,
                    name: doc.display_name().to_string(),
                    is_current: id == active,
                    status: BufferStatus {
                        is_dirty: doc.is_dirty(),
                        is_read_only: doc.is_read_only(),
                        is_special: doc.is_special(),
                    },
                }
            })
            .collect()
    }

    #[must_use]
    pub(crate) fn has_unsaved_changes(&self) -> bool {
        self.documents.values().any(Document::is_dirty)
    }
}

pub(crate) struct BufferInfo {
    pub(crate) id: DocumentId,
    pub(crate) index: usize,
    pub(crate) name: String,
    pub(crate) is_current: bool,
    pub(crate) status: BufferStatus,
}

/// Document-derived state of one buffer list entry.
pub(crate) struct BufferStatus {
    pub(crate) is_dirty: bool,
    pub(crate) is_read_only: bool,
    pub(crate) is_special: bool,
}

impl DocumentManager {
    #[must_use]
    pub(crate) fn is_private(&self, id: DocumentId) -> bool {
        self.private_document_ids.contains(&id)
    }

    #[must_use]
    pub(crate) fn find_open_document_id(&self, path: &Path) -> Option<DocumentId> {
        self.find_open_document(path)
            .and_then(|idx| self.get_document_id_at(idx))
    }
}

impl Default for DocumentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod manager_tests;

#[cfg(test)]
#[path = "tests/manager_tab_tests.rs"]
mod manager_tab_tests;
