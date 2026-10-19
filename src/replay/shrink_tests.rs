use super::*;
#[test]
fn ddmin_finds_the_minimal_cause_in_a_haystack() {
    let keys: Vec<Key> = "ab13x94zqw".chars().map(Key::char).collect();
    let reproduces = |ks: &[Key]| ks.contains(&Key::char('x'));
    let minimal = ddmin(keys, reproduces);
    assert_eq!(minimal, vec![Key::char('x')]);
}

#[test]
fn ddmin_keeps_a_pair_that_must_appear_together() {
    let keys: Vec<Key> = "12ab34".chars().map(Key::char).collect();
    let reproduces = |ks: &[Key]| ks.windows(2).any(|w| w == [Key::char('a'), Key::char('b')]);

    let minimal = ddmin(keys, reproduces);

    assert_eq!(minimal, vec![Key::char('a'), Key::char('b')]);
}
