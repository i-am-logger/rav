//! The quantities rav works in, each with its own type.
//!
//! A bar's height, a distance across the display, a count of terminal cells and
//! a frequency are four different things, and as bare `f32`s and `u16`s the
//! compiler cannot tell them apart. That is not hypothetical tidiness: the
//! layout takes a bar width that means columns on one surface and pixels on
//! another, and passing the wrong one produces a picture that is wrong rather
//! than a build that fails.
//!
//! Every type here is `repr(transparent)` and `Copy`, so this costs nothing at
//! runtime - which is what keeps it usable in a core meant to run on a
//! microcontroller driving an LED strip, where a bar per column is the same
//! problem at 240 lights as at 2400 pixels.
//!
//! Conversions are deliberately explicit. [`Cells`] does not become [`Length`]
//! on its own; it takes a [`CellSize`], because the only correct answer depends
//! on what the terminal reports and guessing it is how issue #63 began.

use std::ops::{Add, Div, Mul, Sub};

/// How loud a band is, as a fraction of full scale: always `0..=1`.
///
/// The output of the ballistics and the input to every surface. Constructing one
/// clamps, so the range holds by construction rather than by each caller
/// remembering - which today they do not always do.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(transparent)]
pub struct Level(f32);

impl Level {
    pub const SILENT: Self = Self(0.0);
    pub const FULL: Self = Self(1.0);

    /// Clamps, and treats NaN as silence.
    ///
    /// A NaN reaching the renderer would propagate into an area and out to the
    /// rasteriser, where it draws nothing and explains nothing. Silence is the
    /// reading a broken magnitude deserves.
    pub fn new(value: f32) -> Self {
        if value.is_nan() {
            Self::SILENT
        } else {
            Self(value.clamp(0.0, 1.0))
        }
    }

    pub const fn fraction(self) -> f32 {
        self.0
    }

    pub fn is_silent(self) -> bool {
        self.0 <= 0.0
    }
}

/// A distance across the display.
///
/// Device pixels in a terminal or a window, and one light on an LED strip. What
/// matters is that it is *not* a count of cells: a cell is made of these rather
/// than measured in them.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(transparent)]
pub struct Length(pub f32);

impl Length {
    pub const NONE: Self = Self(0.0);

    pub const fn get(self) -> f32 {
        self.0
    }

    /// Negative distances are not a thing rav has a use for.
    pub fn or_none(self) -> Self {
        Self(self.0.max(0.0))
    }

    pub fn largest(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    pub fn smallest(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// Held between two bounds, collapsing rather than panicking when they
    /// cross - which a zero-height display produces honestly, since a cap then
    /// has nowhere to go. `f32::clamp` panics in that case.
    pub fn held_between(self, low: Self, high: Self) -> Self {
        if low.0 > high.0 {
            low
        } else {
            Self(self.0.clamp(low.0, high.0))
        }
    }

    /// The fraction of `whole` that this covers, `0..=1`.
    pub fn fraction_of(self, whole: Self) -> f32 {
        if whole.0 <= 0.0 {
            0.0
        } else {
            (self.0 / whole.0).clamp(0.0, 1.0)
        }
    }

    /// How many whole lights this covers, for sizing a buffer.
    pub fn rounded_up(self) -> u32 {
        self.0.max(0.0).ceil() as u32
    }
}

impl Add for Length {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Length {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

/// Scaling a distance by a bare number is meaningful; adding one is not, which
/// is why only these two exist.
impl Mul<f32> for Length {
    type Output = Self;
    fn mul(self, factor: f32) -> Self {
        Self(self.0 * factor)
    }
}

/// A distance scaled by a level - the operation every bar height is.
impl Mul<Level> for Length {
    type Output = Self;
    fn mul(self, level: Level) -> Self {
        Self(self.0 * level.fraction())
    }
}

impl Div<f32> for Length {
    type Output = Self;
    fn div(self, divisor: f32) -> Self {
        Self(self.0 / divisor)
    }
}

/// A count of terminal cells - columns or rows.
///
/// The glyph surface's unit, and the one the pixel surfaces must never inherit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Cells(pub u16);

impl Cells {
    pub const NONE: Self = Self(0);

    pub const fn count(self) -> u16 {
        self.0
    }

    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// The size of one terminal cell, as the terminal reports it.
///
/// The only bridge between [`Cells`] and [`Length`], and it must come from
/// `TIOCGWINSZ` rather than from a font size and a DPI guess: WezTerm draws
/// block glyphs itself at 72 DPI on macOS and 96 elsewhere, so the same
/// configuration gives different cells on different machines. That is the
/// mechanism behind #63.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    pub width: u16,
    pub height: u16,
}

impl CellSize {
    pub fn across(self, cells: Cells) -> Length {
        Length(f32::from(cells.count()) * f32::from(self.width))
    }

    pub fn down(self, cells: Cells) -> Length {
        Length(f32::from(cells.count()) * f32::from(self.height))
    }
}

/// A frequency, in hertz.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Hz(pub f32);

impl Hz {
    pub const fn get(self) -> f32 {
        self.0
    }
}

/// Samples per second.
///
/// Distinct from a buffer length, which is also a count of samples and is what
/// it gets confused with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SampleRate(pub u32);

impl SampleRate {
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The highest frequency this rate can represent.
    pub fn nyquist(self) -> Hz {
        Hz(self.0 as f32 / 2.0)
    }
}

/// Time since the previous frame.
///
/// The ballistics are framerate-independent and take a bare `f32` that must be
/// seconds; handing them milliseconds compiles and makes the bars fall a
/// thousand times too slowly.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(transparent)]
pub struct Elapsed(f32);

impl Elapsed {
    /// Negative time would run the ballistics backwards, and a clock that jumps
    /// is a real thing rather than a hypothetical one.
    pub fn seconds(value: f32) -> Self {
        Self(if value.is_nan() { 0.0 } else { value.max(0.0) })
    }

    pub fn since(then: std::time::Instant) -> Self {
        Self(then.elapsed().as_secs_f32())
    }

    pub const fn as_seconds(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_is_always_in_range() {
        assert_eq!(Level::new(-1.0), Level::SILENT);
        assert_eq!(Level::new(2.0), Level::FULL);
        assert_eq!(Level::new(0.5).fraction(), 0.5);
    }

    #[test]
    fn a_nan_level_reads_as_silence() {
        // Not merely tidy: a NaN reaches the rasteriser as an area that draws
        // nothing, and nothing about the resulting picture says why.
        assert_eq!(Level::new(f32::NAN), Level::SILENT);
    }

    #[test]
    fn a_length_scaled_by_a_level_is_how_tall_that_bar_stands() {
        assert_eq!(Length(60.0) * Level::new(0.5), Length(30.0));
        assert_eq!(Length(60.0) * Level::SILENT, Length::NONE);
        assert_eq!(Length(60.0) * Level::FULL, Length(60.0));
    }

    #[test]
    fn holding_a_length_between_crossed_bounds_does_not_panic() {
        // `f32::clamp` panics when its bounds cross, which a zero-height display
        // produces honestly - the cap has nowhere to go.
        assert_eq!(
            Length(5.0).held_between(Length(10.0), Length::NONE),
            Length(10.0)
        );
    }

    #[test]
    fn a_fraction_of_nothing_is_nothing_rather_than_a_division_by_zero() {
        assert_eq!(Length(5.0).fraction_of(Length::NONE), 0.0);
        assert_eq!(Length(30.0).fraction_of(Length(60.0)), 0.5);
        // Held in range, so a length past the whole cannot index past a ramp.
        assert_eq!(Length(90.0).fraction_of(Length(60.0)), 1.0);
    }

    #[test]
    fn cells_become_lengths_only_through_a_cell_size() {
        let cell = CellSize {
            width: 30,
            height: 60,
        };
        assert_eq!(cell.across(Cells(3)), Length(90.0));
        assert_eq!(cell.down(Cells(3)), Length(180.0));
    }

    #[test]
    fn a_sample_rate_knows_its_nyquist() {
        assert_eq!(SampleRate(48_000).nyquist(), Hz(24_000.0));
    }

    #[test]
    fn time_never_runs_backwards() {
        assert_eq!(Elapsed::seconds(-1.0).as_seconds(), 0.0);
        assert_eq!(Elapsed::seconds(f32::NAN).as_seconds(), 0.0);
    }

    #[test]
    fn lengths_add_and_scale_but_cells_do_not_convert_themselves() {
        assert_eq!(Length(2.0) + Length(3.0), Length(5.0));
        assert_eq!(Length(6.0) - Length(2.0), Length(4.0));
        assert_eq!(Length(2.0) * 3.0, Length(6.0));
        assert_eq!(Length(6.0) / 2.0, Length(3.0));
        // `Cells` has no arithmetic with `Length` at all, which is the point:
        // the absence of a conversion is the invariant.
        assert_eq!(Cells(4).count(), 4);
    }
}
