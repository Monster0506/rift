//! Undo Tree Visualization
//!
//! Renders the undo history as a vertical "git-graph" style tree.

use crate::history::{EditSeq, UndoTree};

/// Render the undo tree to a list of lines (chars) and a mapping of sequences
pub fn render_tree(tree: &UndoTree) -> (Vec<Vec<char>>, Vec<EditSeq>, usize) {
    let mut lines = Vec::new();
    let mut sequences = Vec::new();
    let mut cursor_row = 0;

    // 1. collect all nodes and sort descending (newest at top)
    let mut all_seqs: Vec<EditSeq> = tree.nodes.keys().cloned().collect();
    all_seqs.sort_by(|a, b| b.cmp(a)); // Descending

    // 2. Track columns: each slot contains the `parent_seq` that this column is "tracing" down to find.
    //    If None, the slot is empty/available.
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

        // Determine which column this node belongs to.
        // It belongs to a column if that column is waiting for `seq`.
        // If multiple columns are waiting for `seq`, it's a merge point.
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
            // But wait, we assign it to 'wait' for 'seq' temporarily so we can treat it same as others?
            // Or just pick a slot.
            // We set the "waiting" value to parent later.
            // For now, find a slot.
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

        // Draw the graph implementation is tricky line-by-line.
        // We need to render the state *before* updating columns for the next row?
        // Or render the transition?

        // Simpler approach for "railway":
        // 1. Draw connections from previous row (if merge)
        // 2. Draw current node marker
        // 3. Draw connections to next row (splits?)

        // Actually, splitting happens when a parent has distinct children.
        // But we are iterating children -> parent.
        // So a "Split" in time (1 parent -> 2 children) is seen as a "Merge" in our Top-Down view.
        // We see Child A (col 0) and Child B (col 1). Both want Parent P.
        // When we reach P, both col 0 and col 1 point to P.
        // We merge col 1 into col 0. render: `|/`

        // So we are handling "Merges of columns".

        // Render Logic:
        // Iterate through all columns to print grid.

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
                // This is a merging line (secondary child merging into parent)
                // We render a merge connector later?
                // For the "node line", we might just want to show the merge happening *below*?
                // Or *on* the line?
                // User example: `|/` is on a separate line?
                // `| o | [2]`
                // `| |/`
                // `| o   [1]`

                // If we want compact graph:
                // `| o`  <- Node 2 (col 1)
                // `|/ `  <- Merge
                // `o  `  <- Node 1 (col 0)

                // If we do compact, we render the merge on the SAME line as the node if possible?
                // No, merge usually implies movement.

                // Let's stick to simple:
                // If i is in col_indices (but not main), it effectively merges HERE.
                // So we can draw ` ` (space) or curve.
                // But typically we simply stop drawing the vertical bar for that column from this point on.
                // But we need to draw a horizontal/diagonal connector to `main_col`.

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

        // Drawing the merges (diagonal lines) is hard with just one pass.
        // Let's do a simplified version:
        // Just markers and vertical lines. Merge visualization is bonus.
        // User asked for:
        // | |/
        // | o [*1*]

        // This implies a dedicated "merge row" might be needed if columns shift.
        // But let's try to do it inline if we can or just skip fancy diagonals for MVP 1.

        // Let's settle on:
        // 1. Draw the node at `main_col`.
        // 2. Any other `col_indices` are "merging in".
        // 3. Update `columns`.

        // Update columns rule:
        // - `main_col` now waits for `node.parent`.
        // - Other `col_indices` become empty (None) - they merged.

        // Wait! If `node.parent` is ALREADY waited on by another column (from a third branch),
        // then `main_col` also merges into THAT column?
        // No, `columns` state reflects "waiting for".
        // If we set `columns[main_col] = node.parent`, and some `columns[k] == node.parent`,
        // then next time we visit `node.parent`, we will pick up both `main_col` and `k`.
        // So we defer the merge to when we visit the parent! Excellent.

        // So for THIS node:
        // Only `col_indices` that were waiting for THIS `seq` are merging here.
        // Indeed.

        // Render diagonals?
        // If `col_indices.len() > 1`, we have a merge.
        // We want to visually connect `col_indices[1..]` to `main_col`.
        // It's easier to do this on the line *above* the node (relative to text flow)?
        // No, we are printing top-down.
        // Previous line: `| |`
        // current line:  `| o` (Node 1)
        // If Node 1 was needed by Col 0 and Col 1.
        // Previous lines had Col 0 waiting for 1, Col 1 waiting for 1.
        // Now at 1. We put 1 at Col 0.
        // Col 1 stops.
        // We need a `/` connecting Col 1 to Col 0.
        // Ideally we insert a "connector row" BEFORE this node line.

        if col_indices.len() > 1 {
            // Insert connector row
            // For every col k:
            // if k == main_col: `|`
            // if k in col_indices (secondary): `/` pointing left to main_col?
            //   (Assuming secondary > main). If secondary < main, `\` pointing right.
            // else: normal `|` or empty.

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
            sequences.push(EditSeq::MAX); // Dummy seq for connector line? Or just skip logic in view.
                                          // Actually `sequences` aligns with `lines`.
                                          // `EditSeq::MAX` is safe-ish hack.
        }

        sequences.push(seq); // For the actual node line

        // Now finalize the Node Line string
        // We built `row_str` blindly above. Let's rebuild properly.
        let mut final_row = String::new();
        for c in 0..max_col {
            if c == main_col {
                if is_current {
                    final_row.push('@');
                } else {
                    final_row.push('o');
                }
            } else if col_indices.contains(&c) {
                // It was a merge source, on this line it disappears (connector row handled it)
                // Just space?
                // Actually if we drew connector row, the diagonal handled the transition.
                // So here it's empty.
                final_row.push(' ');
            } else if columns[c].is_some() {
                final_row.push('│');
            } else {
                final_row.push(' ');
            }
            final_row.push(' ');
        }

        // Append Description
        // [*1*] Checkpoint
        let snap_marker = if node.snapshot.is_some() { "*" } else { "" };
        let desc = format!(
            " [{}{}{}] {}",
            snap_marker, seq, snap_marker, node.transaction.description
        );
        final_row.push_str(&desc);

        lines.push(final_row.chars().collect());

        // Update Columns for next step
        // 1. `main_col` now waits for `node.parent`.
        columns[main_col] = node.parent;

        // 2. Other waiting cols are now free (merged).
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
