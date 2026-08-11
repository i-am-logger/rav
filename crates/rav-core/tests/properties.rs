//! What has to hold for *every* input, not just the ones someone thought of.
//!
//! The example-based tests beside each type say what it does; these say what it
//! can never do. They are the tests that matter most for this crate, because it
//! is the one part of rav that has to survive being run somewhere nobody tried:
//! four LEDs on a cap, a 16K display, a screen mid-resize with no area at all.
//!
//! An integration test rather than a `#[cfg(test)]` module, which buys two
//! things. It compiles as an outside consumer, so anything these reach is
//! genuinely public and anything they cannot reach is not part of the crate's
//! offer. And it keeps the properties away from the examples, which are
//! documentation and read as such.
//!
//! # Generators are bounded on purpose
//!
//! Unbounded `usize` finds `rungs * steps` overflowing in a millisecond, and
//! unbounded `f32` finds a screen `1e38` wide where every coordinate is `inf`.
//! Neither is a bug worth the fix: hardening a `no_std` core against inputs no
//! caller can produce costs arithmetic on the path a microcontroller runs, to
//! defend against a display larger than the observable universe. The ranges
//! below are the domain - a four-light strip at one end and a wall of pixels at
//! the other - and a property that only fails outside them has found nothing.

use proptest::prelude::*;
use rav_core::{Anchor, BarLayout, Curve, Length, Level, Screen, Step};

/// Rungs on a ladder: four lights on a speaker grille, up to a tall display.
const RUNGS: core::ops::RangeInclusive<usize> = 1..=4096;
/// Sub-divisions of a rung: one for a plain LED, eight for a glyph ladder, 256
/// for a strip whose brightness is a byte.
const STEPS: core::ops::RangeInclusive<usize> = 1..=256;
/// Device pixels across or down. Zero is included: a terminal mid-resize.
const EXTENT: core::ops::RangeInclusive<f32> = 0.0..=8192.0;

prop_compose! {
    fn a_level()(fraction in 0.0f32..=1.0) -> Level {
        Level::new(fraction)
    }
}

prop_compose! {
    fn a_screen()(width in EXTENT, height in EXTENT) -> Screen {
        Screen::new(Length(width), Length(height))
    }
}

prop_compose! {
    fn a_layout()(width in 1.0f32..=64.0, gap in 0.0f32..=16.0) -> BarLayout {
        BarLayout::new(Length(width), Length(gap))
    }
}

fn an_anchor() -> impl Strategy<Value = Anchor> {
    prop_oneof![
        Just(Anchor::Floor),
        Just(Anchor::Ceiling),
        Just(Anchor::Middle)
    ]
}

fn a_curve() -> impl Strategy<Value = Curve> {
    prop_oneof![
        Just(Curve::Linear),
        (-96.0f32..=-6.0).prop_map(|floor| Curve::Decibel { floor }),
        (0.1f32..=4.0).prop_map(Curve::Gamma),
    ]
}

proptest! {
    /// A louder band never draws shorter, whichever curve is fitted.
    ///
    /// The one promise a curve makes. Break it and the display stops being a
    /// reading of the signal: two passages of different loudness could show the
    /// same height, or the quieter one could show taller.
    #[test]
    fn a_curve_never_turns_louder_into_shorter(
        curve in a_curve(),
        quiet in a_level(),
        louder in a_level(),
    ) {
        prop_assume!(quiet <= louder);
        prop_assert!(
            curve.apply(quiet) <= curve.apply(louder),
            "{curve:?} put {quiet:?} above {louder:?}",
        );
    }

    /// Silence draws nothing, whichever curve is fitted.
    ///
    /// A curve that lifts the quiet end is doing its job right up until it lifts
    /// *nothing* into something, at which point a dead input lights the display
    /// and there is no way to tell it from a live one.
    #[test]
    fn no_curve_lights_a_dead_signal(curve in a_curve()) {
        prop_assert!(curve.apply(Level::SILENT).is_silent(), "{curve:?} lit silence");
    }

    /// Filling more of a ladder never lights fewer of its units.
    ///
    /// Stated in units rather than rungs because the rung and the part above it
    /// move together: going from "two rungs and seven eighths" to "three rungs"
    /// gains a rung and loses the part, and only the total says that went up.
    #[test]
    fn filling_further_never_lights_less(
        rungs in RUNGS,
        steps in STEPS,
        quiet in a_level(),
        louder in a_level(),
    ) {
        prop_assume!(quiet <= louder);
        let units = |level: Level| {
            let fill = level.fill(rungs, steps);
            fill.whole * steps + fill.part.index()
        };
        prop_assert!(units(quiet) <= units(louder));
    }

    /// A full ladder is full rungs and nothing above them.
    ///
    /// The rung above the top one does not exist, so a part there would be a
    /// surface asked to draw off the end of the display.
    #[test]
    fn a_full_ladder_has_nothing_above_it(rungs in RUNGS, steps in STEPS) {
        let fill = Level::FULL.fill(rungs, steps);
        prop_assert_eq!(fill.whole, rungs, "not full");
        prop_assert!(!fill.has_part(), "something above the top rung");
    }

    /// A ladder is only ever as tall as it is.
    #[test]
    fn a_ladder_never_overfills(rungs in RUNGS, steps in STEPS, level in a_level()) {
        let fill = level.fill(rungs, steps);
        prop_assert!(fill.whole <= rungs, "{} of {rungs} rungs", fill.whole);
        prop_assert!(fill.whole < rungs || !fill.has_part(), "past the top");
    }

    /// The ends of a skin stay the ends of it at any resolution.
    ///
    /// What lets one setting drive surfaces that disagree about granularity: an
    /// eight-step glyph ladder and a 200-light strip are the same fraction at
    /// different resolutions. If the ends drifted, a bar that reads full on the
    /// terminal would read a shade under full on the strip beside it.
    #[test]
    fn rescaling_keeps_the_ends_at_the_ends(from in 2usize..=256, to in 1usize..=256) {
        let dimmest = Step::new(0, from).rescaled(to);
        prop_assert!(dimmest.is_first(), "the bottom moved");

        let brightest = Step::new(from - 1, from).rescaled(to);
        prop_assert!(brightest.is_last(), "the top moved");
    }
}

/// A screen too narrow for a single bar still reports one, which does not fit.
///
/// The case `every_bar_that_is_counted_has_room` has to skip, so it is pinned
/// here instead of living only in a `prop_assume!`. Reporting zero would mean a
/// terminal dragged narrow shows nothing at all, which reads as a crash; a bar
/// wider than its screen reads as a bar. The surface clips it - which is what
/// the surface does with every bar anyway, so this costs it nothing.
#[test]
fn a_screen_too_narrow_for_a_bar_still_gets_one_to_clip() {
    let layout = BarLayout::new(Length(20.0), Length(2.0));
    let slit = Screen::new(Length(6.0), Length(100.0));
    assert_eq!(layout.fitting_across(&slit), 1, "something has to be drawn");
    assert!(
        layout.column(0).right() > slit.width(),
        "and the caller has to clip it",
    );

    // No screen at all is different: nothing is being drawn, so nothing is
    // counted. A terminal mid-resize passes through this every time.
    let nothing = Screen::new(Length::NONE, Length::NONE);
    assert_eq!(layout.fitting_across(&nothing), 0);
}

/// A skin with one step rescales to the dimmest, not the brightest.
///
/// `Step::new(0, 1)` is both the first step and the last, so there is no answer
/// to read off it - the position is 0 of 0. Dimmest is the decision: a count of
/// one carries no brightness to preserve, and reading it as full would have a
/// plain on/off lamp drive a 200-light strip to maximum for any signal at all.
///
/// Written as an example rather than a property because it is a choice, and a
/// property would only restate whichever choice the code had made.
#[test]
fn a_single_step_rescales_to_the_dimmest() {
    let only = Step::new(0, 1);
    assert!(only.is_first() && only.is_last(), "one step is both ends");
    assert!(only.rescaled(200).is_first());
    assert!(
        !only.rescaled(200).is_last(),
        "it did not invent brightness"
    );
}

proptest! {
    /// Every bar the layout says fits, fits.
    ///
    /// `fitting_across` is a floating-point division followed by a floor, and a
    /// division that lands a hair over a whole number would report one bar more
    /// than there is room for - which draws the last bar off the edge, where a
    /// rasteriser discards it in silence and the display is short a band with
    /// nothing to say so.
    #[test]
    fn every_bar_that_is_counted_has_room(layout in a_layout(), screen in a_screen()) {
        let count = layout.fitting_across(&screen);
        prop_assert!(count >= 1, "there is always at least one bar to draw");

        // A screen narrower than a single bar still reports one, deliberately:
        // something has to be drawn and the surface clips it. Nothing to check.
        prop_assume!(screen.width() >= layout.bar_width());

        let last = layout.column(count - 1);
        prop_assert!(
            last.right() <= screen.width(),
            "bar {} ends at {:?} on a screen {:?} wide",
            count - 1,
            last.right(),
            screen.width(),
        );
    }

    /// A mirrored pair is a mirror image.
    ///
    /// The stereo arrangement on a car dashboard or a speaker grille is one
    /// channel on `Floor` and the other on `Ceiling`, so any disagreement
    /// between the two shows up as a pair that is visibly not a pair. They are
    /// separate arms of a `match`, which is exactly how the two colour-by-height
    /// rules that this crate exists to unify drifted apart.
    #[test]
    fn floor_and_ceiling_are_reflections_of_each_other(
        level in a_level(),
        screen in a_screen(),
    ) {
        let column = BarLayout::new(Length(8.0), Length(1.0)).column(0);
        let up = column.anchored(Anchor::Floor).bar(level, &screen);
        let down = column.anchored(Anchor::Ceiling).bar(level, &screen);

        let height = screen.height().get();
        prop_assert!(
            (up.top().get() - (height - down.bottom().get())).abs() < 1e-3,
            "{up:?} does not mirror {down:?}",
        );
        prop_assert!((up.height().get() - down.height().get()).abs() < 1e-3);
    }

    /// A bar that grows both ways stays centred.
    #[test]
    fn a_middle_bar_is_centred(level in a_level(), screen in a_screen()) {
        let bar = BarLayout::new(Length(8.0), Length(1.0))
            .column(0)
            .anchored(Anchor::Middle)
            .bar(level, &screen);
        let above = bar.top().get();
        let below = screen.height().get() - bar.bottom().get();
        prop_assert!((above - below).abs() < 1e-3, "{above} above, {below} below");
    }

    /// A cap marks the level without covering it.
    ///
    /// The defect a cell grid cannot fix (#65): a terminal cell holds one symbol,
    /// so drawing a cap there destroys what was beneath it. Here they are two
    /// rectangles, and the promise is that they do not need to compete - the cap
    /// sits entirely beyond the bar whenever there is room for it to.
    #[test]
    fn a_cap_sits_beyond_its_bar_rather_than_over_it(
        anchor in an_anchor(),
        level in a_level(),
        rise in 0.0f32..=1.0,
        thickness in 1.0f32..=8.0,
        height in 1.0f32..=8192.0,
    ) {
        // A peak is the high-water mark of the level, never below it.
        let peak = Level::new(level.fraction() + rise);
        let screen = Screen::new(Length(256.0), Length(height));
        let thickness = Length(thickness);

        // Room for the cap beyond the bar, or it is held inside the screen
        // instead and has to overlap - documented, and the reason a cap at full
        // scale sits on the bar rather than half off the display. `Middle` needs
        // twice the room, because it grows from the centre in both directions.
        let needed = match anchor {
            Anchor::Middle => thickness.get() * 2.0,
            _ => thickness.get(),
        };
        prop_assume!(screen.risen_to(peak).get() + needed <= height);

        let column = BarLayout::new(Length(8.0), Length(1.0)).column(0).anchored(anchor);
        let bar = column.bar(level, &screen);
        let cap = column.cap(peak, thickness, &screen);
        prop_assert!(
            !cap.overlaps(&bar),
            "{anchor:?}: cap {cap:?} over bar {bar:?} at level {level:?} peak {peak:?}",
        );
    }

    /// A cap stays on the display.
    ///
    /// Held inside at both ends: at full scale it would otherwise hang off the
    /// far edge, and at rest off the near one. Guarded to screens at least as
    /// tall as the thinnest cap, since a cap has a floor of one light and a
    /// display shorter than that has nowhere to put it - the documented collapse
    /// rather than a defect.
    #[test]
    fn a_cap_is_always_somewhere_on_the_display(
        anchor in an_anchor(),
        peak in a_level(),
        thickness in 1.0f32..=8.0,
        height in 8.0f32..=8192.0,
    ) {
        let screen = Screen::new(Length(256.0), Length(height));
        let cap = BarLayout::new(Length(8.0), Length(1.0))
            .column(0)
            .anchored(anchor)
            .cap(peak, Length(thickness), &screen);
        prop_assert!(cap.top() >= Length::NONE, "above the top: {cap:?}");
        prop_assert!(cap.bottom() <= screen.height(), "below the floor: {cap:?}");
    }

    /// Crossed bounds collapse rather than panicking.
    ///
    /// `f32::clamp` panics when the low bound is above the high one, and a
    /// zero-height display produces exactly that honestly - a cap thicker than
    /// the screen leaves nowhere to put it. Collapsing to the low bound is the
    /// documented answer, and it is what keeps a terminal mid-resize from
    /// bringing rav down.
    #[test]
    fn crossing_the_bounds_collapses(value in -1e4f32..=1e4, low in -1e4f32..=1e4, high in -1e4f32..=1e4) {
        let held = Length(value).held_between(Length(low), Length(high));
        if low > high {
            prop_assert_eq!(held, Length(low), "it did not collapse to the low bound");
        } else {
            prop_assert!(held >= Length(low) && held <= Length(high));
        }
    }
}

/// A ladder never lights a step the signal has not reached, and is at worst one
/// step shy of one it has.
///
/// The `#63` defect family asked of the core rather than of a terminal: does the
/// level that *means* `k` units survive being a float? Mostly. Sweeping every
/// boundary of every ladder below, **3876 of 101136 come up one unit short and
/// none ever come up long** - `1x49` at `k=27` is the smallest, where `27/49`
/// rounds down as an `f32` and multiplying back by 49 lands at 26.9999.
///
/// Short is the direction that matters. Truncating is the documented rule -
/// "a cell is only as lit as the signal actually reached" - so a boundary that
/// arrives a hair late is that rule being kept, while one that arrived early
/// would be a bar lighting a step the music never got to. Rounding instead would
/// trade a measurement for a flattery at every level, not just these.
///
/// Deliberately not a `proptest!` case. Exhausting the boundaries of small
/// ladders is a stronger statement than sampling large ones, and the failure it
/// looks for lives at the boundaries and nowhere else.
#[test]
fn a_ladder_boundary_is_never_high_and_at_worst_one_unit_low() {
    let mut worst = 0usize;
    let mut soft = 0usize;
    let mut checked = 0usize;
    for rungs in 1..=32usize {
        for steps in [1usize, 2, 3, 5, 7, 8, 16, 49, 100] {
            let total = rungs * steps;
            for k in 0..=total {
                let level = Level::new(k as f32 / total as f32);
                let fill = level.fill(rungs, steps);
                let lit = fill.whole * steps + fill.part.index();
                assert!(
                    lit <= k,
                    "a ladder of {rungs}x{steps} lit {lit} units where {k} was asked for",
                );
                worst = worst.max(k - lit);
                soft += usize::from(lit < k);
                checked += 1;
            }
        }
    }
    std::println!("{soft} of {checked} boundaries soft, worst {worst}");
    assert!(worst <= 1, "a boundary missed by {worst} units");
}

/// Every ladder rav actually draws is exact.
///
/// Which is why the softness above is a note and not a defect. A level lands on
/// a boundary exactly when `k / total` is representable, and it always is when
/// `total` is a power of two - so an eight-step glyph ladder is exact at every
/// height, and so is a four-light strip. `1x49` is not a display anyone builds;
/// it is the arithmetic saying where the edge of its accuracy is.
///
/// The rule for anything that does build one: prefer a power of two. That is a
/// constraint an embedded target wants stated rather than discovered, and it is
/// the same one `microfft` puts on the FFT size.
#[test]
fn a_ladder_whose_size_is_a_power_of_two_is_exact() {
    for rungs in [1usize, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        for steps in [1usize, 2, 4, 8, 16, 32, 64, 128, 256] {
            let total = rungs * steps;
            for k in 0..=total {
                let level = Level::new(k as f32 / total as f32);
                let fill = level.fill(rungs, steps);
                assert_eq!(
                    fill.whole * steps + fill.part.index(),
                    k,
                    "a ladder of {rungs}x{steps} missed boundary {k}",
                );
            }
        }
    }
}
