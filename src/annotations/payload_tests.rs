use super::*;

#[test]
fn tooltip_reads_string_field() {
    let mut p = Value::map();
    p.set("tooltip", Value::Str("boom".into()));
    assert_eq!(tooltip(&p), Some("boom"));
    assert_eq!(tooltip(&Value::Null), None);
}

#[test]
fn fs_entry_id_reads_int_field() {
    let mut p = Value::map();
    p.set("entry_id", Value::Int(42));
    assert_eq!(fs::entry_id(&p), Some(42));
    assert_eq!(fs::entry_id(&Value::map()), None);
}

#[test]
fn lsp_severity_and_message() {
    let mut p = Value::map();
    p.set("severity", Value::Int(1));
    p.set("message", Value::Str("type mismatch".into()));
    assert_eq!(lsp::severity(&p), Some(1));
    assert_eq!(lsp::message(&p), Some("type mismatch"));
}
