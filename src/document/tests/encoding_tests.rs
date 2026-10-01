use super::*;

#[test]
fn test_from_file_strips_utf8_bom() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bom.txt");
    std::fs::write(&path, b"\xEF\xBB\xBFhello").unwrap();

    let doc = Document::from_file(1, &path).unwrap();
    assert_eq!(doc.buffer.to_string(), "hello");
    assert_eq!(doc.buffer.len(), 5);
}

#[test]
fn test_from_file_normalizes_crlf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crlf.txt");
    std::fs::write(&path, b"line1\r\nline2\r\nline3").unwrap();

    let doc = Document::from_file(1, &path).unwrap();
    let text = doc.buffer.to_string();
    assert!(
        !text.contains('\r'),
        "buffer should not contain \\r after CRLF normalization"
    );
    assert_eq!(text, "line1\nline2\nline3");
    assert_eq!(doc.options.line_ending, LineEnding::CRLF);
}

#[test]
fn test_from_bytes_matches_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bytes.txt");
    let content = b"line1\r\nline2".to_vec();
    std::fs::write(&path, &content).unwrap();

    let from_file = Document::from_file(1, &path).unwrap();
    let from_bytes = Document::from_bytes(2, Some(&path), content).unwrap();

    assert_eq!(from_bytes.buffer.to_string(), from_file.buffer.to_string());
    assert_eq!(
        from_bytes.options.line_ending,
        from_file.options.line_ending
    );
    assert_eq!(from_bytes.file_path, from_file.file_path);
}

#[test]
fn test_from_bytes_without_path_has_no_file_path() {
    let doc = Document::from_bytes(1, None, b"hello".to_vec()).unwrap();
    assert_eq!(doc.buffer.to_string(), "hello");
    assert_eq!(doc.file_path, None);
}

#[test]
fn test_from_file_strips_standalone_cr() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bare_cr.txt");
    std::fs::write(&path, b"line1\rline2").unwrap();

    let doc = Document::from_file(1, &path).unwrap();
    let text = doc.buffer.to_string();
    assert!(
        !text.contains('\r'),
        "buffer should not contain standalone \\r (would render as ^M)"
    );
}
