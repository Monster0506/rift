use super::*;
use crate::command_line::commands::SplitSubcommand;
use crate::split::navigation::Direction;
use crate::split::tree::SplitDirection;
use crate::test_utils::MockTerminal;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn process_jobs(editor: &mut Editor<MockTerminal>) {
    use std::time::Duration;
    // Use recv_timeout so we actually wait for the async file-load thread to finish.
    // 100 ms is far more than needed for a tiny temp file but keeps tests fast.
    loop {
        match editor.job_manager.receiver().recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => { editor.handle_job_message(msg).unwrap(); }
            Err(_) => break,
        }
    }
}

fn render_ascii(editor: &mut Editor<MockTerminal>) -> String {
    editor.update_and_render().unwrap();
    let rows = editor.render_system.compositor.rows();
    let cols = editor.render_system.compositor.cols();
    let cells = editor.render_system.compositor.get_composited_slice();
    (0..rows)
        .map(|r| (0..cols).map(|c| cells[r * cols + c].to_char()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn move_window(editor: &mut Editor<MockTerminal>, dir: Direction) {
    editor.do_split_window(SplitDirection::Vertical, SplitSubcommand::Move(dir));
    editor.update_and_render().unwrap();
}

/// Returns the column of the leftmost occurrence of `needle` across all rows.
/// Searches content rows only (skips status bar / last 2 rows).
fn col_of(screen: &str, needle: &str) -> Option<usize> {
    let total_rows: Vec<&str> = screen.lines().collect();
    let content_rows = total_rows.len().saturating_sub(2);
    for line in &total_rows[..content_rows] {
        if let Some(pos) = line.find(needle) {
            return Some(pos);
        }
    }
    None
}

/// Returns all rows (content only, not status bar) that contain `needle`.
fn rows_containing(screen: &str, needle: &str) -> Vec<usize> {
    let all: Vec<&str> = screen.lines().collect();
    let content_rows = all.len().saturating_sub(2);
    all[..content_rows]
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains(needle))
        .map(|(i, _)| i)
        .collect()
}

/// Create three temp files with distinct single-word content.
/// Returns (tempdir_guard, path_a, path_b, path_c).
fn make_files() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = dir.path().join("win_a.txt");
    let b = dir.path().join("win_b.txt");
    let c = dir.path().join("win_c.txt");
    std::fs::write(&a, "WINDOW_A\n").unwrap();
    std::fs::write(&b, "WINDOW_B\n").unwrap();
    std::fs::write(&c, "WINDOW_C\n").unwrap();
    (dir, a, b, c)
}

/// Build the three-pane layout H(V(A, B), C):
///
///   ┌──────────┬──────────┐
///   │ WINDOW_A │ WINDOW_B │  ← top half
///   ├──────────┴──────────┤
///   │       WINDOW_C      │  ← bottom half (full width)
///   └─────────────────────┘
///
/// Returns (editor, win_a, win_b, win_c).
fn setup(
    path_a: &std::path::Path,
    path_b: &std::path::Path,
    path_c: &std::path::Path,
) -> (
    Editor<MockTerminal>,
    crate::split::window::WindowId,
    crate::split::window::WindowId,
    crate::split::window::WindowId,
) {
    // 80 cols, 50 rows — status bar takes 2 rows, leaving 48 content rows
    let term = MockTerminal::new(50, 80);
    let mut editor = Editor::with_file(term, Some(path_a.to_str().unwrap().to_string()))
        .expect("editor init");
    process_jobs(&mut editor);

    let win_a = editor.split_tree.focused_window_id();

    // Horizontal split → C fills the bottom full-width
    editor.do_split_window(
        SplitDirection::Horizontal,
        SplitSubcommand::File(path_c.to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);
    let win_c = editor.split_tree.focused_window_id();

    // Refocus A, then vertical split → B is top-right
    editor.split_tree.set_focus(win_a);
    editor.do_split_window(
        SplitDirection::Vertical,
        SplitSubcommand::File(path_b.to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);
    let win_b = editor.split_tree.focused_window_id();

    editor.update_and_render().unwrap();

    (editor, win_a, win_b, win_c)
}

// ─── screenshot helper ────────────────────────────────────────────────────────

fn print_screen(label: &str, screen: &str) {
    eprintln!("\n=== {label} ===");
    for (i, line) in screen.lines().enumerate() {
        eprintln!("{i:2}: {line}");
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// Verify the baseline layout renders A top-left, B top-right, C bottom.
#[test]
fn baseline_layout_renders_correctly() {
    let (_dir, pa, pb, pc) = make_files();
    let (mut editor, _wa, _wb, _wc) = setup(&pa, &pb, &pc);

    let screen = render_ascii(&mut editor);
    print_screen("baseline", &screen);

    let col_a = col_of(&screen, "WINDOW_A").expect("WINDOW_A not found");
    let col_b = col_of(&screen, "WINDOW_B").expect("WINDOW_B not found");
    let col_c = col_of(&screen, "WINDOW_C").expect("WINDOW_C not found");

    assert!(col_a < 40, "A should be in the left half (col={col_a})");
    assert!(col_b >= 40, "B should be in the right half (col={col_b})\n{screen}");
    assert!(col_c < 5, "C should start at the left edge (col={col_c})");

    let rows_a = rows_containing(&screen, "WINDOW_A");
    let rows_c = rows_containing(&screen, "WINDOW_C");
    assert!(!rows_a.is_empty());
    assert!(!rows_c.is_empty());
    assert!(
        rows_c[0] > rows_a[0],
        "C should be below A (row_a={}, row_c={})",
        rows_a[0],
        rows_c[0]
    );
}

/// Demo 3: A (top-left) presses ^WL → A and B swap.
/// Before: │ A │ B │ / │   C   │
/// After:  │ B │ A │ / │   C   │
#[test]
fn demo3_wl_on_a_swaps_with_b() {
    let (_dir, pa, pb, pc) = make_files();
    let (mut editor, win_a, _win_b, _win_c) = setup(&pa, &pb, &pc);

    editor.split_tree.set_focus(win_a);
    let before = render_ascii(&mut editor);
    print_screen("demo3 BEFORE ^WL on A", &before);

    move_window(&mut editor, Direction::Right);

    let after = render_ascii(&mut editor);
    print_screen("demo3 AFTER ^WL on A", &after);

    let col_a = col_of(&after, "WINDOW_A").expect("WINDOW_A missing after move");
    let col_b = col_of(&after, "WINDOW_B").expect("WINDOW_B missing after move");
    let col_c = col_of(&after, "WINDOW_C").expect("WINDOW_C missing after move");

    assert!(col_a > col_b, "A should now be RIGHT of B (col_a={col_a}, col_b={col_b})\n{after}");
    assert!(col_c < 5, "C should still be at left edge (col={col_c})\n{after}");

    let rows_a = rows_containing(&after, "WINDOW_A");
    let rows_c = rows_containing(&after, "WINDOW_C");
    assert!(rows_c[0] > rows_a[0], "C still below A\n{after}");
}

/// Demo 4: B (top-right) presses ^WH → B and A swap.
/// Before: │ A │ B │ / │   C   │
/// After:  │ B │ A │ / │   C   │
#[test]
fn demo4_wh_on_b_swaps_with_a() {
    let (_dir, pa, pb, pc) = make_files();
    let (mut editor, _win_a, win_b, _win_c) = setup(&pa, &pb, &pc);

    editor.split_tree.set_focus(win_b);
    move_window(&mut editor, Direction::Left);

    let after = render_ascii(&mut editor);
    print_screen("demo4 AFTER ^WH on B", &after);

    let col_a = col_of(&after, "WINDOW_A").expect("WINDOW_A missing");
    let col_b = col_of(&after, "WINDOW_B").expect("WINDOW_B missing");

    assert!(col_b < col_a, "B should now be LEFT of A (col_b={col_b}, col_a={col_a})\n{after}");
}

/// Demo 5: B (top-right) presses ^WL → B escapes to right edge.
/// Before: │ A │ B │ / │   C   │
/// After:  │ A │   │   │   B   │
///         ├───┤   │   └───────┘
///         │ C │   │
///         └───┘   │
/// i.e. B takes the right column full-height; A and C stack on the left.
#[test]
fn demo5_wl_on_b_escapes_right() {
    let (_dir, pa, pb, pc) = make_files();
    let (mut editor, _win_a, win_b, _win_c) = setup(&pa, &pb, &pc);

    editor.split_tree.set_focus(win_b);
    move_window(&mut editor, Direction::Right);

    let after = render_ascii(&mut editor);
    print_screen("demo5 AFTER ^WL on B (escape right)", &after);

    let col_a = col_of(&after, "WINDOW_A").expect("WINDOW_A missing");
    let col_b = col_of(&after, "WINDOW_B").expect("WINDOW_B missing");
    let col_c = col_of(&after, "WINDOW_C").expect("WINDOW_C missing");

    assert!(col_b > col_a, "B should be to the right of A\n{after}");
    assert!(col_b > col_c, "B should be to the right of C\n{after}");

    // A and C should now share the same column (both in the left section)
    assert!((col_a as isize - col_c as isize).abs() < 5,
        "A and C should be in roughly the same column (col_a={col_a}, col_c={col_c})\n{after}");
}

/// Demo 8: C (bottom, full-width) presses ^WK → C joins the top row between A and B.
/// Before: │ A │ B │ / │   C   │
/// After:  │ A │ C │ B │   (no bottom row)
#[test]
fn demo8_wk_on_c_joins_top_row() {
    let (_dir, pa, pb, pc) = make_files();
    let (mut editor, _win_a, _win_b, win_c) = setup(&pa, &pb, &pc);

    editor.split_tree.set_focus(win_c);
    move_window(&mut editor, Direction::Up);

    let after = render_ascii(&mut editor);
    print_screen("demo8 AFTER ^WK on C (joins top row)", &after);

    let col_a = col_of(&after, "WINDOW_A").expect("WINDOW_A missing");
    let col_b = col_of(&after, "WINDOW_B").expect("WINDOW_B missing");
    let col_c = col_of(&after, "WINDOW_C").expect("WINDOW_C missing");

    // All three should be in the SAME row
    let row_a = rows_containing(&after, "WINDOW_A")[0];
    let row_b = rows_containing(&after, "WINDOW_B")[0];
    let row_c = rows_containing(&after, "WINDOW_C")[0];
    assert_eq!(row_a, row_c, "A and C should be in the same row\n{after}");
    assert_eq!(row_b, row_c, "B and C should be in the same row\n{after}");

    // Order: A left, C middle, B right
    assert!(col_a < col_c, "A should be left of C\n{after}");
    assert!(col_c < col_b, "C should be left of B\n{after}");
}

/// Demo 16: B (top-right) presses ^WJ → A expands to full top; C and B in bottom row.
/// Before: │ A │ B │ / │   C   │
/// After:  │     A     │
///         ├─────┬─────┤
///         │  C  │  B  │
#[test]
fn demo16_wj_on_b_joins_bottom_row() {
    let (_dir, pa, pb, pc) = make_files();
    let (mut editor, _win_a, win_b, _win_c) = setup(&pa, &pb, &pc);

    editor.split_tree.set_focus(win_b);
    move_window(&mut editor, Direction::Down);

    let after = render_ascii(&mut editor);
    print_screen("demo16 AFTER ^WJ on B", &after);

    let col_b = col_of(&after, "WINDOW_B").expect("WINDOW_B missing");
    let col_c = col_of(&after, "WINDOW_C").expect("WINDOW_C missing");

    let row_a = rows_containing(&after, "WINDOW_A");
    let row_b = rows_containing(&after, "WINDOW_B");
    let row_c = rows_containing(&after, "WINDOW_C");

    // B and C should be in the same row (bottom)
    assert_eq!(row_b[0], row_c[0], "B and C should share the bottom row\n{after}");
    // A should be above B and C
    assert!(row_a[0] < row_b[0], "A should be above B\n{after}");
    // B should be right of C
    assert!(col_b > col_c, "B should be right of C (col_b={col_b}, col_c={col_c})\n{after}");
}

/// Demo 17: A (top-left) presses ^WJ → B expands to full top; C and A in bottom row.
/// Before: │ A │ B │ / │   C   │
/// After:  │     B     │
///         ├─────┬─────┤
///         │  C  │  A  │
#[test]
fn demo17_wj_on_a_joins_bottom_row() {
    let (_dir, pa, pb, pc) = make_files();
    let (mut editor, win_a, _win_b, _win_c) = setup(&pa, &pb, &pc);

    editor.split_tree.set_focus(win_a);
    move_window(&mut editor, Direction::Down);

    let after = render_ascii(&mut editor);
    print_screen("demo17 AFTER ^WJ on A", &after);

    let col_a = col_of(&after, "WINDOW_A").expect("WINDOW_A missing");
    let col_c = col_of(&after, "WINDOW_C").expect("WINDOW_C missing");

    let row_a = rows_containing(&after, "WINDOW_A");
    let row_b = rows_containing(&after, "WINDOW_B");
    let row_c = rows_containing(&after, "WINDOW_C");

    // A and C should be in the same row (bottom)
    assert_eq!(row_a[0], row_c[0], "A and C should share the bottom row\n{after}");
    // B should be above A and C
    assert!(row_b[0] < row_a[0], "B should be above A\n{after}");
    // A should be right of C
    assert!(col_a > col_c, "A should be right of C (col_a={col_a}, col_c={col_c})\n{after}");
}

/// A fourth file: open D in a fourth pane, then move it around.
/// Layout: H(V(A,B), V(C,D)) — 4 panes.
/// Then move A down: B expands top-left; A joins bottom row between C and D.
#[test]
fn four_pane_move_a_down() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pa = dir.path().join("a.txt");
    let pb = dir.path().join("b.txt");
    let pc = dir.path().join("c.txt");
    let pd = dir.path().join("d.txt");
    std::fs::write(&pa, "WINDOW_A\n").unwrap();
    std::fs::write(&pb, "WINDOW_B\n").unwrap();
    std::fs::write(&pc, "WINDOW_C\n").unwrap();
    std::fs::write(&pd, "WINDOW_D\n").unwrap();

    let term = MockTerminal::new(50, 80);
    let mut editor = Editor::with_file(term, Some(pa.to_str().unwrap().to_string()))
        .expect("editor init");
    process_jobs(&mut editor);
    let win_a = editor.split_tree.focused_window_id();

    // Bottom horizontal split → C
    editor.do_split_window(
        SplitDirection::Horizontal,
        SplitSubcommand::File(pc.to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);
    let win_c = editor.split_tree.focused_window_id();

    // D is a vertical split of C
    editor.do_split_window(
        SplitDirection::Vertical,
        SplitSubcommand::File(pd.to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);

    // B is a vertical split of A
    editor.split_tree.set_focus(win_a);
    editor.do_split_window(
        SplitDirection::Vertical,
        SplitSubcommand::File(pb.to_str().unwrap().to_string()),
    );
    process_jobs(&mut editor);

    editor.update_and_render().unwrap();

    let baseline = render_ascii(&mut editor);
    print_screen("4-pane baseline (H(V(A,B), V(C,D)))", &baseline);

    // Focus A and move down
    editor.split_tree.set_focus(win_a);
    move_window(&mut editor, Direction::Down);

    let after = render_ascii(&mut editor);
    print_screen("4-pane AFTER ^WJ on A", &after);

    let col_a = col_of(&after, "WINDOW_A").expect("WINDOW_A missing");
    let col_c = col_of(&after, "WINDOW_C").expect("WINDOW_C missing");

    let row_a = rows_containing(&after, "WINDOW_A");
    let row_b = rows_containing(&after, "WINDOW_B");
    let row_c = rows_containing(&after, "WINDOW_C");

    // A should now be in the bottom section (below B)
    assert!(row_a[0] > row_b[0], "A should be below B after moving down\n{after}");
    // A and C should be in the same bottom row
    assert_eq!(row_a[0], row_c[0], "A and C should share a row\n{after}");
    // A should be to the right of C (inserted after C)
    assert!(col_a > col_c, "A should be right of C\n{after}");

    // Verify C is still in the bottom section
    assert!(
        row_c[0] > row_b[0],
        "C should still be below B (win_c is in bottom)\n{after}"
    );

    let _ = win_c; // suppress unused warning
}

/// Three windows side by side: | A | B | C |
/// A presses ^WL → A and B swap: | B | A | C |
#[test]
fn three_horizontal_wl_on_a_swaps_with_b() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pa = dir.path().join("a.txt");
    let pb = dir.path().join("b.txt");
    let pc = dir.path().join("c.txt");
    std::fs::write(&pa, "WINDOW_A\n").unwrap();
    std::fs::write(&pb, "WINDOW_B\n").unwrap();
    std::fs::write(&pc, "WINDOW_C\n").unwrap();

    let term = MockTerminal::new(50, 80);
    let mut editor = Editor::with_file(term, Some(pa.to_str().unwrap().to_string()))
        .expect("editor init");
    process_jobs(&mut editor);
    let win_a = editor.split_tree.focused_window_id();

    editor.do_split_window(SplitDirection::Vertical,
        SplitSubcommand::File(pb.to_str().unwrap().to_string()));
    process_jobs(&mut editor);

    editor.do_split_window(SplitDirection::Vertical,
        SplitSubcommand::File(pc.to_str().unwrap().to_string()));
    process_jobs(&mut editor);

    editor.update_and_render().unwrap();
    let baseline = render_ascii(&mut editor);
    print_screen("three-horiz baseline | A | B | C |", &baseline);

    // All windows should be on the same row
    let row_a_before = rows_containing(&baseline, "WINDOW_A");
    let row_b_before = rows_containing(&baseline, "WINDOW_B");
    let row_c_before = rows_containing(&baseline, "WINDOW_C");
    assert!(!row_a_before.is_empty(), "WINDOW_A missing from baseline");
    assert!(!row_b_before.is_empty(), "WINDOW_B missing from baseline");
    assert!(!row_c_before.is_empty(), "WINDOW_C missing from baseline");
    assert_eq!(row_a_before[0], row_b_before[0], "A and B should be on same row in baseline");
    assert_eq!(row_b_before[0], row_c_before[0], "B and C should be on same row in baseline");
    let col_a_before = col_of(&baseline, "WINDOW_A").unwrap();
    let col_b_before = col_of(&baseline, "WINDOW_B").unwrap();
    let col_c_before = col_of(&baseline, "WINDOW_C").unwrap();
    assert!(col_a_before < col_b_before, "A should be left of B in baseline");
    assert!(col_b_before < col_c_before, "B should be left of C in baseline");

    // Move A right
    editor.split_tree.set_focus(win_a);
    move_window(&mut editor, Direction::Right);

    let after = render_ascii(&mut editor);
    print_screen("three-horiz AFTER ^WL on A → | B | A | C |", &after);

    let col_a = col_of(&after, "WINDOW_A").expect("WINDOW_A missing after move");
    let col_b = col_of(&after, "WINDOW_B").expect("WINDOW_B missing after move");
    let col_c = col_of(&after, "WINDOW_C").expect("WINDOW_C missing after move");

    assert!(col_b < col_a, "B should be left of A (col_b={col_b}, col_a={col_a})\n{after}");
    assert!(col_a < col_c, "A should be left of C (col_a={col_a}, col_c={col_c})\n{after}");
}
