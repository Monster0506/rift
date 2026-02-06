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
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "Saved edit"));
    tree.nodes.insert(2, create_dummy_node(2, Some(1), "Latest"));

    tree.saved_seq = 1;
    tree.current = 2;

    let (lines, seqs, _cursor) = render_tree(&tree);

    // Find the line for seq=1 (the saved node)
    let saved_row = seqs.iter().position(|&s| s == 1).unwrap();
    let saved_line: String = lines[saved_row].iter().map(|c| c.to_char()).collect();
    assert!(saved_line.contains('S'), "Saved node should show 'S' marker");

    // Current node should still show '@'
    let current_row = seqs.iter().position(|&s| s == 2).unwrap();
    let current_line: String = lines[current_row].iter().map(|c| c.to_char()).collect();
    assert!(current_line.contains('@'), "Current node should show '@' marker");
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
    assert!(!line.contains('S'), "Current takes priority over saved marker");
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
    assert!(!root_line.contains('S'), "Root node should not show saved marker");
}
