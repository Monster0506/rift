//! Undo/Redo history management with undo tree
//!
//! This module provides a transaction-centric undo tree where:
//! - Every user command is one atomic undo entry (transaction)
//! - Transactions may contain multiple low-level edits
//! - Branches preserve alternative edit histories
//! - Checkpoints enable efficient navigation to distant states

pub mod command;

use crate::character::Character;
use std::collections::HashMap;
use std::time::SystemTime;

/// Unique sequential identifier for each edit
pub type EditSeq = u64;

// =============================================================================
// Position and Range Types
// =============================================================================

/// Position in document (line, column)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Position {
    pub line: u32,
    pub col: u32,
}

impl Position {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

/// Range spanning start to end positions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Check if range is empty (start == end)
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

// =============================================================================
// Edit Operations
// =============================================================================

/// A single atomic edit operation in the document
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOperation {
    /// Insert text at position
    Insert {
        position: Position,
        text: Vec<Character>,
        len: usize,
    },

    /// Delete text in range
    Delete {
        range: Range,
        deleted_text: Vec<Character>,
    },

    /// Replace text (atomic delete + insert)
    Replace {
        range: Range,
        old_text: Vec<Character>,
        new_text: Vec<Character>,
    },

    /// Multi-line block change (e.g., reformat, sort lines)
    BlockChange {
        range: Range,
        old_content: Vec<Vec<Character>>,
        new_content: Vec<Vec<Character>>,
    },
}

/// Error applying an operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    PositionOutOfBounds { position: Position },
    InvalidRange { range: Range },
    EncodingError(String),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::PositionOutOfBounds { position } => {
                write!(f, "Position out of bounds: {:?}", position)
            }
            ApplyError::InvalidRange { range } => {
                write!(f, "Invalid range: {:?}", range)
            }
            ApplyError::EncodingError(msg) => write!(f, "Encoding error: {}", msg),
        }
    }
}

impl std::error::Error for ApplyError {}

impl EditOperation {
    /// Get the inverse operation (for undo)
    #[must_use]
    pub fn inverse(&self) -> EditOperation {
        match self {
            EditOperation::Insert { position, text, .. } => EditOperation::Delete {
                range: Range::new(
                    *position,
                    Position::new(
                        position.line
                            + text.iter().filter(|c| **c == Character::Newline).count() as u32,
                        if text.contains(&Character::Newline) {
                            text.iter()
                                .rev()
                                .take_while(|c| **c != Character::Newline)
                                .count() as u32
                        } else {
                            position.col + text.len() as u32
                        },
                    ),
                ),
                deleted_text: text.clone(),
            },

            EditOperation::Delete {
                range,
                deleted_text,
            } => EditOperation::Insert {
                position: range.start,
                text: deleted_text.clone(),
                len: deleted_text.len(),
            },

            EditOperation::Replace {
                range,
                old_text,
                new_text,
            } => EditOperation::Replace {
                range: Range::new(
                    range.start,
                    Position::new(
                        range.start.line
                            + new_text
                                .iter()
                                .filter(|c| **c == Character::Newline)
                                .count() as u32,
                        if new_text.contains(&Character::Newline) {
                            new_text
                                .iter()
                                .rev()
                                .take_while(|c| **c != Character::Newline)
                                .count() as u32
                        } else {
                            range.start.col + new_text.len() as u32
                        },
                    ),
                ),
                old_text: new_text.clone(),
                new_text: old_text.clone(),
            },

            EditOperation::BlockChange {
                range,
                old_content,
                new_content,
            } => EditOperation::BlockChange {
                range: Range::new(
                    range.start,
                    Position::new(
                        range.start.line + new_content.len().saturating_sub(1) as u32,
                        new_content.last().map_or(0, |s| s.len() as u32),
                    ),
                ),
                old_content: new_content.clone(),
                new_content: old_content.clone(),
            },
        }
    }

    /// Get minimal diff size (for memory estimation)
    #[must_use]
    pub fn estimated_size(&self) -> usize {
        match self {
            EditOperation::Insert { text, .. } => text.len() + 32,
            EditOperation::Delete { deleted_text, .. } => deleted_text.len() + 32,
            EditOperation::Replace {
                old_text, new_text, ..
            } => old_text.len() + new_text.len() + 32,
            EditOperation::BlockChange {
                old_content,
                new_content,
                ..
            } => {
                old_content.iter().map(|s| s.len()).sum::<usize>()
                    + new_content.iter().map(|s| s.len()).sum::<usize>()
                    + 32
            }
        }
    }

    /// Describe operation for UI (e.g., "Delete 42 chars")
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            EditOperation::Insert { text, .. } => {
                if text.len() <= 20 {
                    let s: String = text
                        .iter()
                        .map(|c| c.to_char_lossy())
                        .collect::<String>()
                        .replace('\n', "\\n");
                    format!("Insert '{}'", s)
                } else {
                    format!("Insert {} chars", text.len())
                }
            }
            EditOperation::Delete { deleted_text, .. } => {
                if deleted_text.len() <= 20 {
                    let s: String = deleted_text
                        .iter()
                        .map(|c| c.to_char_lossy())
                        .collect::<String>()
                        .replace('\n', "\\n");
                    format!("Delete '{}'", s)
                } else {
                    format!("Delete {} chars", deleted_text.len())
                }
            }
            EditOperation::Replace {
                old_text, new_text, ..
            } => {
                format!(
                    "Replace {} chars with {} chars",
                    old_text.len(),
                    new_text.len()
                )
            }
            EditOperation::BlockChange {
                old_content,
                new_content,
                ..
            } => {
                format!(
                    "Change {} lines to {} lines",
                    old_content.len(),
                    new_content.len()
                )
            }
        }
    }
}

// =============================================================================
// Transaction
// =============================================================================

/// Transaction groups multiple edits into one undo entry
#[derive(Clone, Debug, Default)]
pub struct EditTransaction {
    pub ops: Vec<EditOperation>,
    pub description: String,
}

impl EditTransaction {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            ops: Vec::new(),
            description: description.into(),
        }
    }

    /// Record operation within transaction
    pub fn record(&mut self, operation: EditOperation) {
        self.ops.push(operation);
    }

    /// Get inverse operations in REVERSE order (for undo)
    #[must_use]
    pub fn inverse(&self) -> Vec<EditOperation> {
        self.ops.iter().rev().map(|op| op.inverse()).collect()
    }

    /// Check if transaction is empty
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Estimated memory size
    pub fn estimated_size(&self) -> usize {
        self.ops.iter().map(|op| op.estimated_size()).sum::<usize>() + self.description.len() + 16
    }
}

// =============================================================================
// Snapshot for Checkpoints
// =============================================================================

/// Snapshot for checkpoint nodes (delta strategy)
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    pub full_text: Vec<Character>,
    pub byte_count: usize,
    pub line_count: u32,
}

impl DocumentSnapshot {
    pub fn new(text: Vec<Character>) -> Self {
        let byte_count = text.len();
        let line_count = text.iter().filter(|c| **c == Character::Newline).count() as u32 + 1;
        Self {
            full_text: text,
            byte_count,
            line_count,
        }
    }
}

// =============================================================================
// Edit Node
// =============================================================================

/// A node in the undo tree
#[derive(Clone, Debug)]
pub struct EditNode {
    pub seq: EditSeq,
    pub transaction: EditTransaction,
    pub parent: Option<EditSeq>,
    pub children: Vec<EditSeq>,
    /// Which child was last visited (for redo path tracking)
    pub last_visited_child: Option<usize>,
    /// Snapshot if this is a checkpoint node
    pub snapshot: Option<Box<DocumentSnapshot>>,
    /// Timestamp when edit was made
    pub timestamp: SystemTime,
}

impl EditNode {
    pub fn new(seq: EditSeq, transaction: EditTransaction, parent: Option<EditSeq>) -> Self {
        Self {
            seq,
            transaction,
            parent,
            children: Vec::new(),
            last_visited_child: None,
            snapshot: None,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a checkpoint node with snapshot
    pub fn with_snapshot(mut self, snapshot: DocumentSnapshot) -> Self {
        self.snapshot = Some(Box::new(snapshot));
        self
    }
}

// =============================================================================
// Replay Path
// =============================================================================

/// Describes how to reach a specific edit via replay
#[derive(Debug, Clone)]
pub struct ReplayPath {
    pub from_seq: EditSeq,
    pub to_seq: EditSeq,
    /// Operations to undo (apply inverse in order)
    pub undo_ops: Vec<EditTransaction>,
    /// Operations to redo (apply forward in order)
    pub redo_ops: Vec<EditTransaction>,
}

// =============================================================================
// Undo Tree
// =============================================================================

/// Error type for undo tree operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoError {
    NoUndoAvailable,
    NoRedoAvailable,
    InvalidSeq(EditSeq),
    InvalidBranch(usize),
}

impl std::fmt::Display for UndoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UndoError::NoUndoAvailable => write!(f, "No undo available"),
            UndoError::NoRedoAvailable => write!(f, "No redo available"),
            UndoError::InvalidSeq(seq) => write!(f, "Invalid edit sequence: {}", seq),
            UndoError::InvalidBranch(n) => write!(f, "Invalid branch index: {}", n),
        }
    }
}

impl std::error::Error for UndoError {}

/// Undo tree with branching history and checkpoint strategy
pub struct UndoTree {
    pub nodes: HashMap<EditSeq, EditNode>,
    pub current: EditSeq,
    next_seq: EditSeq,
    pub root_seq: EditSeq,

    // Checkpoint configuration
    checkpoint_interval: u64,
    checkpoint_memory_threshold: usize,
    edits_since_checkpoint: u64,
    total_memory: usize,
}

impl UndoTree {
    /// Create a new undo tree
    pub fn new() -> Self {
        Self::with_config(50, 1024 * 1024) // Default: checkpoint every 50 edits or 1MB
    }

    /// Create undo tree with custom configuration
    pub fn with_config(checkpoint_interval: u64, memory_threshold: usize) -> Self {
        // Create root node (represents empty/initial state)
        let root = EditNode::new(0, EditTransaction::new("Initial state"), None);
        let mut nodes = HashMap::new();
        nodes.insert(0, root);

        Self {
            nodes,
            current: 0,
            next_seq: 1,
            root_seq: 0,
            checkpoint_interval,
            checkpoint_memory_threshold: memory_threshold,
            edits_since_checkpoint: 0,
            total_memory: 0,
        }
    }

    /// Get current edit sequence
    pub fn current_seq(&self) -> EditSeq {
        self.current
    }

    /// Move to parent node, returns transaction to undo
    pub fn undo(&mut self) -> Option<&EditTransaction> {
        let current_node = self.nodes.get(&self.current)?;
        let parent_seq = current_node.parent?;

        // Move to parent
        self.current = parent_seq;

        // Return the transaction that was undone (caller applies inverse)
        self.nodes
            .get(&(parent_seq + 1))
            .map(|n| &n.transaction)
            .or_else(|| {
                // Find the child we just came from
                let parent = self.nodes.get(&parent_seq)?;
                for &child_seq in &parent.children {
                    if child_seq == self.current + 1 {
                        return self.nodes.get(&child_seq).map(|n| &n.transaction);
                    }
                }
                None
            })
            .or_else(|| {
                // Fallback: return current node's transaction
                Some(&self.nodes.get(&(self.current + 1))?.transaction)
            })
    }

    /// Get the transaction at current position (for undo)
    pub fn current_transaction(&self) -> Option<&EditTransaction> {
        if self.current == self.root_seq {
            return None;
        }
        self.nodes.get(&self.current).map(|n| &n.transaction)
    }

    /// Move to last-visited child, returns transaction to redo
    pub fn redo(&mut self) -> Option<&EditTransaction> {
        let current_node = self.nodes.get(&self.current)?;

        if current_node.children.is_empty() {
            return None;
        }

        let child_idx = current_node.last_visited_child.unwrap_or(0);
        let child_seq = *current_node.children.get(child_idx)?;

        // Move to child
        self.current = child_seq;

        // Return transaction to apply
        self.nodes.get(&child_seq).map(|n| &n.transaction)
    }

    /// Switch to nth child at current branch point
    pub fn goto_branch(&mut self, n: usize) -> Result<(), UndoError> {
        let current_node = self
            .nodes
            .get_mut(&self.current)
            .ok_or(UndoError::InvalidSeq(self.current))?;

        if n >= current_node.children.len() {
            return Err(UndoError::InvalidBranch(n));
        }

        current_node.last_visited_child = Some(n);
        Ok(())
    }

    /// Compute replay path from one edit to another without mutating state
    pub fn compute_replay_path(&self, from: EditSeq, to: EditSeq) -> Result<ReplayPath, UndoError> {
        // Validate targets exist
        if !self.nodes.contains_key(&from) {
            return Err(UndoError::InvalidSeq(from));
        }
        if !self.nodes.contains_key(&to) {
            return Err(UndoError::InvalidSeq(to));
        }

        // If same, return empty
        if from == to {
            return Ok(ReplayPath {
                from_seq: from,
                to_seq: to,
                undo_ops: Vec::new(),
                redo_ops: Vec::new(),
            });
        }

        // Find ancestors
        let current_ancestors = self.get_ancestors(from);
        let target_ancestors = self.get_ancestors(to);

        // Find common ancestor
        let common_ancestor = self.find_common_ancestor(&current_ancestors, &target_ancestors);

        // Build undo path: from -> common_ancestor
        let mut undo_ops = Vec::new();
        let mut seq = from;
        while seq != common_ancestor {
            if let Some(node) = self.nodes.get(&seq) {
                undo_ops.push(node.transaction.clone());
                if let Some(parent) = node.parent {
                    seq = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Build redo path: common_ancestor -> to
        let mut redo_path = Vec::new();
        seq = to;
        while seq != common_ancestor {
            redo_path.push(seq);
            if let Some(node) = self.nodes.get(&seq) {
                if let Some(parent) = node.parent {
                    seq = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        redo_path.reverse();

        let mut redo_ops = Vec::new();
        for seq in redo_path.iter() {
            if let Some(node) = self.nodes.get(seq) {
                redo_ops.push(node.transaction.clone());
            }
        }

        Ok(ReplayPath {
            from_seq: from,
            to_seq: to,
            undo_ops,
            redo_ops,
        })
    }

    /// Jump to edit #n via checkpoint+replay
    ///
    /// Returns a ReplayPath describing the undo/redo operations needed.
    /// The caller is responsible for applying these operations to the buffer.
    pub fn goto_seq(&mut self, target: EditSeq) -> Result<ReplayPath, UndoError> {
        // Compute path first
        let path = self.compute_replay_path(self.current, target)?;

        // Update internal state (last_visited_child) along the redo path
        let current_ancestors = self.get_ancestors(self.current);
        let target_ancestors = self.get_ancestors(target);
        let common_ancestor = self.find_common_ancestor(&current_ancestors, &target_ancestors);

        let mut seq = target;
        let mut path_to_target = vec![target];
        while seq != common_ancestor {
            if let Some(node) = self.nodes.get(&seq) {
                if let Some(parent) = node.parent {
                    path_to_target.push(parent);
                    seq = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        path_to_target.reverse();

        // Update last_visited_child for each node in the path
        for i in 0..path_to_target.len().saturating_sub(1) {
            let parent_seq = path_to_target[i];
            let child_seq = path_to_target[i + 1];
            if let Some(parent_node) = self.nodes.get_mut(&parent_seq) {
                if let Some(child_idx) = parent_node.children.iter().position(|&c| c == child_seq) {
                    parent_node.last_visited_child = Some(child_idx);
                }
            }
        }

        // Move to target
        self.current = target;

        Ok(path)
    }

    /// Get all ancestors of a node (including the node itself)
    fn get_ancestors(&self, seq: EditSeq) -> Vec<EditSeq> {
        let mut ancestors = Vec::new();
        let mut current = seq;
        while let Some(node) = self.nodes.get(&current) {
            ancestors.push(current);
            if let Some(parent) = node.parent {
                current = parent;
            } else {
                break;
            }
        }
        ancestors
    }

    /// Find the lowest common ancestor of two nodes
    fn find_common_ancestor(&self, ancestors_a: &[EditSeq], ancestors_b: &[EditSeq]) -> EditSeq {
        // Convert one to a set for O(1) lookup
        let set_a: std::collections::HashSet<_> = ancestors_a.iter().copied().collect();

        // Find first ancestor of B that's in A's ancestors
        for &seq in ancestors_b {
            if set_a.contains(&seq) {
                return seq;
            }
        }

        // Fallback to root (should always be reachable)
        self.root_seq
    }

    /// Record a new edit (creates new node as child of current)
    pub fn push(
        &mut self,
        transaction: EditTransaction,
        snapshot: Option<DocumentSnapshot>,
    ) -> EditSeq {
        let seq = self.next_seq;
        self.next_seq += 1;

        // Track memory
        let tx_size = transaction.estimated_size();
        self.total_memory += tx_size;
        self.edits_since_checkpoint += 1;

        // Create node
        let mut node = EditNode::new(seq, transaction, Some(self.current));

        // Determine if this should be a checkpoint
        let should_checkpoint = snapshot.is_some()
            || self.edits_since_checkpoint >= self.checkpoint_interval
            || self.total_memory >= self.checkpoint_memory_threshold;

        if should_checkpoint {
            if let Some(snap) = snapshot {
                node.snapshot = Some(Box::new(snap));
            }
            self.edits_since_checkpoint = 0;
        }

        // Update parent's children and last_visited_child
        if let Some(parent) = self.nodes.get_mut(&self.current) {
            let child_idx = parent.children.len();
            parent.children.push(seq);
            parent.last_visited_child = Some(child_idx);
        }

        self.nodes.insert(seq, node);
        self.current = seq;

        seq
    }

    /// Force checkpoint at current position
    pub fn checkpoint(&mut self, snapshot: DocumentSnapshot) {
        if let Some(node) = self.nodes.get_mut(&self.current) {
            node.snapshot = Some(Box::new(snapshot));
            self.edits_since_checkpoint = 0;
        }
    }

    /// Get number of children at current node (for branch info)
    pub fn branch_count(&self) -> usize {
        self.nodes
            .get(&self.current)
            .map(|n| n.children.len())
            .unwrap_or(0)
    }

    /// Check if we can undo
    pub fn can_undo(&self) -> bool {
        self.current != self.root_seq
    }

    /// Check if we can redo
    pub fn can_redo(&self) -> bool {
        self.nodes
            .get(&self.current)
            .map(|n| !n.children.is_empty())
            .unwrap_or(false)
    }

    /// Get current memory usage
    pub fn memory_usage(&self) -> usize {
        self.total_memory
    }

    /// Clear all history (keep only root)
    pub fn clear(&mut self) {
        self.nodes.retain(|&seq, _| seq == self.root_seq);
        self.current = self.root_seq;
        self.next_seq = 1;
        self.total_memory = 0;
        self.edits_since_checkpoint = 0;

        if let Some(root) = self.nodes.get_mut(&self.root_seq) {
            root.children.clear();
            root.last_visited_child = None;
        }
    }
}

impl Default for UndoTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
