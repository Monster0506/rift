use super::*;

#[test]
fn namespace_extracts_leading_segment() {
    assert_eq!(Kind::new("lsp.diagnostic").namespace(), "lsp");
    assert_eq!(Kind::new("ui.checkbox").namespace(), "ui");
    assert_eq!(Kind::new("bare").namespace(), "bare");
}

#[test]
fn prefix_matching() {
    let k = Kind::new("lsp.diagnostic");
    assert!(k.matches_prefix("lsp."));
    assert!(k.matches_prefix("lsp.diagnostic"));
    assert!(k.matches_prefix("lsp"));
    assert!(!k.matches_prefix("git."));
    assert!(!k.matches_prefix("lsp.hint"));
}

#[test]
fn serde_is_transparent_string() {
    let k = Kind::new("ui.button");
    assert_eq!(serde_json::to_string(&k).unwrap(), r#""ui.button""#);
    let back: Kind = serde_json::from_str(r#""ui.button""#).unwrap();
    assert_eq!(back, k);
}
