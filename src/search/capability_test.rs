use super::check_complexity;

#[test]
fn test_complexity_check() {
    // Simple patterns should be false
    assert!(!check_complexity("foo"), "Simple string should be simple");
    assert!(
        !check_complexity("^foo"),
        "Anchored string should be simple"
    );

    // .* is broad
    assert!(check_complexity(".*"), ".* should be complex");
    assert!(check_complexity("foo.*"), "foo.* should be complex");

    // Complex bad pattern from benchmark
    assert!(
        check_complexity("fn.*\\n.*return"),
        "Benchmark pattern should be complex"
    );

    // Bounded range
    assert!(
        !check_complexity(".{0,10}"),
        "Bounded range should be simple"
    );

    // Unbounded range
    assert!(
        check_complexity(".{0,}"),
        "Unbounded range should be complex"
    );

    // Broad inside OneOrMore
    assert!(check_complexity(".+"), ".+ should be complex");
}
