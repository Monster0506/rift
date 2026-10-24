use crate::document::runtime::DocumentHandle;
use crate::document::{Document, DocumentId};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

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
    const fn new(id: DocumentId, handle: DocumentHandle) -> Self {
        Self { id, handle }
    }

    #[must_use]
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

pub(crate) struct PreparedRemoval {
    id: DocumentId,
    intent: RemovalIntent,
    replacement: Option<Document>,
}

impl std::fmt::Debug for PreparedRemoval {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedRemoval")
            .field("id", &self.id)
            .field("intent", &self.intent)
            .field("has_replacement", &self.replacement.is_some())
            .finish()
    }
}

impl PreparedRemoval {
    #[cfg(test)]
    const fn id(&self) -> DocumentId {
        self.id
    }

    #[cfg(test)]
    const fn intent(&self) -> RemovalIntent {
        self.intent
    }

    pub(crate) const fn has_replacement(&self) -> bool {
        self.replacement.is_some()
    }
}

pub(crate) struct DocumentManager {
    documents: BTreeMap<DocumentId, Document>,
    tab_order: Vec<DocumentId>,
    current_tab: usize,
    next_document_id: DocumentId,
    private_document_ids: BTreeSet<DocumentId>,
    most_recent_ghost_doc: Option<DocumentId>,
    next_instance: u64,
    handles: HashMap<DocumentId, DocumentHandle>,
}

impl DocumentManager {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            documents: BTreeMap::new(),
            tab_order: Vec::new(),
            current_tab: 0,
            next_document_id: DocumentId::new(1),
            private_document_ids: BTreeSet::new(),
            most_recent_ghost_doc: None,
            next_instance: 1,
            handles: HashMap::new(),
        }
    }

    #[must_use]
    pub(crate) const fn most_recent_ghost_doc(&self) -> Option<DocumentId> {
        self.most_recent_ghost_doc
    }

    pub(crate) const fn set_most_recent_ghost_doc(&mut self, id: Option<DocumentId>) {
        self.most_recent_ghost_doc = id;
    }

    #[must_use]
    pub(crate) fn get_handle(&self, id: DocumentId) -> Option<DocumentHandle> {
        self.handles.get(&id).copied()
    }

    #[cfg(test)]
    #[must_use]
    fn active_document_handle(&self) -> Option<DocumentHandle> {
        let id = self.active_document_id()?;
        self.get_handle(id)
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

    pub(crate) const fn reserve_creation(&mut self) -> CreationReservation {
        let id = self.next_document_id;
        self.next_document_id = self.next_document_id.next();
        let instance = self.next_instance;
        self.next_instance += 1;
        let handle = DocumentHandle::new(id, instance);
        CreationReservation::new(id, handle)
    }

    pub(crate) fn commit_draft_active(&mut self, draft: DocumentDraft) -> DocumentHandle {
        let handle = draft.reservation.handle();
        let doc = draft.into_document();
        let id = doc.id;
        self.handles.insert(id, handle);
        self.add_document(doc);
        handle
    }

    pub(crate) fn add_document(&mut self, mut document: Document) {
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
        self.current_tab = self.tab_order.len() - 1;
    }

    pub(crate) fn add_document_inactive(&mut self, mut document: Document) {
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
    }

    #[must_use]
    pub(crate) fn active_document_id(&self) -> Option<DocumentId> {
        if self.tab_order.is_empty() {
            None
        } else {
            Some(self.tab_order[self.current_tab])
        }
    }

    #[must_use]
    pub(crate) fn active_document(&self) -> Option<&Document> {
        let id = self.active_document_id()?;
        self.documents.get(&id)
    }

    pub(crate) fn active_document_mut(&mut self) -> Option<&mut Document> {
        let id = self.active_document_id()?;
        self.documents.get_mut(&id)
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

    pub(crate) fn check_invariants(&self) -> Vec<String> {
        let mut out = Vec::new();

        for id in &self.tab_order {
            if !self.documents.contains_key(id) {
                out.push(format!("tab_order references missing document {id}"));
            }
        }
        if self.tab_order.is_empty() {
            if self.current_tab != 0 {
                out.push(format!(
                    "tab_order is empty but current_tab={}",
                    self.current_tab
                ));
            }
        } else if self.current_tab >= self.tab_order.len() {
            out.push(format!(
                "current_tab {} >= tab_order.len() {}",
                self.current_tab,
                self.tab_order.len()
            ));
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
    pub(crate) const fn next_id(&self) -> DocumentId {
        self.next_document_id
    }

    /// # Errors
    /// Returns an error if `id` is not in the tab order.
    pub(crate) fn switch_to_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        if let Some(pos) = self.tab_order.iter().position(|&x| x == id) {
            self.current_tab = pos;
            Ok(())
        } else {
            Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!("Document {id} not found in tabs"),
            ))
        }
    }

    /// # Errors
    /// Returns an error if the document is missing or not in the tab order, has unsaved changes
    /// under a `Normal` removal, or its replacement cannot be created.
    pub(crate) fn prepare_removal(
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

        if !self.tab_order.contains(&id) {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!("Document {id} in storage but not in tab order"),
            ));
        }

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

        let replacement = if self.tab_order.len() == 1 {
            let new_doc = Document::new(self.next_document_id).map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    e.to_string(),
                )
            })?;
            Some(new_doc)
        } else {
            None
        };

        Ok(PreparedRemoval {
            id,
            intent,
            replacement,
        })
    }

    pub(crate) fn commit_removal(&mut self, plan: PreparedRemoval) -> Option<Document> {
        if let Some(replacement) = plan.replacement {
            self.add_document(replacement);
        }

        if let Some(pos) = self.tab_order.iter().position(|&x| x == plan.id) {
            self.tab_order.remove(pos);
            if pos < self.current_tab {
                self.current_tab -= 1;
            } else if pos == self.current_tab && self.current_tab >= self.tab_order.len() {
                self.current_tab = self.tab_order.len().saturating_sub(1);
            }
            if self.current_tab >= self.tab_order.len() {
                self.current_tab = self.tab_order.len().saturating_sub(1);
            }
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
        self.commit_removal(plan);
        Ok(())
    }

    pub(crate) fn switch_next_tab(&mut self) {
        let public_tabs: Vec<usize> = self
            .tab_order
            .iter()
            .enumerate()
            .filter(|(_, id)| !self.private_document_ids.contains(*id))
            .map(|(i, _)| i)
            .collect();
        if public_tabs.is_empty() {
            return;
        }
        let current_pos = public_tabs.iter().position(|&i| i == self.current_tab);
        match current_pos {
            None => self.current_tab = public_tabs[0],
            Some(pos) if public_tabs.len() > 1 => {
                let next_pos = (pos + 1) % public_tabs.len();
                self.current_tab = public_tabs[next_pos];
            }
            Some(_) => {}
        }
    }

    pub(crate) fn switch_prev_tab(&mut self) {
        let public_tabs: Vec<usize> = self
            .tab_order
            .iter()
            .enumerate()
            .filter(|(_, id)| !self.private_document_ids.contains(*id))
            .map(|(i, _)| i)
            .collect();
        if public_tabs.is_empty() {
            return;
        }
        let current_pos = public_tabs.iter().position(|&i| i == self.current_tab);
        match current_pos {
            None => self.current_tab = public_tabs[0],
            Some(pos) if public_tabs.len() > 1 => {
                let prev_pos = if pos == 0 {
                    public_tabs.len() - 1
                } else {
                    pos - 1
                };
                self.current_tab = public_tabs[prev_pos];
            }
            Some(_) => {}
        }
    }

    #[must_use]
    pub(crate) fn tab_count(&self) -> usize {
        self.tab_order
            .iter()
            .filter(|id| !self.private_document_ids.contains(*id))
            .count()
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn active_tab_index(&self) -> usize {
        self.current_tab
    }

    pub(crate) fn iter_documents(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    #[cfg(test)]
    pub(super) fn iter_documents_mut(&mut self) -> impl Iterator<Item = &mut Document> {
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

    /// # Errors
    /// Returns an error if the file cannot be opened or reloaded, including when the path is
    /// a directory, its parent is missing, or there are unsaved changes without `force`.
    pub(crate) fn open_file(
        &mut self,
        file_path: Option<String>,
        force: bool,
    ) -> Result<(), RiftError> {
        if let Some(path_str) = file_path {
            let path = PathBuf::from(&path_str);
            if let Some(tab_idx) = self.find_open_document(&path) {
                self.current_tab = tab_idx;
                return Ok(());
            }

            self.open_existing_or_new_file(&path_str)
        } else {
            self.reload_current_file(force)
        }
    }

    fn find_open_document(&self, path: &Path) -> Option<usize> {
        let normalized_path = crate::fs_backend::backend().canonicalize(path);

        for (idx, &id) in self.tab_order.iter().enumerate() {
            if let Some(doc) = self.documents.get(&id) {
                if doc.path() == Some(normalized_path.as_path()) {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn open_existing_or_new_file(&mut self, path_str: &str) -> Result<(), RiftError> {
        let path = Path::new(path_str);

        let document = if path.exists() {
            if path.is_dir() {
                return Err(RiftError::new(
                    ErrorType::Execution,
                    crate::constants::errors::NOT_A_FILE,
                    crate::constants::errors::MSG_NOT_A_FILE,
                ));
            }
            match Document::try_open_undo_file(self.next_document_id, path) {
                Some(result) => result?,
                None => Document::from_file(self.next_document_id, path_str)?,
            }
        } else if parent_dir_missing(path) {
            return Err(RiftError::new(
                ErrorType::Io,
                crate::constants::errors::PARENT_DIR_MISSING,
                crate::constants::errors::MSG_PARENT_DIR_MISSING,
            ));
        } else {
            let mut doc = Document::new(self.next_document_id)?;
            doc.set_path(path_str);
            doc
        };

        self.add_document(document);
        Ok(())
    }

    fn reload_current_file(&mut self, force: bool) -> Result<(), RiftError> {
        let (is_dirty, has_path) = {
            let doc = self.active_document().ok_or_else(|| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    "No active document",
                )
            })?;
            (doc.is_dirty(), doc.has_path())
        };

        if !force && is_dirty {
            return Err(RiftError {
                severity: ErrorSeverity::Warning,
                kind: ErrorType::Execution,
                code: crate::constants::errors::UNSAVED_CHANGES.to_string(),
                message: crate::constants::errors::MSG_UNSAVED_CHANGES.to_string(),
            });
        }

        if has_path {
            self.active_document_mut().unwrap().reload_from_disk()?;
        } else {
            return Err(RiftError::new(
                ErrorType::Execution,
                crate::constants::errors::NO_PATH,
                crate::constants::errors::MSG_NO_FILE_NAME,
            ));
        }
        Ok(())
    }
    /// # Panics
    /// Panics if a tab ID has no stored document, which the manager's invariants rule out.
    #[must_use]
    pub(crate) fn get_buffer_list(&self) -> Vec<BufferInfo> {
        self.tab_order
            .iter()
            .enumerate()
            .map(|(i, &id)| {
                let doc = self.documents.get(&id).unwrap();
                BufferInfo {
                    id,
                    index: i,
                    name: doc.display_name().to_string(),
                    is_current: i == self.current_tab,
                    status: BufferStatus {
                        is_dirty: doc.is_dirty(),
                        is_read_only: doc.is_read_only(),
                        is_special: doc.is_special(),
                    },
                }
            })
            .collect()
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn has_unsaved_changes(&self) -> bool {
        self.documents.values().any(super::Document::is_dirty)
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

    pub(crate) fn add_private_document(&mut self, mut doc: Document) -> DocumentId {
        let id = doc.id;
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
        doc.set_handle(handle);
        self.documents.insert(id, doc);
        self.tab_order.push(id);
        self.current_tab = self.tab_order.len() - 1;
        self.private_document_ids.insert(id);
        id
    }

    /// # Errors
    /// Returns an error if the new document cannot be created.
    pub(crate) fn create_placeholder(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<DocumentId, RiftError> {
        let mut doc = Document::new(self.next_document_id).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                e.to_string(),
            )
        })?;
        doc.set_path(path.as_ref());
        let id = doc.id;
        self.add_document(doc);
        Ok(id)
    }

    #[must_use]
    pub(crate) fn find_open_document_index(&self, path: &Path) -> Option<usize> {
        self.find_open_document(path)
    }

    #[must_use]
    pub(crate) fn find_open_document_id(&self, path: &Path) -> Option<DocumentId> {
        self.find_open_document(path).map(|idx| self.tab_order[idx])
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
