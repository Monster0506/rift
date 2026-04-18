use std::collections::HashMap;

use crate::document::DocumentId;

use super::navigation::Direction;
use super::window::{Window, WindowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

pub enum SplitNode {
    Leaf(WindowId),
    Split {
        direction: SplitDirection,
        ratio: f64,
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
}

pub struct SplitTree {
    pub root: SplitNode,
    pub windows: HashMap<WindowId, Window>,
    pub focused_window: WindowId,
    pub previous_window: Option<WindowId>,
    next_window_id: WindowId,
}

impl SplitTree {
    pub fn new(doc_id: DocumentId, viewport_rows: usize, viewport_cols: usize) -> Self {
        let window_id = 1;
        let window = Window::new(window_id, doc_id, viewport_rows, viewport_cols);
        let mut windows = HashMap::new();
        windows.insert(window_id, window);

        SplitTree {
            root: SplitNode::Leaf(window_id),
            windows,
            focused_window: window_id,
            previous_window: None,
            next_window_id: 2,
        }
    }

    pub fn split(
        &mut self,
        direction: SplitDirection,
        target_id: WindowId,
        new_doc_id: DocumentId,
        viewport_rows: usize,
        viewport_cols: usize,
    ) -> WindowId {
        let new_id = self.next_window_id;
        self.next_window_id += 1;

        let cursor_pos = self
            .windows
            .get(&target_id)
            .map(|w| {
                if w.document_id == new_doc_id {
                    w.cursor_position
                } else {
                    0
                }
            })
            .unwrap_or(0);

        let mut new_window = Window::new(new_id, new_doc_id, viewport_rows, viewport_cols);
        new_window.cursor_position = cursor_pos;
        self.windows.insert(new_id, new_window);

        self.root = Self::replace_leaf(
            std::mem::replace(&mut self.root, SplitNode::Leaf(0)),
            target_id,
            direction,
            new_id,
        );

        new_id
    }

    fn replace_leaf(
        node: SplitNode,
        target_id: WindowId,
        direction: SplitDirection,
        new_id: WindowId,
    ) -> SplitNode {
        match node {
            SplitNode::Leaf(id) if id == target_id => SplitNode::Split {
                direction,
                ratio: 0.5,
                first: Box::new(SplitNode::Leaf(id)),
                second: Box::new(SplitNode::Leaf(new_id)),
            },
            SplitNode::Split {
                direction: d,
                ratio,
                first,
                second,
            } => SplitNode::Split {
                direction: d,
                ratio,
                first: Box::new(Self::replace_leaf(*first, target_id, direction, new_id)),
                second: Box::new(Self::replace_leaf(*second, target_id, direction, new_id)),
            },
            other => other,
        }
    }

    pub fn close_window(&mut self, id: WindowId) -> bool {
        if self.windows.len() <= 1 {
            return false;
        }

        self.windows.remove(&id);
        self.root = Self::remove_leaf(std::mem::replace(&mut self.root, SplitNode::Leaf(0)), id);

        if self.focused_window == id {
            self.focused_window = *self.windows.keys().next().unwrap();
        }

        true
    }

    fn remove_leaf(node: SplitNode, target_id: WindowId) -> SplitNode {
        match node {
            SplitNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                if matches!(&*first, SplitNode::Leaf(id) if *id == target_id) {
                    return *second;
                }
                if matches!(&*second, SplitNode::Leaf(id) if *id == target_id) {
                    return *first;
                }
                SplitNode::Split {
                    direction,
                    ratio,
                    first: Box::new(Self::remove_leaf(*first, target_id)),
                    second: Box::new(Self::remove_leaf(*second, target_id)),
                }
            }
            other => other,
        }
    }

    pub fn focused_window(&self) -> &Window {
        self.windows.get(&self.focused_window).unwrap()
    }

    pub fn focused_window_mut(&mut self) -> &mut Window {
        self.windows.get_mut(&self.focused_window).unwrap()
    }

    pub fn focused_window_id(&self) -> WindowId {
        self.focused_window
    }

    pub fn set_focus(&mut self, id: WindowId) -> bool {
        if self.windows.contains_key(&id) {
            self.previous_window = Some(self.focused_window);
            self.focused_window = id;
            true
        } else {
            false
        }
    }

    pub fn move_window(&mut self, direction: Direction, layouts: &[super::layout::WindowLayout]) -> bool {
        if self.windows.len() <= 1 {
            return false;
        }

        let focused_id = self.focused_window;
        let neighbor_id = self.navigate(direction, layouts);

        let desired_cols: std::collections::HashMap<WindowId, usize> =
            layouts.iter().map(|l| (l.window_id, l.cols)).collect();

        let (split_dir, focused_is_first) = match direction {
            Direction::Left  => (SplitDirection::Vertical,   true),
            Direction::Right => (SplitDirection::Vertical,   false),
            Direction::Up    => (SplitDirection::Horizontal, true),
            Direction::Down  => (SplitDirection::Horizontal, false),
        };

        if let Some(nid) = neighbor_id {
            let insert_is_first = matches!(direction, Direction::Left);
            self.root = Self::remove_leaf(
                std::mem::replace(&mut self.root, SplitNode::Leaf(0)),
                focused_id,
            );
            self.root = Self::insert_adjacent(
                std::mem::replace(&mut self.root, SplitNode::Leaf(0)),
                nid,
                SplitDirection::Vertical,
                focused_id,
                insert_is_first,
                0.5,
            );
            Self::rebalance_cols(&mut self.root, &desired_cols);
        } else if Self::is_correctly_positioned(&self.root, focused_id, split_dir, focused_is_first) {
            let w_size = layouts.iter().find(|l| l.window_id == focused_id)
                .map(|l| match split_dir {
                    SplitDirection::Vertical   => l.cols,
                    SplitDirection::Horizontal => l.rows,
                })
                .unwrap_or(1);
            let total = match split_dir {
                SplitDirection::Vertical   => layouts.iter().map(|l| l.cols).max().unwrap_or(1),
                SplitDirection::Horizontal => layouts.iter().map(|l| l.rows).max().unwrap_or(1),
            };
            let ratio = if focused_is_first {
                (w_size as f64 / total as f64).clamp(0.1, 0.9)
            } else {
                ((total - w_size) as f64 / total as f64).clamp(0.1, 0.9)
            };

            self.root = Self::remove_leaf(
                std::mem::replace(&mut self.root, SplitNode::Leaf(0)),
                focused_id,
            );
            let old_root = std::mem::replace(&mut self.root, SplitNode::Leaf(0));
            self.root = if focused_is_first {
                SplitNode::Split {
                    direction: split_dir,
                    ratio,
                    first:  Box::new(SplitNode::Leaf(focused_id)),
                    second: Box::new(old_root),
                }
            } else {
                SplitNode::Split {
                    direction: split_dir,
                    ratio,
                    first:  Box::new(old_root),
                    second: Box::new(SplitNode::Leaf(focused_id)),
                }
            };
            Self::rebalance_cols(&mut self.root, &desired_cols);
        } else {
            Self::flip_parent_direction(&mut self.root, focused_id, direction);
        }

        self.focused_window = focused_id;
        true
    }

    fn flip_parent_direction(node: &mut SplitNode, target_id: WindowId, direction: Direction) -> bool {
        match node {
            SplitNode::Leaf(_) => false,
            SplitNode::Split { direction: d, first, second, .. } => {
                let in_first  = Self::contains_window(first,  target_id);
                let in_second = Self::contains_window(second, target_id);

                if !in_first && !in_second {
                    return false;
                }

                let is_first_leaf  = matches!(&**first,  SplitNode::Leaf(id) if *id == target_id);
                let is_second_leaf = matches!(&**second, SplitNode::Leaf(id) if *id == target_id);

                if is_first_leaf || is_second_leaf {
                    let (new_dir, should_be_first) = match direction {
                        Direction::Left  => (SplitDirection::Vertical,   true),
                        Direction::Right => (SplitDirection::Vertical,   false),
                        Direction::Up    => (SplitDirection::Horizontal, true),
                        Direction::Down  => (SplitDirection::Horizontal, false),
                    };
                    *d = new_dir;
                    if should_be_first != is_first_leaf {
                        std::mem::swap(first, second);
                    }
                    return true;
                }

                if in_first {
                    Self::flip_parent_direction(first, target_id, direction)
                } else {
                    Self::flip_parent_direction(second, target_id, direction)
                }
            }
        }
    }

    fn insert_adjacent(
        node: SplitNode,
        target_id: WindowId,
        direction: SplitDirection,
        new_id: WindowId,
        new_is_first: bool,
        ratio: f64,
    ) -> SplitNode {
        match node {
            SplitNode::Leaf(id) if id == target_id => {
                if new_is_first {
                    SplitNode::Split {
                        direction,
                        ratio,
                        first: Box::new(SplitNode::Leaf(new_id)),
                        second: Box::new(SplitNode::Leaf(id)),
                    }
                } else {
                    SplitNode::Split {
                        direction,
                        ratio,
                        first: Box::new(SplitNode::Leaf(id)),
                        second: Box::new(SplitNode::Leaf(new_id)),
                    }
                }
            }
            SplitNode::Split { direction: d, ratio: r, first, second } => SplitNode::Split {
                direction: d,
                ratio: r,
                first:  Box::new(Self::insert_adjacent(*first,  target_id, direction, new_id, new_is_first, ratio)),
                second: Box::new(Self::insert_adjacent(*second, target_id, direction, new_id, new_is_first, ratio)),
            },
            other => other,
        }
    }

    pub fn exchange_windows(&mut self, id1: WindowId, id2: WindowId) -> bool {
        if id1 == id2 || !self.windows.contains_key(&id1) || !self.windows.contains_key(&id2) {
            return false;
        }
        let (doc1, cursor1) = {
            let w = &self.windows[&id1];
            (w.document_id, w.cursor_position)
        };
        let (doc2, cursor2) = {
            let w = &self.windows[&id2];
            (w.document_id, w.cursor_position)
        };
        let w1 = self.windows.get_mut(&id1).unwrap();
        w1.document_id = doc2;
        w1.cursor_position = cursor2;
        let w2 = self.windows.get_mut(&id2).unwrap();
        w2.document_id = doc1;
        w2.cursor_position = cursor1;
        true
    }

    pub fn focus_previous(&mut self) -> Option<WindowId> {
        let prev = self.previous_window?;
        if self.windows.contains_key(&prev) {
            self.set_focus(prev);
            Some(prev)
        } else {
            self.previous_window = None;
            None
        }
    }

    pub fn windows_for_document(&self, doc_id: DocumentId) -> Vec<WindowId> {
        self.windows
            .iter()
            .filter(|(_, w)| w.document_id == doc_id)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn all_window_ids(&self) -> Vec<WindowId> {
        self.windows.keys().copied().collect()
    }

    pub fn get_window(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id)
    }

    pub fn get_window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    pub fn resize_focused(
        &mut self,
        direction: SplitDirection,
        delta: f64,
        _layouts: &[super::layout::WindowLayout],
    ) -> bool {
        Self::resize_node(&mut self.root, self.focused_window, direction, delta)
    }

    fn resize_node(
        node: &mut SplitNode,
        target_id: WindowId,
        direction: SplitDirection,
        delta: f64,
    ) -> bool {
        match node {
            SplitNode::Leaf(_) => false,
            SplitNode::Split {
                direction: d,
                ratio,
                first,
                second,
            } => {
                let in_first = Self::contains_window(first, target_id);
                let in_second = Self::contains_window(second, target_id);

                if !in_first && !in_second {
                    return false;
                }

                // Try deeper first so the innermost matching split is adjusted,
                // not the outermost ancestor (which would shift all sibling panes).
                let child_adjusted = if in_first {
                    Self::resize_node(first, target_id, direction, delta)
                } else {
                    Self::resize_node(second, target_id, direction, delta)
                };

                if child_adjusted {
                    return true;
                }

                // No deeper match in this direction — adjust at this level.
                if *d == direction {
                    let new_ratio = if in_first {
                        *ratio + delta
                    } else {
                        *ratio - delta
                    };
                    *ratio = new_ratio.clamp(0.1, 0.9);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn is_correctly_positioned(
        node: &SplitNode,
        target_id: WindowId,
        expected_dir: SplitDirection,
        should_be_first: bool,
    ) -> bool {
        match node {
            SplitNode::Leaf(_) => false,
            SplitNode::Split { direction, first, second, .. } => {
                let is_first_leaf  = matches!(&**first,  SplitNode::Leaf(id) if *id == target_id);
                let is_second_leaf = matches!(&**second, SplitNode::Leaf(id) if *id == target_id);
                if is_first_leaf && *direction == expected_dir && should_be_first {
                    return true;
                }
                if is_second_leaf && *direction == expected_dir && !should_be_first {
                    return true;
                }
                Self::is_correctly_positioned(first,  target_id, expected_dir, should_be_first)
                    || Self::is_correctly_positioned(second, target_id, expected_dir, should_be_first)
            }
        }
    }

    fn contains_window(node: &SplitNode, target_id: WindowId) -> bool {
        match node {
            SplitNode::Leaf(id) => *id == target_id,
            SplitNode::Split { first, second, .. } => {
                Self::contains_window(first, target_id) || Self::contains_window(second, target_id)
            }
        }
    }

    fn rebalance_cols(node: &mut SplitNode, desired: &std::collections::HashMap<WindowId, usize>) {
        match node {
            SplitNode::Leaf(_) => {}
            SplitNode::Split { direction, ratio, first, second, .. } => {
                if *direction == SplitDirection::Vertical {
                    let fs = Self::sum_leaf_cols(first, desired);
                    let ss = Self::sum_leaf_cols(second, desired);
                    let total = fs + ss;
                    if total > 0 {
                        *ratio = (fs as f64 / total as f64).clamp(0.1, 0.9);
                    }
                }
                Self::rebalance_cols(first, desired);
                Self::rebalance_cols(second, desired);
            }
        }
    }

    fn sum_leaf_cols(node: &SplitNode, desired: &std::collections::HashMap<WindowId, usize>) -> usize {
        match node {
            SplitNode::Leaf(id) => *desired.get(id).unwrap_or(&1),
            SplitNode::Split { direction, first, second, .. } => {
                if *direction == SplitDirection::Vertical {
                    Self::sum_leaf_cols(first, desired) + Self::sum_leaf_cols(second, desired)
                } else {
                    Self::sum_leaf_cols(first, desired)
                }
            }
        }
    }

    pub fn equalize(&mut self, proportional: bool) {
        Self::equalize_node(&mut self.root, proportional);
    }

    fn equalize_node(node: &mut SplitNode, proportional: bool) {
        match node {
            SplitNode::Leaf(_) => {}
            SplitNode::Split { ratio, first, second, .. } => {
                *ratio = if proportional {
                    let fc = Self::count_leaves(first) as f64;
                    let sc = Self::count_leaves(second) as f64;
                    (fc / (fc + sc)).clamp(0.1, 0.9)
                } else {
                    0.5
                };
                Self::equalize_node(first, proportional);
                Self::equalize_node(second, proportional);
            }
        }
    }

    fn count_leaves(node: &SplitNode) -> usize {
        match node {
            SplitNode::Leaf(_) => 1,
            SplitNode::Split { first, second, .. } => {
                Self::count_leaves(first) + Self::count_leaves(second)
            }
        }
    }
}
