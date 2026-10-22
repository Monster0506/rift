//! Lossy numeric conversions with explicit, saturating semantics. They use
//! integer arithmetic and `From`/`TryFrom` only, so call sites need no `as`.

const WORD_BITS: u32 = 32;
const TWO_POW_WORD: f64 = 4_294_967_296.0;
const MANTISSA_BITS: u32 = 52;
const MANTISSA_MASK: u64 = (1 << MANTISSA_BITS) - 1;
const IMPLICIT_BIT: u64 = 1 << MANTISSA_BITS;
const EXPONENT_MASK: u32 = 0x7FF;
const EXPONENT_BIAS_SHIFTED: u32 = 1075;
const MAX_LEFT_SHIFT: u32 = u64::BITS - MANTISSA_BITS - 1;

/// Nearest `f64` to `v` (ties to even), identical to `v as f64`.
#[must_use]
pub fn u64_to_f64(v: u64) -> f64 {
    let hi = u32::try_from(v >> WORD_BITS).unwrap_or(u32::MAX);
    let lo = u32::try_from(v & u64::from(u32::MAX)).unwrap_or(u32::MAX);
    f64::from(hi).mul_add(TWO_POW_WORD, f64::from(lo))
}

/// Nearest `f64` to `v` (ties to even), identical to `v as f64`.
#[must_use]
pub(crate) fn usize_to_f64(v: usize) -> f64 {
    u64_to_f64(u64::try_from(v).unwrap_or(u64::MAX))
}

#[cfg(test)]
/// Nearest `f64` to `v` (ties to even), identical to `v as f64`.
#[must_use]
pub(crate) fn i64_to_f64(v: i64) -> f64 {
    let magnitude = u64_to_f64(v.unsigned_abs());
    if v < 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// Truncates toward zero. NaN and values below 1 give 0; values past
/// `u64::MAX` (including infinity) saturate.
#[must_use]
pub(crate) fn f64_to_u64(x: f64) -> u64 {
    if x.is_nan() || x < 1.0 {
        return 0;
    }

    let bits = x.to_bits();
    let exponent = u32::try_from((bits >> MANTISSA_BITS) & u64::from(EXPONENT_MASK)).unwrap_or(0);
    let mantissa = (bits & MANTISSA_MASK) | IMPLICIT_BIT;

    if exponent >= EXPONENT_BIAS_SHIFTED {
        let shift = exponent - EXPONENT_BIAS_SHIFTED;
        if shift > MAX_LEFT_SHIFT {
            return u64::MAX;
        }
        return mantissa << shift;
    }
    mantissa >> (EXPONENT_BIAS_SHIFTED - exponent)
}

/// Truncates toward zero, saturating at the `i64` range. NaN gives 0.
#[must_use]
pub(crate) fn f64_to_i64(x: f64) -> i64 {
    if x.is_nan() {
        return 0;
    }
    if x < 0.0 {
        let magnitude = f64_to_u64(-x);
        return i64::try_from(magnitude).map_or(i64::MIN, |v| -v);
    }
    i64::try_from(f64_to_u64(x)).unwrap_or(i64::MAX)
}

/// Truncates toward zero. NaN and negatives give 0; saturates at `usize::MAX`.
#[must_use]
pub(crate) fn f64_to_usize(x: f64) -> usize {
    usize::try_from(f64_to_u64(x)).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
