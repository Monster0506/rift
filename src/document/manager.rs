use crate::document::{Document, DocumentHandle, DocumentId};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Whether `path`'s parent directory exists, so a path can only be opened as
/// a new file if no missing directories would need to be created for it.
pub(crate) fn parent_dir_missing(path: &Path) -> bool {
    crate::fs_backend::backend().parent_dir_missing(path)
}

/// Intent for document removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalIntent {
    /// Normal close: checks dirty state and close policy.
    Normal,
    /// Forced close: bypasses dirty checks (e.g. :q! or terminal buffer).
    Force,
}

impl RemovalIntent {
    /// Returns true if this removal is forced.
    pub fn is_forced(self) -> bool {
        matches!(self, Self::Force)
    }
}

/// A reservation for a pending document creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreationReservation {
    id: DocumentId,
    handle: DocumentHandle,
}

impl CreationReservation {
    pub const fn new(id: DocumentId, handle: DocumentHandle) -> Self {
        Self { id, handle }
    }

    pub const fn id(&self) -> DocumentId {
        self.id
    }

    pub const fn handle(&self) -> DocumentHandle {
        self.handle
    }
}

/// An uncommitted document draft that is not yet visible in tabs or document maps.
pub struct DocumentDraft {
    reservation: CreationReservation,
    document: Document,
}

impl DocumentDraft {
    pub fn new(reservation: CreationReservation, document: Document) -> Result<Self, RiftError> {
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

    pub fn reservation(&self) -> CreationReservation {
        self.reservation
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    pub fn into_document(self) -> Document {
        self.document
    }
}

/// Placement target for a committed document draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftCommitTarget {
    /// Add as the active tab.
    ActiveTab,
    /// Add as an inactive tab.
    InactiveTab,
    /// Add as a private document hidden from tab navigation.
    Private,
}

/// Prepared, side-effect-free document removal plan: preflight validation
/// plus pre-materialized replacement state so commit can't fail.
pub struct PreparedRemoval {
    /// Document to remove.
    pub id: DocumentId,
    /// Intent with which removal was requested.
    pub intent: RemovalIntent,
    /// Pre-materialized replacement document if closing the last tab.
    pub replacement: Option<Document>,
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

pub type RemovalPlan = PreparedRemoval;

impl PreparedRemoval {
    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn intent(&self) -> RemovalIntent {
        self.intent
    }

    pub fn replacement_id(&self) -> Option<DocumentId> {
        self.replacement.as_ref().map(|d| d.id)
    }

    pub fn has_replacement(&self) -> bool {
        self.replacement.is_some()
    }
}

/// Manages multiple open documents (tabs)
pub struct DocumentManager {
    /// Active documents mapped by ID
    documents: HashMap<DocumentId, Document>,
    /// Order of documents in tabs
    tab_order: Vec<DocumentId>,
    /// Index of current active tab
    current_tab: usize,
    /// Next available document ID
    next_document_id: DocumentId,
    private_document_ids: HashSet<DocumentId>,
    /// Which document holds the most recently created ghost cut, for Put's
    /// "paste this specific cut back" resolution.
    most_recent_ghost_doc: Option<DocumentId>,
    /// Next available instance generation for DocumentHandle
    next_instance: u64,
    /// Active document handles mapped by DocumentId
    handles: HashMap<DocumentId, DocumentHandle>,
}

impl DocumentManager {
    /// Create a new document manager
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            tab_order: Vec::new(),
            current_tab: 0,
            next_document_id: 1,
            private_document_ids: HashSet::new(),
            most_recent_ghost_doc: None,
            next_instance: 1,
            handles: HashMap::new(),
        }
    }

    /// Which document holds the most recently created ghost cut, if any.
    pub fn most_recent_ghost_doc(&self) -> Option<DocumentId> {
        self.most_recent_ghost_doc
    }

    /// Record which document holds the most recently created ghost cut.
    pub fn set_most_recent_ghost_doc(&mut self, id: Option<DocumentId>) {
        self.most_recent_ghost_doc = id;
    }

    /// Get document handle by document ID
    pub fn get_handle(&self, id: DocumentId) -> Option<DocumentHandle> {
        self.handles.get(&id).copied()
    }

    /// Get handle of the active document
    pub fn active_document_handle(&self) -> Option<DocumentHandle> {
        let id = self.active_document_id()?;
        self.get_handle(id)
    }

    /// Get document by handle, verifying instance generation
    pub fn get_document_by_handle(&self, handle: DocumentHandle) -> Option<&Document> {
        if self.handles.get(&handle.doc_id) == Some(&handle) {
            self.documents.get(&handle.doc_id)
        } else {
            None
        }
    }

    /// Get mutable document by handle, verifying instance generation
    pub fn get_document_by_handle_mut(&mut self, handle: DocumentHandle) -> Option<&mut Document> {
        if self.handles.get(&handle.doc_id) == Some(&handle) {
            self.documents.get_mut(&handle.doc_id)
        } else {
            None
        }
    }

    /// Reserve a fresh document ID and handle for two-stage creation.
    pub fn reserve_creation(&mut self) -> CreationReservation {
        let id = self.next_document_id;
        self.next_document_id += 1;
        let instance = self.next_instance;
        self.next_instance += 1;
        let handle = DocumentHandle::new(id, instance);
        CreationReservation::new(id, handle)
    }

    /// Commit a prepared document draft through an infallible insertion path.
    pub fn commit_draft(
        &mut self,
        draft: DocumentDraft,
        target: DraftCommitTarget,
    ) -> DocumentHandle {
        let handle = draft.reservation.handle();
        let doc = draft.into_document();
        let id = doc.id;
        self.handles.insert(id, handle);
        match target {
            DraftCommitTarget::ActiveTab => {
                self.add_document(doc);
            }
            DraftCommitTarget::InactiveTab => {
                self.add_document_inactive(doc);
            }
            DraftCommitTarget::Private => {
                let _ = self.add_private_document(doc);
            }
        }
        handle
    }

    /// Commit a prepared draft as an active tab.
    pub fn commit_draft_active(&mut self, draft: DocumentDraft) -> DocumentHandle {
        self.commit_draft(draft, DraftCommitTarget::ActiveTab)
    }

    /// Commit a prepared draft as an inactive tab.
    pub fn commit_draft_inactive(&mut self, draft: DocumentDraft) -> DocumentHandle {
        self.commit_draft(draft, DraftCommitTarget::InactiveTab)
    }

    /// Commit a prepared draft as a private document.
    pub fn commit_draft_private(&mut self, draft: DocumentDraft) -> DocumentHandle {
        self.commit_draft(draft, DraftCommitTarget::Private)
    }

    /// Add a document and make it active
    pub fn add_document(&mut self, mut document: Document) {
        let id = document.id;
        if id >= self.next_document_id {
            self.next_document_id = id + 1;
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

    /// Add a document as a tab without making it active.
    pub fn add_document_inactive(&mut self, mut document: Document) {
        let id = document.id;
        if id >= self.next_document_id {
            self.next_document_id = id + 1;
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

    /// Get ID of the active document
    pub fn active_document_id(&self) -> Option<DocumentId> {
        if self.tab_order.is_empty() {
            None
        } else {
            Some(self.tab_order[self.current_tab])
        }
    }

    /// Get reference to active document
    pub fn active_document(&self) -> Option<&Document> {
        let id = self.active_document_id()?;
        self.documents.get(&id)
    }

    /// Get mutable reference to active document
    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        let id = self.active_document_id()?;
        self.documents.get_mut(&id)
    }

    /// Get document by ID
    pub fn get_document(&self, id: DocumentId) -> Option<&Document> {
        self.documents.get(&id)
    }

    /// Get mutable document by ID
    pub fn get_document_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        self.documents.get_mut(&id)
    }

    /// Iterate over every open document (tabs and private), in no particular order.
    pub fn documents_iter(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    /// Find the ID of an open messages buffer, if any.
    pub fn find_messages_doc_id(&self) -> Option<DocumentId> {
        self.documents
            .iter()
            .find(|(_, d)| d.is_messages())
            .map(|(id, _)| *id)
    }

    /// Get next available document ID
    pub fn next_id(&self) -> DocumentId {
        self.next_document_id
    }

    /// Switch active tab to specific document ID
    pub fn switch_to_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        if let Some(pos) = self.tab_order.iter().position(|&x| x == id) {
            self.current_tab = pos;
            Ok(())
        } else {
            Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!("Document {} not found in tabs", id),
            ))
        }
    }

    /// Side-effect-free preflight for document removal; materializes a
    /// last-tab replacement so `commit_removal` cannot fail.
    pub fn prepare_removal(
        &self,
        id: DocumentId,
        intent: RemovalIntent,
    ) -> Result<PreparedRemoval, RiftError> {
        let doc = self.documents.get(&id).ok_or_else(|| {
            RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!("Document {} not found", id),
            )
        })?;

        // Verify document is in tab order
        if !self.tab_order.contains(&id) {
            return Err(RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                format!("Document {} in storage but not in tab order", id),
            ));
        }

        if intent == RemovalIntent::Normal
            && doc.policies().close == crate::document::ClosePolicy::ConfirmDirty
            && doc.is_dirty()
        {
            return Err(RiftError::warning(
                ErrorType::Execution,
                crate::constants::errors::UNSAVED_CHANGES,
                crate::constants::errors::MSG_UNSAVED_CHANGES,
            ));
        }

        // Pre-create replacement document if closing the last tab so commit cannot fail
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

    /// Commit a prepared removal plan; cannot fail after preflight.
    pub fn commit_removal(&mut self, plan: PreparedRemoval) -> Option<Document> {
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

    /// Remove a document by ID with strict tab semantics
    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        if !self.documents.contains_key(&id) {
            return Ok(());
        }
        let plan = self.prepare_removal(id, RemovalIntent::Normal)?;
        self.commit_removal(plan);
        Ok(())
    }

    /// Remove a document by ID, bypassing the dirty check.
    /// Used for terminal buffers which are always "dirty".
    pub fn remove_document_force(&mut self, id: DocumentId) -> Result<(), RiftError> {
        if !self.documents.contains_key(&id) {
            return Ok(());
        }
        let plan = self.prepare_removal(id, RemovalIntent::Force)?;
        self.commit_removal(plan);
        Ok(())
    }

    /// Switch to next tab
    pub fn switch_next_tab(&mut self) {
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
            // Current tab is private (e.g. a frozen/preview buffer): land on
            // the first public tab instead of no-op'ing.
            None => self.current_tab = public_tabs[0],
            Some(pos) if public_tabs.len() > 1 => {
                let next_pos = (pos + 1) % public_tabs.len();
                self.current_tab = public_tabs[next_pos];
            }
            Some(_) => {}
        }
    }

    /// Switch to previous tab
    pub fn switch_prev_tab(&mut self) {
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

    /// Get number of open tabs
    pub fn tab_count(&self) -> usize {
        self.tab_order
            .iter()
            .filter(|id| !self.private_document_ids.contains(*id))
            .count()
    }

    /// Get current active tab index
    pub fn active_tab_index(&self) -> usize {
        self.current_tab
    }

    /// Iterate over all documents (including private ones)
    pub fn iter_documents(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    /// Iterate mutably over all documents (including private ones)
    pub fn iter_documents_mut(&mut self) -> impl Iterator<Item = &mut Document> {
        self.documents.values_mut()
    }

    /// Get document ID at specific tab index
    pub fn get_document_id_at(&self, index: usize) -> Option<DocumentId> {
        if index < self.tab_order.len() {
            Some(self.tab_order[index])
        } else {
            None
        }
    }

    /// Open a file (or verify if already open)
    pub fn open_file(&mut self, file_path: Option<String>, force: bool) -> Result<(), RiftError> {
        if let Some(path_str) = file_path {
            // Check if already open
            let path = PathBuf::from(&path_str);
            if let Some(tab_idx) = self.find_open_document(&path) {
                self.current_tab = tab_idx;
                return Ok(());
            }

            // Not open, try to load it
            self.open_existing_or_new_file(&path_str)
        } else {
            // Reload current file
            self.reload_current_file(force)
        }
    }

    /// Find if a document with the given path is already open
    /// Returns the tab index if found
    fn find_open_document(&self, path: &Path) -> Option<usize> {
        let normalized_path = crate::fs_backend::backend().canonicalize(path);

        // doc.path() is already normalized (set_path/from_bytes do it at
        // write time), so only the incoming target needs it here.
        for (idx, &id) in self.tab_order.iter().enumerate() {
            if let Some(doc) = self.documents.get(&id) {
                if doc.path() == Some(normalized_path.as_path()) {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Open a file from disk, or create a new one if it doesn't exist
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
            Document::from_file(self.next_document_id, path_str)?
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

    /// Reload the current active document from disk
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
    /// Get summary of all open buffers
    pub fn get_buffer_list(&self) -> Vec<BufferInfo> {
        self.tab_order
            .iter()
            .enumerate()
            .map(|(i, &id)| {
                let doc = self.documents.get(&id).unwrap();
                BufferInfo {
                    id,
                    index: i,
                    name: doc.display_name().to_string(),
                    is_dirty: doc.is_dirty(),
                    is_read_only: doc.is_read_only(),
                    is_current: i == self.current_tab,
                    is_special: doc.is_special(),
                }
            })
            .collect()
    }

    /// Check if any document has unsaved changes
    pub fn has_unsaved_changes(&self) -> bool {
        self.documents.values().any(|doc| doc.is_dirty())
    }

    /// Get list of documents with unsaved changes
    pub fn get_unsaved_documents(&self) -> Vec<String> {
        self.documents
            .values()
            .filter(|doc| doc.is_dirty())
            .map(|doc| doc.display_name().to_string())
            .collect()
    }
}

/// Summary information about a buffer for listing
pub struct BufferInfo {
    pub id: DocumentId,
    pub index: usize,
    pub name: String,
    pub is_dirty: bool,
    pub is_read_only: bool,
    pub is_current: bool,
    pub is_special: bool,
}

impl DocumentManager {
    /// Create a private document with a cloned buffer for frozen window isolation.
    /// Not shown in the tab list or buffer navigation.
    pub fn create_private_document(
        &mut self,
        source_buffer: &crate::buffer::TextBuffer,
    ) -> Result<DocumentId, RiftError> {
        let mut doc = Document::new(self.next_document_id).map_err(|e| {
            RiftError::new(
                ErrorType::Internal,
                crate::constants::errors::INTERNAL_ERROR,
                e.to_string(),
            )
        })?;
        doc.buffer = source_buffer.clone();
        let id = doc.id;
        self.add_document(doc);
        self.private_document_ids.insert(id);
        Ok(id)
    }

    /// Remove a private document.
    pub fn remove_private_document(&mut self, id: DocumentId) {
        if self.private_document_ids.remove(&id) {
            let _ = self.remove_document_force(id);
        }
    }

    /// Whether `id` belongs to a panel-owned document.
    pub fn is_private(&self, id: DocumentId) -> bool {
        self.private_document_ids.contains(&id)
    }

    /// Add a fully-constructed document and mark it as private (hidden from tabs).
    pub fn add_private_document(&mut self, mut doc: Document) -> DocumentId {
        let id = doc.id;
        if id >= self.next_document_id {
            self.next_document_id = id + 1;
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

    /// Create a placeholder document for async loading
    pub fn create_placeholder(&mut self, path: impl AsRef<Path>) -> Result<DocumentId, RiftError> {
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

    /// Find if a document with the given path is already open
    /// Returns the tab index if found
    pub fn find_open_document_index(&self, path: &Path) -> Option<usize> {
        self.find_open_document(path)
    }

    pub fn find_open_document_id(&self, path: &Path) -> Option<DocumentId> {
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
