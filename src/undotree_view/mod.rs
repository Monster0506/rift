use crate::character::Character;
use crate::history::{EditSeq, UndoTree};
use crate::time::SystemTime;

fn format_age(timestamp: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(timestamp)
        .unwrap_or_default()
        .as_secs();
    if secs < 5 {
        "now".to_string()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else if secs < 30 * 86400 {
        format!("{}d", secs / 86400)
    } else {
        format!("{}mo", secs / (30 * 86400))
    }
}

pub fn render_tree(tree: &UndoTree) -> (Vec<Vec<crate::layer::Cell>>, Vec<EditSeq>, usize) {
    use crate::color::Color;
    use crate::layer::Cell;

    let mut lines = Vec::new();
    let mut sequences = Vec::new();
    let mut cursor_row = 0;

    let mut all_seqs: Vec<EditSeq> = tree.nodes.keys().cloned().collect();
    all_seqs.sort_by(|a, b| b.cmp(a)); // Descending

    let mut columns: Vec<Option<EditSeq>> = Vec::new();

    let node_color = Color::DarkYellow;
    let branch_color = Color::DarkRed;
    let text_color = Color::Grey;
    let current_text_color = Color::Magenta;
    let snap_text_color = Color::Cyan;
    let saved_color = Color::DarkGreen;

    for &seq in &all_seqs {
        let node = match tree.nodes.get(&seq) {
            Some(n) => n,
            None => continue,
        };

        let is_current = seq == tree.current;
        let is_saved = seq == tree.saved_seq && seq != tree.root_seq;

        let mut col_indices: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, waiting_for)| **waiting_for == Some(seq))
            .map(|(i, _)| i)
            .collect();

        let is_tip = col_indices.is_empty();
        if is_tip {
            let slot = if let Some(idx) = columns.iter().position(|c| c.is_none()) {
                idx
            } else {
                columns.push(None);
                columns.len() - 1
            };
            col_indices.push(slot);
        }

        let main_col = col_indices[0];

        let max_col = columns.len();

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
                    Cell::new(Character::from(' '))
                };
                conn_line.push(cell);
                conn_line.push(Cell::new(Character::from(' ')));
            }
            lines.push(conn_line);
            sequences.push(EditSeq::MAX)
        }

        sequences.push(seq);
        let mut final_row: Vec<Cell> = Vec::new();

        let snap_marker = node.snapshot.is_some();

        const CURRENT_CHAR: char = '@';
        const NODE_CHAR: char = '*';
        const SNAPSHOT_CHAR: char = '#';
        const SAVED_CHAR: char = 'S';
        for (c, item) in columns.iter().enumerate().take(max_col) {
            let cell = if c == main_col {
                if is_current {
                    Cell::from_char(CURRENT_CHAR).with_fg(current_text_color)
                } else if is_saved {
                    Cell::from_char(SAVED_CHAR).with_fg(saved_color)
                } else if snap_marker {
                    Cell::from_char(SNAPSHOT_CHAR).with_fg(snap_text_color)
                } else {
                    Cell::from_char(NODE_CHAR).with_fg(node_color)
                }
            } else if col_indices.contains(&c) {
                Cell::new(Character::from(' '))
            } else if item.is_some() {
                Cell::from_char('│').with_fg(branch_color)
            } else {
                Cell::new(Character::from(' '))
            };
            final_row.push(cell);
            final_row.push(Cell::new(Character::from(' ')));
        }

        let desc_str = format!(
            " [{}] {} - {}",
            seq,
            format_age(node.timestamp),
            node.transaction.description
        );

        let desc_color = if is_current {
            current_text_color
        } else if is_saved {
            saved_color
        } else if snap_marker {
            snap_text_color
        } else {
            text_color
        };

        for ch in desc_str.chars() {
            final_row.push(Cell::from_char(ch).with_fg(desc_color));
        }

        if is_current {
            cursor_row = lines.len();
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

type Highlights = Vec<(std::ops::Range<usize>, crate::color::Color)>;
pub fn render_tree_to_text(tree: &UndoTree) -> (String, Vec<EditSeq>, Highlights) {
    let (lines, sequences, _cursor) = render_tree(tree);
    let mut text = String::new();
    let mut highlights: Highlights = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        for cell in line {
            let ch = cell.to_char();
            if let Some(color) = cell.fg {
                let start = text.len();
                let end = start + ch.len_utf8();
                if let Some(last) = highlights.last_mut() {
                    if last.1 == color && last.0.end == start {
                        last.0.end = end;
                    } else {
                        highlights.push((start..end, color));
                    }
                } else {
                    highlights.push((start..end, color));
                }
            }
            text.push(ch);
        }
        if i + 1 < lines.len() {
            text.push('\n');
        }
    }

    (text, sequences, highlights)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
