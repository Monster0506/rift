use super::*;
use crate::history::{EditNode, EditSeq, EditTransaction, UndoTree};

fn create_dummy_node(seq: EditSeq, parent: Option<EditSeq>, desc: &str) -> EditNode {
    EditNode {
        seq,
        parent,
        children: Vec::new(),
        transaction: EditTransaction {
            ops: Vec::new(),
            description: desc.to_string(),
        },
        snapshot: None,
        timestamp: std::time::SystemTime::UNIX_EPOCH,
        last_visited_child: None,
    }
}

#[test]
fn test_git_graph_render() {
    let mut tree = UndoTree::new();
    // Reset and clear for manual construction (UndoTree::new creates root 0)
    tree.nodes.clear();

    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "Original"));
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "Msg1"));
    tree.nodes.insert(2, create_dummy_node(2, Some(1), "Msg2"));
    tree.nodes.insert(3, create_dummy_node(3, Some(1), "Msg3"));
    tree.nodes.insert(4, create_dummy_node(4, Some(3), "Msg4"));
    tree.nodes.insert(5, create_dummy_node(5, Some(4), "Msg5"));

    tree.current = 5;

    let (lines, _seqs, _cursor) = render_tree(&tree);

    println!("Rendered Tree:");
    for line in &lines {
        let line_str: String = line.iter().map(|c| c.to_char()).collect();
        println!("{}", line_str);
    }

    let l0: String = lines[0].iter().map(|c| c.to_char()).collect();
    assert!(l0.contains("@"));
    assert!(l0.contains("Msg5"));
}

#[test]
fn test_cursor_position_on_merge() {
    let mut tree = UndoTree::new();
    tree.nodes.clear();
    tree.root_seq = 0;

    // Create a scenario where Node 1 has two children (2 and 3), so when rendering Node 1 in descending order,
    // we encounter a merge point (cols for 2 and 3 both pointing to 1).
    tree.nodes.insert(0, create_dummy_node(0, None, "Original"));
    tree.nodes
        .insert(1, create_dummy_node(1, Some(0), "Branch Point"));
    tree.nodes
        .insert(2, create_dummy_node(2, Some(1), "Child A"));
    tree.nodes
        .insert(3, create_dummy_node(3, Some(1), "Child B"));

    // Set current to Branch Point (Seq 1)
    tree.current = 1;

    let (lines, _, cursor_row) = render_tree(&tree);

    // In descending order:
    // 3: Tip
    // 2: Tip
    // 1: Merge of 3 and 2. Should produce a connector line, THEN the node line.
    // 0: Parent of 1

    // If there is a connector line, Node 1 will be on a later line.
    // The cursor should point to Node 1's line (containing "@"), NOT the connector line.

    let cursor_line_str: String = lines[cursor_row].iter().map(|c| c.to_char()).collect();

    // Debug print
    println!("Cursor Row: {}", cursor_row);
    println!("Cursor Line Content: '{}'", cursor_line_str);
    for (i, line) in lines.iter().enumerate() {
        let s: String = line.iter().map(|c| c.to_char()).collect();
        println!("{}: {}", i, s);
    }

    assert!(
        cursor_line_str.contains("@"),
        "Cursor row should contain current node marker '@'"
    );
    assert!(
        cursor_line_str.contains("Branch Point"),
        "Cursor row should contain node description"
    );
}

#[test]
fn test_saved_node_marker() {
    let mut tree = UndoTree::new();
    tree.nodes.clear();

    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "Original"));
    tree.nodes
        .insert(1, create_dummy_node(1, Some(0), "Saved edit"));
    tree.nodes
        .insert(2, create_dummy_node(2, Some(1), "Latest"));

    tree.saved_seq = 1;
    tree.current = 2;

    let (lines, seqs, _cursor) = render_tree(&tree);

    // Find the line for seq=1 (the saved node)
    let saved_row = seqs.iter().position(|&s| s == 1).unwrap();
    let saved_line: String = lines[saved_row].iter().map(|c| c.to_char()).collect();
    assert!(
        saved_line.contains('S'),
        "Saved node should show 'S' marker"
    );

    // Current node should still show '@'
    let current_row = seqs.iter().position(|&s| s == 2).unwrap();
    let current_line: String = lines[current_row].iter().map(|c| c.to_char()).collect();
    assert!(
        current_line.contains('@'),
        "Current node should show '@' marker"
    );
}

#[test]
fn test_saved_current_node_shows_current_marker() {
    let mut tree = UndoTree::new();
    tree.nodes.clear();

    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "Original"));
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "Edit"));

    // Node is both saved and current — current wins
    tree.saved_seq = 1;
    tree.current = 1;

    let (lines, seqs, _cursor) = render_tree(&tree);

    let row = seqs.iter().position(|&s| s == 1).unwrap();
    let line: String = lines[row].iter().map(|c| c.to_char()).collect();
    assert!(line.contains('@'), "Current+saved node should show '@'");
    assert!(
        !line.contains('S'),
        "Current takes priority over saved marker"
    );
}

#[test]
fn test_root_saved_no_marker() {
    let mut tree = UndoTree::new();
    tree.nodes.clear();

    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "Original"));
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "Edit"));

    // saved_seq == root_seq — should not show 'S' on root
    tree.saved_seq = 0;
    tree.current = 1;

    let (lines, seqs, _cursor) = render_tree(&tree);

    let root_row = seqs.iter().position(|&s| s == 0).unwrap();
    let root_line: String = lines[root_row].iter().map(|c| c.to_char()).collect();
    assert!(
        !root_line.contains('S'),
        "Root node should not show saved marker"
    );
}
// render_tree_to_text tests

fn simple_linear_tree() -> UndoTree {
    let mut tree = UndoTree::new();
    tree.nodes.clear();
    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "root"));
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "edit1"));
    tree.current = 1;
    tree
}

#[test]
fn test_render_tree_to_text_returns_string() {
    let tree = simple_linear_tree();
    let (text, _seqs, _hl) = render_tree_to_text(&tree);
    assert!(!text.is_empty(), "rendered text must not be empty");
}

#[test]
fn test_render_tree_to_text_contains_node_description() {
    let tree = simple_linear_tree();
    let (text, _seqs, _hl) = render_tree_to_text(&tree);
    assert!(text.contains("edit1"), "text should contain node description");
    assert!(text.contains("root"), "text should contain root description");
}

#[test]
fn test_render_tree_to_text_current_node_has_at_marker() {
    let tree = simple_linear_tree();
    let (text, seqs, _hl) = render_tree_to_text(&tree);
    let current_line_idx = seqs.iter().position(|&s| s == 1).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[current_line_idx].contains('@'),
            "current node line must have '@': {:?}", lines[current_line_idx]);
}

#[test]
fn test_render_tree_to_text_sequences_match_lines() {
    let tree = simple_linear_tree();
    let (text, seqs, _hl) = render_tree_to_text(&tree);
    let line_count = text.lines().count();
    assert_eq!(seqs.len(), line_count, "sequences must have one entry per line");
}

#[test]
fn test_render_tree_to_text_highlights_non_empty() {
    let tree = simple_linear_tree();
    let (_text, _seqs, highlights) = render_tree_to_text(&tree);
    assert!(!highlights.is_empty(), "must have at least some colored characters");
}

#[test]
fn test_render_tree_to_text_highlights_have_valid_ranges() {
    let tree = simple_linear_tree();
    let (text, _seqs, highlights) = render_tree_to_text(&tree);
    for (range, _color) in &highlights {
        assert!(range.start <= range.end, "range start must be <= end");
        assert!(range.end <= text.len(), "range must not exceed text length");
    }
}

#[test]
fn test_render_tree_to_text_highlights_sorted_and_non_overlapping() {
    let mut tree = UndoTree::new();
    tree.nodes.clear();
    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "root"));
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "a"));
    tree.nodes.insert(2, create_dummy_node(2, Some(1), "b"));
    tree.nodes.insert(3, create_dummy_node(3, Some(1), "c"));
    tree.current = 2;

    let (_text, _seqs, highlights) = render_tree_to_text(&tree);
    for i in 0..highlights.len().saturating_sub(1) {
        assert!(highlights[i].0.end <= highlights[i+1].0.start,
                "highlight ranges must be sorted and non-overlapping");
    }
}

#[test]
fn test_render_tree_to_text_connector_lines_have_max_seq() {
    // A branching tree produces connector lines with EditSeq::MAX
    let mut tree = UndoTree::new();
    tree.nodes.clear();
    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "root"));
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "branch_a"));
    tree.nodes.insert(2, create_dummy_node(2, Some(0), "branch_b"));
    tree.current = 1;

    let (_text, seqs, _hl) = render_tree_to_text(&tree);
    // With 2 branches from root, there should be at least one connector line (MAX sentinel)
    let has_connector = seqs.iter().any(|&s| s == EditSeq::MAX);
    assert!(has_connector, "branching tree must produce connector lines with MAX sentinel");
}

#[test]
fn test_render_tree_to_text_contiguous_same_color_merged() {
    // Any node whose entire description is the same color should produce
    // a single merged highlight range rather than one-per-char ranges.
    let tree = simple_linear_tree();
    let (text, _seqs, highlights) = render_tree_to_text(&tree);

    // Check that there are no adjacent highlight entries with the same color and contiguous ranges
    for i in 0..highlights.len().saturating_sub(1) {
        let (r1, c1) = &highlights[i];
        let (r2, c2) = &highlights[i + 1];
        if c1 == c2 && r1.end == r2.start {
            panic!(
                "adjacent highlights with same color {:?} should have been merged: {:?} and {:?} in text {:?}",
                c1, r1, r2, text
            );
        }
    }
}

#[test]
fn test_render_tree_to_text_current_node_color() {
    let tree = simple_linear_tree();
    let (text, seqs, highlights) = render_tree_to_text(&tree);
    // Find offset of the '@' character (current node marker)
    let current_line_idx = seqs.iter().position(|&s| s == 1).unwrap();
    let line_start: usize = text.lines().take(current_line_idx).map(|l| l.len() + 1).sum();
    let at_offset = text[line_start..].find('@').map(|o| line_start + o).unwrap();

    // The '@' character should be colored (Magenta for current node)
    let colored = highlights.iter().any(|(r, _c)| r.start <= at_offset && r.end > at_offset);
    assert!(colored, "'@' marker at offset {} should be colored", at_offset);
}

#[test]
fn test_render_tree_to_text_seq_ids_in_text() {
    let tree = simple_linear_tree();
    let (text, _seqs, _hl) = render_tree_to_text(&tree);
    // "[1]" and "[0]" should appear in the text
    assert!(text.contains("[1]"), "seq 1 label should appear in text");
    assert!(text.contains("[0]"), "seq 0 label should appear in text");
}

#[test]
fn test_render_tree_to_text_single_root_node() {
    let mut tree = UndoTree::new();
    tree.nodes.clear();
    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "root"));
    tree.current = 0;

    let (text, seqs, highlights) = render_tree_to_text(&tree);
    assert!(!text.is_empty());
    assert_eq!(seqs.len(), text.lines().count());
    assert!(!highlights.is_empty());
}
