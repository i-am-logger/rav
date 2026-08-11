//! The floating-point operations `core` does not have.
//!
//! `f32::floor`, `round`, `log10` and `powf` are `std` methods, not `core` ones,
//! so a `no_std` crate cannot call them however ordinary they look. They are
//! gathered here rather than spread as `libm::floorf` calls through the
//! arithmetic, so the call sites still read as maths and the dependency is
//! visible in one place.
//!
//! Same results as the `std` methods on every value rav produces; `libm` is the
//! rust-lang port of musl's, which is what `std` itself uses on many targets.

/// Toward negative infinity.
pub(crate) fn floor(value: f32) -> f32 {
    libm::floorf(value)
}

/// To the nearest, halfway away from zero - the same rule `f32::round` uses.
pub(crate) fn round(value: f32) -> f32 {
    libm::roundf(value)
}

/// Toward positive infinity.
pub(crate) fn ceil(value: f32) -> f32 {
    libm::ceilf(value)
}

/// Base-ten logarithm. Only [`crate::units::Curve::Decibel`] needs it.
pub(crate) fn log10(value: f32) -> f32 {
    libm::log10f(value)
}

/// Raise to a power. Only [`crate::units::Curve::Gamma`] needs it.
pub(crate) fn powf(value: f32, exponent: f32) -> f32 {
    libm::powf(value, exponent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn these_agree_with_the_std_methods_they_stand_in_for() {
        // The reason to check: a `no_std` crate cannot call the originals, so
        // nothing else would notice if one of these were subtly different.
        for value in [-2.5f32, -0.5, 0.0, 0.4, 0.5, 0.6, 1.5, 2.5, 60.0, 1023.75] {
            assert_eq!(floor(value), f32::floor(value), "floor({value})");
            assert_eq!(round(value), f32::round(value), "round({value})");
            assert_eq!(ceil(value), f32::ceil(value), "ceil({value})");
        }
        for value in [0.001f32, 0.5, 1.0, 10.0, 1000.0] {
            let mine = log10(value);
            assert!(
                (mine - f32::log10(value)).abs() < 1e-6,
                "log10({value}): {mine}"
            );
        }
        for (value, exponent) in [(0.25f32, 0.5f32), (2.0, 10.0), (0.5, 2.0)] {
            let mine = powf(value, exponent);
            assert!(
                (mine - f32::powf(value, exponent)).abs() < 1e-6,
                "powf({value}, {exponent}): {mine}"
            );
        }
    }
}
