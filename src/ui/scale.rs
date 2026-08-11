//! Value-to-cell conversions for the analyser.
//!
//! These three look interchangeable and are not: one truncates, one rounds, one
//! truncates and then clamps. Getting them mixed up produces a display that is
//! subtly wrong in ways that are very hard to see, so each is its own named
//! function with its own tests.
//!
//! All of them clamp their input. Callers already clip to full scale upstream,
//! but `as usize` on a negative float saturates silently rather than failing, and
//! a helper that trusts its caller eventually gets a bad value.

use crate::units::Level;

/// Solid-bar height in eighths of a cell. **Truncates.**
///
/// Sub-cell resolution comes from the ⅛ block glyphs, so a bar of 3.4 cells is
/// drawn as three full cells plus a `▄`.
#[inline]
pub fn bar_eighths(value: f32, height: u16) -> usize {
    let v = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((v * height as f32 * 8.0) as usize).min(height as usize * 8)
}

/// Number of lit rows in segmented style. **Rounds.**
#[inline]
pub fn segment_top(value: f32, height: u16) -> usize {
    let v = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((v * height as f32).round() as usize).min(height as usize)
}

/// Cap glyphs by position within a cell.
///
/// `▁` and `▔` are exact one-eighth blocks and match each other by
/// construction. There is no middle one-eighth block in Unicode, so `━` is a
/// heavy box-drawing stroke whose weight the font decides rather than the cell.
pub const CAP_LOW: &str = "\u{2581}";
pub const CAP_MID: &str = "\u{2501}";
pub const CAP_HIGH: &str = "\u{2594}";

/// Row from the bottom a cap sits on, and how far up that row it falls (0.0..1.0).
///
/// Splitting the row from the glyph choice keeps every cap style working off one
/// piece of arithmetic, so they cannot disagree about which row a cap is on.
#[inline]
pub fn cap_position(peak: f32, height: u16) -> (u16, f32) {
    if height == 0 {
        return (0, 0.0);
    }
    let p = if peak.is_finite() {
        peak.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let exact = p * height as f32;
    let row = (exact as usize).min(height as usize - 1);
    (row as u16, (exact - row as f32).clamp(0.0, 1.0))
}

/// Row from the bottom that a cap occupies. **Truncates, then clamps.**
#[inline]
pub fn cap_row(peak: f32, height: u16) -> u16 {
    if height == 0 {
        return 0;
    }
    let p = if peak.is_finite() {
        peak.clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((p * height as f32) as usize).min(height as usize - 1) as u16
}

/// How sharply stop selection is skewed when the ramp has to be subsampled.
///
/// The classic ramp is green-heavy: stops 0-5 of 16 are all greens, distinguishable
/// on a 16px panel but not when a short terminal picks only three or four of
/// them. Spacing stops evenly by index then spends half a four-row display on
/// colours that read as one, which is what "two greens, one yellow, one orange"
/// looks like. Skewing the selection climbs out of the green block faster and
/// gives four distinct colours instead.
///
/// The value is chosen so a four-row display lands on one green, one yellow,
/// one orange and one red - a full hue sweep in the smallest window worth
/// drawing. It is a display judgement, not a value any API or theme owns.
///
/// The alternative - sampling at uniform *perceptual* distance - gives red,
/// yellow, yellow-green, green at four rows, with no orange at all.
/// Its six greens sit within 20% of the ramp's total colour distance while
/// the green-to-yellow-green step is the largest single jump in it, so spacing
/// by colour difference clusters the picks at the top rather than spreading them
/// across hues. Spanning the hues is what reads as a spectrum.
///
/// Applied *only* while subsampling; once there is a row per stop the ramp is
/// used exactly as the theme has it.
const SUBSAMPLE_SKEW: f32 = 0.57;

/// Map a screen row (counted from the bottom) onto a stop in the theme's ramp.
///
/// The ramp is **stretched** to fit, so the bottom row always takes the first
/// stop and the top row the last, at any height. The original panel was always
/// exactly 16 rows so this never came up for it; a resizable terminal has to
/// choose, and sampling a fixed 16-stop ramp positionally means a display
/// shorter than 16 rows never reaches red.
///
/// Stops stay discrete rather than interpolated: they are 16 colours the theme
/// actually contains, and blending would invent ones it does not.
#[inline]
pub fn ramp_index(row_from_bottom: u16, height: u16, bands: usize) -> usize {
    let last_band = bands.saturating_sub(1);
    if height <= 1 {
        // A single row is neither the bottom nor the top of the ramp; the middle
        // is the only answer that is not arbitrary.
        return last_band / 2;
    }
    // Stretched, not positional: row 0 is the floor of the ramp and row
    // `height - 1` its ceiling, whatever the height. A continuous surface has no
    // rows to stretch between and measures from the floor instead - which is the
    // one genuine difference between the two, and is why this computes a
    // fraction rather than reaching for the stop itself.
    let t = row_from_bottom.min(height - 1) as f32 / (height - 1) as f32;
    // Only skew while stops are being skipped. With a row per stop every colour
    // is reachable anyway and the ramp should be used exactly as the theme has it.
    let t = if (height as usize) < bands {
        t.powf(SUBSAMPLE_SKEW)
    } else {
        t
    };
    // The selection rule itself lives in `units`, so the terminal and every
    // pixel surface share one implementation of "which stop is this". They had
    // two, and the two disagreed on 679 of 19980 row/height pairs.
    Level::new(t).nearest_step(bands).index()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_eighths_truncates() {
        // 0.5 of 10 rows = 5 cells = 40 eighths.
        assert_eq!(bar_eighths(0.5, 10), 40);
        // A hair under a whole cell must not round up to it.
        assert_eq!(bar_eighths(0.0999, 10), 7);
        assert_eq!(bar_eighths(1.0, 10), 80);
        assert_eq!(bar_eighths(0.0, 10), 0);
    }

    #[test]
    fn segment_top_rounds() {
        // The distinction from bar_eighths: 0.44 of 10 rounds up to 4, and 0.46
        // rounds to 5, where truncation would give 4 for both.
        assert_eq!(segment_top(0.46, 10), 5);
        assert_eq!(segment_top(0.44, 10), 4);
        assert_eq!(segment_top(1.0, 10), 10);
    }

    #[test]
    fn cap_row_stays_inside_the_area() {
        assert_eq!(cap_row(0.0, 10), 0);
        assert_eq!(cap_row(1.0, 10), 9, "full scale clamps to the top row");
        assert_eq!(cap_row(0.5, 10), 5);
        assert_eq!(cap_row(0.5, 0), 0, "zero height must not panic");
    }

    #[test]
    fn cap_position_splits_a_row_into_a_fraction() {
        let (row, frac) = cap_position(0.55, 10);
        assert_eq!(row, 5);
        assert!((frac - 0.5).abs() < 1e-5, "frac was {frac}");
        assert_eq!(
            cap_position(1.0, 10).0,
            9,
            "full scale clamps to the top row"
        );
    }

    #[test]
    fn cap_position_agrees_with_cap_row_and_survives_nonsense() {
        for peak in [0.0f32, 0.13, 0.5, 0.99, 1.0] {
            assert_eq!(cap_position(peak, 24).0, cap_row(peak, 24), "at {peak}");
        }
        for bad in [f32::NAN, f32::INFINITY, -5.0, 99.0] {
            let (row, frac) = cap_position(bad, 10);
            assert!(row < 10);
            assert!((0.0..=1.0).contains(&frac));
        }
        assert_eq!(cap_position(0.5, 0), (0, 0.0), "zero height must not panic");
    }

    #[test]
    fn conversions_reject_nonsense_without_panicking() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 99.0] {
            assert!(bar_eighths(bad, 10) <= 80);
            assert!(segment_top(bad, 10) <= 10);
            assert!(cap_row(bad, 10) <= 9);
        }
    }

    #[test]
    fn the_ramp_spans_the_display_at_every_height() {
        // The ramp is stretched to the window, not sampled positionally from it,
        // so a full-height bar reads red however short the terminal is.
        for h in [2u16, 3, 5, 8, 16, 24, 60] {
            assert_eq!(ramp_index(0, h, 16), 0, "height {h}: bottom not green");
            assert_eq!(ramp_index(h - 1, h, 16), 15, "height {h}: top not red");
        }
    }

    #[test]
    fn a_single_row_takes_the_middle_of_the_ramp() {
        // Neither the bottom nor the top; anything else would be arbitrary.
        assert_eq!(ramp_index(0, 1, 16), 7);
        assert_eq!(ramp_index(0, 0, 16), 7, "zero height must not panic");
    }

    #[test]
    fn four_rows_sweep_green_yellow_orange_red() {
        // One green, one yellow, one orange, one red - a full hue sweep in the
        // smallest window worth drawing, and the only place `SUBSAMPLE_SKEW`'s
        // value is answerable. Even index spacing would pick stops 0 and 5 for
        // the bottom two rows, both green, and the window would read as a
        // single colour.
        let ramp = &crate::visual::Theme::default().bars;
        let rows: Vec<usize> = (0..4).map(|r| ramp_index(r, 4, ramp.len())).collect();
        assert_eq!(rows, vec![0, 8, 12, 15]);
        assert!(rows[1] > 5, "second row must clear the green block");

        // And they are those hues, rather than four indices trusted to be. Green
        // to red is the red channel gaining on the green one - which holds
        // across the yellow in the middle, where neither channel alone does,
        // since yellow is a *brighter* green than green is.
        let toward_red = |stop: usize| match ramp[stop] {
            crate::render::Ink::Rgb(red, green, _) => f32::from(red) / f32::from(green.max(1)),
            _ => panic!("the rav ramp spells its colours out"),
        };
        for pair in rows.windows(2) {
            assert!(
                toward_red(pair[1]) > toward_red(pair[0]),
                "stop {} is no further towards red than stop {}",
                pair[1],
                pair[0],
            );
        }
    }

    #[test]
    fn a_tall_display_uses_the_ramp_exactly_as_the_theme_has_it() {
        // No skew once there is a row per stop.
        let rows: Vec<usize> = (0..16).map(|r| ramp_index(r, 16, 16)).collect();
        assert_eq!(rows, (0..16).collect::<Vec<_>>());
    }

    #[test]
    fn the_ramp_is_monotonic_up_the_screen() {
        let mut last = 0;
        for r in 0..40u16 {
            let idx = ramp_index(r, 40, 16);
            assert!(idx >= last, "colour went backwards at row {r}");
            last = idx;
        }
    }
}
