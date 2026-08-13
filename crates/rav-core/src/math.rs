//! Arithmetic every surface needs, and the operations `core` does not have.
//!
//! Two things live here, and the difference matters.
//!
//! **The shims.** `f32::floor`, `round`, `ceil`, `log10` and `powf` are `std`
//! methods, not `core` ones, so a `no_std` crate cannot call them however
//! ordinary they look. They are gathered here rather than spread as
//! `libm::floorf` calls through the arithmetic, so the call sites still read as
//! maths and the dependency is visible in one place. Same results as the `std`
//! methods on every value rav produces; `libm` is the rust-lang port of musl's,
//! which is what `std` itself uses on many targets.
//!
//! **The equations.** Arithmetic that is the same on a terminal, a window and a
//! strip of LEDs, and that a microcontroller needs verbatim. The bar is high:
//! it must be pure, allocate nothing, and answer a question nothing else in
//! this crate already answers. A survey of all sixty-one equations in rav
//! proposed twenty-seven for this module and twenty-five turned out to be
//! duplicates of [`crate::units`] or [`crate::geometry`] - which is the outcome
//! the crate boundary exists to prevent, not one it should collect.
//!
//! Everything else stays where it is used. A rate written per frame, a ramp's
//! stripe placement, how much of the width a display spends on the bass: those
//! are judgements about a picture, and this crate answers how many and which
//! one, never what anything looks like.

/// Toward negative infinity.
pub fn floor(value: f32) -> f32 {
    libm::floorf(value)
}

/// To the nearest, halfway away from zero - the same rule `f32::round` uses.
pub fn round(value: f32) -> f32 {
    libm::roundf(value)
}

/// Toward positive infinity.
pub fn ceil(value: f32) -> f32 {
    libm::ceilf(value)
}

/// Base-ten logarithm. Only [`crate::units::Curve::Decibel`] needs it.
pub fn log10(value: f32) -> f32 {
    libm::log10f(value)
}

/// Raise to a power. Only [`crate::units::Curve::Gamma`] needs it.
pub fn powf(value: f32, exponent: f32) -> f32 {
    libm::powf(value, exponent)
}

/// Sine, in radians. Only [`crate::transform`] needs it, for the rotations.
pub fn sin(radians: f32) -> f32 {
    libm::sinf(radians)
}

/// Cosine, in radians. The other half of a rotation.
pub fn cos(radians: f32) -> f32 {
    libm::cosf(radians)
}

/// How far something travels when its speed is multiplied by `growth` every
/// frame, in units of the speed it started at.
///
/// A peak cap does not fall at a steady rate. Its velocity compounds, so the
/// distance it covers is the sum of a geometric series and not speed times
/// frames. Writing it the obvious way gives motion that is nearly right at the
/// rate it was tuned against and visibly wrong at any other, which is the kind
/// of mistake that survives being looked at.
///
/// `growth` of 1 is a steady speed, where the series degenerates to the frame
/// count - handled directly, because the closed form divides by zero there.
///
/// ```
/// use rav_core::{math::compounded_travel, Frames};
/// // Steady speed for ten frames covers ten frames' worth of ground.
/// assert!((compounded_travel(1.0, Frames::count(10.0)) - 10.0).abs() < 1e-4);
/// // Speed doubling each frame covers 1 + 2 + 4 + 8 over four.
/// assert!((compounded_travel(2.0, Frames::count(4.0)) - 15.0).abs() < 1e-4);
/// ```
pub fn compounded_travel(growth: f32, frames: crate::units::Frames) -> f32 {
    let frames = frames.get();
    if (growth - 1.0).abs() < f32::EPSILON {
        return frames;
    }
    (powf(growth, frames) - 1.0) / (growth - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Frames;

    #[test]
    fn a_compounding_speed_covers_the_sum_of_the_series() {
        // The whole reason this is not speed times frames.
        assert!((compounded_travel(2.0, Frames::count(1.0)) - 1.0).abs() < 1e-4);
        assert!((compounded_travel(2.0, Frames::count(3.0)) - 7.0).abs() < 1e-4);
        assert!((compounded_travel(3.0, Frames::count(3.0)) - 13.0).abs() < 1e-4);
        assert_eq!(compounded_travel(2.0, Frames::NONE), 0.0);
    }

    #[test]
    fn a_steady_speed_is_just_the_frame_count() {
        // The closed form divides by zero at a growth of one, and a rate that
        // does not compound is a perfectly ordinary thing to ask for.
        for frames in [0.0f32, 1.0, 7.5, 240.0] {
            assert_eq!(compounded_travel(1.0, Frames::count(frames)), frames);
        }
    }

    #[test]
    fn cutting_the_frames_finer_covers_the_same_ground() {
        // What makes a rate written per-frame survive a different display. Ten
        // frames in one step and ten frames in twenty must arrive together, or
        // the same music falls at different speeds on different machines.
        let whole = compounded_travel(1.1, Frames::count(10.0));
        let mut piecewise = 0.0;
        let mut speed = 1.0f32;
        for _ in 0..20 {
            piecewise += speed * compounded_travel(1.1, Frames::count(0.5));
            speed *= powf(1.1, 0.5);
        }
        assert!(
            (whole - piecewise).abs() < 1e-3,
            "{whole} in one step, {piecewise} in twenty",
        );
    }

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
