//! Turning geometry and colour into pixels.
//!
//! The one rule carried over from the terminal renderer: **colour is a function
//! of height, never of a bar's own value**. A tall bar and a short one agree on
//! the colour they share, and the unlit backdrop shows the colour a bar will
//! reach when it gets there. In a cell grid that fell out of indexing a
//! per-row table; here it is [`Ramp`], asked for the colour at a coordinate.
//!
//! Bands are placed to match what the terminal does rather than to look tidy.
//! `ramp_index` rounds to the nearest stop, so the bottom and top bands come out
//! half as tall as the rest - the first stop only holds until the fraction
//! reaches half a band. Spacing the stops evenly instead would shift every
//! colour boundary, which is precisely the difference the pixel release is not
//! allowed to introduce.

use crate::render::geometry::Rect;
use crate::render::ink::Colour;
use tiny_skia::{Paint, Pixmap, Rect as SkRect, Transform};

/// A colour ramp, bottom-up: `stops[0]` is the floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ramp {
    stops: Vec<Colour>,
}

impl Ramp {
    /// An empty ramp is not representable; a caller with no colours gets black
    /// rather than a panic at the point of drawing.
    pub fn new(stops: Vec<Colour>) -> Self {
        let stops = if stops.is_empty() {
            vec![Colour::rgb(0, 0, 0)]
        } else {
            stops
        };
        Self { stops }
    }

    pub fn len(&self) -> usize {
        self.stops.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// The colour at `y`, in a viewport `height` tall.
    ///
    /// `y` is measured downward from the top, as everything in `geometry` is, so
    /// the floor is `y == height` and the fraction is inverted here.
    pub fn at(&self, y: f32, height: f32) -> Colour {
        let last = self.stops.len() - 1;
        if last == 0 || height <= 0.0 {
            return self.stops[0];
        }
        let t = ((height - y) / height).clamp(0.0, 1.0);
        self.stops[((t * last as f32).round() as usize).min(last)]
    }

    /// Each band as `(top, bottom, colour)` in a viewport `height` tall.
    ///
    /// Half-height end bands are the point - see the module note. Returned
    /// top-down so a caller can clip against a rectangle in the same order it
    /// stores one.
    pub fn bands(&self, height: f32) -> Vec<(f32, f32, Colour)> {
        let last = self.stops.len() - 1;
        if last == 0 || height <= 0.0 {
            return vec![(0.0, height.max(0.0), self.stops[0])];
        }
        let last_f = last as f32;
        (0..=last)
            .rev()
            .map(|k| {
                // The fractions at which rounding tips into and out of stop `k`.
                let t_lo = ((k as f32 - 0.5) / last_f).clamp(0.0, 1.0);
                let t_hi = ((k as f32 + 0.5) / last_f).clamp(0.0, 1.0);
                (height * (1.0 - t_hi), height * (1.0 - t_lo), self.stops[k])
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
    /// programming error and is the caller's to skip.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Pixmap::new(width, height).map(|pixmap| Self { pixmap })
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

    /// Paint `rect` in one colour.
    ///
    /// A rectangle with no area is skipped rather than refused: a bar at rest is
    /// zero pixels tall, and every frame of silence would otherwise be an error
    /// to handle.
    pub fn fill(&mut self, rect: Rect, colour: Colour) {
        let Some(sk) = SkRect::from_xywh(rect.x, rect.y, rect.w, rect.h) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(colour.red, colour.green, colour.blue, colour.alpha);
        // Antialiasing on, which is the whole argument for drawing pixels: a bar
        // edge lands where the arithmetic puts it instead of being snapped to a
        // boundary, and that snapping is the mechanism behind #63.
        paint.anti_alias = true;
        self.pixmap
            .fill_rect(sk, &paint, Transform::identity(), None);
    }

    /// Paint `rect` with the colour the ramp gives at each height.
    ///
    /// Filled band by band rather than row by row, so the cost is the number of
    /// stops and not the height of the rectangle.
    pub fn fill_ramped(&mut self, rect: Rect, ramp: &Ramp, view_height: f32) {
        for (top, bottom, colour) in ramp.bands(view_height) {
            let y = top.max(rect.y);
            let h = bottom.min(rect.bottom()) - y;
            if h <= 0.0 {
                continue;
            }
            self.fill(
                Rect {
                    x: rect.x,
                    y,
                    w: rect.w,
                    h,
                },
                colour,
            );
        }
    }

    /// The frame in straight-alpha RGBA, which is what `f=32` means to a
    /// terminal and what a window blit expects.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixmap.data().len());
        for px in self.pixmap.pixels() {
            let c = px.demultiply();
            out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        out
    }

    /// The premultiplied buffer, for a target that wants it that way.
    pub fn premultiplied(&self) -> &[u8] {
        self.pixmap.data()
    }
}

/// Everything a surface needs to draw one frame.
///
/// The seam between the analyser and whatever is drawing. Deliberately not a
/// retained scene graph: the glyph renderer has to quantise to cells with its
/// own rules - the fill-versus-glyph backdrop, cap lifting, partial blocks - and
/// quantising a shared list of rectangles back into cells cannot reproduce them.
/// Both surfaces take the same numbers and each is free to be itself.
pub struct Scene<'a> {
    /// Bar heights, 0..=1, already through the ballistics.
    pub values: &'a [f32],
    /// Peak positions, 0..=1. `None` when caps are switched off.
    pub peaks: Option<&'a [f32]>,
    pub layout: crate::render::Layout,
    pub view: crate::render::Viewport,
    /// Colour of a lit bar, by height.
    pub ramp: &'a Ramp,
    /// Colour of the unlit backdrop, by height. `None` leaves the terminal's own
    /// background showing, which is what a theme without a grid asks for.
    pub grid: Option<&'a Ramp>,
    pub cap: Colour,
    pub cap_thickness: f32,
}

/// Draw `scene` into `canvas`, back to front.
///
/// Backdrop, then bars, then caps - the same order the glyph renderer uses, and
/// the reason a cap can sit over the backdrop instead of replacing it.
pub fn draw(scene: &Scene, canvas: &mut Canvas) {
    use crate::render::geometry::{backdrop, bars, caps};

    canvas.clear();

    // Both counts are the glyph renderer's, taken verbatim: the backdrop fills
    // every column that fits, and bars draw the lesser of what fits and what
    // exists. Letting `values.len()` drive it instead would put rectangles past
    // the viewport - which tiny-skia clips in silence, so the two surfaces would
    // disagree on bar count for the same input and nothing would say so.
    let columns = scene.layout.count(scene.view.width);
    let drawable = columns.min(scene.values.len());

    if let Some(grid) = scene.grid {
        for rect in backdrop(columns, &scene.layout, &scene.view) {
            canvas.fill_ramped(rect, grid, scene.view.height);
        }
    }

    for rect in bars(&scene.values[..drawable], &scene.layout, &scene.view) {
        canvas.fill_ramped(rect, scene.ramp, scene.view.height);
    }

    if let Some(peaks) = scene.peaks {
        let capped = drawable.min(peaks.len());
        for rect in caps(
            &peaks[..capped],
            scene.cap_thickness,
            &scene.layout,
            &scene.view,
        ) {
            canvas.fill(rect, scene.cap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::geometry::{Viewport, backdrop, bars, caps};

    fn ramp() -> Ramp {
        Ramp::new(vec![
            Colour::rgb(0x00, 0xff, 0x00),
            Colour::rgb(0xff, 0xff, 0x00),
            Colour::rgb(0xff, 0x00, 0x00),
        ])
    }

    /// The pixel at `(x, y)` as straight RGBA.
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

    #[test]
    fn a_cap_composites_over_the_backdrop_instead_of_erasing_it() {
        // The defect that cannot be fixed in a cell grid (#65): one cell holds
        // one glyph, so writing the cap destroys the backdrop beneath it. Here
        // both are drawn and the backdrop survives either side of the cap.
        let view = Viewport::new(20.0, 60.0);
        let layout = crate::render::Layout::new(10.0, 2.0);
        let mut canvas = Canvas::new(20, 60).unwrap();
        canvas.clear();

        let grid = Colour::rgb(0x00, 0x40, 0x00);
        let cap = Colour::rgb(0xff, 0xff, 0xff);
        canvas.fill(backdrop(1, &layout, &view)[0], grid);
        let cap_rect = caps(&[0.5], 4.0, &layout, &view)[0];
        canvas.fill(cap_rect, cap);

        let cap_y = cap_rect.y as u32 + 1;
        assert_eq!(at(&canvas, 2, cap_y), cap, "the cap is drawn");
        // Above and below it the backdrop is untouched, which is the whole point.
        assert_eq!(at(&canvas, 2, cap_y.saturating_sub(6)), grid);
        assert_eq!(at(&canvas, 2, cap_rect.bottom() as u32 + 4), grid);
    }

    #[test]
    fn colour_follows_height_not_the_bar_value() {
        // The invariant the terminal renderer is built on, restated in pixels:
        // where a short bar and a tall one overlap they are the same colour.
        let view = Viewport::new(10.0, 60.0);
        let layout = crate::render::Layout::new(10.0, 0.0);
        let r = ramp();

        let mut short = Canvas::new(10, 60).unwrap();
        short.clear();
        short.fill_ramped(bars(&[0.3], &layout, &view)[0], &r, view.height);

        let mut tall = Canvas::new(10, 60).unwrap();
        tall.clear();
        tall.fill_ramped(bars(&[0.9], &layout, &view)[0], &r, view.height);

        // Every row the short bar reaches must match the tall one there.
        for y in 43..60 {
            assert_eq!(
                at(&short, 5, y),
                at(&tall, 5, y),
                "row {y} disagrees between a short bar and a tall one"
            );
        }
    }

    #[test]
    fn the_ramp_runs_bottom_up() {
        let r = ramp();
        assert_eq!(r.at(60.0, 60.0), Colour::rgb(0x00, 0xff, 0x00), "floor");
        assert_eq!(r.at(0.0, 60.0), Colour::rgb(0xff, 0x00, 0x00), "ceiling");
        assert_eq!(r.at(30.0, 60.0), Colour::rgb(0xff, 0xff, 0x00), "middle");
    }

    #[test]
    fn the_end_bands_are_half_height() {
        // Not a quirk to tidy away: `ramp_index` rounds to the nearest stop, so
        // the first stop holds only until the fraction reaches half a band.
        // Evenly spaced stops would move every boundary in the picture.
        let r = ramp();
        let bands = r.bands(60.0);
        assert_eq!(bands.len(), 3);
        let height = |(top, bottom, _): &(f32, f32, Colour)| bottom - top;
        assert!((height(&bands[0]) - 15.0).abs() < 1e-4, "top band");
        assert!((height(&bands[1]) - 30.0).abs() < 1e-4, "middle band");
        assert!((height(&bands[2]) - 15.0).abs() < 1e-4, "bottom band");
        // And they tile the viewport exactly, with no seam and no overlap.
        let total: f32 = bands.iter().map(height).sum();
        assert!((total - 60.0).abs() < 1e-4, "bands must tile the height");
    }

    #[test]
    fn bands_agree_with_asking_for_a_single_point() {
        // Two routes to the same answer, so filling by band cannot drift from
        // what `at` reports without a test noticing.
        let r = ramp();
        for (top, bottom, colour) in r.bands(60.0) {
            let middle = (top + bottom) / 2.0;
            assert_eq!(r.at(middle, 60.0), colour, "band at y={middle}");
        }
    }

    #[test]
    fn a_bar_at_rest_draws_nothing_and_does_not_panic() {
        let view = Viewport::new(10.0, 60.0);
        let layout = crate::render::Layout::new(10.0, 0.0);
        let mut canvas = Canvas::new(10, 60).unwrap();
        canvas.clear();
        canvas.fill_ramped(bars(&[0.0], &layout, &view)[0], &ramp(), view.height);
        assert!(
            canvas.to_rgba().chunks(4).all(|px| px[3] == 0),
            "silence must leave the frame transparent"
        );
    }

    #[test]
    fn a_single_stop_ramp_is_flat() {
        let r = Ramp::new(vec![Colour::rgb(1, 2, 3)]);
        assert_eq!(r.at(0.0, 60.0), Colour::rgb(1, 2, 3));
        assert_eq!(r.at(60.0, 60.0), Colour::rgb(1, 2, 3));
        assert_eq!(r.bands(60.0).len(), 1);
    }

    #[test]
    fn an_empty_ramp_is_not_representable() {
        let r = Ramp::new(vec![]);
        assert_eq!(r.len(), 1);
        assert_eq!(r.at(0.0, 60.0), Colour::rgb(0, 0, 0));
    }

    #[test]
    fn a_zero_height_viewport_produces_nothing_that_panics() {
        let r = ramp();
        assert_eq!(r.at(0.0, 0.0), Colour::rgb(0x00, 0xff, 0x00));
        assert_eq!(r.bands(0.0).len(), 1);
        assert!(Canvas::new(0, 0).is_none());
    }

    fn scene<'a>(
        values: &'a [f32],
        peaks: Option<&'a [f32]>,
        ramp: &'a Ramp,
        grid: Option<&'a Ramp>,
        view: Viewport,
    ) -> Scene<'a> {
        Scene {
            values,
            peaks,
            layout: crate::render::Layout::new(10.0, 0.0),
            view,
            ramp,
            grid,
            cap: Colour::rgb(0xff, 0xff, 0xff),
            cap_thickness: 3.0,
        }
    }

    #[test]
    fn a_whole_scene_layers_backdrop_then_bars_then_cap() {
        let view = Viewport::new(10.0, 60.0);
        let r = ramp();
        let grid = Ramp::new(vec![Colour::rgb(0x00, 0x20, 0x00)]);
        let mut canvas = Canvas::new(10, 60).unwrap();
        draw(
            &scene(&[0.5], Some(&[0.5]), &r, Some(&grid), view),
            &mut canvas,
        );

        // Above the cap: backdrop only, because nothing else reaches there.
        assert_eq!(at(&canvas, 5, 5), Colour::rgb(0x00, 0x20, 0x00));
        // The bar sits on the floor and is lit, so it is not the backdrop.
        assert_ne!(at(&canvas, 5, 58), Colour::rgb(0x00, 0x20, 0x00));
        // The cap is above the bar's top edge and is the cap colour.
        let cap_rect = caps(&[0.5], 3.0, &crate::render::Layout::new(10.0, 0.0), &view)[0];
        assert_eq!(
            at(&canvas, 5, cap_rect.y as u32 + 1),
            Colour::rgb(0xff, 0xff, 0xff)
        );
    }

    #[test]
    fn caps_switched_off_draw_nothing() {
        let view = Viewport::new(10.0, 60.0);
        let r = ramp();
        let grid = Ramp::new(vec![Colour::rgb(0x00, 0x20, 0x00)]);

        let mut with = Canvas::new(10, 60).unwrap();
        draw(
            &scene(&[0.5], Some(&[0.9]), &r, Some(&grid), view),
            &mut with,
        );
        let mut without = Canvas::new(10, 60).unwrap();
        draw(&scene(&[0.5], None, &r, Some(&grid), view), &mut without);

        assert_ne!(with.to_rgba(), without.to_rgba(), "a cap must be visible");
        // And with peaks off, the row the cap would occupy is plain backdrop.
        assert_eq!(at(&without, 5, 8), Colour::rgb(0x00, 0x20, 0x00));
    }

    #[test]
    fn no_grid_leaves_the_terminal_background_showing() {
        // A theme without a grid must not invent one - the terminal's own
        // background is what shows through, so those pixels stay transparent.
        let view = Viewport::new(10.0, 60.0);
        let r = ramp();
        let mut canvas = Canvas::new(10, 60).unwrap();
        draw(&scene(&[0.25], None, &r, None, view), &mut canvas);
        assert_eq!(
            at(&canvas, 5, 5).alpha,
            0,
            "unlit rows must stay transparent"
        );
        assert_eq!(at(&canvas, 5, 58).alpha, 0xff, "the lit bar is still drawn");
    }

    #[test]
    fn silence_leaves_the_backdrop_and_the_caps_on_the_floor() {
        let view = Viewport::new(10.0, 60.0);
        let r = ramp();
        let grid = Ramp::new(vec![Colour::rgb(0x00, 0x20, 0x00)]);
        let mut canvas = Canvas::new(10, 60).unwrap();
        draw(
            &scene(&[0.0], Some(&[0.0]), &r, Some(&grid), view),
            &mut canvas,
        );
        // The backdrop still shows the column.
        assert_eq!(at(&canvas, 5, 5), Colour::rgb(0x00, 0x20, 0x00));
        // The cap rests on the floor rather than sinking out of sight.
        assert_eq!(at(&canvas, 5, 58), Colour::rgb(0xff, 0xff, 0xff));
    }

    #[test]
    fn an_antialiased_bar_edge_blends_into_the_backdrop() {
        // Every layer being opaque makes source-over and replace-the-pixel look
        // identical, so a whole-scene test cannot tell whether compositing works
        // at all. A bar whose top edge lands mid-pixel is where it shows: that
        // edge must be part bar and part backdrop, not part bar and part hole.
        let view = Viewport::new(10.0, 60.0);
        let r = Ramp::new(vec![Colour::rgb(0xff, 0x00, 0x00)]);
        let grid = Ramp::new(vec![Colour::rgb(0x00, 0x00, 0xff)]);

        // 0.333 * 60 = 19.98, so the top edge sits a hundredth of a pixel inside
        // row 40 and that row is almost entirely backdrop.
        let mut canvas = Canvas::new(10, 60).unwrap();
        draw(&scene(&[0.333], None, &r, Some(&grid), view), &mut canvas);
        let edge = at(&canvas, 5, 40);
        assert_eq!(edge.alpha, 0xff, "the backdrop keeps the edge opaque");
        assert!(
            edge.red > 0 && edge.blue > 0,
            "the edge should carry both bar and backdrop, got {edge:?}"
        );

        // Without a backdrop the same edge reaches the terminal semi-transparent
        // and the terminal composites it. Different pixels, deliberately.
        let mut bare = Canvas::new(10, 60).unwrap();
        draw(&scene(&[0.333], None, &r, None, view), &mut bare);
        let bare_edge = at(&bare, 5, 40);
        assert!(
            bare_edge.alpha > 0 && bare_edge.alpha < 0xff,
            "a bare edge stays partly transparent, got {bare_edge:?}"
        );
    }

    #[test]
    fn more_values_than_fit_do_not_draw_past_the_viewport() {
        // The glyph renderer draws the lesser of what fits and what exists. A
        // pixel surface that drew them all would have tiny-skia clip the excess
        // in silence, so the two would disagree on bar count with nothing to say
        // so - and "matches the glyph release" is the gate for shipping pixels.
        let view = Viewport::new(30.0, 60.0);
        let r = Ramp::new(vec![Colour::rgb(0xff, 0x00, 0x00)]);
        let layout = crate::render::Layout::new(10.0, 0.0);
        assert_eq!(layout.count(view.width), 3, "three bars fit");

        let mut three = Canvas::new(30, 60).unwrap();
        draw(
            &Scene {
                values: &[1.0, 1.0, 1.0],
                ..scene(&[], None, &r, None, view)
            },
            &mut three,
        );
        let mut ten = Canvas::new(30, 60).unwrap();
        draw(
            &Scene {
                values: &[1.0; 10],
                ..scene(&[], None, &r, None, view)
            },
            &mut ten,
        );
        assert_eq!(
            three.to_rgba(),
            ten.to_rgba(),
            "bars past the viewport must not change the picture"
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
            Rect {
                x: 0.0,
                y: 0.0,
                w: 4.0,
                h: 4.0,
            },
            Colour {
                red: 0xff,
                green: 0x00,
                blue: 0x00,
                alpha: 0x80,
            },
        );
        let px = at(&canvas, 2, 2);
        assert_eq!(px.alpha, 0x80, "alpha is preserved");
        assert!(px.red >= 0xfe, "red came back as {} not 0xff", px.red);
    }
}
