use crate::document::{Document, DocumentId};
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
}

impl DocumentManager {
    /// Create a new document manager
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            tab_order: Vec::new(),
            current_tab: 0,
            next_document_id: 1,
        }
    }

    /// Add a document and make it active
    pub fn add_document(&mut self, document: Document) {
        let id = document.id;
        // Ensure we advance next ID if we're adding one manually
        if id >= self.next_document_id {
            self.next_document_id = id + 1;
        }

        self.documents.insert(id, document);
        self.tab_order.push(id);
        self.current_tab = self.tab_order.len() - 1;
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

    /// Remove a document by ID with strict tab semantics
    pub fn remove_document(&mut self, id: DocumentId) -> Result<(), RiftError> {
        // 1. Check if document exists
        if !self.documents.contains_key(&id) {
            return Ok(());
        }

        // 2. Check dirty state
        if self.documents.get(&id).unwrap().is_dirty() {
            return Err(RiftError::warning(
                ErrorType::Execution,
                crate::constants::errors::UNSAVED_CHANGES,
                crate::constants::errors::MSG_UNSAVED_CHANGES,
            ));
        }

        self.remove_document_inner(id)
    }

    /// Remove a document by ID, bypassing the dirty check.
    /// Used for terminal buffers which are always "dirty".
    pub fn remove_document_force(&mut self, id: DocumentId) -> Result<(), RiftError> {
        if !self.documents.contains_key(&id) {
            return Ok(());
        }
        self.remove_document_inner(id)
    }

    /// Internal removal logic shared by remove_document and remove_document_force
    fn remove_document_inner(&mut self, id: DocumentId) -> Result<(), RiftError> {
        // Auto-create new document if closing last tab
        if self.tab_order.len() == 1 {
            let new_id = self.next_document_id;
            let new_doc = Document::new(new_id).map_err(|e| {
                RiftError::new(
                    ErrorType::Internal,
                    crate::constants::errors::INTERNAL_ERROR,
                    e.to_string(),
                )
            })?;
            self.add_document(new_doc);
        }

        // Re-find position as it MUST exist (checked above) and tab_order might have changed if we added one
        let pos = self
            .tab_order
            .iter()
            .position(|&x| x == id)
            .expect("Document in storage but not in tab_order");

        // Remove
        self.tab_order.remove(pos);
        self.documents.remove(&id);

        // Update active tab
        if pos < self.current_tab {
            self.current_tab -= 1;
        } else if pos == self.current_tab && self.current_tab >= self.tab_order.len() {
            self.current_tab = self.tab_order.len().saturating_sub(1);
        }

        // Ensure bounds validation
        if self.current_tab >= self.tab_order.len() {
            self.current_tab = self.tab_order.len().saturating_sub(1);
        }

        Ok(())
    }

    /// Switch to next tab
    pub fn switch_next_tab(&mut self) {
        if self.tab_order.len() > 1 {
            self.current_tab = (self.current_tab + 1) % self.tab_order.len();
        }
    }

    /// Switch to previous tab
    pub fn switch_prev_tab(&mut self) {
        if self.tab_order.len() > 1 {
            if self.current_tab == 0 {
                self.current_tab = self.tab_order.len() - 1;
            } else {
                self.current_tab -= 1;
            }
        }
    }

    /// Get number of open tabs
    pub fn tab_count(&self) -> usize {
        self.tab_order.len()
    }

    /// Get current active tab index
    pub fn active_tab_index(&self) -> usize {
        self.current_tab
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
        let normalized_path = Self::normalize_path(path);

        for (idx, &id) in self.tab_order.iter().enumerate() {
            if let Some(doc) = self.documents.get(&id) {
                if let Some(doc_path) = doc.path() {
                    let doc_normalized = Self::normalize_path(doc_path);
                    if doc_normalized == normalized_path {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    /// Normalize a path to an absolute path for consistent comparison.
    /// Uses canonicalize for existing files, or constructs absolute path for new files.
    fn normalize_path(path: &Path) -> PathBuf {
        // Try canonicalize first (works if file exists)
        if let Ok(canonical) = std::fs::canonicalize(path) {
            return canonical;
        }

        // File doesn't exist yet - construct absolute path manually
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            // Prepend current working directory
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    }

    /// Open a file from disk, or create a new one if it doesn't exist
    fn open_existing_or_new_file(&mut self, path_str: &str) -> Result<(), RiftError> {
        let document_result = Document::from_file(self.next_document_id, path_str);

        let document = match document_result {
            Ok(doc) => doc,
            Err(e) => {
                if e.kind == ErrorType::Io
                    && e.message
                        .contains(crate::constants::errors::MSG_FILE_NOT_FOUND_WIN)
                {
                    if Path::new(path_str).exists() {
                        // File exists but we couldn't read it (AccessDenied,
                        // IsDir, etc.)
                        return Err(e);
                    } else {
                        // File doesn't exist, so we are creating a new one
                        let mut doc = Document::new(self.next_document_id)?;
                        doc.set_path(path_str);
                        doc
                    }
                } else {
                    return Err(e);
                }
            }
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
                    is_read_only: doc.is_read_only,
                    is_current: i == self.current_tab,
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
}

impl DocumentManager {
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
        self.find_open_document(path)
            .map(|idx| self.tab_order[idx])
    }
}

impl Default for DocumentManager {
    fn default() -> Self {
        Self::new()
    }
}
