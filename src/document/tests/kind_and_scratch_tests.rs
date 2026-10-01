use super::*;

#[test]
fn test_new_doc_is_file_kind() {
    let doc = Document::new(1).unwrap();
    assert!(doc.is_file());
}

#[test]
fn test_is_terminal_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_terminal());
}

#[test]
fn test_is_directory_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_directory());
}

#[test]
fn test_is_undotree_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_undotree());
}

#[test]
fn test_is_special_false_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(!doc.is_special());
}

#[test]
fn test_new_directory_kind() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp/test")).unwrap();
    assert!(doc.is_directory());
    assert!(!doc.is_terminal());
    assert!(!doc.is_undotree());
    assert!(doc.is_special());
}

#[test]
fn test_new_directory_show_hidden_defaults_false() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp/test")).unwrap();
    assert_eq!(doc.directory_show_hidden(), Some(false));
}

#[test]
fn test_new_scratch_has_no_path() {
    let lines = vec!["one".to_string(), "two".to_string(), "three".to_string()];
    let doc = Document::new_scratch(1, "[Scratch] test".to_string(), &lines).unwrap();
    assert!(doc.path().is_none());
    assert_eq!(doc.display_name(), "[Scratch] test");
    assert_eq!(doc.kind.kind_str(), "scratch");
}

#[test]
fn test_new_scratch_content_joins_lines_with_newline() {
    use crate::buffer::api::BufferView;
    let lines = vec!["one".to_string(), "two".to_string(), "three".to_string()];
    let doc = Document::new_scratch(1, "scratch".to_string(), &lines).unwrap();
    let text: String = doc
        .buffer
        .chars(0..doc.buffer.len())
        .map(|c| c.to_string())
        .collect();
    assert_eq!(text, "one\ntwo\nthree");
}

#[test]
fn test_new_scratch_empty_lines() {
    let doc = Document::new_scratch(1, "scratch".to_string(), &[]).unwrap();
    assert_eq!(doc.buffer.len(), 0);
}

#[test]
fn test_new_undotree_kind() {
    let doc = Document::new_undotree(1, 42).unwrap();
    assert!(doc.is_undotree());
    assert!(!doc.is_terminal());
    assert!(!doc.is_directory());
    assert!(doc.is_special());
}

#[test]
fn test_new_undotree_is_read_only() {
    let doc = Document::new_undotree(1, 42).unwrap();
    assert!(doc.is_read_only());
}

#[test]
fn test_new_directory_not_read_only() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    assert!(!doc.is_read_only());
}

#[test]
fn test_new_undotree_stores_linked_doc_id() {
    let doc = Document::new_undotree(5, 99).unwrap();
    assert_eq!(doc.undotree_linked_doc_id(), Some(99));
}

#[test]
fn test_new_directory_stores_path() {
    let path = PathBuf::from("/home/user/projects");
    let doc = Document::new_directory(1, path.clone()).unwrap();
    assert_eq!(doc.directory_path(), Some(&path));
}

#[test]
fn test_new_directory_entries_empty() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    assert!(doc.directory_entries().is_some_and(<[DirEntry]>::is_empty));
}

#[test]
fn test_new_undotree_sequences_empty() {
    let doc = Document::new_undotree(1, 42).unwrap();
    assert!(doc
        .undotree_sequences()
        .is_some_and(<[crate::history::EditSeq]>::is_empty));
}

#[test]
fn test_custom_highlights_empty_for_new_file() {
    let doc = Document::new(1).unwrap();
    assert!(doc.custom_highlights.is_empty());
}

#[test]
fn test_custom_highlights_empty_for_new_directory() {
    let doc = Document::new_directory(1, PathBuf::from("/tmp")).unwrap();
    assert!(doc.custom_highlights.is_empty());
}

#[test]
fn test_custom_highlights_empty_for_new_undotree() {
    let doc = Document::new_undotree(1, 2).unwrap();
    assert!(doc.custom_highlights.is_empty());
}

#[test]
fn test_directory_path_returns_none_for_file() {
    let doc = Document::new(1).unwrap();
    assert!(doc.directory_path().is_none());
}

#[test]
fn test_directory_path_returns_none_for_undotree() {
    let doc = Document::new_undotree(1, 2).unwrap();
    assert!(doc.directory_path().is_none());
}

#[test]
fn test_directory_path_returns_path_for_directory() {
    let path = PathBuf::from("/srv/data");
    let doc = Document::new_directory(1, path.clone()).unwrap();
    assert_eq!(doc.directory_path(), Some(&path));
}

#[test]
fn wrap_default_is_auto() {
    let doc = Document::new(1).unwrap();
    assert_eq!(
        doc.options.wrap,
        Some(definitions::WrapMode::Expr("auto".to_string()))
    );
}

#[test]
fn wrap_resolve_auto() {
    let mode = definitions::WrapMode::Expr("auto".to_string());
    assert_eq!(mode.resolve(100), 100);
}

#[test]
fn wrap_resolve_literal() {
    let mode = definitions::WrapMode::Expr("80".to_string());
    assert_eq!(mode.resolve(200), 80);
}

#[test]
fn wrap_resolve_auto_minus() {
    let mode = definitions::WrapMode::Expr("auto-5".to_string());
    assert_eq!(mode.resolve(100), 95);
}

#[test]
fn wrap_resolve_auto_plus() {
    let mode = definitions::WrapMode::Expr("auto+10".to_string());
    assert_eq!(mode.resolve(100), 110);
}

#[test]
fn wrap_resolve_auto_div() {
    let mode = definitions::WrapMode::Expr("auto/2".to_string());
    assert_eq!(mode.resolve(100), 50);
}

#[test]
fn wrap_resolve_auto_div_plus() {
    let mode = definitions::WrapMode::Expr("auto/2+5".to_string());
    assert_eq!(mode.resolve(100), 55);
}

#[test]
fn wrap_resolve_parens() {
    let mode = definitions::WrapMode::Expr("(auto-10)/2".to_string());
    assert_eq!(mode.resolve(100), 45);
}

#[test]
fn wrap_resolve_floors_to_one() {
    let mode = definitions::WrapMode::Expr("auto-200".to_string());
    assert_eq!(mode.resolve(10), 1);
}
