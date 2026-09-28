use super::{Decoded, DecodedNode, Sha256Digest};
use crate::character::Character;
use crate::history::{EditOperation, EditSeq, EditTransaction, Position, Range, UndoTree};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum CharacterV1 {
    Unicode(char),
    Byte(u8),
    Tab,
    Newline,
    Control(u8),
}

impl From<Character> for CharacterV1 {
    fn from(c: Character) -> Self {
        match c {
            Character::Unicode(ch) => CharacterV1::Unicode(ch),
            Character::Byte(b) => CharacterV1::Byte(b),
            Character::Tab => CharacterV1::Tab,
            Character::Newline => CharacterV1::Newline,
            Character::Control(b) => CharacterV1::Control(b),
        }
    }
}

impl From<CharacterV1> for Character {
    fn from(c: CharacterV1) -> Self {
        match c {
            CharacterV1::Unicode(ch) => Character::Unicode(ch),
            CharacterV1::Byte(b) => Character::Byte(b),
            CharacterV1::Tab => Character::Tab,
            CharacterV1::Newline => Character::Newline,
            CharacterV1::Control(b) => Character::Control(b),
        }
    }
}

fn chars_to_v1(chars: &[Character]) -> Vec<CharacterV1> {
    chars.iter().copied().map(CharacterV1::from).collect()
}

fn chars_from_v1(chars: Vec<CharacterV1>) -> Vec<Character> {
    chars.into_iter().map(Character::from).collect()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct PositionV1 {
    line: u64,
    col: u64,
}

impl From<Position> for PositionV1 {
    fn from(p: Position) -> Self {
        Self {
            line: u64::from(p.line),
            col: u64::from(p.col),
        }
    }
}

impl TryFrom<PositionV1> for Position {
    type Error = String;
    fn try_from(p: PositionV1) -> Result<Self, String> {
        let line = u32::try_from(p.line).map_err(|_| "position line out of range".to_string())?;
        let col = u32::try_from(p.col).map_err(|_| "position col out of range".to_string())?;
        Ok(Position::new(line, col))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct RangeV1 {
    start: PositionV1,
    end: PositionV1,
}

impl From<Range> for RangeV1 {
    fn from(r: Range) -> Self {
        Self {
            start: r.start.into(),
            end: r.end.into(),
        }
    }
}

impl TryFrom<RangeV1> for Range {
    type Error = String;
    fn try_from(r: RangeV1) -> Result<Self, String> {
        Ok(Range::new(r.start.try_into()?, r.end.try_into()?))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum OperationV1 {
    Insert {
        position: PositionV1,
        text: Vec<CharacterV1>,
        len: u64,
    },
    Delete {
        range: RangeV1,
        deleted_text: Vec<CharacterV1>,
    },
    Replace {
        range: RangeV1,
        old_text: Vec<CharacterV1>,
        new_text: Vec<CharacterV1>,
    },
    BlockChange {
        range: RangeV1,
        old_content: Vec<Vec<CharacterV1>>,
        new_content: Vec<Vec<CharacterV1>>,
    },
}

impl From<&EditOperation> for OperationV1 {
    fn from(op: &EditOperation) -> Self {
        match op {
            EditOperation::Insert {
                position,
                text,
                len,
            } => OperationV1::Insert {
                position: (*position).into(),
                text: chars_to_v1(text),
                len: *len as u64,
            },
            EditOperation::Delete {
                range,
                deleted_text,
            } => OperationV1::Delete {
                range: (*range).into(),
                deleted_text: chars_to_v1(deleted_text),
            },
            EditOperation::Replace {
                range,
                old_text,
                new_text,
            } => OperationV1::Replace {
                range: (*range).into(),
                old_text: chars_to_v1(old_text),
                new_text: chars_to_v1(new_text),
            },
            EditOperation::BlockChange {
                range,
                old_content,
                new_content,
            } => OperationV1::BlockChange {
                range: (*range).into(),
                old_content: old_content.iter().map(|line| chars_to_v1(line)).collect(),
                new_content: new_content.iter().map(|line| chars_to_v1(line)).collect(),
            },
        }
    }
}

impl TryFrom<OperationV1> for EditOperation {
    type Error = String;
    fn try_from(op: OperationV1) -> Result<Self, String> {
        Ok(match op {
            OperationV1::Insert {
                position,
                text,
                len,
            } => EditOperation::Insert {
                position: position.try_into()?,
                text: chars_from_v1(text),
                len: usize::try_from(len).map_err(|_| "insert len out of range".to_string())?,
            },
            OperationV1::Delete {
                range,
                deleted_text,
            } => EditOperation::Delete {
                range: range.try_into()?,
                deleted_text: chars_from_v1(deleted_text),
            },
            OperationV1::Replace {
                range,
                old_text,
                new_text,
            } => EditOperation::Replace {
                range: range.try_into()?,
                old_text: chars_from_v1(old_text),
                new_text: chars_from_v1(new_text),
            },
            OperationV1::BlockChange {
                range,
                old_content,
                new_content,
            } => EditOperation::BlockChange {
                range: range.try_into()?,
                old_content: old_content.into_iter().map(chars_from_v1).collect(),
                new_content: new_content.into_iter().map(chars_from_v1).collect(),
            },
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TransactionV1 {
    ops: Vec<OperationV1>,
    description: String,
    cursor_before: Option<u64>,
    cursor_after: Option<u64>,
}

impl From<&EditTransaction> for TransactionV1 {
    fn from(tx: &EditTransaction) -> Self {
        Self {
            ops: tx.ops.iter().map(OperationV1::from).collect(),
            description: tx.description.clone(),
            cursor_before: tx.cursor_before.map(|c| c as u64),
            cursor_after: tx.cursor_after.map(|c| c as u64),
        }
    }
}

impl TryFrom<TransactionV1> for EditTransaction {
    type Error = String;
    fn try_from(tx: TransactionV1) -> Result<Self, String> {
        let ops = tx
            .ops
            .into_iter()
            .map(EditOperation::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let to_usize =
            |c: u64| usize::try_from(c).map_err(|_| "cursor offset out of range".to_string());
        Ok(EditTransaction {
            ops,
            description: tx.description,
            cursor_before: tx.cursor_before.map(to_usize).transpose()?,
            cursor_after: tx.cursor_after.map(to_usize).transpose()?,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct NodeV1 {
    seq: u64,
    parent: Option<u64>,
    last_visited_child: Option<u64>,
    timestamp_unix_secs: u64,
    transaction: TransactionV1,
}

#[derive(Serialize, Deserialize)]
struct UndoFileV1 {
    canonical_path: String,
    content_hash: Sha256Digest,
    current: u64,
    nodes: Vec<NodeV1>,
}

pub(super) fn encode(
    canonical_path: &str,
    content_hash: Sha256Digest,
    current: EditSeq,
    tree: &UndoTree,
) -> Vec<u8> {
    let nodes = tree
        .nodes
        .values()
        .map(|n| NodeV1 {
            seq: n.seq,
            parent: n.parent,
            last_visited_child: n.last_visited_child.map(|c| c as u64),
            timestamp_unix_secs: n
                .timestamp
                .duration_since(crate::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            transaction: TransactionV1::from(&n.transaction),
        })
        .collect();
    let file = UndoFileV1 {
        canonical_path: canonical_path.to_string(),
        content_hash,
        current,
        nodes,
    };
    bincode::serialize(&file).expect("v1 undo file must serialize")
}

pub(super) fn decode(payload: &[u8]) -> Result<Decoded, String> {
    let file: UndoFileV1 =
        bincode::deserialize(payload).map_err(|e| format!("corrupt v1 undo file: {e}"))?;
    let nodes = file
        .nodes
        .into_iter()
        .map(|n| {
            let last_visited_child = n
                .last_visited_child
                .map(|c| {
                    usize::try_from(c).map_err(|_| "last_visited_child out of range".to_string())
                })
                .transpose()?;
            Ok(DecodedNode {
                seq: n.seq,
                parent: n.parent,
                last_visited_child,
                timestamp_unix_secs: n.timestamp_unix_secs,
                transaction: EditTransaction::try_from(n.transaction)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Decoded {
        canonical_path: file.canonical_path,
        content_hash: file.content_hash,
        current: file.current,
        nodes,
    })
}

#[cfg(test)]
#[path = "v1_tests.rs"]
mod v1_tests;
