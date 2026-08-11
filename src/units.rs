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

// `core`, not `std`: nothing in this module may need an allocator, a clock or an
// operating system. That is what makes it usable in a core meant to run on a
// microcontroller, and the import is where it would first be broken.
use core::ops::{Add, Div, Mul, Sub};

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

    /// Which of `count` steps this level is nearest.
    ///
    /// What a colour ramp asks: a height picks the stop closest to it, which is
    /// why the first and last stops hold only half a stripe each. Rounding
    /// rather than truncating is the rule the terminal renderer already uses,
    /// and changing it would move every colour boundary in the picture.
    pub fn nearest_step(self, count: usize) -> Step {
        let last = count.saturating_sub(1);
        Step::new((self.0 * last as f32).round() as usize, count)
    }

    /// How far this level fills a ladder of `rungs`, each divisible into `steps`.
    ///
    /// What a glyph ladder asks: how many whole cells are lit, and how full the
    /// one above them is. Truncating rather than rounding, because a cell is
    /// only as lit as the signal actually reached.
    pub fn fill(self, rungs: usize, steps: usize) -> Fill {
        if rungs == 0 || steps == 0 {
            return Fill {
                whole: 0,
                part: Step::new(0, steps.max(1)),
            };
        }
        let total = self.0 * (rungs * steps) as f32;
        let filled = (total.floor() as usize).min(rungs * steps);
        Fill {
            whole: filled / steps,
            part: Step::new(filled % steps, steps),
        }
    }
}

/// One of a fixed number of steps a skin or a theme offers.
///
/// The whole of what the core knows about either: **how many, and which one -
/// never what any of them look like.** A skin says it has eight steps; the core
/// answers "step five" and the terminal surface turns that into `▅`, the pixel
/// surface into a shape, an LED strip into a brightness. A theme says its ramp
/// has sixteen stops; the core answers "stop eleven" and each surface resolves
/// it in the only way it can.
///
/// That boundary is what keeps `terminal` and `mono` working: those themes exist
/// to defer the colour to whatever palette the user runs, which is impossible if
/// the core has already decided it. It is also what lets a skin be swapped
/// without touching any arithmetic - a sixteen-step skin and an eight-step one
/// differ here by one integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    index: usize,
    count: usize,
}

impl Step {
    /// Held inside the range, so an index can never address past a skin's
    /// glyphs or a theme's stops however it was arrived at.
    pub fn new(index: usize, count: usize) -> Self {
        let count = count.max(1);
        Self {
            index: index.min(count - 1),
            count,
        }
    }

    pub const fn index(self) -> usize {
        self.index
    }

    pub const fn count(self) -> usize {
        self.count
    }

    pub fn is_first(self) -> bool {
        self.index == 0
    }

    pub fn is_last(self) -> bool {
        self.index + 1 == self.count
    }

    /// The same position expressed against a different number of steps.
    ///
    /// How one setting drives surfaces that disagree about resolution: an
    /// eight-step glyph ladder and a two-hundred-step LED strip are the same
    /// fraction at different granularities.
    pub fn rescaled(self, count: usize) -> Self {
        if self.count <= 1 {
            return Self::new(0, count);
        }
        let fraction = self.index as f32 / (self.count - 1) as f32;
        Self::new(
            (fraction * count.max(1).saturating_sub(1) as f32).round() as usize,
            count,
        )
    }
}

/// How far a level fills a ladder: whole rungs, plus how full the next one is.
///
/// A bar in a terminal is this - three whole rows lit and the fourth five
/// eighths full - without the core ever knowing what an eighth looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fill {
    /// Rungs completely filled.
    pub whole: usize,
    /// How far into the rung above them.
    pub part: Step,
}

impl Fill {
    /// Whether the rung above the whole ones carries anything at all.
    pub fn has_part(self) -> bool {
        !self.part.is_first()
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

    /// From a duration, for a caller that has a clock.
    ///
    /// Deliberately takes the duration rather than reading a clock itself. A
    /// clock is the one thing a core cannot have: an embedded target may not
    /// have `std::time` at all, and a test needs to state the elapsed time
    /// rather than wait for it.
    pub fn from_secs_f32(seconds: f32) -> Self {
        Self::seconds(seconds)
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
    fn a_level_picks_the_nearest_stop() {
        // Rounding, not truncating: the rule the terminal renderer already uses,
        // and the reason a ramp's first and last stops hold half a stripe each.
        assert_eq!(Level::SILENT.nearest_step(3).index(), 0);
        assert_eq!(Level::new(0.5).nearest_step(3).index(), 1);
        assert_eq!(Level::FULL.nearest_step(3).index(), 2);
        // Just past the halfway point of the first stop, it tips to the second.
        assert_eq!(Level::new(0.26).nearest_step(3).index(), 1);
        assert_eq!(Level::new(0.24).nearest_step(3).index(), 0);
    }

    #[test]
    fn a_step_can_never_address_past_what_a_skin_offers() {
        // The invariant that makes swapping a skin safe: however an index was
        // arrived at, it indexes something that exists.
        assert_eq!(Step::new(99, 8).index(), 7);
        assert_eq!(Step::new(0, 0).count(), 1, "a skin always has one step");
        assert!(Step::new(7, 8).is_last());
        assert!(Step::new(0, 8).is_first());
    }

    #[test]
    fn a_level_fills_whole_rungs_and_part_of_the_next() {
        // A bar in a terminal: three rows lit and the fourth part-full, with the
        // core never knowing what an eighth looks like.
        let fill = Level::new(0.5).fill(8, 8);
        assert_eq!(fill.whole, 4, "half of eight rows");
        assert!(
            !fill.has_part(),
            "landing on a boundary leaves no remainder"
        );

        // One step of one of eight rows is 1/64, so 0.51 still floors onto the
        // boundary and 0.55 is the first level that lights part of the next.
        let fill = Level::new(0.51).fill(8, 8);
        assert!(!fill.has_part(), "0.51 is still short of a whole step");
        let fill = Level::new(0.55).fill(8, 8);
        assert_eq!(fill.whole, 4);
        assert!(fill.has_part(), "past the boundary lights the next rung");

        // Truncating, not rounding: a cell is only as lit as the signal reached.
        assert_eq!(Level::new(0.99).fill(1, 8).part.index(), 7);
        assert_eq!(Level::FULL.fill(1, 8).whole, 1);
        assert_eq!(Level::SILENT.fill(8, 8).whole, 0);
    }

    #[test]
    fn a_ladder_with_no_rungs_is_not_a_division_by_zero() {
        assert_eq!(Level::FULL.fill(0, 8).whole, 0);
        assert_eq!(Level::FULL.fill(8, 0).whole, 0);
    }

    #[test]
    fn a_step_rescales_between_surfaces_that_disagree_on_resolution() {
        // One setting driving an eight-step glyph ladder and a long LED strip:
        // the same fraction, at whatever granularity each can manage.
        assert_eq!(Step::new(7, 8).rescaled(200).index(), 199);
        assert_eq!(Step::new(0, 8).rescaled(200).index(), 0);
        assert_eq!(Step::new(4, 9).rescaled(9).index(), 4, "a no-op rescale");
        // Coarsening loses precision without ever escaping the range.
        assert!(Step::new(150, 200).rescaled(8).index() < 8);
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
