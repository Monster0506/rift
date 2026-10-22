use super::tree::{SplitNode, SplitTree};
use super::window::WindowId;
use crate::num::f64_to_usize;

pub(crate) const MIN_WINDOW_ROWS: usize = 3;
pub(crate) const MIN_WINDOW_COLS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowLayout {
    pub(crate) window_id: WindowId,
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
}

impl SplitTree {
    #[must_use]
    pub(crate) fn compute_layout(&self, total_rows: usize, total_cols: usize) -> Vec<WindowLayout> {
        let mut layouts = Vec::new();
        compute_node_layout(&self.root, 0, 0, total_rows, total_cols, &mut layouts);
        layouts
    }
}

fn compute_node_layout(
    node: &SplitNode,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
    layouts: &mut Vec<WindowLayout>,
) {
    match node {
        SplitNode::Leaf(window_id) => {
            layouts.push(WindowLayout {
                window_id: *window_id,
                row,
                col,
                rows,
                cols,
            });
        }
        SplitNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            use super::tree::{cells_to_f64, SplitDirection};
            match direction {
                SplitDirection::Horizontal => {
                    let available = rows.saturating_sub(1);
                    let first_rows = f64_to_usize((cells_to_f64(available) * ratio).round());
                    let first_rows = if available < 2 * MIN_WINDOW_ROWS {
                        available / 2
                    } else {
                        let lo = MIN_WINDOW_ROWS.min(available / 2);
                        first_rows.clamp(lo, available - lo)
                    };
                    let second_rows = available.saturating_sub(first_rows);

                    compute_node_layout(first, row, col, first_rows, cols, layouts);
                    compute_node_layout(
                        second,
                        row + first_rows + 1,
                        col,
                        second_rows,
                        cols,
                        layouts,
                    );
                }
                SplitDirection::Vertical => {
                    let available = cols.saturating_sub(1);
                    let first_cols = f64_to_usize((cells_to_f64(available) * ratio).round());
                    let first_cols = if available < 2 * MIN_WINDOW_COLS {
                        available / 2
                    } else {
                        let lo = MIN_WINDOW_COLS.min(available / 2);
                        first_cols.clamp(lo, available - lo)
                    };
                    let second_cols = available.saturating_sub(first_cols);

                    compute_node_layout(first, row, col, rows, first_cols, layouts);
                    compute_node_layout(
                        second,
                        row,
                        col + first_cols + 1,
                        rows,
                        second_cols,
                        layouts,
                    );
                }
            }
        }
    }
}
