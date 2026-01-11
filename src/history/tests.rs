use super::*;

// =============================================================================
// Position and Range Tests
// =============================================================================

#[test]
fn test_position_new() {
    let pos = Position::new(10, 5);
    assert_eq!(pos.line, 10);
    assert_eq!(pos.col, 5);
}

#[test]
fn test_range_is_empty() {
    let pos = Position::new(1, 1);
    let empty_range = Range::new(pos, pos);
    assert!(empty_range.is_empty());

    let non_empty = Range::new(Position::new(1, 1), Position::new(1, 5));
    assert!(!non_empty.is_empty());
}

// =============================================================================
// EditOperation Tests
// =============================================================================

#[test]
fn test_insert_operation_inverse() {
    let insert = EditOperation::Insert {
        position: Position::new(0, 0),
        text: "hello".to_string(),
        len: 5,
    };

    let inverse = insert.inverse();
    match inverse {
        EditOperation::Delete {
            range,
            deleted_text,
        } => {
            assert_eq!(range.start, Position::new(0, 0));
            assert_eq!(deleted_text, "hello");
        }
        _ => panic!("Expected Delete operation"),
    }
}

#[test]
fn test_delete_operation_inverse() {
    let delete = EditOperation::Delete {
        range: Range::new(Position::new(0, 0), Position::new(0, 5)),
        deleted_text: "hello".to_string(),
    };

    let inverse = delete.inverse();
    match inverse {
        EditOperation::Insert {
            position,
            text,
            len,
        } => {
            assert_eq!(position, Position::new(0, 0));
            assert_eq!(text, "hello");
            assert_eq!(len, 5);
        }
        _ => panic!("Expected Insert operation"),
    }
}

#[test]
fn test_replace_operation_inverse() {
    let replace = EditOperation::Replace {
        range: Range::new(Position::new(0, 0), Position::new(0, 5)),
        old_text: "hello".to_string(),
        new_text: "world".to_string(),
    };

    let inverse = replace.inverse();
    match inverse {
        EditOperation::Replace {
            old_text, new_text, ..
        } => {
            assert_eq!(old_text, "world");
            assert_eq!(new_text, "hello");
        }
        _ => panic!("Expected Replace operation"),
    }
}

#[test]
fn test_operation_estimated_size() {
    let insert = EditOperation::Insert {
        position: Position::new(0, 0),
        text: "hello".to_string(),
        len: 5,
    };
    assert!(insert.estimated_size() > 5);

    let delete = EditOperation::Delete {
        range: Range::new(Position::new(0, 0), Position::new(0, 10)),
        deleted_text: "0123456789".to_string(),
    };
    assert!(delete.estimated_size() > 10);
}

#[test]
fn test_operation_description() {
    let insert = EditOperation::Insert {
        position: Position::new(0, 0),
        text: "hi".to_string(),
        len: 2,
    };
    assert!(insert.description().contains("Insert"));

    let delete = EditOperation::Delete {
        range: Range::new(Position::new(0, 0), Position::new(0, 2)),
        deleted_text: "hi".to_string(),
    };
    assert!(delete.description().contains("Delete"));
}

// =============================================================================
// EditTransaction Tests
// =============================================================================

#[test]
fn test_transaction_new() {
    let tx = EditTransaction::new("Test transaction");
    assert!(tx.is_empty());
    assert_eq!(tx.description, "Test transaction");
}

#[test]
fn test_transaction_record() {
    let mut tx = EditTransaction::new("Insert hello");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "hello".to_string(),
        len: 5,
    });

    assert!(!tx.is_empty());
    assert_eq!(tx.ops.len(), 1);
}

#[test]
fn test_transaction_inverse() {
    let mut tx = EditTransaction::new("Multiple ops");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tx.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "b".to_string(),
        len: 1,
    });

    let inverse = tx.inverse();

    // Should be in reverse order
    assert_eq!(inverse.len(), 2);
    // First inverse should be for "b" (last op)
    match &inverse[0] {
        EditOperation::Delete { deleted_text, .. } => assert_eq!(deleted_text, "b"),
        _ => panic!("Expected Delete"),
    }
    // Second inverse should be for "a" (first op)
    match &inverse[1] {
        EditOperation::Delete { deleted_text, .. } => assert_eq!(deleted_text, "a"),
        _ => panic!("Expected Delete"),
    }
}

// =============================================================================
// UndoTree Tests
// =============================================================================

#[test]
fn test_undo_tree_new() {
    let tree = UndoTree::new();
    assert_eq!(tree.current_seq(), 0);
    assert!(!tree.can_undo());
    assert!(!tree.can_redo());
}

#[test]
fn test_undo_tree_push() {
    let mut tree = UndoTree::new();

    let mut tx = EditTransaction::new("Insert hello");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "hello".to_string(),
        len: 5,
    });

    let seq = tree.push(tx, None);
    assert_eq!(seq, 1);
    assert_eq!(tree.current_seq(), 1);
    assert!(tree.can_undo());
    assert!(!tree.can_redo());
}

#[test]
fn test_undo_tree_basic_undo_redo() {
    let mut tree = UndoTree::new();

    // Push first edit
    let mut tx1 = EditTransaction::new("Insert a");
    tx1.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx1, None);

    // Push second edit
    let mut tx2 = EditTransaction::new("Insert b");
    tx2.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "b".to_string(),
        len: 1,
    });
    tree.push(tx2, None);

    assert_eq!(tree.current_seq(), 2);

    // Undo
    assert!(tree.can_undo());
    tree.undo();
    assert_eq!(tree.current_seq(), 1);

    // Undo again
    tree.undo();
    assert_eq!(tree.current_seq(), 0);
    assert!(!tree.can_undo());

    // Redo
    assert!(tree.can_redo());
    tree.redo();
    assert_eq!(tree.current_seq(), 1);

    // Redo again
    tree.redo();
    assert_eq!(tree.current_seq(), 2);
    assert!(!tree.can_redo());
}

#[test]
fn test_undo_tree_branching() {
    let mut tree = UndoTree::new();

    // Push first edit
    let mut tx1 = EditTransaction::new("Insert a");
    tx1.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx1, None);

    // Push second edit
    let mut tx2 = EditTransaction::new("Insert b");
    tx2.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "b".to_string(),
        len: 1,
    });
    tree.push(tx2, None);

    // Undo back to seq=1
    tree.undo();
    assert_eq!(tree.current_seq(), 1);

    // Push different edit (creates branch)
    let mut tx3 = EditTransaction::new("Insert c");
    tx3.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "c".to_string(),
        len: 1,
    });
    tree.push(tx3, None);

    // Should be at seq=3 now
    assert_eq!(tree.current_seq(), 3);

    // Node at seq=1 should have 2 children
    assert_eq!(tree.branch_count(), 0); // Current node (3) has no children

    // Undo to seq=1
    tree.undo();
    assert_eq!(tree.current_seq(), 1);
    assert_eq!(tree.branch_count(), 2); // Node 1 has 2 children (2 and 3)
}

#[test]
fn test_undo_tree_goto_branch() {
    let mut tree = UndoTree::new();

    // Create branching structure
    let mut tx1 = EditTransaction::new("Base");
    tx1.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "base".to_string(),
        len: 4,
    });
    tree.push(tx1, None); // seq=1

    let mut tx2 = EditTransaction::new("Branch A");
    tx2.record(EditOperation::Insert {
        position: Position::new(0, 4),
        text: "A".to_string(),
        len: 1,
    });
    tree.push(tx2, None); // seq=2

    tree.undo(); // Back to seq=1

    let mut tx3 = EditTransaction::new("Branch B");
    tx3.record(EditOperation::Insert {
        position: Position::new(0, 4),
        text: "B".to_string(),
        len: 1,
    });
    tree.push(tx3, None); // seq=3

    tree.undo(); // Back to seq=1

    // Now at seq=1 with children [2, 3], last_visited_child=1 (pointing to seq=3)
    // Redo should go to seq=3
    tree.redo();
    assert_eq!(tree.current_seq(), 3);

    tree.undo(); // Back to seq=1

    // Switch to branch 0
    tree.goto_branch(0).unwrap();
    tree.redo();
    assert_eq!(tree.current_seq(), 2);
}

#[test]
fn test_undo_tree_clear() {
    let mut tree = UndoTree::new();

    for i in 0..5 {
        let mut tx = EditTransaction::new(format!("Edit {}", i));
        tx.record(EditOperation::Insert {
            position: Position::new(0, i as u32),
            text: i.to_string(),
            len: 1,
        });
        tree.push(tx, None);
    }

    assert_eq!(tree.current_seq(), 5);

    tree.clear();

    assert_eq!(tree.current_seq(), 0);
    assert!(!tree.can_undo());
    assert!(!tree.can_redo());
}

// =============================================================================
// DocumentSnapshot Tests
// =============================================================================

#[test]
fn test_document_snapshot() {
    let snap = DocumentSnapshot::new("Hello\nWorld\n".to_string());
    assert_eq!(snap.byte_count, 12);
    assert_eq!(snap.line_count, 3); // "Hello", "World", ""
}

// =============================================================================
// Additional UndoTree Tests
// =============================================================================

#[test]
fn test_undo_tree_memory_tracking() {
    let mut tree = UndoTree::new();
    let initial_memory = tree.memory_usage();

    let mut tx = EditTransaction::new("Large insert");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".repeat(1000),
        len: 1000,
    });
    tree.push(tx, None);

    assert!(tree.memory_usage() > initial_memory);
}

#[test]
fn test_undo_tree_multiple_undo_redo_cycles() {
    let mut tree = UndoTree::new();

    // Push 5 edits
    for i in 0..5 {
        let mut tx = EditTransaction::new(format!("Edit {}", i));
        tx.record(EditOperation::Insert {
            position: Position::new(0, i as u32),
            text: format!("{}", i),
            len: 1,
        });
        tree.push(tx, None);
    }

    // Undo all
    for expected in (0..5).rev() {
        assert!(tree.can_undo());
        tree.undo();
        assert_eq!(tree.current_seq(), expected);
    }

    // Redo all
    for expected in 1..=5 {
        assert!(tree.can_redo());
        tree.redo();
        assert_eq!(tree.current_seq(), expected);
    }

    // Undo 3, then push new edit (creates branch)
    tree.undo();
    tree.undo();
    tree.undo();
    assert_eq!(tree.current_seq(), 2);

    let mut tx = EditTransaction::new("Branch edit");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "X".to_string(),
        len: 1,
    });
    tree.push(tx, None);
    assert_eq!(tree.current_seq(), 6);

    // Should still be able to undo
    assert!(tree.can_undo());
}

#[test]
fn test_undo_tree_cannot_redo_after_new_edit() {
    let mut tree = UndoTree::new();

    let mut tx1 = EditTransaction::new("First");
    tx1.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx1, None);

    let mut tx2 = EditTransaction::new("Second");
    tx2.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "b".to_string(),
        len: 1,
    });
    tree.push(tx2, None);

    // Undo once
    tree.undo();
    assert!(tree.can_redo());

    // Push new edit - redo should still work (it's a branch, not lost)
    let mut tx3 = EditTransaction::new("New branch");
    tx3.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "c".to_string(),
        len: 1,
    });
    tree.push(tx3, None);

    // Cannot redo from current position (at leaf)
    assert!(!tree.can_redo());

    // But the branch still exists
    tree.undo(); // Back to seq=1
    assert_eq!(tree.branch_count(), 2);
}

#[test]
fn test_undo_tree_empty_transaction_not_pushed() {
    let mut tree = UndoTree::new();

    let tx = EditTransaction::new("Empty");
    // Don't record any operations

    // Push empty transaction - it should still be pushed (tree doesn't check)
    tree.push(tx, None);
    assert_eq!(tree.current_seq(), 1);
}

#[test]
fn test_undo_tree_goto_branch_invalid() {
    let mut tree = UndoTree::new();

    let mut tx = EditTransaction::new("Edit");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx, None);

    // Try to goto invalid branch
    let result = tree.goto_branch(5);
    assert!(result.is_err());
}

#[test]
fn test_undo_tree_deep_branching() {
    let mut tree = UndoTree::new();

    // Create initial edit
    let mut tx = EditTransaction::new("Root");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "root".to_string(),
        len: 4,
    });
    tree.push(tx, None); // seq=1

    // Create 5 branches from root
    for i in 0..5 {
        tree.undo(); // Back to seq=0

        if tree.can_redo() {
            tree.redo();
        } else {
            // Re-push the root
            let mut tx = EditTransaction::new("Root");
            tx.record(EditOperation::Insert {
                position: Position::new(0, 0),
                text: "root".to_string(),
                len: 4,
            });
            tree.push(tx, None);
        }

        let mut tx = EditTransaction::new(format!("Branch {}", i));
        tx.record(EditOperation::Insert {
            position: Position::new(0, 4),
            text: format!("{}", i),
            len: 1,
        });
        tree.push(tx, None);
    }

    // Verify we can navigate
    assert!(tree.can_undo());
}

#[test]
fn test_undo_tree_transaction_with_multiple_ops() {
    let mut tree = UndoTree::new();

    let mut tx = EditTransaction::new("Multi-op");
    for i in 0..10 {
        tx.record(EditOperation::Insert {
            position: Position::new(0, i as u32),
            text: format!("{}", i),
            len: 1,
        });
    }
    tree.push(tx, None);

    assert_eq!(tree.current_seq(), 1);

    // Get the transaction
    let current_tx = tree.current_transaction().unwrap();
    assert_eq!(current_tx.ops.len(), 10);

    // Inverse should have 10 ops in reverse order
    let inverse = current_tx.inverse();
    assert_eq!(inverse.len(), 10);
}

#[test]
fn test_block_change_operation_inverse() {
    let op = EditOperation::BlockChange {
        range: Range::new(Position::new(0, 0), Position::new(2, 0)),
        old_content: vec!["line1".to_string(), "line2".to_string()],
        new_content: vec!["new1".to_string(), "new2".to_string(), "new3".to_string()],
    };

    let inverse = op.inverse();
    match inverse {
        EditOperation::BlockChange {
            old_content,
            new_content,
            ..
        } => {
            // Swapped
            assert_eq!(old_content, vec!["new1", "new2", "new3"]);
            assert_eq!(new_content, vec!["line1", "line2"]);
        }
        _ => panic!("Expected BlockChange"),
    }
}

#[test]
fn test_operation_description_long_text() {
    let insert = EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".repeat(100),
        len: 100,
    };
    let desc = insert.description();
    assert!(desc.contains("100 chars"));
    assert!(!desc.contains("aaaa")); // Should not show the actual text
}

#[test]
fn test_undo_at_root_returns_none() {
    let mut tree = UndoTree::new();
    assert!(!tree.can_undo());
    assert!(tree.undo().is_none());
}

#[test]
fn test_redo_at_leaf_returns_none() {
    let mut tree = UndoTree::new();

    let mut tx = EditTransaction::new("Edit");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx, None);

    assert!(!tree.can_redo());
    assert!(tree.redo().is_none());
}

// =============================================================================
// goto_seq Tests
// =============================================================================

#[test]
fn test_goto_seq_same_position() {
    let mut tree = UndoTree::new();

    let mut tx = EditTransaction::new("Edit");
    tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx, None);

    let result = tree.goto_seq(1).unwrap();
    assert_eq!(result.undo_ops.len(), 0);
    assert_eq!(result.redo_ops.len(), 0);
    assert_eq!(tree.current_seq(), 1);
}

#[test]
fn test_goto_seq_backward() {
    let mut tree = UndoTree::new();

    // Push 3 edits
    for i in 0..3 {
        let mut tx = EditTransaction::new(format!("Edit {}", i));
        tx.record(EditOperation::Insert {
            position: Position::new(0, i as u32),
            text: format!("{}", i),
            len: 1,
        });
        tree.push(tx, None);
    }

    assert_eq!(tree.current_seq(), 3);

    // Go back to seq=1
    let result = tree.goto_seq(1).unwrap();
    assert_eq!(tree.current_seq(), 1);
    assert_eq!(result.undo_ops.len(), 2); // Undo seq=3 and seq=2
    assert_eq!(result.redo_ops.len(), 0);
}

#[test]
fn test_goto_seq_forward() {
    let mut tree = UndoTree::new();

    // Push 3 edits
    for i in 0..3 {
        let mut tx = EditTransaction::new(format!("Edit {}", i));
        tx.record(EditOperation::Insert {
            position: Position::new(0, i as u32),
            text: format!("{}", i),
            len: 1,
        });
        tree.push(tx, None);
    }

    // Go back to root via undo
    tree.undo();
    tree.undo();
    tree.undo();
    assert_eq!(tree.current_seq(), 0);

    // Jump forward to seq=2
    let result = tree.goto_seq(2).unwrap();
    assert_eq!(tree.current_seq(), 2);
    assert_eq!(result.undo_ops.len(), 0);
    assert_eq!(result.redo_ops.len(), 2); // Redo seq=1 and seq=2
}

#[test]
fn test_goto_seq_to_root() {
    let mut tree = UndoTree::new();

    // Push 3 edits
    for i in 0..3 {
        let mut tx = EditTransaction::new(format!("Edit {}", i));
        tx.record(EditOperation::Insert {
            position: Position::new(0, i as u32),
            text: format!("{}", i),
            len: 1,
        });
        tree.push(tx, None);
    }

    // Jump to root
    let result = tree.goto_seq(0).unwrap();
    assert_eq!(tree.current_seq(), 0);
    assert_eq!(result.undo_ops.len(), 3);
    assert_eq!(result.redo_ops.len(), 0);
}

#[test]
fn test_goto_seq_cross_branch() {
    let mut tree = UndoTree::new();

    // Create branching structure:
    // 0 -> 1 -> 2
    //      |
    //      +-> 3

    let mut tx1 = EditTransaction::new("Edit 1");
    tx1.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx1, None); // seq=1

    let mut tx2 = EditTransaction::new("Edit 2");
    tx2.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "b".to_string(),
        len: 1,
    });
    tree.push(tx2, None); // seq=2

    // Go back to seq=1
    tree.undo();
    assert_eq!(tree.current_seq(), 1);

    // Create branch
    let mut tx3 = EditTransaction::new("Edit 3");
    tx3.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "c".to_string(),
        len: 1,
    });
    tree.push(tx3, None); // seq=3

    // Now at seq=3, jump to seq=2 (different branch)
    let result = tree.goto_seq(2).unwrap();
    assert_eq!(tree.current_seq(), 2);
    // Should undo seq=3 (back to seq=1), then redo seq=2
    assert_eq!(result.undo_ops.len(), 1);
    assert_eq!(result.redo_ops.len(), 1);
}

#[test]
fn test_goto_seq_invalid_target() {
    let mut tree = UndoTree::new();

    let result = tree.goto_seq(999);
    assert!(result.is_err());
    match result {
        Err(UndoError::InvalidSeq(seq)) => assert_eq!(seq, 999),
        _ => panic!("Expected InvalidSeq error"),
    }
}

#[test]
fn test_goto_seq_updates_last_visited_child() {
    let mut tree = UndoTree::new();

    // Create branching structure
    let mut tx1 = EditTransaction::new("Edit 1");
    tx1.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx1, None); // seq=1

    let mut tx2 = EditTransaction::new("Edit 2");
    tx2.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "b".to_string(),
        len: 1,
    });
    tree.push(tx2, None); // seq=2

    tree.undo(); // Back to seq=1

    let mut tx3 = EditTransaction::new("Edit 3");
    tx3.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "c".to_string(),
        len: 1,
    });
    tree.push(tx3, None); // seq=3

    // Now goto seq=2
    tree.goto_seq(2).unwrap();

    // Undo back to seq=1
    tree.undo();
    assert_eq!(tree.current_seq(), 1);

    // Redo should go to seq=2 (not seq=3) because goto_seq updated last_visited_child
    tree.redo();
    assert_eq!(tree.current_seq(), 2);
}

#[test]
fn test_compute_replay_path() {
    let mut tree = UndoTree::new();

    // Create branching structure:
    // 0 -> 1 -> 2
    //      |
    //      +-> 3

    let mut tx1 = EditTransaction::new("Edit 1");
    tx1.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: "a".to_string(),
        len: 1,
    });
    tree.push(tx1, None); // seq=1

    let mut tx2 = EditTransaction::new("Edit 2");
    tx2.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "b".to_string(),
        len: 1,
    });
    tree.push(tx2, None); // seq=2

    tree.undo(); // Back to seq=1

    let mut tx3 = EditTransaction::new("Edit 3");
    tx3.record(EditOperation::Insert {
        position: Position::new(0, 1),
        text: "c".to_string(),
        len: 1,
    });
    tree.push(tx3, None); // seq=3

    // Current is seq=3. Compute path to seq=2.
    // Should undo seq=3 (to seq=1) and redo seq=2.
    let replay = tree.compute_replay_path(3, 2).unwrap();

    assert_eq!(replay.from_seq, 3);
    assert_eq!(replay.to_seq, 2);
    assert_eq!(replay.undo_ops.len(), 1);
    assert_eq!(replay.undo_ops[0].description, "Edit 3");
    assert_eq!(replay.redo_ops.len(), 1);
    assert_eq!(replay.redo_ops[0].description, "Edit 2");

    // Verify state was NOT mutated
    assert_eq!(tree.current_seq(), 3);
}
