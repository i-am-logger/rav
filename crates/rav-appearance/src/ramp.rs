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
