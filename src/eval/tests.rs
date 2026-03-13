use super::*;

fn no_kw(_: &str) -> Option<usize> {
    None
}

fn with_auto(kw: &str) -> Option<usize> {
    if kw == "auto" { Some(100) } else { None }
}

#[test]
fn literal() {
    assert_eq!(eval("80", &no_kw), Ok(80));
}

#[test]
fn keyword() {
    assert_eq!(eval("auto", &with_auto), Ok(100));
}

#[test]
fn expr_auto_div() {
    assert_eq!(eval("auto / 2", &with_auto), Ok(50));
}

#[test]
fn expr_auto_div_plus() {
    assert_eq!(eval("auto / 2 + 5", &with_auto), Ok(55));
}

#[test]
fn expr_precedence() {
    assert_eq!(eval("2 + 3 * 4", &no_kw), Ok(14));
}

#[test]
fn expr_parens() {
    assert_eq!(eval("(2 + 3) * 4", &no_kw), Ok(20));
}

#[test]
fn expr_auto_minus() {
    assert_eq!(eval("auto - 5", &with_auto), Ok(95));
}

#[test]
fn div_by_zero() {
    assert!(eval("auto / 0", &with_auto).is_err());
}

#[test]
fn unknown_keyword() {
    assert!(eval("foo + 1", &no_kw).is_err());
}

#[test]
fn unexpected_char() {
    assert!(eval("auto @ 2", &with_auto).is_err());
}

#[test]
fn missing_closing_paren() {
    assert!(eval("(auto + 1", &with_auto).is_err());
}

#[test]
fn empty_input() {
    assert!(eval("", &no_kw).is_err());
}
