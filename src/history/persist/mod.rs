mod v1;

use super::{EditNode, EditSeq, EditTransaction, UndoTree};
use crate::time::UNIX_EPOCH;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub const MAGIC: [u8; 8] = *b"\x89RiftUDO";

const CURRENT_VERSION: u16 = 1;

pub type Sha256Digest = [u8; 32];

struct DecodedNode {
    seq: EditSeq,
    parent: Option<EditSeq>,
    last_visited_child: Option<usize>,
    timestamp_unix_secs: u64,
    transaction: EditTransaction,
}

struct Decoded {
    canonical_path: String,
    content_hash: Sha256Digest,
    current: EditSeq,
    nodes: Vec<DecodedNode>,
}

#[derive(Debug)]
pub struct ParsedUndoFile {
    pub source_path: PathBuf,
    pub content_hash: Sha256Digest,
    pub tree: UndoTree,
}

#[must_use]
pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).into()
}

#[must_use]
pub fn expand_tilde(path: &str) -> PathBuf {
    let home = || {
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        std::env::var(var).ok()
    };
    if let Some(rest) = path.strip_prefix('~') {
        if rest.is_empty() {
            if let Some(home) = home() {
                return PathBuf::from(home);
            }
        } else if let Some(rest) = rest.strip_prefix(['/', '\\']) {
            if let Some(home) = home() {
                return PathBuf::from(home).join(rest);
            }
        }
    }
    PathBuf::from(path)
}

#[must_use]
pub fn resolve_undo_dir(configured: Option<&Path>) -> PathBuf {
    configured
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate::editor::user_config_dir().join("undofiles"))
}

fn undo_file_name(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "buffer".to_string());
    format!("{stem}.{hash:016x}.undo")
}

#[must_use]
pub fn undo_file_path(undo_dir: &Path, doc_path: &Path) -> PathBuf {
    undo_dir.join(undo_file_name(doc_path))
}

#[must_use]
pub fn has_magic(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() + 2 && bytes[..MAGIC.len()] == MAGIC
}

fn split_envelope(bytes: &[u8]) -> Option<(u16, &[u8])> {
    if !has_magic(bytes) {
        return None;
    }
    let version = u16::from_le_bytes([bytes[MAGIC.len()], bytes[MAGIC.len() + 1]]);
    Some((version, &bytes[MAGIC.len() + 2..]))
}

fn decode_payload(version: u16, payload: &[u8]) -> Result<Decoded, String> {
    match version {
        1 => v1::decode(payload),
        other => Err(format!("unsupported undo file version {other}")),
    }
}

pub fn save(
    undo_dir: &Path,
    doc_path: &Path,
    tree: &UndoTree,
    content_hash: Sha256Digest,
) -> std::io::Result<()> {
    std::fs::create_dir_all(undo_dir)?;
    let canonical_path = doc_path.to_string_lossy();
    let payload = v1::encode(&canonical_path, content_hash, tree.current_seq(), tree);

    let mut out = Vec::with_capacity(MAGIC.len() + 2 + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&CURRENT_VERSION.to_le_bytes());
    out.extend_from_slice(&payload);
    std::fs::write(undo_file_path(undo_dir, doc_path), out)
}

fn build_tree(nodes: Vec<DecodedNode>, current: EditSeq) -> Option<UndoTree> {
    let mut nodes: HashMap<EditSeq, EditNode> = nodes
        .into_iter()
        .map(|n| {
            let timestamp = UNIX_EPOCH + std::time::Duration::from_secs(n.timestamp_unix_secs);
            (
                n.seq,
                EditNode {
                    seq: n.seq,
                    transaction: n.transaction,
                    parent: n.parent,
                    children: Vec::new(),
                    last_visited_child: n.last_visited_child,
                    snapshot: None,
                    timestamp,
                },
            )
        })
        .collect();

    let mut seqs: Vec<EditSeq> = nodes.keys().copied().collect();
    seqs.sort_unstable();
    for seq in seqs {
        let Some(parent_seq) = nodes.get(&seq).and_then(|n| n.parent) else {
            continue;
        };
        if let Some(parent) = nodes.get_mut(&parent_seq) {
            parent.children.push(seq);
        }
    }

    if !nodes.contains_key(&0) || !nodes.contains_key(&current) {
        return None;
    }
    Some(UndoTree::from_parts(nodes, current))
}

#[must_use]
pub fn load(undo_dir: &Path, doc_path: &Path, content_hash: Sha256Digest) -> Option<UndoTree> {
    let bytes = std::fs::read(undo_file_path(undo_dir, doc_path)).ok()?;
    let (version, payload) = split_envelope(&bytes)?;
    let file = decode_payload(version, payload).ok()?;
    let doc_path_str = doc_path.to_string_lossy();
    if file.content_hash != content_hash || file.canonical_path.as_str() != doc_path_str.as_ref() {
        return None;
    }
    build_tree(file.nodes, file.current)
}

pub fn parse_for_display(bytes: &[u8]) -> Result<ParsedUndoFile, String> {
    let (version, payload) =
        split_envelope(bytes).ok_or_else(|| "unrecognized undo file format".to_string())?;
    let file = decode_payload(version, payload)?;
    let source_path = PathBuf::from(&file.canonical_path);
    let content_hash = file.content_hash;
    let tree = build_tree(file.nodes, file.current)
        .ok_or_else(|| "corrupt undo file: missing root or current node".to_string())?;
    Ok(ParsedUndoFile {
        source_path,
        content_hash,
        tree,
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
