use super::*;

pub(super) fn create_manager() -> DocumentManager {
    DocumentManager::new()
}

pub(super) fn make_dir_entries(names: &[(&str, bool)], base: &str) -> Vec<DirEntry> {
    names
        .iter()
        .map(|(name, is_dir)| DirEntry {
            path: PathBuf::from(base).join(name),
            is_dir: *is_dir,
            id: 0, // assigned by populate_directory_buffer
        })
        .collect()
}

pub(super) fn make_populated_directory_doc(dir: &str, names: &[(&str, bool)]) -> Document {
    let mut doc = Document::new_directory(1, PathBuf::from(dir)).unwrap();
    let entries = make_dir_entries(names, dir);
    doc.populate_directory_buffer(entries);
    doc
}

pub(super) fn set_buffer_text(doc: &mut Document, text: &str) {
    let old_len = doc.buffer.len();
    let _ = doc.buffer.set_cursor(0);
    let _ = doc.delete_range(0, old_len);
    let _ = doc.insert_str(text);
}

pub(super) fn set_annotated_buffer(doc: &mut Document, text: &str) {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut plain_lines: Vec<&str> = Vec::new();
    let mut line_annotations: Vec<(usize, u16)> = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let b = line.as_bytes();
        let has_annotation_prefix = b.len() >= 5
            && b[0] == b'/'
            && b[1].is_ascii_digit()
            && b[2].is_ascii_digit()
            && b[3].is_ascii_digit()
            && b[4] == b' ';

        if has_annotation_prefix {
            if let Ok(digits) = std::str::from_utf8(&b[1..4]) {
                if let Ok(entry_id) = digits.parse::<u16>() {
                    line_annotations.push((line_idx, entry_id));
                }
            }
            plain_lines.push(&line[5..]);
        } else {
            plain_lines.push(line);
        }
    }

    let plain_text = plain_lines.join("\n");
    set_buffer_text(doc, &plain_text);

    doc.annotations.clear();
    for (line, entry_id) in line_annotations {
        doc.annotations.create_directory_entry(line, entry_id);
    }
}

pub(super) fn line_anchor(doc: &Document, id: crate::annotations::AnnotationId) -> Option<usize> {
    match doc.annotations.get(id)?.anchor {
        crate::annotations::Anchor::Line(l) => Some(l),
        other => panic!("expected line anchor, got {:?}", other),
    }
}

pub(super) fn staged_entry(path: &str) -> crate::git::status::StatusEntry {
    crate::git::status::StatusEntry {
        path: PathBuf::from(path),
        orig_path: None,
        index_state: crate::git::status::FileState::Modified,
        worktree_state: crate::git::status::FileState::Unmodified,
        kind: crate::git::status::EntryKind::Ordinary,
    }
}

pub(super) fn unstaged_entry(path: &str) -> crate::git::status::StatusEntry {
    crate::git::status::StatusEntry {
        path: PathBuf::from(path),
        orig_path: None,
        index_state: crate::git::status::FileState::Unmodified,
        worktree_state: crate::git::status::FileState::Modified,
        kind: crate::git::status::EntryKind::Ordinary,
    }
}

pub(super) fn untracked_entry(path: &str) -> crate::git::status::StatusEntry {
    crate::git::status::StatusEntry {
        path: PathBuf::from(path),
        orig_path: None,
        index_state: crate::git::status::FileState::Unmodified,
        worktree_state: crate::git::status::FileState::Unmodified,
        kind: crate::git::status::EntryKind::Untracked,
    }
}

pub(super) fn unmerged_entry(path: &str) -> crate::git::status::StatusEntry {
    crate::git::status::StatusEntry {
        path: PathBuf::from(path),
        orig_path: None,
        index_state: crate::git::status::FileState::UpdatedUnmerged,
        worktree_state: crate::git::status::FileState::UpdatedUnmerged,
        kind: crate::git::status::EntryKind::Unmerged,
    }
}

pub(super) fn make_git_status_doc(entries: Vec<crate::git::status::StatusEntry>) -> Document {
    let mut doc = Document::new_git_status(1, PathBuf::from("/repo")).unwrap();
    let snapshot = crate::git::status::StatusSnapshot {
        branch: crate::git::status::BranchInfo::default(),
        entries,
    };
    doc.populate_git_status_buffer(snapshot, None);
    doc
}
