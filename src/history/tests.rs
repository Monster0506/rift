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
