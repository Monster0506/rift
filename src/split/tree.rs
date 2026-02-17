use std::collections::HashMap;

use crate::document::DocumentId;

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
            .map(|w| if w.document_id == new_doc_id { w.cursor_position } else { 0 })
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
        self.root = Self::remove_leaf(
            std::mem::replace(&mut self.root, SplitNode::Leaf(0)),
            id,
        );

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
            self.focused_window = id;
            true
        } else {
            false
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

                if *d == direction && (in_first || in_second) {
                    let new_ratio = if in_first {
                        *ratio + delta
                    } else {
                        *ratio - delta
                    };
                    *ratio = new_ratio.clamp(0.1, 0.9);
                    return true;
                }

                if in_first {
                    Self::resize_node(first, target_id, direction, delta)
                } else {
                    Self::resize_node(second, target_id, direction, delta)
                }
            }
        }
    }

    fn contains_window(node: &SplitNode, target_id: WindowId) -> bool {
        match node {
            SplitNode::Leaf(id) => *id == target_id,
            SplitNode::Split { first, second, .. } => {
                Self::contains_window(first, target_id)
                    || Self::contains_window(second, target_id)
            }
        }
    }
}
