//! A ramp of colours, and the stripes it paints.
//!
//! The one rule carried over from the terminal renderer: **colour is a function
//! of height, never of a bar's own level**. A tall bar and a short one agree on
//! the colour they share, and the unlit backdrop shows the colour a bar will
//! reach when it gets there. In a cell grid that fell out of indexing a per-row
//! table; here it is a [`Ramp`], asked for the colour at a height.
//!
//! Stripes are placed to match what the terminal does rather than to look tidy.
//! `ramp_index` rounds to the nearest stop, so the bottom and top stripes come
//! out half as tall as the rest - the first stop only holds until the fraction
//! reaches half a stripe. Spacing the stops evenly instead would shift every
//! colour boundary, which is precisely the difference the pixel release is not
//! allowed to introduce.

use crate::ink::Colour;
use alloc::vec;
use alloc::vec::Vec;
use rav_core::geometry::Rectangle;
use rav_core::units::{Length, Level};
/// A horizontal band of the screen, in one colour.
///
/// Knows how to clip a rectangle to itself, so neither the rasteriser nor a
/// surface has to work that out with `max` and `min`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stripe {
    pub top: Length,
    pub bottom: Length,
    pub colour: Colour,
}

impl Stripe {
    /// The part of `area` lying inside this stripe, if any.
    pub fn clip(&self, area: Rectangle) -> Option<Rectangle> {
        area.between(self.top, self.bottom)
    }

    pub fn height(&self) -> Length {
        self.bottom - self.top
    }
}

/// A colour ramp, bottom-up: the first stop is the floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ramp {
    stops: Vec<Colour>,
}

impl Ramp {
    /// An empty ramp is not representable; a caller with no colours gets black
    /// rather than a panic at the point of drawing.
    pub fn new(stops: Vec<Colour>) -> Self {
        Self {
            stops: if stops.is_empty() {
                vec![Colour::BLACK]
            } else {
                stops
            },
        }
    }

    pub fn len(&self) -> usize {
        self.stops.len()
    }

    /// The stops themselves, floor first.
    pub fn stops(&self) -> &[Colour] {
        &self.stops
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// The colour at a height above the floor, on a screen this tall.
    ///
    /// Positional: measured from the floor, because a continuous surface has no
    /// rows to stretch between. The terminal's `ramp_index` stretches instead,
    /// so that row `height - 1` is always the last stop however few rows there
    /// are. That difference in *input* is deliberate and is the only one - the
    /// selection rule below is shared, and
    /// `the_two_surfaces_never_differ_by_more_than_one_stop` bounds what the
    /// difference can cost.
    pub fn at(&self, height_above_floor: Length, screen_height: Length) -> Colour {
        let fraction = height_above_floor.fraction_of(screen_height);
        self.stops[Level::new(fraction).nearest_step(self.stops.len()).index()]
    }

    /// The stripes this ramp paints on a screen, ceiling first.
    ///
    /// Half-height stripes at each end are the point - see the module note.
    pub fn stripes(&self, screen_height: Length) -> Vec<Stripe> {
        let last = self.stops.len() - 1;
        if last == 0 || screen_height <= Length::NONE {
            return vec![Stripe {
                top: Length::NONE,
                bottom: screen_height.or_none(),
                colour: self.stops[0],
            }];
        }
        let last_stop = last as f32;
        (0..=last)
            .rev()
            .map(|stop| {
                // The fractions at which rounding tips into and out of `stop`.
                let lowest = ((stop as f32 - 0.5) / last_stop).clamp(0.0, 1.0);
                let highest = ((stop as f32 + 0.5) / last_stop).clamp(0.0, 1.0);
                Stripe {
                    top: screen_height * (1.0 - highest),
                    bottom: screen_height * (1.0 - lowest),
                    colour: self.stops[stop],
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rav_core::geometry::Rectangle;

    /// Four stops, each tellable from the others at a glance in a failure.
    fn ramp() -> Ramp {
        Ramp::new(vec![
            Colour::BLACK,
            Colour::RED,
            Colour::YELLOW,
            Colour::WHITE,
        ])
    }

    #[test]
    fn the_stripes_cover_the_screen_exactly_once() {
        // A gap is an unpainted line across the display and an overlap is a
        // colour drawn twice; both look like a rendering bug rather than a
        // ramp bug, which is why this is asserted here and not noticed there.
        let screen = Length(360.0);
        let stripes = ramp().stripes(screen);

        assert_eq!(stripes[0].top, Length::NONE, "a gap at the ceiling");
        assert_eq!(stripes.last().unwrap().bottom, screen, "a gap at the floor",);
        for pair in stripes.windows(2) {
            assert_eq!(
                pair[0].bottom, pair[1].top,
                "{:?} and {:?} do not meet",
                pair[0], pair[1],
            );
        }
    }

    #[test]
    fn the_end_stripes_are_half_as_tall_as_the_rest() {
        // Not tidiness - it is what `nearest_step` rounding produces, and what
        // the terminal renderer has always drawn. Spacing the stops evenly
        // instead would move every colour boundary in the picture, which is
        // exactly the difference a pixel surface is not allowed to introduce.
        let stripes = ramp().stripes(Length(300.0));
        let middle = stripes[1].height().get();
        assert!((stripes[0].height().get() - middle / 2.0).abs() < 1e-3);
        assert!((stripes.last().unwrap().height().get() - middle / 2.0).abs() < 1e-3);
    }

    #[test]
    fn asking_for_a_colour_and_reading_it_off_a_stripe_give_the_same_answer() {
        // Two ways to the same fact, which is the shape that drifts: the
        // rasteriser paints stripes and a surface that samples per bar calls
        // `at`. A ramp module whose two halves disagreed would be the 679/19980
        // divergence again, one layer up.
        //
        // Sixteen stops, sampled near both edges of each stripe as well as its
        // middle. Any two placements of the stops agree in the middle of a
        // stripe and part company towards its ends, so midpoints alone cover
        // only the half of the range where a misplaced boundary cannot show.
        let screen = Length(480.0);
        let ramp = Ramp::new((0..16).map(|i| Colour::rgb(i * 16, 0, 0)).collect());
        for stripe in ramp.stripes(screen) {
            let height = stripe.height().get();
            for inset in [0.05, 0.5, 0.95] {
                let from_top = stripe.top + Length(height * inset);
                let above_floor = screen - from_top;
                assert_eq!(
                    ramp.at(above_floor, screen),
                    stripe.colour,
                    "{inset} of the way down {stripe:?}, at {above_floor:?}",
                );
            }
        }
    }

    #[test]
    fn the_floor_is_the_first_stop_and_the_ceiling_is_the_last() {
        // The rule the whole look rests on: a bar's foot is always the bottom of
        // the ramp and a full-height bar always reaches the top, whatever the
        // screen is. A theme is written as a ladder and never has to ask how
        // tall the display is.
        let screen = Length(200.0);
        let ramp = ramp();
        assert_eq!(ramp.at(Length::NONE, screen), Colour::BLACK);
        assert_eq!(ramp.at(screen, screen), Colour::WHITE);
    }

    #[test]
    fn a_ramp_of_one_colour_paints_the_whole_screen_in_it() {
        let flat = Ramp::new(vec![Colour::CYAN]);
        let stripes = flat.stripes(Length(120.0));
        assert_eq!(stripes.len(), 1);
        assert_eq!(stripes[0].height(), Length(120.0));
        assert_eq!(flat.at(Length(60.0), Length(120.0)), Colour::CYAN);
    }

    #[test]
    fn a_screen_with_no_height_still_answers() {
        // A terminal mid-resize. Dividing by the height would give NaN, and a
        // NaN colour index panics at the point of drawing rather than here.
        let stripes = ramp().stripes(Length::NONE);
        assert_eq!(stripes.len(), 1, "nothing to divide into");
        assert_eq!(ramp().at(Length::NONE, Length::NONE), Colour::BLACK);
    }

    #[test]
    fn a_ramp_with_no_colours_is_black_rather_than_a_panic() {
        // Reachable from a theme file with an empty ramp. Black is wrong-looking
        // and obvious; a panic mid-render loses the terminal.
        let empty = Ramp::new(vec![]);
        assert_eq!(empty.len(), 1);
        assert!(!empty.is_empty());
        assert_eq!(empty.at(Length(10.0), Length(20.0)), Colour::BLACK);
    }

    #[test]
    fn a_bar_cut_into_stripes_reassembles_into_the_bar() {
        // What the rasteriser does every frame: take one bar and paint it in as
        // many pieces as it crosses colours. Losing height in the cut would
        // shorten every bar by a sliver at each boundary.
        let screen = Length(400.0);
        let bar = Rectangle::new(Length(10.0), Length(100.0), Length(8.0), Length(250.0));
        let pieces: Vec<Rectangle> = ramp()
            .stripes(screen)
            .iter()
            .filter_map(|stripe| stripe.clip(bar))
            .collect();

        assert!(pieces.len() > 1, "the bar crosses more than one colour");
        let painted: f32 = pieces.iter().map(|piece| piece.height().get()).sum();
        assert!(
            (painted - bar.height().get()).abs() < 1e-3,
            "painted {painted} of {:?}",
            bar.height(),
        );
        // Ceiling-first, so the pieces come back in order and touching.
        for pair in pieces.windows(2) {
            assert_eq!(pair[0].bottom(), pair[1].top(), "a seam in the bar");
        }
    }
}
