use super::*;
#[test]
fn ddmin_finds_the_minimal_cause_in_a_haystack() {
    let keys: Vec<Key> = "ab13x94zqw".chars().map(Key::Char).collect();
    let reproduces = |ks: &[Key]| ks.iter().any(|k| *k == Key::Char('x'));
    let minimal = ddmin(keys, reproduces);
    assert_eq!(minimal, vec![Key::Char('x')]);
}

#[test]
fn ddmin_keeps_a_pair_that_must_appear_together() {
    let keys: Vec<Key> = "12ab34".chars().map(Key::Char).collect();
    let reproduces = |ks: &[Key]| ks.windows(2).any(|w| w == [Key::Char('a'), Key::Char('b')]);

    let minimal = ddmin(keys, reproduces);

    assert_eq!(minimal, vec![Key::Char('a'), Key::Char('b')]);
}
