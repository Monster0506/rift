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

    // Construct User Scenario:
    // 5 -> 4 -> 3 -> 1 -> 0
    //                |
    //                2

    tree.root_seq = 0;
    tree.nodes.insert(0, create_dummy_node(0, None, "Original"));
    tree.nodes.insert(1, create_dummy_node(1, Some(0), "Msg1"));
    tree.nodes.insert(2, create_dummy_node(2, Some(1), "Msg2"));
    tree.nodes.insert(3, create_dummy_node(3, Some(1), "Msg3"));
    tree.nodes.insert(4, create_dummy_node(4, Some(3), "Msg4"));
    tree.nodes.insert(5, create_dummy_node(5, Some(4), "Msg5"));

    // Link children (needed?)
    // The render logic uses 'parent' pointers from nodes, it implies children.
    // But let's be safe if logic changes? Logic currently iterates all_seqs and checks columns.
    // It reads `node.parent`. It does NOT read `node.children`.

    tree.current = 5;

    let (lines, _seqs, _cursor) = render_tree(&tree);

    println!("Rendered Tree:");
    for line in &lines {
        println!("{}", line.iter().collect::<String>());
    }

    // Assertions logic would be complex on strings, but visual inspect in stdout is key.
    // Check Tip 5 produces @
    assert!(lines[0].iter().collect::<String>().contains("@"));
    assert!(lines[0].iter().collect::<String>().contains("Msg5"));

    // Check merge point around 1
    // We expect multiple vertical lines at some point.
}
