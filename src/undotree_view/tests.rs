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

    // Assertions logic would be complex on strings, but visual inspect in stdout is key.
    // Check Tip 5 produces @
    let l0: String = lines[0].iter().map(|c| c.to_char()).collect();
    assert!(l0.contains("@"));
    assert!(l0.contains("Msg5"));
}
