use super::*;

#[test]
fn u64_to_f64_is_exact_below_2_pow_53() {
    assert_eq!(u64_to_f64(0).to_bits(), 0.0_f64.to_bits());
    assert_eq!(u64_to_f64(1).to_bits(), 1.0_f64.to_bits());
    assert_eq!(
        u64_to_f64(9_007_199_254_740_992).to_bits(),
        9_007_199_254_740_992.0_f64.to_bits()
    );
    assert_eq!(
        u64_to_f64(4_294_967_297).to_bits(),
        4_294_967_297.0_f64.to_bits()
    );
}

#[test]
fn u64_to_f64_rounds_ties_to_even_like_a_cast() {
    // 2^53 + 1 is a tie between 2^53 and 2^53 + 2; even mantissa wins.
    assert_eq!(
        u64_to_f64(9_007_199_254_740_993).to_bits(),
        9_007_199_254_740_992.0_f64.to_bits()
    );
    // 2^53 + 3 ties between 2^53 + 2 and 2^53 + 4; even mantissa is 2^53 + 4.
    assert_eq!(
        u64_to_f64(9_007_199_254_740_995).to_bits(),
        9_007_199_254_740_996.0_f64.to_bits()
    );
}

#[test]
fn u64_to_f64_max_rounds_up_to_2_pow_64() {
    assert_eq!(
        u64_to_f64(u64::MAX).to_bits(),
        18_446_744_073_709_551_616.0_f64.to_bits()
    );
}

#[test]
fn i64_to_f64_keeps_sign_and_handles_min() {
    assert_eq!(i64_to_f64(-5).to_bits(), (-5.0_f64).to_bits());
    assert_eq!(i64_to_f64(5).to_bits(), 5.0_f64.to_bits());
    assert_eq!(
        i64_to_f64(i64::MIN).to_bits(),
        (-9_223_372_036_854_775_808.0_f64).to_bits()
    );
}

#[test]
fn f64_to_u64_truncates_toward_zero() {
    assert_eq!(f64_to_u64(0.0), 0);
    assert_eq!(f64_to_u64(0.999), 0);
    assert_eq!(f64_to_u64(1.0), 1);
    assert_eq!(f64_to_u64(1.999), 1);
    assert_eq!(f64_to_u64(4_294_967_296.5), 4_294_967_296);
}

#[test]
fn f64_to_u64_rejects_nan_and_negatives() {
    assert_eq!(f64_to_u64(f64::NAN), 0);
    assert_eq!(f64_to_u64(-1.0), 0);
    assert_eq!(f64_to_u64(f64::NEG_INFINITY), 0);
}

#[test]
fn f64_to_u64_is_exact_across_the_2_pow_53_boundary() {
    assert_eq!(f64_to_u64(9_007_199_254_740_992.0), 9_007_199_254_740_992);
    assert_eq!(f64_to_u64(9_007_199_254_740_994.0), 9_007_199_254_740_994);
    assert_eq!(
        f64_to_u64(1_152_921_504_606_846_976.0),
        1_152_921_504_606_846_976
    );
}

#[test]
fn f64_to_u64_saturates_at_the_top() {
    assert_eq!(
        f64_to_u64(9_223_372_036_854_775_808.0),
        9_223_372_036_854_775_808
    );
    assert_eq!(f64_to_u64(18_446_744_073_709_551_616.0), u64::MAX);
    assert_eq!(f64_to_u64(f64::INFINITY), u64::MAX);
}

#[test]
fn f64_to_i64_truncates_and_saturates_both_ways() {
    assert_eq!(f64_to_i64(2.9), 2);
    assert_eq!(f64_to_i64(-2.9), -2);
    assert_eq!(f64_to_i64(-0.5), 0);
    assert_eq!(f64_to_i64(f64::NAN), 0);
    assert_eq!(f64_to_i64(1e30), i64::MAX);
    assert_eq!(f64_to_i64(-1e30), i64::MIN);
    assert_eq!(f64_to_i64(-9_223_372_036_854_775_808.0), i64::MIN);
    assert_eq!(f64_to_i64(f64::NEG_INFINITY), i64::MIN);
}

#[test]
fn f64_to_usize_truncates_and_clamps_negatives() {
    assert_eq!(f64_to_usize(7.9), 7);
    assert_eq!(f64_to_usize(-3.0), 0);
    assert_eq!(f64_to_usize(f64::NAN), 0);
}
