//! Undo Tree Visualization
//!
//! Renders the undo history as a vertical "git-graph" style tree.

use crate::history::{EditSeq, UndoTree};

/// Render the undo tree to a list of lines (chars) and a mapping of sequences
pub fn render_tree(tree: &UndoTree) -> (Vec<Vec<char>>, Vec<EditSeq>, usize) {
    let mut lines = Vec::new();
    let mut sequences = Vec::new();
    let mut cursor_row = 0;

    let mut all_seqs: Vec<EditSeq> = tree.nodes.keys().cloned().collect();
    all_seqs.sort_by(|a, b| b.cmp(a)); // Descending

    let mut columns: Vec<Option<EditSeq>> = Vec::new();

    for &seq in &all_seqs {
        let node = match tree.nodes.get(&seq) {
            Some(n) => n,
            None => continue,
        };

        let is_current = seq == tree.current;
        if is_current {
            cursor_row = lines.len();
        }

        let mut col_indices: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, waiting_for)| **waiting_for == Some(seq))
            .map(|(i, _)| i)
            .collect();

        // If no column is waiting for us, we are a Tip (new branch head)
        let is_tip = col_indices.is_empty();
        if is_tip {
            // Assign a new column (allocating slot)
            let slot = if let Some(idx) = columns.iter().position(|c| c.is_none()) {
                idx
            } else {
                columns.push(None);
                columns.len() - 1
            };
            col_indices.push(slot);
        }

        // The "main" column for this node is usually the first one found or allocated
        let main_col = col_indices[0];

        // We need to extend chars to cover all active columns
        let max_col = columns.len();

        // Build the string for this row
        let mut row_str = String::new();

        // Render node marker and vertical lines
        for i in 0..max_col {
            if i == main_col {
                // This is the node
                if is_current {
                    row_str.push('@');
                } else {
                    row_str.push('o');
                }
            } else if col_indices.contains(&i) {
                row_str.push(' '); // Placeholder
            } else {
                // Unrelated column
                if let Some(_) = columns[i] {
                    row_str.push('│');
                } else {
                    row_str.push(' ');
                }
            }
            row_str.push(' '); // Spacing
        }

        if col_indices.len() > 1 {
            let mut conn_str = String::new();
            for c in 0..max_col {
                if c == main_col {
                    conn_str.push('│');
                } else if col_indices.contains(&c) {
                    if c > main_col {
                        conn_str.push('/'); // Shifts left
                    } else {
                        conn_str.push('\\'); // Shifts right
                    }
                } else if columns[c].is_some() {
                    conn_str.push('│');
                } else {
                    conn_str.push(' ');
                }
                conn_str.push(' ');
            }
            lines.push(conn_str.chars().collect());
            sequences.push(EditSeq::MAX)
        }

        sequences.push(seq);
        let mut final_row = String::new();
        for c in 0..max_col {
            if c == main_col {
                if is_current {
                    final_row.push('@');
                } else {
                    final_row.push('o');
                }
            } else if col_indices.contains(&c) {
                final_row.push(' ');
            } else if columns[c].is_some() {
                final_row.push('│');
            } else {
                final_row.push(' ');
            }
            final_row.push(' ');
        }

        let snap_marker = if node.snapshot.is_some() { "*" } else { "" };
        let desc = format!(
            " [{}{}{}] {}",
            snap_marker, seq, snap_marker, node.transaction.description
        );
        final_row.push_str(&desc);

        lines.push(final_row.chars().collect());

        columns[main_col] = node.parent;

        for &idx in &col_indices {
            if idx != main_col {
                columns[idx] = None;
            }
        }
    }

    (lines, sequences, cursor_row)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
