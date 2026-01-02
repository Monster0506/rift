//! Undo Tree Visualization
//!
//! Renders the undo history as a vertical "git-graph" style tree.

use crate::history::{EditSeq, UndoTree};

/// Render the undo tree to a list of lines (Cells) and a mapping of sequences
pub fn render_tree(tree: &UndoTree) -> (Vec<Vec<crate::layer::Cell>>, Vec<EditSeq>, usize) {
    use crate::color::Color;
    use crate::layer::Cell;

    let mut lines = Vec::new();
    let mut sequences = Vec::new();
    let mut cursor_row = 0;

    let mut all_seqs: Vec<EditSeq> = tree.nodes.keys().cloned().collect();
    all_seqs.sort_by(|a, b| b.cmp(a)); // Descending

    let mut columns: Vec<Option<EditSeq>> = Vec::new();

    // Define colors
    let current_node_color = Color::Green;
    let node_color = Color::DarkYellow;
    let branch_color = Color::Blue;
    let text_color = Color::Grey;
    let current_text_color = Color::White;

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

        // Build the current row (connector line above the node)
        // Actually, previous logic output TWO lines? No, it output ONE line IF multiple parents/children involved?
        // Wait, the previous logic had a block: `if col_indices.len() > 1`.
        // This handles "merges" or multiple incoming branches?
        // Let's preserve that logic but use Cells.

        if col_indices.len() > 1 {
            let mut conn_line: Vec<Cell> = Vec::new();
            for (c, item) in columns.iter().enumerate().take(max_col) {
                let cell = if c == main_col {
                    Cell::from_char('│').with_fg(branch_color)
                } else if col_indices.contains(&c) {
                    if c > main_col {
                        Cell::from_char('/').with_fg(branch_color)
                    } else {
                        Cell::from_char('\\').with_fg(branch_color)
                    }
                } else if item.is_some() {
                    Cell::from_char('│').with_fg(branch_color)
                } else {
                    Cell::new(b' ')
                };
                conn_line.push(cell);
                conn_line.push(Cell::new(b' '));
            }
            lines.push(conn_line);
            sequences.push(EditSeq::MAX)
        }

        sequences.push(seq);
        let mut final_row: Vec<Cell> = Vec::new();

        // 1. Draw Graph part
        for (c, item) in columns.iter().enumerate().take(max_col) {
            let cell = if c == main_col {
                if is_current {
                    Cell::from_char('@').with_fg(current_node_color)
                } else {
                    Cell::from_char('o').with_fg(node_color)
                }
            } else if col_indices.contains(&c) {
                // Should not happen if we did potential connector line above?
                // Actually if len > 1 we output connector line, then THIS line puts node at main_col.
                // The OTHER columns at this point are just spaces since they merged?
                // Wait, previous logic: `if col_indices.contains(&c) { push(' ') }`
                Cell::new(b' ')
            } else if item.is_some() {
                Cell::from_char('│').with_fg(branch_color)
            } else {
                Cell::new(b' ')
            };
            final_row.push(cell);
            final_row.push(Cell::new(b' '));
        }

        // 2. Draw Text part
        let snap_marker = if node.snapshot.is_some() { "*" } else { "" };
        let desc_str = format!(
            " [{}{}{}] {}",
            snap_marker, seq, snap_marker, node.transaction.description
        );

        let desc_color = if is_current {
            current_text_color
        } else {
            text_color
        };

        for ch in desc_str.chars() {
            final_row.push(Cell::from_char(ch).with_fg(desc_color));
        }

        lines.push(final_row);

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
