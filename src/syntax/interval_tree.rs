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

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
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
mod tests {
    use super::*;

    #[test]
    fn test_interval_tree_basic() {
        let items = vec![(0..10, 1), (5..15, 2), (20..30, 3)];

        let tree = IntervalTree::new(items);

        let res = tree.query(0..5);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].1, 1);

        let res = tree.query(5..10);
        assert_eq!(res.len(), 2);

        let res = tree.query(16..19);
        assert_eq!(res.len(), 0);

        let res = tree.query(25..26);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].1, 3);
    }

    #[test]
    fn test_interval_tree_nested() {
        let items = vec![(0..100, 1), (10..20, 2), (50..60, 3)];

        let tree = IntervalTree::new(items);

        let res = tree.query(15..16);
        assert_eq!(res.len(), 2);

        let res = tree.query(55..56);
        assert_eq!(res.len(), 2);

        let res = tree.query(5..6);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].1, 1);
    }

    #[test]
    fn test_interval_tree_empty() {
        let tree: IntervalTree<i32> = IntervalTree::new(vec![]);
        assert!(tree.query(0..10).is_empty());
    }
    #[test]
    fn test_interval_tree_sorted_query() {
        // Tree structure: Root (5..15), Left (0..10), Right (20..30)
        let items = vec![(0..10, 1), (5..15, 2), (20..30, 3)];

        let tree = IntervalTree::new(items);

        // Query (0..30) should return all, sorted by start
        let res = tree.query(0..30);
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].1, 1); // 0..10 sorted first
        assert_eq!(res[1].1, 2); // 5..15
        assert_eq!(res[2].1, 3); // 20..30
    }
}
