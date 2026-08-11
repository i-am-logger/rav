//! Turning geometry and colour into pixels.
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

use crate::render::geometry::{BarLayout, Rectangle, Screen};
use crate::render::ink::Colour;
use crate::units::{Length, Level};
use tiny_skia::{Paint, Pixmap, Rect as SkRect, Transform};

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

/// An RGBA buffer that geometry is drawn into.
///
/// Owns a `tiny_skia::Pixmap` rather than exposing it, because the pixels leave
/// here in straight alpha and tiny-skia stores them premultiplied - a caller
/// handed the raw buffer would send subtly wrong edges to the terminal and have
/// no reason to suspect it.
pub struct Canvas {
    pixmap: Pixmap,
}

impl Canvas {
    /// `None` for a zero dimension, which is a terminal mid-resize rather than a
    /// programming error, and is the caller's to skip.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Pixmap::new(width, height).map(|pixmap| Self { pixmap })
    }

    /// Sized to hold a screen exactly.
    pub fn for_screen(screen: &Screen) -> Option<Self> {
        Self::new(screen.width().rounded_up(), screen.height().rounded_up())
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Reset to fully transparent, so the terminal's own background shows
    /// wherever rav has not drawn.
    pub fn clear(&mut self) {
        self.pixmap.fill(tiny_skia::Color::TRANSPARENT);
    }

    /// Paint one rectangle in one colour.
    ///
    /// An empty rectangle is skipped rather than refused: a bar at rest is one,
    /// and every frame of silence would otherwise be an error to handle.
    pub fn fill(&mut self, area: Rectangle, colour: Colour) {
        // The one place a distance stops being a `Length` and becomes the number
        // tiny-skia takes, so the unit cannot be lost anywhere above here.
        let Some(rect) = SkRect::from_xywh(
            area.left().get(),
            area.top().get(),
            area.width().get(),
            area.height().get(),
        ) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(colour.red, colour.green, colour.blue, colour.alpha);
        // Antialiasing on, which is the whole argument for drawing pixels: a bar
        // edge lands where the arithmetic puts it instead of being snapped to a
        // boundary, and that snapping is the mechanism behind #63.
        paint.anti_alias = true;
        self.pixmap
            .fill_rect(rect, &paint, Transform::identity(), None);
    }

    /// Paint one rectangle in whatever colour the ramp gives at each height.
    ///
    /// Stripe by stripe rather than row by row, so the cost is the number of
    /// stops and not the height of the rectangle.
    pub fn fill_ramped(&mut self, area: Rectangle, ramp: &Ramp, screen: &Screen) {
        for stripe in ramp.stripes(screen.height()) {
            if let Some(part) = stripe.clip(area) {
                self.fill(part, stripe.colour);
            }
        }
    }

    /// The frame in straight-alpha RGBA, which is what `f=32` means to a
    /// terminal and what a window blit expects.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixmap.data().len());
        for pixel in self.pixmap.pixels() {
            let straight = pixel.demultiply();
            out.extend_from_slice(&[
                straight.red(),
                straight.green(),
                straight.blue(),
                straight.alpha(),
            ]);
        }
        out
    }

    /// The premultiplied buffer, for a target that wants it that way.
    pub fn premultiplied(&self) -> &[u8] {
        self.pixmap.data()
    }
}

/// One frequency band, as the analyser currently sees it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Band {
    /// How loud it is now.
    pub level: Level,
    /// How loud it recently was - where the cap rides.
    pub peak: Level,
}

impl Band {
    pub fn new(level: Level, peak: Level) -> Self {
        Self { level, peak }
    }
}

/// Everything a surface needs to draw one frame.
///
/// The seam between the analyser and whatever is drawing. Deliberately not a
/// retained scene graph: the glyph renderer has to quantise to cells with its
/// own rules - the fill-versus-glyph backdrop, cap lifting, partial blocks - and
/// quantising a shared list of rectangles back into cells cannot reproduce them.
/// Every surface takes the same scene and each is free to be itself.
pub struct Scene<'a> {
    pub bands: &'a [Band],
    pub layout: BarLayout,
    pub screen: Screen,
    /// Colour of a lit bar, by height.
    pub bar_ramp: &'a Ramp,
    /// Colour of the unlit backdrop, by height. `None` leaves the terminal's own
    /// background showing, which is what a theme without a grid asks for.
    pub grid_ramp: Option<&'a Ramp>,
    /// `None` when caps are switched off.
    pub cap: Option<CapStyle>,
}

/// How the peak caps are drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapStyle {
    pub colour: Colour,
    pub thickness: Length,
}

impl Scene<'_> {
    /// How many bands are actually drawn.
    ///
    /// The glyph renderer's rule, taken verbatim: the lesser of what fits and
    /// what exists. Drawing one per band regardless would put rectangles past
    /// the screen, which the rasteriser discards in silence - so the surfaces
    /// would disagree on bar count with nothing to say so.
    pub fn visible_bands(&self) -> usize {
        self.layout
            .fitting_across(&self.screen)
            .min(self.bands.len())
    }

    /// Draw into `canvas`, back to front.
    ///
    /// Backdrop, then bars, then caps - the order that lets a cap lie over the
    /// backdrop instead of replacing it.
    pub fn draw(&self, canvas: &mut Canvas) {
        canvas.clear();

        if let Some(grid) = self.grid_ramp {
            for column in self.layout.columns(&self.screen) {
                canvas.fill_ramped(column.backdrop(&self.screen), grid, &self.screen);
            }
        }

        for (index, band) in self.bands.iter().take(self.visible_bands()).enumerate() {
            let column = self.layout.column(index);
            canvas.fill_ramped(
                column.bar(band.level, &self.screen),
                self.bar_ramp,
                &self.screen,
            );
        }

        if let Some(style) = self.cap {
            for (index, band) in self.bands.iter().take(self.visible_bands()).enumerate() {
                let column = self.layout.column(index);
                canvas.fill(
                    column.cap(band.peak, style.thickness, &self.screen),
                    style.colour,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(width: f32, height: f32) -> Screen {
        Screen::new(Length(width), Length(height))
    }

    fn layout(width: f32, gap: f32) -> BarLayout {
        BarLayout::new(Length(width), Length(gap))
    }

    fn traffic_light() -> Ramp {
        Ramp::new(vec![Colour::GREEN, Colour::YELLOW, Colour::RED])
    }

    fn bands(levels: &[f32]) -> Vec<Band> {
        levels
            .iter()
            .map(|&l| Band::new(Level::new(l), Level::new(l)))
            .collect()
    }

    /// The colour at a pixel, in straight alpha.
    fn at(canvas: &Canvas, x: u32, y: u32) -> Colour {
        let data = canvas.to_rgba();
        let i = ((y * canvas.width() + x) * 4) as usize;
        Colour {
            red: data[i],
            green: data[i + 1],
            blue: data[i + 2],
            alpha: data[i + 3],
        }
    }

    fn scene<'a>(bands: &'a [Band], bar_ramp: &'a Ramp, grid_ramp: Option<&'a Ramp>) -> Scene<'a> {
        Scene {
            bands,
            layout: layout(10.0, 0.0),
            screen: screen(10.0, 60.0),
            bar_ramp,
            grid_ramp,
            cap: Some(CapStyle {
                colour: Colour::WHITE,
                thickness: Length(3.0),
            }),
        }
    }

    #[test]
    fn a_cap_composites_over_the_backdrop_instead_of_erasing_it() {
        // The defect that cannot be fixed in a cell grid (#65): one cell holds
        // one glyph, so writing the cap destroys the backdrop beneath it. Here
        // both are drawn and the backdrop survives either side of the cap.
        let screen = screen(20.0, 60.0);
        let column = layout(10.0, 2.0).column(0);
        let mut canvas = Canvas::for_screen(&screen).unwrap();
        canvas.clear();

        let grid = Colour::rgb(0x00, 0x40, 0x00);
        canvas.fill(column.backdrop(&screen), grid);
        let cap = column.cap(Level::new(0.5), Length(4.0), &screen);
        canvas.fill(cap, Colour::WHITE);

        let inside_cap = cap.top().get() as u32 + 1;
        assert_eq!(
            at(&canvas, 2, inside_cap),
            Colour::WHITE,
            "the cap is drawn"
        );
        // Above and below it the backdrop is untouched, which is the whole point.
        assert_eq!(at(&canvas, 2, inside_cap.saturating_sub(6)), grid);
        assert_eq!(at(&canvas, 2, cap.bottom().get() as u32 + 4), grid);
    }

    #[test]
    fn colour_follows_height_not_the_bands_level() {
        // The invariant the terminal renderer is built on, restated in pixels:
        // where a quiet band and a loud one overlap they are the same colour.
        let ramp = traffic_light();
        let quiet = bands(&[0.3]);
        let loud = bands(&[0.9]);

        let mut short = Canvas::new(10, 60).unwrap();
        scene(&quiet, &ramp, None).draw(&mut short);
        let mut tall = Canvas::new(10, 60).unwrap();
        scene(&loud, &ramp, None).draw(&mut tall);

        for y in 43..60 {
            assert_eq!(
                at(&short, 5, y),
                at(&tall, 5, y),
                "row {y} disagrees between a quiet band and a loud one"
            );
        }
    }

    #[test]
    fn the_two_surfaces_never_differ_by_more_than_one_stop() {
        // The terminal and the rasteriser each pick a stop by height, and they
        // used to do it with two separate implementations that disagreed on 679
        // of 19980 row/height pairs. They now share one rule and differ only in
        // how each computes its fraction: the terminal stretches row 0..height-1
        // across the whole ramp so a short display still reaches the last
        // colour, while a continuous surface measures from the floor.
        //
        // That residue is inherent to quantising a continuous height onto cells,
        // so this bounds it rather than forbidding it. If the two ever drift
        // further apart than one stop, they have stopped being one rule again.
        use crate::ui::scale::ramp_index;
        let stops: Vec<Colour> = (0..16).map(|i| Colour::rgb(i as u8, 0, 0)).collect();
        let ramp = Ramp::new(stops);

        for rows in 16u16..=200 {
            for row in 0..rows {
                let terminal = ramp_index(row, rows, 16) as i32;
                // The same row's centre, as a continuous height above the floor.
                let centre = Length((row as f32 + 0.5) / rows as f32);
                let pixel = ramp.at(centre, Length(1.0)).red as i32;
                assert!(
                    (terminal - pixel).abs() <= 1,
                    "rows {rows} row {row}: terminal {terminal}, pixel {pixel}"
                );
            }
        }
    }

    #[test]
    fn a_ramp_runs_bottom_up() {
        let ramp = traffic_light();
        assert_eq!(ramp.at(Length::NONE, Length(60.0)), Colour::GREEN, "floor");
        assert_eq!(ramp.at(Length(60.0), Length(60.0)), Colour::RED, "ceiling");
        assert_eq!(
            ramp.at(Length(30.0), Length(60.0)),
            Colour::YELLOW,
            "middle"
        );
    }

    #[test]
    fn the_end_stripes_are_half_height() {
        // Not a quirk to tidy away: `ramp_index` rounds to the nearest stop, so
        // the first stop holds only until the fraction reaches half a stripe.
        // Evenly spaced stops would move every boundary in the picture.
        let stripes = traffic_light().stripes(Length(60.0));
        assert_eq!(stripes.len(), 3);
        assert!((stripes[0].height().get() - 15.0).abs() < 1e-4, "top");
        assert!((stripes[1].height().get() - 30.0).abs() < 1e-4, "middle");
        assert!((stripes[2].height().get() - 15.0).abs() < 1e-4, "bottom");
        // And they tile the screen exactly, with no seam and no overlap.
        let total: f32 = stripes.iter().map(|s| s.height().get()).sum();
        assert!((total - 60.0).abs() < 1e-4, "stripes must tile the height");
    }

    #[test]
    fn a_stripe_clips_a_bar_to_itself() {
        let stripes = traffic_light().stripes(Length(60.0));
        let full_column = Rectangle::new(Length::NONE, Length::NONE, Length(10.0), Length(60.0));
        for stripe in &stripes {
            let part = stripe.clip(full_column).expect("the column spans them all");
            assert_eq!(part.top(), stripe.top);
            assert_eq!(part.bottom(), stripe.bottom);
        }
        // A bar that does not reach a stripe is clipped away entirely, rather
        // than becoming a rectangle of negative height.
        let floor_only = Rectangle::new(Length::NONE, Length(55.0), Length(10.0), Length(5.0));
        assert_eq!(
            stripes[0].clip(floor_only),
            None,
            "the top stripe is missed"
        );
    }

    #[test]
    fn a_silent_band_draws_nothing_and_does_not_panic() {
        let ramp = traffic_light();
        let silent = bands(&[0.0]);
        let mut canvas = Canvas::new(10, 60).unwrap();
        Scene {
            cap: None,
            ..scene(&silent, &ramp, None)
        }
        .draw(&mut canvas);
        assert!(
            canvas.to_rgba().chunks(4).all(|pixel| pixel[3] == 0),
            "silence must leave the frame transparent"
        );
    }

    #[test]
    fn caps_switched_off_draw_nothing() {
        let ramp = traffic_light();
        let grid = Ramp::new(vec![Colour::rgb(0x00, 0x20, 0x00)]);
        let loud = bands(&[0.5]);

        let mut with = Canvas::new(10, 60).unwrap();
        scene(&loud, &ramp, Some(&grid)).draw(&mut with);
        let mut without = Canvas::new(10, 60).unwrap();
        Scene {
            cap: None,
            ..scene(&loud, &ramp, Some(&grid))
        }
        .draw(&mut without);

        assert_ne!(with.to_rgba(), without.to_rgba(), "a cap must be visible");
    }

    #[test]
    fn no_grid_leaves_the_terminal_background_showing() {
        // A theme without a grid must not invent one - the terminal's own
        // background is what shows through, so those pixels stay transparent.
        let ramp = traffic_light();
        let quiet = bands(&[0.25]);
        let mut canvas = Canvas::new(10, 60).unwrap();
        Scene {
            cap: None,
            ..scene(&quiet, &ramp, None)
        }
        .draw(&mut canvas);
        assert_eq!(at(&canvas, 5, 5).alpha, 0, "unlit rows stay transparent");
        assert_eq!(at(&canvas, 5, 58).alpha, 0xff, "the lit bar is still drawn");
    }

    #[test]
    fn an_antialiased_bar_edge_blends_into_the_backdrop() {
        // Every layer being opaque makes source-over and replace-the-pixel look
        // identical, so a whole-scene test cannot tell whether compositing works
        // at all. A bar whose top edge lands mid-pixel is where it shows: that
        // edge must be part bar and part backdrop, not part bar and part hole.
        let ramp = Ramp::new(vec![Colour::RED]);
        let grid = Ramp::new(vec![Colour::BLUE]);
        // 0.333 * 60 = 19.98, so the edge sits a hundredth of a pixel inside
        // row 40 and that row is almost entirely backdrop.
        let edge_case = bands(&[0.333]);

        let mut over_grid = Canvas::new(10, 60).unwrap();
        Scene {
            cap: None,
            ..scene(&edge_case, &ramp, Some(&grid))
        }
        .draw(&mut over_grid);
        let blended = at(&over_grid, 5, 40);
        assert_eq!(blended.alpha, 0xff, "the backdrop keeps the edge opaque");
        assert!(
            blended.red > 0 && blended.blue > 0,
            "the edge should carry bar and backdrop, got {blended:?}"
        );

        // Without a backdrop the same edge reaches the terminal semi-transparent
        // and the terminal composites it. Different pixels, deliberately.
        let mut bare = Canvas::new(10, 60).unwrap();
        Scene {
            cap: None,
            ..scene(&edge_case, &ramp, None)
        }
        .draw(&mut bare);
        let alone = at(&bare, 5, 40);
        assert!(
            alone.alpha > 0 && alone.alpha < 0xff,
            "a bare edge stays partly transparent, got {alone:?}"
        );
    }

    #[test]
    fn more_bands_than_fit_do_not_draw_past_the_screen() {
        // The glyph renderer draws the lesser of what fits and what exists. A
        // pixel surface drawing them all would have the rasteriser clip the
        // excess in silence, so the two would disagree on bar count with nothing
        // to say so - and "matches the glyph release" is the gate for shipping.
        let ramp = Ramp::new(vec![Colour::RED]);
        let wide = screen(30.0, 60.0);
        assert_eq!(layout(10.0, 0.0).fitting_across(&wide), 3, "three bars fit");

        let three = bands(&[1.0, 1.0, 1.0]);
        let ten = bands(&[1.0; 10]);

        let mut exact = Canvas::new(30, 60).unwrap();
        Scene {
            screen: wide,
            cap: None,
            ..scene(&three, &ramp, None)
        }
        .draw(&mut exact);
        let mut overflowing = Canvas::new(30, 60).unwrap();
        Scene {
            screen: wide,
            cap: None,
            ..scene(&ten, &ramp, None)
        }
        .draw(&mut overflowing);

        assert_eq!(
            exact.to_rgba(),
            overflowing.to_rgba(),
            "bands past the screen must not change the picture"
        );
    }

    #[test]
    fn transparency_survives_the_round_trip_to_straight_alpha() {
        // Premultiplied storage loses colour as alpha falls, so a half-alpha red
        // is stored as (128,0,0,128) and must come back as (255,0,0,128) or
        // every antialiased edge reaches the terminal too dark.
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.clear();
        canvas.fill(
            Rectangle::new(Length::NONE, Length::NONE, Length(4.0), Length(4.0)),
            Colour {
                alpha: 0x80,
                ..Colour::RED
            },
        );
        let pixel = at(&canvas, 2, 2);
        assert_eq!(pixel.alpha, 0x80, "alpha is preserved");
        assert!(pixel.red >= 0xfe, "red came back as {} not 0xff", pixel.red);
    }

    #[test]
    fn an_empty_ramp_is_not_representable() {
        let ramp = Ramp::new(vec![]);
        assert_eq!(ramp.len(), 1);
        assert_eq!(ramp.at(Length::NONE, Length(60.0)), Colour::BLACK);
    }

    #[test]
    fn a_screen_with_no_area_produces_nothing_that_panics() {
        let ramp = traffic_light();
        assert_eq!(ramp.at(Length::NONE, Length::NONE), Colour::GREEN);
        assert_eq!(ramp.stripes(Length::NONE).len(), 1);
        assert!(Canvas::for_screen(&screen(0.0, 0.0)).is_none());
    }
}
