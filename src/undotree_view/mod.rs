use crate::character::Character;
use crate::color::Color;
use crate::history::{EditSeq, UndoTree};
use crate::layer::Cell;
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

const CURRENT_CHAR: char = '@';
const NODE_CHAR: char = '*';
const SNAPSHOT_CHAR: char = '#';
const SAVED_CHAR: char = 'S';
const NODE_COLOR: Color = Color::DarkYellow;
const BRANCH_COLOR: Color = Color::DarkRed;
const TEXT_COLOR: Color = Color::Grey;
const CURRENT_TEXT_COLOR: Color = Color::Magenta;
const SNAP_TEXT_COLOR: Color = Color::Cyan;
const SAVED_COLOR: Color = Color::DarkGreen;

#[derive(Clone, Copy)]
struct Status {
    current: bool,
    saved: bool,
    snapshot: bool,
}

impl Status {
    const fn marker(self) -> (char, Color) {
        if self.current {
            (CURRENT_CHAR, CURRENT_TEXT_COLOR)
        } else if self.saved {
            (SAVED_CHAR, SAVED_COLOR)
        } else if self.snapshot {
            (SNAPSHOT_CHAR, SNAP_TEXT_COLOR)
        } else {
            (NODE_CHAR, NODE_COLOR)
        }
    }

    const fn text_color(self) -> Color {
        if self.current {
            CURRENT_TEXT_COLOR
        } else if self.saved {
            SAVED_COLOR
        } else if self.snapshot {
            SNAP_TEXT_COLOR
        } else {
            TEXT_COLOR
        }
    }
}

fn text_cells(text: &str, color: Color) -> Vec<Cell> {
    text.chars()
        .map(|ch| Cell::from_char(ch).with_fg(color))
        .collect()
}

// Column lanes and output rows shared across the tree walk.
#[derive(Default)]
struct TreeRows {
    lines: Vec<Vec<Cell>>,
    sequences: Vec<EditSeq>,
    cursor_row: usize,
    columns: Vec<Option<EditSeq>>,
}

impl TreeRows {
    // Lanes waiting on seq; a tip with none claims a free or new lane.
    fn claim_columns(&mut self, seq: EditSeq) -> Vec<usize> {
        let mut col_indices: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, waiting_for)| **waiting_for == Some(seq))
            .map(|(i, _)| i)
            .collect();
        if col_indices.is_empty() {
            let free = self.columns.iter().position(std::option::Option::is_none);
            let slot = if let Some(idx) = free {
                idx
            } else {
                self.columns.push(None);
                self.columns.len() - 1
            };
            col_indices.push(slot);
        }
        col_indices
    }

    fn push_connector(&mut self, col_indices: &[usize], main_col: usize) {
        let mut conn_line: Vec<Cell> = Vec::new();
        for (c, item) in self.columns.iter().enumerate() {
            let cell = if c == main_col {
                Cell::from_char('│').with_fg(BRANCH_COLOR)
            } else if col_indices.contains(&c) {
                if c > main_col {
                    Cell::from_char('/').with_fg(BRANCH_COLOR)
                } else {
                    Cell::from_char('\\').with_fg(BRANCH_COLOR)
                }
            } else if item.is_some() {
                Cell::from_char('│').with_fg(BRANCH_COLOR)
            } else {
                Cell::new(Character::from(' '))
            };
            conn_line.push(cell);
            conn_line.push(Cell::new(Character::from(' ')));
        }
        self.lines.push(conn_line);
        self.sequences.push(EditSeq::MAX);
    }

    fn node_cells(&self, col_indices: &[usize], main_col: usize, status: Status) -> Vec<Cell> {
        let (marker, marker_color) = status.marker();
        let mut row: Vec<Cell> = Vec::new();
        for (c, item) in self.columns.iter().enumerate() {
            let cell = if c == main_col {
                Cell::from_char(marker).with_fg(marker_color)
            } else if col_indices.contains(&c) {
                Cell::new(Character::from(' '))
            } else if item.is_some() {
                Cell::from_char('│').with_fg(BRANCH_COLOR)
            } else {
                Cell::new(Character::from(' '))
            };
            row.push(cell);
            row.push(Cell::new(Character::from(' ')));
        }
        row
    }

    fn advance(&mut self, main_col: usize, col_indices: &[usize], parent: Option<EditSeq>) {
        self.columns[main_col] = parent;
        for &idx in col_indices {
            if idx != main_col {
                self.columns[idx] = None;
            }
        }
    }
}

#[must_use]
// One layout pass; the column state is shared across every row.
fn render_tree(tree: &UndoTree) -> (Vec<Vec<crate::layer::Cell>>, Vec<EditSeq>, usize) {
    let mut all_seqs: Vec<EditSeq> = tree.nodes.keys().copied().collect();
    all_seqs.sort_by(|a, b| b.cmp(a)); // Descending

    let mut rows = TreeRows::default();
    for &seq in &all_seqs {
        let Some(node) = tree.nodes.get(&seq) else {
            continue;
        };
        let status = Status {
            current: seq == tree.current,
            saved: seq == tree.saved_seq && seq != tree.root_seq,
            snapshot: node.snapshot.is_some(),
        };

        let col_indices = rows.claim_columns(seq);
        let main_col = col_indices[0];
        if col_indices.len() > 1 {
            rows.push_connector(&col_indices, main_col);
        }

        rows.sequences.push(seq);
        let mut final_row = rows.node_cells(&col_indices, main_col, status);
        let desc_str = format!(
            " [{}] {} - {}",
            seq,
            format_age(node.timestamp),
            node.transaction.description
        );
        final_row.extend(text_cells(&desc_str, status.text_color()));

        if status.current {
            rows.cursor_row = rows.lines.len();
        }
        rows.lines.push(final_row);
        rows.advance(main_col, &col_indices, node.parent);
    }

    (rows.lines, rows.sequences, rows.cursor_row)
}

type Highlights = Vec<(std::ops::Range<usize>, crate::color::Color)>;
#[must_use]
pub(crate) fn render_tree_to_text(tree: &UndoTree) -> (String, Vec<EditSeq>, Highlights) {
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
