//! Annotation system: structured sidecar metadata alongside buffer content.
//!
//! Annotations are a separate, persistent layer that live alongside buffer content
//! without polluting it. They survive edits and are extensible by plugins.
//!

/// Stable, unique identifier for an annotation. Does not change across edits.
pub type AnnotationId = u64;

/// Classifies what produced an annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationKind {
    /// A directory buffer entry mapping to a filesystem path.
    DirectoryEntry,
    /// An LSP diagnostic (error, warning, info, hint).
    LspDiagnostic,
    /// A git blame annotation.
    GitBlame,
    /// An inline hint (e.g. type hint from LSP).
    InlineHint,
    /// A user-created bookmark or mark.
    UserMark,
    /// A plugin-defined annotation kind.
    Plugin(String),
}

/// Identifies the subsystem or plugin that created an annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationOwner {
    /// Created by the editor core.
    System,
    /// Created by an LSP server.
    Lsp,
    /// Created by a named plugin.
    Plugin(String),
    /// Created by the user.
    User,
}

/// Where an annotation lives in the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// A single character offset. Moves with buffer edits via delta tracking.
    Point(usize),
    /// A character range [start, end). Both ends move with buffer edits.
    Range(usize, usize),
    /// An entire line identified by its zero-based line number.
    /// Deletes when the line is removed (Stickiness::Delete) or survives (Stickiness::Persist).
    Line(usize),
}

/// What happens to a Line-anchored annotation when its line is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stickiness {
    /// Annotation is removed when its anchor line is deleted.
    Delete,
    /// Annotation survives at the nearest remaining position.
    Persist,
}

/// A single annotation record.
#[derive(Debug, Clone)]
pub struct Annotation {
    pub id: AnnotationId,
    pub anchor: Anchor,
    pub kind: AnnotationKind,
    pub owner: AnnotationOwner,
    pub stickiness: Stickiness,
    pub visible: bool,
    pub read_only: bool,
    /// For `DirectoryEntry` annotations: the stable numeric entry ID (1-based).
    /// ID 0 is the sentinel "unassigned" value and is silently ignored by diff parsing.
    pub entry_id: Option<u16>,
    /// Optional tooltip shown when the cursor rests on this annotation.
    pub tooltip: Option<String>,
}

/// Persistent store of annotations for a single document.
///
/// Lives inside `Document`; never a global singleton. Position delta tracking
/// (via `on_lines_deleted` / `on_line_inserted`) is driven synchronously by the
/// document's edit pipeline.
pub struct AnnotationStore {
    annotations: Vec<Annotation>,
    next_id: AnnotationId,
}

impl Default for AnnotationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AnnotationStore {
    pub fn new() -> Self {
        Self {
            annotations: Vec::new(),
            next_id: 1,
        }
    }

    /// Remove all annotations from the store.
    pub fn clear(&mut self) {
        self.annotations.clear();
    }

    /// Create a `DirectoryEntry` annotation anchored at `line` with a stable entry ID.
    ///
    /// Returns the new annotation's ID.
    pub fn create_directory_entry(&mut self, line: usize, entry_id: u16) -> AnnotationId {
        let id = self.next_id;
        self.next_id += 1;
        self.annotations.push(Annotation {
            id,
            anchor: Anchor::Line(line),
            kind: AnnotationKind::DirectoryEntry,
            owner: AnnotationOwner::System,
            stickiness: Stickiness::Delete,
            visible: false,
            read_only: true,
            entry_id: Some(entry_id),
            tooltip: None,
        });
        id
    }

    /// Return the directory entry ID for the annotation anchored at the given line,
    /// or `None` if no `DirectoryEntry` annotation exists there.
    pub fn directory_entry_id_at_line(&self, line: usize) -> Option<u16> {
        self.annotations
            .iter()
            .find(|a| a.kind == AnnotationKind::DirectoryEntry && a.anchor == Anchor::Line(line))
            .and_then(|a| a.entry_id)
    }

    /// Return all `(line, entry_id)` pairs for directory entry annotations, sorted by line.
    pub fn directory_entries_by_line(&self) -> Vec<(usize, u16)> {
        let mut entries: Vec<(usize, u16)> = self
            .annotations
            .iter()
            .filter(|a| a.kind == AnnotationKind::DirectoryEntry)
            .filter_map(|a| {
                if let Anchor::Line(l) = a.anchor {
                    a.entry_id.map(|eid| (l, eid))
                } else {
                    None
                }
            })
            .collect();
        entries.sort_by_key(|&(l, _)| l);
        entries
    }

    /// Update `Line` anchors after `count` lines are deleted starting at `first_line`.
    ///
    /// Annotations with `Stickiness::Delete` that fall within the deleted range are
    /// removed. All surviving annotations at lines >= `first_line + count` shift down
    /// by `count`.
    pub fn on_lines_deleted(&mut self, first_line: usize, count: usize) {
        if count == 0 {
            return;
        }
        let last_exclusive = first_line + count;
        self.annotations.retain(|a| {
            if let Anchor::Line(l) = a.anchor {
                if l >= first_line && l < last_exclusive {
                    // In-range: keep only if Persist
                    a.stickiness == Stickiness::Persist
                } else {
                    true
                }
            } else {
                true
            }
        });
        for a in &mut self.annotations {
            if let Anchor::Line(ref mut l) = a.anchor {
                if *l >= last_exclusive {
                    *l -= count;
                }
            }
        }
    }

    /// Update `Line` anchors after a new line is inserted at `at_line`.
    ///
    /// All annotations at lines >= `at_line` shift up by 1.
    pub fn on_line_inserted(&mut self, at_line: usize) {
        for a in &mut self.annotations {
            if let Anchor::Line(ref mut l) = a.anchor {
                if *l >= at_line {
                    *l += 1;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
