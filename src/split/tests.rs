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

#[test]
fn hsplit_30_70_ratio() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    tree.resize_focused(SplitDirection::Horizontal, -0.2, &[]);

    let layouts = tree.compute_layout(21, 80);
    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();

    assert!(l1.rows < l2.rows);
    assert_eq!(l1.rows + 1 + l2.rows, 21);
}

#[test]
fn vsplit_60_40_ratio() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40);
    tree.resize_focused(SplitDirection::Vertical, 0.1, &[]);

    let layouts = tree.compute_layout(24, 81);
    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();

    assert!(l1.cols > l2.cols);
    assert_eq!(l1.cols + 1 + l2.cols, 81);
}

#[test]
fn layout_enforces_minimum_rows() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    tree.resize_focused(SplitDirection::Horizontal, 0.4, &[]);

    let layouts = tree.compute_layout(9, 80);
    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();

    assert!(l1.rows >= 3);
    assert!(l2.rows >= 3);
}

#[test]
fn layout_enforces_minimum_cols() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40);
    tree.resize_focused(SplitDirection::Vertical, 0.4, &[]);

    let layouts = tree.compute_layout(24, 25);
    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();

    assert!(l1.cols >= 10);
    assert!(l2.cols >= 10);
}

#[test]
fn layout_rows_cols_sum_to_total() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    for total_rows in [7, 11, 25, 50] {
        let layouts = tree.compute_layout(total_rows, 80);
        let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
        let l2 = layouts.iter().find(|l| l.window_id == w2).unwrap();
        assert_eq!(l1.rows + 1 + l2.rows, total_rows);
    }
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

#[test]
fn navigate_l_shaped_layout() {
    // w1 spans full width on top, w2 and w3 split the bottom
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w_bottom = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);
    let w3 = tree.split(SplitDirection::Vertical, w_bottom, 1, 12, 40);

    let layouts = tree.compute_layout(25, 81);

    tree.set_focus(w1);
    assert_eq!(tree.navigate(Direction::Down, &layouts), Some(w_bottom));
    assert_eq!(tree.navigate(Direction::Left, &layouts), None);
    assert_eq!(tree.navigate(Direction::Right, &layouts), None);

    tree.set_focus(w_bottom);
    assert_eq!(tree.navigate(Direction::Up, &layouts), Some(w1));
    assert_eq!(tree.navigate(Direction::Right, &layouts), Some(w3));

    tree.set_focus(w3);
    assert_eq!(tree.navigate(Direction::Up, &layouts), Some(w1));
    assert_eq!(tree.navigate(Direction::Left, &layouts), Some(w_bottom));
}

#[test]
fn resize_changes_ratio() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    let before = tree.compute_layout(25, 80);
    let l1_before = before.iter().find(|l| l.window_id == w1).unwrap().rows;

    tree.set_focus(w1);
    tree.resize_focused(SplitDirection::Horizontal, 0.1, &before);

    let after = tree.compute_layout(25, 80);
    let l1_after = after.iter().find(|l| l.window_id == w1).unwrap().rows;
    let l2_after = after.iter().find(|l| l.window_id == w2).unwrap().rows;

    assert!(l1_after > l1_before);
    assert_eq!(l1_after + 1 + l2_after, 25);
}

#[test]
fn resize_clamps_at_bounds() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let _w2 = tree.split(SplitDirection::Horizontal, w1, 1, 12, 80);

    tree.set_focus(w1);
    for _ in 0..20 {
        tree.resize_focused(SplitDirection::Horizontal, 0.1, &[]);
    }

    let layouts = tree.compute_layout(25, 80);
    let l1 = layouts.iter().find(|l| l.window_id == w1).unwrap();
    let _l2 = layouts.iter().find(|l| l.window_id != w1).unwrap();

    assert!(l1.rows >= 3);
    assert!(_l2.rows >= 3);
}

// ============================================================
// Previous window tracking
// ============================================================

#[test]
fn set_focus_tracks_previous() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 2, 12, 80);

    assert!(tree.set_focus(w2));
    assert_eq!(tree.previous_window, Some(w1));
    assert_eq!(tree.focused_window_id(), w2);
}

#[test]
fn focus_previous_toggles() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 2, 12, 80);

    tree.set_focus(w2);
    assert_eq!(tree.focus_previous(), Some(w1));
    assert_eq!(tree.focused_window_id(), w1);

    // Next call goes back to w2
    assert_eq!(tree.focus_previous(), Some(w2));
    assert_eq!(tree.focused_window_id(), w2);
}

#[test]
fn focus_previous_returns_none_when_no_history() {
    let mut tree = SplitTree::new(1, 24, 80);
    assert_eq!(tree.focus_previous(), None);
}

// ============================================================
// Window exchange
// ============================================================

#[test]
fn exchange_windows_swaps_documents() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 2, 12, 80);

    tree.get_window_mut(w1).unwrap().cursor_position = 10;
    tree.get_window_mut(w2).unwrap().cursor_position = 20;

    assert!(tree.exchange_windows(w1, w2));

    assert_eq!(tree.get_window(w1).unwrap().document_id, 2);
    assert_eq!(tree.get_window(w1).unwrap().cursor_position, 20);
    assert_eq!(tree.get_window(w2).unwrap().document_id, 1);
    assert_eq!(tree.get_window(w2).unwrap().cursor_position, 10);
}

#[test]
fn exchange_windows_same_id_returns_false() {
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    assert!(!tree.exchange_windows(w1, w1));
}

// ============================================================
// Window move
// ============================================================

#[test]
fn move_window_single_window_returns_false() {
    let mut tree = SplitTree::new(1, 24, 80);
    let layouts = tree.compute_layout(24, 80);
    assert!(!tree.move_window(Direction::Right, &layouts));
}

// --- no-neighbor: flip parent direction ---

#[test]
fn move_window_no_neighbor_flips_parent_direction() {
    // HSplit(w1, w2(focused)); w2 has no right neighbor.
    // ^WL should flip parent to VSplit, w2 stays second → w2 on right.
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 2, 12, 80);

    tree.set_focus(w2);
    let layouts = tree.compute_layout(25, 80);
    assert!(tree.move_window(Direction::Right, &layouts));

    assert_eq!(tree.window_count(), 2);
    assert_eq!(tree.focused_window_id(), w2);

    let new_layouts = tree.compute_layout(24, 81);
    let l1 = new_layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = new_layouts.iter().find(|l| l.window_id == w2).unwrap();
    assert!(
        l2.col > l1.col,
        "w2 should be to the right of w1 after flip"
    );
}

#[test]
fn move_window_no_neighbor_flip_left_swaps_children() {
    // HSplit(w1, w2(focused)); w2 has no left neighbor.
    // ^WH flips parent to VSplit, w2 should move to first (left) → children swapped.
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 2, 12, 80);

    tree.set_focus(w2);
    let layouts = tree.compute_layout(25, 80);
    assert!(tree.move_window(Direction::Left, &layouts));

    let new_layouts = tree.compute_layout(24, 81);
    let l1 = new_layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = new_layouts.iter().find(|l| l.window_id == w2).unwrap();
    assert!(
        l2.col < l1.col,
        "w2 should be to the left of w1 after flip+swap"
    );
}

#[test]
fn move_window_no_neighbor_preserves_ratio() {
    // Ratio should be unchanged after a direction flip.
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Horizontal, w1, 2, 12, 80);
    tree.resize_focused(SplitDirection::Horizontal, 0.2, &[]);

    tree.set_focus(w2);
    let layouts = tree.compute_layout(25, 80);

    // Capture w2's fraction of total before move
    let l2_before = layouts.iter().find(|l| l.window_id == w2).unwrap().rows;
    let total_before = 25usize;

    tree.move_window(Direction::Right, &layouts);

    let new_layouts = tree.compute_layout(24, 81);
    let l1_after = new_layouts.iter().find(|l| l.window_id == w1).unwrap().cols;
    let l2_after = new_layouts.iter().find(|l| l.window_id == w2).unwrap().cols;
    // The ratio in the tree is preserved: the second child's share should match
    // what it was before (within rounding).
    let ratio_before = l2_before as f64 / total_before as f64;
    let ratio_after = l2_after as f64 / (l1_after + 1 + l2_after) as f64;
    assert!(
        (ratio_before - ratio_after).abs() < 0.05,
        "ratio should be approximately preserved"
    );
}

// --- has-neighbor: re-insert adjacent ---

#[test]
fn move_window_toward_neighbor() {
    // VSplit(w1(focused), w2); w1 presses Right → w1 lands to the right of w2.
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40);

    let layouts = tree.compute_layout(24, 81);
    assert!(tree.move_window(Direction::Right, &layouts));

    assert_eq!(tree.window_count(), 2);
    let new_layouts = tree.compute_layout(24, 81);
    let l1 = new_layouts.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = new_layouts.iter().find(|l| l.window_id == w2).unwrap();
    assert!(l1.col > l2.col, "w1 should now be to the right of w2");
}

#[test]
fn move_window_preserves_size_when_has_neighbor() {
    // VSplit(w1=40cols, w2=40cols); move w1 right → new VSplit(w2, w1).
    // w1 should still occupy ~40 cols (half) in the new split.
    let mut tree = SplitTree::new(1, 24, 80);
    let w1 = tree.focused_window_id();
    let _w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40);

    let layouts = tree.compute_layout(24, 81);
    let w1_cols_before = layouts.iter().find(|l| l.window_id == w1).unwrap().cols;

    tree.move_window(Direction::Right, &layouts);

    let new_layouts = tree.compute_layout(24, 81);
    let w1_cols_after = new_layouts.iter().find(|l| l.window_id == w1).unwrap().cols;
    // Allow ±2 cols tolerance for separator and rounding
    assert!(
        (w1_cols_after as i32 - w1_cols_before as i32).abs() <= 2,
        "w1 cols before={w1_cols_before} after={w1_cols_after}"
    );
}

// Helper: build H(V(w1,w2), w3) and return (tree, w1, w2, w3).
// w1=top-left, w2=top-right, w3=full-width bottom.
fn three_window_tree() -> (
    SplitTree,
    super::window::WindowId,
    super::window::WindowId,
    super::window::WindowId,
) {
    let mut tree = SplitTree::new(1, 48, 80);
    let w1 = tree.focused_window_id();
    let w3 = tree.split(SplitDirection::Horizontal, w1, 3, 24, 80); // H(w1, w3)
    let w2 = tree.split(SplitDirection::Vertical, w1, 2, 24, 40); // H(V(w1,w2), w3)
    (tree, w1, w2, w3)
}

#[test]
fn demo3_left_neighbor_swap() {
    // H(V([w1],w2), w3) + Right → H(V(w2,[w1]), w3).
    // w1 moves right: now to the right of w2. w3 unchanged at bottom.
    let (mut tree, w1, w2, w3) = three_window_tree();
    tree.set_focus(w1);
    let layouts = tree.compute_layout(48, 80);
    assert!(tree.move_window(Direction::Right, &layouts));

    let nl = tree.compute_layout(48, 80);
    let l1 = nl.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = nl.iter().find(|l| l.window_id == w2).unwrap();
    let l3 = nl.iter().find(|l| l.window_id == w3).unwrap();
    assert_eq!(l1.row, l2.row, "w1 and w2 still share top row");
    assert!(l1.col > l2.col, "w1 is now right of w2");
    assert!(l3.row > l1.row, "w3 still at bottom");
    assert_eq!(l3.col, 0, "w3 left edge unchanged");
}

#[test]
fn demo8_up_joins_top_row_between() {
    // H(V(w1,w2), [w3]) + Up → V(V(w1,[w3]), w2).
    // w3 (full-width bottom) moves up, lands between w1 and w2.
    // After: all 3 in one row; w3 between w1 (left) and w2 (right).
    let (mut tree, w1, w2, w3) = three_window_tree();
    tree.set_focus(w3);
    let layouts = tree.compute_layout(48, 80);
    assert!(tree.move_window(Direction::Up, &layouts));

    let nl = tree.compute_layout(48, 80);
    let l1 = nl.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = nl.iter().find(|l| l.window_id == w2).unwrap();
    let l3 = nl.iter().find(|l| l.window_id == w3).unwrap();
    assert_eq!(l1.row, l3.row, "w3 is in the same row as w1");
    assert_eq!(l1.row, l2.row, "all three share one row");
    assert!(l3.col > l1.col, "w3 is right of w1");
    assert!(l3.col < l2.col, "w3 is left of w2");
}

#[test]
fn demo16_down_moves_to_bottom_row() {
    // H(V(w1,[w2]), w3) + Down → H(w1, V(w3,[w2])).
    // w2 (top-right) moves down; w1 expands to full top; w2 lands right of w3.
    let (mut tree, w1, w2, w3) = three_window_tree();
    tree.set_focus(w2);
    let layouts = tree.compute_layout(48, 80);
    assert!(tree.move_window(Direction::Down, &layouts));

    let nl = tree.compute_layout(48, 80);
    let l1 = nl.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = nl.iter().find(|l| l.window_id == w2).unwrap();
    let l3 = nl.iter().find(|l| l.window_id == w3).unwrap();
    assert!(l2.row > l1.row, "w2 is now below w1");
    assert_eq!(l2.row, l3.row, "w2 and w3 share bottom row");
    assert!(l2.col > l3.col, "w2 is right of w3");
    assert_eq!(l1.col, 0, "w1 left edge at 0");
    // w1 now spans the full top (no sibling in its row)
    assert!(l1.cols > l2.cols, "w1 is wider than w2 (w1 is full top)");
}

#[test]
fn demo17_down_moves_to_bottom_row_left() {
    // H(V([w1],w2), w3) + Down → H(w2, V(w3,[w1])).
    // w1 (top-left) moves down; w2 expands to full top; w1 lands right of w3.
    let (mut tree, w1, w2, w3) = three_window_tree();
    tree.set_focus(w1);
    let layouts = tree.compute_layout(48, 80);
    assert!(tree.move_window(Direction::Down, &layouts));

    let nl = tree.compute_layout(48, 80);
    let l1 = nl.iter().find(|l| l.window_id == w1).unwrap();
    let l2 = nl.iter().find(|l| l.window_id == w2).unwrap();
    let l3 = nl.iter().find(|l| l.window_id == w3).unwrap();
    assert!(l1.row > l2.row, "w1 is now below w2");
    assert_eq!(l1.row, l3.row, "w1 and w3 share bottom row");
    assert!(l1.col > l3.col, "w1 is right of w3");
    assert!(l2.cols > l1.cols, "w2 is wider than w1 (w2 is full top)");
}
