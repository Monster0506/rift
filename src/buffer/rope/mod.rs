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

use crate::character::Character;
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
    pub len: usize, // Length in Characters
}

#[derive(Clone, Debug)]
struct Node {
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
    piece: Piece,

    // Metadata
    len: usize,      // Length in Characters of this subtree
    byte_len: usize, // Length in bytes (UTF-8) of this subtree
    newlines: usize, // Number of newlines in this subtree
    piece_newlines: usize,
    piece_byte_len: usize, // Cached byte length of the piece
    height: usize,         // Height for AVL balancing
}

/// A Piece Table backed by an AVL Tree (Rope).
#[derive(Clone)]
pub struct PieceTable {
    original: Arc<Vec<Character>>,
    add: Vec<Character>,
    root: Option<Box<Node>>,
}

impl PieceTable {
    pub fn new(original: Vec<Character>) -> Self {
        let len = original.len();
        let (newlines, byte_len) = count_stats(&original);
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
                byte_len,
                newlines,
                piece_newlines: newlines,
                piece_byte_len: byte_len,
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

    pub fn byte_len(&self) -> usize {
        self.root.as_ref().map_or(0, |n| n.byte_len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&mut self, pos: usize, text: &[Character]) {
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

        let (newlines, byte_len) = count_stats(text);
        let new_node = Box::new(Node {
            left: None,
            right: None,
            piece: new_piece,
            len: add_len,
            byte_len,
            newlines,
            piece_newlines: newlines,
            piece_byte_len: byte_len,
            height: 1,
        });

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

    pub fn char_at(&self, pos: usize) -> Character {
        if pos >= self.len() {
            panic!("Index out of bounds");
        }
        get_char_recursive(self.root.as_deref(), pos, &self.original, &self.add)
    }

    /// Get the Character at the given index.
    pub fn get(&self, index: usize) -> Option<Character> {
        if index >= self.len() {
            None
        } else {
            Some(self.char_at(index))
        }
    }

    pub fn bytes_range(&self, range: std::ops::Range<usize>) -> Vec<u8> {
        let mut result = Vec::new();
        collect_bytes_range(
            self.root.as_deref(),
            range,
            &self.original,
            &self.add,
            &mut result,
        );
        result
    }

    /// Collect Characters in a range
    pub fn chars_on_line(&self, line_idx: usize) -> Vec<Character> {
        let start = self.line_start_offset(line_idx);
        let end = if line_idx + 1 >= self.get_line_count() {
            self.len()
        } else {
            self.line_start_offset(line_idx + 1)
        };

        let mut result = Vec::with_capacity(end - start);
        collect_chars_range(
            self.root.as_deref(),
            start..end,
            &self.original,
            &self.add,
            &mut result,
        );
        result
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

    pub fn line_at_char(&self, pos: usize) -> usize {
        if pos >= self.len() {
            return self.get_line_count().saturating_sub(1);
        }
        get_line_at_pos(self.root.as_deref(), pos, &self.original, &self.add)
    }

    /// Convert character index to byte offset
    pub fn char_to_byte(&self, char_index: usize) -> usize {
        if char_index >= self.len() {
            return self.byte_len();
        }
        get_byte_offset_recursive(self.root.as_deref(), char_index, &self.original, &self.add)
    }

    /// Convert byte offset to character index
    /// Returns the index of the character containing the byte, or the char starting at that byte.
    pub fn byte_to_char(&self, byte_offset: usize) -> usize {
        if byte_offset >= self.byte_len() {
            return self.len();
        }
        get_char_idx_recursive(self.root.as_deref(), byte_offset, &self.original, &self.add)
    }

    /// Get an O(N) iterator over the characters
    pub fn iter(&self) -> PieceTableIterator {
        PieceTableIterator::new(self.root.as_deref(), &self.original, &self.add)
    }

    /// Collect all logical bytes (UTF-8) from the buffer
    pub fn to_logical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.byte_len());
        for c in self.iter() {
            c.encode_utf8(&mut buf);
        }
        buf
    }

    /// Get an iterator starting at a specific character index
    pub fn iter_at(&self, start_pos: usize) -> PieceTableIterator {
        let mut iter = self.iter();
        if start_pos > 0 {
            // Optimization TODO: Implement proper seek in iterator
            for _ in 0..start_pos {
                iter.next();
            }
        }
        iter
    }
}

/// efficient O(N) iterator for PieceTable
pub struct PieceTableIterator<'a> {
    stack: Vec<&'a Node>,
    current_piece: Option<&'a [Character]>,
    current_piece_idx: usize,
    original: &'a [Character],
    add: &'a [Character],
}

impl<'a> PieceTableIterator<'a> {
    fn new(root: Option<&'a Node>, original: &'a [Character], add: &'a [Character]) -> Self {
        let mut iter = Self {
            stack: Vec::new(),
            current_piece: None,
            current_piece_idx: 0,
            original,
            add,
        };
        if let Some(node) = root {
            iter.push_left(node);
        }
        iter
    }

    fn push_left(&mut self, mut node: &'a Node) {
        self.stack.push(node);
        while let Some(left) = &node.left {
            self.stack.push(left);
            node = left;
        }
    }
}

impl<'a> Iterator for PieceTableIterator<'a> {
    type Item = Character;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(slice) = self.current_piece {
            if self.current_piece_idx < slice.len() {
                let c = slice[self.current_piece_idx];
                self.current_piece_idx += 1;
                return Some(c);
            } else {
                self.current_piece = None;
            }
        }

        if let Some(node) = self.stack.pop() {
            let slice = get_piece_slice(&node.piece, self.original, self.add);
            if !slice.is_empty() {
                self.current_piece = Some(slice);
                self.current_piece_idx = 1;
                if let Some(right) = &node.right {
                    self.push_left(right);
                }
                return Some(slice[0]);
            } else {
                if let Some(right) = &node.right {
                    self.push_left(right);
                }
                return self.next();
            }
        }
        None
    }
}

impl std::fmt::Display for PieceTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // This is inefficient but functional for Debug/Display
        let mut chars = Vec::with_capacity(self.len());
        collect_chars(self.root.as_deref(), &self.original, &self.add, &mut chars);
        for ch in chars {
            ch.render(f)?;
        }
        Ok(())
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

    let left_byte_len = node.left.as_ref().map_or(0, |n| n.byte_len);
    let right_byte_len = node.right.as_ref().map_or(0, |n| n.byte_len);
    node.byte_len = left_byte_len + node.piece_byte_len + right_byte_len;

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
    original: &[Character],
    add: &[Character],
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
                        byte_len: 0,
                        newlines: 0,
                        piece_newlines: 0, // will update
                        piece_byte_len: 0,
                        height: 1, // will update
                    });
                    let mut n1 = n1;
                    update_node_metadata(&mut n1, original, add);

                    let n2 = Box::new(Node {
                        left: None,
                        right: right_child,
                        piece: p2,
                        len: 0,
                        byte_len: 0,
                        newlines: 0,
                        piece_newlines: 0,
                        piece_byte_len: 0,
                        height: 1, // will update
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
    original: &[Character],
    add: &[Character],
) -> Option<Box<Node>> {
    let (left, right) = split(root, pos, original, add);
    Some(join_with_root(left, new_node, right))
}

// --- Helpers ---

fn count_stats(chars: &[Character]) -> (usize, usize) {
    let mut newlines = 0;
    let mut byte_len = 0;
    for c in chars {
        if *c == Character::Newline {
            newlines += 1;
        }
        byte_len += c.len_utf8();
    }
    (newlines, byte_len)
}

fn get_piece_slice<'a>(
    piece: &Piece,
    original: &'a [Character],
    add: &'a [Character],
) -> &'a [Character] {
    match piece.source {
        BufferSource::Original => &original[piece.start..piece.start + piece.len],
        BufferSource::Add => &add[piece.start..piece.start + piece.len],
    }
}

fn update_node_metadata(node: &mut Box<Node>, original: &[Character], add: &[Character]) {
    let left_len = node.left.as_ref().map_or(0, |n| n.len);
    let right_len = node.right.as_ref().map_or(0, |n| n.len);
    node.len = left_len + node.piece.len + right_len;

    let left_byte_len = node.left.as_ref().map_or(0, |n| n.byte_len);
    let right_byte_len = node.right.as_ref().map_or(0, |n| n.byte_len);

    let slice = get_piece_slice(&node.piece, original, add);
    let (piece_nl, piece_bytes) = count_stats(slice);

    node.piece_newlines = piece_nl;
    node.piece_byte_len = piece_bytes;

    node.byte_len = left_byte_len + piece_bytes + right_byte_len;

    let left_nl = node.left.as_ref().map_or(0, |n| n.newlines);
    let right_nl = node.right.as_ref().map_or(0, |n| n.newlines);
    node.newlines = left_nl + piece_nl + right_nl;

    node.height = 1 + max(height(&node.left), height(&node.right));
}

fn collect_chars(
    node: Option<&Node>,
    original: &[Character],
    add: &[Character],
    out: &mut Vec<Character>,
) {
    if let Some(n) = node {
        collect_chars(n.left.as_deref(), original, add, out);
        out.extend_from_slice(get_piece_slice(&n.piece, original, add));
        collect_chars(n.right.as_deref(), original, add, out);
    }
}

fn get_char_recursive(
    node: Option<&Node>,
    pos: usize,
    original: &[Character],
    add: &[Character],
) -> Character {
    let node = node.unwrap();
    let left_len = node.left.as_ref().map_or(0, |n| n.len);

    if pos < left_len {
        get_char_recursive(node.left.as_deref(), pos, original, add)
    } else if pos < left_len + node.piece.len {
        let offset = pos - left_len;
        let slice = get_piece_slice(&node.piece, original, add);
        slice[offset]
    } else {
        get_char_recursive(
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
    original: &[Character],
    add: &[Character],
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

            let slice = get_piece_slice(&n.piece, original, add);
            for ch in &slice[p_start..p_end] {
                // use std::fmt::Write;
                // Reconstruct bytes logic: simplified.
                // We should use Character::render to a byte buffer?
                // Character::render uses fmt::Write (String).
                // We can render to local string and push bytes.
                let mut buf = String::new();
                let _ = ch.render(&mut buf);
                out.extend_from_slice(buf.as_bytes());
            }
        }

        // Check intersection with right child
        if range.end > current_len {
            let r_start = std::cmp::max(range.start, current_len) - current_len;
            let r_end = range.end - current_len;
            collect_bytes_range(n.right.as_deref(), r_start..r_end, original, add, out);
        }
    }
}

fn collect_chars_range(
    node: Option<&Node>,
    range: std::ops::Range<usize>,
    original: &[Character],
    add: &[Character],
    out: &mut Vec<Character>,
) {
    if let Some(n) = node {
        let left_len = n.left.as_ref().map_or(0, |n| n.len);
        let piece_len = n.piece.len;
        let current_len = left_len + piece_len;

        // Check intersection with left child
        if range.start < left_len {
            let l_start = range.start;
            let l_end = std::cmp::min(range.end, left_len);
            collect_chars_range(n.left.as_deref(), l_start..l_end, original, add, out);
        }

        // Check intersection with current piece
        if range.end > left_len && range.start < current_len {
            let p_start = std::cmp::max(range.start, left_len) - left_len;
            let p_end = std::cmp::min(range.end, current_len) - left_len;

            let slice = get_piece_slice(&n.piece, original, add);
            out.extend_from_slice(&slice[p_start..p_end]);
        }

        // Check intersection with right child
        if range.end > current_len {
            let r_start = std::cmp::max(range.start, current_len) - current_len;
            let r_end = range.end - current_len;
            collect_chars_range(n.right.as_deref(), r_start..r_end, original, add, out);
        }
    }
}

fn find_nth_newline_end(
    node: Option<&Node>,
    target: usize,
    original: &[Character],
    add: &[Character],
) -> usize {
    let node = node.unwrap();
    let left_nl = node.left.as_ref().map_or(0, |n| n.newlines);

    if target <= left_nl {
        find_nth_newline_end(node.left.as_deref(), target, original, add)
    } else {
        let slice = get_piece_slice(&node.piece, original, add);
        let (piece_nl, _) = count_stats(slice);
        let current_nl = left_nl + piece_nl;

        if target <= current_nl {
            // In this piece
            let needed_in_piece = target - left_nl;

            let mut count = 0;
            for (i, c) in slice.iter().enumerate() {
                if *c == Character::Newline {
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

fn get_line_at_pos(
    node: Option<&Node>,
    pos: usize,
    original: &[Character],
    add: &[Character],
) -> usize {
    let node = node.unwrap();
    let left_len = node.left.as_ref().map_or(0, |n| n.len);

    if pos < left_len {
        get_line_at_pos(node.left.as_deref(), pos, original, add)
    } else {
        let left_nl = node.left.as_ref().map_or(0, |n| n.newlines);

        if pos < left_len + node.piece.len {
            // In this piece
            let offset = pos - left_len;
            let slice = get_piece_slice(&node.piece, original, add);

            let (piece_nl_before, _) = count_stats(&slice[..offset]);
            left_nl + piece_nl_before
        } else {
            // In right child
            let slice = get_piece_slice(&node.piece, original, add);
            let (piece_nl, _) = count_stats(slice);
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

fn get_byte_offset_recursive(
    node: Option<&Node>,
    char_pos: usize,
    original: &[Character],
    add: &[Character],
) -> usize {
    let node = node.unwrap();
    let left_len = node.left.as_ref().map_or(0, |n| n.len);
    let left_byte_len = node.left.as_ref().map_or(0, |n| n.byte_len);

    if char_pos < left_len {
        get_byte_offset_recursive(node.left.as_deref(), char_pos, original, add)
    } else if char_pos < left_len + node.piece.len {
        // In this piece
        let offset = char_pos - left_len;
        let slice = get_piece_slice(&node.piece, original, add);

        // Sum byte len of `offset` characters
        let mut piece_bytes = 0;
        for i in 0..offset {
            piece_bytes += slice[i].len_utf8();
        }
        left_byte_len + piece_bytes
    } else {
        // In right child
        left_byte_len
            + node.piece_byte_len
            + get_byte_offset_recursive(
                node.right.as_deref(),
                char_pos - left_len - node.piece.len,
                original,
                add,
            )
    }
}

fn get_char_idx_recursive(
    node: Option<&Node>,
    byte_pos: usize,
    original: &[Character],
    add: &[Character],
) -> usize {
    let node = node.unwrap();
    let left_byte_len = node.left.as_ref().map_or(0, |n| n.byte_len);
    let left_len = node.left.as_ref().map_or(0, |n| n.len);

    if byte_pos < left_byte_len {
        get_char_idx_recursive(node.left.as_deref(), byte_pos, original, add)
    } else if byte_pos < left_byte_len + node.piece_byte_len {
        // In this piece
        let target_in_piece = byte_pos - left_byte_len;
        let slice = get_piece_slice(&node.piece, original, add);

        let mut current_bytes = 0;
        for (i, c) in slice.iter().enumerate() {
            let clen = c.len_utf8();
            if current_bytes + clen > target_in_piece {
                // The byte is within this character
                return left_len + i;
            }
            current_bytes += clen;
            // If we hit exact match (start of next char), loop continues
            if current_bytes == target_in_piece {
                // Technically points to start of next char,
                // but loop will likely continue to next iteration where we'll hit condition?
                // No, if `byte_pos` points to start of char, `current_bytes` matches `target_in_piece`.
                // We need to return `i+1`?
                // Wait.
                // If `byte_pos` is 0, `target` is 0. `current` starts 0.
                // Loop 0: `clen`=1. `0+1 > 0` is true. Returns `left_len + 0`. Correct.

                // If `byte_pos` is 1 (start of 2nd char). `target` is 1.
                // Loop 0: `c`='A' (1 byte). `0+1 > 1` is FALSE.
                // `current` becomes 1. `1 == 1`.
                // Loop 1: `c`='B'. `1+1 > 1` is true. Match. Returns `left_len + 1`. Correct.
            }
        }
        // Should have found it in this piece
        left_len + slice.len()
    } else {
        // In right child
        left_len
            + node.piece.len
            + get_char_idx_recursive(
                node.right.as_deref(),
                byte_pos - left_byte_len - node.piece_byte_len,
                original,
                add,
            )
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
mod conversion_tests;

#[cfg(test)]
mod iterator_tests;

#[cfg(test)]
mod logical_bytes_tests;
