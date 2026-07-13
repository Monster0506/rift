use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntervalTree<T> {
    nodes: Vec<Node<T>>,
    root: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Node<T> {
    range: Range<usize>,
    val: T,
    max: usize, // Max end point in this subtree
    left: Option<usize>,
    right: Option<usize>,
}

impl<T: Clone> IntervalTree<T> {
    pub fn new(items: Vec<(Range<usize>, T)>) -> Self {
        if items.is_empty() {
            return Self {
                nodes: Vec::new(),
                root: None,
            };
        }

        let mut items = items;
        items.sort_by(|a, b| {
            a.0.start
                .cmp(&b.0.start)
                .then_with(|| a.0.end.cmp(&b.0.end))
        });

        let mut nodes = Vec::with_capacity(items.len());
        let root = Self::build(&items, &mut nodes);

        Self { nodes, root }
    }

    // Builds a balanced tree from sorted items
    fn build(items: &[(Range<usize>, T)], nodes: &mut Vec<Node<T>>) -> Option<usize> {
        if items.is_empty() {
            return None;
        }

        let mid = items.len() / 2;
        let (range, val) = &items[mid];

        let idx = nodes.len();
        // Reserve slot
        nodes.push(Node {
            range: range.clone(),
            val: val.clone(),
            max: range.end,
            left: None,
            right: None,
        });

        let left_idx = Self::build(&items[..mid], nodes);
        let right_idx = Self::build(&items[mid + 1..], nodes);

        nodes[idx].left = left_idx;
        nodes[idx].right = right_idx;

        // Update max
        let mut max_end = range.end;
        if let Some(l) = left_idx {
            if nodes[l].max > max_end {
                max_end = nodes[l].max;
            }
        }
        if let Some(r) = right_idx {
            if nodes[r].max > max_end {
                max_end = nodes[r].max;
            }
        }
        nodes[idx].max = max_end;

        Some(idx)
    }

    pub fn query(&self, query_range: Range<usize>) -> Vec<(Range<usize>, T)> {
        let mut results = Vec::new();
        if let Some(root) = self.root {
            self.query_recursive(root, &query_range, &mut results);
        }
        results
    }

    // Returns all items that OVERLAP the query range
    fn query_recursive(
        &self,
        node_idx: usize,
        query: &Range<usize>,
        results: &mut Vec<(Range<usize>, T)>,
    ) {
        let node = &self.nodes[node_idx];

        // Left child: visit if `max > query.start`
        if let Some(left) = node.left {
            if self.nodes[left].max > query.start {
                self.query_recursive(left, query, results);
            }
        }

        // Check overlap: start < query.end && end > query.start
        if node.range.start < query.end && node.range.end > query.start {
            results.push((node.range.clone(), node.val.clone()));
        }

        if let Some(right) = node.right {
            if self.nodes[right].max > query.start && node.range.start < query.end {
                self.query_recursive(right, query, results);
            }
        }
    }

    // Helper to get all items (e.g. for iteration)
    pub fn iter(&self) -> impl Iterator<Item = (&Range<usize>, &T)> {
        self.nodes.iter().map(|n| (&n.range, &n.val))
    }

    /// In-order traversal: same relative ordering as `query`, unlike `iter`
    /// (which walks `self.nodes` in build order and can reorder same-range ties).
    fn in_order(&self) -> Vec<(Range<usize>, T)> {
        let mut results = Vec::with_capacity(self.nodes.len());
        if let Some(root) = self.root {
            self.in_order_recursive(root, &mut results);
        }
        results
    }

    fn in_order_recursive(&self, node_idx: usize, results: &mut Vec<(Range<usize>, T)>) {
        let node = &self.nodes[node_idx];
        if let Some(left) = node.left {
            self.in_order_recursive(left, results);
        }
        results.push((node.range.clone(), node.val.clone()));
        if let Some(right) = node.right {
            self.in_order_recursive(right, results);
        }
    }

    /// Update every node's range from `get_range` in place, preserving tree topology, then recompute `max` bottom-up.
    pub fn resync(&mut self, mut get_range: impl FnMut(&T) -> Option<Range<usize>>) {
        for node in &mut self.nodes {
            if let Some(r) = get_range(&node.val) {
                node.range = r;
            }
        }
        // Children always have a higher index than their parent, so a reverse
        // pass finalizes both children before their parent.
        for idx in (0..self.nodes.len()).rev() {
            let mut max_end = self.nodes[idx].range.end;
            if let Some(l) = self.nodes[idx].left {
                max_end = max_end.max(self.nodes[l].max);
            }
            if let Some(r) = self.nodes[idx].right {
                max_end = max_end.max(self.nodes[r].max);
            }
            self.nodes[idx].max = max_end;
        }
    }

    /// Ranges at/before the edit are kept, at/after its old end shift by the
    /// delta, overlapping it are dropped (caller must also filter its fresh requery for touching cases).
    pub fn shift_for_edit(
        &self,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> Vec<(Range<usize>, T)> {
        let delta = new_end_byte as i64 - old_end_byte as i64;
        self.in_order()
            .into_iter()
            .filter_map(|(r, val)| {
                if r.end <= start_byte {
                    Some((r, val))
                } else if r.start >= old_end_byte {
                    let new_start = (r.start as i64 + delta) as usize;
                    let new_end = (r.end as i64 + delta) as usize;
                    Some((new_start..new_end, val))
                } else {
                    None
                }
            })
            .collect()
    }
}

// Default for easy instantiation
impl<T: Clone> Default for IntervalTree<T> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
        }
    }
}

#[cfg(test)]
#[path = "interval_tree_tests.rs"]
mod tests;
