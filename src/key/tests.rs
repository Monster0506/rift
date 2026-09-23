use super::*;

#[test]
fn lt_escapes_to_literal_char() {
    assert_eq!(parse_key_sequence("<lt>"), Some(vec![Key::Char('<')]));
}

#[test]
fn gt_needs_no_escaping() {
    assert_eq!(parse_key_sequence(">"), Some(vec![Key::Char('>')]));
}

#[test]
fn alt_and_alt_shift_notation_normalize_case() {
    assert_eq!(parse_key_sequence("<A-P>"), Some(vec![Key::Alt(b'p')]));
    assert_eq!(
        parse_key_sequence("<A-S-P>"),
        Some(vec![Key::AltShift(b'p')])
    );
}

#[test]
fn bare_lt_without_closing_gt_is_invalid() {
    assert_eq!(parse_key_sequence("<"), None);
}
