use super::tree::{SplitNode, SplitTree};
use super::window::WindowId;

pub const MIN_WINDOW_ROWS: usize = 3;
pub const MIN_WINDOW_COLS: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct WindowLayout {
    pub window_id: WindowId,
    pub row: usize,
    pub col: usize,
    pub rows: usize,
    pub cols: usize,
}

impl SplitTree {
    pub fn compute_layout(&self, total_rows: usize, total_cols: usize) -> Vec<WindowLayout> {
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
            use super::tree::SplitDirection;
            match direction {
                SplitDirection::Horizontal => {
                    let available = rows.saturating_sub(1);
                    let first_rows = ((available as f64) * ratio).round() as usize;
                    let first_rows = first_rows
                        .max(MIN_WINDOW_ROWS)
                        .min(available.saturating_sub(MIN_WINDOW_ROWS));
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
                    let first_cols = ((available as f64) * ratio).round() as usize;
                    let first_cols = first_cols
                        .max(MIN_WINDOW_COLS)
                        .min(available.saturating_sub(MIN_WINDOW_COLS));
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
