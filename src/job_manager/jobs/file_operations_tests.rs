use super::*;
use crate::character::Character;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use std::sync::mpsc;

fn make_signal() -> CancellationSignal {
    CancellationSignal::new(false)
}

fn run_load_job(path: PathBuf) -> FileLoadResult {
    let (tx, rx) = mpsc::channel();
    let doc_id: crate::document::DocumentId = 42;
    let job = Box::new(FileLoadJob::new(doc_id, path));
    job.run(1, tx, make_signal());

    *crate::job_manager::jobs::test_support::recv_custom_payload::<FileLoadResult>(&rx)
        .expect("FileLoadJob did not produce a FileLoadResult")
}

#[test]
fn file_load_job_strips_utf8_bom() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bom.txt");
    std::fs::write(&path, b"\xEF\xBB\xBFhello").unwrap();

    let result = run_load_job(path);
    let chars: Vec<Character> = result.line_index.table.iter().collect();

    assert_eq!(chars.len(), 5);
    assert_eq!(chars[0], Character::Unicode('h'));
}

fn run_save_job(path: PathBuf, content: &str) -> bool {
    let (tx, rx) = mpsc::channel();
    let chars: Vec<Character> = content.chars().map(Character::Unicode).collect();
    let piece_table = crate::buffer::rope::PieceTable::new(chars);
    let job = Box::new(FileSaveJob::new(
        1,
        piece_table,
        path,
        crate::document::LineEnding::LF,
        0,
    ));
    job.run(1, tx, make_signal());
    rx.into_iter()
        .any(|m| matches!(m, JobMessage::Finished(_, true)))
}

#[test]
fn file_save_job_uses_tilde_suffix_for_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.txt");

    let succeeded = run_save_job(path.clone(), "hello");
    assert!(succeeded);

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");

    let old_tmp = dir.path().join(".hello.txt.tmp");
    assert!(
        !old_tmp.exists(),
        "old-style temp file should not exist: {old_tmp:?}"
    );

    let tilde_tmp = dir.path().join("hello.txt~");
    assert!(
        !tilde_tmp.exists(),
        "tilde temp file should be renamed away: {tilde_tmp:?}"
    );
}

#[test]
fn file_load_job_decodes_multibyte_utf8() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("em_dash.txt");
    std::fs::write(&path, "a—b").unwrap();

    let result = run_load_job(path);
    let chars: Vec<Character> = result.line_index.table.iter().collect();

    assert_eq!(chars.len(), 3);
    assert_eq!(chars[0], Character::Unicode('a'));
    assert_eq!(chars[1], Character::Unicode('—'));
    assert_eq!(chars[2], Character::Unicode('b'));
}
