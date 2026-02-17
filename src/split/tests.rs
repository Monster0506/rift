use super::layout::WindowLayout;
use super::navigation::Direction;
use super::tree::{SplitDirection, SplitTree};

// ============================================================
// Tree construction & manipulation
// ============================================================

#[test]
fn single_window_tree() {
    let tree = SplitTree::new(1, 24, 80);
    assert_eq!(tree.window_count(), 1);
    assert_eq!(tree.focused_window().document_id, 1);
    assert_eq!(tree.focused_window().cursor_position, 0);
    assert!(!tree.focused_window().frozen);
}

#[test]
fn split_horizontal_creates_two_windows() {
    let mut tree = SplitTree::new(1, 24, 80);
    let focused = tree.focused_window_id();
    let new_id = tree.split(SplitDirection::Horizontal, focused, 1, 12, 80);

    assert_eq!(tree.window_count(), 2);
    assert!(tree.get_window(focused).is_some());
    assert!(tree.get_window(new_id).is_some());
}

#[test]
fn split_vertical_creates_two_windows() {
    let mut tree = SplitTree::new(1, 24, 80);
    let focused = tree.focused_window_id();
    let new_id = tree.split(SplitDirection::Vertical, focused, 2, 24, 40);

    assert_eq!(tree.window_count(), 2);
    assert_eq!(tree.get_window(new_id).unwrap().document_id, 2);
}

#[test]
fn split_three_windows() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    let w3 = tree.split(SplitDirection::Vertical, w2, 2, 12, 40);

    assert_eq!(tree.window_count(), 3);
    assert!(tree.get_window(w1).is_some());
    assert!(tree.get_window(w2).is_some());
    assert!(tree.get_window(w3).is_some());
}

#[test]
fn close_window_reduces_count() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    assert_eq!(tree.window_count(), 2);
    assert!(tree.close_window(w2));
    assert_eq!(tree.window_count(), 1);
}

#[test]
fn close_last_window_returns_false() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    assert!(!tree.close_window(w1));
    assert_eq!(tree.window_count(), 1);
}

#[test]
fn close_focused_window_moves_focus() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let _w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    tree.set_focus(w1);
    assert!(tree.close_window(w1));
    assert_ne!(tree.focused_window_id(), w1);
    assert_eq!(tree.window_count(), 1);
}

#[test]
fn close_middle_window_in_three() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    let w3 = tree.split(SplitDirection::Horizontal, w2, 1, 6, 80);

    assert_eq!(tree.window_count(), 3);
    assert!(tree.close_window(w2));
    assert_eq!(tree.window_count(), 2);
    assert!(tree.get_window(w1).is_some());
    assert!(tree.get_window(w3).is_some());
}

// ============================================================
// Focus management
// ============================================================

#[test]
fn set_focus_works() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    assert_eq!(tree.focused_window_id(), w1);
    assert!(tree.set_focus(w2));
    assert_eq!(tree.focused_window_id(), w2);
}

#[test]
fn set_focus_invalid_id_returns_false() {
    let mut tree = SplitTree::new(1, 24, 80);
    assert!(!tree.set_focus(999));
}

// ============================================================
// Document queries
// ============================================================

#[test]
fn windows_for_document() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    let _w3 = tree.split(SplitDirection::Vertical, w2, 2, 12, 40);

    let mut doc1_windows = tree.windows_for_document(1);
    doc1_windows.sort();
    assert_eq!(doc1_windows.len(), 2);
    assert_eq!(tree.windows_for_document(2).len(), 1);
    assert_eq!(tree.windows_for_document(999).len(), 0);
}

#[test]
fn all_window_ids_returns_all() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    let w3 = tree.split(SplitDirection::Vertical, w2, 2, 12, 40);

    let mut ids = tree.all_window_ids();
    ids.sort();
    assert_eq!(ids, vec![w1, w2, w3]);
}

// ============================================================
// Per-window cursor
// ============================================================

#[test]
fn new_split_copies_cursor_for_same_doc() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();

    tree.get_window_mut(w1).unwrap().cursor_position = 42;

    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    assert_eq!(tree.get_window(w2).unwrap().cursor_position, 42);
}

#[test]
fn new_split_zero_cursor_for_different_doc() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();

    tree.get_window_mut(w1).unwrap().cursor_position = 42;

    let w2 = tree.split(SplitDirection::Horizontal, w1, 2, 12, 80);
    assert_eq!(tree.get_window(w2).unwrap().cursor_position, 0);
}

#[test]
fn independent_cursor_positions() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    tree.get_window_mut(w1).unwrap().cursor_position = 10;
    tree.get_window_mut(w2).unwrap().cursor_position = 50;

    assert_eq!(tree.get_window(w1).unwrap().cursor_position, 10);
    assert_eq!(tree.get_window(w2).unwrap().cursor_position, 50);
}

// ============================================================
// Freeze
// ============================================================

#[test]
fn freeze_flag() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    assert!(!tree.focused_window().frozen);

    tree.get_window_mut(w1).unwrap().frozen = true;
    assert!(tree.focused_window().frozen);

    tree.get_window_mut(w1).unwrap().frozen = false;
    assert!(!tree.focused_window().frozen);
}

// ============================================================
// Layout computation
// ============================================================

#[test]
fn single_window_full_screen() {
    let tree = SplitTree::new(1, 24, 80);
    let layouts = tree.compute_layout(24, 80);
    assert_eq!(layouts.len(), 1);
    assert_eq!(
        layouts[0],
        WindowLayout {
            window_id: 1,
            row: 0,
            col: 0,
            rows: 24,
            cols: 80,
        }
    );
}

#[test]
fn two_windows_hsplit_50() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    let layouts = tree.compute_layout(25, 80);
    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();

    assert_eq!(l1.row, 0);
    assert_eq!(l1.rows, 12);
    assert_eq!(l1.cols, 80);
    assert_eq!(l2.row, 13);
    assert_eq!(l2.rows, 12);
    assert_eq!(l2.cols, 80);
}

#[test]
fn two_windows_vsplit_50() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40);

    let layouts = tree.compute_layout(24, 81);
    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();

    assert_eq!(l1.col, 0);
    assert_eq!(l1.cols, 40);
    assert_eq!(l1.rows, 24);
    assert_eq!(l2.col, 41);
    assert_eq!(l2.cols, 40);
    assert_eq!(l2.rows, 24);
}

#[test]
fn three_windows_nested_layout() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40);
    let w3 = tree.split(SplitDirection::Horizontal, w2, 3, 12, 40);

    let layouts = tree.compute_layout(24, 81);
    assert_eq!(layouts.len(), 3);

    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();
    let l3 = layouts.iter().find(|l| l.window_id == w3).unwrap();

    assert_eq!(l1.col, 0);
    assert_eq!(l1.cols, 40);
    assert_eq!(l1.rows, 24);

    assert_eq!(l2.col, 41);
    assert_eq!(l3.col, 41);
    assert_eq!(l2.cols, 40);
    assert_eq!(l3.cols, 40);
    assert!(l2.row < l3.row);
}

// ============================================================
// Navigation
// ============================================================

#[test]
fn navigate_hsplit_up_down() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    let layouts = tree.compute_layout(25, 80);

    tree.set_focus(w1);
    assert_eq!(tree.navigate(Direction::Down, &layouts), Some(w2));
    assert_eq!(tree.navigate(Direction::Left, &layouts), None);
    assert_eq!(tree.navigate(Direction::Right, &layouts), None);

    tree.set_focus(w2);
    assert_eq!(tree.navigate(Direction::Up, &layouts), Some(w1));
    assert_eq!(tree.navigate(Direction::Down, &layouts), None);
}

#[test]
fn navigate_vsplit_left_right() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40);

    let layouts = tree.compute_layout(24, 81);

    tree.set_focus(w1);
    assert_eq!(tree.navigate(Direction::Right, &layouts), Some(w2));
    assert_eq!(tree.navigate(Direction::Up, &layouts), None);
    assert_eq!(tree.navigate(Direction::Down, &layouts), None);

    tree.set_focus(w2);
    assert_eq!(tree.navigate(Direction::Left, &layouts), Some(w1));
    assert_eq!(tree.navigate(Direction::Right, &layouts), None);
}

#[test]
fn navigate_four_windows() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w_bottom = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    let w2 = tree.split(SplitDirection::Vertical, w1, 1, 12, 40);
    let w4 = tree.split(SplitDirection::Vertical, w_bottom, 1, 12, 40);

    let layouts = tree.compute_layout(25, 81);

    tree.set_focus(w1);
    assert_eq!(tree.navigate(Direction::Right, &layouts), Some(w2));
    assert_eq!(tree.navigate(Direction::Down, &layouts), Some(w_bottom));
    assert_eq!(tree.navigate(Direction::Left, &layouts), None);
    assert_eq!(tree.navigate(Direction::Up, &layouts), None);

    tree.set_focus(w2);
    assert_eq!(tree.navigate(Direction::Left, &layouts), Some(w1));
    assert_eq!(tree.navigate(Direction::Down, &layouts), Some(w4));
}

// ============================================================
// Resize
// ============================================================

#[test]
fn resize_hsplit() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let _w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    let layouts = tree.compute_layout(25, 80);

    tree.set_focus(w1);
    let result = tree.resize_focused(SplitDirection::Horizontal, 0.1, &layouts);
    assert!(result);
}

#[test]
fn resize_wrong_direction_returns_false() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let _w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    let layouts = tree.compute_layout(25, 80);

    tree.set_focus(w1);
    let result = tree.resize_focused(SplitDirection::Vertical, 0.1, &layouts);
    assert!(!result);
}
