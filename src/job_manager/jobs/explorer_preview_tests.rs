use super::*;
use crate::job_manager::{CancellationSignal, JobMessage, JobPayload};
use std::sync::mpsc;

fn make_signal(cancelled: bool) -> CancellationSignal {
    CancellationSignal::new(cancelled)
}

#[test]
fn test_explorer_preview_result_job_payload() {
    let r = ExplorerPreviewResult {
        right_doc_id: 5,
        path: PathBuf::from("/tmp"),
        dir_entries: Some(vec![]),
        file_text: None,
        undo_file: None,
    };
    let boxed: Box<dyn JobPayload> = Box::new(r);
    assert!(boxed
        .as_any()
        .downcast_ref::<ExplorerPreviewResult>()
        .is_some());
}

#[test]
fn test_explorer_preview_job_is_silent() {
    let job = ExplorerPreviewJob::new(1, PathBuf::from("/tmp"), false);
    assert!(job.is_silent());
}

#[test]
fn test_explorer_preview_dir_returns_entries() {
    let tmp = std::env::temp_dir();
    let job = Box::new(ExplorerPreviewJob::new(42, tmp.clone(), false));
    let (tx, rx) = mpsc::channel();
    job.run(1, tx, make_signal(false));

    let result =
        crate::job_manager::jobs::test_support::recv_custom_payload::<ExplorerPreviewResult>(&rx)
            .expect("should have Custom message");

    assert_eq!(result.right_doc_id, 42);
    assert_eq!(result.path, tmp);
    assert!(
        result.dir_entries.is_some(),
        "dir preview should have entries"
    );
    assert!(result.file_text.is_none());
}

#[test]
fn test_explorer_preview_nonexistent_file_returns_placeholder() {
    let path = PathBuf::from("/nonexistent_path_xyz_rift_test/file.txt");
    let job = Box::new(ExplorerPreviewJob::new(1, path, false));
    let (tx, rx) = mpsc::channel();
    job.run(1, tx, make_signal(false));

    let result =
        crate::job_manager::jobs::test_support::recv_custom_payload::<ExplorerPreviewResult>(&rx)
            .expect("should have Custom message");
    assert!(result.file_text.is_some());
    assert!(
        result
            .file_text
            .as_deref()
            .unwrap()
            .contains("cannot open file")
            || result.file_text.as_deref().unwrap().is_empty()
    );
}

#[test]
fn test_explorer_preview_cancelled_before_run() {
    let job = Box::new(ExplorerPreviewJob::new(1, std::env::temp_dir(), false));
    let (tx, rx) = mpsc::channel();
    job.run(1, tx, make_signal(true));
    let msgs: Vec<JobMessage> = rx.try_iter().collect();
    assert!(!msgs.iter().any(|m| matches!(m, JobMessage::Custom(_, _))));
}

#[test]
fn test_explorer_preview_dir_entries_sorted_dirs_first() {
    let dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    let job = Box::new(ExplorerPreviewJob::new(1, dir, true));
    let (tx, rx) = mpsc::channel();
    job.run(1, tx, make_signal(false));

    let result =
        crate::job_manager::jobs::test_support::recv_custom_payload::<ExplorerPreviewResult>(&rx)
            .map(|r| *r);
    if let Some(entries) = result.and_then(|r| r.dir_entries) {
        let mut saw_file = false;
        for entry in &entries {
            if !entry.is_dir {
                saw_file = true;
            }
            if saw_file && entry.is_dir {
                panic!("directory found after file - sorting is wrong");
            }
        }
    }
}

#[test]
fn test_explorer_preview_finished_message_is_sent() {
    let job = Box::new(ExplorerPreviewJob::new(1, std::env::temp_dir(), false));
    let (tx, rx) = mpsc::channel();
    job.run(1, tx, make_signal(false));
    let msgs: Vec<JobMessage> = rx.try_iter().collect();
    assert!(msgs
        .iter()
        .any(|m| matches!(m, JobMessage::Finished(1, true))));
}

#[test]
fn test_explorer_preview_valid_utf8_split_at_boundary_is_not_binary() {
    let mut content = vec![b'a'; FILE_PREVIEW_BYTES - 2];
    content.extend_from_slice("\u{20ac}".as_bytes());
    content.extend_from_slice(b"trailing text after the split char");

    let path = std::env::temp_dir().join(format!(
        "rift_preview_split_utf8_{}_{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, &content).unwrap();

    let job = Box::new(ExplorerPreviewJob::new(1, path.clone(), false));
    let (tx, rx) = mpsc::channel();
    job.run(1, tx, make_signal(false));
    let result =
        crate::job_manager::jobs::test_support::recv_custom_payload::<ExplorerPreviewResult>(&rx)
            .expect("should have Custom message");
    let text = result.file_text.unwrap();

    let _ = std::fs::remove_file(&path);

    assert_ne!(
        text, "<binary file>",
        "valid UTF-8 file misclassified as binary due to a multibyte char \
         straddling the read boundary"
    );
    assert!(text.starts_with(&"a".repeat(100)));
}

#[test]
fn test_explorer_preview_recognizes_a_persisted_undo_file() {
    let dir = std::env::temp_dir().join(format!(
        "rift_preview_undo_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let source_path = dir.join("notes.txt");
    let undo_dir = dir.join("undo");

    let mut tree = crate::history::UndoTree::new();
    let mut tx = crate::history::EditTransaction::new("insert 'hi'");
    tx.record(crate::history::EditOperation::Insert {
        position: crate::history::Position::new(0, 0),
        text: "hi"
            .chars()
            .map(crate::character::Character::from)
            .collect(),
        len: 2,
    });
    tree.push(tx, None);
    let hash = crate::history::persist::sha256(b"hi");
    crate::history::persist::save(&undo_dir, &source_path, &tree, hash).unwrap();
    let undo_file_path = crate::history::persist::undo_file_path(&undo_dir, &source_path);

    let job = Box::new(ExplorerPreviewJob::new(1, undo_file_path.clone(), false));
    let (tx_chan, rx) = mpsc::channel();
    job.run(1, tx_chan, make_signal(false));
    let result =
        crate::job_manager::jobs::test_support::recv_custom_payload::<ExplorerPreviewResult>(&rx)
            .expect("should have Custom message");

    let _ = std::fs::remove_dir_all(&dir);

    assert!(result.file_text.is_none());
    let parsed = result.undo_file.expect("undo file must be recognized");
    assert_eq!(parsed.tree.current_seq(), tree.current_seq());
    assert_eq!(parsed.tree.nodes[&1].transaction.description, "insert 'hi'");
}
