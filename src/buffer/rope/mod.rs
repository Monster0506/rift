//! Piece Table implementation backed by a Rope (AVL Tree)
//!
//! This module provides a `PieceTable` that manages text using a piece table data structure.
//! The pieces are stored in a balanced binary tree (AVL) to ensure O(log N) performance
//! for insertions, deletions, and lookups.
//!
//! It supports:
//! - Efficient insertion and deletion
//! - Line counting and indexing via tree metadata
//! - Immutable buffer backing (Original) and append-only buffer (Add)

use std::cmp::max;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferSource {
    Original,
    Add,
}

#[derive(Clone, Debug)]
pub struct Piece {
    pub source: BufferSource,
    pub start: usize,
    pub len: usize,
}

#[derive(Clone, Debug)]
struct Node {
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
    piece: Piece,

    // Metadata
    len: usize,      // Length in bytes of this subtree
    newlines: usize, // Number of newlines in this subtree
    piece_newlines: usize,
    height: usize, // Height for AVL balancing
}

/// A Piece Table backed by an AVL Tree (Rope).
pub struct PieceTable {
    original: Arc<Vec<u8>>,
    add: Vec<u8>,
    root: Option<Box<Node>>,
}

impl PieceTable {
    pub fn new(original: Vec<u8>) -> Self {
        let len = original.len();
        let newlines = count_newlines(&original);
        let piece = Piece {
            source: BufferSource::Original,
            start: 0,
            len,
        };

        let root = if len > 0 {
            Some(Box::new(Node {
                left: None,
                right: None,
                piece,
                len,
                newlines,
                piece_newlines: newlines,
                height: 1,
            }))
        } else {
            None
        };

        Self {
            original: Arc::new(original),
            add: Vec::new(),
            root,
        }
    }

    pub fn len(&self) -> usize {
        self.root.as_ref().map_or(0, |n| n.len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&mut self, pos: usize, text: &[u8]) {
        if text.is_empty() {
            return;
        }

        let add_start = self.add.len();
        self.add.extend_from_slice(text);
        let add_len = text.len();

        let new_piece = Piece {
            source: BufferSource::Add,
            start: add_start,
            len: add_len,
        };

        let newlines = count_newlines(text);
        let new_node = Box::new(Node {
            left: None,
            right: None,
            piece: new_piece,
            len: add_len,
            newlines,
            piece_newlines: newlines,
            height: 1,
        });

        // Insert logic: Split tree at pos, insert new node, merge.
        // For simplicity in this prototype, we'll use a naive approach or a simple split/concat if we had it.
        // Since implementing full AVL split/merge is complex, we will implement a basic insert into the tree.

        self.root = insert_node(self.root.take(), pos, new_node, &self.original, &self.add);
    }

    pub fn delete(&mut self, range: std::ops::Range<usize>) {
        let start = range.start;
        let end = range.end;
        if start >= end {
            return;
        }

        // Delete logic: Split at start, split the right part at (end - start), drop the middle.
        let (left, right_part) = split(self.root.take(), start, &self.original, &self.add);
        let (_, right) = split(right_part, end - start, &self.original, &self.add);

        self.root = merge(left, right);
    }

    pub fn get_line_count(&self) -> usize {
        self.root.as_ref().map_or(0, |n| n.newlines) + 1
    }

    pub fn byte_at(&self, pos: usize) -> u8 {
        if pos >= self.len() {
            panic!("Index out of bounds");
        }
        get_byte_recursive(self.root.as_deref(), pos, &self.original, &self.add)
    }

    pub fn bytes_range(&self, range: std::ops::Range<usize>) -> Vec<u8> {
        let mut result = Vec::with_capacity(range.len());
        collect_bytes_range(
            self.root.as_deref(),
            range,
            &self.original,
            &self.add,
            &mut result,
        );
        result
    }

    pub fn chunks_in_range(&self, range: std::ops::Range<usize>) -> impl Iterator<Item = &[u8]> {
        let mut chunks = Vec::new();
        collect_chunks_range(
            self.root.as_deref(),
            range,
            &self.original,
            &self.add,
            &mut chunks,
        );
        chunks.into_iter()
    }

    pub fn line_start_offset(&self, line_idx: usize) -> usize {
        if line_idx == 0 {
            return 0;
        }
        if line_idx >= self.get_line_count() {
            return self.len();
        }
        // We want the position after the (line_idx)th newline (1-based count of newlines essentially)
        // If line_idx is 1, we want position after 1st newline.
        find_nth_newline_end(self.root.as_deref(), line_idx, &self.original, &self.add)
    }

    pub fn line_at_byte(&self, pos: usize) -> usize {
        if pos >= self.len() {
            return self.get_line_count().saturating_sub(1);
        }
        get_line_at_pos(self.root.as_deref(), pos, &self.original, &self.add)
    }
}

impl std::fmt::Display for PieceTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut bytes = Vec::with_capacity(self.len());
        collect_bytes(self.root.as_deref(), &self.original, &self.add, &mut bytes);
        write!(f, "{}", String::from_utf8_lossy(&bytes))
    }
}

// --- Tree Operations ---

fn height(node: &Option<Box<Node>>) -> usize {
    node.as_ref().map_or(0, |n| n.height)
}

fn update(node: &mut Box<Node>) {
    let left_len = node.left.as_ref().map_or(0, |n| n.len);
    let right_len = node.right.as_ref().map_or(0, |n| n.len);
    node.len = left_len + node.piece.len + right_len;

    let left_nl = node.left.as_ref().map_or(0, |n| n.newlines);
    let right_nl = node.right.as_ref().map_or(0, |n| n.newlines);
    node.newlines = left_nl + node.piece_newlines + right_nl;

    node.height = 1 + max(height(&node.left), height(&node.right));
}

fn balance(mut node: Box<Node>) -> Box<Node> {
    update(&mut node);
    let balance_factor = height(&node.left) as isize - height(&node.right) as isize;

    if balance_factor > 1 {
        if height(&node.left.as_ref().unwrap().left) >= height(&node.left.as_ref().unwrap().right) {
            return rotate_right(node);
        } else {
            return rotate_left_right(node);
        }
    } else if balance_factor < -1 {
        if height(&node.right.as_ref().unwrap().right) >= height(&node.right.as_ref().unwrap().left)
        {
            return rotate_left(node);
        } else {
            return rotate_right_left(node);
        }
    }
    node
}

// Simplified rotation placeholders - full AVL implementation requires careful metadata management
fn rotate_right(mut node: Box<Node>) -> Box<Node> {
    let mut new_root = node.left.take().unwrap();
    node.left = new_root.right.take();
    update(&mut node);
    new_root.right = Some(node);
    update(&mut new_root);
    new_root
}

fn rotate_left(mut node: Box<Node>) -> Box<Node> {
    let mut new_root = node.right.take().unwrap();
    node.right = new_root.left.take();
    update(&mut node);
    new_root.left = Some(node);
    update(&mut new_root);
    new_root
}

fn rotate_left_right(mut node: Box<Node>) -> Box<Node> {
    let left = node.left.take().unwrap();
    node.left = Some(rotate_left(left));
    rotate_right(node)
}

fn rotate_right_left(mut node: Box<Node>) -> Box<Node> {
    let right = node.right.take().unwrap();
    node.right = Some(rotate_right(right));
    rotate_left(node)
}

// --- Split and Merge ---

fn split(
    root: Option<Box<Node>>,
    pos: usize,
    original: &[u8],
    add: &[u8],
) -> (Option<Box<Node>>, Option<Box<Node>>) {
    match root {
        None => (None, None),
        Some(mut node) => {
            let left_len = node.left.as_ref().map_or(0, |n| n.len);

            if pos < left_len {
                // Split in left child
                let (l, r) = split(node.left.take(), pos, original, add);
                node.left = r;
                update_node_metadata(&mut node, original, add);
                (l, Some(node))
            } else if pos > left_len + node.piece.len {
                // Split in right child
                let (l, r) = split(
                    node.right.take(),
                    pos - (left_len + node.piece.len),
                    original,
                    add,
                );
                node.right = l;
                update_node_metadata(&mut node, original, add);
                (Some(node), r)
            } else {
                // Split in this node's piece
                let offset = pos - left_len;
                let right_child = node.right.take();
                let left_child = node.left.take();

                if offset == 0 {
                    node.left = None;
                    node.right = right_child;
                    update_node_metadata(&mut node, original, add);
                    (left_child, Some(node))
                } else if offset == node.piece.len {
                    node.left = left_child;
                    node.right = None;
                    update_node_metadata(&mut node, original, add);
                    (Some(node), right_child)
                } else {
                    // Actual split of the piece
                    let p1 = Piece {
                        source: node.piece.source,
                        start: node.piece.start,
                        len: offset,
                    };
                    let p2 = Piece {
                        source: node.piece.source,
                        start: node.piece.start + offset,
                        len: node.piece.len - offset,
                    };

                    let n1 = Box::new(Node {
                        left: left_child,
                        right: None,
                        piece: p1,
                        len: 0,
                        newlines: 0,
                        piece_newlines: 0, // will update
                        height: 1,         // will update
                    });
                    let mut n1 = n1;
                    update_node_metadata(&mut n1, original, add);

                    let n2 = Box::new(Node {
                        left: None,
                        right: right_child,
                        piece: p2,
                        len: 0,
                        newlines: 0,
                        piece_newlines: 0, // will update
                        height: 1,         // will update
                    });
                    let mut n2 = n2;
                    update_node_metadata(&mut n2, original, add);

                    (Some(n1), Some(n2))
                }
            }
        }
    }
}

fn merge(left: Option<Box<Node>>, right: Option<Box<Node>>) -> Option<Box<Node>> {
    match (left, right) {
        (None, r) => r,
        (l, None) => l,
        (Some(l), Some(r)) => {
            let (new_left, center) = delete_max(l);
            Some(join_with_root(new_left, center, Some(r)))
        }
    }
}

fn delete_max(mut node: Box<Node>) -> (Option<Box<Node>>, Box<Node>) {
    if let Some(right) = node.right.take() {
        let (new_right, max) = delete_max(right);
        node.right = new_right;
        (Some(balance(node)), max)
    } else {
        (node.left.take(), node)
    }
}

fn join_with_root(
    left: Option<Box<Node>>,
    mut center: Box<Node>,
    right: Option<Box<Node>>,
) -> Box<Node> {
    let lh = height(&left);
    let rh = height(&right);

    if (lh as isize - rh as isize).abs() <= 1 {
        center.left = left;
        center.right = right;
        update(&mut center);
        center
    } else if lh > rh {
        let mut left_node = left.unwrap();
        let new_right = join_with_root(left_node.right.take(), center, right);
        left_node.right = Some(new_right);
        balance(left_node)
    } else {
        let mut right_node = right.unwrap();
        let new_left = join_with_root(left, center, right_node.left.take());
        right_node.left = Some(new_left);
        balance(right_node)
    }
}

fn insert_node(
    root: Option<Box<Node>>,
    pos: usize,
    new_node: Box<Node>,
    original: &[u8],
    add: &[u8],
) -> Option<Box<Node>> {
    let (left, right) = split(root, pos, original, add);
    Some(join_with_root(left, new_node, right))
}

// --- Helpers ---

fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == b'\n').count()
}

fn get_piece_newlines(piece: &Piece, original: &[u8], add: &[u8]) -> usize {
    let slice = match piece.source {
        BufferSource::Original => &original[piece.start..piece.start + piece.len],
        BufferSource::Add => &add[piece.start..piece.start + piece.len],
    };
    count_newlines(slice)
}

fn update_node_metadata(node: &mut Box<Node>, original: &[u8], add: &[u8]) {
    let left_len = node.left.as_ref().map_or(0, |n| n.len);
    let right_len = node.right.as_ref().map_or(0, |n| n.len);
    node.len = left_len + node.piece.len + right_len;

    let left_nl = node.left.as_ref().map_or(0, |n| n.newlines);
    let right_nl = node.right.as_ref().map_or(0, |n| n.newlines);
    let piece_nl = get_piece_newlines(&node.piece, original, add);
    node.piece_newlines = piece_nl;
    node.newlines = left_nl + piece_nl + right_nl;

    node.height = 1 + max(height(&node.left), height(&node.right));
}

fn collect_bytes(node: Option<&Node>, original: &[u8], add: &[u8], out: &mut Vec<u8>) {
    if let Some(n) = node {
        collect_bytes(n.left.as_deref(), original, add, out);
        let slice = match n.piece.source {
            BufferSource::Original => &original[n.piece.start..n.piece.start + n.piece.len],
            BufferSource::Add => &add[n.piece.start..n.piece.start + n.piece.len],
        };
        out.extend_from_slice(slice);
        collect_bytes(n.right.as_deref(), original, add, out);
    }
}

fn get_byte_recursive(node: Option<&Node>, pos: usize, original: &[u8], add: &[u8]) -> u8 {
    let node = node.unwrap();
    let left_len = node.left.as_ref().map_or(0, |n| n.len);

    if pos < left_len {
        get_byte_recursive(node.left.as_deref(), pos, original, add)
    } else if pos < left_len + node.piece.len {
        let offset = pos - left_len;
        let slice = match node.piece.source {
            BufferSource::Original => {
                &original[node.piece.start..node.piece.start + node.piece.len]
            }
            BufferSource::Add => &add[node.piece.start..node.piece.start + node.piece.len],
        };
        slice[offset]
    } else {
        get_byte_recursive(
            node.right.as_deref(),
            pos - left_len - node.piece.len,
            original,
            add,
        )
    }
}

fn collect_bytes_range(
    node: Option<&Node>,
    range: std::ops::Range<usize>,
    original: &[u8],
    add: &[u8],
    out: &mut Vec<u8>,
) {
    if let Some(n) = node {
        let left_len = n.left.as_ref().map_or(0, |n| n.len);
        let piece_len = n.piece.len;
        let current_len = left_len + piece_len;

        // Check intersection with left child
        if range.start < left_len {
            let l_start = range.start;
            let l_end = std::cmp::min(range.end, left_len);
            collect_bytes_range(n.left.as_deref(), l_start..l_end, original, add, out);
        }

        // Check intersection with current piece
        if range.end > left_len && range.start < current_len {
            let p_start = std::cmp::max(range.start, left_len) - left_len;
            let p_end = std::cmp::min(range.end, current_len) - left_len;

            let slice = match n.piece.source {
                BufferSource::Original => &original[n.piece.start..n.piece.start + n.piece.len],
                BufferSource::Add => &add[n.piece.start..n.piece.start + n.piece.len],
            };
            out.extend_from_slice(&slice[p_start..p_end]);
        }

        // Check intersection with right child
        if range.end > current_len {
            let r_start = std::cmp::max(range.start, current_len) - current_len;
            let r_end = range.end - current_len;
            collect_bytes_range(n.right.as_deref(), r_start..r_end, original, add, out);
        }
    }
}

fn collect_chunks_range<'a>(
    node: Option<&'a Node>,
    range: std::ops::Range<usize>,
    original: &'a [u8],
    add: &'a [u8],
    out: &mut Vec<&'a [u8]>,
) {
    if let Some(n) = node {
        let left_len = n.left.as_ref().map_or(0, |n| n.len);
        let piece_len = n.piece.len;
        let current_len = left_len + piece_len;

        // Check intersection with left child
        if range.start < left_len {
            let l_start = range.start;
            let l_end = std::cmp::min(range.end, left_len);
            collect_chunks_range(n.left.as_deref(), l_start..l_end, original, add, out);
        }

        // Check intersection with current piece
        if range.end > left_len && range.start < current_len {
            let p_start = std::cmp::max(range.start, left_len) - left_len;
            let p_end = std::cmp::min(range.end, current_len) - left_len;

            let slice = match n.piece.source {
                BufferSource::Original => &original[n.piece.start..n.piece.start + n.piece.len],
                BufferSource::Add => &add[n.piece.start..n.piece.start + n.piece.len],
            };
            out.push(&slice[p_start..p_end]);
        }

        // Check intersection with right child
        if range.end > current_len {
            let r_start = std::cmp::max(range.start, current_len) - current_len;
            let r_end = range.end - current_len;
            collect_chunks_range(n.right.as_deref(), r_start..r_end, original, add, out);
        }
    }
}

fn find_nth_newline_end(node: Option<&Node>, target: usize, original: &[u8], add: &[u8]) -> usize {
    let node = node.unwrap();
    let left_nl = node.left.as_ref().map_or(0, |n| n.newlines);

    if target <= left_nl {
        find_nth_newline_end(node.left.as_deref(), target, original, add)
    } else {
        let piece_nl = get_piece_newlines(&node.piece, original, add);
        let current_nl = left_nl + piece_nl;

        if target <= current_nl {
            // In this piece
            let needed_in_piece = target - left_nl;
            let slice = match node.piece.source {
                BufferSource::Original => {
                    &original[node.piece.start..node.piece.start + node.piece.len]
                }
                BufferSource::Add => &add[node.piece.start..node.piece.start + node.piece.len],
            };

            let mut count = 0;
            for (i, &b) in slice.iter().enumerate() {
                if b == b'\n' {
                    count += 1;
                    if count == needed_in_piece {
                        let left_len = node.left.as_ref().map_or(0, |n| n.len);
                        return left_len + i + 1;
                    }
                }
            }

            unreachable!("Metadata mismatch: newline not found in piece");
        } else {
            // In right child
            let left_len = node.left.as_ref().map_or(0, |n| n.len);
            left_len
                + node.piece.len
                + find_nth_newline_end(node.right.as_deref(), target - current_nl, original, add)
        }
    }
}

fn get_line_at_pos(node: Option<&Node>, pos: usize, original: &[u8], add: &[u8]) -> usize {
    let node = node.unwrap();
    let left_len = node.left.as_ref().map_or(0, |n| n.len);

    if pos < left_len {
        get_line_at_pos(node.left.as_deref(), pos, original, add)
    } else {
        let left_nl = node.left.as_ref().map_or(0, |n| n.newlines);

        if pos < left_len + node.piece.len {
            // In this piece
            let offset = pos - left_len;
            let slice = match node.piece.source {
                BufferSource::Original => {
                    &original[node.piece.start..node.piece.start + node.piece.len]
                }
                BufferSource::Add => &add[node.piece.start..node.piece.start + node.piece.len],
            };

            let piece_nl_before = count_newlines(&slice[..offset]);
            left_nl + piece_nl_before
        } else {
            // In right child
            let piece_nl = get_piece_newlines(&node.piece, original, add);
            left_nl
                + piece_nl
                + get_line_at_pos(
                    node.right.as_deref(),
                    pos - left_len - node.piece.len,
                    original,
                    add,
                )
        }
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
