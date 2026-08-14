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
        Step::new(crate::math::round(self.0 * last as f32) as usize, count)
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
        let filled = (crate::math::floor(total) as usize).min(rungs * steps);
        Fill {
            whole: filled / steps,
            part: Step::new(filled % steps, steps),
        }
    }
}

/// How a measured amplitude becomes a level to display.
///
/// rav measures linearly and always has: the magnitude is never put through a
/// logarithm, there is no dB window and no per-frame normalisation, and
/// `amplitude_response_is_linear_not_logarithmic` holds it there. That is the
/// right default - it is what makes a quiet passage read quiet.
///
/// It is also what a four-light bar cannot afford. Spread linearly, four rungs
/// have their boundaries at 0.25, 0.5 and 0.75 of full scale - which is -12, -6
/// and -2.5 dBFS. Three of the four lights live in the top 12 dB, and ordinary
/// music spends almost none of its time there, so the strip sits dark.
///
/// So the curve is a property something carries rather than a decision the
/// renderer makes. A desktop stays linear and faithful; a five-light strip on a
/// speaker compresses so its four boundaries land where the music actually is.
/// Both are rav, dressed for the hardware they are on.
///
/// **`no_std` note:** `Decibel` needs `log10` and `Gamma` needs `powf`, neither
/// of which is in `core`. An allocator-free target wants `libm`, or `Linear`,
/// which needs nothing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub enum Curve {
    /// The display *is* the amplitude. rav's default and its documented choice.
    #[default]
    Linear,
    /// Decibels across a window, so the range is spent where the music is.
    ///
    /// `floor` is the quietest amplitude that still lights anything, in dB below
    /// full scale, and must be negative. -48 is a reasonable window for a small
    /// strip; -60 is generous; -24 is aggressive.
    Decibel { floor: f32 },
    /// A power curve. Below 1 it lifts the quiet end, above 1 it flattens it.
    ///
    /// Cheaper than `Decibel` and has no floor to choose, at the cost of not
    /// meaning anything an audio engineer would recognise.
    Gamma(f32),
}

impl Curve {
    /// Map a measured amplitude to the level to draw.
    ///
    /// Monotonic in every form - louder never draws shorter - and silence always
    /// maps to silence, so a dead signal cannot light the display.
    pub fn apply(self, amplitude: Level) -> Level {
        let value = amplitude.fraction();
        match self {
            Self::Linear => amplitude,
            Self::Decibel { floor } => {
                // A floor at or above full scale has no range to map into, and
                // silence has no logarithm.
                if value <= 0.0 || floor >= 0.0 {
                    return Level::SILENT;
                }
                let db = 20.0 * crate::math::log10(value);
                Level::new((db - floor) / -floor)
            }
            Self::Gamma(exponent) => {
                if value <= 0.0 || exponent <= 0.0 {
                    return Level::SILENT;
                }
                Level::new(crate::math::powf(value, exponent))
            }
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
            crate::math::round(fraction * count.max(1).saturating_sub(1) as f32) as usize,
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

/// A count that refuses to be outside its range.
///
/// Rust has no bounded integers, so this is the newtype-with-a-checked-
/// constructor that stands in for one - with the bounds as const generics, so
/// the range is part of the type rather than a comment near it. Two different
/// ranges are two different types and cannot be swapped by accident.
///
/// **Refuses rather than clamps.** [`Level`] clamps because an out-of-range
/// magnitude is a measurement that overshot and silence is a sane reading of it.
/// A count is different: a caller asking for a two-light strip when the mode
/// needs four has made a mistake, and clamping to four would draw a picture that
/// misrepresents the hardware. `None` makes them deal with it.
///
/// ```
/// use rav_core::Bounded;
/// type Rungs = Bounded<4, 4096>;
/// assert!(Rungs::new(3).is_none(), "too few to say anything");
/// assert_eq!(Rungs::new(5).map(Bounded::get), Some(5));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Bounded<const MIN: usize, const MAX: usize>(usize);

impl<const MIN: usize, const MAX: usize> Bounded<MIN, MAX> {
    /// The smallest value this type admits.
    pub const MIN: usize = MIN;
    /// The largest.
    pub const MAX: usize = MAX;

    /// `None` when out of range, so an impossible count cannot be built at all.
    pub const fn new(value: usize) -> Option<Self> {
        if value < MIN || value > MAX {
            None
        } else {
            Some(Self(value))
        }
    }

    /// For a value known good at compile time - a literal in a const, or a
    /// bound this type already guarantees. Panics rather than returning, since
    /// there is no runtime to hand a `None` to.
    pub const fn known(value: usize) -> Self {
        assert!(value >= MIN && value <= MAX, "outside the bounds");
        Self(value)
    }

    /// The nearest admissible value.
    ///
    /// For the case where clamping genuinely is right - fitting a request to
    /// hardware that has what it has - so the choice is made at the call site
    /// with a name on it rather than by a constructor that always clamps.
    pub const fn nearest(value: usize) -> Self {
        if value < MIN {
            Self(MIN)
        } else if value > MAX {
            Self(MAX)
        } else {
            Self(value)
        }
    }

    pub const fn get(self) -> usize {
        self.0
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
        crate::math::ceil(self.0.max(0.0)) as u32
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

    /// This much time counted in frames of a display running at `fps`.
    ///
    /// The bridge every rate written per-frame has to cross. Constants copied
    /// from something that drew sixty times a second mean nothing until they
    /// are told how many of those frames have gone by.
    ///
    /// A rate below one a second gives a fraction of a frame, which is the
    /// honest answer; a nonsensical one gives none.
    pub fn frames_at(self, fps: f32) -> Frames {
        Frames::count(self.0 * fps)
    }
}

/// A count of frames of the display a rate was written against.
///
/// Not a count of frames rav drew, and not a duration. A cap's velocity is
/// multiplied by 1.1 "per frame", which is meaningless until it says *whose*
/// frames - and 1.1 per frame at 30 a second is not 1.1 per frame at 60, nor
/// 1.21. Keeping this apart from [`Elapsed`] is what stops a rate tuned on one
/// display being applied unchanged to another.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(transparent)]
pub struct Frames(f32);

impl Frames {
    pub const NONE: Self = Self(0.0);

    /// For a caller that genuinely has a frame count rather than a duration.
    pub fn count(value: f32) -> Self {
        Self(if value.is_nan() { 0.0 } else { value.max(0.0) })
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    pub fn is_none(self) -> bool {
        self.0 <= 0.0
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

    /// Enough rungs to say anything at all - the four-light bar is the floor.
    type Rungs = Bounded<4, 4096>;

    /// The amplitude of a signal `db` below full scale.
    ///
    /// Shares the 20 with [`Curve::Decibel`] on purpose - it is the same
    /// definition - which is exactly why it cannot be the only thing checking
    /// it. See `a_decibel_is_the_amplitude_kind_not_the_power_kind`.
    fn at_dbfs(db: f32) -> Level {
        Level::new(10f32.powf(db / 20.0))
    }

    #[test]
    fn a_decibel_is_the_amplitude_kind_not_the_power_kind() {
        // A level is an amplitude, so dBFS is 20*log10 of it. The power form,
        // 10*log10, halves every figure: a four-light strip's boundaries would
        // land at -6, -3 and -1.25 instead of the -12, -6 and -2.5 this type
        // documents, and every strip preset would be tuned against a window
        // twice the size it thought.
        //
        // Literal pairs on purpose. `at_dbfs` above shares the definition with
        // the implementation, so the two agree whatever the coefficient says -
        // change both together and nothing else notices.
        let window = Curve::Decibel { floor: -12.0 };

        // Full scale is 0 dBFS: the top of any window.
        assert!((window.apply(Level::FULL).fraction() - 1.0).abs() < 1e-3);

        // Half amplitude is -6.02 dBFS, which is halfway up a 12 dB window.
        let half = window.apply(Level::new(0.5)).fraction();
        assert!((half - 0.4983).abs() < 1e-3, "0.5 amplitude read {half}");

        // A quarter is -12.04 dBFS, which is under the floor and draws nothing.
        assert!(window.apply(Level::new(0.25)).fraction() < 0.01);
    }

    #[test]
    fn a_linear_curve_changes_nothing() {
        // rav's default and its documented choice: the display is the amplitude.
        for value in [0.0f32, 0.1, 0.5, 0.9, 1.0] {
            let level = Level::new(value);
            assert_eq!(Curve::Linear.apply(level), level);
        }
    }

    #[test]
    fn a_four_light_bar_is_nearly_all_headroom_until_a_curve_fixes_it() {
        // The finding this exists for. Four rungs spread linearly put their
        // boundaries at 0.25/0.5/0.75 of full scale - which is -12, -6 and
        // -2.5 dBFS - so three of four lights cover the top 12 dB and ordinary
        // music leaves the strip dark.
        let quiet = at_dbfs(-24.0);
        assert_eq!(
            Curve::Linear.apply(quiet).fill(4, 1).whole,
            0,
            "at -24 dBFS a linear four-light bar shows nothing at all"
        );

        // A window wide enough to hold the music lights half the bar instead.
        let compressed = Curve::Decibel { floor: -48.0 };
        assert_eq!(
            compressed.apply(quiet).fill(4, 1).whole,
            2,
            "-24 dBFS is halfway through a -48 dB window"
        );
    }

    #[test]
    fn a_decibel_curve_spends_its_range_where_the_music_is() {
        let curve = Curve::Decibel { floor: -48.0 };
        // The window's ends are the display's ends.
        assert_eq!(curve.apply(Level::FULL), Level::FULL, "0 dBFS is the top");
        assert_eq!(curve.apply(at_dbfs(-48.0)), Level::SILENT, "the floor");
        assert_eq!(curve.apply(at_dbfs(-60.0)), Level::SILENT, "below it too");
        // And the midpoint of the window is the midpoint of the display, which
        // is the whole point - linearly, -24 dBFS is 6% of the way up.
        let half = curve.apply(at_dbfs(-24.0)).fraction();
        assert!(
            (half - 0.5).abs() < 0.01,
            "-24 dB should be halfway, got {half}"
        );
    }

    #[test]
    fn every_curve_is_monotonic_and_keeps_silence_silent() {
        // Louder never draws shorter, and a dead signal never lights anything -
        // the two properties a display curve may not break however it is tuned.
        for curve in [
            Curve::Linear,
            Curve::Decibel { floor: -48.0 },
            Curve::Decibel { floor: -24.0 },
            Curve::Gamma(0.5),
            Curve::Gamma(2.0),
        ] {
            assert_eq!(curve.apply(Level::SILENT), Level::SILENT, "{curve:?}");
            assert_eq!(curve.apply(Level::FULL), Level::FULL, "{curve:?}");
            let mut previous = Level::SILENT;
            for step in 0..=100 {
                let drawn = curve.apply(Level::new(step as f32 / 100.0));
                assert!(drawn >= previous, "{curve:?} fell at step {step}");
                previous = drawn;
            }
        }
    }

    #[test]
    fn a_gamma_below_one_lifts_the_quiet_end() {
        let quiet = Level::new(0.25);
        assert!(Curve::Gamma(0.5).apply(quiet) > quiet, "lifted");
        assert!(Curve::Gamma(2.0).apply(quiet) < quiet, "flattened");
    }

    #[test]
    fn a_nonsensical_curve_draws_nothing_rather_than_misbehaving() {
        // A floor at or above full scale has no range to map into, and a
        // non-positive exponent is not a curve. Neither should reach the screen
        // as a NaN or an inverted display.
        assert_eq!(
            Curve::Decibel { floor: 0.0 }.apply(Level::new(0.5)),
            Level::SILENT
        );
        assert_eq!(Curve::Gamma(0.0).apply(Level::new(0.5)), Level::SILENT);
        assert_eq!(Curve::Gamma(-1.0).apply(Level::new(0.5)), Level::SILENT);
    }

    #[test]
    fn a_bounded_count_refuses_what_is_out_of_range() {
        // The point of the type: a three-light strip cannot be built where four
        // is the floor, so a mode never has to check at the point of drawing.
        assert_eq!(Rungs::new(3), None);
        assert_eq!(Rungs::new(0), None);
        assert_eq!(Rungs::new(9999), None);
        assert_eq!(Rungs::new(4).map(Bounded::get), Some(4));
        assert_eq!(Rungs::new(4096).map(Bounded::get), Some(4096));
    }

    #[test]
    fn clamping_is_available_but_has_to_be_asked_for_by_name() {
        // Refusing is the default because clamping a count silently draws a
        // picture that misrepresents the hardware. Where clamping *is* right -
        // fitting a request to whatever the strip actually has - the call site
        // says so.
        assert_eq!(Rungs::nearest(3).get(), 4);
        assert_eq!(Rungs::nearest(99999).get(), 4096);
        assert_eq!(Rungs::nearest(60).get(), 60);
    }

    #[test]
    fn two_ranges_are_two_types() {
        // What the const generics buy: a count bounded one way cannot be passed
        // where a count bounded another way belongs. This compiles only because
        // the values are unwrapped to usize first - the types themselves do not
        // interconvert, which is the guarantee.
        type Channels = Bounded<1, 2>;
        assert_eq!(Channels::new(2).map(Bounded::get), Some(2));
        assert_eq!(Channels::new(4), None, "there is no four-channel rav");
        assert_eq!(Rungs::MIN, 4);
        assert_eq!(Channels::MAX, 2);
    }

    #[test]
    fn a_known_good_value_is_const_constructible() {
        // So a preset or a board definition can state its geometry as a const,
        // with no allocator and no runtime check.
        const STRIP: Rungs = Rungs::known(5);
        assert_eq!(STRIP.get(), 5);
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
    fn a_duration_counts_frames_of_the_display_it_is_asked_about() {
        // The same half second is thirty frames of one display and fifteen of
        // another, which is why a rate written per frame has to say whose.
        assert_eq!(Elapsed::seconds(0.5).frames_at(60.0), Frames::count(30.0));
        assert_eq!(Elapsed::seconds(0.5).frames_at(30.0), Frames::count(15.0));

        // A display slower than a frame a second gets a fraction of one, not a
        // whole one - rounding up here would make a rate fall faster than asked
        // on exactly the hardware least able to hide it.
        assert_eq!(Elapsed::seconds(1.0).frames_at(0.5), Frames::count(0.5));

        // A rate nobody could run at counts nothing, rather than running the
        // motion backwards or filling it with NaN.
        assert!(Elapsed::seconds(1.0).frames_at(f32::NAN).is_none());
        assert!(Elapsed::seconds(1.0).frames_at(-60.0).is_none());
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
