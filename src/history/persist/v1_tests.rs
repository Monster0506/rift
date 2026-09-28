use super::super::{Decoded, DecodedNode};
use super::*;
use crate::character::Character;
use crate::history::{EditTransaction, Position, Range};
use std::collections::HashMap;

fn chars(s: &str) -> Vec<Character> {
    s.chars().map(Character::from).collect()
}

fn sample_tree() -> UndoTree {
    let mut tree = UndoTree::new();

    let mut insert_tx = EditTransaction::new("insert");
    insert_tx.cursor_before = Some(0);
    insert_tx.cursor_after = Some(3);
    insert_tx.record(EditOperation::Insert {
        position: Position::new(0, 0),
        text: chars("abc"),
        len: 3,
    });
    tree.push(insert_tx, None);

    let mut delete_tx = EditTransaction::new("delete");
    delete_tx.record(EditOperation::Delete {
        range: Range::new(Position::new(0, 0), Position::new(0, 1)),
        deleted_text: chars("a"),
    });
    tree.push(delete_tx, None);

    let mut replace_tx = EditTransaction::new("replace");
    replace_tx.record(EditOperation::Replace {
        range: Range::new(Position::new(0, 0), Position::new(0, 2)),
        old_text: chars("bc"),
        new_text: chars("xyz"),
    });
    tree.push(replace_tx, None);

    let mut block_tx = EditTransaction::new("block change");
    block_tx.record(EditOperation::BlockChange {
        range: Range::new(Position::new(0, 0), Position::new(1, 0)),
        old_content: vec![chars("line1"), chars("line2")],
        new_content: vec![chars("LINE1"), chars("LINE2"), chars("LINE3")],
    });
    tree.push(block_tx, None);

    tree
}

#[test]
fn round_trip_preserves_every_operation_kind_and_cursor_positions() {
    let tree = sample_tree();
    let payload = encode("/tmp/x.txt", [7u8; 32], tree.current_seq(), &tree);
    let decoded: Decoded = decode(&payload).expect("must decode what was just encoded");

    assert_eq!(decoded.canonical_path, "/tmp/x.txt");
    assert_eq!(decoded.content_hash, [7u8; 32]);
    assert_eq!(decoded.current, tree.current_seq());
    assert_eq!(decoded.nodes.len(), tree.nodes.len());

    let by_seq: HashMap<u64, DecodedNode> = decoded.nodes.into_iter().map(|n| (n.seq, n)).collect();

    let insert_node = &by_seq[&1];
    assert_eq!(insert_node.transaction.description, "insert");
    assert_eq!(insert_node.transaction.cursor_before, Some(0));
    assert_eq!(insert_node.transaction.cursor_after, Some(3));
    match &insert_node.transaction.ops[0] {
        EditOperation::Insert {
            position,
            text,
            len,
        } => {
            assert_eq!(*position, Position::new(0, 0));
            assert_eq!(text, &chars("abc"));
            assert_eq!(*len, 3);
        }
        other => panic!("expected Insert, got {other:?}"),
    }

    match &by_seq[&2].transaction.ops[0] {
        EditOperation::Delete {
            range,
            deleted_text,
        } => {
            assert_eq!(*range, Range::new(Position::new(0, 0), Position::new(0, 1)));
            assert_eq!(deleted_text, &chars("a"));
        }
        other => panic!("expected Delete, got {other:?}"),
    }

    match &by_seq[&3].transaction.ops[0] {
        EditOperation::Replace {
            old_text, new_text, ..
        } => {
            assert_eq!(old_text, &chars("bc"));
            assert_eq!(new_text, &chars("xyz"));
        }
        other => panic!("expected Replace, got {other:?}"),
    }

    match &by_seq[&4].transaction.ops[0] {
        EditOperation::BlockChange {
            old_content,
            new_content,
            ..
        } => {
            assert_eq!(old_content, &vec![chars("line1"), chars("line2")]);
            assert_eq!(
                new_content,
                &vec![chars("LINE1"), chars("LINE2"), chars("LINE3")]
            );
        }
        other => panic!("expected BlockChange, got {other:?}"),
    }
}

#[test]
fn position_conversion_round_trips_within_u32_range() {
    let p = PositionV1::from(Position::new(42, 7));
    assert_eq!(Position::try_from(p).unwrap(), Position::new(42, 7));
}

#[test]
fn position_conversion_rejects_a_line_beyond_u32_range() {
    let bogus = PositionV1 {
        line: u64::from(u32::MAX) + 1,
        col: 0,
    };
    assert!(Position::try_from(bogus).is_err());
}

#[test]
fn decode_rejects_truncated_payload_bytes() {
    let tree = sample_tree();
    let payload = encode("/tmp/x.txt", [0u8; 32], tree.current_seq(), &tree);
    assert!(decode(&payload[..payload.len() / 2]).is_err());
}

#[test]
fn decode_rejects_a_payload_missing_the_root_or_current_node() {
    let empty = UndoFileV1 {
        canonical_path: "/tmp/x.txt".to_string(),
        content_hash: [0u8; 32],
        current: 0,
        nodes: Vec::new(),
    };
    let payload = bincode::serialize(&empty).unwrap();
    let decoded = decode(&payload).unwrap();
    assert!(decoded.nodes.is_empty());
}
