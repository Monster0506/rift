use super::layout::WindowLayout;
use super::tree::SplitTree;
use super::window::WindowId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl SplitTree {
    pub fn navigate(&self, direction: Direction, layouts: &[WindowLayout]) -> Option<WindowId> {
        let focused = layouts.iter().find(|l| l.window_id == self.focused_window)?;
        navigate_from(focused, direction, layouts)
    }
}

fn navigate_from(
    from: &WindowLayout,
    direction: Direction,
    layouts: &[WindowLayout],
) -> Option<WindowId> {
    let mut best: Option<(WindowId, usize)> = None;

    for candidate in layouts {
        if candidate.window_id == from.window_id {
            continue;
        }

        let adjacent = match direction {
            Direction::Left => {
                candidate.col + candidate.cols == from.col
                    || candidate.col + candidate.cols + 1 == from.col
            }
            Direction::Right => {
                from.col + from.cols == candidate.col
                    || from.col + from.cols + 1 == candidate.col
            }
            Direction::Up => {
                candidate.row + candidate.rows == from.row
                    || candidate.row + candidate.rows + 1 == from.row
            }
            Direction::Down => {
                from.row + from.rows == candidate.row
                    || from.row + from.rows + 1 == candidate.row
            }
        };

        if !adjacent {
            continue;
        }

        let overlap = match direction {
            Direction::Left | Direction::Right => {
                let start = from.row.max(candidate.row);
                let end = (from.row + from.rows).min(candidate.row + candidate.rows);
                end.saturating_sub(start)
            }
            Direction::Up | Direction::Down => {
                let start = from.col.max(candidate.col);
                let end = (from.col + from.cols).min(candidate.col + candidate.cols);
                end.saturating_sub(start)
            }
        };

        if overlap == 0 {
            continue;
        }

        if best.map_or(true, |(_, best_overlap)| overlap > best_overlap) {
            best = Some((candidate.window_id, overlap));
        }
    }

    best.map(|(id, _)| id)
}
