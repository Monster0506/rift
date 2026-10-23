pub(crate) mod command;
pub(crate) mod persist;

use crate::character::Character;
use crate::time::SystemTime;
use std::collections::BTreeMap;

pub(crate) type EditSeq = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Position {
    pub(crate) line: u32,
    pub(crate) col: u32,
}

impl Position {
    #[must_use]
    pub const fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub(crate) start: Position,
    pub(crate) end: Position,
}

impl Range {
    #[must_use]
    pub(crate) const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    #[cfg(test)]
    #[must_use]
    fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOperation {
    Insert {
        position: Position,
        text: Vec<Character>,
        len: usize,
    },

    Delete {
        range: Range,
        deleted_text: Vec<Character>,
    },

    Replace {
        range: Range,
        old_text: Vec<Character>,
        new_text: Vec<Character>,
    },

    BlockChange {
        range: Range,
        old_content: Vec<Vec<Character>>,
        new_content: Vec<Vec<Character>>,
    },
}

/// Positions are `u32`; a count above `u32::MAX` saturates instead of wrapping.
fn saturating_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl EditOperation {
    #[must_use]
    fn inverse(&self) -> Self {
        match self {
            Self::Insert { position, text, .. } => Self::Delete {
                range: Range::new(
                    *position,
                    Position::new(
                        position.line
                            + saturating_u32(
                                text.iter().filter(|c| **c == Character::Newline).count(),
                            ),
                        if text.contains(&Character::Newline) {
                            saturating_u32(
                                text.iter()
                                    .rev()
                                    .take_while(|c| **c != Character::Newline)
                                    .count(),
                            )
                        } else {
                            position.col + saturating_u32(text.len())
                        },
                    ),
                ),
                deleted_text: text.clone(),
            },

            Self::Delete {
                range,
                deleted_text,
            } => Self::Insert {
                position: range.start,
                text: deleted_text.clone(),
                len: deleted_text.len(),
            },

            Self::Replace {
                range,
                old_text,
                new_text,
            } => Self::Replace {
                range: Range::new(
                    range.start,
                    Position::new(
                        range.start.line
                            + saturating_u32(
                                new_text
                                    .iter()
                                    .filter(|c| **c == Character::Newline)
                                    .count(),
                            ),
                        if new_text.contains(&Character::Newline) {
                            saturating_u32(
                                new_text
                                    .iter()
                                    .rev()
                                    .take_while(|c| **c != Character::Newline)
                                    .count(),
                            )
                        } else {
                            range.start.col + saturating_u32(new_text.len())
                        },
                    ),
                ),
                old_text: new_text.clone(),
                new_text: old_text.clone(),
            },

            Self::BlockChange {
                range,
                old_content,
                new_content,
            } => Self::BlockChange {
                range: Range::new(
                    range.start,
                    Position::new(
                        range.start.line + saturating_u32(new_content.len().saturating_sub(1)),
                        new_content.last().map_or(0, |s| saturating_u32(s.len())),
                    ),
                ),
                old_content: new_content.clone(),
                new_content: old_content.clone(),
            },
        }
    }

    #[must_use]
    fn estimated_size(&self) -> usize {
        match self {
            Self::Insert { text, .. } => text.len() + 32,
            Self::Delete { deleted_text, .. } => deleted_text.len() + 32,
            Self::Replace {
                old_text, new_text, ..
            } => old_text.len() + new_text.len() + 32,
            Self::BlockChange {
                old_content,
                new_content,
                ..
            } => {
                old_content.iter().map(std::vec::Vec::len).sum::<usize>()
                    + new_content.iter().map(std::vec::Vec::len).sum::<usize>()
                    + 32
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EditTransaction {
    pub(crate) ops: Vec<EditOperation>,
    pub(crate) description: String,
    pub(crate) cursor_before: Option<usize>,
    pub(crate) cursor_after: Option<usize>,
}

impl EditTransaction {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            ops: Vec::new(),
            description: description.into(),
            cursor_before: None,
            cursor_after: None,
        }
    }

    pub fn record(&mut self, operation: EditOperation) {
        self.ops.push(operation);
    }

    #[must_use]
    pub(crate) fn inverse(&self) -> Vec<EditOperation> {
        self.ops.iter().rev().map(EditOperation::inverse).collect()
    }

    #[must_use]
    pub(crate) const fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    #[must_use]
    fn estimated_size(&self) -> usize {
        self.ops
            .iter()
            .map(EditOperation::estimated_size)
            .sum::<usize>()
            + self.description.len()
            + 16
    }
}

#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    pub(crate) full_text: Vec<Character>,
    pub(crate) annotations: Vec<crate::annotations::Annotation>,
}

impl DocumentSnapshot {
    #[must_use]
    pub(crate) fn new(
        text: Vec<Character>,
        annotations: Vec<crate::annotations::Annotation>,
    ) -> Self {
        Self {
            full_text: text,
            annotations,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EditNode {
    pub(crate) seq: EditSeq,
    pub(crate) transaction: EditTransaction,
    pub(crate) parent: Option<EditSeq>,
    pub(crate) children: Vec<EditSeq>,
    pub(crate) last_visited_child: Option<usize>,
    pub(crate) snapshot: Option<Box<DocumentSnapshot>>,
    pub(crate) timestamp: SystemTime,
}

impl EditNode {
    #[must_use]
    pub(crate) fn new(seq: EditSeq, transaction: EditTransaction, parent: Option<EditSeq>) -> Self {
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
}

#[derive(Debug, Clone)]
pub struct ReplayPath {
    pub(crate) undo_ops: Vec<EditTransaction>,
    pub(crate) redo_ops: Vec<EditTransaction>,
    pub(crate) snapshot_restore: Option<DocumentSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoError {
    NoUndoAvailable,
    NoRedoAvailable,
    InvalidSeq(EditSeq),
}

impl std::fmt::Display for UndoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoUndoAvailable => write!(f, "No undo available"),
            Self::NoRedoAvailable => write!(f, "No redo available"),
            Self::InvalidSeq(seq) => write!(f, "Invalid edit sequence: {seq}"),
        }
    }
}

impl std::error::Error for UndoError {}

#[derive(Clone, Debug)]
pub struct UndoTree {
    pub(crate) nodes: BTreeMap<EditSeq, EditNode>,
    pub(crate) current: EditSeq,
    next_seq: EditSeq,
    pub(crate) root_seq: EditSeq,
    pub(crate) saved_seq: EditSeq,

    checkpoint_interval: u64,
    checkpoint_memory_threshold: usize,
    edits_since_checkpoint: u64,
    total_memory: usize,
}

impl UndoTree {
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(50, 1024 * 1024) // Default: checkpoint every 50 edits or 1MB
    }

    #[must_use]
    fn with_config(checkpoint_interval: u64, memory_threshold: usize) -> Self {
        let root = EditNode::new(0, EditTransaction::new("Initial state"), None);
        let mut nodes = BTreeMap::new();
        nodes.insert(0, root);

        Self {
            nodes,
            current: 0,
            next_seq: 1,
            root_seq: 0,
            saved_seq: 0,
            checkpoint_interval,
            checkpoint_memory_threshold: memory_threshold,
            edits_since_checkpoint: 0,
            total_memory: 0,
        }
    }

    fn from_parts(nodes: BTreeMap<EditSeq, EditNode>, current: EditSeq) -> Self {
        let total_memory = nodes.values().map(|n| n.transaction.estimated_size()).sum();
        let next_seq = nodes.keys().copied().max().map_or(1, |max| max + 1);
        let mut tree = Self::new();
        tree.nodes = nodes;
        tree.current = current;
        tree.next_seq = next_seq;
        tree.saved_seq = current;
        tree.total_memory = total_memory;
        tree
    }

    #[must_use]
    pub const fn current_seq(&self) -> EditSeq {
        self.current
    }

    pub fn undo(&mut self) -> Option<&EditTransaction> {
        let current_node = self.nodes.get(&self.current)?;
        let parent_seq = current_node.parent?;

        self.current = parent_seq;

        self.nodes
            .get(&(parent_seq + 1))
            .map(|n| &n.transaction)
            .or_else(|| {
                let parent = self.nodes.get(&parent_seq)?;
                for &child_seq in &parent.children {
                    if child_seq == self.current + 1 {
                        return self.nodes.get(&child_seq).map(|n| &n.transaction);
                    }
                }
                None
            })
            .or_else(|| Some(&self.nodes.get(&(self.current + 1))?.transaction))
    }

    #[must_use]
    pub(crate) fn current_transaction(&self) -> Option<&EditTransaction> {
        if self.current == self.root_seq {
            return None;
        }
        self.nodes.get(&self.current).map(|n| &n.transaction)
    }

    pub fn redo(&mut self) -> Option<&EditTransaction> {
        let current_node = self.nodes.get(&self.current)?;

        if current_node.children.is_empty() {
            return None;
        }

        let child_idx = current_node.last_visited_child.unwrap_or(0);
        let child_seq = *current_node.children.get(child_idx)?;

        self.current = child_seq;

        self.nodes.get(&child_seq).map(|n| &n.transaction)
    }

    fn nearest_checkpoint(&self, ancestors: &[EditSeq]) -> Option<(usize, &DocumentSnapshot)> {
        ancestors.iter().enumerate().find_map(|(depth, seq)| {
            self.nodes
                .get(seq)
                .and_then(|n| n.snapshot.as_deref())
                .map(|snap| (depth, snap))
        })
    }

    /// Computes the undo and redo operations that move the tree from `from` to `to`.
    ///
    /// # Errors
    /// Returns `Err` when `from` or `to` is not a node in the tree.
    pub fn compute_replay_path(&self, from: EditSeq, to: EditSeq) -> Result<ReplayPath, UndoError> {
        if !self.nodes.contains_key(&from) {
            return Err(UndoError::InvalidSeq(from));
        }
        if !self.nodes.contains_key(&to) {
            return Err(UndoError::InvalidSeq(to));
        }

        if from == to {
            return Ok(ReplayPath {
                undo_ops: Vec::new(),
                redo_ops: Vec::new(),
                snapshot_restore: None,
            });
        }

        let current_ancestors = self.get_ancestors(from);
        let target_ancestors = self.get_ancestors(to);

        let common_ancestor = self.find_common_ancestor(&current_ancestors, &target_ancestors);

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
        for seq in &redo_path {
            if let Some(node) = self.nodes.get(seq) {
                redo_ops.push(node.transaction.clone());
            }
        }

        if let Some((depth, snapshot)) = self.nearest_checkpoint(&target_ancestors) {
            if depth < undo_ops.len() + redo_ops.len() {
                let mut snap_redo_ops = Vec::new();
                for seq in target_ancestors[..depth].iter().rev() {
                    if let Some(node) = self.nodes.get(seq) {
                        snap_redo_ops.push(node.transaction.clone());
                    }
                }
                return Ok(ReplayPath {
                    undo_ops: Vec::new(),
                    redo_ops: snap_redo_ops,
                    snapshot_restore: Some(snapshot.clone()),
                });
            }
        }

        Ok(ReplayPath {
            undo_ops,
            redo_ops,
            snapshot_restore: None,
        })
    }

    /// Moves the tree's current position to `target`, returning the replay path taken.
    ///
    /// # Errors
    /// Returns `Err` when `target` is not a node in the tree.
    pub(crate) fn goto_seq(&mut self, target: EditSeq) -> Result<ReplayPath, UndoError> {
        let path = self.compute_replay_path(self.current, target)?;

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

        for i in 0..path_to_target.len().saturating_sub(1) {
            let parent_seq = path_to_target[i];
            let child_seq = path_to_target[i + 1];
            if let Some(parent_node) = self.nodes.get_mut(&parent_seq) {
                if let Some(child_idx) = parent_node.children.iter().position(|&c| c == child_seq) {
                    parent_node.last_visited_child = Some(child_idx);
                }
            }
        }

        self.current = target;

        debug_assert_eq!(
            self.current_seq(),
            target,
            "goto_seq: ended at {} instead of target {}",
            self.current_seq(),
            target
        );

        Ok(path)
    }

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

    fn find_common_ancestor(&self, ancestors_a: &[EditSeq], ancestors_b: &[EditSeq]) -> EditSeq {
        let set_a: std::collections::HashSet<_> = ancestors_a.iter().copied().collect();

        for &seq in ancestors_b {
            if set_a.contains(&seq) {
                return seq;
            }
        }

        self.root_seq
    }

    pub fn push(
        &mut self,
        transaction: EditTransaction,
        snapshot: Option<DocumentSnapshot>,
    ) -> EditSeq {
        let seq = self.next_seq;
        self.next_seq += 1;

        let tx_size = transaction.estimated_size();
        self.total_memory += tx_size;
        self.edits_since_checkpoint += 1;

        let mut node = EditNode::new(seq, transaction, Some(self.current));

        let should_checkpoint = snapshot.is_some()
            || (self.total_memory < self.checkpoint_memory_threshold
                && self.edits_since_checkpoint >= self.checkpoint_interval);

        if should_checkpoint {
            if let Some(snap) = snapshot {
                node.snapshot = Some(Box::new(snap));
            }
            self.edits_since_checkpoint = 0;
        }

        if let Some(parent) = self.nodes.get_mut(&self.current) {
            let child_idx = parent.children.len();
            parent.children.push(seq);
            parent.last_visited_child = Some(child_idx);
        }

        self.nodes.insert(seq, node);
        self.current = seq;

        seq
    }

    pub(crate) fn checkpoint(&mut self, snapshot: DocumentSnapshot) {
        if let Some(node) = self.nodes.get_mut(&self.current) {
            node.snapshot = Some(Box::new(snapshot));
            self.edits_since_checkpoint = 0;
        }
    }

    #[cfg(test)]
    #[must_use]
    fn branch_count(&self) -> usize {
        self.nodes
            .get(&self.current)
            .map_or(0, |n| n.children.len())
    }

    #[must_use]
    pub(crate) const fn can_undo(&self) -> bool {
        self.current != self.root_seq
    }

    #[must_use]
    pub(crate) fn can_redo(&self) -> bool {
        self.nodes
            .get(&self.current)
            .is_some_and(|n| !n.children.is_empty())
    }

    #[cfg(test)]
    fn clear(&mut self) {
        self.nodes.retain(|&seq, _| seq == self.root_seq);
        self.current = self.root_seq;
        self.next_seq = 1;
        self.saved_seq = self.root_seq;
        self.total_memory = 0;
        self.edits_since_checkpoint = 0;

        if let Some(root) = self.nodes.get_mut(&self.root_seq) {
            root.children.clear();
            root.last_visited_child = None;
        }
    }

    pub(crate) const fn mark_saved(&mut self) {
        self.saved_seq = self.current;
    }

    pub(crate) const fn mark_saved_at(&mut self, seq: EditSeq) {
        self.saved_seq = seq;
    }

    pub(crate) fn check_invariants(&self, tier: crate::invariants::InvariantTier) -> Vec<String> {
        use crate::invariants::InvariantTier;

        let mut out = Vec::new();

        if !self.nodes.contains_key(&self.current) {
            out.push(format!("current={} not in nodes", self.current));
        }
        if !self.nodes.contains_key(&self.saved_seq) {
            out.push(format!("saved_seq={} not in nodes", self.saved_seq));
        }
        match self.nodes.get(&self.root_seq) {
            None => out.push(format!("root_seq={} not in nodes", self.root_seq)),
            Some(root) if root.parent.is_some() => out.push(format!(
                "root node {} has a parent ({:?}); roots must be parentless",
                self.root_seq, root.parent
            )),
            _ => {}
        }
        let max_seq = self.nodes.keys().copied().max().unwrap_or(self.root_seq);
        if self.next_seq <= max_seq {
            out.push(format!(
                "next_seq {} <= max existing node seq {max_seq}",
                self.next_seq
            ));
        }

        if tier >= InvariantTier::Deep {
            for (&seq, node) in &self.nodes {
                if let Some(parent_seq) = node.parent {
                    match self.nodes.get(&parent_seq) {
                        None => {
                            out.push(format!("node {seq}'s parent {parent_seq} does not exist"));
                        }
                        Some(parent) if !parent.children.contains(&seq) => out.push(format!(
                            "node {seq}'s parent {parent_seq} does not list it as a child"
                        )),
                        _ => {}
                    }
                }
                for &child_seq in &node.children {
                    match self.nodes.get(&child_seq) {
                        None => out.push(format!("node {seq}'s child {child_seq} does not exist")),
                        Some(child) if child.parent != Some(seq) => out.push(format!(
                            "node {seq}'s child {child_seq} has parent {:?} instead",
                            child.parent
                        )),
                        _ => {}
                    }
                }
            }
        }

        out
    }

    #[must_use]
    pub(crate) const fn is_at_saved(&self) -> bool {
        self.current == self.saved_seq
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
