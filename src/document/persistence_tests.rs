use crate::document::Document;

#[test]
fn save_as_uses_tilde_suffix_for_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");

    let mut doc = Document::new(1).unwrap();
    doc.save_as(&path).unwrap();

    assert!(path.exists());

    let old_tmp = dir.path().join(".note.txt.tmp");
    assert!(
        !old_tmp.exists(),
        "old-style temp file should not exist: {old_tmp:?}"
    );

    let tilde_tmp = dir.path().join("note.txt~");
    assert!(
        !tilde_tmp.exists(),
        "tilde temp file should be renamed away: {tilde_tmp:?}"
    );
}
